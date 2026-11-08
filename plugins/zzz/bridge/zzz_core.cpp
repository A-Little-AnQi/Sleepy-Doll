// zzz_core.cpp - 绝区零一条龙原生反射桥核心（生产单元；纯逻辑在 zzz_pure）。
//
// 运行形态：本模块随 zzz_bridge.dll 注入用户自己启动的一条龙 GUI 进程，
// 只通过 GetProcAddress 解析已加载 CPython DLL 的 C API；不链接 Python SDK、
// 不创建 Python 进程、不执行任何 Python 源码字符串（无 PyRun/eval/exec）。
// 宿主读写全部通过 Py_AddPendingCall 派发到 CPython 主线程闭包内完成；
// 闭包只对真实对象的固定白名单属性/方法做 PyObject_GetAttrString / PyObject_Call。
// 排队超时跳过、执行中超时如实报 unknown，绝不自动重试。
//
// 配置端只读 factory._config_cache[f"{instance_idx}_{group_id}"] 真实缓存，
// 不调用 get_config 懒创建，保证只读操作不落盘。

#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>

#include <winrt/Windows.Data.Json.h>
#include <winrt/Windows.Foundation.Collections.h>

#include <atomic>
#include <deque>
#include <memory>
#include <mutex>
#include <new>
#include <string>
#include <thread>
#include <utility>
#include <vector>

#include <winsock2.h>
#include <ws2tcpip.h>

#pragma comment(lib, "ws2_32.lib")
#pragma comment(lib, "windowsapp.lib")

#include "zzz_bridge_abi.h"
#include "zzz_pure.hpp"

namespace {
constexpr const char* kBridgeName = "zzz-native-bridge";
constexpr const char* kBridgeVersion = "0.3.0";
constexpr DWORD kDispatchWaitMs = 15000;
constexpr DWORD kStopPollTotalMs = 2000;
constexpr int kPyEq = 2;  // Py_EQ
constexpr int kMaxConnections = 8;  // 并发连接线程上限，超过直接拒绝
std::atomic<uint64_t> g_sessionGeneration{1};
std::atomic<uint64_t> g_sessionNonce{0};
thread_local uint64_t g_requestGeneration = 0;

winrt::hstring Widen(const std::string& utf8) {
    return winrt::to_hstring(utf8);
}
}  // namespace

// CPython C API：只经函数指针使用；类型对象按数据符号取得，类型判断走
// PyObject_Type（new ref 即时释放），不按偏移硬读对象头。
// ----------------------------------------------------------------------

// 绑定集合与类型判断方式已在 CPython 3.11 / 3.12（普通 GIL、x64）官方 DLL
// 导出上核实；更早或 free-threaded 构建未核实，靠解析失败自然拒绝附着。
bool ResolvePythonApi(HMODULE pythonDll) {
    if (!pythonDll) return false;
    // 已附着进程的 C API 地址不变；重连时不能改写仍被回调使用的函数表。
    if (g_py.ok) return true;
    auto fn = [&](const char* name) { return ::GetProcAddress(pythonDll, name); };
    auto data = [&](const char* name) { return reinterpret_cast<PyObject*>(::GetProcAddress(pythonDll, name)); };
    PythonApi api;
#define ZZZ_BIND(field, name) \
    api.field = reinterpret_cast<decltype(api.field)>(fn(name)); \
    if (!api.field) return false
    ZZZ_BIND(Py_IsInitialized, "Py_IsInitialized");
    api.Py_IsFinalizing = reinterpret_cast<decltype(api.Py_IsFinalizing)>(fn("Py_IsFinalizing"));
    if (!api.Py_IsFinalizing) api.Py_IsFinalizing = reinterpret_cast<decltype(api.Py_IsFinalizing)>(fn("_Py_IsFinalizing"));
    ZZZ_BIND(Py_IncRef, "Py_IncRef");
    ZZZ_BIND(Py_DecRef, "Py_DecRef");
    ZZZ_BIND(PyGILState_Ensure, "PyGILState_Ensure");
    ZZZ_BIND(PyGILState_Release, "PyGILState_Release");
    api.Py_AddPendingCall = reinterpret_cast<decltype(api.Py_AddPendingCall)>(fn("Py_AddPendingCall"));
    if (!api.Py_AddPendingCall) {
        api.Py_AddPendingCall = reinterpret_cast<decltype(api.Py_AddPendingCall)>(fn("PyOS_AddPendingCall"));
    }
    if (!api.Py_AddPendingCall) return false;
    ZZZ_BIND(PyImport_ImportModule, "PyImport_ImportModule");
    ZZZ_BIND(PyImport_GetModuleDict, "PyImport_GetModuleDict");
    ZZZ_BIND(PyObject_GetAttrString, "PyObject_GetAttrString");
    ZZZ_BIND(PyObject_SetAttrString, "PyObject_SetAttrString");
    ZZZ_BIND(PyObject_GetItem, "PyObject_GetItem");
    ZZZ_BIND(PyObject_Call, "PyObject_Call");
    ZZZ_BIND(PyObject_CallObject, "PyObject_CallObject");
    ZZZ_BIND(PyObject_Str, "PyObject_Str");
    ZZZ_BIND(PyObject_Type, "PyObject_Type");
    ZZZ_BIND(PyObject_IsTrue, "PyObject_IsTrue");
    ZZZ_BIND(PyObject_RichCompareBool, "PyObject_RichCompareBool");
    ZZZ_BIND(PyErr_Occurred, "PyErr_Occurred");
    ZZZ_BIND(PyErr_Clear, "PyErr_Clear");
    ZZZ_BIND(PyUnicode_FromString, "PyUnicode_FromString");
    ZZZ_BIND(PyUnicode_AsUTF8, "PyUnicode_AsUTF8");
    ZZZ_BIND(PyUnicode_FromFormat, "PyUnicode_FromFormat");
    ZZZ_BIND(PyLong_FromLongLong, "PyLong_FromLongLong");
    ZZZ_BIND(PyLong_AsLongLong, "PyLong_AsLongLong");
    ZZZ_BIND(PyFloat_FromDouble, "PyFloat_FromDouble");
    ZZZ_BIND(PyFloat_AsDouble, "PyFloat_AsDouble");
    ZZZ_BIND(PyBool_FromLong, "PyBool_FromLong");
    ZZZ_BIND(PyDict_New, "PyDict_New");
    ZZZ_BIND(PyDict_SetItemString, "PyDict_SetItemString");
    ZZZ_BIND(PyDict_GetItemString, "PyDict_GetItemString");
    ZZZ_BIND(PyDict_GetItemWithError, "PyDict_GetItemWithError");
    ZZZ_BIND(PyDict_Copy, "PyDict_Copy");
    ZZZ_BIND(PyDict_Next, "PyDict_Next");
    ZZZ_BIND(PyList_Size, "PyList_Size");
    ZZZ_BIND(PyList_GetItem, "PyList_GetItem");
    ZZZ_BIND(PyList_New, "PyList_New");
    ZZZ_BIND(PyList_Append, "PyList_Append");
    ZZZ_BIND(PyDict_SetItem, "PyDict_SetItem");
    ZZZ_BIND(PyTuple_New, "PyTuple_New");
    ZZZ_BIND(PyTuple_SetItem, "PyTuple_SetItem");
    // Qt poster 需要的官方导出；缺失即解析失败、拒绝启动（fail closed）。
    ZZZ_BIND(PyCFunction_NewEx, "PyCFunction_NewEx");
    ZZZ_BIND(PyCapsule_New, "PyCapsule_New");
    ZZZ_BIND(PyCapsule_GetPointer, "PyCapsule_GetPointer");
#undef ZZZ_BIND
    api.pyLongType = data("PyLong_Type");
    api.pyFloatType = data("PyFloat_Type");
    api.pyUnicodeType = data("PyUnicode_Type");
    api.pyDictType = data("PyDict_Type");
    api.pyListType = data("PyList_Type");
    api.pyBoolType = data("PyBool_Type");
    api.pyNone = data("_Py_NoneStruct");
    if (!api.pyLongType || !api.pyFloatType || !api.pyUnicodeType || !api.pyDictType ||
        !api.pyListType || !api.pyBoolType || !api.pyNone) {
        return false;
    }
    api.ok = true;
    g_py = api;
    return true;
}

// 类型比较：PyObject_Type 返回 new ref，比较后立即释放，不读对象布局偏移。
bool TypeIs(PyObject* object, PyObject* expectedType) {
    if (!object || !expectedType) return false;
    PyObject* actual = g_py.PyObject_Type(object);
    if (!actual) {
        g_py.PyErr_Clear();
        return false;
    }
    bool same = actual == expectedType;
    g_py.Py_DecRef(actual);
    return same;
}

HMODULE FindLoadedPython() {
    const wchar_t* kNames[] = {
        L"python312.dll", L"python311.dll", L"python313.dll",
        L"python310.dll", L"python39.dll", L"python38.dll",
    };
    for (const wchar_t* name : kNames) {
        HMODULE module = ::GetModuleHandleW(name);
        if (module) return module;
    }
    return nullptr;
}

// ----------------------------------------------------------------------
// Qt 队列 poster：把主线程闭包经宿主已加载的 PySide6 调度到 Qt GUI 线程。
//
// Py_AddPendingCall 只在字节码边界执行；Qt GUI 长期停在 C++ 事件循环时
// pending call 可能一直等到用户活动，BGI 式空闲窗口无法保证响应。
// 改用官方 PySide6 QTimer.singleShot(0, context, functor) 重载：带 context
// 时可在任意线程安全调用，functor 在 context 所属的 GUI 线程、持有 GIL 时
// 执行。全程只用 CPython C API 反射已加载对象，不 import 业务模块、不执行
// 任何 Python 源码字符串、不链接 Qt SDK。
// ----------------------------------------------------------------------

// PyMethodDef 是 PyCFunction_NewEx 的文档化公共 ABI（name/meth/flags/doc
// 顺序自 CPython 1.x 起未变）；定义只使用公共前四字段，零初始化其余。
struct QtMethodDef {
    const char* ml_name;
    PyObject* (*ml_meth)(PyObject*, PyObject*);
    int ml_flags;
    const char* ml_doc;
};
constexpr int kMethNoargs = 0x0004;

// 一次投递：func（GUI 线程执行体）与 discard（丢弃回收钩子）只会经由
// consumed 原子标志二选一执行一次；call 本身只由 capsule 析构释放。
struct QtCall {
    std::atomic<bool> consumed{false};
    int (*func)(void*);
    void (*discard)(void*);
    void* arg;
};

struct QtPosterState {
    bool ready = false;              // 启动时一次性验证成功
    PyObject* singleShot = nullptr;  // PySide6.QtCore.QTimer.singleShot（强引用）
    PyObject* context = nullptr;     // QApplication.instance() 现存实例（强引用）
};
QtPosterState g_qtPoster;

// capsule 析构（Qt context 销毁丢弃回调、或投递失败回收）：不执行工作
// （它应跑在 GUI 线程），经 discard 取消并释放 ticket，再释放 call。
void QtCapsuleDtor(PyObject* capsule) noexcept {
    void* pointer = g_py.PyCapsule_GetPointer(capsule, nullptr);
    if (!pointer) {
        g_py.PyErr_Clear();
        return;
    }
    auto* call = static_cast<QtCall*>(pointer);
    bool expected = false;
    if (call->consumed.compare_exchange_strong(expected, true)) {
        call->discard(call->arg);
    }
    delete call;
}

// PySide 调用的 native 回调：在 Qt GUI 线程、持有 GIL 时进入；
// noinline 防优化器把异常传播假设引入 C 边界。
PyObject* QtPosterCallback(PyObject* self, PyObject* /*args*/) noexcept {
    try {
        void* pointer = g_py.PyCapsule_GetPointer(self, nullptr);
        if (pointer) {
            auto* call = static_cast<QtCall*>(pointer);
            bool expected = false;
            if (call->consumed.compare_exchange_strong(expected, true)) {
                call->func(call->arg);  // HostJob::PyEntry 自身 noexcept + catch 全部
            }
        } else {
            g_py.PyErr_Clear();
        }
    } catch (...) {
        // 不越 C 边界；不自动任务。
    }
    g_py.Py_IncRef(g_py.pyNone);
    return g_py.pyNone;
}

