// zzz_dll.cpp - 桥 DLL 入口：DllMain 只禁用线程通知并拉起控制面 worker，
// 立即返回；本模块不导出任何供远程线程调用的函数。
//
// 注入方式（与 bgi-bridge/native/injector.cpp 一致）：注入器只做一次
// CreateRemoteThread(LoadLibraryW)。DllMain 是加载器调用的，不受 CFG 对
// "远程线程指向刚映射模块"的限制；真正的初始化全部发生在 worker 线程，
// 且保证在加载器锁释放之后才开始执行（新线程的启动需要 loader lock）。
//
// worker（loader lock 外）：
//   1. pin 本模块（迟到 pending 回调绝不跳进已卸载代码）。
//   2. 创建命名管道 \\.\pipe\SleepyDoll.ZzzBridge.<当前PID>：
//      PIPE_REJECT_REMOTE_CLIENTS + 仅当前用户 DACL。
//   3. 循环等待注入器连接：收 ZzzControlRequest，按命令调用
//      StartBridgeCore / StopBridgeCore，回执真实结果码。
//      桥停止后 worker 仍在，支持再次启动（HTTP 停桥 / 管道再启动共存）。
//
// 线程纪律：worker 用 Win32 线程、全程 try/catch（不跨 native 边界抛出）；
// 不保留任何 joinable std::thread 全局；进程退出由 OS 回收，不做清理回调。
// 不碰 GIL、不碰 JSON、不做文件 IO；令牌只存内存。

#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>

#include <sddl.h>

#include <string>

#include "zzz_bridge_abi.h"
#include "control_ipc.hpp"

#pragma comment(lib, "advapi32.lib")

uint32_t StartBridgeCore(const ZzzBridgeConfig& config);
uint32_t StopBridgeCore();

namespace {

// 仅当前用户的 DACL：本地同用户进程可连，远程与其他用户被拒。
bool BuildCurrentUserSecurity(SECURITY_ATTRIBUTES& attributes) {
    HANDLE token = nullptr;
    if (!::OpenProcessToken(::GetCurrentProcess(), TOKEN_QUERY, &token)) return false;
    DWORD needed = 0;
    ::GetTokenInformation(token, TokenUser, nullptr, 0, &needed);
    if (needed == 0) {
        ::CloseHandle(token);
        return false;
    }
    std::string buffer(needed, '\0');
    BOOL got = ::GetTokenInformation(token, TokenUser, buffer.data(), needed, &needed);
    ::CloseHandle(token);
    if (!got) return false;
    auto* user = reinterpret_cast<TOKEN_USER*>(buffer.data());
    LPWSTR sidText = nullptr;
    if (!::ConvertSidToStringSidW(user->User.Sid, &sidText) || !sidText) return false;
    std::wstring sddl = std::wstring(L"D:P(A;;GA;;;") + sidText + L")";
    ::LocalFree(sidText);
    PSECURITY_DESCRIPTOR descriptor = nullptr;
    if (!::ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.c_str(), SDDL_REVISION_1, &descriptor, nullptr)) {
        return false;
    }
    attributes.nLength = sizeof(attributes);
    attributes.lpSecurityDescriptor = descriptor;
    attributes.bInheritHandle = FALSE;
    return true;
}

void HandleClient(HANDLE pipe) {
    ZzzControlRequest request{};
    DWORD readBytes = 0;
    if (!::ReadFile(pipe, &request, sizeof(request), &readBytes, nullptr) ||
        readBytes != sizeof(request) ||
        request.magic != ZZZ_CTRL_MAGIC_REQUEST ||
        request.abiVersion != ZZZ_BRIDGE_ABI_VERSION ||
        request.reserved != 0 ||
        (request.command != ZZZ_CTRL_CMD_START && request.command != ZZZ_CTRL_CMD_STOP)) {
        return;  // 协议不符：直接断开，不回执
    }
    uint32_t result = 0;
    if (request.command == ZZZ_CTRL_CMD_START) {
        result = StartBridgeCore(request.config);
    } else {
        result = StopBridgeCore();
    }
    ZzzControlReply reply{};
    reply.magic = ZZZ_CTRL_MAGIC_REPLY;
    reply.result = result;
    DWORD written = 0;
    ::WriteFile(pipe, &reply, sizeof(reply), &written, nullptr);
    ::FlushFileBuffers(pipe);
}

DWORD WINAPI ControlWorker(LPVOID) {
    try {
        // pin 本模块：宿主退出前绝不卸载，迟到回调永远落在有效代码上。
        HMODULE self = nullptr;
        ::GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_PIN | GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS,
                             reinterpret_cast<LPCWSTR>(&ControlWorker), &self);
        SECURITY_ATTRIBUTES attributes{};
        PSECURITY_DESCRIPTOR descriptor = nullptr;
        if (!BuildCurrentUserSecurity(attributes)) {
            return 0;  // 无法建立安全描述符就不开控制面；不崩溃宿主
        }
        descriptor = attributes.lpSecurityDescriptor;
        const std::wstring pipeName = ZzzPipeNameForPid(::GetCurrentProcessId());
        for (;;) {
            HANDLE pipe = ::CreateNamedPipeW(
                pipeName.c_str(),
                PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                1,  // 单实例：重复注入由注入器侧“已加载则跳过”兜底
                sizeof(ZzzControlRequest), sizeof(ZzzControlReply), 0,
                &attributes);
            if (pipe == INVALID_HANDLE_VALUE) {
                break;  // 已有实例（不应发生）或系统拒绝：worker 退出，不碰宿主
            }
            if (::ConnectNamedPipe(pipe, nullptr) || ::GetLastError() == ERROR_PIPE_CONNECTED) {
                try {
                    HandleClient(pipe);
                } catch (...) {
                    // 单个客户端的异常不结束控制面。
                }
            }
            ::DisconnectNamedPipe(pipe);
            ::CloseHandle(pipe);
        }
        ::LocalFree(descriptor);
    } catch (...) {
        // 控制面任何异常都不跨 native 边界、不影响宿主。
    }
    return 0;
}

}  // namespace

BOOL APIENTRY DllMain(HMODULE module, DWORD reason, LPVOID) {
    // 加载器锁内只做两件事：禁线程通知、拉起 worker。
    // CreateThread 创建的线程要等 loader lock 释放后才会进入入口，
    // 因此 worker 一定在锁外运行。
    switch (reason) {
        case DLL_PROCESS_ATTACH: {
            DisableThreadLibraryCalls(module);
            HANDLE thread = ::CreateThread(nullptr, 0, &ControlWorker, nullptr, 0, nullptr);
            if (thread) ::CloseHandle(thread);
            break;
        }
        case DLL_PROCESS_DETACH:
        case DLL_THREAD_ATTACH:
        case DLL_THREAD_DETACH:
            break;
    }
    return TRUE;
}
