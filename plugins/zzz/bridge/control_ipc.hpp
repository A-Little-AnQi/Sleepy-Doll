// control_ipc.hpp - zzz 桥 DLL 与注入器之间的本地命名管道控制面协议（仅 zzz 内部）。
//
// 背景：对注入 DLL 再开第二个指向导出函数的远程线程，在启用 CFG 的宿主会以
// 0xC0000409/0xA 杀死宿主（见 bgi-bridge/native/injector.cpp 顶部记载）。
// 因此注入器只做一次系统 LoadLibraryW 远程线程；DLL 由 DllMain 启动 worker
// （loader lock 外运行），通过本管道接收启动/停止指令并回执真实结果。
//
// 管道：\\.\pipe\SleepyDoll.ZzzBridge.<宿主PID>
// 安全：PIPE_REJECT_REMOTE_CLIENTS + 仅当前用户的 DACL；不落盘、不写宿主文件、
// 不启动系统服务。注入器侧必须 GetNamedPipeServerProcessId == 显式目标 PID
// 才发送令牌。

#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>

#include <cstdint>
#include <string>

#include "zzz_bridge_abi.h"

#define ZZZ_PIPE_PREFIX L"\\\\.\\pipe\\SleepyDoll.ZzzBridge."

constexpr uint32_t ZZZ_CTRL_MAGIC_REQUEST = 0x3151425Au;  // 'ZBQ1'
constexpr uint32_t ZZZ_CTRL_MAGIC_REPLY = 0x3152425Au;    // 'ZBR1'
constexpr uint32_t ZZZ_CTRL_CMD_START = 1u;
constexpr uint32_t ZZZ_CTRL_CMD_STOP = 2u;

#pragma pack(push, 8)
struct ZzzControlRequest {
    uint32_t magic;      // ZZZ_CTRL_MAGIC_REQUEST
    uint32_t abiVersion; // 必须等于 ZZZ_BRIDGE_ABI_VERSION
    uint32_t command;    // ZZZ_CTRL_CMD_START / STOP
    uint32_t reserved;   // 必须为 0
    ZzzBridgeConfig config;  // start 时有效；stop 时忽略
};

struct ZzzControlReply {
    uint32_t magic;      // ZZZ_CTRL_MAGIC_REPLY
    uint32_t result;     // StartBridgeCore / StopBridgeCore 的真实返回码
    uint32_t reserved;   // 0
};
#pragma pack(pop)

inline std::wstring ZzzPipeNameForPid(DWORD pid) {
    return std::wstring(ZZZ_PIPE_PREFIX) + std::to_wstring(pid);
}
