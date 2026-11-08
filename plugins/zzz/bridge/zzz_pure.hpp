// zzz_pure.hpp - zzz 原生桥的纯逻辑单元（仅本插件内部使用）。
//
// 只包含键/值安全规则、规范 hash、HTTP 请求解析与响应构造、JVar 中立值、
// 主线程 job 状态机；不包含任何宿主进程接口（无 socket connect、无进程
// 注入 API、无 CPython 加载），测试目标单独链接本单元。
//
// JobHub 生命周期约定：
//   - job 由 shared_ptr 持有；CPython pending 回调参数是堆上 JobTicket，
//     回调与等待方共同保活，任何一方提前离开都不释放对方仍要触碰的对象。
//   - Phase 为 Queued/Running/Done/Cancelled 原子状态；排队超时 CAS 取消，
//     晚到回调看到 Cancelled 不执行；已 Running 的超时如实报 unknown，
//     绝不自动重试；回调入口 catch 全部异常，不允许异常跨 C 边界。
//   - 完成事件句柄只由 HostJob 析构关闭；注册表只存 weak_ptr 用于观测，
//     绝不释放尚未回调的 ticket/job；取消的 queued ticket 只能由回调
//     （或永不发生）回收，等待方不得删除。

#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>
#include <bcrypt.h>

#include <algorithm>
#include <atomic>
#include <cctype>
#include <cmath>
#include <cstdio>
#include <cstring>
#include <functional>
#include <memory>
#include <mutex>
#include <string>
#include <utility>
#include <vector>

#pragma comment(lib, "bcrypt.lib")

namespace winrt::Windows::Data::Json {
struct JsonValue;
struct IJsonValue;
}

// ----------------------------------------------------------------------
// CPython C API 函数指针表（实现由生产 core 运行时填充；测试用 fake 注入）
// ----------------------------------------------------------------------

struct PyObject;
using Py_ssize_t = long long;
using PyGilState = int;