// 静态方法定义：DLL 被 pin 至宿主退出，生命周期覆盖所有回调。
QtMethodDef g_qtPosterMethod = {"_zzz_bridge_dispatch", &QtPosterCallback, kMethNoargs, nullptr};

// 从已加载 sys.modules 解析 PySide6 QTimer.singleShot 与现存 QApplication
// 实例（只取 existing instance，绝不创建 App）。任何一环缺失都返回失败。
// 必须在持有 GIL 时调用。
bool EnsureQtPoster() {
    if (g_qtPoster.ready) return true;
    PyObject* modules = g_py.PyImport_GetModuleDict();
    if (!modules) return false;
    PyObject* qtCore = g_py.PyDict_GetItemString(modules, "PySide6.QtCore");  // borrowed
    if (!qtCore) return false;
    PyObject* timer = g_py.PyObject_GetAttrString(qtCore, "QTimer");
    if (!timer) {
        g_py.PyErr_Clear();
        return false;
    }
    PyObject* singleShot = g_py.PyObject_GetAttrString(timer, "singleShot");
    g_py.Py_DecRef(timer);
    if (!singleShot) {
        g_py.PyErr_Clear();
        return false;
    }
    PyObject* qtWidgets = g_py.PyDict_GetItemString(modules, "PySide6.QtWidgets");
    if (!qtWidgets) {
        g_py.Py_DecRef(singleShot);
        return false;
    }
    PyObject* appClass = g_py.PyObject_GetAttrString(qtWidgets, "QApplication");
    if (!appClass) {
        g_py.PyErr_Clear();
        g_py.Py_DecRef(singleShot);
        return false;
    }
    PyObject* instanceAttr = g_py.PyObject_GetAttrString(appClass, "instance");
    g_py.Py_DecRef(appClass);
    if (!instanceAttr) {
        g_py.PyErr_Clear();
        g_py.Py_DecRef(singleShot);
        return false;
    }
    PyObject* context = g_py.PyObject_CallObject(instanceAttr, nullptr);
    g_py.Py_DecRef(instanceAttr);
    if (!context) {
        g_py.PyErr_Clear();
        g_py.Py_DecRef(singleShot);
        return false;
    }
    if (context == g_py.pyNone) {
        // 无现存 GUI 实例（可能仍在启动或非 GUI 形态）：明确失败，不造假上下文。
        g_py.Py_DecRef(context);
        g_py.Py_DecRef(singleShot);
        return false;
    }
    g_qtPoster.singleShot = singleShot;
    g_qtPoster.context = context;
    g_qtPoster.ready = true;
    return true;
}

// 生产投递入口：GIL 仅在创建与投递期间持有，等待回调期间已释放。
// 返回 0 = 已投递（所有权经 capsule 转移给 Qt 队列）；非 0 = 投递失败且
// arg 已释放（deadline 超限、poster 不可用、Qt 拒绝调用等，一律明确失败，
// 不降级 pending 路线）。
int ZzzPostViaQt(int (*func)(void*), void (*discard)(void*), void* arg, DWORD budgetMs) noexcept {
    DWORD started = ::GetTickCount();
    PyGilState gil = g_py.PyGILState_Ensure();  // 仅创建/投递期间持有
    int status = -1;
    bool ownershipTransferred = false;
    do {
        if (::GetTickCount() - started >= budgetMs) {
            // 拿 GIL 已耗尽请求预算：写操作不得再晚投递，如实失败。
            status = -2;
            break;
        }
        if (!g_qtPoster.ready) {
            status = -3;  // core 启动时已验证；运行中变 false 属不可能，fail closed
            break;
        }
        auto* call = new (std::nothrow) QtCall();
        if (!call) {
            status = -4;
            break;
        }
        call->func = func;
        call->discard = discard;
        call->arg = arg;
        // capsule 拥有 call；dtor 负责唯一释放（含 discard 路径）。
        // PyCapsule_New(pointer, name, destructor)。
        PyObject* capsule = g_py.PyCapsule_New(call, nullptr, &QtCapsuleDtor);
        if (!capsule) {
            g_py.PyErr_Clear();
            delete call;
            status = -5;
            break;
        }
        // METH_NOARGS 的 self 即 capsule；PyCFunction_NewEx 持有 capsule 强引用。
        PyObject* callback = g_py.PyCFunction_NewEx(&g_qtPosterMethod, capsule, nullptr);
        g_py.Py_DecRef(capsule);  // 释放本地引用，所有权交给 callback
        if (!callback) {
            g_py.PyErr_Clear();
            status = -5;  // capsule 引用归零已触发 dtor → discard(arg) 已释放
            ownershipTransferred = true;
            break;
        }
        PyObject* zero = g_py.PyLong_FromLongLong(0);
        PyObject* args = zero ? g_py.PyTuple_New(3) : nullptr;
        if (!args) {
            if (zero) g_py.Py_DecRef(zero);
            g_py.PyErr_Clear();
            g_py.Py_DecRef(callback);  // → capsule dtor → discard(arg)
            status = -5;
            ownershipTransferred = true;
            break;
        }
        g_py.PyTuple_SetItem(args, 0, zero);  // steals zero
        g_py.Py_IncRef(g_qtPoster.context);
        g_py.PyTuple_SetItem(args, 1, g_qtPoster.context);  // steals our reference
        g_py.PyTuple_SetItem(args, 2, callback);            // steals callback
        PyObject* result = g_py.PyObject_Call(g_qtPoster.singleShot, args, nullptr);
        g_py.Py_DecRef(args);
        if (!result) {
            g_py.PyErr_Clear();
            // args 释放时 callback/capsule 引用归零 → dtor → discard(arg)。
            status = -6;
            ownershipTransferred = true;
            break;
        }
        g_py.Py_DecRef(result);
        status = 0;
        ownershipTransferred = true;  // Qt 队列持有 callback（→capsule→call→arg）
    } while (false);
    g_py.PyGILState_Release(gil);
    if (!ownershipTransferred && status != 0) {
        discard(arg);  // 未进入 capsule 的失败分支：统一在此释放
    }
    return status;
}

// ----------------------------------------------------------------------
// HostReflector：白名单反射（只运行在主线程闭包内）
// ----------------------------------------------------------------------

struct AppEntry {
    std::string appId, appName;
    bool defaultGroup = false, internalApp = false, needNotify = false;
};

struct InstanceEntry {
    long long idx = 0;
    std::string name;
    bool active = false;
};

struct CatalogData {
    std::vector<AppEntry> apps;  // 按 appId 排序
    std::vector<InstanceEntry> instances;
    std::string projectName;
    long long currentInstanceIdx = -1;
    uint64_t sessionNonce = 0;

    std::string Hash() const {
        JVar appsVar{JVar::T::Arr};
        for (const auto& app : this->apps) {
            JVar item{JVar::T::Obj};
            item.obj.emplace_back("appId", JVar{JVar::T::Str, false, 0, 0, app.appId});
            item.obj.emplace_back("internalApp", JVar{JVar::T::Bool, app.internalApp});
            item.obj.emplace_back("defaultGroup", JVar{JVar::T::Bool, app.defaultGroup});
            appsVar.arr.push_back(std::move(item));
        }
        JVar instancesVar{JVar::T::Arr};
        for (const auto& instance : this->instances) {
            JVar item{JVar::T::Obj};
            item.obj.emplace_back("idx", JVar{JVar::T::Int, false, instance.idx});
            item.obj.emplace_back("name", JVar{JVar::T::Str, false, 0, 0, instance.name});
            instancesVar.arr.push_back(std::move(item));
        }
        JVar root{JVar::T::Obj};
        root.obj.emplace_back("apps", std::move(appsVar));
        root.obj.emplace_back("instances", std::move(instancesVar));
        root.obj.emplace_back("project", JVar{JVar::T::Str, false, 0, 0, projectName});
        root.obj.emplace_back("currentInstanceIdx", JVar{JVar::T::Int, false, currentInstanceIdx});
        root.obj.emplace_back("bridgeSession", JVar{JVar::T::Str, false, 0, 0, std::to_string(sessionNonce)});
        return CanonicalHash(root);
    }
};

// 每次操作的堆上上下文：闭包与等待线程共享；排队/执行超时后闭包延迟
// 执行也只写这份共享存储，不会触碰已返回线程的栈。
struct OpContext {
    CatalogData catalog;
    std::string state;
    std::string currentApp;
    long long currentInstance = -1;
    std::string currentGroup;
    JVar lastRunResult{JVar::T::Null};
    bool hasLastRun = false;
    JVar lastAppResult{JVar::T::Null};
    bool ready = false;
    bool submitted = false;
    bool configFound = false;
    bool keyed = true;
    JVar configData{JVar::T::Null};
    std::string newConfigHash;
    std::vector<std::string> cachedGroups;
    std::string describeNote;
};

struct HostReflector {
    PyObject* ctx = nullptr;  // 强引用缓存，找不到唯一实例前保持 null

    std::string Utf8Of(PyObject* value) {
        if (!value) return {};
        if (TypeIs(value, g_py.pyUnicodeType)) {
            const char* text = g_py.PyUnicode_AsUTF8(value);
            return text ? text : "";
        }
        PyObject* text = g_py.PyObject_Str(value);
        if (!text) {
            g_py.PyErr_Clear();
            return {};
        }
        const char* utf8 = g_py.PyUnicode_AsUTF8(text);
        std::string out = utf8 ? utf8 : "";
        g_py.Py_DecRef(text);
        return out;
    }

    PyObject* Attr(PyObject* object, const char* name) {
        PyObject* value = g_py.PyObject_GetAttrString(object, name);
        if (!value) g_py.PyErr_Clear();
        return value;  // new ref or null
    }

    // sys.modules['zzz_od.context.zzz_context'].ZContext + gc.get_objects() 找唯一实例。
    bool LocateContext(HostJob& job) {
        if (ctx) return true;
        PyObject* modules = g_py.PyImport_GetModuleDict();
        if (!modules) return job.Fail(503, "host_not_ready", "宿主 sys.modules 不可用");
        PyObject* module = g_py.PyDict_GetItemString(modules, "zzz_od.context.zzz_context");
        if (!module) {
            return job.Fail(503, "ctx_not_loaded", "宿主尚未加载 zzz_od.context.zzz_context 模块");
        }
        PyObject* zctxClass = Attr(module, "ZContext");
        if (!zctxClass) {
            return job.Fail(503, "host_incompatible", "ZContext 类不存在，宿主版本不受支持");
        }
        PyObject* gcModule = g_py.PyImport_ImportModule("gc");
        if (!gcModule) {
            g_py.Py_DecRef(zctxClass);
            g_py.PyErr_Clear();
            return job.Fail(503, "host_not_ready", "宿主 gc 模块不可用");
        }
        PyObject* getObjects = Attr(gcModule, "get_objects");
        g_py.Py_DecRef(gcModule);
        if (!getObjects) {
            g_py.Py_DecRef(zctxClass);
            return job.Fail(503, "host_incompatible", "gc.get_objects 不可用");
        }
        PyObject* objects = g_py.PyObject_CallObject(getObjects, nullptr);
        g_py.Py_DecRef(getObjects);
        if (!objects || !TypeIs(objects, g_py.pyListType)) {
            g_py.Py_DecRef(zctxClass);
            g_py.PyErr_Clear();
            return job.Fail(503, "host_not_ready", "gc.get_objects 未返回对象列表");
        }
        PyObject* found = nullptr;
        size_t count = 0;
        Py_ssize_t size = g_py.PyList_Size(objects);
        for (Py_ssize_t i = 0; i < size; ++i) {
            PyObject* item = g_py.PyList_GetItem(objects, i);  // borrowed
            if (item && TypeIs(item, zctxClass)) {
                if (++count == 1) {
                    g_py.Py_IncRef(item);
                    found = item;
                }
            }
        }
        g_py.Py_DecRef(objects);
        g_py.Py_DecRef(zctxClass);
        if (count == 0) return job.Fail(503, "ctx_not_found", "gc 中没有 ZContext 实例，等宿主完成启动后再试");
        if (count > 1) return job.Fail(503, "ctx_ambiguous", "发现多个 ZContext 实例，拒绝猜测目标");
        ctx = found;
        return true;
    }

