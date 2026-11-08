// injector.cpp - zzz 原生桥注入器：把 zzz_bridge.dll 送进用户指定的现存进程。
//
// 只附着、不启动：--pid 是必填的显式目标。不向宿主目录写任何文件。
//
// 注入流程（与 bgi-bridge/native/injector.cpp 的已验证做法一致，本文件只有
// 一次 CreateRemoteThread，且只指向系统 LoadLibraryW）：
//   1. 注入前验证：双方 x64、目标已加载 python3xx.dll。
//   2. 若同路径 DLL 未加载：远程 CreateRemoteThread(LoadLibraryW) 映射 DLL；
//      DllMain 只拉起控制面 worker；等远程线程退出确认映射后才释放路径内存；
//      超时则保留分配（结果未知），绝不 TerminateThread、不重试注入。
//   3. 连接 \\.\pipe\SleepyDoll.ZzzBridge.<PID>；必须
//      GetNamedPipeServerProcessId == 显式目标 PID 之后才发送令牌，
//      再经管道下发启动配置并读取 DLL 侧真实结果回执。
//   4. 通过本机 HTTP /status 验证桥已监听且定位到唯一 ZContext。
//
// 绝不做第二个指向注入 DLL 导出的远程线程：CFG 宿主会以 0xC0000409/0xA
// 终止宿主进程。

#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>

#include <winsock2.h>
#include <ws2tcpip.h>
#include <tlhelp32.h>

#include <cstdio>
#include <cstring>
#include <string>

#include "zzz_bridge_abi.h"
#include "control_ipc.hpp"

#pragma comment(lib, "ws2_32.lib")
#pragma warning(disable : 4127)  // do{}while(false) cleanup pattern under /W4

