// control_ipc.hpp - 三月七桥 DLL 与注入器之间的本地命名管道控制面协议
//（仅 march7th 内部，与其它插件互不依赖）。
//
// 背景：对注入 DLL 再开第二个指向其导出函数的远程线程，在启用 CFG 的宿主会以
// 0xC0000409 杀死宿主（见 bgi-bridge/native/injector.cpp 顶部记载）。因此注入器
// 只做一次指向系统 LoadLibraryW 的远程线程；DLL 不导出任何入口，由 DllMain 拉起
// worker（加载器锁外运行），经本管道接收启动/停止指令并回执真实结果码。
//
// 管道：\\.\pipe\SleepyDoll.March7thBridge.<宿主PID>
// 安全：PIPE_REJECT_REMOTE_CLIENTS + 仅当前用户的 DACL；不落盘、不写宿主文件、
// 不启动系统服务。注入器侧必须 GetNamedPipeServerProcessId == 显式目标 PID
// 之后才发送令牌。

#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>

#include <cstdint>
#include <string>

#define MARCH7TH_BRIDGE_ABI_VERSION 1u
#define MARCH7TH_BRIDGE_TOKEN_CHARS 264u // 含 NUL；令牌本体 16..256 个可打印 ASCII

constexpr uint32_t kMarch7thCtrlMagicRequest = 0x3151424Du; // 'MBQ1'
constexpr uint32_t kMarch7thCtrlMagicReply = 0x3152424Du;   // 'MBR1'
constexpr uint32_t kMarch7thCtrlCmdStart = 1u;
constexpr uint32_t kMarch7thCtrlCmdStop = 2u;

#pragma pack(push, 8)
struct March7thBridgeConfig {
    uint32_t abiVersion; // 必须等于 MARCH7TH_BRIDGE_ABI_VERSION
    uint32_t port;       // 仅监听 127.0.0.1
    uint32_t tokenChars; // 不含 NUL 的令牌长度
    uint32_t reserved;   // 必须为 0
    wchar_t token[MARCH7TH_BRIDGE_TOKEN_CHARS]; // NUL 结尾，仅可打印 ASCII
};

struct March7thControlRequest {
    uint32_t magic;      // kMarch7thCtrlMagicRequest
    uint32_t abiVersion; // MARCH7TH_BRIDGE_ABI_VERSION
    uint32_t command;    // kMarch7thCtrlCmdStart / kMarch7thCtrlCmdStop
    uint32_t reserved;   // 必须为 0
    March7thBridgeConfig config; // start 时有效；stop 时忽略
};

struct March7thControlReply {
    uint32_t magic;    // kMarch7thCtrlMagicReply
    uint32_t result;   // StartBridgeImpl / StopBridgeImpl 的真实返回码
    uint32_t reserved; // 0
};
#pragma pack(pop)

// DLL 不导出任何供远程线程调用的入口（CFG 宿主会以 0xC0000409 终止）。
// 启动/停止经由 DllMain 拉起的本地命名管道控制面（本文件）。

inline std::wstring March7thPipeNameForPid(DWORD pid) {
    return std::wstring(L"\\\\.\\pipe\\SleepyDoll.March7thBridge.") + std::to_wstring(pid);
}