    PyObject* RunContext(HostJob& job) {
        if (!LocateContext(job)) return nullptr;
        PyObject* run = Attr(ctx, "run_context");
        if (!run) {
            return job.Fail(503, "host_incompatible", "宿主缺少 run_context") ? nullptr : nullptr;
        }
        return run;
    }

    bool CollectCatalog(HostJob& job, CatalogData& out) {
        out.sessionNonce = g_sessionNonce.load();
        PyObject* run = RunContext(job);
        if (!run) return false;
        PyObject* factories = Attr(run, "_application_factory_map");
        g_py.Py_DecRef(run);
        if (!factories || !TypeIs(factories, g_py.pyDictType)) {
            if (factories) g_py.Py_DecRef(factories);
            return job.Fail(503, "host_not_ready", "宿主尚未完成应用注册");
        }
        Py_ssize_t pos = 0;
        PyObject* key = nullptr;
        PyObject* factory = nullptr;
        while (g_py.PyDict_Next(factories, &pos, &key, &factory)) {
            AppEntry entry;
            entry.appId = Utf8Of(key);
            PyObject* name = Attr(factory, "app_name");
            if (name) {
                entry.appName = Utf8Of(name);
                g_py.Py_DecRef(name);
            }
            if (entry.appName.empty()) entry.appName = entry.appId;
            auto flag = [&](const char* attr) {
                PyObject* value = Attr(factory, attr);
                bool result = false;
                if (value) {
                    result = g_py.PyObject_IsTrue(value) == 1;
                    g_py.Py_DecRef(value);
                }
                return result;
            };
            entry.defaultGroup = flag("default_group");
            entry.internalApp = flag("internal_app");
            entry.needNotify = flag("need_notify");
            out.apps.push_back(std::move(entry));
        }
        g_py.Py_DecRef(factories);
        std::sort(out.apps.begin(), out.apps.end(),
                  [](const AppEntry& a, const AppEntry& b) { return a.appId < b.appId; });

        PyObject* oneDragon = Attr(ctx, "one_dragon_config");
        if (!oneDragon) return job.Fail(503, "host_incompatible", "宿主缺少 one_dragon_config");
        PyObject* instances = Attr(oneDragon, "instance_list");
        g_py.Py_DecRef(oneDragon);
        if (!instances || !TypeIs(instances, g_py.pyListType)) {
            if (instances) g_py.Py_DecRef(instances);
            return job.Fail(503, "host_incompatible", "宿主 instance_list 不可读");
        }
        Py_ssize_t count = g_py.PyList_Size(instances);
        for (Py_ssize_t i = 0; i < count; ++i) {
            PyObject* item = g_py.PyList_GetItem(instances, i);
            if (!item) continue;
            InstanceEntry entry;
            PyObject* idx = Attr(item, "idx");
            if (idx) {
                entry.idx = g_py.PyLong_AsLongLong(idx);
                g_py.Py_DecRef(idx);
            }
            PyObject* name = Attr(item, "name");
            if (name) {
                entry.name = Utf8Of(name);
                g_py.Py_DecRef(name);
            }
            PyObject* active = Attr(item, "active");
            if (active) {
                entry.active = g_py.PyObject_IsTrue(active) == 1;
                g_py.Py_DecRef(active);
            }
            out.instances.push_back(std::move(entry));
        }
        g_py.Py_DecRef(instances);
        PyObject* projectConfig = Attr(ctx, "project_config");
        if (projectConfig) {
            PyObject* name = Attr(projectConfig, "project_name");
            if (name) {
                out.projectName = Utf8Of(name);
                g_py.Py_DecRef(name);
            }
            g_py.Py_DecRef(projectConfig);
        }
        if (out.projectName.empty()) out.projectName = "unknown";
        PyObject* currentIdx = Attr(ctx, "current_instance_idx");
        if (currentIdx) {
            out.currentInstanceIdx = g_py.PyLong_AsLongLong(currentIdx);
            g_py.Py_DecRef(currentIdx);
        }
        return true;
    }

    // Python 值 → JVar；读方向最深 4 层，更深以占位呈现；未知类型只报类型名。
    bool VarFromPython(HostJob& job, PyObject* value, int depth, JVar& out) {
        if (!value) return job.Fail(500, "read_failed", "宿主返回空值");
        if (value == g_py.pyNone) {
            out = JVar{JVar::T::Null};
            return true;
        }
        if (TypeIs(value, g_py.pyBoolType)) {
            out = JVar{JVar::T::Bool, g_py.PyObject_IsTrue(value) == 1};
            return true;
        }
        if (TypeIs(value, g_py.pyLongType)) {
            long long number = g_py.PyLong_AsLongLong(value);
            if (number == -1 && g_py.PyErr_Occurred()) {
                g_py.PyErr_Clear();
                out = JVar{JVar::T::Str, false, 0, 0, Utf8Of(value)};  // 大整数不造假精度
                return true;
            }
            out = JVar{JVar::T::Int, false, number};
            return true;
        }
        if (TypeIs(value, g_py.pyFloatType)) {
            out = JVar{JVar::T::Real};
            out.d = g_py.PyFloat_AsDouble(value);
            return true;
        }
        if (TypeIs(value, g_py.pyUnicodeType)) {
            out = JVar{JVar::T::Str, false, 0, 0, Utf8Of(value)};
            return true;
        }
        if (depth >= 4) {
            out = JVar{JVar::T::Str, false, 0, 0, "[深层结构]"};
            return true;
        }
        if (TypeIs(value, g_py.pyListType)) {
            out = JVar{JVar::T::Arr};
            Py_ssize_t size = g_py.PyList_Size(value);
            for (Py_ssize_t i = 0; i < size; ++i) {
                JVar item;
                if (!VarFromPython(job, g_py.PyList_GetItem(value, i), depth + 1, item)) return false;
                out.arr.push_back(std::move(item));
            }
            return true;
        }
        if (TypeIs(value, g_py.pyDictType)) {
            out = JVar{JVar::T::Obj};
            Py_ssize_t pos = 0;
            PyObject* k = nullptr;
            PyObject* v = nullptr;
            while (g_py.PyDict_Next(value, &pos, &k, &v)) {
                JVar item;
                if (!VarFromPython(job, v, depth + 1, item)) return false;
                out.obj.emplace_back(Utf8Of(k), std::move(item));
            }
            return true;
        }
        // 未知类型只取类型名：PyObject_Type 是 new ref，用完即释放。
        PyObject* typeObject = g_py.PyObject_Type(value);
        if (!typeObject) {
            g_py.PyErr_Clear();
            out = JVar{JVar::T::Str, false, 0, 0, "<object>"};
            return true;
        }
        PyObject* typeName = Attr(typeObject, "__name__");
        std::string label = typeName ? Utf8Of(typeName) : "object";
        if (typeName) g_py.Py_DecRef(typeName);
        g_py.Py_DecRef(typeObject);
        out = JVar{JVar::T::Str, false, 0, 0, "<" + label + ">"};
        return true;
    }

    // JVar → PyObject（写方向值已按安全规则校验过）。
    PyObject* VarToPython(const JVar& value) {
        switch (value.t) {
            case JVar::T::Null: { PyObject* none = g_py.pyNone; g_py.Py_IncRef(none); return none; }
            case JVar::T::Bool: return g_py.PyBool_FromLong(value.b ? 1 : 0);
            case JVar::T::Int: return g_py.PyLong_FromLongLong(value.i);
            case JVar::T::Real: return g_py.PyFloat_FromDouble(value.d);
            case JVar::T::Str: return g_py.PyUnicode_FromString(value.s.c_str());
            case JVar::T::Arr: {
                PyObject* list = g_py.PyList_New(0);
                if (!list) return nullptr;
                for (const auto& item : value.arr) {
                    PyObject* element = VarToPython(item);
                    if (!element || g_py.PyList_Append(list, element) != 0) {
                        if (element) g_py.Py_DecRef(element);
                        g_py.Py_DecRef(list);
                        g_py.PyErr_Clear();
                        return nullptr;
                    }
                    g_py.Py_DecRef(element);
                }
                return list;
            }
            case JVar::T::Obj: {
                PyObject* dict = g_py.PyDict_New();
                if (!dict) return nullptr;
                for (const auto& kv : value.obj) {
                    PyObject* element = VarToPython(kv.second);
                    PyObject* keyObj = g_py.PyUnicode_FromString(kv.first.c_str());
                    if (!element || !keyObj || g_py.PyDict_SetItem(dict, keyObj, element) != 0) {
                        if (element) g_py.Py_DecRef(element);
                        if (keyObj) g_py.Py_DecRef(keyObj);
                        g_py.Py_DecRef(dict);
                        g_py.PyErr_Clear();
                        return nullptr;
                    }
                    g_py.Py_DecRef(element);
                    g_py.Py_DecRef(keyObj);
                }
                return dict;
            }
        }
        return nullptr;
    }

    // 只读真实缓存 factory._config_cache[f"{instance_idx}_{group_id}"]；
    // factoryOut（可选）给调用方继续读该工厂的组缓存键。
    PyObject* CachedConfig(HostJob& job, const std::string& appId, long long instanceIdx,
                           const std::string& groupId, PyObject** factoryOut = nullptr) {
        PyObject* run = RunContext(job);
        if (!run) return nullptr;
        PyObject* factories = Attr(run, "_application_factory_map");
        g_py.Py_DecRef(run);
        if (!factories || !TypeIs(factories, g_py.pyDictType)) {
            if (factories) g_py.Py_DecRef(factories);
            return job.Fail(503, "host_not_ready", "宿主尚未完成应用注册") ? nullptr : nullptr;
        }
        PyObject* appIdObj = g_py.PyUnicode_FromString(appId.c_str());
        PyObject* factory = appIdObj ? g_py.PyObject_GetItem(factories, appIdObj) : nullptr;
        if (appIdObj) g_py.Py_DecRef(appIdObj);
        g_py.Py_DecRef(factories);
        if (!factory) {
            g_py.PyErr_Clear();
            job.Fail(404, "app_not_found", "应用未注册：" + appId);
            return nullptr;
        }
        if (factoryOut) {
            g_py.Py_IncRef(factory);
            *factoryOut = factory;
        }
        PyObject* cache = Attr(factory, "_config_cache");
        if (!cache || !TypeIs(cache, g_py.pyDictType)) {
            if (cache) g_py.Py_DecRef(cache);
            g_py.Py_DecRef(factory);
            job.Fail(503, "host_incompatible", "宿主工厂缺少 _config_cache");
            return nullptr;
        }
        PyObject* keyObj = g_py.PyUnicode_FromFormat("%d_%s", static_cast<int>(instanceIdx), groupId.c_str());
        PyObject* config = keyObj ? g_py.PyDict_GetItemWithError(cache, keyObj) : nullptr;  // borrowed
        if (keyObj) g_py.Py_DecRef(keyObj);
        g_py.Py_DecRef(cache);
        if (!config) {
            g_py.PyErr_Clear();
            g_py.Py_DecRef(factory);
            job.Fail(422, "config_not_cached",
                     "该实例与应用组没有已加载的配置缓存；本桥不自动创建，请在宿主界面打开一次该配置");
            return nullptr;
        }
        g_py.Py_IncRef(config);
        if (!factoryOut) g_py.Py_DecRef(factory);
        return config;  // new ref
    }

