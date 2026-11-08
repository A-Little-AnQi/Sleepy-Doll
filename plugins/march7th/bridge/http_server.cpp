// 三月七原生桥：回环 WinSock HTTP 传输层实现。
#include "http_server.hpp"

#include <winsock2.h>
#include <ws2tcpip.h>

#include <chrono>
#include <cstring>
#include <thread>

#pragma comment(lib, "ws2_32.lib")

namespace sdm::march7th {

namespace {

constexpr size_t kMaxHeaderBytes = 8 * 1024;
constexpr uint32_t kIoTimeoutMs = 5000;
constexpr uint32_t kRequestDeadlineMs = 10000;

struct ServerContext {
    Bridge* bridge = nullptr;
    SOCKET listenSocket = INVALID_SOCKET;
    std::function<void()> onStopped;
};

void SendSimpleResponse(SOCKET socket, int status, const std::string& body) {
    const char* reason = "OK";
    switch (status) {
        case 200: reason = "OK"; break;
        case 400: reason = "Bad Request"; break;
        case 401: reason = "Unauthorized"; break;
        case 403: reason = "Forbidden"; break;
        case 404: reason = "Not Found"; break;
        case 405: reason = "Method Not Allowed"; break;
        case 411: reason = "Length Required"; break;
        case 413: reason = "Payload Too Large"; break;
        case 429: reason = "Too Many Requests"; break;
        case 500: reason = "Internal Server Error"; break;
        case 503: reason = "Service Unavailable"; break;
        default: reason = "Error"; break;
    }
    std::string head = "HTTP/1.1 " + std::to_string(status) + " " + reason + "\r\n";
    head += "Content-Type: application/json; charset=utf-8\r\n";
    head += "Content-Length: " + std::to_string(body.size()) + "\r\n";
    head += "Cache-Control: no-store\r\n";
    head += "Connection: close\r\n\r\n";
    int headResult = ::send(socket, head.data(), int(head.size()), 0);
    int bodyResult = ::send(socket, body.data(), int(body.size()), 0);
    (void)headResult;
    (void)bodyResult;
}

bool RecvSome(SOCKET socket, std::string& buffer, size_t cap) {
    char chunk[2048];
    int got = ::recv(socket, chunk, int(sizeof(chunk)), 0);
    if (got <= 0) {
        return false;
    }
    if (buffer.size() + size_t(got) > cap) {
        return false;
    }
    buffer.append(chunk, size_t(got));
    return true;
}

bool SendAll(SOCKET socket, const char* data, size_t length) {
    size_t sent = 0;
    while (sent < length) {
        int chunk = ::send(socket, data + sent, int(std::min<size_t>(length - sent, 32768)), 0);
        if (chunk <= 0) {
            return false;
        }
        sent += size_t(chunk);
    }
    return true;
}

void HandleConnection(Bridge& bridge, SOCKET socket) {
    auto deadline = std::chrono::steady_clock::now() +
                    std::chrono::milliseconds(kRequestDeadlineMs);

    DWORD receiveTimeout = kIoTimeoutMs;
    DWORD sendTimeout = kIoTimeoutMs;
    ::setsockopt(socket, SOL_SOCKET, SO_RCVTIMEO, (const char*)&receiveTimeout, sizeof(receiveTimeout));
    ::setsockopt(socket, SOL_SOCKET, SO_SNDTIMEO, (const char*)&sendTimeout, sizeof(sendTimeout));

    std::string raw;
    size_t headerEnd = std::string::npos;
    while (headerEnd == std::string::npos) {
        if (!RecvSome(socket, raw, kMaxHeaderBytes + kMaxBodyBytes + 16)) {
            return;
        }
        headerEnd = raw.find("\r\n\r\n");
        if (raw.size() > kMaxHeaderBytes && headerEnd == std::string::npos) {
            SendSimpleResponse(socket, 431, "{\"error\":{\"code\":\"headers_too_large\",\"message\":\"请求头过大\"}}");
            return;
        }
        if (std::chrono::steady_clock::now() > deadline) {
            return;
        }
    }

    std::string headerBlock = raw.substr(0, headerEnd);
    std::string rest = raw.substr(headerEnd + 4);

    size_t lineEnd = headerBlock.find("\r\n");
    std::string requestLine = lineEnd == std::string::npos ? headerBlock : headerBlock.substr(0, lineEnd);
    HttpRequest request;
    {
        size_t space1 = requestLine.find(' ');
        size_t space2 = space1 == std::string::npos ? std::string::npos
                                                    : requestLine.find(' ', space1 + 1);
        if (space1 == std::string::npos || space2 == std::string::npos) {
            SendSimpleResponse(socket, 400, "{\"error\":{\"code\":\"bad_request\",\"message\":\"请求行无效\"}}");
            return;
        }
        request.method = requestLine.substr(0, space1);
        request.rawPath = requestLine.substr(space1 + 1, space2 - space1 - 1);
        for (char& c : request.method) {
            if (c >= 'a' && c <= 'z') {
                c = char(c - 'a' + 'A');
            }
        }
    }

    size_t cursor = lineEnd == std::string::npos ? headerBlock.size() : lineEnd + 2;
    while (cursor < headerBlock.size()) {
        size_t eol = headerBlock.find("\r\n", cursor);
        if (eol == std::string::npos) {
            eol = headerBlock.size();
        }
        std::string line = headerBlock.substr(cursor, eol - cursor);
        cursor = eol + 2;
        size_t colon = line.find(':');
        if (colon == std::string::npos) {
            continue;
        }
        std::string name = line.substr(0, colon);
        size_t valueStart = colon + 1;
        while (valueStart < line.size() && (line[valueStart] == ' ' || line[valueStart] == '\t')) {
            ++valueStart;
        }
        std::string value = line.substr(valueStart);
        for (char& c : name) {
            if (c >= 'A' && c <= 'Z') {
                c = char(c - 'A' + 'a');
            }
        }
        if (name == "content-length") {
            try {
                request.declaredContentLength = std::stoll(value);
            } catch (...) {
                request.declaredContentLength = -2; // 非法
            }
        } else {
            request.headers[name] = value;
        }
    }
    if (request.declaredContentLength == -2) {
        SendSimpleResponse(socket, 400, "{\"error\":{\"code\":\"bad_request\",\"message\":\"Content-Length 无效\"}}");
        return;
    }
    if (request.declaredContentLength > int64_t(kMaxBodyBytes)) {
        // 不读请求体，直接拒绝并关闭连接。
        SendSimpleResponse(socket, 413,
                           "{\"error\":{\"code\":\"payload_too_large\",\"message\":\"请求体超过上限\"}}");
        return;
    }
    request.body = std::move(rest);
    if (request.declaredContentLength >= 0) {
        while (request.body.size() < size_t(request.declaredContentLength)) {
            if (!RecvSome(socket, request.body, kMaxBodyBytes + 16)) {
                return;
            }
            if (std::chrono::steady_clock::now() > deadline) {
                return;
            }
        }
    }

    HttpResponse response = bridge.Handle(request);
    std::string head = "HTTP/1.1 " + std::to_string(response.status) + " OK\r\n";
    if (response.status != 200) {
        // 错误状态统一给对应 reason；简化处理保留 OK 之外的基础映射。
        head = "HTTP/1.1 " + std::to_string(response.status) + " Error\r\n";
    }
    head += "Content-Type: application/json; charset=utf-8\r\n";
    head += "Content-Length: " + std::to_string(response.body.size()) + "\r\n";
    head += "Cache-Control: no-store\r\n";
    head += "Connection: close\r\n\r\n";
    SendAll(socket, head.data(), head.size());
    SendAll(socket, response.body.data(), response.body.size());
}

DWORD WINAPI ServerThread(void* parameter) {
    ServerContext* context = static_cast<ServerContext*>(parameter);
    std::function<void()> onStopped = context->onStopped;
    winrt::init_apartment(winrt::apartment_type::multi_threaded);
    // 简单限流：滑动 1 秒窗口内最多处理 30 个连接，超出直接 429。
    DWORD windowStart = ::GetTickCount();
    int windowCount = 0;
    while (!context->bridge->ShutdownRequested()) {
        fd_set readSet;
        FD_ZERO(&readSet);
        FD_SET(context->listenSocket, &readSet);
        timeval timeout{0, 200 * 1000};
        int ready = ::select(0, &readSet, nullptr, nullptr, &timeout);
        if (ready <= 0) {
            continue;
        }
        SOCKET client = ::accept(context->listenSocket, nullptr, nullptr);
        if (client == INVALID_SOCKET) {
            continue;
        }
        DWORD now = ::GetTickCount();
        if (now - windowStart >= 1000) {
            windowStart = now;
            windowCount = 0;
        }
        if (++windowCount > 30) {
            SendSimpleResponse(
                client, 429,
                "{\"error\":{\"code\":\"rate_limited\",\"message\":\"请求过于频繁，稍后再试\"}}");
            ::shutdown(client, SD_BOTH);
            ::closesocket(client);
            continue;
        }
        HandleConnection(*context->bridge, client);
        ::shutdown(client, SD_BOTH);
        ::closesocket(client);
    }
    ::closesocket(context->listenSocket);
    delete context;
    // 在本服务线程上通知生命周期复位；回调异常不得阻止线程退出。
    if (onStopped) {
        try {
            onStopped();
        } catch (...) {
        }
    }
    return 0;
}

} // namespace

bool StartHttpServer(Bridge& bridge, uint16_t port, HttpServerHandle& handle, std::string& error) {
    WSADATA wsaData{};
    if (::WSAStartup(MAKEWORD(2, 2), &wsaData) != 0) {
        error = "WSAStartup 失败";
        return false;
    }
    SOCKET listenSocket = ::socket(AF_INET, SOCK_STREAM, IPPROTO_TCP);
    if (listenSocket == INVALID_SOCKET) {
        error = "创建监听套接字失败";
        return false;
    }
    sockaddr_in address{};
    address.sin_family = AF_INET;
    address.sin_port = htons(port);
    if (::inet_pton(AF_INET, "127.0.0.1", &address.sin_addr) != 1) {
        ::closesocket(listenSocket);
        error = "回环地址解析失败";
        return false;
    }
    if (::bind(listenSocket, (sockaddr*)&address, sizeof(address)) == SOCKET_ERROR ||
        ::listen(listenSocket, 4) == SOCKET_ERROR) {
        int code = ::WSAGetLastError();
        ::closesocket(listenSocket);
        error = "绑定 127.0.0.1:" + std::to_string(port) + " 失败（WinSock 错误 " +
                std::to_string(code) + "；端口可能被占用）";
        return false;
    }
    sockaddr_in bound{};
    int boundSize = sizeof(bound);
    if (::getsockname(listenSocket, (sockaddr*)&bound, &boundSize) == SOCKET_ERROR) {
        ::closesocket(listenSocket);
        error = "查询绑定端口失败";
        return false;
    }
    auto* context = new ServerContext;
    context->bridge = &bridge;
    context->listenSocket = listenSocket;
    context->onStopped = handle.onStopped;
    HANDLE thread = ::CreateThread(nullptr, 0, &ServerThread, context, 0, nullptr);
    if (thread == nullptr) {
        ::closesocket(listenSocket);
        delete context;
        error = "创建服务线程失败";
        return false;
    }
    handle.thread = thread;
    handle.boundPort = int32_t(ntohs(bound.sin_port));
    return true;
}

void JoinHttpServer(HttpServerHandle& handle) {
    if (handle.thread != nullptr) {
        ::WaitForSingleObject(handle.thread, 15000);
        ::CloseHandle(handle.thread);
        handle.thread = nullptr;
    }
}

} // namespace sdm::march7th
