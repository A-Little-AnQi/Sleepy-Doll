// 三月七原生桥 DLL 入口：DllMain 只禁用线程通知并拉起控制面 worker，立即返回；
// 本模块不导出任何供远程线程调用的函数。
//
// 注入方式（与 bgi-bridge/native/injector.cpp 的已验证做法一致）：注入器只做
// 一次指向系统 LoadLibraryW 的 CreateRemoteThread。DllMain 是加载器调用的，
// 不受 CFG 对"远程线程指向刚映射模块"的限制；真正的初始化全部发生在 worker
// 线程，且保证在加载器锁释放之后才开始执行（新线程的启动需要 loader lock）。
//
// worker（加载器锁外）：
//   1. pin 本模块（迟到 pending 回调绝不跳进已卸载代码）；
//   2. 创建命名管道 \\.\pipe\SleepyDoll.March7thBridge.<当前PID>：
//      PIPE_REJECT_REMOTE_CLIENTS + 仅当前用户 DACL；
//   3. 循环等待注入器连接：收 March7thControlRequest，按命令调用
//      StartBridgeImpl / StopBridgeImpl，回执真实结果码。worker 常驻：
//      HTTP 停桥后仍可经管道再次启动。
//
// 线程纪律：worker 用 Win32 线程、全程 try/catch（不跨 native 边界抛出）；
// 不保留任何 joinable std::thread 全局；进程退出由 OS 回收，不做清理回调。
// 不碰 GIL、不做文件 IO；令牌只经管道与内存传递，不落盘。

#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>

#include <sddl.h>

#include <string>

#include "control_ipc.hpp"

#pragma comment(lib, "advapi32.lib")

namespace sdm::march7th {

// host_python.cpp 提供：绑定 CPython、附着宿主、启动/停止回环 HTTP 服务。
// StartBridge 返回码见 README「错误码」一节；message 只写内部缓冲不外发。
unsigned long StartBridgeImpl(const wchar_t* argsJson, std::string& message);
unsigned long StopBridgeImpl();

} // namespace sdm::march7th

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
    if (!::ConvertSidToStringSidW(user->User.Sid, &sidText) || sidText == nullptr) return false;
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

// 令牌是可打印 ASCII（允许含 " 与 \），拼 JSON 前转义。
std::string JsonEscapeAscii(const std::string& text) {
    std::string out;
    out.reserve(text.size());
    for (char c : text) {
        if (c == '"' || c == '\\') {
            out.push_back('\\');
        }
        out.push_back(c);
    }
    return out;
}

bool ValidControlConfig(const March7thBridgeConfig& config) {
    if (config.abiVersion != MARCH7TH_BRIDGE_ABI_VERSION) return false;
    if (config.reserved != 0) return false;
    if (config.port == 0 || config.port > 65535) return false;
    uint32_t length = config.tokenChars;
    if (length < 16 || length >= MARCH7TH_BRIDGE_TOKEN_CHARS) return false;
    size_t terminator = 0;
    bool nul = false;
    for (uint32_t i = 0; i < MARCH7TH_BRIDGE_TOKEN_CHARS; ++i) {
        if (config.token[i] == L'\0') {
            nul = true;
            terminator = i;
            break;
        }
    }
    if (!nul || terminator != length) return false;
    for (uint32_t i = 0; i < length; ++i) {
        unsigned int c = static_cast<unsigned int>(config.token[i]);
        if (c < 0x21u || c > 0x7Eu) return false;
    }
    return true;
}

void HandleClient(HANDLE pipe) {
    March7thControlRequest request{};
    DWORD readBytes = 0;
    if (!::ReadFile(pipe, &request, sizeof(request), &readBytes, nullptr) ||
        readBytes != sizeof(request) ||
        request.magic != kMarch7thCtrlMagicRequest ||
        request.abiVersion != MARCH7TH_BRIDGE_ABI_VERSION ||
        request.reserved != 0 ||
        (request.command != kMarch7thCtrlCmdStart &&
         request.command != kMarch7thCtrlCmdStop)) {
        return; // 协议不符：直接断开，不回执
    }
    uint32_t result = 0;
    if (request.command == kMarch7thCtrlCmdStart) {
        if (!ValidControlConfig(request.config)) {
            result = 0x46; // 控制面配置不合规
        } else {
            std::string token;
            token.reserve(request.config.tokenChars);
            for (uint32_t i = 0; i < request.config.tokenChars; ++i) {
                token.push_back(static_cast<char>(request.config.token[i]));
            }
            std::string args = "{\"token\":\"" + JsonEscapeAscii(token) +
                               "\",\"port\":" + std::to_string(request.config.port) + "}";
            // args 为纯 ASCII，直接放宽字符。
            std::wstring wide(args.begin(), args.end());
            std::string message;
            result = sdm::march7th::StartBridgeImpl(wide.c_str(), message);
        }
    } else {
        result = sdm::march7th::StopBridgeImpl();
    }
    March7thControlReply reply{};
    reply.magic = kMarch7thCtrlMagicReply;
    reply.result = result;
    reply.reserved = 0;
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
            return 0; // 无法建立安全描述符就不开控制面；不崩溃宿主
        }
        descriptor = attributes.lpSecurityDescriptor;
        const std::wstring pipeName = March7thPipeNameForPid(::GetCurrentProcessId());
        for (;;) {
            HANDLE pipe = ::CreateNamedPipeW(
                pipeName.c_str(),
                PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                1, // 单实例：重复注入由注入器侧"已加载则跳过"兜底
                sizeof(March7thControlRequest), sizeof(March7thControlReply), 0,
                &attributes);
            if (pipe == INVALID_HANDLE_VALUE) {
                break; // 已有实例（不应发生）或系统拒绝：worker 退出，不碰宿主
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

} // namespace

BOOL APIENTRY DllMain(HMODULE module, DWORD reason, LPVOID) {
    // 加载器锁内只做两件事：禁线程通知、拉起 worker。
    // CreateThread 创建的线程要等 loader lock 释放后才会进入入口，
    // 因此 worker 一定在锁外运行。
    switch (reason) {
        case DLL_PROCESS_ATTACH: {
            ::DisableThreadLibraryCalls(module);
            HANDLE thread = ::CreateThread(nullptr, 0, &ControlWorker, nullptr, 0, nullptr);
            if (thread != nullptr) {
                ::CloseHandle(thread);
            }
            break;
        }
        case DLL_PROCESS_DETACH:
        case DLL_THREAD_ATTACH:
        case DLL_THREAD_DETACH:
            break;
    }
    return TRUE;
}