    // 收集某工厂在所有实例下已缓存的组名（describe 用；只读真实缓存键）。
    void CollectCachedGroups(PyObject* factory, std::vector<std::string>& groups) {
        PyObject* cache = Attr(factory, "_config_cache");
        if (!cache || !TypeIs(cache, g_py.pyDictType)) {
            if (cache) g_py.Py_DecRef(cache);
            return;
        }
        Py_ssize_t pos = 0;
        PyObject* k = nullptr;
        PyObject* v = nullptr;
        while (g_py.PyDict_Next(cache, &pos, &k, &v)) {
            std::string key = Utf8Of(k);
            size_t underscore = key.find('_');
            if (underscore != std::string::npos && underscore + 1 < key.size()) {
                std::string group = key.substr(underscore + 1);
                if (std::find(groups.begin(), groups.end(), group) == groups.end()) {
                    groups.push_back(group);
                }
            }
        }
        g_py.Py_DecRef(cache);
        std::sort(groups.begin(), groups.end());
    }

    void ReleaseContext() {
        if (!ctx) return;
        if (g_py.Py_IsInitialized && (!g_py.Py_IsFinalizing || g_py.Py_IsFinalizing() == 0)) {
            PyGilState state = g_py.PyGILState_Ensure();
            g_py.Py_DecRef(ctx);
            g_py.PyGILState_Release(state);
        }
        ctx = nullptr;
    }
};

// ----------------------------------------------------------------------
// 桥状态、请求 JSON 解析与七个工具路由
// ----------------------------------------------------------------------

// 桥运行状态：无 joinable 线程成员（工作线程全部 detach），停止只翻转
// 标志并关监听，不在 DllMain 或 GIL 边界做任何 join。
struct BridgeState {
    std::mutex authMutex;
    uint16_t port = ZZZ_BRIDGE_DEFAULT_PORT;
    std::string token;
    std::atomic<bool> running{false};
    std::atomic<int> activeConnections{0};
    JobHub jobs;
    HostReflector host;
    SOCKET listenSocket = INVALID_SOCKET;

    // 防护与鉴权复用纯逻辑单元的 VerifyHttpRequest。
    const char* VerifyRequest(const HttpRequest& request) {
        std::lock_guard<std::mutex> lock(authMutex);
        return VerifyHttpRequest(request, port, token);
    }
};

BridgeState g_state;

JobHub::Result DispatchHost(std::function<bool(HostJob&)> work, DWORD waitMs) {
    const uint64_t generation = g_requestGeneration ? g_requestGeneration : g_sessionGeneration.load();
    return g_state.jobs.Dispatch(
        [generation, work = std::move(work)](HostJob& job) {
            if (!g_state.running.load() || generation != g_sessionGeneration.load()) {
                return job.Fail(409, "bridge_session_changed", "桥会话已变化，本次未执行，请重新读取状态");
            }
            return work(job);
        }, waitMs);
}

struct ToolResponse {
    int status = 200;
    std::string json;
};

// ServeConnection 在定义之前使用；桥停止绝不结束宿主进程。
uint32_t StopBridgeCore();

bool ParseBodyObject(const std::string& body, JVar& out, ToolResponse& response) {
    if (body.empty()) {
        out = JVar{JVar::T::Obj};
        return true;
    }
    try {
        namespace W = winrt::Windows::Data::Json;
        W::JsonValue parsed = W::JsonValue::Parse(Widen(body));
        JVar value;
        if (!JsonToJVar(parsed, value) || value.t != JVar::T::Obj) {
            response.status = 400;
            response.json = ErrorJson("bad_json", "请求体必须是 JSON 对象");
            return false;
        }
        out = std::move(value);
        return true;
    } catch (...) {
        response.status = 400;
        response.json = ErrorJson("bad_json", "请求体不是合法 JSON");
        return false;
    }
}

const JVar* FindField(const JVar& object, const std::string& key) {
    for (const auto& kv : object.obj) {
        if (kv.first == key) return &kv.second;
    }
    return nullptr;
}

bool FieldString(const JVar& object, const std::string& key, std::string& out) {
    const JVar* field = FindField(object, key);
    if (!field || field->t != JVar::T::Str) return false;
    out = field->s;
    return true;
}

bool FieldInteger(const JVar& object, const std::string& key, long long& out) {
    const JVar* field = FindField(object, key);
    if (!field || field->t != JVar::T::Int) return false;
    out = field->i;
    return true;
}

// 主线程闭包错误 → HTTP 响应。
void ApplyDispatchFailure(const JobHub::Result& result, ToolResponse& response) {
    if (result.outcome == DispatchOutcome::Executed && !result.ok) {
        response.status = result.failStatus;
        response.json = ErrorJson(result.failCode, result.failMessage);
        return;
    }
    if (result.outcome == DispatchOutcome::QueueTimeout) {
        response.status = 503;
        response.json = ErrorJson("host_busy", "宿主主线程未处理排队请求，本次已跳过，请稍后重试");
        return;
    }
    if (result.outcome == DispatchOutcome::RunTimeout) {
        response.status = 504;
        response.json = ErrorJson("dispatch_timeout", "宿主主线程执行超时，结果未知；不会自动重试");
        return;
    }
    response.status = 503;
    response.json = ErrorJson("queue_full",
                              "未能投递到宿主执行队列（无可用 PySide6 调度上下文、队列已满或已超请求期限），"
                              "本次未执行，请稍后重试");
}

// 读取运行状态的小闭包（复用于 status / api.read / 轮询）。
JobHub::Result DispatchStatusProbe(std::shared_ptr<OpContext> data, bool withCatalog) {
    return DispatchHost(
        [data, withCatalog](HostJob& job) {
            HostReflector& host = g_state.host;
            PyObject* run = host.RunContext(job);
            if (!run) return false;
            auto flag = [&](const char* attr) {
                PyObject* value = host.Attr(run, attr);
                bool result = false;
                if (value) {
                    result = g_py.PyObject_IsTrue(value) == 1;
                    g_py.Py_DecRef(value);
                }
                return result;
            };
            bool stop = flag("is_context_stop");
            bool pause = flag("is_context_pause");
            data->state = stop ? "stop" : (pause ? "pause" : "running");
            auto text = [&](const char* attr, std::string& out) {
                PyObject* value = host.Attr(run, attr);
                if (value) {
                    out = host.Utf8Of(value);
                    g_py.Py_DecRef(value);
                }
            };
            auto number = [&](const char* attr, long long& out) {
                PyObject* value = host.Attr(run, attr);
                if (value) {
                    out = g_py.PyLong_AsLongLong(value);
                    g_py.Py_DecRef(value);
                }
            };
            text("current_app_id", data->currentApp);
            text("current_group_id", data->currentGroup);
            number("current_instance_idx", data->currentInstance);
            PyObject* lastRun = host.Attr(run, "last_run_result");
            if (lastRun) {
                if (lastRun != g_py.pyNone) {
                    data->hasLastRun = true;
                    if (!host.VarFromPython(job, lastRun, 0, data->lastRunResult)) {
                        g_py.Py_DecRef(lastRun);
                        g_py.Py_DecRef(run);
                        return false;
                    }
                }
                g_py.Py_DecRef(lastRun);
            }
            PyObject* lastApp = host.Attr(run, "last_application_result");
            if (lastApp) {
                if (lastApp != g_py.pyNone) {
                    if (!host.VarFromPython(job, lastApp, 0, data->lastAppResult)) {
                        g_py.Py_DecRef(lastApp);
                        g_py.Py_DecRef(run);
                        return false;
                    }
                }
                g_py.Py_DecRef(lastApp);
            }
            g_py.Py_DecRef(run);
            PyObject* ready = host.Attr(g_state.host.ctx, "ready_for_application");
            if (ready) {
                data->ready = g_py.PyObject_IsTrue(ready) == 1;
                g_py.Py_DecRef(ready);
            }
            if (withCatalog && !host.CollectCatalog(job, data->catalog)) return false;
            return true;
        },
        kDispatchWaitMs);
}

void ToolStatus(const JVar& /*body*/, ToolResponse& response) {
    auto data = std::make_shared<OpContext>();
    JobHub::Result result = DispatchStatusProbe(data, true);
    if (result.outcome != DispatchOutcome::Executed || !result.ok) {
        ApplyDispatchFailure(result, response);
        return;
    }
    JVar host{JVar::T::Obj};
    host.obj.emplace_back("projectName", JVar{JVar::T::Str, false, 0, 0, data->catalog.projectName});
    host.obj.emplace_back("attached", JVar{JVar::T::Bool, true});
    host.obj.emplace_back("readyForApplication", JVar{JVar::T::Bool, data->ready});
    host.obj.emplace_back("contextState", JVar{JVar::T::Str, false, 0, 0, data->state});
    JVar current{JVar::T::Obj};
    current.obj.emplace_back("appId",
                             data->currentApp.empty() ? JVar{JVar::T::Null}
                                                      : JVar{JVar::T::Str, false, 0, 0, data->currentApp});
    current.obj.emplace_back("instanceIdx",
                             data->currentInstance < 0 ? JVar{JVar::T::Null}
                                                       : JVar{JVar::T::Int, false, data->currentInstance});
    current.obj.emplace_back("currentGroup",
                             data->currentGroup.empty() ? JVar{JVar::T::Null}
                                                        : JVar{JVar::T::Str, false, 0, 0, data->currentGroup});
    host.obj.emplace_back("current", std::move(current));
    host.obj.emplace_back("lastRunResult",
                          data->hasLastRun ? data->lastRunResult : JVar{JVar::T::Null});
    JVar instances{JVar::T::Arr};
    for (const auto& instance : data->catalog.instances) {
        JVar item{JVar::T::Obj};
        item.obj.emplace_back("idx", JVar{JVar::T::Int, false, instance.idx});
        item.obj.emplace_back("name", JVar{JVar::T::Str, false, 0, 0, instance.name});
        item.obj.emplace_back("active", JVar{JVar::T::Bool, instance.active});
        instances.arr.push_back(std::move(item));
    }
    host.obj.emplace_back("instances", std::move(instances));
    host.obj.emplace_back("currentInstanceIdx", JVar{JVar::T::Int, false, data->catalog.currentInstanceIdx});
    // 宿主代码版本需要执行 git 命令，桥不代取；以目录 hash 代替。
    host.obj.emplace_back("hostVersion", JVar{JVar::T::Null});
    JVar root{JVar::T::Obj};
    root.obj.emplace_back("bridge", JVar{JVar::T::Str, false, 0, 0, kBridgeName});
    root.obj.emplace_back("bridgeVersion", JVar{JVar::T::Str, false, 0, 0, kBridgeVersion});
    root.obj.emplace_back("host", std::move(host));
    root.obj.emplace_back("catalogHash",
                          JVar{JVar::T::Str, false, 0, 0, data->catalog.Hash()});
    root.obj.emplace_back("appCount", JVar{JVar::T::Int, false, (long long)data->catalog.apps.size()});
    response.json = SerializeJVar(root);
}

