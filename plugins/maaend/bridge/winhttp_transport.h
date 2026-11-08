// winhttp_transport.h - MaaEnd 桥的 WinHTTP 传输实现
//
// 只允许 http + 127.0.0.1 + 启动时指定的固定端口；关闭系统代理与自动重定向；
// 每次请求独立建连、完整读取，响应体与各阶段超时受硬上限约束。
// 请求路径由核心层的固定白名单产生，本层不接受来自请求的 URL。

#pragma once

#include "maaend_bridge_core.h"

#include <winhttp.h>

#pragma comment(lib, "winhttp.lib")

namespace maaend {

class WinHttpTransport final : public ITransport {
 public:
  explicit WinHttpTransport(const BridgeOptions& options)
      : port_(options.upstream_port), timeout_ms_(options.upstream_receive_timeout_ms) {}

  HttpReply Request(const HttpExchange& exchange) override {
    HttpReply reply;
    // 防御性再校验：上游目标固定为 http://127.0.0.1:<port>，其余一律拒绝。
    if (!AllowedUpstreamTarget("http", "127.0.0.1") || port_ <= 0 || port_ > 65535) {
      reply.network_error = true;
      reply.network_reason = "forbidden_target";
      return reply;
    }
    // 核心层路径来自固定白名单；实例段已被字符白名单限制，无 URL 注入面。
    if (exchange.path.empty() || exchange.path[0] != '/' ||
        exchange.path.find("..") != std::string::npos ||
        exchange.path.find_first_of("?#") != std::string::npos ||
        (exchange.method != "GET" && exchange.method != "POST")) {
      reply.network_error = true;
      reply.network_reason = "forbidden_request";
      return reply;
    }

    HINTERNET session = nullptr;
    HINTERNET connect = nullptr;
    HINTERNET request = nullptr;
    auto close_all = [&]() {
      if (request) WinHttpCloseHandle(request);
      if (connect) WinHttpCloseHandle(connect);
      if (session) WinHttpCloseHandle(session);
    };

    // 每请求独立会话：无跨线程共享句柄，天然线程安全。
    session = WinHttpOpen(L"maaend-bridge/0.1", WINHTTP_ACCESS_TYPE_NO_PROXY,
                          WINHTTP_NO_PROXY_NAME, WINHTTP_NO_PROXY_BYPASS, 0);
    if (!session) {
      reply.network_error = true;
      reply.network_reason = NetworkReasonFromLastError();
      return reply;
    }
    WinHttpSetTimeouts(session, 2000, 2000, 5000, timeout_ms_);

    connect = WinHttpConnect(session, L"127.0.0.1", static_cast<INTERNET_PORT>(port_), 0);
    if (!connect) {
      reply.network_error = true;
      reply.network_reason = NetworkReasonFromLastError();
      close_all();
      return reply;
    }

    const wchar_t* verb = exchange.method == "GET" ? L"GET" : L"POST";
    request = WinHttpOpenRequest(connect, verb, ToH(exchange.path).c_str(), nullptr,
                                 WINHTTP_NO_REFERER, nullptr, 0);
    if (!request) {
      reply.network_error = true;
      reply.network_reason = NetworkReasonFromLastError();
      close_all();
      return reply;
    }
    // 不跟随重定向：上游若返回 3xx，把它作为协议错误交回核心层。
    DWORD disabled = WINHTTP_DISABLE_REDIRECTS;
    WinHttpSetOption(request, WINHTTP_OPTION_DISABLE_FEATURE, &disabled, sizeof(disabled));

    const void* body_ptr = exchange.body.empty() ? nullptr : exchange.body.data();
    DWORD body_len = static_cast<DWORD>(exchange.body.size());
    BOOL sent = WinHttpSendRequest(
        request, L"Content-Type: application/json\r\nAccept: application/json\r\n", -1,
        const_cast<void*>(body_ptr), body_len, body_len, 0);
    if (sent && !WinHttpReceiveResponse(request, nullptr)) {
      sent = FALSE;
    }
    if (!sent) {
      reply.network_error = true;
      reply.network_reason = NetworkReasonFromLastError();
      close_all();
      return reply;
    }

    DWORD status_code = 0;
    DWORD size = sizeof(status_code);
    if (!WinHttpQueryHeaders(request, WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
                             WINHTTP_HEADER_NAME_BY_INDEX, &status_code, &size, WINHTTP_NO_HEADER_INDEX)) {
      reply.network_error = true;
      reply.network_reason = "bad_response";
      close_all();
      return reply;
    }
    reply.status = static_cast<int>(status_code);

    // 已知 Content-Length 先做上限拦截；无长度则按流式上限读取。
    DWORD content_length = 0;
    size = sizeof(content_length);
    if (WinHttpQueryHeaders(request, WINHTTP_QUERY_CONTENT_LENGTH | WINHTTP_QUERY_FLAG_NUMBER,
                            nullptr, &content_length, &size, nullptr)) {
      if (content_length > kMaxUpstreamBytes) {
        reply.network_error = true;
        reply.network_reason = "too_large";
        close_all();
        return reply;
      }
    }

    std::string body;
    char buffer[65536];
    for (;;) {
      DWORD available = 0;
      if (!WinHttpQueryDataAvailable(request, &available)) {
        reply.network_error = true;
        reply.network_reason = NetworkReasonFromLastError();
        reply.body.clear();
        close_all();
        return reply;
      }
      if (available == 0) break;
      DWORD to_read = available < sizeof(buffer) ? available : (DWORD)sizeof(buffer);
      DWORD read_bytes = 0;
      if (!WinHttpReadData(request, buffer, to_read, &read_bytes)) {
        reply.network_error = true;
        reply.network_reason = NetworkReasonFromLastError();
        reply.body.clear();
        close_all();
        return reply;
      }
      if (read_bytes == 0) break;
      if (body.size() + read_bytes > kMaxUpstreamBytes) {
        reply.network_error = true;
        reply.network_reason = "too_large";
        reply.body.clear();
        close_all();
        return reply;
      }
      body.append(buffer, read_bytes);
    }
    reply.body = std::move(body);
    close_all();
    return reply;
  }

 private:
  static std::string NetworkReasonFromLastError() {
    DWORD err = GetLastError();
    switch (err) {
      case ERROR_WINHTTP_TIMEOUT:
        return "timeout";
      case ERROR_WINHTTP_CANNOT_CONNECT:
      case ERROR_WINHTTP_CONNECTION_ERROR:
        return "unreachable";
      default:
        return "winhttp_error_" + std::to_string(err);
    }
  }

  int port_ = 12701;
  int timeout_ms_ = 10000;
};

}  // namespace maaend
