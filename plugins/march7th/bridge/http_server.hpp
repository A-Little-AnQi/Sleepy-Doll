// 三月七原生桥：回环 WinSock HTTP 传输层。
// 只接受 127.0.0.1 绑定；连接读写均带超时；解析后交给纯逻辑层 Bridge。
#pragma once

#include "core.hpp"

#include <atomic>
#include <cstdint>
#include <functional>

namespace sdm::march7th {

struct HttpServerHandle {
    void* thread = nullptr; // HANDLE
    int32_t boundPort = 0;
    // 服务线程（含 /shutdown 与控制面 STOP 两条退出路径）退出时在其自身
    // 线程上回调，用于复位桥生命周期状态；回调内不得再触碰本句柄的线程字段。
    std::function<void()> onStopped;
};

// 启动服务线程；线程内轮询 Bridge::ShutdownRequested() 决定退出。
// 失败返回 false 并给出消息；成功时 handle->boundPort 为实际端口。
bool StartHttpServer(Bridge& bridge, uint16_t port, HttpServerHandle& handle,
                     std::string& error);

// 等待服务线程结束并释放资源（进程内优雅退出用）。
void JoinHttpServer(HttpServerHandle& handle);

} // namespace sdm::march7th