void ToolCatalogSearch(const JVar& body, ToolResponse& response) {
    std::string query;
    bool includeInternal = false;
    FieldString(body, "query", query);
    if (const JVar* flag = FindField(body, "includeInternal")) {
        includeInternal = flag->t == JVar::T::Bool && flag->b;
    }
    auto data = std::make_shared<OpContext>();
    JobHub::Result result = DispatchHost(
        [data](HostJob& job) { return g_state.host.CollectCatalog(job, data->catalog); },
        kDispatchWaitMs);
    if (result.outcome != DispatchOutcome::Executed || !result.ok) {
        ApplyDispatchFailure(result, response);
        return;
    }
    JVar apps{JVar::T::Arr};
    size_t total = 0;
    std::string needle;
    for (char c : query) needle.push_back(static_cast<char>(std::tolower(static_cast<unsigned char>(c))));
    for (const auto& app : data->catalog.apps) {
        if (app.internalApp && !includeInternal) continue;
        if (!needle.empty()) {
            std::string haystack = app.appId + "\n" + app.appName;
            for (char& c : haystack) c = static_cast<char>(std::tolower(static_cast<unsigned char>(c)));
            if (haystack.find(needle) == std::string::npos) continue;
        }
        ++total;
        JVar item{JVar::T::Obj};
        item.obj.emplace_back("appId", JVar{JVar::T::Str, false, 0, 0, app.appId});
        item.obj.emplace_back("appName", JVar{JVar::T::Str, false, 0, 0, app.appName});
        item.obj.emplace_back("defaultGroup", JVar{JVar::T::Bool, app.defaultGroup});
        item.obj.emplace_back("internalApp", JVar{JVar::T::Bool, app.internalApp});
        item.obj.emplace_back("needNotify", JVar{JVar::T::Bool, app.needNotify});
        apps.arr.push_back(std::move(item));
    }
    JVar instances{JVar::T::Arr};
    for (const auto& instance : data->catalog.instances) {
        JVar item{JVar::T::Obj};
        item.obj.emplace_back("idx", JVar{JVar::T::Int, false, instance.idx});
        item.obj.emplace_back("name", JVar{JVar::T::Str, false, 0, 0, instance.name});
        item.obj.emplace_back("active", JVar{JVar::T::Bool, instance.active});
        instances.arr.push_back(std::move(item));
    }
    JVar root{JVar::T::Obj};
    root.obj.emplace_back("apps", std::move(apps));
    root.obj.emplace_back("total", JVar{JVar::T::Int, false, (long long)total});
    root.obj.emplace_back("catalogHash", JVar{JVar::T::Str, false, 0, 0, data->catalog.Hash()});
    root.obj.emplace_back("instances", std::move(instances));
    response.json = SerializeJVar(root);
}

void ToolCatalogDescribe(const JVar& body, ToolResponse& response) {
    std::string appId;
    if (!FieldString(body, "appId", appId) || !IsValidAppId(appId)) {
        response.status = 400;
        response.json = ErrorJson("app_id_required", "describe 必须携带合法 appId");
        return;
    }
    auto data = std::make_shared<OpContext>();
    JobHub::Result result = DispatchHost(
        [data, appId](HostJob& job) {
            HostReflector& host = g_state.host;
            if (!host.CollectCatalog(job, data->catalog)) return false;
            const AppEntry* entry = nullptr;
            for (const auto& app : data->catalog.apps) {
                if (app.appId == appId) entry = &app;
            }
            if (!entry) return job.Fail(404, "app_not_found", "应用未注册：" + appId);
            PyObject* run = host.RunContext(job);
            if (!run) return false;
            PyObject* factories = host.Attr(run, "_application_factory_map");
            g_py.Py_DecRef(run);
            if (!factories) return job.Fail(503, "host_not_ready", "宿主尚未完成应用注册");
            PyObject* appIdObj = g_py.PyUnicode_FromString(appId.c_str());
            PyObject* factory = appIdObj ? g_py.PyObject_GetItem(factories, appIdObj) : nullptr;
            if (appIdObj) g_py.Py_DecRef(appIdObj);
            g_py.Py_DecRef(factories);
            if (!factory) {
                g_py.PyErr_Clear();
                return job.Fail(404, "app_not_found", "应用未注册：" + appId);
            }
            host.CollectCachedGroups(factory, data->cachedGroups);
            bool configDescribed = false;
            long long instanceIdx = data->catalog.currentInstanceIdx;
            if (!data->cachedGroups.empty() && instanceIdx >= 0) {
                PyObject* cache = host.Attr(factory, "_config_cache");
                if (cache && TypeIs(cache, g_py.pyDictType)) {
                    // 取当前实例第一个已缓存组描述字段结构。
                    PyObject* keyObj = g_py.PyUnicode_FromFormat("%d_%s", static_cast<int>(instanceIdx),
                                                                data->cachedGroups.front().c_str());
                    PyObject* config = keyObj ? g_py.PyDict_GetItemWithError(cache, keyObj) : nullptr;
                    if (keyObj) g_py.Py_DecRef(keyObj);
                    if (config) {
                        PyObject* configData = host.Attr(config, "data");
                        if (configData && TypeIs(configData, g_py.pyDictType)) {
                            JVar fields;
                            if (host.VarFromPython(job, configData, 0, fields)) {
                                data->configData = std::move(fields);
                                data->keyed = true;
                                data->configFound = true;
                                configDescribed = true;
                            }
                        } else if (configData) {
                            data->keyed = false;
                            configDescribed = true;
                        }
                        if (configData) g_py.Py_DecRef(configData);
                    } else {
                        g_py.PyErr_Clear();
                    }
                }
                if (cache) g_py.Py_DecRef(cache);
            }
            if (!configDescribed) {
                data->describeNote = "当前实例没有该应用的已加载配置缓存；字段结构请在宿主打开一次配置后查询";
            }
            g_py.Py_DecRef(factory);
            return true;
        },
        kDispatchWaitMs);
    if (result.outcome != DispatchOutcome::Executed || !result.ok) {
        ApplyDispatchFailure(result, response);
        return;
    }
    const AppEntry* entry = nullptr;
    for (const auto& app : data->catalog.apps) {
        if (app.appId == appId) entry = &app;
    }
    JVar root{JVar::T::Obj};
    root.obj.emplace_back("appId", JVar{JVar::T::Str, false, 0, 0, entry->appId});
    root.obj.emplace_back("appName", JVar{JVar::T::Str, false, 0, 0, entry->appName});
    root.obj.emplace_back("defaultGroup", JVar{JVar::T::Bool, entry->defaultGroup});
    root.obj.emplace_back("internalApp", JVar{JVar::T::Bool, entry->internalApp});
    root.obj.emplace_back("needNotify", JVar{JVar::T::Bool, entry->needNotify});
    root.obj.emplace_back("catalogHash", JVar{JVar::T::Str, false, 0, 0, data->catalog.Hash()});
    JVar groups{JVar::T::Arr};
    for (const auto& group : data->cachedGroups) {
        groups.arr.emplace_back(JVar{JVar::T::Str, false, 0, 0, group});
    }
    root.obj.emplace_back("cachedGroups", std::move(groups));
    if (data->configFound && data->keyed) {
        JVar fields{JVar::T::Arr};
        std::vector<std::pair<std::string, JVar>> sorted(data->configData.obj);
        std::sort(sorted.begin(), sorted.end(),
                  [](const auto& a, const auto& b) { return a.first < b.first; });
        for (const auto& kv : sorted) {
            JVar item{JVar::T::Obj};
            item.obj.emplace_back("key", JVar{JVar::T::Str, false, 0, 0, kv.first});
            item.obj.emplace_back("valueType", JVar{JVar::T::Str, false, 0, 0, TypeNameOf(kv.second)});
            item.obj.emplace_back("sensitive", JVar{JVar::T::Bool, IsSensitiveKey(kv.first)});
            item.obj.emplace_back("writable",
                                  JVar{JVar::T::Bool, !IsSensitiveKey(kv.first) && !IsForbiddenWriteKey(kv.first)});
            fields.arr.push_back(std::move(item));
        }
        root.obj.emplace_back("configFields", std::move(fields));
    } else if (!data->keyed) {
        root.obj.emplace_back("configUnavailable",
                              JVar{JVar::T::Str, false, 0, 0, "该配置根不是键值结构，不能通过本桥管理"});
    } else {
        root.obj.emplace_back("configUnavailable", JVar{JVar::T::Str, false, 0, 0, data->describeNote});
    }
    response.json = SerializeJVar(root);
}

void ToolApiRead(const JVar& body, ToolResponse& response) {
    const JVar* fields = FindField(body, "fields");
    if (!fields || fields->t != JVar::T::Arr || fields->arr.empty() || fields->arr.size() > 16) {
        response.status = 400;
        response.json = ErrorJson("fields_required", "api.read 必须携带 1-16 个 fields");
        return;
    }
    static const char* kAllowed[] = {
        "context.state", "context.current", "context.lastRunResult",
        "context.lastApplicationResult", "context.ready", "instance.currentIdx",
    };
    for (const auto& field : fields->arr) {
        if (field.t != JVar::T::Str) {
            response.status = 422;
            response.json = ErrorJson("field_not_allowed", "fields 只允许字符串白名单");
            return;
        }
        bool known = false;
        for (const char* allowed : kAllowed) {
            if (field.s == allowed) known = true;
        }
        if (!known) {
            response.status = 422;
            response.json = ErrorJson("field_not_allowed", "不允许读取的字段：" + field.s);
            return;
        }
    }
    auto data = std::make_shared<OpContext>();
    JobHub::Result result = DispatchStatusProbe(data, false);
    if (result.outcome != DispatchOutcome::Executed || !result.ok) {
        ApplyDispatchFailure(result, response);
        return;
    }
    JobHub::Result instanceResult = DispatchHost(
        [data](HostJob& job) {
            PyObject* value = g_state.host.Attr(g_state.host.ctx, "current_instance_idx");
            if (!value) return job.Fail(503, "host_not_ready", "宿主尚未设置当前实例");
            data->currentInstance = g_py.PyLong_AsLongLong(value);
            g_py.Py_DecRef(value);
            return true;
        },
        kDispatchWaitMs);
    if (instanceResult.outcome != DispatchOutcome::Executed || !instanceResult.ok) {
        ApplyDispatchFailure(instanceResult, response);
        return;
    }
    JVar values{JVar::T::Obj};
    for (const auto& field : fields->arr) {
        if (field.s == "context.state") {
            values.obj.emplace_back(field.s, JVar{JVar::T::Str, false, 0, 0, data->state});
        } else if (field.s == "context.ready") {
            values.obj.emplace_back(field.s, JVar{JVar::T::Bool, data->ready});
        } else if (field.s == "instance.currentIdx") {
            values.obj.emplace_back(field.s, JVar{JVar::T::Int, false, data->currentInstance});
        } else if (field.s == "context.lastRunResult") {
            values.obj.emplace_back(field.s, data->lastRunResult);
        } else if (field.s == "context.lastApplicationResult") {
            values.obj.emplace_back(field.s, data->lastAppResult);
        } else {  // context.current
            JVar current{JVar::T::Obj};
            current.obj.emplace_back(
                "appId", data->currentApp.empty() ? JVar{JVar::T::Null}
                                                  : JVar{JVar::T::Str, false, 0, 0, data->currentApp});
            current.obj.emplace_back(
                "instanceIdx", data->currentInstance < 0 ? JVar{JVar::T::Null}
                                                         : JVar{JVar::T::Int, false, data->currentInstance});
            current.obj.emplace_back(
                "groupId", data->currentGroup.empty() ? JVar{JVar::T::Null}
                                                      : JVar{JVar::T::Str, false, 0, 0, data->currentGroup});
            values.obj.emplace_back(field.s, std::move(current));
        }
    }
    JVar root{JVar::T::Obj};
    root.obj.emplace_back("values", std::move(values));
    response.json = SerializeJVar(root);
}

// 校验 catalogHash 并取回目录数据；失败时已写响应。
bool CheckCatalogHash(const JVar& body, CatalogData& catalog, ToolResponse& response) {
    std::string expected;
    if (!FieldString(body, "catalogHash", expected)) {
        response.status = 400;
        response.json = ErrorJson("catalog_hash_required", "本操作必须携带 status 或目录工具返回的 catalogHash");
        return false;
    }
    auto data = std::make_shared<OpContext>();
    JobHub::Result result = DispatchHost(
        [data](HostJob& job) { return g_state.host.CollectCatalog(job, data->catalog); },
        kDispatchWaitMs);
    if (result.outcome != DispatchOutcome::Executed || !result.ok) {
        ApplyDispatchFailure(result, response);
        return false;
    }
    std::string actual = data->catalog.Hash();
    if (!SecureTokenEqual(expected, actual)) {
        response.status = 409;
        response.json = ErrorJson("catalog_hash_mismatch", "宿主应用目录已变化，请重新获取目录后再操作");
        return false;
    }
    catalog = std::move(data->catalog);
    return true;
}