struct PythonApi {
    int (*Py_IsInitialized)() = nullptr;
    int (*Py_IsFinalizing)() = nullptr;
    void (*Py_IncRef)(PyObject*) = nullptr;
    void (*Py_DecRef)(PyObject*) = nullptr;
    PyGilState (*PyGILState_Ensure)() = nullptr;
    void (*PyGILState_Release)(PyGilState) = nullptr;
    int (*Py_AddPendingCall)(int (*)(void*), void*) = nullptr;
    PyObject* (*PyImport_ImportModule)(const char*) = nullptr;
    PyObject* (*PyImport_GetModuleDict)() = nullptr;
    PyObject* (*PyObject_GetAttrString)(PyObject*, const char*) = nullptr;
    int (*PyObject_SetAttrString)(PyObject*, const char*, PyObject*) = nullptr;
    PyObject* (*PyObject_GetItem)(PyObject*, PyObject*) = nullptr;
    PyObject* (*PyObject_Call)(PyObject*, PyObject*, PyObject*) = nullptr;
    PyObject* (*PyObject_CallObject)(PyObject*, PyObject*) = nullptr;
    PyObject* (*PyObject_Str)(PyObject*) = nullptr;
    PyObject* (*PyObject_Type)(PyObject*) = nullptr;
    int (*PyObject_IsTrue)(PyObject*) = nullptr;
    int (*PyObject_RichCompareBool)(PyObject*, PyObject*, int) = nullptr;
    PyObject* (*PyErr_Occurred)() = nullptr;
    void (*PyErr_Clear)() = nullptr;
    PyObject* (*PyUnicode_FromString)(const char*) = nullptr;
    const char* (*PyUnicode_AsUTF8)(PyObject*) = nullptr;
    PyObject* (*PyUnicode_FromFormat)(const char*, ...) = nullptr;
    PyObject* (*PyLong_FromLongLong)(long long) = nullptr;
    long long (*PyLong_AsLongLong)(PyObject*) = nullptr;
    PyObject* (*PyFloat_FromDouble)(double) = nullptr;
    double (*PyFloat_AsDouble)(PyObject*) = nullptr;
    PyObject* (*PyBool_FromLong)(long) = nullptr;
    PyObject* (*PyDict_New)() = nullptr;
    int (*PyDict_SetItemString)(PyObject*, const char*, PyObject*) = nullptr;
    PyObject* (*PyDict_GetItemString)(PyObject*, const char*) = nullptr;
    PyObject* (*PyDict_GetItemWithError)(PyObject*, PyObject*) = nullptr;
    PyObject* (*PyDict_Copy)(PyObject*) = nullptr;
    int (*PyDict_Next)(PyObject*, Py_ssize_t*, PyObject**, PyObject**) = nullptr;
    Py_ssize_t (*PyList_Size)(PyObject*) = nullptr;
    PyObject* (*PyList_GetItem)(PyObject*, Py_ssize_t) = nullptr;
    PyObject* (*PyList_New)(Py_ssize_t) = nullptr;
    int (*PyList_Append)(PyObject*, PyObject*) = nullptr;
    int (*PyDict_SetItem)(PyObject*, PyObject*, PyObject*) = nullptr;
    PyObject* (*PyTuple_New)(Py_ssize_t) = nullptr;
    int (*PyTuple_SetItem)(PyObject*, Py_ssize_t, PyObject*) = nullptr;
    PyObject* pyLongType = nullptr;
    PyObject* pyFloatType = nullptr;
    PyObject* pyUnicodeType = nullptr;
    PyObject* pyDictType = nullptr;
    PyObject* pyListType = nullptr;
    PyObject* pyBoolType = nullptr;
    PyObject* pyNone = nullptr;
    // Qt poster 所需（全部为官方导出；解析失败即 fail closed）：
    PyObject* (*PyCFunction_NewEx)(void*, PyObject*, PyObject*) = nullptr;
    PyObject* (*PyCapsule_New)(void*, const char*, void (*)(PyObject*)) = nullptr;
    void* (*PyCapsule_GetPointer)(PyObject*, const char*) = nullptr;
    bool ok = false;
};
extern PythonApi g_py;

// 主线程投递入口：生产 = Qt 上下文 poster（宿主已加载 PySide6 的
// QTimer.singleShot(0, context, callable)，回调在 Qt GUI 线程、持有 GIL 时
// 执行）；隔离测试 = fake 队列；未设置时回退 CPython pending call（仅测试
// 用——Py_AddPendingCall 只在字节码边界执行，Qt GUI 停在 C++ 事件循环时
// 可能长期饥饿；生产 core 启动时验证 poster 并显式设置，失败即拒绝启动，
// 不存在悄悄降级）。返回 0 表示所有权已转移（回调或 discard 钩子负责释放
// arg）；非 0 表示投递失败且 post 内部已通过 discard(arg) 释放。
// budgetMs 是本次请求的剩余等待预算：拿到 GIL 前已超预算的写操作不得再
// 晚投递，必须返回失败。
using PostCallFn = int (*)(int (*func)(void*), void (*discard)(void*), void* arg, DWORD budgetMs);
extern PostCallFn g_postCall;

// ----------------------------------------------------------------------
// 键与值的安全规则、令牌比较
// ----------------------------------------------------------------------

inline const char* SensitiveMask() { return "[已屏蔽]"; }

inline bool IsSensitiveKey(const std::string& key) {
    static const char* kSuffixes[] = {
        "KEY", "TOKEN", "SECRET", "WEBHOOK", "PASSWORD",
        "APPSECRET", "USER_ID", "QQ", "APP_ID",
    };
    std::string upper;
    upper.reserve(key.size());
    for (char c : key) {
        upper.push_back(static_cast<char>(std::toupper(static_cast<unsigned char>(c))));
    }
    for (const char* token : {"KEY", "TOKEN", "SECRET", "PASSWORD", "COOKIE", "WEBHOOK", "URL", "AUTHORIZATION"}) {
        if (upper.find(token) != std::string::npos) return true;
    }
    for (const char* suffix : kSuffixes) {
        size_t len = std::strlen(suffix);
        if (upper.size() >= len && upper.compare(upper.size() - len, len, suffix) == 0) return true;
    }
    return false;
}

