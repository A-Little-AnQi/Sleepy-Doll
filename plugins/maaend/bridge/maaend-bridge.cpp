// maaend-bridge.cpp - MaaEnd 独立原生桥服务入口
//
// WinSock 只绑定 127.0.0.1，向宿主暴露固定白名单的桥端点；所有对 MaaEnd 的
// 访问经 WinHttpTransport 走官方本机 HTTP API。本程序不启动、不注入、不加载
// 任何目标软件；实例与控制器由用户在 MaaEnd 界面自行准备。

#ifndef NOMINMAX
#define NOMINMAX
#endif
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif

#include <winsock2.h>
#include <ws2tcpip.h>

#include "winhttp_transport.h"

#include <atomic>
#include <cstdio>
#include <cstring>
#include <mutex>
#include <string>
#include <thread>

#pragma comment(lib, "ws2_32.lib")

namespace {

using maaend::BridgeContext;
using maaend::BridgeOptions;

BridgeOptions g_options;
maaend::WinHttpTransport* g_transport = nullptr;
std::atomic<int> g_active_connections{0};
// 写类端点串行化：invoke / task/stop / config/update 一次只有一个在飞。
std::mutex g_write_mutex;

bool IsWriteRoute(const std::string& path) {
  return path == "/invoke" || path == "/task/stop" || path == "/config/update";
}

std::string ReadTokenFromEnv() {
  char buffer[512];
  DWORD n = GetEnvironmentVariableA(g_options.token_env.c_str(), buffer, sizeof(buffer));
  if (n == 0 || n >= sizeof(buffer)) return {};
  return std::string(buffer, n);
}

// 每连接处理：一次完整的接收、校验、分发与应答，随后关闭连接。
void SendAll(SOCKET sock, const std::string& data) {
  size_t sent = 0;
  while (sent < data.size()) {
    int n = ::send(sock, data.data() + sent, (int)(data.size() - sent), 0);
    if (n <= 0) return;
    sent += (size_t)n;
  }
}

void SendJsonResponse(SOCKET sock, int status, const std::string& body) {
  const char* reason = status == 200 ? "OK"
                       : status == 400 ? "Bad Request"
                       : status == 401 ? "Unauthorized"
                       : status == 403 ? "Forbidden"
                       : status == 404 ? "Not Found"
                       : status == 405 ? "Method Not Allowed"
                       : status == 413 ? "Payload Too Large"
                       : status == 500 ? "Internal Server Error"
                       : status == 503 ? "Service Unavailable"
                                       : "Error";
  std::string head = "HTTP/1.1 " + std::to_string(status) + " " + reason +
                     "\r\nContent-Type: application/json; charset=utf-8\r\n"
                     "Content-Length: " +
                     std::to_string(body.size()) +
                     "\r\nConnection: close\r\nCache-Control: no-store\r\n\r\n";
  SendAll(sock, head + body);
}

std::string Trim(std::string_view s) {
  size_t begin = 0, end = s.size();
  while (begin < end && (s[begin] == ' ' || s[begin] == '\t')) ++begin;
  while (end > begin && (s[end - 1] == ' ' || s[end - 1] == '\t')) --end;
  return std::string(s.substr(begin, end - begin));
}

struct ParsedRequest {
  std::string method;
  std::string path;
  std::map<std::string, std::string> headers;  // 键为小写
  std::string body;
  bool ok = false;
};

bool RecvExact(SOCKET sock, std::string& out, size_t want) {
  out.clear();
  out.reserve(want);
  char buffer[16384];
  while (out.size() < want) {
    size_t chunk = want - out.size();
    if (chunk > sizeof(buffer)) chunk = sizeof(buffer);
    int n = ::recv(sock, buffer, (int)chunk, 0);
    if (n <= 0) return false;
    out.append(buffer, (size_t)n);
  }
  return true;
}

ParsedRequest ReadRequest(SOCKET sock) {
  ParsedRequest req;
  std::string raw;
  raw.reserve(2048);
  char buffer[4096];
  size_t header_end = std::string::npos;
  while (raw.size() < maaend::kMaxHeaderBytes + maaend::kMaxInboundBody) {
    int n = ::recv(sock, buffer, sizeof(buffer), 0);
    if (n <= 0) return req;
    raw.append(buffer, (size_t)n);
    header_end = raw.find("\r\n\r\n");
    if (header_end != std::string::npos) break;
  }
  if (header_end == std::string::npos) return req;
  std::string head = raw.substr(0, header_end);
  size_t body_received = raw.size() - (header_end + 4);

  // 请求行
  size_t line_end = head.find("\r\n");
  std::string request_line = line_end == std::string::npos ? head : head.substr(0, line_end);
  {
    size_t sp1 = request_line.find(' ');
    size_t sp2 = sp1 == std::string::npos ? std::string::npos
                                          : request_line.find(' ', sp1 + 1);
    if (sp1 == std::string::npos || sp2 == std::string::npos) return req;
    req.method = Trim(request_line.substr(0, sp1));
    req.path = Trim(request_line.substr(sp1 + 1, sp2 - sp1 - 1));
  }
  // 头（小写键）
  size_t pos = line_end == std::string::npos ? head.size() : line_end + 2;
  while (pos < head.size()) {
    size_t next = head.find("\r\n", pos);
    if (next == std::string::npos) next = head.size();
    std::string line = head.substr(pos, next - pos);
    pos = next + 2;
    size_t colon = line.find(':');
    if (colon == std::string::npos) continue;
    std::string key = maaend::LowerAscii(Trim(line.substr(0, colon)));
    req.headers[key] = Trim(line.substr(colon + 1));
  }
  if (req.headers.count("transfer-encoding")) return req;  // 不支持分块
  size_t content_length = 0;
  auto it = req.headers.find("content-length");
  if (it != req.headers.end()) {
    try {
      content_length = (size_t)std::stoull(it->second);
    } catch (...) {
      return req;
    }
  }
  if (content_length > maaend::kMaxInboundBody) {
    req.method.clear();
    req.path = "__too_large__";
    req.ok = false;
    return req;
  }
  if (content_length > 0) {
    if (body_received >= content_length) {
      req.body = raw.substr(header_end + 4, content_length);
    } else {
      std::string rest;
      if (!RecvExact(sock, rest, content_length - body_received)) return req;
      req.body = raw.substr(header_end + 4) + rest;
      if (req.body.size() > content_length) req.body.resize(content_length);
    }
  }
  req.ok = true;
  return req;
}

void HandleConnection(SOCKET sock) {
  ParsedRequest req = ReadRequest(sock);
  std::string token = ReadTokenFromEnv();
  auto header_or_empty = [&](const char* name) -> std::string {
    auto it = req.headers.find(name);
    return it == req.headers.end() ? std::string() : it->second;
  };
  if (req.path == "__too_large__") {
    SendJsonResponse(sock, 413, maaend::Stringify(maaend::MakeError(
                                   "request_too_large", "请求体超出上限")));
    ::closesocket(sock);
    return;
  }
  if (!req.ok) {
    ::closesocket(sock);
    return;
  }
  maaend::InboundDecision decision =
      maaend::CheckInbound(req.method, req.path, header_or_empty("host"),
                           req.headers.count("origin") > 0, header_or_empty("x-sleepy-token"),
                           !token.empty(), token, req.body.size());
  if (!decision.allowed) {
    SendJsonResponse(sock, decision.http_status,
                     maaend::Stringify(maaend::MakeError(decision.code, decision.message)));
    ::closesocket(sock);
    return;
  }
  // POST 端点要求 JSON 对象请求体
  maaend::json::JsonObject args;
  if (!req.body.empty() || req.method == "POST") {
    if (req.body.empty()) req.body = "{}";
    if (!maaend::ParseJsonObject(req.body, args)) {
      SendJsonResponse(sock, 400, maaend::Stringify(maaend::MakeError(
                                     "invalid_json", "请求体必须是 JSON 对象")));
      ::closesocket(sock);
      return;
    }
  }
  BridgeContext ctx{g_transport, &g_options};
  maaend::json::JsonObject response;
  if (IsWriteRoute(req.path)) {
    std::lock_guard<std::mutex> lock(g_write_mutex);
    response = maaend::Dispatch(ctx, req.path, args);
  } else {
    response = maaend::Dispatch(ctx, req.path, args);
  }
  SendJsonResponse(sock, 200, maaend::SerializeResponse(response));
  ::closesocket(sock);
}

int RunServer() {
  WSADATA wsa{};
  if (WSAStartup(MAKEWORD(2, 2), &wsa) != 0) {
    std::fprintf(stderr, "maaend-bridge: WSAStartup failed\n");
    return 1;
  }
  SOCKET listener = socket(AF_INET, SOCK_STREAM, IPPROTO_TCP);
  if (listener == INVALID_SOCKET) {
    std::fprintf(stderr, "maaend-bridge: socket failed (%d)\n", WSAGetLastError());
    WSACleanup();
    return 1;
  }
  sockaddr_in addr{};
  addr.sin_family = AF_INET;
  addr.sin_port = htons((u_short)g_options.port);
  if (inet_pton(AF_INET, "127.0.0.1", &addr.sin_addr) != 1) {
    std::fprintf(stderr, "maaend-bridge: invalid bind address\n");
    ::closesocket(listener);
    WSACleanup();
    return 1;
  }
  if (bind(listener, reinterpret_cast<sockaddr*>(&addr), sizeof(addr)) == SOCKET_ERROR) {
    std::fprintf(stderr, "maaend-bridge: bind 127.0.0.1:%d failed (%d)\n", g_options.port,
                 WSAGetLastError());
    ::closesocket(listener);
    WSACleanup();
    return 1;
  }
  if (listen(listener, 16) == SOCKET_ERROR) {
    std::fprintf(stderr, "maaend-bridge: listen failed (%d)\n", WSAGetLastError());
    ::closesocket(listener);
    WSACleanup();
    return 1;
  }
  std::fprintf(stderr, "maaend-bridge: listening on 127.0.0.1:%d, upstream 127.0.0.1:%d\n",
               g_options.port, g_options.upstream_port);
  std::fprintf(stderr, "maaend-bridge: token env '%s'%s\n", g_options.token_env.c_str(),
               ReadTokenFromEnv().empty() ? " (NOT SET - all requests will be refused)" : "");

  for (;;) {
    SOCKET client = accept(listener, nullptr, nullptr);
    if (client == INVALID_SOCKET) continue;
    if (g_active_connections.load() >= maaend::kMaxConcurrentConnections) {
      SendJsonResponse(client, 503,
                       maaend::Stringify(maaend::MakeError("busy", "并发连接过多，请稍后重试")));
      ::closesocket(client);
      continue;
    }
    g_active_connections.fetch_add(1);
    std::thread([client]() {
      bool apartment = false;
      try {
        winrt::init_apartment(winrt::apartment_type::multi_threaded);
        apartment = true;
        HandleConnection(client);
      } catch (...) {
        SendJsonResponse(client, 500,
            "{\"ok\":false,\"error\":{\"code\":\"bridge_internal_error\",\"message\":\"桥处理失败，结果未确认\"}}");
        ::closesocket(client);
      }
      if (apartment) winrt::uninit_apartment();
      g_active_connections.fetch_sub(1);
    }).detach();
  }
}

}  // namespace

int main(int argc, char** argv) {
  auto options = maaend::ParseOptions(argc, argv);
  if (!options) {
    std::fputs(maaend::UsageText().c_str(), stderr);
    return 2;
  }
  g_options = *options;
  winrt::init_apartment(winrt::apartment_type::multi_threaded);
  maaend::WinHttpTransport transport(g_options);
  g_transport = &transport;
  return RunServer();
}