void ToolApiInvoke(const JVar& body, ToolResponse& response) {
    CatalogData catalog;
    if (!CheckCatalogHash(body, catalog, response)) return;
    std::string method;
    if (!FieldString(body, "method", method)) {
        response.status = 400;
        response.json = ErrorJson("bad_method", "method 只支持 run 或 stop");
        return;
    }
    if (method == "stop") {
        // stop_running 返回的是逻辑停止（状态机收口），不代表工作线程已结束；
        // 确认窗口内未观察到停止则如实返回 unknown。
        auto stopData = std::make_shared<OpContext>();
        JobHub::Result stopResult = DispatchHost(
            [stopData](HostJob& job) {
                PyObject* run = g_state.host.RunContext(job);
                if (!run) return false;
                PyObject* stop = g_state.host.Attr(run, "stop_running");
                g_py.Py_DecRef(run);
                if (!stop) return job.Fail(503, "host_incompatible", "宿主缺少 stop_running");
                PyObject* result = g_py.PyObject_CallObject(stop, nullptr);
                g_py.Py_DecRef(stop);
                if (!result) {
                    g_py.PyErr_Clear();
                    return job.Fail(500, "invoke_failed", "stop_running 调用失败");
                }
                g_py.Py_DecRef(result);
                return true;
            },
            kDispatchWaitMs);
        if (stopResult.outcome != DispatchOutcome::Executed || !stopResult.ok) {
            ApplyDispatchFailure(stopResult, response);
            return;
        }
        DWORD deadline = ::GetTickCount() + kStopPollTotalMs;
        std::string stopStatus = "unknown";
        while (::GetTickCount() < deadline) {
            ::Sleep(50);
            auto probe = std::make_shared<OpContext>();
            JobHub::Result state = DispatchStatusProbe(probe, false);
            if (state.outcome == DispatchOutcome::Executed && state.ok) {
                if (probe->state == "stop") {
                    stopStatus = "stopped";
                    break;
                }
            } else if (state.outcome == DispatchOutcome::QueueTimeout ||
                       state.outcome == DispatchOutcome::RunTimeout) {
                break;  // 主线程忙/执行中卡住：不重试，保持 unknown
            }
        }
        auto final = std::make_shared<OpContext>();
        JobHub::Result finalProbe = DispatchStatusProbe(final, false);
        JVar root{JVar::T::Obj};
        root.obj.emplace_back("stopStatus", JVar{JVar::T::Str, false, 0, 0, stopStatus});
        if (finalProbe.outcome == DispatchOutcome::Executed && finalProbe.ok) {
            root.obj.emplace_back("contextState", JVar{JVar::T::Str, false, 0, 0, final->state});
            root.obj.emplace_back("lastRunResult",
                                  final->hasLastRun ? final->lastRunResult : JVar{JVar::T::Null});
        } else {
            root.obj.emplace_back("contextState", JVar{JVar::T::Str, false, 0, 0, "unknown"});
            root.obj.emplace_back("lastRunResult", JVar{JVar::T::Null});
        }
        root.obj.emplace_back(
            "note",
            JVar{JVar::T::Str, false, 0, 0,
                 stopStatus == "stopped"
                     ? "已确认宿主运行状态机进入停止"
                     : "停止请求已发出，但确认窗口内未观察到停止（stop_running 只代表逻辑停止）；状态未知，稍后用 api.read 复核"});
        response.json = SerializeJVar(root);
        return;
    }
    if (method != "run") {
        response.status = 400;
        response.json = ErrorJson("bad_method", "method 只支持 run 或 stop");
        return;
    }
    std::string appId, groupId;
    long long instanceIdx = -1;
    if (!FieldString(body, "appId", appId) || !FieldInteger(body, "instanceIdx", instanceIdx)) {
        response.status = 400;
        response.json = ErrorJson("run_target_required", "run 必须携带 appId 与整型 instanceIdx");
        return;
    }
    if (!IsValidAppId(appId)) {
        response.status = 422;
        response.json = ErrorJson("bad_app_id", "appId 含非法字符");
        return;
    }
    if (const JVar* group = FindField(body, "groupId")) {
        if (group->t != JVar::T::Str || !IsValidGroupId(group->s)) {
            response.status = 422;
            response.json = ErrorJson("bad_group_id", "groupId 含非法字符");
            return;
        }
        groupId = group->s;
    } else {
        groupId = "one_dragon";
    }
    const AppEntry* entry = nullptr;
    for (const auto& app : catalog.apps) {
        if (app.appId == appId) entry = &app;
    }
    if (!entry) {
        response.status = 404;
        response.json = ErrorJson("app_not_found", "应用未注册：" + appId);
        return;
    }
    if (entry->internalApp) {
        response.status = 422;
        response.json = ErrorJson("internal_app_forbidden", "应用 " + appId + " 只能由宿主固定入口调用");
        return;
    }
    bool instanceKnown = false;
    for (const auto& instance : catalog.instances) {
        if (instance.idx == instanceIdx) instanceKnown = true;
    }
    if (!instanceKnown) {
        response.status = 422;
        response.json = ErrorJson("unknown_instance", "实例不存在，请以目录实例列表为准");
        return;
    }
    // run_application_async(app_id=..., instance_idx=..., group_id=...)：
    // 真实方法返回 bool 只表示是否提交线程池，不代表执行完成。
    auto runData = std::make_shared<OpContext>();
    JobHub::Result runResult = DispatchHost(
        [runData, appId, instanceIdx, groupId](HostJob& job) {
            HostReflector& host = g_state.host;
            PyObject* run = host.RunContext(job);
            if (!run) return false;
            bool stopping = false;
            PyObject* stopFlag = host.Attr(run, "is_context_stop");
            if (stopFlag) {
                stopping = g_py.PyObject_IsTrue(stopFlag) == 1;
                g_py.Py_DecRef(stopFlag);
            }
            if (!stopping) {
                g_py.Py_DecRef(run);
                return job.Fail(409, "already_running", "宿主当前有任务在运行，请先停止");
            }
            PyObject* submit = host.Attr(run, "run_application_async");
            g_py.Py_DecRef(run);
            if (!submit) return job.Fail(503, "host_incompatible", "宿主缺少 run_application_async");
            PyObject* args = g_py.PyTuple_New(0);
            PyObject* kwargs = g_py.PyDict_New();
            if (!args || !kwargs) {
                if (args) g_py.Py_DecRef(args);
                if (kwargs) g_py.Py_DecRef(kwargs);
                g_py.Py_DecRef(submit);
                g_py.PyErr_Clear();
                return job.Fail(500, "invoke_failed", "构造调用参数失败");
            }
            auto setItem = [&](const char* name, PyObject* value) {
                g_py.PyDict_SetItemString(kwargs, name, value);
                g_py.Py_DecRef(value);
            };
            setItem("app_id", g_py.PyUnicode_FromString(appId.c_str()));
            setItem("instance_idx", g_py.PyLong_FromLongLong(instanceIdx));
            setItem("group_id", g_py.PyUnicode_FromString(groupId.c_str()));
            PyObject* submitted = g_py.PyObject_Call(submit, args, kwargs);
            g_py.Py_DecRef(args);
            g_py.Py_DecRef(kwargs);
            g_py.Py_DecRef(submit);
            if (!submitted) {
                g_py.PyErr_Clear();
                return job.Fail(500, "invoke_failed", "run_application_async 调用失败");
            }
            runData->submitted = g_py.PyObject_IsTrue(submitted) == 1;
            g_py.Py_DecRef(submitted);
            return true;
        },
        kDispatchWaitMs);
    if (runResult.outcome != DispatchOutcome::Executed || !runResult.ok) {
        ApplyDispatchFailure(runResult, response);
        return;
    }
    if (!runData->submitted) {
        JVar root{JVar::T::Obj};
        root.obj.emplace_back("accepted", JVar{JVar::T::Bool, false});
        root.obj.emplace_back("submissionStatus", JVar{JVar::T::Str, false, 0, 0, "notSubmitted"});
        JVar error{JVar::T::Obj};
        error.obj.emplace_back("code", JVar{JVar::T::Str, false, 0, 0, "submission_rejected"});
        error.obj.emplace_back("message", JVar{JVar::T::Str, false, 0, 0, "宿主拒绝提交，本次未启动应用"});
        root.obj.emplace_back("error", std::move(error));
        root.obj.emplace_back(
            "reason",
            JVar{JVar::T::Str, false, 0, 0, "宿主拒绝提交（可能刚好有其他任务启动），请稍后用 api.read 核对状态"});
        response.status = 409;
        response.json = SerializeJVar(root);
        return;
    }
    auto probe = std::make_shared<OpContext>();
    JobHub::Result state = DispatchStatusProbe(probe, false);
    JVar root{JVar::T::Obj};
    root.obj.emplace_back("accepted", JVar{JVar::T::Bool, true});
    JVar verification{JVar::T::Obj};
    verification.obj.emplace_back("status", JVar{JVar::T::Str, false, 0, 0, "succeeded"});
    verification.obj.emplace_back("scope", JVar{JVar::T::Str, false, 0, 0, "submission"});
    root.obj.emplace_back("verification", std::move(verification));
    root.obj.emplace_back("appId", JVar{JVar::T::Str, false, 0, 0, appId});
    root.obj.emplace_back("instanceIdx", JVar{JVar::T::Int, false, instanceIdx});
    root.obj.emplace_back("groupId", JVar{JVar::T::Str, false, 0, 0, groupId});
    root.obj.emplace_back(
        "contextState",
        JVar{JVar::T::Str, false, 0, 0,
             state.outcome == DispatchOutcome::Executed && state.ok ? probe->state : "unknown"});
    root.obj.emplace_back(
        "note",
        JVar{JVar::T::Str, false, 0, 0,
             "已提交运行请求（submitted），不代表执行完成；用 api.read 跟进 context.state 与 context.lastRunResult"});
    response.json = SerializeJVar(root);
}

// config.read / config.update 共同的目标解析。
bool ParseConfigTarget(const JVar& body, std::string& appId, long long& instanceIdx,
                       std::string& groupId, ToolResponse& response) {
    if (!FieldString(body, "appId", appId) || !IsValidAppId(appId)) {
        response.status = 400;
        response.json = ErrorJson("app_id_required", "必须携带合法 appId");
        return false;
    }
    if (!FieldInteger(body, "instanceIdx", instanceIdx) || instanceIdx < 0) {
        response.status = 400;
        response.json = ErrorJson("instance_idx_required", "必须携带非负整型 instanceIdx");
        return false;
    }
    if (const JVar* group = FindField(body, "groupId")) {
        if (group->t != JVar::T::Str || !IsValidGroupId(group->s)) {
            response.status = 422;
            response.json = ErrorJson("bad_group_id", "groupId 含非法字符");
            return false;
        }
        groupId = group->s;
    } else {
        groupId = "one_dragon";
    }
    return true;
}