inline bool IsForbiddenWriteKey(const std::string& key) {
    if (key.find('/') != std::string::npos || key.find('\\') != std::string::npos ||
        key.find("..") != std::string::npos) {
        return true;
    }
    std::string lower;
    lower.reserve(key.size());
    for (char c : key) {
        lower.push_back(static_cast<char>(std::tolower(static_cast<unsigned char>(c))));
    }
    static const char* kWords[] = {
        "path", "script", "pipeline", "command", "cmd", "shell", "exec", "exe", "bat", "startup",
    };
    for (const char* word : kWords) {
        if (lower.find(word) != std::string::npos) return true;
    }
    return false;
}

// groupId 只允许字母、数字、下划线、短横线；拒绝 ../、斜杠、盘符等路径形态。
inline bool IsValidGroupId(const std::string& groupId) {
    if (groupId.empty() || groupId.size() > 48) return false;
    for (unsigned char c : groupId) {
        if (!std::isalnum(c) && c != '_' && c != '-') return false;
    }
    return groupId != "." && groupId != "..";
}

inline bool IsValidAppId(const std::string& appId) {
    if (appId.empty() || appId.size() > 64) return false;
    for (unsigned char c : appId) {
        if (!std::isalnum(c) && c != '_' && c != '-') return false;
    }
    return true;
}

// 常量时间令牌比较。
inline bool SecureTokenEqual(const std::string& given, const std::string& expected) {
    if (given.size() != expected.size()) return false;
    volatile unsigned char diff = 0;
    for (size_t i = 0; i < given.size(); ++i) {
        diff |= static_cast<unsigned char>(given[i]) ^ static_cast<unsigned char>(expected[i]);
    }
    return diff == 0;
}

// ----------------------------------------------------------------------
// JVar：中立值表示（闭包产出与请求侧安全值共用）
// ----------------------------------------------------------------------

struct JVar {
    enum class T { Null, Bool, Int, Real, Str, Arr, Obj } t = T::Null;
    bool b = false;
    long long i = 0;
    double d = 0;
    std::string s;
    std::vector<JVar> arr;
    std::vector<std::pair<std::string, JVar>> obj;
};

inline const char* TypeNameOf(const JVar& value) {
    switch (value.t) {
        case JVar::T::Null: return "null";
        case JVar::T::Bool: return "bool";
        case JVar::T::Int: return "int";
        case JVar::T::Real: return "float";
        case JVar::T::Str: return "str";
        case JVar::T::Arr: return "list";
        case JVar::T::Obj: return "dict";
    }
    return "unknown";
}

// 规范序列化（键排序、类型标记、长度前缀），作为内容 hash 的唯一输入。
inline void CanonicalFeed(const JVar& value, std::string& out) {
    switch (value.t) {
        case JVar::T::Null: out.push_back('n'); break;
        case JVar::T::Bool: out.push_back('b'); out.push_back(value.b ? '1' : '0'); break;
        case JVar::T::Int: out.push_back('i'); out += std::to_string(value.i); break;
        case JVar::T::Real: out.push_back('r'); out += std::to_string(value.d); break;
        case JVar::T::Str:
            out.push_back('s');
            out += std::to_string(value.s.size());
            out.push_back(':');
            out += value.s;
            break;
        case JVar::T::Arr: {
            out.push_back('[');
            for (const auto& item : value.arr) CanonicalFeed(item, out);
            out.push_back(']');
            break;
        }
        case JVar::T::Obj: {
            out.push_back('{');
            std::vector<const std::pair<std::string, JVar>*> pairs;
            pairs.reserve(value.obj.size());
            for (const auto& kv : value.obj) pairs.push_back(&kv);
            std::sort(pairs.begin(), pairs.end(),
                      [](auto* a, auto* b) { return a->first < b->first; });
            for (const auto* kv : pairs) {
                out += std::to_string(kv->first.size());
                out.push_back(':');
                out += kv->first;
                CanonicalFeed(kv->second, out);
            }
            out.push_back('}');
            break;
        }
    }
}

