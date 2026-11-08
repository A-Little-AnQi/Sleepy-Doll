// zzz_bridge_abi.h - injector 与桥 DLL 共享的启动参数布局（保持两侧一致）。
#pragma once

#include <stdint.h>

#define ZZZ_BRIDGE_ABI_VERSION 1u
#define ZZZ_BRIDGE_DEFAULT_PORT 23101u
#define ZZZ_BRIDGE_TOKEN_CHARS 96u

#pragma pack(push, 8)
struct ZzzBridgeConfig {
    uint32_t abiVersion;                // 必须等于 ZZZ_BRIDGE_ABI_VERSION
    uint32_t port;                      // 仅监听 127.0.0.1
    wchar_t token[ZZZ_BRIDGE_TOKEN_CHARS];  // NUL 结尾；过短时 DLL 拒绝启动
};
#pragma pack(pop)

// DLL 不导出任何供远程线程调用的入口（CFG 宿主会以 0xC0000409 终止）。
// 启动/停止经由 DllMain 拉起的本地命名管道控制面（见 control_ipc.hpp）。

// Start 返回码（injector 侧解释）。
#define ZZZ_START_OK 0u
#define ZZZ_START_BAD_ABI 101u
#define ZZZ_START_BAD_PORT 102u
#define ZZZ_START_WEAK_TOKEN 103u
#define ZZZ_START_NO_PYTHON 110u
#define ZZZ_START_NO_PYAPI 111u
#define ZZZ_START_PY_NOT_INITIALIZED 112u
#define ZZZ_START_WSA 120u
#define ZZZ_START_SOCKET 121u
#define ZZZ_START_BIND 122u