void ToolConfigRead(const JVar& body, ToolResponse& response) {
    std::string appId, groupId;
    long long instanceIdx = -1;
    if (!ParseConfigTarget(body, appId, instanceIdx, groupId, response)) return;
    auto data = std::make_shared<OpContext>();
    JobHub::Result result = DispatchHost(
        [data, appId, instanceIdx, groupId](HostJob& job) {
            HostReflector& host = g_state.host;
            PyObject* config = host.CachedConfig(job, appId, instanceIdx, groupId);
            if (!config) return false;
            PyObject* configData = host.Attr(config, "data");
            g_py.Py_DecRef(config);
            if (!configData) {
                return job.Fail(503, "host_incompatible", "宿主配置缺少 data 属性");
            }
            if (!TypeIs(configData, g_py.pyDictType)) {
                g_py.Py_DecRef(configData);
                return job.Fail(422, "config_not_keyed", "该配置根不是键值结构，不能通过本桥管理");
            }
            bool ok = host.VarFromPython(job, configData, 0, data->configData);
            g_py.Py_DecRef(configData);
            data->configFound = true;
            return ok;
        },
        kDispatchWaitMs);
    if (result.outcome != DispatchOutcome::Executed || !result.ok) {
        ApplyDispatchFailure(result, response);
        return;
    }
    std::string configHash = CanonicalHash(data->configData);
    JVar masked = data->configData;
    MaskSensitive(masked);
    JVar maskedKeys{JVar::T::Arr};
    for (const auto& kv : data->configData.obj) {
        if (IsSensitiveKey(kv.first)) {
            maskedKeys.arr.emplace_back(JVar{JVar::T::Str, false, 0, 0, kv.first});
        }
    }
    JVar root{JVar::T::Obj};
    root.obj.emplace_back("appId", JVar{JVar::T::Str, false, 0, 0, appId});
    root.obj.emplace_back("instanceIdx", JVar{JVar::T::Int, false, instanceIdx});
    root.obj.emplace_back("groupId", JVar{JVar::T::Str, false, 0, 0, groupId});
    root.obj.emplace_back("configHash", JVar{JVar::T::Str, false, 0, 0, configHash});
    root.obj.emplace_back("data", std::move(masked));
    root.obj.emplace_back("maskedKeys", std::move(maskedKeys));
    response.json = SerializeJVar(root);
}

void ToolConfigUpdate(const JVar& body, ToolResponse& response) {
    CatalogData catalog;
    if (!CheckCatalogHash(body, catalog, response)) return;
    std::string expectedConfigHash;
    if (!FieldString(body, "configHash", expectedConfigHash)) {
        response.status = 400;
        response.json = ErrorJson("config_hash_required", "config.update 必须携带 config.read 返回的 configHash");
        return;
    }
    std::string appId, groupId;
    long long instanceIdx = -1;
    if (!ParseConfigTarget(body, appId, instanceIdx, groupId, response)) return;
    const JVar* updates = FindField(body, "updates");
    if (!updates || updates->t != JVar::T::Arr || updates->arr.empty() || updates->arr.size() > 32) {
        response.status = 400;
        response.json = ErrorJson("updates_required", "updates 必须是 1-32 个 {key,value} 项");
        return;
    }
    struct UpdateItem {
        std::string key;
        JVar value;
    };
    std::vector<UpdateItem> items;
    for (const auto& raw : updates->arr) {
        if (raw.t != JVar::T::Obj || raw.obj.size() != 2) {
            response.status = 400;
            response.json = ErrorJson("bad_update_item", "每个更新项必须只含 key 与 value");
            return;
        }
        UpdateItem item;
        bool hasKey = false, hasValue = false;
        for (const auto& kv : raw.obj) {
            if (kv.first == "key") {
                if (kv.second.t != JVar::T::Str) {
                    response.status = 422;
                    response.json = ErrorJson("bad_config_key", "key 必须是字符串");
                    return;
                }
                item.key = kv.second.s;
                hasKey = true;
            } else if (kv.first == "value") {
                item.value = kv.second;
                hasValue = true;
            }
        }
        if (!hasKey || !hasValue) {
            response.status = 400;
            response.json = ErrorJson("bad_update_item", "每个更新项必须同时有 key 与 value");
            return;
        }
        if (item.key.empty() || item.key.size() > 64) {
            response.status = 422;
            response.json = ErrorJson("bad_config_key", "配置键必须是 1-64 字符");
            return;
        }
        if (IsSensitiveKey(item.key)) {
            response.status = 422;
            response.json = ErrorJson("sensitive_key_forbidden",
                                      "字段 " + item.key + " 属于通知账号或密钥类，禁止通过本桥读写");
            return;
        }
        if (IsForbiddenWriteKey(item.key)) {
            response.status = 422;
            response.json = ErrorJson("forbidden_key",
                                      "字段 " + item.key + " 涉及路径、脚本、流水线或启动命令，禁止通过本桥写入");
            return;
        }
        std::string why;
        if (!IsSafeWriteValue(item.value, 0, why)) {
            response.status = 422;
            response.json = ErrorJson("unsafe_config_value", "字段 " + item.key + " 的值被拒绝：" + why);
            return;
        }
        items.push_back(std::move(item));
    }
    const AppEntry* entry = nullptr;
    for (const auto& app : catalog.apps) {
        if (app.appId == appId) entry = &app;
    }
    if (!entry) {
        response.status = 404;
        response.json = ErrorJson("app_not_found", "应用未注册：" + appId);
        return;
    }
    if (entry->internalApp) {
        response.status = 422;
        response.json = ErrorJson("internal_app_forbidden", "应用 " + appId + " 只能由宿主固定入口调用");
        return;
    }
    bool instanceKnown = false;
    for (const auto& instance : catalog.instances) {
        if (instance.idx == instanceIdx) instanceKnown = true;
    }
    if (!instanceKnown) {
        response.status = 422;
        response.json = ErrorJson("unknown_instance", "实例不存在，请以目录实例列表为准");
        return;
    }
    // configHash 校验、写入、回读核验在同一个主线程闭包内完成；
    // 任一环节失败恢复快照，绝不留下半写状态。items 按值捕获：
    // 排队/执行超时后闭包延迟执行也只碰自己的存储，不触碰调用方栈。
    auto data = std::make_shared<OpContext>();
    JobHub::Result result = DispatchHost(
        [data, appId, instanceIdx, groupId, expectedConfigHash, items](HostJob& job) {
            HostReflector& host = g_state.host;
            PyObject* config = host.CachedConfig(job, appId, instanceIdx, groupId);
            if (!config) return false;
            PyObject* configData = host.Attr(config, "data");
            if (!configData || !TypeIs(configData, g_py.pyDictType)) {
                if (configData) g_py.Py_DecRef(configData);
                g_py.Py_DecRef(config);
                return job.Fail(422, "config_not_keyed", "该配置根不是键值结构，不能通过本桥管理");
            }
            JVar current;
            if (!host.VarFromPython(job, configData, 0, current)) {
                g_py.Py_DecRef(configData);
                g_py.Py_DecRef(config);
                return false;
            }
            if (!SecureTokenEqual(expectedConfigHash, CanonicalHash(current))) {
                g_py.Py_DecRef(configData);
                g_py.Py_DecRef(config);
                return job.Fail(409, "config_hash_mismatch",
                                "配置在读取之后已被宿主修改，请重新 config.read 后再更新");
            }
            for (const auto& item : items) {
                PyObject* keyObj = g_py.PyUnicode_FromString(item.key.c_str());
                PyObject* present = keyObj ? g_py.PyDict_GetItemWithError(configData, keyObj) : nullptr;
                bool exists = present != nullptr;
                if (!exists) g_py.PyErr_Clear();
                if (keyObj) g_py.Py_DecRef(keyObj);
                if (!exists) {
                    g_py.Py_DecRef(configData);
                    g_py.Py_DecRef(config);
                    return job.Fail(422, "unknown_config_key",
                                    "字段 " + item.key + " 不在当前配置中，本桥只允许修改已存在的字段");
                }
            }
            PyObject* backup = g_py.PyDict_Copy(configData);
            PyObject* updateMethod = host.Attr(config, "update");
            if (!backup || !updateMethod) {
                if (backup) g_py.Py_DecRef(backup);
                g_py.Py_DecRef(configData);
                g_py.Py_DecRef(config);
                g_py.PyErr_Clear();
                return job.Fail(503, "host_incompatible", "宿主配置对象缺少 update 能力");
            }
            auto rollback = [&]() {
                g_py.PyObject_SetAttrString(config, "data", backup);  // setattr 持有新引用
                PyObject* saveMethod = host.Attr(config, "save");
                if (saveMethod) {
                    PyObject* saved = g_py.PyObject_CallObject(saveMethod, nullptr);
                    if (saved) g_py.Py_DecRef(saved);
                    g_py.PyErr_Clear();
                    g_py.Py_DecRef(saveMethod);
                }
            };
            for (const auto& item : items) {
                PyObject* keyObj = g_py.PyUnicode_FromString(item.key.c_str());
                PyObject* valueObj = host.VarToPython(item.value);
                PyObject* args = g_py.PyTuple_New(2);
                if (!keyObj || !valueObj || !args) {
                    if (keyObj) g_py.Py_DecRef(keyObj);
                    if (valueObj) g_py.Py_DecRef(valueObj);
                    if (args) g_py.Py_DecRef(args);
                    g_py.PyErr_Clear();
                    rollback();
                    g_py.Py_DecRef(backup);
                    g_py.Py_DecRef(updateMethod);
                    g_py.Py_DecRef(configData);
                    g_py.Py_DecRef(config);
                    return job.Fail(500, "config_write_failed", "构造写入参数失败：" + item.key);
                }
                g_py.PyTuple_SetItem(args, 0, keyObj);      // steals
                g_py.PyTuple_SetItem(args, 1, valueObj);    // steals
                PyObject* applied = g_py.PyObject_CallObject(updateMethod, args);
                g_py.Py_DecRef(args);
                if (!applied) {
                    g_py.PyErr_Clear();
                    rollback();
                    g_py.Py_DecRef(backup);
                    g_py.Py_DecRef(updateMethod);
                    g_py.Py_DecRef(configData);
                    g_py.Py_DecRef(config);
                    return job.Fail(500, "config_write_failed", "宿主 update 调用失败：" + item.key);
                }
                g_py.Py_DecRef(applied);
                // 回读核验：必须与写入值相等。
                PyObject* readKey = g_py.PyUnicode_FromString(item.key.c_str());
                PyObject* readback = readKey ? g_py.PyDict_GetItemWithError(configData, readKey) : nullptr;
                if (readKey) g_py.Py_DecRef(readKey);
                if (!readback) {
                    g_py.PyErr_Clear();
                    rollback();
                    g_py.Py_DecRef(backup);
                    g_py.Py_DecRef(updateMethod);
                    g_py.Py_DecRef(configData);
                    g_py.Py_DecRef(config);
                    return job.Fail(500, "config_verify_failed",
                                    "字段 " + item.key + " 写入后回读不到，已恢复原配置");
                }
                PyObject* expectedValue = host.VarToPython(item.value);
                int equal = expectedValue ? g_py.PyObject_RichCompareBool(readback, expectedValue, kPyEq) : -1;
                if (expectedValue) g_py.Py_DecRef(expectedValue);
                if (equal != 1) {
                    g_py.PyErr_Clear();
                    rollback();
                    g_py.Py_DecRef(backup);
                    g_py.Py_DecRef(updateMethod);
                    g_py.Py_DecRef(configData);
                    g_py.Py_DecRef(config);
                    return job.Fail(500, "config_verify_failed",
                                    "字段 " + item.key + " 写入后回读不一致，已恢复原配置");
                }
            }
            g_py.Py_DecRef(backup);
            g_py.Py_DecRef(updateMethod);
            PyObject* finalData = host.Attr(config, "data");
            if (finalData && TypeIs(finalData, g_py.pyDictType)) {
                JVar finalVar;
                if (host.VarFromPython(job, finalData, 0, finalVar)) {
                    data->newConfigHash = CanonicalHash(finalVar);
                }
                g_py.Py_DecRef(finalData);
            }
            g_py.Py_DecRef(configData);
            g_py.Py_DecRef(config);
            return true;
        },
        kDispatchWaitMs);
    if (result.outcome != DispatchOutcome::Executed || !result.ok) {
        ApplyDispatchFailure(result, response);
        return;
    }
    JVar updated{JVar::T::Arr};
    for (const auto& item : items) {
        updated.arr.emplace_back(JVar{JVar::T::Str, false, 0, 0, item.key});
    }
    JVar root{JVar::T::Obj};
    root.obj.emplace_back("appId", JVar{JVar::T::Str, false, 0, 0, appId});
    root.obj.emplace_back("instanceIdx", JVar{JVar::T::Int, false, instanceIdx});
    root.obj.emplace_back("groupId", JVar{JVar::T::Str, false, 0, 0, groupId});
    root.obj.emplace_back("updated", std::move(updated));
    root.obj.emplace_back("configHash", JVar{JVar::T::Str, false, 0, 0, data->newConfigHash});
    root.obj.emplace_back("verified", JVar{JVar::T::Bool, true});
    response.json = SerializeJVar(root);
}