inline std::string Sha256Hex(const std::string& bytes) {
    BCRYPT_ALG_HANDLE alg = nullptr;
    if (BCryptOpenAlgorithmProvider(&alg, BCRYPT_SHA256_ALGORITHM, nullptr, 0) != 0) return {};
    unsigned char hash[32]{};
    std::string out;
    BCRYPT_HASH_HANDLE handle = nullptr;
    if (BCryptCreateHash(alg, &handle, nullptr, 0, nullptr, 0, 0) == 0) {
        if (BCryptHashData(handle, (PUCHAR)bytes.data(), (ULONG)bytes.size(), 0) == 0 &&
            BCryptFinishHash(handle, hash, sizeof(hash), 0) == 0) {
            static const char* kHex = "0123456789abcdef";
            out.reserve(64);
            for (unsigned char b : hash) {
                out.push_back(kHex[b >> 4]);
                out.push_back(kHex[b & 0xF]);
            }
        }
        BCryptDestroyHash(handle);
    }
    BCryptCloseAlgorithmProvider(alg, 0);
    return out;
}

inline std::string CanonicalHash(const JVar& value) {
    std::string feed;
    feed.reserve(256);
    CanonicalFeed(value, feed);
    return Sha256Hex(feed);
}

// 写方向安全值：简单标量与浅层集合；尺寸与深度受限。
inline bool IsSafeWriteValue(const JVar& value, int depth, std::string& why) {
    switch (value.t) {
        case JVar::T::Null:
        case JVar::T::Bool:
            return true;
        case JVar::T::Int:
            if (value.i > 9007199254740992LL || value.i < -9007199254740992LL) {
                why = "整数超出安全范围";
                return false;
            }
            return true;
        case JVar::T::Real:
            if (!std::isfinite(value.d)) {
                why = "不接受 NaN 或无穷浮点数";
                return false;
            }
            return true;
        case JVar::T::Str:
            if (value.s.size() > 512) {
                why = "字符串超过 512 字符";
                return false;
            }
            return true;
        case JVar::T::Arr:
            if (depth >= 2) {
                why = "集合嵌套超过两层";
                return false;
            }
            if (value.arr.size() > 32) {
                why = "列表超过 32 项";
                return false;
            }
            for (const auto& item : value.arr) {
                if (!IsSafeWriteValue(item, depth + 1, why)) return false;
            }
            return true;
        case JVar::T::Obj:
            if (depth >= 2) {
                why = "集合嵌套超过两层";
                return false;
            }
            if (value.obj.size() > 32) {
                why = "对象超过 32 个字段";
                return false;
            }
            for (const auto& kv : value.obj) {
                if (kv.first.empty() || kv.first.size() > 64) {
                    why = "对象键必须是 1-64 字符";
                    return false;
                }
                if (IsSensitiveKey(kv.first) || IsForbiddenWriteKey(kv.first)) {
                    why = "嵌套字段涉及凭据、路径或可执行配置";
                    return false;
                }
                if (!IsSafeWriteValue(kv.second, depth + 1, why)) return false;
            }
            return true;
    }
    return false;
}