namespace {

std::wstring DirectoryOf(const std::wstring& path) {
    const auto slash = path.find_last_of(L"\\/");
    return slash == std::wstring::npos ? std::wstring() : path.substr(0, slash);
}

std::wstring ModulePath(HMODULE module) {
    wchar_t buffer[MAX_PATH * 2]{};
    DWORD length = ::GetModuleFileNameW(module, buffer, MAX_PATH * 2);
    return length ? std::wstring(buffer, length) : std::wstring();
}

void Say(const std::wstring& message) {
    std::fwprintf(stdout, L"%ls\n", message.c_str());
    std::fflush(stdout);
}

void Fail(const std::wstring& message) {
    std::fwprintf(stderr, L"%ls\n", message.c_str());
    std::fflush(stderr);
}

// ASCII/UTF-8 → 宽字符（错误消息用）。
std::wstring WidenText(const std::string& text) {
    std::wstring out;
    out.reserve(text.size());
    for (unsigned char c : text) out.push_back(static_cast<wchar_t>(c));
    return out;
}

constexpr DWORD kAccess = PROCESS_CREATE_THREAD | PROCESS_QUERY_INFORMATION |
                          PROCESS_VM_OPERATION | PROCESS_VM_WRITE | PROCESS_VM_READ;

// 枚举目标进程模块：python 运行时存在性、桥是否已加载。
struct ModuleScan {
    bool foundPython = false;
    bool bridgeLoaded = false;
};

bool ScanModules(DWORD pid, const std::wstring& bridgePath, ModuleScan& out) {
    HANDLE snapshot = ::CreateToolhelp32Snapshot(TH32CS_SNAPMODULE | TH32CS_SNAPMODULE32, pid);
    if (snapshot == INVALID_HANDLE_VALUE) return false;
    MODULEENTRY32W module{};
    module.dwSize = sizeof(module);
    bool ok = false;
    if (::Module32FirstW(snapshot, &module)) {
        ok = true;
        do {
            const std::wstring name(module.szModule);
            if (name.size() > 11 && name.compare(0, 6, L"python") == 0 &&
                name.compare(name.size() - 4, 4, L".dll") == 0) {
                out.foundPython = true;
            }
            if (_wcsicmp(module.szExePath, bridgePath.c_str()) == 0) {
                out.bridgeLoaded = true;
            }
        } while (::Module32NextW(snapshot, &module));
    }
    ::CloseHandle(snapshot);
    return ok;
}

void* WriteRemote(HANDLE process, const void* data, size_t bytes) {
    void* remote = ::VirtualAllocEx(process, nullptr, bytes, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE);
    if (!remote) return nullptr;
    if (!::WriteProcessMemory(process, remote, data, bytes, nullptr)) {
        ::VirtualFreeEx(process, remote, 0, MEM_RELEASE);
        return nullptr;
    }
    return remote;
}

// 循环发送直到全部发出；不把 partial send 当成功。
bool SendAll(SOCKET socket, const std::string& data) {
    size_t sent = 0;
    while (sent < data.size()) {
        int wrote = ::send(socket, data.data() + sent, static_cast<int>(data.size() - sent), 0);
        if (wrote <= 0) return false;
        sent += static_cast<size_t>(wrote);
    }
    return true;
}

struct HttpProbe {
    bool ok = false;
    std::string body;
};

HttpProbe PostStatus(uint16_t port, const std::string& token) {
    HttpProbe result;
    SOCKET socket = ::socket(AF_INET, SOCK_STREAM, IPPROTO_TCP);
    if (socket == INVALID_SOCKET) return result;
    sockaddr_in address{};
    address.sin_family = AF_INET;
    address.sin_port = htons(port);
    address.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
    if (::connect(socket, (sockaddr*)&address, sizeof(address)) != 0) {
        ::closesocket(socket);
        return result;
    }
    std::string request = "POST /status HTTP/1.1\r\nHost: 127.0.0.1:" + std::to_string(port) +
                          "\r\nX-Sleepy-Token: " + token +
                          "\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}";
    if (!SendAll(socket, request)) {
        ::closesocket(socket);
        return result;
    }
    char buffer[8192];
    int got = 0;
    std::string raw;
    while ((got = ::recv(socket, buffer, sizeof(buffer), 0)) > 0) {
        raw.append(buffer, static_cast<size_t>(got));
        if (raw.size() > 64 * 1024) break;
    }
    ::closesocket(socket);
    size_t split = raw.find("\r\n\r\n");
    if (split != std::string::npos) {
        result.body = raw.substr(split + 4);
        result.ok = raw.compare(0, 9, "HTTP/1.1 ") == 0 && raw.compare(9, 3, "200") == 0;
    }
    return result;
}

// 经命名管道控制面下发启动指令；服务器 PID 必须等于显式目标，
// 核对通过之前不发送令牌。回执是 DLL 侧 StartBridgeCore 的真实结果码。
bool SendStartOverPipe(DWORD targetPid, uint32_t port, const std::string& token, uint32_t& resultCode) {
    const std::wstring pipeName = ZzzPipeNameForPid(targetPid);
    if (!::WaitNamedPipeW(pipeName.c_str(), 15000)) {
        resultCode = 140;
        return false;
    }
    HANDLE pipe = ::CreateFileW(pipeName.c_str(), GENERIC_READ | GENERIC_WRITE, 0, nullptr,
                                OPEN_EXISTING, 0, nullptr);
    if (pipe == INVALID_HANDLE_VALUE) {
        resultCode = 141;
        return false;
    }
    DWORD mode = PIPE_READMODE_BYTE | PIPE_WAIT;
    ::SetNamedPipeHandleState(pipe, &mode, nullptr, nullptr);
    ULONG serverPid = 0;
    if (!::GetNamedPipeServerProcessId(pipe, &serverPid) || serverPid != targetPid) {
        ::CloseHandle(pipe);
        resultCode = 142;
        return false;
    }
    ZzzControlRequest request{};
    request.magic = ZZZ_CTRL_MAGIC_REQUEST;
    request.abiVersion = ZZZ_BRIDGE_ABI_VERSION;
    request.command = ZZZ_CTRL_CMD_START;
    request.reserved = 0;
    request.config.abiVersion = ZZZ_BRIDGE_ABI_VERSION;
    request.config.port = port;
    size_t copied = 0;
    for (; copied < token.size() && copied < ZZZ_BRIDGE_TOKEN_CHARS - 1; ++copied) {
        request.config.token[copied] = static_cast<wchar_t>(token[copied]);
    }
    request.config.token[copied] = L'\0';
    DWORD written = 0;
    if (!::WriteFile(pipe, &request, sizeof(request), &written, nullptr) ||
        written != sizeof(request)) {
        ::CloseHandle(pipe);
        resultCode = 143;
        return false;
    }
    ZzzControlReply reply{};
    DWORD readBytes = 0;
    if (!::ReadFile(pipe, &reply, sizeof(reply), &readBytes, nullptr) ||
        readBytes != sizeof(reply) || reply.magic != ZZZ_CTRL_MAGIC_REPLY) {
        ::CloseHandle(pipe);
        resultCode = 144;
        return false;
    }
    ::CloseHandle(pipe);
    resultCode = reply.result;
    return reply.result == ZZZ_START_OK;
}

std::string Usage() {
    return
        "zzz_injector: attach the native zzz bridge DLL to an existing process\n"
        "\n"
        "  --pid <PID>        required, the running OneDragon GUI process\n"
        "  --token-env <NAME> environment variable holding the shared token\n"
        "                     (default ZZZ_BRIDGE_TOKEN; must also be set on the\n"
        "                     Sleepy Doll side)\n"
        "  --port <N>         bridge port, default 23101\n"
        "  --dll <PATH>       zzz_bridge.dll path, default beside this exe\n"
        "  --check            only run pre-injection checks, do not inject\n"
        "\n"
        "A single remote thread is used, targeting only LoadLibraryW. The DLL\n"
        "receives its start configuration over a local named pipe bound to the\n"
        "target PID; the pipe server PID is verified before the token is sent.\n"
        "The injector never starts the target and writes nothing to its directory.";
}

}  // namespace

