// maaend_bridge_core.h - MaaEnd 独立桥核心逻辑（与传输实现解耦）
//
// 桥只做一件事：把 MaaEnd（MXU 内核）本机 HTTP API 的一个固定白名单子集，
// 以受限、带证据校验的工具协议暴露给宿主。它是对官方 HTTP 契约的原生发现与
// 包装（与 Tauri 共享同一份真实 MaaState），不是进程注入，也不加载目标 DLL。
//
// 本文件不包含 WinSock/WinHTTP；所有上游访问经 ITransport，便于用假传输测试。

#pragma once

#ifndef NOMINMAX
#define NOMINMAX
#endif
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif

#include <windows.h>
#include <winrt/base.h>
#include <winrt/Windows.Data.Json.h>
#include <winrt/Windows.Foundation.Collections.h>

#include <algorithm>
#include <bcrypt.h>
#include <cctype>
#include <chrono>
#include <cwctype>
#include <map>
#include <optional>
#include <regex>
#include <set>
#include <sstream>
#include <string>
#include <string_view>
#include <thread>
#include <vector>
#include "hotkey_codes.hpp"

namespace maaend {

namespace json = winrt::Windows::Data::Json;

// ---------------------------------------------------------------------------
// 常量与限额
// ---------------------------------------------------------------------------

constexpr std::size_t kMaxInboundBody = 256 * 1024;       // 请求体上限
constexpr std::size_t kMaxHeaderBytes = 16 * 1024;        // 请求头上限
constexpr std::size_t kMaxUpstreamBytes = 8 * 1024 * 1024; // 上游响应上限
constexpr std::size_t kMaxResponseBytes = 512 * 1024;     // 桥响应上限
constexpr int kMaxConcurrentConnections = 8;              // 并发连接上限
constexpr const char* kClientId = "sleepy-doll-maaend";   // POST /api/config 的 client_id
constexpr const wchar_t* kExpectedProjectName = L"MaaEnd"; // interface.name 必须是它

// ---------------------------------------------------------------------------
// 命令行选项
// ---------------------------------------------------------------------------

struct BridgeOptions {
  int port = 23121;                    // 桥监听端口（仅 127.0.0.1）
  int upstream_port = 12701;           // MaaEnd 内置 HTTP 端口
  std::string token_env = "MAAEND_BRIDGE_TOKEN"; // token 所在环境变量名
  int stop_poll_interval_ms = 300;     // 停止后回读状态的间隔
  int stop_poll_attempts = 10;         // 停止后回读状态的次数上限
  int upstream_receive_timeout_ms = 10000;
};

inline std::string UsageText() {
  return "maaend-bridge - MaaEnd 本机 HTTP 契约反射桥\n"
         "用法: maaend-bridge.exe [--port 23121] [--upstream-port 12701] "
         "[--token-env MAAEND_BRIDGE_TOKEN]\n"
         "只监听 127.0.0.1；只访问 127.0.0.1 上已运行的 MaaEnd；不启动、不注入任何目标程序。\n";
}

inline bool ValidPort(int p) { return p > 0 && p <= 65535; }

inline bool ValidEnvName(const std::string& name) {
  if (name.empty() || name.size() > 64) return false;
  if (!(std::isalpha((unsigned char)name[0]) || name[0] == '_')) return false;
  for (char c : name) {
    if (!(std::isalnum((unsigned char)c) || c == '_')) return false;
  }
  return true;
}

inline std::optional<BridgeOptions> ParseOptions(int argc, char** argv) {
  BridgeOptions opt;
  for (int i = 1; i < argc; ++i) {
    std::string_view key = argv[i];
    auto need_value = [&](std::string_view) -> std::string {
      if (i + 1 >= argc) return {};
      return argv[++i];
    };
    if (key == "--port") {
      auto v = need_value(key);
      if (v.empty()) return std::nullopt;
      opt.port = std::atoi(v.c_str());
    } else if (key == "--upstream-port") {
      auto v = need_value(key);
      if (v.empty()) return std::nullopt;
      opt.upstream_port = std::atoi(v.c_str());
    } else if (key == "--token-env") {
      auto v = need_value(key);
      if (v.empty()) return std::nullopt;
      opt.token_env = v;
    } else if (key == "--stop-poll-interval-ms") {
      auto v = need_value(key);
      if (v.empty()) return std::nullopt;
      opt.stop_poll_interval_ms = std::atoi(v.c_str());
    } else if (key == "--stop-poll-attempts") {
      auto v = need_value(key);
      if (v.empty()) return std::nullopt;
      opt.stop_poll_attempts = std::atoi(v.c_str());
    } else if (key == "--upstream-receive-timeout-ms") {
      auto v = need_value(key);
      if (v.empty()) return std::nullopt;
      opt.upstream_receive_timeout_ms = std::atoi(v.c_str());
    } else {
      return std::nullopt;
    }
  }
  if (!ValidPort(opt.port) || !ValidPort(opt.upstream_port) ||
      !ValidEnvName(opt.token_env) || opt.stop_poll_interval_ms < 0 ||
      opt.stop_poll_attempts < 1 || opt.upstream_receive_timeout_ms < 100) {
    return std::nullopt;
  }
  return opt;
}

// ---------------------------------------------------------------------------
// 传输抽象
// ---------------------------------------------------------------------------

struct HttpExchange {
  std::string method = "GET";  // 仅 GET / POST
  std::string path;            // 固定白名单路径，如 "/api/interface"
  std::string body;            // UTF-8 JSON（POST 时）
};

struct HttpReply {
  int status = 0;              // HTTP 状态码；0 = 未得到 HTTP 响应
  std::string body;            // UTF-8 响应体
  bool network_error = false;  // 连不上 / 超时 / 中断
  std::string network_reason;  // "unreachable" | "timeout" | "too_large"
};

struct ITransport {
  virtual ~ITransport() = default;
  virtual HttpReply Request(const HttpExchange& exchange) = 0;
};

// 上游目标只允许 http + 字面回环地址；测试直接调用该纯函数覆盖“地址拒绝”。
inline bool AllowedUpstreamTarget(std::string_view scheme, std::string_view host) {
  if (scheme != "http") return false;
  return host == "127.0.0.1";
}

// ---------------------------------------------------------------------------
// JSON / 字符串辅助
// ---------------------------------------------------------------------------

inline std::string ToUtf8(winrt::hstring const& h) { return winrt::to_string(h); }
inline std::string ToUtf8(const std::wstring& s) { return winrt::to_string(winrt::hstring(s)); }
inline winrt::hstring ToH(std::string_view utf8) { return winrt::to_hstring(utf8); }
inline std::wstring ToW(std::string_view utf8) { return std::wstring(ToH(utf8)); }

inline std::string Sha256Hex(std::string_view data) {
  std::string out;
  BCRYPT_ALG_HANDLE alg = nullptr;
  NTSTATUS st = BCryptOpenAlgorithmProvider(&alg, BCRYPT_SHA256_ALGORITHM, nullptr, 0);
  if (!BCRYPT_SUCCESS(st)) return out;
  BCRYPT_HASH_HANDLE hash = nullptr;
  st = BCryptCreateHash(alg, &hash, nullptr, 0, nullptr, 0, 0);
  if (BCRYPT_SUCCESS(st)) {
    st = BCryptHashData(hash, const_cast<PUCHAR>(reinterpret_cast<const UCHAR*>(data.data())),
                        static_cast<ULONG>(data.size()), 0);
    if (BCRYPT_SUCCESS(st)) {
      BYTE buf[32];
      st = BCryptFinishHash(hash, buf, sizeof(buf), 0);
      if (BCRYPT_SUCCESS(st)) {
        static const char* kHex = "0123456789abcdef";
        for (BYTE b : buf) {
          out.push_back(kHex[b >> 4]);
          out.push_back(kHex[b & 0xF]);
        }
      }
    }
    BCryptDestroyHash(hash);
  }
  BCryptCloseAlgorithmProvider(alg, 0);
  return out;
}

inline std::wstring Lower(std::wstring s) {
  std::transform(s.begin(), s.end(), s.begin(), [](wchar_t c) { return std::towlower(c); });
  return s;
}

inline std::string LowerAscii(std::string s) {
  std::transform(s.begin(), s.end(), s.begin(),
                 [](char c) { return (char)std::tolower((unsigned char)c); });
  return s;
}

// 常数时间比较，避免 token 早退泄漏。
inline bool ConstantTimeEqual(std::string_view a, std::string_view b) {
  unsigned char diff = (unsigned char)(a.size() ^ b.size());
  size_t n = std::max(a.size(), b.size());
  for (size_t i = 0; i < n; ++i) {
    unsigned char ca = i < a.size() ? (unsigned char)a[i] : 0;
    unsigned char cb = i < b.size() ? (unsigned char)b[i] : 0;
    diff |= (unsigned char)(ca ^ cb);
  }
  return diff == 0;
}

inline bool TryGetObject(json::JsonObject const& o, std::wstring_view key, json::JsonObject& out) {
  if (!o.HasKey(key)) return false;
  auto v = o.Lookup(key);
  if (v.ValueType() != json::JsonValueType::Object) return false;
  out = v.GetObject();
  return true;
}

inline bool TryGetArray(json::JsonObject const& o, std::wstring_view key, json::JsonArray& out) {
  if (!o.HasKey(key)) return false;
  auto v = o.Lookup(key);
  if (v.ValueType() != json::JsonValueType::Array) return false;
  out = v.GetArray();
  return true;
}

inline bool TryGetString(json::JsonObject const& o, std::wstring_view key, std::wstring& out) {
  if (!o.HasKey(key)) return false;
  auto v = o.Lookup(key);
  if (v.ValueType() != json::JsonValueType::String) return false;
  out = std::wstring(v.GetString());
  return true;
}

inline bool TryGetBool(json::JsonObject const& o, std::wstring_view key, bool& out) {
  if (!o.HasKey(key)) return false;
  auto v = o.Lookup(key);
  if (v.ValueType() != json::JsonValueType::Boolean) return false;
  out = v.GetBoolean();
  return true;
}

inline std::vector<std::wstring> StringArray(json::JsonObject const& o, std::wstring_view key) {
  std::vector<std::wstring> out;
  json::JsonArray arr;
  if (TryGetArray(o, key, arr)) {
    for (auto&& v : arr) {
      if (v.ValueType() == json::JsonValueType::String) out.push_back(std::wstring(v.GetString()));
    }
  }
  return out;
}

inline void InsertStr(json::JsonObject const& o, std::wstring_view key, std::wstring_view value) {
  o.Insert(key, json::JsonValue::CreateStringValue(value));
}
inline void InsertStrUtf8(json::JsonObject const& o, std::wstring_view key, std::string_view utf8) {
  o.Insert(key, json::JsonValue::CreateStringValue(ToH(utf8)));
}
inline void InsertBool(json::JsonObject const& o, std::wstring_view key, bool value) {
  o.Insert(key, json::JsonValue::CreateBooleanValue(value));
}
inline void InsertInt(json::JsonObject const& o, std::wstring_view key, long long value) {
  o.Insert(key, json::JsonValue::CreateNumberValue((double)value));
}

// 统一错误负载：{ok:false, error:{code, message, ...}}
inline json::JsonObject MakeError(std::string_view code, std::string_view message) {
  json::JsonObject err;
  InsertStrUtf8(err, L"code", code);
  InsertStrUtf8(err, L"message", message);
  json::JsonObject root;
  InsertBool(root, L"ok", false);
  root.Insert(L"error", err);
  return root;
}

inline json::JsonObject MakeOk() {
  json::JsonObject root;
  InsertBool(root, L"ok", true);
  return root;
}

inline std::string Stringify(json::JsonObject const& o) { return ToUtf8(o.Stringify()); }

// 结构化深度相等（键序无关），用于改动范围与回读核实。
inline bool DeepEqual(json::IJsonValue const& a, json::IJsonValue const& b) {
  using json::JsonValueType;
  if (a.ValueType() != b.ValueType()) return false;
  switch (a.ValueType()) {
    case JsonValueType::Null:
      return true;
    case JsonValueType::Boolean:
      return a.GetBoolean() == b.GetBoolean();
    case JsonValueType::Number:
      return a.GetNumber() == b.GetNumber();
    case JsonValueType::String:
      return std::wstring(a.GetString()) == std::wstring(b.GetString());
    case JsonValueType::Array: {
      auto aa = a.GetArray();
      auto ba = b.GetArray();
      if (aa.Size() != ba.Size()) return false;
      for (uint32_t i = 0; i < aa.Size(); ++i) {
        if (!DeepEqual(aa.GetAt(i), ba.GetAt(i))) return false;
      }
      return true;
    }
    case JsonValueType::Object: {
      auto ao = a.GetObject();
      auto bo = b.GetObject();
      if (ao.Size() != bo.Size()) return false;
      for (auto&& kv : ao) {
        if (!bo.HasKey(std::wstring(kv.Key()))) return false;
        if (!DeepEqual(kv.Value(), bo.Lookup(std::wstring(kv.Key())))) return false;
      }
      return true;
    }
    default:
      return false;
  }
}

inline bool ParseJsonObject(std::string_view utf8, json::JsonObject& out) {
  try {
    out = json::JsonObject::Parse(ToH(utf8));
    return true;
  } catch (const winrt::hresult_error&) {
    return false;
  } catch (...) {
    return false;
  }
}

// ---------------------------------------------------------------------------
// 入站请求校验（纯函数，服务层与测试共用）
// ---------------------------------------------------------------------------

struct RouteInfo {
  const char* path;
  const char* method;  // "GET" | "POST"
};

inline const std::vector<RouteInfo>& Routes() {
  static const std::vector<RouteInfo> routes = {
      {"/health", "GET"},         {"/status", "POST"},   {"/catalog/search", "POST"},
      {"/catalog/describe", "POST"}, {"/read", "POST"},  {"/invoke", "POST"},
      {"/task/stop", "POST"},     {"/config/read", "POST"},
      {"/config/update", "POST"},
  };
  return routes;
}

inline const RouteInfo* FindRoute(std::string_view path) {
  for (auto&& r : Routes()) {
    if (path == r.path) return &r;
  }
  return nullptr;
}

struct InboundDecision {
  bool allowed = false;
  int http_status = 0;
  std::string code;
  std::string message;
};

// Host 必须是本机字面地址；带 Origin 的一律拒绝；token 常数时间比对。
inline bool HostIsLoopbackLiteral(std::string_view host) {
  std::string h = LowerAscii(std::string(host));
  const std::string kWithPortPrefix1 = "127.0.0.1:";
  const std::string kWithPortPrefix2 = "localhost:";
  if (h == "127.0.0.1" || h == "localhost") return true;
  if (h.rfind(kWithPortPrefix1, 0) == 0) {
    std::string port = h.substr(kWithPortPrefix1.size());
    return !port.empty() && port.find_first_not_of("0123456789") == std::string::npos;
  }
  if (h.rfind(kWithPortPrefix2, 0) == 0) {
    std::string port = h.substr(kWithPortPrefix2.size());
    return !port.empty() && port.find_first_not_of("0123456789") == std::string::npos;
  }
  return false;
}

inline InboundDecision CheckInbound(std::string_view method, std::string_view raw_path,
                                    std::string_view host_header, bool has_origin_header,
                                    std::string_view token_header, bool token_configured,
                                    std::string_view expected_token, std::size_t content_length) {
  InboundDecision d;
  auto deny = [&](int status, std::string code, std::string message) {
    d.allowed = false;
    d.http_status = status;
    d.code = std::move(code);
    d.message = std::move(message);
    return d;
  };
  if (raw_path.find('?') != std::string_view::npos) {
    return deny(404, "unknown_route", "路径不支持查询串");
  }
  const RouteInfo* route = FindRoute(raw_path);
  if (!route) return deny(404, "unknown_route", "未知路径");
  if (method != route->method) {
    return deny(405, "method_not_allowed", "该路径不支持此 HTTP 方法");
  }
  if (host_header.empty()) return deny(400, "missing_host", "缺少 Host 头");
  if (!HostIsLoopbackLiteral(host_header)) {
    return deny(403, "bad_host", "Host 必须是 127.0.0.1 或 localhost");
  }
  if (has_origin_header) {
    return deny(403, "origin_rejected", "桥不接受浏览器来源请求");
  }
  if (!token_configured) {
    return deny(403, "token_not_configured",
                "未配置访问令牌环境变量，桥已拒绝所有请求");
  }
  if (token_header.empty()) return deny(401, "missing_token", "缺少 X-Sleepy-Token 头");
  if (!ConstantTimeEqual(token_header, expected_token)) {
    return deny(403, "bad_token", "访问令牌不匹配");
  }
  if (content_length > kMaxInboundBody) {
    return deny(413, "request_too_large", "请求体超出上限");
  }
  d.allowed = true;
  d.http_status = 200;
  return d;
}

inline bool ValidInstanceId(std::string_view id) {
  if (id.empty() || id.size() > 128) return false;
  for (char c : id) {
    if (!(std::isalnum((unsigned char)c) || c == '_' || c == '-')) return false;
  }
  return true;
}

inline bool ValidCatalogHash(std::string_view h) {
  if (h.size() != 64) return false;
  for (char c : h) {
    if (!std::isxdigit((unsigned char)c)) return false;
  }
  return true;
}

// ---------------------------------------------------------------------------
// 桥上下文与上游访问
// ---------------------------------------------------------------------------

struct BridgeContext {
  ITransport* transport = nullptr;
  const BridgeOptions* options = nullptr;
};

struct UpstreamOutcome {
  // 以下三种互斥：network_error / redirect / 拿到 HTTP 状态。
  bool network_error = false;
  std::string network_reason;
  bool redirect = false;
  int status = 0;
  std::string body;
  bool http_ok() const { return !network_error && !redirect && status >= 200 && status < 300; }
};

inline UpstreamOutcome Upstream(BridgeContext& ctx, const HttpExchange& ex) {
  UpstreamOutcome out;
  HttpReply reply = ctx.transport->Request(ex);
  if (reply.network_error) {
    out.network_error = true;
    out.network_reason = reply.network_reason;
    return out;
  }
  if (reply.status >= 300 && reply.status < 400) {
    out.redirect = true;
    out.status = reply.status;
    return out;
  }
  out.status = reply.status;
  out.body = std::move(reply.body);
  return out;
}

inline std::string Excerpt(std::string_view s, std::size_t n = 300) {
  if (s.size() <= n) return std::string(s);
  return std::string(s.substr(0, n)) + "...";
}

// 上游通用失败 → 桥错误负载（正文截断，永不包含 token）。
inline json::JsonObject UpstreamFailure(const UpstreamOutcome& r, const char* when) {
  if (r.network_error) {
    std::string msg = std::string("访问 MaaEnd 后端失败（") + when +
                      "）：" + (r.network_reason.empty() ? "网络错误" : r.network_reason) +
                      "。请确认 MaaEnd 已由你手动启动且内置 Web 服务已开启。";
    return MakeError("upstream_unavailable", msg);
  }
  if (r.redirect) {
    return MakeError("upstream_redirect",
                     "MaaEnd 后端返回了重定向，桥不跟随重定向。请检查端口是否指向 MaaEnd 本身。");
  }
  return MakeError("upstream_error",
                   std::string("MaaEnd 后端返回错误（") + when +
                       "）：HTTP " + std::to_string(r.status) + " " + Excerpt(r.body));
}

// ---------------------------------------------------------------------------
// 目录（实时反射 GET /api/interface）
// ---------------------------------------------------------------------------

struct CatalogFetch {
  bool ok = false;
  json::JsonObject error;       // 失败时的错误负载
  std::string raw;              // 原始响应体
  std::string hash;             // raw 的 SHA-256（目录证据）
  json::JsonObject root;        // 整个响应对象
  json::JsonObject iface;       // interface 对象
  std::wstring project_name;    // interface.name
  std::wstring project_version; // interface.version
};

// 拒绝非 MaaEnd 后端：interface.name/version 必须符合预期。
inline CatalogFetch FetchCatalog(BridgeContext& ctx) {
  CatalogFetch c;
  HttpExchange ex;
  ex.method = "GET";
  ex.path = "/api/interface";
  UpstreamOutcome r = Upstream(ctx, ex);
  if (!r.http_ok()) {
    c.error = UpstreamFailure(r, "GET /api/interface");
    return c;
  }
  c.raw = r.body;
  c.hash = Sha256Hex(c.raw);
  json::JsonObject root;
  if (!ParseJsonObject(c.raw, root)) {
    c.error = MakeError("bad_interface_response", "/api/interface 返回的不是 JSON 对象");
    return c;
  }
  c.root = root;
  json::JsonObject iface;
  if (!TryGetObject(root, L"interface", iface) || !TryGetString(iface, L"name", c.project_name)) {
    c.error = MakeError("bad_interface_response",
                        "/api/interface 缺少 interface.name，无法确认后端身份");
    return c;
  }
  TryGetString(iface, L"version", c.project_version);
  if (c.project_name != kExpectedProjectName || c.project_version.empty()) {
    c.error = MakeError(
        "wrong_backend",
        "上游不是预期的 MaaEnd 后端（interface.name/version 不符）。桥拒绝继续操作。");
    return c;
  }
  c.iface = iface;
  c.ok = true;
  return c;
}

struct TaskDecl {
  bool found = false;
  std::wstring name, entry, label, description;
  std::vector<std::wstring> groups, controllers, resources, option_keys;
  json::JsonObject raw;  // 原始任务声明
};

inline TaskDecl FindTask(const CatalogFetch& c, std::wstring_view name) {
  TaskDecl t;
  if (!c.ok) return t;
  json::JsonArray tasks;
  if (!TryGetArray(c.iface, L"task", tasks)) return t;
  for (auto&& v : tasks) {
    if (v.ValueType() != json::JsonValueType::Object) continue;
    auto obj = v.GetObject();
    std::wstring n;
    if (!TryGetString(obj, L"name", n) || n != name) continue;
    t.found = true;
    t.raw = obj;
    t.name = n;
    TryGetString(obj, L"entry", t.entry);
    TryGetString(obj, L"label", t.label);
    TryGetString(obj, L"description", t.description);
    t.groups = StringArray(obj, L"group");
    t.controllers = StringArray(obj, L"controller");
    t.resources = StringArray(obj, L"resource");
    t.option_keys = StringArray(obj, L"option");
    return t;
  }
  return t;
}

inline bool GetOptionDef(const CatalogFetch& c, std::wstring_view key, json::JsonObject& out) {
  if (!c.ok) return false;
  json::JsonObject opts;
  if (!TryGetObject(c.iface, L"option", opts)) return false;
  return TryGetObject(opts, key, out);
}

// label 解析："$key" 形式查 translations（zh_cn 优先，en_us 次之）。
inline std::wstring ResolveLabel(const CatalogFetch& c, const std::wstring& label) {
  if (label.empty() || label[0] != L'$') return label;
  std::wstring key = label.substr(1);
  if (!c.ok) return label;
  json::JsonObject translations;
  if (!TryGetObject(c.root, L"translations", translations)) return label;
  for (const wchar_t* lang : {L"zh_cn", L"en_us"}) {
    json::JsonObject table;
    if (!TryGetObject(translations, lang, table)) continue;
    if (table.HasKey(key)) {
      auto v = table.Lookup(key);
      if (v.ValueType() == json::JsonValueType::String) return std::wstring(v.GetString());
    }
  }
  return label;
}

// 该任务可写的选项键：global_option + 匹配资源的 option + 匹配控制器的 option + 任务自身。
inline std::set<std::wstring> ReachableOptionKeys(const CatalogFetch& c, const TaskDecl& task,
                                                  const std::wstring& controller_name,
                                                  const std::wstring& resource_name) {
  std::set<std::wstring> keys;
  if (!c.ok || !task.found) return keys;
  for (auto&& k : StringArray(c.iface, L"global_option")) keys.insert(k);
  json::JsonArray resources;
  if (TryGetArray(c.iface, L"resource", resources)) {
    for (auto&& v : resources) {
      if (v.ValueType() != json::JsonValueType::Object) continue;
      auto obj = v.GetObject();
      std::wstring n;
      if (TryGetString(obj, L"name", n) && n == resource_name) {
        for (auto&& k : StringArray(obj, L"option")) keys.insert(k);
      }
    }
  }
  json::JsonArray controllers;
  if (TryGetArray(c.iface, L"controller", controllers)) {
    for (auto&& v : controllers) {
      if (v.ValueType() != json::JsonValueType::Object) continue;
      auto obj = v.GetObject();
      std::wstring n;
      if (TryGetString(obj, L"name", n) && n == controller_name) {
        for (auto&& k : StringArray(obj, L"option")) keys.insert(k);
      }
    }
  }
  for (auto&& k : task.option_keys) keys.insert(k);
  return keys;
}

// ---------------------------------------------------------------------------
// 选项值校验（只允许已声明、无副作用语义的安全标量）
// ---------------------------------------------------------------------------

struct OptionCheck {
  bool ok = false;
  json::JsonObject value;   // 归一化后的 OptionValue
  std::string code, message;
};

inline OptionCheck CheckSelectValue(json::JsonObject const& def, json::JsonObject const& value,
                                    const std::wstring& controller_name,
                                    const std::wstring& resource_name) {
  OptionCheck out;
  std::wstring type;
  if (!TryGetString(value, L"type", type) || type != L"select") {
    out.code = "invalid_option_value";
    out.message = "select 选项的值形状必须是 {type:'select', caseName}";
    return out;
  }
  std::wstring case_name;
  if (!TryGetString(value, L"caseName", case_name) || case_name.empty() ||
      case_name.size() > 200) {
    out.code = "invalid_option_value";
    out.message = "caseName 必须是不超过 200 字符的字符串";
    return out;
  }
  json::JsonArray cases;
  if (!TryGetArray(def, L"cases", cases)) {
    out.code = "structure_drift";
    out.message = "选项声明缺少 cases，拒绝写入";
    return out;
  }
  bool found = false;
  json::JsonObject chosen;
  for (auto&& v : cases) {
    if (v.ValueType() != json::JsonValueType::Object) continue;
    auto c = v.GetObject();
    std::wstring n;
    if (TryGetString(c, L"name", n) && n == case_name) {
      found = true;
      chosen = c;
      break;
    }
  }
  if (!found) {
    out.code = "invalid_option_value";
    out.message = "caseName 未在选项声明中";
    return out;
  }
  auto case_controllers = StringArray(chosen, L"controller");
  if (!case_controllers.empty() &&
      std::find(case_controllers.begin(), case_controllers.end(), controller_name) ==
          case_controllers.end()) {
    out.code = "invalid_option_value";
    out.message = "该 case 不适用于当前控制器";
    return out;
  }
  auto case_resources = StringArray(chosen, L"resource");
  if (!case_resources.empty() &&
      std::find(case_resources.begin(), case_resources.end(), resource_name) ==
          case_resources.end()) {
    out.code = "invalid_option_value";
    out.message = "该 case 不适用于当前资源";
    return out;
  }
  json::JsonObject norm;
  InsertStr(norm, L"type", L"select");
  InsertStr(norm, L"caseName", case_name);
  out.value = norm;
  out.ok = true;
  return out;
}

inline OptionCheck CheckSwitchValue(json::JsonObject const& value) {
  OptionCheck out;
  std::wstring type;
  if (value.Size() != 2 || !TryGetString(value, L"type", type) || type != L"switch" ||
      !value.HasKey(L"value") ||
      value.Lookup(L"value").ValueType() != json::JsonValueType::Boolean) {
    out.code = "invalid_option_value";
    out.message = "switch 选项的值形状必须是 {type:'switch', value:boolean}";
    return out;
  }
  json::JsonObject norm;
  InsertStr(norm, L"type", L"switch");
  InsertBool(norm, L"value", value.Lookup(L"value").GetBoolean());
  out.value = norm;
  out.ok = true;
  return out;
}

inline OptionCheck CheckCheckboxValue(json::JsonObject const& def, json::JsonObject const& value) {
  OptionCheck out;
  std::wstring type;
  if (value.Size() != 2 || !TryGetString(value, L"type", type) || type != L"checkbox" ||
      !value.HasKey(L"caseNames") ||
      value.Lookup(L"caseNames").ValueType() != json::JsonValueType::Array) {
    out.code = "invalid_option_value";
    out.message = "checkbox 选项的值形状必须是 {type:'checkbox', caseNames:[]}";
    return out;
  }
  auto arr = value.Lookup(L"caseNames").GetArray();
  if (arr.Size() > 64) {
    out.code = "invalid_option_value";
    out.message = "caseNames 数量超出上限";
    return out;
  }
  std::vector<std::wstring> names;
  std::set<std::wstring> seen;
  for (auto&& v : arr) {
    if (v.ValueType() != json::JsonValueType::String) {
      out.code = "invalid_option_value";
      out.message = "caseNames 必须是字符串数组";
      return out;
    }
    auto n = std::wstring(v.GetString());
    if (!seen.insert(n).second) {
      out.code = "invalid_option_value";
      out.message = "caseNames 存在重复项";
      return out;
    }
    names.push_back(n);
  }
  std::set<std::wstring> declared;
  json::JsonArray cases;
  if (!TryGetArray(def, L"cases", cases)) {
    out.code = "structure_drift";
    out.message = "选项声明缺少 cases，拒绝写入";
    return out;
  }
  for (auto&& v : cases) {
    if (v.ValueType() != json::JsonValueType::Object) continue;
    std::wstring n;
    if (TryGetString(v.GetObject(), L"name", n)) declared.insert(n);
  }
  for (auto&& n : names) {
    if (!declared.count(n)) {
      out.code = "invalid_option_value";
      out.message = "caseNames 含未声明项";
      return out;
    }
  }
  double min_count = -1, max_count = -1;
  if (def.HasKey(L"min_count") && def.Lookup(L"min_count").ValueType() == json::JsonValueType::Number)
    min_count = def.Lookup(L"min_count").GetNumber();
  if (def.HasKey(L"max_count") && def.Lookup(L"max_count").ValueType() == json::JsonValueType::Number)
    max_count = def.Lookup(L"max_count").GetNumber();
  if (min_count >= 0 && (double)names.size() < min_count) {
    out.code = "invalid_option_value";
    out.message = "选择数量低于该选项声明的最少数量";
    return out;
  }
  if (max_count >= 0 && (double)names.size() > max_count) {
    out.code = "invalid_option_value";
    out.message = "选择数量超过该选项声明的最多数量";
    return out;
  }
  json::JsonObject norm;
  InsertStr(norm, L"type", L"checkbox");
  json::JsonArray arr_out;
  for (auto&& n : names) arr_out.Append(json::JsonValue::CreateStringValue(n));
  norm.Insert(L"caseNames", arr_out);
  out.value = norm;
  out.ok = true;
  return out;
}

struct InputFieldRules {
  bool found = false;
  bool password = false;
  bool is_path = false;      // input_type file/directory
  std::wstring pipeline_type;  // "", "string", "int", "bool"
  std::wstring verify;         // 正则（可空）
};

inline InputFieldRules FindInputRules(json::JsonObject const& def, const std::wstring& input_name,
                                      bool hotkey) {
  InputFieldRules rules;
  const wchar_t* list_key = hotkey ? L"hotkeys" : L"inputs";
  json::JsonArray items;
  if (!TryGetArray(def, list_key, items)) return rules;
  for (auto&& v : items) {
    if (v.ValueType() != json::JsonValueType::Object) continue;
    auto item = v.GetObject();
    std::wstring n;
    if (!TryGetString(item, L"name", n) || n != input_name) continue;
    rules.found = true;
    bool password = false;
    if (TryGetBool(item, L"password", password)) rules.password = password;
    std::wstring input_type;
    if (TryGetString(item, L"input_type", input_type) &&
        (input_type == L"file" || input_type == L"directory")) {
      rules.is_path = true;
    }
    std::wstring pt;
    if (TryGetString(item, L"pipeline_type", pt)) rules.pipeline_type = pt;
    TryGetString(item, L"verify", rules.verify);
    return rules;
  }
  return rules;
}

inline OptionCheck CheckInputValue(json::JsonObject const& def, json::JsonObject const& value) {
  OptionCheck out;
  std::wstring type;
  if (!TryGetString(value, L"type", type) ||
      (type != L"input" && type != L"hotkey")) {
    out.code = "invalid_option_value";
    out.message = "input 选项的值形状必须是 {type:'input', values:{...}}";
    return out;
  }
  if (value.Size() != 2 || !value.HasKey(L"values") ||
      value.Lookup(L"values").ValueType() != json::JsonValueType::Object) {
    out.code = "invalid_option_value";
    out.message = "input 选项的值形状必须是 {type:'input', values:{...}}";
    return out;
  }
  if (type == L"hotkey") {
    out.code = "unsupported_option";
    out.message = "热键选项需要按键码映射，桥无法安全支持，请在 MaaEnd 界面里修改";
    return out;
  }
  auto values = value.Lookup(L"values").GetObject();
  if (values.Size() > 64) {
    out.code = "invalid_option_value";
    out.message = "values 键数超出上限";
    return out;
  }
  json::JsonObject norm_values;
  for (auto&& kv : values) {
    if (kv.Value().ValueType() != json::JsonValueType::String) {
      out.code = "invalid_option_value";
      out.message = "values 的值必须是字符串";
      return out;
    }
    auto name = std::wstring(kv.Key());
    std::wstring text(kv.Value().GetString());
    if (text.size() > 512) {
      out.code = "invalid_option_value";
      out.message = "输入值长度超出上限";
      return out;
    }
    InputFieldRules rules = FindInputRules(def, name, false);
    if (!rules.found) {
      out.code = "invalid_option_value";
      out.message = "values 含未声明的输入字段";
      return out;
    }
    if (rules.password) {
      out.code = "unsupported_option";
      out.message = "密码字段属于凭据，桥拒绝读写，请在 MaaEnd 界面里修改";
      return out;
    }
    if (rules.is_path) {
      out.code = "unsupported_option";
      out.message = "文件/目录路径字段由桥拒绝，请在 MaaEnd 界面里修改";
      return out;
    }
    if (rules.pipeline_type == L"int") {
      if (!std::regex_match(text, std::wregex(L"-?\\d+"))) {
        out.code = "invalid_option_value";
        out.message = "该输入字段要求整数";
        return out;
      }
    } else if (rules.pipeline_type == L"bool") {
      std::wstring low = Lower(text);
      if (!std::regex_match(low, std::wregex(L"(true|false|1|0|yes|no|y|n)"))) {
        out.code = "invalid_option_value";
        out.message = "该输入字段要求布尔值（true/false/1/0/yes/no）";
        return out;
      }
    }
    if (!rules.verify.empty()) {
      try {
        std::wregex re(rules.verify);
        if (!std::regex_search(text, re)) {
          out.code = "invalid_option_value";
          out.message = "输入值不符合该字段声明的校验规则";
          return out;
        }
      } catch (const std::regex_error&) {
        out.code = "unsupported_option";
        out.message = "字段校验规则无法被桥理解，拒绝写入";
        return out;
      }
    }
    norm_values.Insert(name, json::JsonValue::CreateStringValue(text));
  }
  json::JsonObject norm;
  InsertStr(norm, L"type", L"input");
  norm.Insert(L"values", norm_values);
  out.value = norm;
  out.ok = true;
  return out;
}

inline std::vector<std::wstring> SplitHotkey(const std::wstring& combo) {
  std::vector<std::wstring> parts;
  size_t start = 0;
  for (;;) {
    const size_t end = combo.find(L'+', start);
    auto part = maaend_hotkeys::Normalize(combo.substr(start, end == std::wstring::npos ? end : end - start));
    if (!part.empty()) parts.push_back(std::move(part));
    if (end == std::wstring::npos) break;
    start = end + 1;
  }
  return parts;
}

inline OptionCheck CheckHotkeyValue(json::JsonObject const& def, json::JsonObject const& value) {
  OptionCheck out;
  std::wstring type;
  json::JsonObject values;
  if (value.Size() != 2 || !TryGetString(value, L"type", type) || type != L"hotkey" || !TryGetObject(value, L"values", values)) {
    out.code = "invalid_option_value";
    out.message = "热键值必须为 type:hotkey 与 values 对象";
    return out;
  }
  std::set<std::wstring> declared;
  json::JsonArray fields;
  if (TryGetArray(def, L"hotkeys", fields)) {
    for (const auto& field : fields) {
      std::wstring name;
      if (field.ValueType() == json::JsonValueType::Object && TryGetString(field.GetObject(), L"name", name)) declared.insert(name);
    }
  }
  for (const auto& item : values) {
    if (!declared.count(std::wstring(item.Key())) || item.Value().ValueType() != json::JsonValueType::String || item.Value().GetString().size() > 128) {
      out.code = "invalid_option_value";
      out.message = "热键包含未声明字段或无效文本";
      return out;
    }
    for (const auto& token : SplitHotkey(std::wstring(item.Value().GetString()))) {
      if (!maaend_hotkeys::Known(token)) {
        out.code = "invalid_option_value";
        out.message = "热键包含无法识别的按键名称";
        return out;
      }
    }
  }
  out.ok = true;
  out.value = value;
  return out;
}

inline OptionCheck ValidateOptionValue(json::JsonObject const& def, json::JsonObject const& value,
                                       const std::wstring& controller_name,
                                       const std::wstring& resource_name) {
  OptionCheck out;
  std::wstring type;
  if (!TryGetString(def, L"type", type)) {
    if (def.HasKey(L"type")) {
      out.code = "structure_drift";
      out.message = "选项 type 类型无效，拒绝写入";
      return out;
    }
    type = L"select";
  }
  if (type == L"select") return CheckSelectValue(def, value, controller_name, resource_name);
  if (type == L"switch") return CheckSwitchValue(value);
  if (type == L"checkbox") return CheckCheckboxValue(def, value);
  if (type == L"input") return CheckInputValue(def, value);
  if (type == L"hotkey") {
    return CheckHotkeyValue(def, value);
  }
  out.code = "unsupported_option";
  out.message = "未知选项类型，桥拒绝写入";
  return out;
}

// ---------------------------------------------------------------------------
// 实例状态（GET /api/maa/state）
// ---------------------------------------------------------------------------

struct StateFetch {
  bool ok = false;
  json::JsonObject error;
  json::JsonObject instances;  // id -> {connected, resource_loaded, tasker_inited, is_running, task_run_state}
};

inline StateFetch FetchState(BridgeContext& ctx) {
  StateFetch s;
  HttpExchange ex;
  ex.method = "GET";
  ex.path = "/api/maa/state";
  UpstreamOutcome r = Upstream(ctx, ex);
  if (!r.http_ok()) {
    s.error = UpstreamFailure(r, "GET /api/maa/state");
    return s;
  }
  json::JsonObject root;
  if (!ParseJsonObject(r.body, root) || !TryGetObject(root, L"instances", s.instances)) {
    s.error = MakeError("bad_state_response", "/api/maa/state 缺少 instances 对象");
    return s;
  }
  s.ok = true;
  return s;
}

inline bool InstanceExists(const StateFetch& s, std::string_view id, json::JsonObject& out) {
  if (!s.ok) return false;
  auto key = ToH(id);
  if (!s.instances.HasKey(key)) return false;
  auto v = s.instances.Lookup(key);
  if (v.ValueType() != json::JsonValueType::Object) return false;
  out = v.GetObject();
  return true;
}

inline bool InstanceBool(json::JsonObject const& inst, const wchar_t* key) {
  bool v = false;
  TryGetBool(inst, key, v);
  return v;
}

// ---------------------------------------------------------------------------
// 配置快照（GET /api/config/snapshot；旧版 GET /api/config 仅用于只读回退）
// ---------------------------------------------------------------------------

struct SnapshotFetch {
  bool ok = false;          // 200 且带 config/revision
  bool missing = false;     // 404：上游无 snapshot 路由
  json::JsonObject error;
  json::JsonObject config;
  std::wstring revision;
};

inline SnapshotFetch FetchSnapshot(BridgeContext& ctx) {
  SnapshotFetch snap;
  HttpExchange ex;
  ex.method = "GET";
  ex.path = "/api/config/snapshot";
  UpstreamOutcome r = Upstream(ctx, ex);
  if (r.network_error || r.redirect) {
    snap.error = UpstreamFailure(r, "GET /api/config/snapshot");
    return snap;
  }
  if (r.status == 404) {
    snap.missing = true;
    snap.error = MakeError("snapshot_unsupported", "上游缺少 /api/config/snapshot");
    return snap;
  }
  if (!r.http_ok()) {
    snap.error = UpstreamFailure(r, "GET /api/config/snapshot");
    return snap;
  }
  json::JsonObject root;
  if (!ParseJsonObject(r.body, root) || !TryGetObject(root, L"config", snap.config) ||
      !TryGetString(root, L"revision", snap.revision) || snap.revision.empty()) {
    snap.error = MakeError("snapshot_invalid",
                           "配置快照缺少 config/revision，带版本写入不可用");
    return snap;
  }
  snap.ok = true;
  return snap;
}

inline std::optional<json::JsonObject> FindInstanceById(json::JsonArray const& instances,
                                                         std::string_view id) {
  auto key = ToH(id);
  for (auto&& v : instances) {
    if (v.ValueType() != json::JsonValueType::Object) continue;
    auto obj = v.GetObject();
    std::wstring iid;
    if (TryGetString(obj, L"id", iid) && iid == key) return obj;
  }
  return std::nullopt;
}

// ---------------------------------------------------------------------------
// 运行选项 → pipeline_override（对齐 MXU GUI generateTaskPipelineOverride 口径）
//
// 覆盖顺序：global_option → resource.option → controller.option → task.option，
// 每级内先压入 case/pipeline_override 再递归嵌套 option；input 选项按 GUI 的
// 字符串占位符替换（"{name}" 带引号优先、裸占位符兜底）生成后再解析回 JSON。
// 与 GUI 不同的两点（刻意收紧）：热键与未知选项类型在提交前明确拒绝，不做
// 键码映射；生成结果解析失败也拒绝，不静默丢弃覆盖。
// ---------------------------------------------------------------------------

struct OverrideFlow {
  bool ok = true;
  std::string code, message;
};

inline OverrideFlow OverrideFail(std::string code, std::string message) {
  OverrideFlow f;
  f.ok = false;
  f.code = std::move(code);
  f.message = std::move(message);
  return f;
}

inline std::wstring EffectiveOptionType(json::JsonObject const& def) {
  std::wstring type;
  if (!TryGetString(def, L"type", type) || type.empty()) return L"select";
  return type;
}

// GUI sanitizeOptionValue：保存值与当前声明不符时返回 nullopt（回退默认值）。
inline std::optional<json::JsonObject> SanitizeSavedOptionValue(json::JsonObject const& def,
                                                                const std::wstring& type,
                                                                json::JsonObject const& value) {
  std::wstring value_type;
  if (!TryGetString(value, L"type", value_type) || value_type != type) return std::nullopt;
  if (type == L"select") {
    std::wstring case_name;
    if (!TryGetString(value, L"caseName", case_name)) return std::nullopt;
    json::JsonArray cases;
    if (TryGetArray(def, L"cases", cases)) {
      for (auto&& v : cases) {
        if (v.ValueType() != json::JsonValueType::Object) continue;
        std::wstring n;
        if (TryGetString(v.GetObject(), L"name", n) && n == case_name) return value;
      }
    }
    return std::nullopt;
  }
  if (type == L"checkbox") {
    json::JsonArray arr;
    if (!TryGetArray(value, L"caseNames", arr)) return std::nullopt;
    std::set<std::wstring> declared;
    json::JsonArray cases;
    if (TryGetArray(def, L"cases", cases)) {
      for (auto&& v : cases) {
        if (v.ValueType() != json::JsonValueType::Object) continue;
        std::wstring n;
        if (TryGetString(v.GetObject(), L"name", n)) declared.insert(n);
      }
    }
    json::JsonObject out;
    InsertStr(out, L"type", L"checkbox");
    json::JsonArray kept;
    for (auto&& v : arr) {
      if (v.ValueType() != json::JsonValueType::String) continue;
      auto n = std::wstring(v.GetString());
      if (!declared.count(n)) continue;
      kept.Append(json::JsonValue::CreateStringValue(n));
    }
    if (arr.Size() > 0 && kept.Size() == 0) return std::nullopt;
    out.Insert(L"caseNames", kept);
    return out;
  }
  if (type == L"input") {
    // 只保留已声明输入字段；encryptedValues（凭据密文）原样保留
    std::set<std::wstring> declared;
    json::JsonArray inputs;
    if (TryGetArray(def, L"inputs", inputs)) {
      for (auto&& v : inputs) {
        if (v.ValueType() != json::JsonValueType::Object) continue;
        std::wstring n;
        if (TryGetString(v.GetObject(), L"name", n)) declared.insert(n);
      }
    }
    json::JsonObject out;
    InsertStr(out, L"type", L"input");
    json::JsonObject values, encrypted;
    json::JsonObject src_values, src_encrypted;
    if (TryGetObject(value, L"values", src_values)) {
      for (auto&& kv : src_values) {
        if (declared.count(std::wstring(kv.Key())))
          values.Insert(std::wstring(kv.Key()), kv.Value());
      }
    }
    if (TryGetObject(value, L"encryptedValues", src_encrypted)) {
      for (auto&& kv : src_encrypted) {
        if (declared.count(std::wstring(kv.Key())))
          encrypted.Insert(std::wstring(kv.Key()), kv.Value());
      }
    }
    out.Insert(L"values", values);
    if (encrypted.Size() > 0) out.Insert(L"encryptedValues", encrypted);
    return out;
  }
  return value;  // switch / hotkey 原样
}

// GUI createDefaultOptionValue：未保存或保存值失效时的默认选项值。
inline json::JsonObject DefaultOptionValue(json::JsonObject const& def, const std::wstring& type) {
  json::JsonObject out;
  if (type == L"input" || type == L"hotkey") {
    InsertStr(out, L"type", type);
    json::JsonObject values;
    json::JsonArray items;
    if (TryGetArray(def, type == L"hotkey" ? L"hotkeys" : L"inputs", items)) {
      for (auto&& v : items) {
        if (v.ValueType() != json::JsonValueType::Object) continue;
        auto item = v.GetObject();
        std::wstring name, defv;
        if (!TryGetString(item, L"name", name)) continue;
        TryGetString(item, L"default", defv);
        InsertStr(values, name, defv);
      }
    }
    out.Insert(L"values", values);
    return out;
  }
  if (type == L"switch") {
    std::wstring default_case;
    if (!TryGetString(def, L"default_case", default_case) || default_case.empty()) {
      json::JsonArray cases;
      if (TryGetArray(def, L"cases", cases) && cases.Size() > 0 &&
          cases.GetAt(0).ValueType() == json::JsonValueType::Object) {
        TryGetString(cases.GetAt(0).GetObject(), L"name", default_case);
      }
    }
    if (default_case.empty()) default_case = L"Yes";
    bool is_yes = default_case == L"Yes" || default_case == L"yes" || default_case == L"Y" ||
                  default_case == L"y";
    InsertStr(out, L"type", L"switch");
    InsertBool(out, L"value", is_yes);
    return out;
  }
  if (type == L"checkbox") {
    InsertStr(out, L"type", L"checkbox");
    json::JsonArray names;
    json::JsonArray defaults;
    if (TryGetArray(def, L"default_case", defaults)) {
      for (auto&& v : defaults) {
        if (v.ValueType() == json::JsonValueType::String)
          names.Append(json::JsonValue::CreateStringValue(v.GetString()));
      }
    }
    out.Insert(L"caseNames", names);
    return out;
  }
  // select（GUI 默认类型）
  std::wstring case_name;
  if (!TryGetString(def, L"default_case", case_name) || case_name.empty()) {
    json::JsonArray cases;
    if (TryGetArray(def, L"cases", cases) && cases.Size() > 0 &&
        cases.GetAt(0).ValueType() == json::JsonValueType::Object) {
      TryGetString(cases.GetAt(0).GetObject(), L"name", case_name);
    }
  }
  InsertStr(out, L"type", L"select");
  InsertStr(out, L"caseName", case_name);
  return out;
}

// GUI findSwitchCase：Yes/yes/Y/y 与 No/no/N/n，找不到按勾选状态回退 Yes/No。
inline std::wstring SwitchCaseName(json::JsonObject const& def, bool checked) {
  static const wchar_t* kYes[] = {L"Yes", L"yes", L"Y", L"y"};
  static const wchar_t* kNo[] = {L"No", L"no", L"N", L"n"};
  json::JsonArray cases;
  if (TryGetArray(def, L"cases", cases)) {
    for (auto&& v : cases) {
      if (v.ValueType() != json::JsonValueType::Object) continue;
      std::wstring n;
      if (!TryGetString(v.GetObject(), L"name", n)) continue;
      for (const wchar_t* cand : (checked ? kYes : kNo)) {
        if (n == cand) return n;
      }
    }
  }
  return checked ? L"Yes" : L"No";
}

// GUI isOptionIncompatible：选项声明限定 controller/resource 且不含当前名（或当前名
// 为空）时不适用，跳过不报错。
inline bool OptionAppliesTo(json::JsonObject const& def, const std::wstring& controller_name,
                            const std::wstring& resource_name) {
  auto controllers = StringArray(def, L"controller");
  if (!controllers.empty() &&
      (controller_name.empty() ||
       std::find(controllers.begin(), controllers.end(), controller_name) == controllers.end())) {
    return false;
  }
  auto resources = StringArray(def, L"resource");
  if (!resources.empty() &&
      (resource_name.empty() ||
       std::find(resources.begin(), resources.end(), resource_name) == resources.end())) {
    return false;
  }
  return true;
}

inline std::string ReplaceAllUtf8(std::string s, const std::string& from, const std::string& to) {
  if (from.empty()) return s;
  size_t pos = 0;
  while ((pos = s.find(from, pos)) != std::string::npos) {
    s.replace(pos, from.size(), to);
    pos += to.size();
  }
  return s;
}

inline std::wstring HotkeyControllerType(const CatalogFetch& catalog, const std::wstring& name) {
  json::JsonArray controllers;
  if (TryGetArray(catalog.iface, L"controller", controllers)) {
    for (const auto& item : controllers) {
      if (item.ValueType() != json::JsonValueType::Object) continue;
      auto object = item.GetObject();
      std::wstring id, type;
      if (!TryGetString(object, L"name", id) || id != name) continue;
      TryGetString(object, L"type", type);
      json::JsonObject linuxOptions;
      bool win32 = false;
      if (type == L"Linux" && TryGetObject(object, L"linux", linuxOptions) && TryGetBool(linuxOptions, L"use_win32_vk_code", win32) && win32) return L"Win32";
      return type;
    }
  }
  return L"Win32";
}

inline OverrideFlow CollectOptionOverrides(const CatalogFetch& catalog,
                                           const std::wstring& option_key,
                                           json::JsonObject const& scope_values,
                                           json::JsonArray& overrides,
                                           const std::wstring& controller_name,
                                           const std::wstring& resource_name, int depth) {
  if (depth > 16) {
    return OverrideFail("structure_drift", "选项嵌套层级过深，疑似声明成环，拒绝生成");
  }
  json::JsonObject def;
  if (!GetOptionDef(catalog, option_key, def)) {
    return OverrideFail("structure_drift",
                        "选项声明缺失（目录 option 表中没有该键），拒绝生成任务参数");
  }
  std::wstring type = EffectiveOptionType(def);
  if (type != L"select" && type != L"switch" && type != L"checkbox" && type != L"input" && type != L"hotkey") {
    return OverrideFail("unsupported_option", "未知选项类型，桥拒绝按猜测生成任务参数");
  }
  if (!OptionAppliesTo(def, controller_name, resource_name)) return {};  // 声明不适用：跳过
  // 保存值 → 按当前声明清洗 → 失效则回退默认值
  json::JsonObject value;
  bool have_saved = scope_values.HasKey(option_key) &&
                    scope_values.Lookup(option_key).ValueType() == json::JsonValueType::Object;
  if (have_saved) {
    auto sanitized =
        SanitizeSavedOptionValue(def, type, scope_values.Lookup(option_key).GetObject());
    value = sanitized ? *sanitized : DefaultOptionValue(def, type);
  } else {
    value = DefaultOptionValue(def, type);
  }

  if (type == L"checkbox") {
    std::set<std::wstring> selected;
    json::JsonArray names;
    if (TryGetArray(value, L"caseNames", names)) {
      for (auto&& v : names) {
        if (v.ValueType() == json::JsonValueType::String) selected.insert(std::wstring(v.GetString()));
      }
    }
    json::JsonArray cases;
    if (TryGetArray(def, L"cases", cases)) {
      for (auto&& v : cases) {  // 按 cases 声明顺序合并
        if (v.ValueType() != json::JsonValueType::Object) continue;
        auto c = v.GetObject();
        std::wstring n;
        if (!TryGetString(c, L"name", n) || !selected.count(n)) continue;
        if (c.HasKey(L"pipeline_override") &&
            c.Lookup(L"pipeline_override").ValueType() == json::JsonValueType::Object) {
          overrides.Append(c.Lookup(L"pipeline_override"));
        }
        for (auto& nested : StringArray(c, L"option")) {
          auto flow = CollectOptionOverrides(catalog, nested, scope_values, overrides,
                                             controller_name, resource_name, depth + 1);
          if (!flow.ok) return flow;
        }
      }
    }
    return {};
  }
  if (type == L"select" || type == L"switch") {
    std::wstring case_name;
    if (type == L"switch") {
      bool checked = false;
      TryGetBool(value, L"value", checked);
      case_name = SwitchCaseName(def, checked);
    } else {
      TryGetString(value, L"caseName", case_name);
    }
    json::JsonArray cases;
    if (TryGetArray(def, L"cases", cases)) {
      for (auto&& v : cases) {
        if (v.ValueType() != json::JsonValueType::Object) continue;
        auto c = v.GetObject();
        std::wstring n;
        if (!TryGetString(c, L"name", n) || n != case_name) continue;
        if (c.HasKey(L"pipeline_override") &&
            c.Lookup(L"pipeline_override").ValueType() == json::JsonValueType::Object) {
          overrides.Append(c.Lookup(L"pipeline_override"));
        }
        for (auto& nested : StringArray(c, L"option")) {
          auto flow = CollectOptionOverrides(catalog, nested, scope_values, overrides,
                                             controller_name, resource_name, depth + 1);
          if (!flow.ok) return flow;
        }
        break;
      }
    }
    return {};
  }
  // input：GUI 只在选项自身声明 pipeline_override 时做占位符替换
  if (!def.HasKey(L"pipeline_override") ||
      def.Lookup(L"pipeline_override").ValueType() != json::JsonValueType::Object) {
    return {};
  }
  json::JsonObject task_values;
  TryGetObject(value, L"values", task_values);
  json::JsonObject encrypted_values;
  TryGetObject(value, L"encryptedValues", encrypted_values);
  std::string override_str = ToUtf8(def.Lookup(L"pipeline_override").GetObject().Stringify());
  json::JsonArray inputs;
  if (TryGetArray(def, type == L"hotkey" ? L"hotkeys" : L"inputs", inputs)) {
    for (auto&& v : inputs) {
      if (v.ValueType() != json::JsonValueType::Object) continue;
      auto item = v.GetObject();
      std::wstring name, item_default;
      if (!TryGetString(item, L"name", name)) continue;
      TryGetString(item, L"default", item_default);
      std::wstring raw;
      bool have_raw = false;
      if (task_values.HasKey(name) &&
          task_values.Lookup(name).ValueType() == json::JsonValueType::String) {
        raw = std::wstring(task_values.Lookup(name).GetString());
        have_raw = true;
      }
      if (!have_raw) raw = item_default;  // GUI: values[name] ?? default ?? ''
      if (type == L"hotkey") {
        const auto parts = SplitHotkey(raw);
        const auto keymap = HotkeyControllerType(catalog, controller_name);
        const std::wstring primary = parts.empty() ? L"" : parts.back();
        const std::wstring modifier1 = parts.size() >= 2 ? parts.front() : L"";
        const std::wstring modifier2 = parts.size() >= 3 ? parts[1] : L"";
        const std::pair<std::string, std::wstring> slots[] = {
            {"{" + ToUtf8(name) + "}", primary}, {"{" + ToUtf8(name) + ".primary}", primary},
            {"{" + ToUtf8(name) + ".modifier1}", modifier1}, {"{" + ToUtf8(name) + ".modifier2}", modifier2}};
        for (const auto& slot : slots) {
          auto code = slot.second.empty() ? std::optional<int>{0} : maaend_hotkeys::Code(slot.second, keymap);
          if (!code) return OverrideFail("invalid_option_value", "热键在当前控制器类型下无对应键码，本次未提交");
          const auto number = std::to_string(*code);
          override_str = ReplaceAllUtf8(override_str, "\"" + slot.first + "\"", number);
          override_str = ReplaceAllUtf8(override_str, slot.first, number);
        }
        continue;
      }
      InputFieldRules rules = FindInputRules(def, name, false);
      if (rules.password && raw.empty() && encrypted_values.HasKey(name) &&
          encrypted_values.Lookup(name).ValueType() == json::JsonValueType::String &&
          !std::wstring(encrypted_values.Lookup(name).GetString()).empty()) {
        return OverrideFail(
            "unsupported_option",
            "选项覆盖需要密码字段，但保存值只有密文；桥无法解密，请在 MaaEnd 界面运行该任务");
      }
      std::wstring pipeline_type = rules.pipeline_type.empty() ? L"string" : rules.pipeline_type;
      std::string placeholder = "{" + ToUtf8(name) + "}";
      std::string quoted = "\"" + placeholder + "\"";
      std::string quoted_repl, bare_repl;
      if (pipeline_type == L"int") {
        quoted_repl = bare_repl = raw.empty() ? "0" : ToUtf8(raw);
      } else if (pipeline_type == L"bool") {
        std::wstring low = Lower(raw);
        bool truthy = low == L"true" || low == L"1" || low == L"yes" || low == L"y";
        quoted_repl = bare_repl = truthy ? "true" : "false";
      } else {
        quoted_repl = ToUtf8(json::JsonValue::CreateStringValue(raw).Stringify());
        bare_repl = quoted_repl.size() >= 2 ? quoted_repl.substr(1, quoted_repl.size() - 2)
                                            : quoted_repl;
      }
      override_str = ReplaceAllUtf8(override_str, quoted, quoted_repl);
      override_str = ReplaceAllUtf8(override_str, placeholder, bare_repl);
    }
  }
  json::JsonObject parsed;
  if (!ParseJsonObject(override_str, parsed)) {
    return OverrideFail("invalid_option_value",
                        "选项占位符替换后的覆盖不是合法 JSON，拒绝提交（输入值可能违反声明类型）");
  }
  overrides.Append(parsed);
  return {};
}

struct PipelineGenResult {
  bool ok = false;
  std::string pipeline;  // JSON 数组字符串（与 GUI generateTaskPipelineOverride 同形）
  std::string code, message;
};

inline PipelineGenResult GenerateTaskPipelineOverride(
    const CatalogFetch& catalog, json::JsonObject const& task_def,
    json::JsonObject const& selected_task, const std::wstring& controller_name,
    const std::wstring& resource_name, json::JsonObject const& global_values) {
  PipelineGenResult out;
  json::JsonArray overrides;
  if (task_def.HasKey(L"pipeline_override") &&
      task_def.Lookup(L"pipeline_override").ValueType() == json::JsonValueType::Object) {
    overrides.Append(task_def.Lookup(L"pipeline_override"));
  }
  json::JsonObject task_values;
  TryGetObject(selected_task, L"optionValues", task_values);
  auto run_chain = [&](const std::vector<std::wstring>& keys,
                       json::JsonObject const& values) -> OverrideFlow {
    for (auto& key : keys) {
      auto flow = CollectOptionOverrides(catalog, key, values, overrides, controller_name,
                                         resource_name, 0);
      if (!flow.ok) return flow;
    }
    return {};
  };
  // 覆盖顺序（GUI v2.3.0）：global_option → resource.option → controller.option → task.option
  OverrideFlow flow = run_chain(StringArray(catalog.iface, L"global_option"), global_values);
  if (flow.ok) {
    std::vector<std::wstring> keys;
    json::JsonArray resources;
    if (TryGetArray(catalog.iface, L"resource", resources)) {
      for (auto&& v : resources) {
        if (v.ValueType() != json::JsonValueType::Object) continue;
        std::wstring n;
        if (TryGetString(v.GetObject(), L"name", n) && n == resource_name) {
          keys = StringArray(v.GetObject(), L"option");
          break;
        }
      }
    }
    flow = run_chain(keys, task_values);
  }
  if (flow.ok) {
    std::vector<std::wstring> keys;
    json::JsonArray controllers;
    if (TryGetArray(catalog.iface, L"controller", controllers)) {
      for (auto&& v : controllers) {
        if (v.ValueType() != json::JsonValueType::Object) continue;
        std::wstring n;
        if (TryGetString(v.GetObject(), L"name", n) && n == controller_name) {
          keys = StringArray(v.GetObject(), L"option");
          break;
        }
      }
    }
    flow = run_chain(keys, task_values);
  }
  if (flow.ok) flow = run_chain(StringArray(task_def, L"option"), task_values);
  if (!flow.ok) {
    out.code = flow.code;
    out.message = flow.message;
    return out;
  }
  out.pipeline = ToUtf8(overrides.Stringify());
  out.ok = true;
  return out;
}

// ---------------------------------------------------------------------------
// Agent 声明（interface.agent）与 PI 环境变量
// ---------------------------------------------------------------------------

struct AgentPlan {
  bool ok = false;
  bool use_start = false;        // 声明了 agent 时走 /tasks/start（GUI 同款启动路径）
  json::JsonArray configs;       // 归一化后的 AgentConfig 数组（来自 interface 声明）
  std::string code, message;
};

inline AgentPlan ResolveAgentPlan(const CatalogFetch& catalog) {
  AgentPlan plan;
  plan.ok = true;
  if (!catalog.iface.HasKey(L"agent")) return plan;  // 无声明 → /tasks/run
  auto v = catalog.iface.Lookup(L"agent");
  json::JsonArray configs;
  if (v.ValueType() == json::JsonValueType::Object) {
    configs.Append(v);
  } else if (v.ValueType() == json::JsonValueType::Array) {
    configs = v.GetArray();
  } else {
    plan.ok = false;
    plan.code = "structure_drift";
    plan.message = "interface.agent 声明形状不受支持（需对象或数组），拒绝启动 Agent";
    return plan;
  }
  if (configs.Size() == 0) return plan;  // 空数组等价无 Agent → /tasks/run
  for (auto&& item : configs) {
    if (item.ValueType() != json::JsonValueType::Object) {
      plan.ok = false;
      plan.code = "structure_drift";
      plan.message = "interface.agent 含非对象项，拒绝启动 Agent";
      return plan;
    }
    auto cfg = item.GetObject();
    std::wstring child_exec;
    if (!TryGetString(cfg, L"child_exec", child_exec) || child_exec.empty() ||
        child_exec.size() > 512) {
      plan.ok = false;
      plan.code = "structure_drift";
      plan.message = "interface.agent 项缺少合法 child_exec，拒绝启动 Agent";
      return plan;
    }
    if (cfg.HasKey(L"child_args")) {
      auto args = cfg.Lookup(L"child_args");
      if (args.ValueType() != json::JsonValueType::Array) {
        plan.ok = false;
        plan.code = "structure_drift";
        plan.message = "interface.agent 的 child_args 必须是字符串数组，拒绝启动 Agent";
        return plan;
      }
      for (auto&& a : args.GetArray()) {
        if (a.ValueType() != json::JsonValueType::String) {
          plan.ok = false;
          plan.code = "structure_drift";
          plan.message = "interface.agent 的 child_args 必须是字符串数组，拒绝启动 Agent";
          return plan;
        }
      }
    }
    if (cfg.HasKey(L"identifier") &&
        cfg.Lookup(L"identifier").ValueType() != json::JsonValueType::String) {
      plan.ok = false;
      plan.code = "structure_drift";
      plan.message = "interface.agent 的 identifier 必须是字符串，拒绝启动 Agent";
      return plan;
    }
  }
  plan.configs = configs;
  plan.use_start = true;
  return plan;
}

// GUI getInterfaceLangKey：语言偏好 → interface 翻译键；'system' 用系统 UI 语言解析。
inline std::wstring InterfaceLangKey(const std::wstring& pref) {
  const std::pair<std::wstring, std::wstring> kMap[] = {
      {L"zh-CN", L"zh_cn"}, {L"zh-TW", L"zh_tw"}, {L"en-US", L"en_us"},
      {L"ja-JP", L"ja_jp"}, {L"ko-KR", L"ko_kr"}};
  for (auto& [k, v] : kMap) {
    if (pref == k) return v;
  }
  if (pref == L"system") {
    LANGID lang = GetUserDefaultUILanguage();
    switch (PRIMARYLANGID(lang)) {
      case LANG_CHINESE: {
        WORD sub = SUBLANGID(lang);
        if (sub == SUBLANG_CHINESE_TRADITIONAL || sub == SUBLANG_CHINESE_HONGKONG ||
            sub == SUBLANG_CHINESE_MACAU) {
          return L"zh_tw";
        }
        return L"zh_cn";
      }
      case LANG_JAPANESE:
        return L"ja_jp";
      case LANG_KOREAN:
        return L"ko_kr";
      default:
        return L"en_us";
    }
  }
  return L"en_us";
}

// GUI resolvePiI18nText/resolveI18nValue：'$key' 查当前语言表，缺失回退去掉 $ 的键。
inline json::IJsonValue ResolvePiI18n(json::IJsonValue const& v, const CatalogFetch& c,
                                      const std::wstring& lang) {
  using json::JsonValueType;
  if (v.ValueType() == JsonValueType::String) {
    auto s = std::wstring(v.GetString());
    if (s.empty() || s[0] != L'$') return v;
    std::wstring key = s.substr(1);
    json::JsonObject translations, table;
    if (TryGetObject(c.root, L"translations", translations) &&
        TryGetObject(translations, lang, table) && table.HasKey(key)) {
      auto hit = table.Lookup(key);
      if (hit.ValueType() == JsonValueType::String) return hit;
    }
    return json::JsonValue::CreateStringValue(key);
  }
  if (v.ValueType() == JsonValueType::Array) {
    json::JsonArray out;
    for (auto&& item : v.GetArray()) out.Append(ResolvePiI18n(item, c, lang));
    return out;
  }
  if (v.ValueType() == JsonValueType::Object) {
    json::JsonObject out;
    for (auto&& kv : v.GetObject())
      out.Insert(std::wstring(kv.Key()), ResolvePiI18n(kv.Value(), c, lang));
    return out;
  }
  return v;
}

// GUI buildPiEnvVars：只保留能从实时声明与后端状态核实的键；不伪造客户端版本号。
inline json::JsonObject BuildPiEnvs(const CatalogFetch& c, const std::wstring& controller_name,
                                    const std::wstring& resource_name, const std::wstring& lang,
                                    const std::wstring& maafw_version) {
  json::JsonObject envs;
  InsertStrUtf8(envs, L"PI_INTERFACE_VERSION", "v2.11.0");
  InsertStrUtf8(envs, L"PI_CLIENT_NAME", "MXU");
  if (!lang.empty()) InsertStr(envs, L"PI_CLIENT_LANGUAGE", lang);
  if (!c.project_version.empty()) InsertStr(envs, L"PI_VERSION", c.project_version);
  if (!maafw_version.empty()) {
    InsertStr(envs, L"PI_CLIENT_MAAFW_VERSION",
              maafw_version[0] == L'v' ? maafw_version : L"v" + maafw_version);
  }
  json::JsonArray defs;
  const wchar_t* list_key = nullptr;
  const std::wstring* want = nullptr;
  const wchar_t* env_name = nullptr;
  for (int pass = 0; pass < 2; ++pass) {
    list_key = pass == 0 ? L"controller" : L"resource";
    want = pass == 0 ? &controller_name : &resource_name;
    env_name = pass == 0 ? L"PI_CONTROLLER" : L"PI_RESOURCE";
    if (want->empty() || !TryGetArray(c.iface, list_key, defs)) continue;
    for (auto&& v : defs) {
      if (v.ValueType() != json::JsonValueType::Object) continue;
      std::wstring n;
      if (!TryGetString(v.GetObject(), L"name", n) || n != *want) continue;
      json::IJsonValue resolved = ResolvePiI18n(v, c, lang);
      if (resolved.ValueType() == json::JsonValueType::Object) {
        InsertStrUtf8(envs, env_name, ToUtf8(resolved.GetObject().Stringify()));
      }
      break;
    }
  }
  return envs;
}

inline std::wstring FetchMaaVersion(BridgeContext& ctx) {
  HttpExchange ex;
  ex.method = "GET";
  ex.path = "/api/maa/initialized";
  UpstreamOutcome r = Upstream(ctx, ex);
  if (!r.http_ok()) return {};
  json::JsonObject root;
  std::wstring ver;
  if (ParseJsonObject(r.body, root)) TryGetString(root, L"version", ver);
  return ver;
}

// ---------------------------------------------------------------------------
// 配置改动记录与范围核实
// ---------------------------------------------------------------------------

struct MutationRecord {
  std::wstring instance_id;
  std::vector<std::wstring> new_order;                              // 空 = 不调整
  std::map<std::wstring, bool> enabled;                             // selectedTask id -> enabled
  std::map<std::wstring, std::map<std::wstring, json::JsonObject>> option_values;
};

// 校验“改动只落在允许的字段上”：instances 之外不变；其它实例不变；目标实例除
// selectedTasks 外不变；selectedTasks 内同 id 元素除 enabled / 被写的 optionValues 键外不变。
inline bool ChangesWithinScope(json::JsonObject const& base, json::JsonObject const& next,
                               const MutationRecord& rec, std::string& reason) {
  if (base.Size() != next.Size()) {
    reason = "config-top-level-keys-changed";
    return false;
  }
  auto base_has_instances = base.HasKey(L"instances");
  auto next_has_instances = next.HasKey(L"instances");
  if (base_has_instances != next_has_instances) {
    reason = "instances-presence-changed";
    return false;
  }
  for (auto&& kv : base) {
    if (std::wstring(kv.Key()) == L"instances") continue;
    if (!next.HasKey(std::wstring(kv.Key())) || !DeepEqual(kv.Value(), next.Lookup(std::wstring(kv.Key())))) {
      reason = "non-instance-key-changed";
      return false;
    }
  }
  json::JsonArray base_arr, next_arr;
  if (!TryGetArray(base, L"instances", base_arr) || !TryGetArray(next, L"instances", next_arr)) {
    reason = "instances-missing";
    return false;
  }
  if (base_arr.Size() != next_arr.Size()) {
    reason = "instance-count-changed";
    return false;
  }
  auto target_key = rec.instance_id;
  for (uint32_t i = 0; i < base_arr.Size(); ++i) {
    auto bv = base_arr.GetAt(i);
    auto nv = next_arr.GetAt(i);
    if (bv.ValueType() != json::JsonValueType::Object ||
        nv.ValueType() != json::JsonValueType::Object) {
      reason = "instance-not-object";
      return false;
    }
    auto bo = bv.GetObject();
    auto no = nv.GetObject();
    std::wstring bid, nid;
    if (!TryGetString(bo, L"id", bid) || !TryGetString(no, L"id", nid)) {
      reason = "instance-id-missing";
      return false;
    }
    if (bid != nid) {
      reason = "instance-order-changed";
      return false;
    }
    if (bid != target_key) {
      if (!DeepEqual(bo, no)) {
        reason = "other-instance-changed";
        return false;
      }
      continue;
    }
    // 目标实例：除 selectedTasks 外全部一致
    if (bo.Size() != no.Size()) {
      reason = "target-instance-keys-changed";
      return false;
    }
    for (auto&& kv2 : bo) {
      if (std::wstring(kv2.Key()) == L"selectedTasks") continue;
      if (!no.HasKey(std::wstring(kv2.Key())) || !DeepEqual(kv2.Value(), no.Lookup(std::wstring(kv2.Key())))) {
        reason = "target-instance-field-changed";
        return false;
      }
    }
    json::JsonArray bt, nt;
    if (!TryGetArray(bo, L"selectedTasks", bt) || !TryGetArray(no, L"selectedTasks", nt)) {
      reason = "selected-tasks-missing";
      return false;
    }
    if (bt.Size() != nt.Size()) {
      reason = "selected-task-count-changed";
      return false;
    }
    std::map<std::wstring, json::JsonObject> base_tasks, next_tasks;
    for (auto&& v : bt) {
      if (v.ValueType() != json::JsonValueType::Object) {
        reason = "selected-task-not-object";
        return false;
      }
      std::wstring tid;
      if (!TryGetString(v.GetObject(), L"id", tid)) {
        reason = "selected-task-id-missing";
        return false;
      }
      base_tasks[tid] = v.GetObject();
    }
    for (auto&& v : nt) {
      if (v.ValueType() != json::JsonValueType::Object) {
        reason = "selected-task-not-object";
        return false;
      }
      std::wstring tid;
      if (!TryGetString(v.GetObject(), L"id", tid)) {
        reason = "selected-task-id-missing";
        return false;
      }
      next_tasks[tid] = v.GetObject();
    }
    if (base_tasks.size() != next_tasks.size() ||
        base_tasks.size() != bt.Size()) {
      reason = "selected-task-id-set-changed";
      return false;
    }
    for (auto&& [tid, base_task] : base_tasks) {
      auto it = next_tasks.find(tid);
      if (it == next_tasks.end()) {
        reason = "selected-task-id-set-changed";
        return false;
      }
      auto& next_task = it->second;
      bool enabled_touched = rec.enabled.count(tid) > 0;
      auto opt_it = rec.option_values.find(tid);
      const std::map<std::wstring, json::JsonObject>* touched_opts =
          opt_it != rec.option_values.end() ? &opt_it->second : nullptr;
      if (base_task.Size() != next_task.Size()) {
        reason = "selected-task-keys-changed";
        return false;
      }
      for (auto&& field : base_task) {
        const std::wstring& f = std::wstring(field.Key());
        if (f == L"enabled" && enabled_touched) continue;
        if (f == L"optionValues" && touched_opts) continue;
        if (!next_task.HasKey(f) || !DeepEqual(field.Value(), next_task.Lookup(f))) {
          reason = "selected-task-field-changed";
          return false;
        }
      }
      if (touched_opts) {
        json::JsonObject base_opts, next_opts;
        bool has_base = TryGetObject(base_task, L"optionValues", base_opts);
        bool has_next = TryGetObject(next_task, L"optionValues", next_opts);
        if (!has_base || !has_next) {
          reason = "option-values-missing";
          return false;
        }
        if (base_opts.Size() != next_opts.Size()) {
          reason = "option-values-key-set-changed";
          return false;
        }
        for (auto&& okv : base_opts) {
          if (!next_opts.HasKey(std::wstring(okv.Key()))) {
            reason = "option-values-key-set-changed";
            return false;
          }
          bool this_key_touched = touched_opts->count(std::wstring(okv.Key())) > 0;
          if (this_key_touched) {
            const json::JsonObject& expected = touched_opts->at(std::wstring(okv.Key()));
            if (!DeepEqual(next_opts.Lookup(std::wstring(okv.Key())), expected)) {
              reason = "option-value-not-as-requested";
              return false;
            }
          } else if (!DeepEqual(okv.Value(), next_opts.Lookup(std::wstring(okv.Key())))) {
            reason = "untouched-option-value-changed";
            return false;
          }
        }
      }
    }
  }
  return true;
}

// ---------------------------------------------------------------------------
// 处理器
// ---------------------------------------------------------------------------

inline json::JsonObject HandleHealth(BridgeContext& ctx, const json::JsonObject&) {
  json::JsonObject out = MakeOk();
  InsertStrUtf8(out, L"bridge", "maaend-bridge");
  InsertStrUtf8(out, L"version", "0.1.0");
  InsertStrUtf8(out, L"upstream",
                std::string("http://127.0.0.1:") + std::to_string(ctx.options->upstream_port));
  InsertBool(out, L"tokenConfigured", !ctx.options->token_env.empty());
  return out;
}

inline void AppendInstanceSummary(json::JsonObject& out, const std::wstring& id,
                                  json::JsonObject const& inst) {
  json::JsonObject item;
  InsertBool(item, L"connected", InstanceBool(inst, L"connected"));
  InsertBool(item, L"resource_loaded", InstanceBool(inst, L"resource_loaded"));
  InsertBool(item, L"tasker_inited", InstanceBool(inst, L"tasker_inited"));
  InsertBool(item, L"is_running", InstanceBool(inst, L"is_running"));
  if (inst.HasKey(L"task_run_state") &&
      inst.Lookup(L"task_run_state").ValueType() == json::JsonValueType::Object) {
    item.Insert(L"task_run_state", inst.Lookup(L"task_run_state"));
  }
  out.Insert(id, item);
}

inline json::JsonObject HandleStatus(BridgeContext& ctx, const json::JsonObject&) {
  json::JsonObject out = MakeOk();
  // heartbeat
  HttpExchange hb;
  hb.method = "GET";
  hb.path = "/api/heartbeat";
  UpstreamOutcome hr = Upstream(ctx, hb);
  json::JsonObject upstream;
  InsertBool(upstream, L"reachable", hr.http_ok());
  if (hr.network_error) InsertStrUtf8(upstream, L"networkReason", hr.network_reason);
  upstream.Insert(L"detail", [&] {
    json::JsonObject d;
    InsertStrUtf8(d, L"base",
                  std::string("http://127.0.0.1:") + std::to_string(ctx.options->upstream_port));
    return d;
  }());
  out.Insert(L"upstream", upstream);
  // maa initialized
  HttpExchange init;
  init.method = "GET";
  init.path = "/api/maa/initialized";
  UpstreamOutcome ir = Upstream(ctx, init);
  if (ir.http_ok()) {
    json::JsonObject maa;
    json::JsonObject root;
    if (ParseJsonObject(ir.body, root)) {
      bool flag = false;
      if (TryGetBool(root, L"initialized", flag)) InsertBool(maa, L"initialized", flag);
      std::wstring ver;
      if (TryGetString(root, L"version", ver)) InsertStr(maa, L"version", ver);
    }
    out.Insert(L"maa", maa);
  }
  // instances
  StateFetch state = FetchState(ctx);
  if (state.ok) {
    json::JsonObject instances;
    for (auto&& kv : state.instances) {
      if (kv.Value().ValueType() != json::JsonValueType::Object) continue;
      AppendInstanceSummary(instances, std::wstring(kv.Key()), kv.Value().GetObject());
    }
    out.Insert(L"instances", instances);
  }
  // catalog
  CatalogFetch catalog = FetchCatalog(ctx);
  if (catalog.ok) {
    json::JsonObject cat;
    InsertStrUtf8(cat, L"catalogHash", catalog.hash);
    InsertStr(cat, L"projectName", catalog.project_name);
    InsertStr(cat, L"projectVersion", catalog.project_version);
    json::JsonArray tasks;
    if (TryGetArray(catalog.iface, L"task", tasks)) InsertInt(cat, L"taskCount", tasks.Size());
    json::JsonObject opts;
    if (TryGetObject(catalog.iface, L"option", opts)) InsertInt(cat, L"optionCount", opts.Size());
    out.Insert(L"catalog", cat);
  } else {
    json::JsonObject cat_error = catalog.error;
    if (cat_error.HasKey(L"error")) {
      out.Insert(L"catalogError", cat_error.Lookup(L"error"));
    }
  }
  // snapshot 能力（决定配置写是否可用）
  SnapshotFetch snap = FetchSnapshot(ctx);
  json::JsonObject config_cap;
  if (snap.ok) {
    InsertBool(config_cap, L"snapshotSupported", true);
    InsertStr(config_cap, L"revision", snap.revision);
  } else if (snap.missing) {
    InsertBool(config_cap, L"snapshotSupported", false);
    InsertBool(config_cap, L"versionedWriteAllowed", false);
  }
  out.Insert(L"config", config_cap);
  return out;
}

inline json::JsonObject HandleCatalogSearch(BridgeContext& ctx, const json::JsonObject& args) {
  std::wstring query;
  if (!TryGetString(args, L"query", query) || query.empty()) {
    return MakeError("invalid_request", "缺少 query");
  }
  int limit = 20;
  if (args.HasKey(L"limit") && args.Lookup(L"limit").ValueType() == json::JsonValueType::Number) {
    limit = (int)args.Lookup(L"limit").GetNumber();
    if (limit < 1 || limit > 50) return MakeError("invalid_request", "limit 取值 1..50");
  }
  CatalogFetch catalog = FetchCatalog(ctx);
  if (!catalog.ok) return catalog.error;
  std::wstring needle = Lower(query);
  json::JsonArray matches;
  int total = 0;
  json::JsonArray tasks;
  if (TryGetArray(catalog.iface, L"task", tasks)) {
    for (auto&& v : tasks) {
      if (v.ValueType() != json::JsonValueType::Object) continue;
      auto t = v.GetObject();
      std::wstring name;
      if (!TryGetString(t, L"name", name)) continue;
      std::wstring label, entry, desc;
      TryGetString(t, L"label", label);
      TryGetString(t, L"entry", entry);
      TryGetString(t, L"description", desc);
      std::wstring resolved = ResolveLabel(catalog, label);
      std::wstring resolved_desc = ResolveLabel(catalog, desc);
      std::wstring hay = Lower(name + L" " + entry + L" " + label + L" " + resolved + L" " +
                               desc + L" " + resolved_desc);
      if (hay.find(needle) == std::wstring::npos) continue;
      ++total;
      if ((int)matches.Size() >= limit) continue;
      json::JsonObject m;
      InsertStrUtf8(m, L"kind", "task");
      InsertStr(m, L"name", name);
      InsertStr(m, L"entry", entry);
      InsertStr(m, L"label", resolved);
      matches.Append(m);
    }
  }
  json::JsonObject opts;
  if (TryGetObject(catalog.iface, L"option", opts)) {
    for (auto&& kv : opts) {
      auto def = kv.Value();
      if (def.ValueType() != json::JsonValueType::Object) continue;
      auto d = def.GetObject();
      std::wstring label;
      TryGetString(d, L"label", label);
      std::wstring resolved = ResolveLabel(catalog, label);
      std::wstring type;
      TryGetString(d, L"type", type);
      std::wstring hay = Lower(std::wstring(kv.Key()) + L" " + label + L" " + resolved + L" " + type);
      if (hay.find(needle) == std::wstring::npos) continue;
      ++total;
      if ((int)matches.Size() >= limit) continue;
      json::JsonObject m;
      InsertStrUtf8(m, L"kind", "option");
      InsertStr(m, L"key", std::wstring(kv.Key()));
      InsertStr(m, L"type", type);
      InsertStr(m, L"label", resolved);
      matches.Append(m);
    }
  }
  json::JsonObject out = MakeOk();
  InsertStrUtf8(out, L"catalogHash", catalog.hash);
  InsertInt(out, L"total", total);
  out.Insert(L"matches", matches);
  return out;
}

inline json::JsonObject DescribeOption(const CatalogFetch& catalog, const std::wstring& key,
                                       json::JsonObject const& def) {
  json::JsonObject out = MakeOk();
  InsertStrUtf8(out, L"kind", "option");
  InsertStr(out, L"key", key);
  std::wstring type, label;
  TryGetString(def, L"type", type);
  TryGetString(def, L"label", label);
  InsertStr(out, L"type", type);
  InsertStr(out, L"label", ResolveLabel(catalog, label));
  json::JsonArray cases;
  if (TryGetArray(def, L"cases", cases)) {
    json::JsonArray case_list;
    for (auto&& v : cases) {
      if (v.ValueType() != json::JsonValueType::Object) continue;
      auto c = v.GetObject();
      std::wstring name, clabel;
      TryGetString(c, L"name", name);
      TryGetString(c, L"label", clabel);
      json::JsonObject item;
      InsertStr(item, L"name", name);
      InsertStr(item, L"label", ResolveLabel(catalog, clabel));
      case_list.Append(item);
    }
    out.Insert(L"cases", case_list);
  }
  json::JsonArray inputs;
  if (TryGetArray(def, L"inputs", inputs)) {
    json::JsonArray input_list;
    for (auto&& v : inputs) {
      if (v.ValueType() != json::JsonValueType::Object) continue;
      auto f = v.GetObject();
      std::wstring name, flabel, defv, pt, it;
      bool password = false;
      TryGetString(f, L"name", name);
      TryGetString(f, L"label", flabel);
      TryGetString(f, L"default", defv);
      TryGetString(f, L"pipeline_type", pt);
      TryGetString(f, L"input_type", it);
      TryGetBool(f, L"password", password);
      json::JsonObject item;
      InsertStr(item, L"name", name);
      InsertStr(item, L"label", ResolveLabel(catalog, flabel));
      InsertStr(item, L"pipeline_type", pt);
      InsertStr(item, L"input_type", it);
      InsertBool(item, L"password", password);
      input_list.Append(item);
    }
    out.Insert(L"inputs", input_list);
  }
  // 哪些任务引用了该选项
  json::JsonArray used_by;
  json::JsonArray tasks;
  if (TryGetArray(catalog.iface, L"task", tasks)) {
    for (auto&& v : tasks) {
      if (v.ValueType() != json::JsonValueType::Object) continue;
      auto t = v.GetObject();
      auto keys = StringArray(t, L"option");
      if (std::find(keys.begin(), keys.end(), key) != keys.end()) {
        std::wstring name;
        if (TryGetString(t, L"name", name)) used_by.Append(json::JsonValue::CreateStringValue(name));
      }
    }
  }
  out.Insert(L"usedByTasks", used_by);
  InsertStrUtf8(out, L"catalogHash", catalog.hash);
  return out;
}

inline json::JsonObject HandleCatalogDescribe(BridgeContext& ctx, const json::JsonObject& args) {
  std::wstring name;
  if (!TryGetString(args, L"name", name) || name.empty()) {
    return MakeError("invalid_request", "缺少 name");
  }
  CatalogFetch catalog = FetchCatalog(ctx);
  if (!catalog.ok) return catalog.error;
  TaskDecl task = FindTask(catalog, name);
  if (task.found) {
    json::JsonObject out = MakeOk();
    InsertStrUtf8(out, L"kind", "task");
    InsertStr(out, L"name", task.name);
    InsertStr(out, L"entry", task.entry);
    InsertStr(out, L"label", ResolveLabel(catalog, task.label));
    InsertStr(out, L"description", ResolveLabel(catalog, task.description));
    json::JsonArray groups;
    for (auto& g : task.groups) groups.Append(json::JsonValue::CreateStringValue(g));
    out.Insert(L"group", groups);
    json::JsonArray option_list;
    for (auto& key : task.option_keys) {
      json::JsonObject def;
      if (GetOptionDef(catalog, key, def)) {
        std::wstring type, label;
        TryGetString(def, L"type", type);
        TryGetString(def, L"label", label);
        json::JsonObject item;
        InsertStr(item, L"key", key);
        InsertStr(item, L"type", type);
        InsertStr(item, L"label", ResolveLabel(catalog, label));
        option_list.Append(item);
      }
    }
    out.Insert(L"options", option_list);
    InsertStrUtf8(out, L"catalogHash", catalog.hash);
    return out;
  }
  json::JsonObject def;
  if (GetOptionDef(catalog, name, def)) return DescribeOption(catalog, name, def);
  return MakeError("not_found", "目录中没有该任务或选项");
}

inline json::JsonObject HandleRead(BridgeContext& ctx, const json::JsonObject& args) {
  StateFetch state = FetchState(ctx);
  if (!state.ok) return state.error;
  json::JsonObject out = MakeOk();
  std::wstring instance_id;
  bool filtered = TryGetString(args, L"instanceId", instance_id) && !instance_id.empty();
  if (filtered) {
    json::JsonObject inst;
    if (!InstanceExists(state, winrt::to_string(instance_id), inst)) {
      return MakeError("unknown_instance",
                       "运行状态中没有该实例。实例由用户在 MaaEnd 界面创建，桥不创建实例。");
    }
    json::JsonObject instances;
    AppendInstanceSummary(instances, instance_id, inst);
    out.Insert(L"instances", instances);
  } else {
    HttpExchange init;
    init.method = "GET";
    init.path = "/api/maa/initialized";
    UpstreamOutcome ir = Upstream(ctx, init);
    if (ir.http_ok()) {
      json::JsonObject root;
      if (ParseJsonObject(ir.body, root)) {
        json::JsonObject maa;
        bool flag = false;
        if (TryGetBool(root, L"initialized", flag)) InsertBool(maa, L"initialized", flag);
        std::wstring ver;
        if (TryGetString(root, L"version", ver)) InsertStr(maa, L"version", ver);
        out.Insert(L"maa", maa);
      }
    }
    json::JsonObject instances;
    for (auto&& kv : state.instances) {
      if (kv.Value().ValueType() != json::JsonValueType::Object) continue;
      AppendInstanceSummary(instances, std::wstring(kv.Key()), kv.Value().GetObject());
    }
    out.Insert(L"instances", instances);
  }
  return out;
}

inline json::JsonObject HandleInvoke(BridgeContext& ctx, const json::JsonObject& args) {
  // 1) 参数白名单：不接受任何执行参数（pipeline/agent/命令一律由桥按声明生成）
  static const std::set<std::wstring> kAllowedKeys = {L"instanceId", L"taskName",
                                                      L"catalogHash", L"selectedTaskId"};
  for (auto&& kv : args) {
    if (!kAllowedKeys.count(std::wstring(kv.Key()))) {
      return MakeError("invalid_request", "请求包含未支持的字段；任务参数只来自已保存配置");
    }
  }
  std::wstring instance_id_w, task_name, catalog_hash_w, selected_task_id;
  if (!TryGetString(args, L"instanceId", instance_id_w) || instance_id_w.empty() ||
      !TryGetString(args, L"taskName", task_name) || task_name.empty() ||
      !TryGetString(args, L"catalogHash", catalog_hash_w)) {
    return MakeError("invalid_request", "缺少 instanceId/taskName/catalogHash");
  }
  TryGetString(args, L"selectedTaskId", selected_task_id);
  std::string instance_id = winrt::to_string(instance_id_w);
  std::string catalog_hash = winrt::to_string(catalog_hash_w);
  if (!ValidInstanceId(instance_id)) return MakeError("invalid_request", "instanceId 非法");
  if (!ValidCatalogHash(catalog_hash)) return MakeError("invalid_request", "catalogHash 非法");
  if (!selected_task_id.empty() && !ValidInstanceId(winrt::to_string(selected_task_id))) {
    return MakeError("invalid_request", "selectedTaskId 非法");
  }
  if (task_name.rfind(L"__MXU_", 0) == 0) {
    return MakeError("unsupported_task",
                     "该任务是 MaaEnd 界面内置特殊任务（由前端生成），桥只运行目录声明的任务");
  }
  // 2) 目录证据
  CatalogFetch catalog = FetchCatalog(ctx);
  if (!catalog.ok) return catalog.error;
  if (catalog_hash != catalog.hash) {
    json::JsonObject err = MakeError(
        "catalog_conflict",
        "目录已变化，请重新查询目录后再操作");
    err.Lookup(L"error").GetObject().Insert(L"currentCatalogHash",
                                            json::JsonValue::CreateStringValue(ToH(catalog.hash)));
    return err;
  }
  TaskDecl task = FindTask(catalog, task_name);
  if (!task.found) {
    return MakeError("unknown_task", "任务未在当前目录中声明，桥只运行已声明任务");
  }
  if (task.entry.empty()) {
    return MakeError("structure_drift", "任务声明缺少 entry，桥拒绝运行");
  }
  // 3) 实例就绪（实例由用户在界面准备，桥不创建、不初始化）
  StateFetch state = FetchState(ctx);
  if (!state.ok) return state.error;
  json::JsonObject inst;
  if (!InstanceExists(state, instance_id, inst)) {
    return MakeError("unknown_instance",
                     "运行状态中没有该实例。实例由用户在 MaaEnd 界面创建并连接，桥不创建实例。");
  }
  if (!InstanceBool(inst, L"connected") || !InstanceBool(inst, L"resource_loaded") ||
      !InstanceBool(inst, L"tasker_inited")) {
    return MakeError("instance_not_ready",
                     "实例未同时满足 connected/resource_loaded/tasker_inited；请在 MaaEnd 界面完成连接与资源加载，桥不代做初始化。");
  }
  if (InstanceBool(inst, L"is_running")) {
    return MakeError("instance_busy", "实例正在运行任务，先停止或等待完成");
  }
  // 4) 配置快照：运行必须遵守已保存选项，无快照即拒绝
  SnapshotFetch snap = FetchSnapshot(ctx);
  if (!snap.ok) {
    if (snap.missing) {
      return MakeError("snapshot_unsupported",
                       "上游缺少 /api/config/snapshot，桥无法按已保存选项生成任务参数，拒绝运行");
    }
    return snap.error;
  }
  json::JsonArray config_instances;
  if (!TryGetArray(snap.config, L"instances", config_instances)) {
    return MakeError("structure_drift", "配置缺少 instances 数组");
  }
  auto config_instance = FindInstanceById(config_instances, instance_id);
  if (!config_instance) {
    return MakeError("unknown_instance", "配置中没有该实例，桥只运行用户已配置的实例任务");
  }
  std::wstring controller_name, resource_name;
  TryGetString(*config_instance, L"controllerName", controller_name);
  TryGetString(*config_instance, L"resourceName", resource_name);
  // 5) 任务对当前控制器/资源可用（GUI isTaskCompatible 口径）
  if (!task.controllers.empty() &&
      (controller_name.empty() ||
       std::find(task.controllers.begin(), task.controllers.end(), controller_name) ==
           task.controllers.end())) {
    return MakeError("task_not_compatible", "任务声明不适用于该实例的当前控制器，桥拒绝运行");
  }
  if (!task.resources.empty() &&
      (resource_name.empty() ||
       std::find(task.resources.begin(), task.resources.end(), resource_name) ==
           task.resources.end())) {
    return MakeError("task_not_compatible", "任务声明不适用于该实例的当前资源，桥拒绝运行");
  }
  // 6) 定位 selectedTasks 中的既有条目：同名多条必须给 selectedTaskId，不默选第一条
  json::JsonArray selected;
  if (!TryGetArray(*config_instance, L"selectedTasks", selected)) {
    return MakeError("structure_drift", "实例缺少 selectedTasks 数组");
  }
  json::JsonObject chosen;
  std::wstring chosen_id;
  if (!selected_task_id.empty()) {
    for (auto&& v : selected) {
      if (v.ValueType() != json::JsonValueType::Object) continue;
      auto st = v.GetObject();
      std::wstring id, tn;
      if (TryGetString(st, L"id", id) && id == selected_task_id) {
        if (!TryGetString(st, L"taskName", tn) || tn != task_name) {
          return MakeError("invalid_request", "selectedTaskId 对应条目的任务名与 taskName 不一致");
        }
        chosen = st;
        chosen_id = id;
        break;
      }
    }
    if (chosen_id.empty()) {
      return MakeError("unknown_selected_task", "该实例任务列表中没有此 selectedTaskId");
    }
  } else {
    int matches = 0;
    for (auto&& v : selected) {
      if (v.ValueType() != json::JsonValueType::Object) continue;
      auto st = v.GetObject();
      std::wstring id, tn;
      if (TryGetString(st, L"taskName", tn) && tn == task_name && TryGetString(st, L"id", id)) {
        ++matches;
        chosen = st;
        chosen_id = id;
      }
    }
    if (matches == 0) {
      return MakeError("task_not_selected",
                       "该任务不在实例的任务列表中；请先在 MaaEnd 界面添加，或用 config.update 调整后重试");
    }
    if (matches > 1) {
      return MakeError("ambiguous_task",
                       "实例中有多个同名任务条目，请提供 selectedTaskId 精确指定要运行的条目");
    }
  }
  bool enabled = true;
  TryGetBool(chosen, L"enabled", enabled);
  if (!enabled) {
    return MakeError("task_disabled",
                     "该任务条目处于停用状态；如需运行请先在 MaaEnd 界面启用或用 config.update 启用");
  }
  // 7) 按已保存选项生成 pipeline_override（GUI 同口径：全局→资源→控制器→任务）
  json::JsonObject global_values;
  TryGetObject(snap.config, L"globalOptionValues", global_values);
  PipelineGenResult gen = GenerateTaskPipelineOverride(catalog, task.raw, chosen,
                                                       controller_name, resource_name,
                                                       global_values);
  if (!gen.ok) return MakeError(gen.code, gen.message);
  // 8) Agent：仅当运行宿主的 interface 声明了 agent 才走 /tasks/start（GUI 同款
  // 启动路径）；agent_configs/pi_envs 全部来自实时声明与后端状态，模型不提供。
  AgentPlan plan = ResolveAgentPlan(catalog);
  if (!plan.ok) return MakeError(plan.code, plan.message);
  json::JsonObject task_config;
  InsertStr(task_config, L"entry", task.entry);
  InsertStrUtf8(task_config, L"pipeline_override", gen.pipeline);
  if (!chosen_id.empty()) InsertStr(task_config, L"selected_task_id", chosen_id);
  InsertStr(task_config, L"task_name", task_name);
  json::JsonArray task_list;
  task_list.Append(task_config);
  HttpExchange ex;
  ex.method = "POST";
  if (plan.use_start) {
    json::JsonObject settings;
    TryGetObject(snap.config, L"settings", settings);
    bool tcp_compat = false;
    TryGetBool(settings, L"tcpCompatMode", tcp_compat);
    std::wstring language;
    TryGetString(settings, L"language", language);
    std::wstring lang_key = InterfaceLangKey(language);
    std::wstring maafw_version = FetchMaaVersion(ctx);
    json::JsonObject start_body;
    start_body.Insert(L"tasks", task_list);
    start_body.Insert(L"agent_configs", plan.configs);
    InsertBool(start_body, L"tcp_compat_mode", tcp_compat);
    start_body.Insert(L"pi_envs", BuildPiEnvs(catalog, controller_name, resource_name, lang_key,
                                              maafw_version));
    ex.path = "/api/maa/instances/" + instance_id + "/tasks/start";
    ex.body = ToUtf8(start_body.Stringify());
  } else {
    ex.path = "/api/maa/instances/" + instance_id + "/tasks/run";
    ex.body = ToUtf8(task_list.Stringify());
  }
  UpstreamOutcome r = Upstream(ctx, ex);
  if (r.network_error) {
    json::JsonObject err = MakeError(
        "outcome_unknown",
        "任务提交请求超时或响应丢失，结果未知。不要重试提交；用 maaend.http.read 查询实例状态。");
    InsertBool(err.Lookup(L"error").GetObject(), L"retriable", false);
    return err;
  }
  if (!r.http_ok()) return UpstreamFailure(r, plan.use_start ? "POST tasks/start" : "POST tasks/run");
  // 9) 2xx 回执必须符合真实契约（{"taskIds":[数字]} 且数量与提交一致）才算接纳
  json::JsonObject root;
  json::JsonArray task_ids;
  bool receipt_valid = ParseJsonObject(r.body, root) && TryGetArray(root, L"taskIds", task_ids) &&
                       task_ids.Size() == task_list.Size();
  if (receipt_valid) {
    for (auto&& v : task_ids) {
      if (v.ValueType() != json::JsonValueType::Number) {
        receipt_valid = false;
        break;
      }
    }
  }
  if (!receipt_valid) {
    json::JsonObject err = MakeError(
        "outcome_unknown",
        "上游对任务提交返回了 2xx 但回执不符合契约（缺少与提交数量一致的 taskIds 数字数组），"
        "结果未知。不要重试提交；用 maaend.http.read 查询实例状态。");
    InsertBool(err.Lookup(L"error").GetObject(), L"retriable", false);
    return err;
  }
  json::JsonObject out = MakeOk();
  InsertBool(out, L"accepted", true);
  json::JsonObject verification;
  InsertStrUtf8(verification, L"status", "succeeded");
  InsertStrUtf8(verification, L"scope", "submission");
  out.Insert(L"verification", verification);
  out.Insert(L"taskIds", task_ids);
  InsertStr(out, L"taskName", task_name);
  InsertStrUtf8(out, L"instanceId", instance_id);
  if (!chosen_id.empty()) InsertStr(out, L"selectedTaskId", chosen_id);
  InsertBool(out, L"agentStarted", plan.use_start);
  InsertStrUtf8(out, L"note",
                "接纳不等于业务成功；请用 maaend.http.read 查看 task_run_state 与 is_running 判断真实结果。"
                "任务按该实例已保存的选项配置运行（含默认值）。");
  return out;
}

inline json::JsonObject HandleTaskStop(BridgeContext& ctx, const json::JsonObject& args) {
  std::wstring instance_id_w;
  if (!TryGetString(args, L"instanceId", instance_id_w) || instance_id_w.empty()) {
    return MakeError("invalid_request", "缺少 instanceId");
  }
  std::string instance_id = winrt::to_string(instance_id_w);
  if (!ValidInstanceId(instance_id)) return MakeError("invalid_request", "instanceId 非法");
  StateFetch pre = FetchState(ctx);
  if (!pre.ok) return pre.error;
  json::JsonObject inst;
  if (!InstanceExists(pre, instance_id, inst)) {
    return MakeError("unknown_instance", "运行状态中没有该实例");
  }
  HttpExchange ex;
  ex.method = "POST";
  ex.path = "/api/maa/instances/" + instance_id + "/tasks/stop";
  ex.body = "{}";
  UpstreamOutcome r = Upstream(ctx, ex);
  if (r.network_error) {
    json::JsonObject err = MakeError(
        "outcome_unknown",
        "停止请求超时或响应丢失，结果未知。不要重试；用 maaend.http.read 查询实例状态。");
    InsertBool(err.Lookup(L"error").GetObject(), L"retriable", false);
    return err;
  }
  if (!r.http_ok()) return UpstreamFailure(r, "POST tasks/stop");
  // 回读核实：轮询 state 直到 is_running 变为 false
  bool stopped = false;
  bool verified = false;
  json::JsonObject final_inst;
  std::string poll_error;
  for (int i = 0; i < ctx.options->stop_poll_attempts; ++i) {
    if (i > 0) {
      std::this_thread::sleep_for(
          std::chrono::milliseconds(ctx.options->stop_poll_interval_ms));
    }
    StateFetch s = FetchState(ctx);
    if (!s.ok) {
      poll_error = "state_unavailable";
      continue;
    }
    json::JsonObject cur;
    if (!InstanceExists(s, instance_id, cur)) {
      poll_error = "instance_disappeared";
      break;
    }
    if (!InstanceBool(cur, L"is_running")) {
      stopped = true;
      verified = true;
      final_inst = cur;
      break;
    }
  }
  json::JsonObject out = MakeOk();
  InsertBool(out, L"accepted", true);
  InsertBool(out, L"stopped", stopped);
  InsertBool(out, L"verified", verified);
  if (!poll_error.empty()) InsertStrUtf8(out, L"verifyNote", poll_error);
  if (final_inst) {
    if (final_inst.HasKey(L"task_run_state") &&
        final_inst.Lookup(L"task_run_state").ValueType() == json::JsonValueType::Object) {
      out.Insert(L"task_run_state", final_inst.Lookup(L"task_run_state"));
    }
    InsertBool(out, L"is_running", InstanceBool(final_inst, L"is_running"));
  }
  if (!stopped) {
    InsertStrUtf8(out, L"note",
                  "停止请求已被接纳，但回读时任务仍在运行或状态不可读；请用 maaend.http.read 复查。");
  }
  return out;
}

// ---------------------------------------------------------------------------
// 配置读取（脱敏视图）
// ---------------------------------------------------------------------------

inline json::JsonObject ConfigRedactedView(const CatalogFetch& catalog, json::JsonObject const& cfg,
                                           const std::wstring& only_instance) {
  json::JsonObject view;
  std::wstring ver;
  if (TryGetString(cfg, L"version", ver)) InsertStr(view, L"version", ver);
  json::JsonArray instances;
  if (TryGetArray(cfg, L"instances", instances)) {
    json::JsonArray out_instances;
    for (auto&& v : instances) {
      if (v.ValueType() != json::JsonValueType::Object) continue;
      auto inst = v.GetObject();
      std::wstring id;
      if (!TryGetString(inst, L"id", id)) continue;
      if (!only_instance.empty() && id != only_instance) continue;
      json::JsonObject item;
      InsertStr(item, L"id", id);
      std::wstring name, controller, resource;
      TryGetString(inst, L"name", name);
      TryGetString(inst, L"controllerName", controller);
      TryGetString(inst, L"resourceName", resource);
      if (!name.empty()) InsertStr(item, L"name", name);
      if (!controller.empty()) InsertStr(item, L"controllerName", controller);
      if (!resource.empty()) InsertStr(item, L"resourceName", resource);
      json::JsonArray selected;
      if (TryGetArray(inst, L"selectedTasks", selected)) {
        json::JsonArray out_tasks;
        for (auto&& tv : selected) {
          if (tv.ValueType() != json::JsonValueType::Object) continue;
          auto st = tv.GetObject();
          std::wstring tid, tname, custom;
          if (!TryGetString(st, L"id", tid) || !TryGetString(st, L"taskName", tname)) continue;
          json::JsonObject t;
          InsertStr(t, L"id", tid);
          InsertStr(t, L"taskName", tname);
          if (TryGetString(st, L"customName", custom) && !custom.empty()) {
            InsertStr(t, L"customName", custom);
          }
          bool enabled = true, run_once = false, expanded = true;
          if (TryGetBool(st, L"enabled", enabled)) InsertBool(t, L"enabled", enabled);
          if (TryGetBool(st, L"runOnce", run_once) && run_once) InsertBool(t, L"runOnce", run_once);
          if (TryGetBool(st, L"expanded", expanded)) InsertBool(t, L"expanded", expanded);
          json::JsonObject opts;
          if (TryGetObject(st, L"optionValues", opts)) {
            json::JsonObject out_opts;
            for (auto&& kv : opts) {
              auto key = std::wstring(kv.Key());
              if (kv.Value().ValueType() != json::JsonValueType::Object) continue;
              auto val = kv.Value().GetObject();
              std::wstring type;
              if (!TryGetString(val, L"type", type)) continue;
              json::JsonObject def;
              bool declared = GetOptionDef(catalog, key, def);
              if (!declared || EffectiveOptionType(def) != type || (type != L"select" && type != L"switch" && type != L"checkbox" && type != L"input" && type != L"hotkey")) {
                // 未识别/无声明的选项：只回显类型并标记脱敏
                json::JsonObject masked;
                InsertStr(masked, L"type", type);
                InsertBool(masked, L"redacted", true);
                out_opts.Insert(key, masked);
                continue;
              }
              if (type == L"hotkey" && !CheckHotkeyValue(def, val).ok) {
                json::JsonObject masked;
                InsertStr(masked, L"type", type);
                InsertBool(masked, L"redacted", true);
                out_opts.Insert(key, masked);
              } else if (type == L"input") {
                json::JsonObject masked;
                InsertStr(masked, L"type", type);
                json::JsonObject values;
                if (TryGetObject(val, L"values", values)) {
                  json::JsonObject out_values;
                  for (auto&& ikv : values) {
                    InputFieldRules rules = FindInputRules(def, std::wstring(ikv.Key()), false);
                    if (!rules.found || rules.password) {
                      InsertStr(out_values, std::wstring(ikv.Key()), L"<redacted>");
                    } else {
                      out_values.Insert(std::wstring(ikv.Key()), ikv.Value());
                    }
                  }
                  masked.Insert(L"values", out_values);
                }
                out_opts.Insert(key, masked);
              } else {
                out_opts.Insert(key, val);  // select/switch/checkbox 原样
              }
            }
            t.Insert(L"optionValues", out_opts);
          }
          out_tasks.Append(t);
        }
        item.Insert(L"selectedTasks", out_tasks);
      }
      json::JsonArray pre;
      if (TryGetArray(inst, L"preActions", pre)) {
        InsertInt(item, L"preActionCount", pre.Size());  // 含外部程序，不回显内容
      }
      out_instances.Append(item);
    }
    view.Insert(L"instances", out_instances);
  }
  json::JsonObject settings;
  if (TryGetObject(cfg, L"settings", settings)) {
    json::JsonArray keys;
    for (auto&& kv : settings) keys.Append(json::JsonValue::CreateStringValue(std::wstring(kv.Key())));
    view.Insert(L"settingKeys", keys);  // 只回显键名，值可能含路径等
  }
  return view;
}

inline json::JsonObject HandleConfigRead(BridgeContext& ctx, const json::JsonObject& args) {
  CatalogFetch catalog = FetchCatalog(ctx);
  if (!catalog.ok) return catalog.error;
  std::wstring only_instance;
  TryGetString(args, L"instanceId", only_instance);
  SnapshotFetch snap = FetchSnapshot(ctx);
  json::JsonObject out;
  if (snap.ok) {
    out = MakeOk();
    InsertStrUtf8(out, L"catalogHash", catalog.hash);
    InsertStr(out, L"revision", snap.revision);
    InsertBool(out, L"snapshotSupported", true);
    out.Insert(L"config", ConfigRedactedView(catalog, snap.config, only_instance));
    return out;
  }
  if (snap.missing) {
    // 旧后端：允许无版本只读回退（GET /api/config），但明确告知写入将被拒绝
    HttpExchange ex;
    ex.method = "GET";
    ex.path = "/api/config";
    UpstreamOutcome r = Upstream(ctx, ex);
    if (!r.http_ok()) return UpstreamFailure(r, "GET /api/config");
    json::JsonObject cfg;
    if (!ParseJsonObject(r.body, cfg)) {
      return MakeError("bad_config_response", "/api/config 返回的不是 JSON 对象");
    }
    out = MakeOk();
    InsertStrUtf8(out, L"catalogHash", catalog.hash);
    InsertBool(out, L"snapshotSupported", false);
    InsertBool(out, L"versionedWriteAllowed", false);
    InsertStrUtf8(out, L"revision", "");
    InsertStrUtf8(out, L"warning",
                  "上游缺少 /api/config/snapshot；无版本读取仅用于查看，任何配置写入都会被拒绝。");
    out.Insert(L"config", ConfigRedactedView(catalog, cfg, only_instance));
    return out;
  }
  return snap.error;
}

// ---------------------------------------------------------------------------
// 配置有限更新
// ---------------------------------------------------------------------------

namespace detail {

inline json::JsonObject ArgsError(const char* msg) {
  return MakeError("invalid_request", msg);
}

}  // namespace detail

inline json::JsonObject HandleConfigUpdate(BridgeContext& ctx, const json::JsonObject& args) {
  // 1) 参数形状：只接受白名单键
  static const std::set<std::wstring> kAllowedKeys = {
      L"instanceId", L"catalogHash", L"baseRevision", L"taskOrder", L"taskEnabled",
      L"optionValues"};
  for (auto&& kv : args) {
    if (!kAllowedKeys.count(std::wstring(kv.Key()))) return detail::ArgsError("请求包含未支持的字段");
  }
  std::wstring instance_id_w, catalog_hash_w, base_revision;
  if (!TryGetString(args, L"instanceId", instance_id_w) ||
      !TryGetString(args, L"catalogHash", catalog_hash_w) ||
      !TryGetString(args, L"baseRevision", base_revision) || base_revision.empty() ||
      base_revision.size() > 128) {
    return detail::ArgsError("缺少 instanceId/catalogHash/baseRevision 证据");
  }
  std::string instance_id = winrt::to_string(instance_id_w);
  if (!ValidInstanceId(instance_id)) return detail::ArgsError("instanceId 非法");
  if (!ValidCatalogHash(winrt::to_string(catalog_hash_w))) {
    return detail::ArgsError("catalogHash 非法");
  }
  bool has_order = args.HasKey(L"taskOrder");
  bool has_enabled = args.HasKey(L"taskEnabled");
  bool has_options = args.HasKey(L"optionValues");
  if (!has_order && !has_enabled && !has_options) {
    return detail::ArgsError("至少提供 taskOrder/taskEnabled/optionValues 之一");
  }
  // 2) 目录证据
  CatalogFetch catalog = FetchCatalog(ctx);
  if (!catalog.ok) return catalog.error;
  if (winrt::to_string(catalog_hash_w) != catalog.hash) {
    json::JsonObject err =
        MakeError("catalog_conflict", "目录已变化，请重新查询目录与配置后再修改");
    err.Lookup(L"error").GetObject().Insert(
        L"currentCatalogHash", json::JsonValue::CreateStringValue(ToH(catalog.hash)));
    return err;
  }
  // 3) 快照 + revision 证据（严格：无 snapshot 即拒绝写，不退到无版本写）
  SnapshotFetch snap = FetchSnapshot(ctx);
  if (!snap.ok) {
    if (snap.missing) {
      return MakeError("unsupported_versioned_write",
                       "上游缺少 /api/config/snapshot，无法进行带版本写入；桥不退回无版本写。");
    }
    return snap.error;
  }
  if (base_revision != snap.revision) {
    json::JsonObject err = MakeError("revision_conflict",
                                     "配置已被其它客户端更新，请重新读取配置后再修改");
    err.Lookup(L"error").GetObject().Insert(L"currentRevision",
                                            json::JsonValue::CreateStringValue(snap.revision));
    return err;
  }
  // 4) 定位实例与 selectedTasks
  json::JsonArray instances;
  if (!TryGetArray(snap.config, L"instances", instances)) {
    return MakeError("structure_drift", "配置缺少 instances 数组");
  }
  auto base_instance = FindInstanceById(instances, instance_id);
  if (!base_instance) {
    return MakeError("unknown_instance", "配置中没有该实例；桥只修改既有实例");
  }
  json::JsonArray base_selected;
  if (!TryGetArray(*base_instance, L"selectedTasks", base_selected)) {
    return MakeError("structure_drift", "实例缺少 selectedTasks 数组");
  }
  std::vector<std::wstring> ids;
  std::map<std::wstring, json::JsonObject> task_map;
  for (auto&& v : base_selected) {
    if (v.ValueType() != json::JsonValueType::Object) {
      return MakeError("structure_drift", "selectedTasks 含非对象项");
    }
    std::wstring tid;
    if (!TryGetString(v.GetObject(), L"id", tid)) {
      return MakeError("structure_drift", "selectedTasks 项缺少 id");
    }
    ids.push_back(tid);
    task_map[tid] = v.GetObject();
  }
  MutationRecord rec;
  rec.instance_id = instance_id_w;
  std::vector<std::string> changes;
  // 5) taskOrder：必须是既有 id 的重排
  std::vector<std::wstring> new_order;
  if (has_order) {
    if (args.Lookup(L"taskOrder").ValueType() != json::JsonValueType::Array) {
      return detail::ArgsError("taskOrder 必须是字符串数组");
    }
    auto arr = args.Lookup(L"taskOrder").GetArray();
    std::set<std::wstring> given;
    for (auto&& v : arr) {
      if (v.ValueType() != json::JsonValueType::String) {
        return detail::ArgsError("taskOrder 必须是字符串数组");
      }
      auto id = std::wstring(v.GetString());
      if (id.size() > 128) return detail::ArgsError("taskOrder 项过长");
      if (!given.insert(id).second) return detail::ArgsError("taskOrder 含重复 id");
      new_order.push_back(id);
    }
    std::set<std::wstring> existing(ids.begin(), ids.end());
    if (given != existing) {
      return MakeError("invalid_task_order",
                       "taskOrder 必须是既有任务 id 的完整重排，不能新增或删除任务");
    }
    rec.new_order = new_order;
    changes.push_back("adjusted task order");
  } else {
    new_order = ids;
  }
  // 6) taskEnabled
  std::map<std::wstring, bool> enabled_map;
  if (has_enabled) {
    if (args.Lookup(L"taskEnabled").ValueType() != json::JsonValueType::Object) {
      return detail::ArgsError("taskEnabled 必须是对象");
    }
    auto obj = args.Lookup(L"taskEnabled").GetObject();
    for (auto&& kv : obj) {
      if (kv.Value().ValueType() != json::JsonValueType::Boolean) {
        return detail::ArgsError("taskEnabled 的值必须是布尔");
      }
      if (!task_map.count(std::wstring(kv.Key()))) {
        return MakeError("unknown_task", "taskEnabled 引用了不存在的任务 id");
      }
      enabled_map[std::wstring(kv.Key())] = kv.Value().GetBoolean();
    }
    if (enabled_map.empty()) return detail::ArgsError("taskEnabled 不能为空");
    rec.enabled = enabled_map;
    changes.push_back("changed task enabled flags");
  }
  // 7) optionValues
  std::map<std::wstring, std::map<std::wstring, json::JsonObject>> option_updates;
  if (has_options) {
    if (args.Lookup(L"optionValues").ValueType() != json::JsonValueType::Object) {
      return detail::ArgsError("optionValues 必须是对象");
    }
    auto outer = args.Lookup(L"optionValues").GetObject();
    for (auto&& tkv : outer) {
      if (!task_map.count(std::wstring(tkv.Key()))) {
        return MakeError("unknown_task", "optionValues 引用了不存在的任务 id");
      }
      if (tkv.Value().ValueType() != json::JsonValueType::Object) {
        return detail::ArgsError("optionValues 每项必须是对象");
      }
      auto st = task_map[std::wstring(tkv.Key())];
      std::wstring task_name;
      if (!TryGetString(st, L"taskName", task_name)) {
        return MakeError("structure_drift", "selectedTasks 项缺少 taskName");
      }
      TaskDecl decl = FindTask(catalog, task_name);
      if (!decl.found) {
        return MakeError("undeclared_task",
                         "该任务不在当前目录中（可能已升级变化），桥拒绝修改它的选项");
      }
      std::wstring controller_name, resource_name;
      TryGetString(*base_instance, L"controllerName", controller_name);
      TryGetString(*base_instance, L"resourceName", resource_name);
      auto reachable = ReachableOptionKeys(catalog, decl, controller_name, resource_name);
      json::JsonObject existing_values;
      TryGetObject(st, L"optionValues", existing_values);
      auto inner = tkv.Value().GetObject();
      for (auto&& okv : inner) {
        if (!reachable.count(std::wstring(okv.Key()))) {
          return MakeError("unknown_option",
                           "选项未声明对该任务可用（不在任务/控制器/资源/全局声明内）");
        }
        json::JsonObject def;
        if (!GetOptionDef(catalog, std::wstring(okv.Key()), def)) {
          return MakeError("structure_drift", "选项声明缺失，拒绝写入");
        }
        if (okv.Value().ValueType() != json::JsonValueType::Object) {
          return detail::ArgsError("选项值必须是对象");
        }
        OptionCheck check =
            ValidateOptionValue(def, okv.Value().GetObject(), controller_name, resource_name);
        if (!check.ok) {
          return MakeError(check.code, check.message);
        }
        // 保留原值中的 encryptedValues（凭据字段不因写入而丢失）
        json::JsonObject normalized = check.value;
        if (existing_values.HasKey(std::wstring(okv.Key())) &&
            existing_values.Lookup(std::wstring(okv.Key())).ValueType() == json::JsonValueType::Object) {
          auto old = existing_values.Lookup(std::wstring(okv.Key())).GetObject();
          if (old.HasKey(L"encryptedValues") &&
              old.Lookup(L"encryptedValues").ValueType() == json::JsonValueType::Object) {
            normalized.Insert(L"encryptedValues", old.Lookup(L"encryptedValues"));
          }
        }
        option_updates[std::wstring(tkv.Key())][std::wstring(okv.Key())] = normalized;
      }
    }
    if (option_updates.empty()) return detail::ArgsError("optionValues 不能为空");
    rec.option_values = option_updates;
    changes.push_back("updated declared option values");
  }
  // 8) 生成新配置（深拷贝后仅改允许字段）
  json::JsonObject next_config;
  if (!ParseJsonObject(ToUtf8(snap.config.Stringify()), next_config)) {
    return MakeError("internal_error", "配置深拷贝失败");
  }
  json::JsonArray next_instances;
  if (!TryGetArray(next_config, L"instances", next_instances)) {
    return MakeError("structure_drift", "新配置缺少 instances");
  }
  json::JsonObject next_instance;
  {
    auto found = FindInstanceById(next_instances, instance_id);
    if (!found) return MakeError("internal_error", "新配置中找不到目标实例");
    next_instance = *found;
  }
  json::JsonArray new_selected;
  for (auto& id : new_order) {
    auto it = task_map.find(id);
    if (it == task_map.end()) return MakeError("internal_error", "任务 id 丢失");
    json::JsonObject st;
    if (!ParseJsonObject(ToUtf8(it->second.Stringify()), st)) {
      return MakeError("internal_error", "任务深拷贝失败");
    }
    auto eit = enabled_map.find(id);
    if (eit != enabled_map.end()) {
      st.Insert(L"enabled", json::JsonValue::CreateBooleanValue(eit->second));
    }
    auto oit = option_updates.find(id);
    if (oit != option_updates.end()) {
      json::JsonObject values;
      if (!TryGetObject(st, L"optionValues", values)) {
        values = json::JsonObject();
      }
      for (auto& [key, val] : oit->second) values.Insert(key, val);
      st.Insert(L"optionValues", values);
    }
    new_selected.Append(st);
  }
  next_instance.Insert(L"selectedTasks", new_selected);
  for (uint32_t i = 0; i < next_instances.Size(); ++i) {
    auto v = next_instances.GetAt(i);
    if (v.ValueType() != json::JsonValueType::Object) continue;
    std::wstring nid;
    if (TryGetString(v.GetObject(), L"id", nid) && nid == instance_id_w) {
      next_instances.SetAt(i, next_instance);
      break;
    }
  }
  next_config.Insert(L"instances", next_instances);
  // 9) 改动范围自查：越界即中止（不发写请求）
  {
    std::string reason;
    if (!ChangesWithinScope(snap.config, next_config, rec, reason)) {
      return MakeError("internal_error",
                       std::string("改动范围校验失败，已中止：") + reason);
    }
  }
  // 10) 单次写入 POST /api/config（不重试）
  json::JsonObject save_body;
  save_body.Insert(L"config", next_config);
  InsertStr(save_body, L"revision", snap.revision);
  InsertStrUtf8(save_body, L"client_id", kClientId);
  HttpExchange save;
  save.method = "POST";
  save.path = "/api/config";
  save.body = ToUtf8(save_body.Stringify());
  UpstreamOutcome sr = Upstream(ctx, save);
  if (sr.network_error) {
    // 结果未知：不重试，尽力回读一次提供证据
    json::JsonObject err = MakeError(
        "outcome_unknown",
        "配置写入请求超时或响应丢失，结果未知。不要重试；请用 maaend.http.config.read 复查。");
    json::JsonObject detail = err.Lookup(L"error").GetObject();
    InsertBool(detail, L"retriable", false);
    SnapshotFetch probe = FetchSnapshot(ctx);
    if (probe.ok) {
      InsertStr(detail, L"evidenceRevision", probe.revision);
      InsertBool(detail, L"possiblyApplied", probe.revision != snap.revision);
    }
    return err;
  }
  if (sr.status == 409 || sr.status == 428) {
    return MakeError("revision_conflict",
                     "上游判定版本冲突或缺少版本；请重新读取配置后再修改。");
  }
  if (sr.status == 400) {
    return MakeError("upstream_rejected",
                     std::string("上游拒绝新配置：") + Excerpt(sr.body));
  }
  if (!sr.http_ok()) return UpstreamFailure(sr, "POST /api/config");
  json::JsonObject save_root;
  std::wstring new_revision;
  if (!ParseJsonObject(sr.body, save_root) ||
      !TryGetString(save_root, L"revision", new_revision) || new_revision.empty()) {
    return MakeError("bad_save_response", "上游保存响应缺少 revision");
  }
  // 11) 回读核实
  bool verified = false;
  SnapshotFetch back = FetchSnapshot(ctx);
  if (back.ok && back.revision == new_revision) {
    std::string reason;
    verified = ChangesWithinScope(snap.config, back.config, rec, reason);
  }
  json::JsonObject out = MakeOk();
  InsertBool(out, L"applied", true);
  InsertBool(out, L"verified", verified);
  InsertStr(out, L"revision", new_revision);
  json::JsonArray ch;
  for (auto& c : changes) ch.Append(json::JsonValue::CreateStringValue(ToH(c)));
  out.Insert(L"changes", ch);
  if (!verified) {
    InsertStrUtf8(out, L"note", "已写入但回读核实未通过；请用 maaend.http.config.read 复查。");
  } else {
    InsertStrUtf8(out, L"note",
                  "改动只影响该实例任务列表的顺序、启停与已声明选项；对正在运行的任务不生效。");
  }
  return out;
}

// ---------------------------------------------------------------------------
// 分发
// ---------------------------------------------------------------------------

inline json::JsonObject Dispatch(BridgeContext& ctx, std::string_view path,
                                 const json::JsonObject& args) {
  try {
    if (path == "/health") return HandleHealth(ctx, args);
    if (path == "/status") return HandleStatus(ctx, args);
    if (path == "/catalog/search") return HandleCatalogSearch(ctx, args);
    if (path == "/catalog/describe") return HandleCatalogDescribe(ctx, args);
    if (path == "/read") return HandleRead(ctx, args);
    if (path == "/invoke") return HandleInvoke(ctx, args);
    if (path == "/task/stop") return HandleTaskStop(ctx, args);
    if (path == "/config/read") return HandleConfigRead(ctx, args);
    if (path == "/config/update") return HandleConfigUpdate(ctx, args);
    return MakeError("unknown_route", "未知路径");
  } catch (const winrt::hresult_error& e) {
    return MakeError("internal_error", ToUtf8(e.message()));
  } catch (const std::exception& e) {
    return MakeError("internal_error", e.what());
  } catch (...) {
    return MakeError("internal_error", "未分类异常");
  }
}

// 序列化并施加响应体上限；超限时返回错误负载。
inline std::string SerializeResponse(json::JsonObject const& response) {
  std::string body = Stringify(response);
  if (body.size() > kMaxResponseBytes) {
    return Stringify(MakeError("response_too_large", "响应超出桥的体积上限"));
  }
  return body;
}

}  // namespace maaend