// 递归脱敏：嵌套对象里的敏感键同样屏蔽值。
inline void MaskSensitive(JVar& value) {
    switch (value.t) {
        case JVar::T::Arr:
            for (auto& item : value.arr) MaskSensitive(item);
            break;
        case JVar::T::Obj:
            for (auto& kv : value.obj) {
                if (IsSensitiveKey(kv.first)) {
                    kv.second = JVar{JVar::T::Str, false, 0, 0, SensitiveMask()};
                } else {
                    MaskSensitive(kv.second);
                }
            }
            break;
        default:
            break;
    }
}

inline std::string EscapeJsonString(const std::string& text) {
    std::string out;
    out.reserve(text.size() + 8);
    for (unsigned char c : text) {
        if (c == '"' || c == '\\') {
            out.push_back('\\');
            out.push_back(static_cast<char>(c));
        } else if (c < 0x20) {
            char buffer[8];
            std::snprintf(buffer, sizeof(buffer), "\\u%04x", c);
            out += buffer;
        } else {
            out.push_back(static_cast<char>(c));
        }
    }
    return out;
}

inline void SerializeJVar(const JVar& value, std::string& out) {
    switch (value.t) {
        case JVar::T::Null: out += "null"; break;
        case JVar::T::Bool: out += value.b ? "true" : "false"; break;
        case JVar::T::Int: out += std::to_string(value.i); break;
        case JVar::T::Real: {
            if (!std::isfinite(value.d)) {
                out += "null";  // 序列化方向不会出现，兜底不输出非法字面量
            } else {
                char buffer[40];
                std::snprintf(buffer, sizeof(buffer), "%.17g", value.d);
                out += buffer;
            }
            break;
        }
        case JVar::T::Str:
            out.push_back('"');
            out += EscapeJsonString(value.s);
            out.push_back('"');
            break;
        case JVar::T::Arr: {
            out.push_back('[');
            bool first = true;
            for (const auto& item : value.arr) {
                if (!first) out.push_back(',');
                first = false;
                SerializeJVar(item, out);
            }
            out.push_back(']');
            break;
        }
        case JVar::T::Obj: {
            out.push_back('{');
            bool first = true;
            for (const auto& kv : value.obj) {
                if (!first) out.push_back(',');
                first = false;
                out.push_back('"');
                out += EscapeJsonString(kv.first);
                out += "\":";
                SerializeJVar(kv.second, out);
            }
            out.push_back('}');
            break;
        }
    }
}

inline std::string SerializeJVar(const JVar& value) {
    std::string out;
    SerializeJVar(value, out);
    return out;
}

inline std::string ErrorJson(const std::string& code, const std::string& message) {
    JVar error{JVar::T::Obj};
    error.obj.emplace_back("code", JVar{JVar::T::Str, false, 0, 0, code});
    error.obj.emplace_back("message", JVar{JVar::T::Str, false, 0, 0, message});
    JVar root{JVar::T::Obj};
    root.obj.emplace_back("error", std::move(error));
    return SerializeJVar(root);
}

// WinRT JSON（Windows.Data.Json）→ JVar；实现链接在 zzz_pure.cpp。
bool JsonToJVar(const winrt::Windows::Data::Json::IJsonValue& value, JVar& out);

// ----------------------------------------------------------------------
// HTTP 请求解析与响应构造（纯字符串逻辑，无 socket）
// ----------------------------------------------------------------------

constexpr size_t kMaxBodyBytes = 64 * 1024;
constexpr size_t kMaxHeaderBytes = 16 * 1024;

struct HttpRequest {
    std::string method;
    std::string path;
    std::string body;
    std::string host;
    std::string origin;
    std::string referer;
    std::string token;
    size_t contentLength = 0;
    bool headerComplete = false;
    bool tooLarge = false;
    bool malformed = false;
};

inline std::string Trim(const std::string& text) {
    size_t begin = 0, end = text.size();
    while (begin < end && std::isspace(static_cast<unsigned char>(text[begin]))) ++begin;
    while (end > begin && std::isspace(static_cast<unsigned char>(text[end - 1]))) --end;
    return text.substr(begin, end - begin);
}