void HandleTool(const std::string& path, const std::string& body, ToolResponse& response) {
    JVar parsed;
    if (!ParseBodyObject(body, parsed, response)) return;
    if (path == "/status") ToolStatus(parsed, response);
    else if (path == "/catalog/search") ToolCatalogSearch(parsed, response);
    else if (path == "/catalog/describe") ToolCatalogDescribe(parsed, response);
    else if (path == "/api/read") ToolApiRead(parsed, response);
    else if (path == "/api/invoke") ToolApiInvoke(parsed, response);
    else if (path == "/config/read") ToolConfigRead(parsed, response);
    else if (path == "/config/update") ToolConfigUpdate(parsed, response);
    else if (path == "/bridge/stop") {
        // 管理路由：只停桥的 HTTP 服务与引用，绝不结束宿主进程。
        response.json = "{\"stopped\":true}";
    } else {
        response.status = 404;
        response.json = ErrorJson("route_not_found", "未知路由：" + path);
    }
}

// ----------------------------------------------------------------------
// WinSock 服务：仅 127.0.0.1，线程与读写都带超时
// ----------------------------------------------------------------------

// 循环发送直到全部发出或连接失败；不把 partial send 当成功。
bool SendAll(SOCKET client, const std::string& data) {
    size_t sent = 0;
    while (sent < data.size()) {
        int wrote = ::send(client, data.data() + sent, static_cast<int>(data.size() - sent), 0);
        if (wrote <= 0) return false;
        sent += static_cast<size_t>(wrote);
    }
    return true;
}

void ServeConnection(SOCKET client, uint64_t generation) {
    g_requestGeneration = generation;
    try {
        winrt::init_apartment(winrt::apartment_type::multi_threaded);
    } catch (...) {
    }
    DWORD timeout = 5000;
    ::setsockopt(client, SOL_SOCKET, SO_RCVTIMEO, (const char*)&timeout, sizeof(timeout));
    ::setsockopt(client, SOL_SOCKET, SO_SNDTIMEO, (const char*)&timeout, sizeof(timeout));
    std::string raw;
    raw.reserve(2048);
    HttpRequest request;
    bool complete = false;
    char buffer[4096];
    while (raw.size() <= kMaxHeaderBytes + kMaxBodyBytes) {
        int got = ::recv(client, buffer, sizeof(buffer), 0);
        if (got <= 0) break;
        raw.append(buffer, static_cast<size_t>(got));
        if (ParseHttpRequest(raw, request)) {
            complete = true;
            break;
        }
        if (request.tooLarge || request.malformed) break;
    }
    int status = 200;
    std::string payload;
    if (!g_state.running.load() || generation != g_sessionGeneration.load()) {
        // 桥正在停止：新请求一律拒绝，不吞请求也不假装修好了。
        status = 503;
        payload = ErrorJson("bridge_stopping", "桥正在停止，不再接受新请求");
    } else if (request.tooLarge) {
        status = 413;
        payload = ErrorJson("body_too_large", "请求超过大小上限");
    } else if (request.malformed || !complete) {
        status = 400;
        payload = ErrorJson("bad_request", "请求不完整或格式非法");
    } else if (request.method != "POST") {
        status = 405;
        payload = ErrorJson("method_not_allowed", "本桥所有工具路由都要求 POST");
    } else if (const char* guard = g_state.VerifyRequest(request)) {
        status = std::strcmp(guard, "unauthorized") == 0 ? 401 : 403;
        payload = ErrorJson(guard, std::strcmp(guard, "unauthorized") == 0
                                      ? "缺少或错误的访问令牌"
                                      : std::strcmp(guard, "bad_host") == 0
                                            ? "Host 头不符合本桥监听地址"
                                            : "拒绝携带浏览器来源头的请求");
    } else {
        ToolResponse response;
        try {
            HandleTool(request.path, request.body, response);
        } catch (...) {
            // 不吞异常假装成功；连接层兜底转为 500。
            response.status = 500;
            response.json = ErrorJson("internal_error", "工具处理发生未预期异常");
        }
        status = response.status;
        payload = response.json;
    }
    std::string out = BuildHttpResponse(status, payload);
    SendAll(client, out);
    ::closesocket(client);
    g_state.jobs.ForgetExpired();
    const bool stopRequested = request.path == "/bridge/stop" && status == 200;
    g_state.activeConnections.fetch_sub(1);
    if (stopRequested) {
        StopBridgeCore();  // 响应已发出后再停服务；不会结束宿主进程
    }
}

void AcceptLoop(SOCKET listener, uint64_t generation) {
    while (g_state.running.load() && generation == g_sessionGeneration.load()) {
        sockaddr_in peer{};
        int peerSize = sizeof(peer);
        SOCKET client = ::accept(listener, (sockaddr*)&peer, &peerSize);
        if (client == INVALID_SOCKET) {
            if (!g_state.running.load() || generation != g_sessionGeneration.load()) break;
            continue;
        }
        if (peer.sin_family != AF_INET || peer.sin_addr.S_un.S_addr != htonl(INADDR_LOOPBACK)) {
            ::closesocket(client);
            continue;
        }
        if (g_state.activeConnections.load() >= kMaxConnections) {
            // 并发上限：直接拒绝，防止线程堆积。
            std::string busy = BuildHttpResponse(503, ErrorJson("busy", "并发请求过多，请稍后重试"));
            SendAll(client, busy);
            ::closesocket(client);
            continue;
        }
        g_state.activeConnections.fetch_add(1);
        try {
            std::thread(ServeConnection, client, generation).detach();
        } catch (...) {
            g_state.activeConnections.fetch_sub(1);
            ::closesocket(client);
        }
    }
}

// 桥生命周期：Idle → Starting → Running → Stopping → Idle。
// Start 并发或重复调用被拒绝，Stop 可重入。
enum class BridgePhase { Idle, Starting, Running, Stopping };
std::atomic<BridgePhase> g_bridgePhase{BridgePhase::Idle};

uint32_t StartBridgeCore(const ZzzBridgeConfig& config) {
    BridgePhase expected = BridgePhase::Idle;
    if (!g_bridgePhase.compare_exchange_strong(expected, BridgePhase::Starting)) {
        return 130;  // 已在启动、运行或停止中：拒绝双启动
    }
    auto giveUp = []() { g_bridgePhase.store(BridgePhase::Idle); };
    if (config.abiVersion != ZZZ_BRIDGE_ABI_VERSION) { giveUp(); return 101; }
    if (config.port == 0) { giveUp(); return 102; }
    size_t tokenLength = 0;
    while (tokenLength < ZZZ_BRIDGE_TOKEN_CHARS - 1 && config.token[tokenLength] != L'\0') ++tokenLength;
    if (tokenLength < 16) { giveUp(); return 103; }  // 令牌过短直接拒绝
    std::string token;
    token.reserve(tokenLength);
    for (size_t i = 0; i < tokenLength; ++i) {
        wchar_t wide = config.token[i];
        if (wide < 0x21 || wide > 0x7E) { giveUp(); return 104; }  // 令牌只接受可见 ASCII
        token.push_back(static_cast<char>(wide));
    }
    // 模块 pin：注入后本 DLL 保持映射到宿主退出，迟到的 pending 回调
    // 绝不会跳进已卸载的代码。
    HMODULE self = nullptr;
    if (!::GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_PIN | GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS,
                              reinterpret_cast<LPCWSTR>(&StartBridgeCore), &self)) {
        // pin 失败不致命：DLL 本就无人显式卸载。
        (void)self;
    }
    HMODULE pythonDll = FindLoadedPython();
    if (!pythonDll) { giveUp(); return 110; }  // 目标进程没有已加载的 CPython
    if (!ResolvePythonApi(pythonDll)) { giveUp(); return 111; }
    if (!g_py.Py_IsInitialized || !g_py.Py_IsInitialized() || (g_py.Py_IsFinalizing && g_py.Py_IsFinalizing())) { giveUp(); return 112; }
    // 验证并启用 Qt 队列 poster：宿主必须已加载 PySide6 且存在现存
    // QApplication 实例；失败即拒绝启动（fail closed），不降级 pending 路线。
    {
        PyGilState posterGil = g_py.PyGILState_Ensure();
        bool posterReady = EnsureQtPoster();
        g_py.PyGILState_Release(posterGil);
        if (!posterReady) { giveUp(); return 105; }
        if (!g_postCall) g_postCall = &ZzzPostViaQt;
    }
    WSADATA wsa{};
    if (::WSAStartup(MAKEWORD(2, 2), &wsa) != 0) { giveUp(); return 120; }
    SOCKET listenSocket = ::socket(AF_INET, SOCK_STREAM, IPPROTO_TCP);
    if (listenSocket == INVALID_SOCKET) { giveUp(); ::WSACleanup(); return 121; }
    sockaddr_in address{};
    address.sin_family = AF_INET;
    address.sin_addr.s_addr = htonl(INADDR_LOOPBACK);  // 仅回环
    address.sin_port = htons(static_cast<u_short>(config.port));
    BOOL exclusive = TRUE;
    ::setsockopt(listenSocket, SOL_SOCKET, SO_EXCLUSIVEADDRUSE, (const char*)&exclusive, sizeof(exclusive));
    if (::bind(listenSocket, (sockaddr*)&address, sizeof(address)) == SOCKET_ERROR ||
        ::listen(listenSocket, 8) == SOCKET_ERROR) {
        ::closesocket(listenSocket);
        giveUp();
        ::WSACleanup();
        return 122;
    }
    try {
        winrt::init_apartment(winrt::apartment_type::multi_threaded);
    } catch (...) {
    }
    uint64_t nonce = 0;
    if (::BCryptGenRandom(nullptr, reinterpret_cast<PUCHAR>(&nonce), sizeof(nonce), BCRYPT_USE_SYSTEM_PREFERRED_RNG) < 0) {
        ::closesocket(listenSocket);
        ::WSACleanup();
        giveUp();
        return 113;
    }
    g_sessionNonce.store(nonce);
    const uint64_t generation = g_sessionGeneration.fetch_add(1) + 1;
    {
        std::lock_guard<std::mutex> lock(g_state.authMutex);
        g_state.port = static_cast<uint16_t>(config.port);
        g_state.token = std::move(token);
    }
    g_state.listenSocket = listenSocket;
    g_state.running.store(true);
    g_bridgePhase.store(BridgePhase::Running);
    std::thread(AcceptLoop, listenSocket, generation).detach();
    return 0;
}

uint32_t StopBridgeCore() {
    BridgePhase expected = BridgePhase::Running;
    if (!g_bridgePhase.compare_exchange_strong(expected, BridgePhase::Stopping)) {
        return 0;  // 未运行或已在停止：可重入
    }
    g_state.running.store(false);       // 新请求随即收到 bridge_stopping
    g_sessionGeneration.fetch_add(1);
    g_state.jobs.CancelQueued();
    ::closesocket(g_state.listenSocket);
    g_state.listenSocket = INVALID_SOCKET;
    // 等待存量连接线程结束（有限窗口）；超时不再等待，线程按自身超时退出。
    for (int waited = 0; waited < 100 && g_state.activeConnections.load() > 0; ++waited) {
        ::Sleep(20);
    }
    // 执行中的主线程闭包不强行打断：状态由调用方语义（unknown）覆盖。
    // 取消的 queued ticket 留给 CPython 迟到回调自行回收，绝不在此删除。
    // 已执行的回调仍可持有上下文；与已 pin 的 DLL 一同保留到进程退出。
    g_state.jobs.ForgetExpired();
    ::WSACleanup();
    g_bridgePhase.store(BridgePhase::Idle);
    return 0;
}