int wmain(int argc, wchar_t** argv) {
    DWORD pid = 0;
    std::wstring tokenEnv = L"ZZZ_BRIDGE_TOKEN";
    uint32_t port = ZZZ_BRIDGE_DEFAULT_PORT;
    std::wstring dllPath;
    bool checkOnly = false;

    const std::wstring selfDir = DirectoryOf(ModulePath(nullptr));

    for (int i = 1; i < argc; ++i) {
        const std::wstring argument = argv[i];
        auto next = [&]() -> std::wstring {
            return i + 1 < argc ? argv[++i] : std::wstring();
        };
        if (argument == L"--help" || argument == L"-h") {
            std::printf("%s\n", Usage().c_str());
            return 0;
        }
        if (argument == L"--pid" && i + 1 < argc) {
            wchar_t* end = nullptr;
            const unsigned long value = wcstoul(argv[++i], &end, 10);
            if (!value || !end || *end) {
                Fail(L"--pid 需要一个有效的十进制 PID。");
                return 1;
            }
            pid = static_cast<DWORD>(value);
            continue;
        }
        if (argument == L"--token-env" && i + 1 < argc) {
            tokenEnv = next();
            if (tokenEnv.empty()) {
                Fail(L"--token-env 需要环境变量名。");
                return 1;
            }
            continue;
        }
        if (argument == L"--port" && i + 1 < argc) {
            wchar_t* end = nullptr;
            const unsigned long value = wcstoul(argv[++i], &end, 10);
            if (!value || value > 65535 || (end && *end)) {
                Fail(L"--port 需要在 1-65535 之间的端口号。");
                return 1;
            }
            port = static_cast<uint32_t>(value);
            continue;
        }
        if (argument == L"--dll" && i + 1 < argc) {
            dllPath = next();
            continue;
        }
        if (argument == L"--check") {
            checkOnly = true;
            continue;
        }
        Fail(L"无法识别的参数：" + argument);
        std::printf("%s\n", Usage().c_str());
        return 1;
    }

    if (!pid) {
        Fail(L"--pid 是必填项：只附着用户明确指定的现存进程，不自动寻找、不启动目标。");
        std::printf("%s\n", Usage().c_str());
        return 1;
    }
    if (dllPath.empty()) dllPath = selfDir + L"\\zzz_bridge.dll";
    wchar_t absolute[MAX_PATH * 2]{};
    if (!::GetFullPathNameW(dllPath.c_str(), MAX_PATH * 2, absolute, nullptr) ||
        ::GetFileAttributesW(absolute) == INVALID_FILE_ATTRIBUTES) {
        Fail(L"找不到桥 DLL：" + dllPath);
        return 1;
    }
    dllPath = absolute;

    // token 从本进程环境读取；经本机同用户 ACL 管道明文送达，不落任何文件。
    std::string token;
    {
        std::wstring wide;
        wchar_t buffer[ZZZ_BRIDGE_TOKEN_CHARS]{};
        if (::GetEnvironmentVariableW(tokenEnv.c_str(), buffer, ZZZ_BRIDGE_TOKEN_CHARS) > 0) {
            wide = buffer;
        }
        if (wide.empty() || wide.size() >= ZZZ_BRIDGE_TOKEN_CHARS - 1) {
            Fail(L"环境变量 " + tokenEnv + L" 未设置或令牌过长（上限 94 字符）；"
                 L"Sleepy Doll 侧必须设置同值变量后才能连接。");
            return 1;
        }
        for (wchar_t c : wide) {
            if (c >= 0x80) {
                Fail(L"令牌请使用 ASCII 字符。");
                return 1;
            }
            token.push_back(static_cast<char>(c));
        }
        if (token.size() < 16) {
            Fail(L"令牌过短（至少 16 字符），拒绝弱鉴权配置。");
            return 1;
        }
    }

    if (sizeof(void*) != 8) {
        Fail(L"注入器必须是 x64 构建。");
        return 2;
    }

    HANDLE process = ::OpenProcess(kAccess, FALSE, pid);
    if (!process) {
        Fail(L"无法打开目标进程 " + std::to_wstring(pid) +
             L"（权限不足或进程已退出）。请以与目标相同的权限运行。");
        return 2;
    }
    int exitCode = 0;
    do {
        BOOL wow64 = FALSE;
        if (!::IsWow64Process(process, &wow64) || wow64) {
            Fail(L"目标不是 x64 进程，拒绝注入 x64 桥。");
            exitCode = 2;
            break;
        }
        ModuleScan scan;
        if (!ScanModules(pid, dllPath, scan)) {
            Fail(L"无法枚举目标进程模块（权限或进程状态）。");
            exitCode = 2;
            break;
        }
        if (!scan.foundPython) {
            Fail(L"目标进程没有已加载的 python3xx.dll；桥只支持内嵌 CPython 的一条龙宿主。");
            exitCode = 3;
            break;
        }
        Say(L"已确认目标为 x64 且加载了 CPython 运行时。");
        if (scan.bridgeLoaded) {
            Say(L"桥 DLL 已在目标进程中；跳过重复注入，直接走控制面。");
        }
        if (checkOnly) {
            Say(L"--check 完成：注入前检查全部通过。");
            break;
        }

        if (!scan.bridgeLoaded) {
            // 本文件唯一一次远程线程，且只指向系统 LoadLibraryW。
            void* remotePath = WriteRemote(process, dllPath.c_str(), (dllPath.size() + 1) * sizeof(wchar_t));
            if (!remotePath) {
                Fail(L"无法写入远程 DLL 路径。");
                exitCode = 5;
                break;
            }
            auto loadLibrary = reinterpret_cast<LPTHREAD_START_ROUTINE>(::GetProcAddress(
                ::GetModuleHandleW(L"kernel32.dll"), "LoadLibraryW"));
            HANDLE thread = ::CreateRemoteThread(process, nullptr, 0, loadLibrary, remotePath, 0, nullptr);
            if (!thread) {
                ::VirtualFreeEx(process, remotePath, 0, MEM_RELEASE);
                Fail(L"远程 LoadLibraryW 线程创建失败（错误码 " + std::to_wstring(::GetLastError()) + L"）。");
                exitCode = 5;
                break;
            }
            // 等远程线程退出后才释放路径内存；超时保留分配直到宿主退出
            //（结果未知），绝不 TerminateThread、不重试注入。
            DWORD wait = ::WaitForSingleObject(thread, 60000);
            ::CloseHandle(thread);
            if (wait != WAIT_OBJECT_0) {
                Fail(L"等待远程加载超时；路径内存保留至宿主退出，结果未知，请勿立即重复注入。");
                exitCode = 5;
                break;
            }
            ::VirtualFreeEx(process, remotePath, 0, MEM_RELEASE);
            ModuleScan after{};
            if (!ScanModules(pid, dllPath, after) || !after.bridgeLoaded) {
                Fail(L"远程加载线程结束但未检测到桥 DLL；确认 DLL 与目标同为 x64。");
                exitCode = 5;
                break;
            }
        }

        // 控制面：管道服务器 PID 核对通过后才发令牌；回执是 DLL 侧真实结果。
        uint32_t resultCode = 0;
        if (!SendStartOverPipe(pid, port, token, resultCode)) {
            Fail(L"控制面启动失败（" + std::to_wstring(resultCode) +
                 L"：140=管道未就绪，141/143/144=连接或协议失败，142=服务器 PID 不符，"
                 L"110=无 CPython，111=C API 不全，112=解释器未初始化，122=端口占用，"
                 L"103=令牌过短，130=已在启动或运行）。");
            exitCode = 6;
            break;
        }
        Say(L"桥已按控制面回执启动；正在验证监听与 ZContext 定位……");

        // 注入后验证——/status 必须可达且已定位唯一 ZContext。
        WSADATA wsa{};
        bool wsaReady = ::WSAStartup(MAKEWORD(2, 2), &wsa) == 0;
        bool attached = false;
        std::string lastBody;
        for (int attempt = 0; attempt < 30 && !attached; ++attempt) {
            ::Sleep(500);
            if (wsaReady) {
                HttpProbe probe = PostStatus(static_cast<uint16_t>(port), token);
                if (probe.ok) {
                    lastBody = probe.body;
                    if (probe.body.find("\"attached\":true") != std::string::npos) {
                        attached = true;
                    }
                }
            }
        }
        if (wsaReady) ::WSACleanup();
        if (!attached) {
            Fail(L"桥未能完成 ZContext 定位或监听验证；宿主可能尚未完成初始化，"
                 L"或不是受支持的一条龙 GUI。未造假上下文，服务未通过验证。"
                 + (lastBody.empty() ? L"" : L"\nstatus 摘要：" + WidenText(lastBody)));
            exitCode = 7;
            break;
        }
        Say(L"验证通过：桥已监听 127.0.0.1:" + std::to_wstring(port) + L" 并定位到唯一 ZContext。");
        Say(L"Sleepy Doll 侧需设置同值 " + tokenEnv + L" 环境变量后重载扩展。");
    } while (false);

    ::CloseHandle(process);
    return exitCode;
}