inline std::string LowerCopy(std::string text) {
    for (char& c : text) c = static_cast<char>(std::tolower(static_cast<unsigned char>(c)));
    return text;
}

// raw 是连接累计收到的字节；body 未收满时返回 false 且 headerComplete=true。
inline bool ParseHttpRequest(const std::string& raw, HttpRequest& out) {
    size_t split = raw.find("\r\n\r\n");
    if (split == std::string::npos) {
        out.tooLarge = raw.size() > kMaxHeaderBytes;
        return false;
    }
    out.headerComplete = true;
    std::string head = raw.substr(0, split);
    size_t lineEnd = head.find("\r\n");
    std::string requestLine = lineEnd == std::string::npos ? head : head.substr(0, lineEnd);
    size_t first = requestLine.find(' ');
    size_t second = requestLine.find(' ', first == std::string::npos ? 0 : first + 1);
    if (first == std::string::npos || second == std::string::npos) {
        out.malformed = true;
        return false;
    }
    out.method = Trim(requestLine.substr(0, first));
    out.path = Trim(requestLine.substr(first + 1, second - first - 1));
    size_t cursor = lineEnd == std::string::npos ? head.size() : lineEnd + 2;
    while (cursor < head.size()) {
        size_t eol = head.find("\r\n", cursor);
        if (eol == std::string::npos) eol = head.size();
        std::string line = head.substr(cursor, eol - cursor);
        cursor = eol + 2;
        size_t colon = line.find(':');
        if (colon == std::string::npos) continue;
        std::string name = LowerCopy(Trim(line.substr(0, colon)));
        std::string value = Trim(line.substr(colon + 1));
        if (name == "host") out.host = value;
        else if (name == "origin") out.origin = value;
        else if (name == "referer") out.referer = value;
        else if (name == "x-sleepy-token") out.token = value;
        else if (name == "content-length") {
            unsigned long long parsed = 0;
            try {
                parsed = std::stoull(value);
            } catch (...) {
                out.malformed = true;
                return false;
            }
            out.contentLength = static_cast<size_t>(parsed);
        }
    }
    if (out.contentLength > kMaxBodyBytes) {
        out.tooLarge = true;
        return false;
    }
    size_t bodyStart = split + 4;
    size_t bodyHave = raw.size() - bodyStart;
    if (bodyHave < out.contentLength) return false;
    out.body = raw.substr(bodyStart, out.contentLength);
    return true;
}

inline const char* StatusText(int status) {
    switch (status) {
        case 200: return "OK";
        case 400: return "Bad Request";
        case 401: return "Unauthorized";
        case 403: return "Forbidden";
        case 404: return "Not Found";
        case 405: return "Method Not Allowed";
        case 409: return "Conflict";
        case 413: return "Payload Too Large";
        case 422: return "Unprocessable Entity";
        case 500: return "Internal Server Error";
        case 503: return "Service Unavailable";
        case 504: return "Gateway Timeout";
        default: return "Error";
    }
}

inline std::string BuildHttpResponse(int status, const std::string& json) {
    std::string head = "HTTP/1.1 " + std::to_string(status) + " " + StatusText(status) + "\r\n";
    head += "Content-Type: application/json; charset=utf-8\r\n";
    head += "Content-Length: " + std::to_string(json.size()) + "\r\n";
    head += "Cache-Control: no-store\r\nConnection: close\r\n\r\n";
    return head + json;
}

// 防护与鉴权（纯逻辑）：Host 精确匹配、拒绝浏览器来源、令牌常量时间比较。
// 通过返回 nullptr，否则返回稳定错误码。
inline const char* VerifyHttpRequest(const HttpRequest& request, uint16_t port, const std::string& token) {
    if (request.host != "127.0.0.1:" + std::to_string(port)) return "bad_host";
    if (!request.origin.empty() || !request.referer.empty()) return "browser_forbidden";
    if (!SecureTokenEqual(request.token, token)) return "unauthorized";
    return nullptr;
}

// ----------------------------------------------------------------------
// 主线程 job 状态机（纯逻辑；CPython 接口仅经 g_py 函数指针）
// ----------------------------------------------------------------------

enum class DispatchOutcome { Executed, QueueFull, QueueTimeout, RunTimeout };

struct HostJob {
    enum class Phase { Queued, Running, Done, Cancelled };
    std::atomic<Phase> phase{Phase::Queued};
    HANDLE doneEvent = nullptr;  // 析构唯一关闭
    std::function<bool(HostJob&)> work;  // 在 CPython 主线程、持有 GIL 时执行
    bool ok = false;
    int failStatus = 0;
    std::string failCode, failMessage;

    HostJob() = default;
    ~HostJob() {
        if (doneEvent) ::CloseHandle(doneEvent);
    }
    HostJob(const HostJob&) = delete;
    HostJob& operator=(const HostJob&) = delete;

    bool Fail(int status, const char* code, std::string message) {
        ok = false;
        failStatus = status;
        failCode = code;
        failMessage = std::move(message);
        return false;
    }

    // CPython pending 回调参数：堆上持有 shared_ptr，回调与等待方共同保活。
    struct Ticket {
        std::shared_ptr<HostJob> job;
    };

    // 回调入口：noexcept 贯穿，任何异常都在此吞掉转为失败，不跨 C 边界。
    static int PyEntry(void* self) noexcept {
        Ticket* ticket = static_cast<Ticket*>(self);
        std::shared_ptr<HostJob> job = std::move(ticket->job);  // 回调期间保活
        delete ticket;  // 每个投递物只回调一次；删除权仅在此或 PyDiscard
        Phase expected = Phase::Queued;
        if (!job->phase.compare_exchange_strong(expected, Phase::Running)) {
            return 0;  // 已被等待方取消（Cancelled）或已完成：不再执行
        }
        try {
            job->ok = job->work(*job);
        } catch (...) {
            job->ok = false;
            job->failStatus = 500;
            job->failCode = "host_exception";
            job->failMessage = "宿主主线程闭包内发生未预期异常";
        }
        job->phase.store(Phase::Done);
        ::SetEvent(job->doneEvent);
        return 0;
    }

    // 丢弃路径（Qt context 销毁丢弃回调 / 投递失败回收）：不执行工作，
    // 标记取消并释放 ticket。等待方持有的 shared_ptr 保证 job 与事件句柄
    // 此时仍存活；SetEvent 唤醒等待方走取消分支，之后最后一个引用释放时
    // 才由析构关闭句柄——无悬挂句柄、无二次释放。
    static void PyDiscard(void* self) noexcept {
        Ticket* ticket = static_cast<Ticket*>(self);
        Phase expected = Phase::Queued;
        if (ticket->job->phase.compare_exchange_strong(expected, Phase::Cancelled)) {
            ::SetEvent(ticket->job->doneEvent);
        }
        delete ticket;
    }
};

class JobHub {
public:
    struct Result {
        DispatchOutcome outcome = DispatchOutcome::QueueFull;
        bool ok = false;
        int failStatus = 0;
        std::string failCode, failMessage;
    };

    // 派发一个主线程闭包。等待方超时离开后：
    //   Queued → CAS 取消（晚回调不执行）；Running → 如实 unknown（不重试）；
    //   恰好 Done → 正常带回结果。
    Result Dispatch(std::function<bool(HostJob&)> work, DWORD waitMs) {
        Result result;
        result.outcome = DispatchOutcome::QueueFull;
        auto job = std::make_shared<HostJob>();
        job->work = std::move(work);
        job->doneEvent = ::CreateEventW(nullptr, TRUE, FALSE, nullptr);
        if (!job->doneEvent) return result;
        auto* ticket = new HostJob::Ticket{job};
        {
            std::lock_guard<std::mutex> guard(lock_);
            tracked_.push_back(job);  // 仅 weak 观测，不保活
        }
        // 投递入口：生产走 Qt 上下文 poster（g_postCall，core 启动时已
        // 验证并设置，失败即失败、不降级到会饥饿的 pending 路线）；
        // 隔离测试用 fake；未设置时回退 CPython pending call（仅测试语义）。
        int posted = -1;
        if (g_postCall) {
            posted = g_postCall(&HostJob::PyEntry, &HostJob::PyDiscard, ticket, waitMs);
        } else if (g_py.Py_AddPendingCall && g_py.Py_AddPendingCall(&HostJob::PyEntry, ticket) == 0) {
            posted = 0;
        } else {
            // 未入队成功：CPython 绝不会回调，统一经 discard 释放。
            HostJob::PyDiscard(ticket);
        }
        if (posted != 0) {
            // 投递失败（无可用 poster、队列满或已超请求期限）：
            // arg 已由 post/discard 释放，如实返回失败。
            ForgetExpired();
            return result;
        }
        // 排队成功后 ticket 归回调删除；job 由 ticket 与本函数的 shared_ptr 共同保活。
        ::WaitForSingleObject(job->doneEvent, waitMs);
        HostJob::Phase phase = job->phase.load();
        if (phase == HostJob::Phase::Done) {
            result.outcome = DispatchOutcome::Executed;
            result.ok = job->ok;
            result.failStatus = job->failStatus;
            result.failCode = job->failCode;
            result.failMessage = job->failMessage;
        } else if (phase == HostJob::Phase::Cancelled) {
            result.outcome = DispatchOutcome::QueueTimeout;
        } else if (phase == HostJob::Phase::Queued) {
            HostJob::Phase expected = HostJob::Phase::Queued;
            if (job->phase.compare_exchange_strong(expected, HostJob::Phase::Cancelled)) {
                result.outcome = DispatchOutcome::QueueTimeout;  // 跳过，不重试
            } else if (expected == HostJob::Phase::Done) {
                // 与完成恰好在同一边界：带回结果。
                result.outcome = DispatchOutcome::Executed;
                result.ok = job->ok;
                result.failStatus = job->failStatus;
                result.failCode = job->failCode;
                result.failMessage = job->failMessage;
            } else {
                result.outcome = DispatchOutcome::RunTimeout;  // Running：unknown
            }
        } else {
            result.outcome = DispatchOutcome::RunTimeout;  // Running：unknown，不重试
        }
        ForgetExpired();
        return result;
    }

    void CancelQueued() {
        std::lock_guard<std::mutex> guard(lock_);
        for (const auto& weak : tracked_) {
            if (auto job = weak.lock()) {
                HostJob::Phase expected = HostJob::Phase::Queued;
                if (job->phase.compare_exchange_strong(expected, HostJob::Phase::Cancelled)) {
                    ::SetEvent(job->doneEvent);
                }
            }
        }
    }

    // 只清过期的 weak 观测项；绝不触碰未回调的 ticket/job。
    void ForgetExpired() {
        std::lock_guard<std::mutex> guard(lock_);
        tracked_.erase(std::remove_if(tracked_.begin(), tracked_.end(),
                                      [](const std::weak_ptr<HostJob>& weak) { return weak.expired(); }),
                       tracked_.end());
    }

    // 观测用：当前仍被某方持有引用的 job 数（测试断言生命周期，不靠字符串）。
    size_t TrackedAlive() {
        std::lock_guard<std::mutex> guard(lock_);
        size_t alive = 0;
        for (const auto& weak : tracked_) {
            if (!weak.expired()) ++alive;
        }
        return alive;
    }

private:
    std::mutex lock_;
    std::vector<std::weak_ptr<HostJob>> tracked_;
};
