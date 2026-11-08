// March7th native bridge injector.
//
// Attaches the bridge DLL to an ALREADY RUNNING March7th Assistant process.
// It never starts any target process, never writes into the target install
// directory, and never prints the token value.
//
// Usage:
//   march7th_injector.exe --pid <pid> [--token-env MARCH7TH_BRIDGE_TOKEN]
//                         [--port 23111] [--dll path\to\march7th_bridge.dll]
//
// Steps:
//   1. validate token (ASCII printable, 16..256 chars), port, DLL file;
//   2. OpenProcess with injection rights; verify both processes are x64
//      and the target has a python3*.dll loaded (fail closed otherwise);
//   3. if the bridge DLL is not loaded yet: ONE remote thread targeting the
//      system LoadLibraryW only. On timeout the remote path buffer stays
//      allocated until the target exits (outcome unknown) - never
//      TerminateThread, never free a buffer the thread may still read;
//   4. connect \\.\pipe\SleepyDoll.March7thBridge.<pid>; the pipe server PID
//      must equal the explicit target PID before the token is sent;
//   5. send the strict ABI start config and report the DLL-side real result
//      code from the control-plane reply (see README error codes).
//
// A second remote thread pointing at an export of the injected DLL would get
// a CFG-enabled host killed with 0xC0000409 - that path is forbidden here.
//
// All console messages are ASCII English on purpose (console codepage safety).

#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>
#include <tlhelp32.h>

#include <cstdio>
#include <cstring>
#include <string>

#include "control_ipc.hpp"

namespace {

constexpr unsigned long kExitOk = 0;
constexpr unsigned long kExitUsage = 2;
constexpr unsigned long kExitToken = 3;
constexpr unsigned long kExitOpen = 4;
constexpr unsigned long kExitArch = 5;
constexpr unsigned long kExitPython = 6;
constexpr unsigned long kExitDll = 7;
constexpr unsigned long kExitRemote = 8;
constexpr unsigned long kExitBridge = 9;
constexpr unsigned long kExitPipe = 10;

void Fail(unsigned long code, const char* message) {
    std::fprintf(stderr, "injector: %s (code %lu)\n", message, code);
    std::exit(int(code));
}

std::wstring ModuleDirectoryOfSelf() {
    wchar_t path[MAX_PATH * 2]{};
    DWORD length = ::GetModuleFileNameW(nullptr, path, DWORD(std::size(path)));
    if (length == 0 || length >= std::size(path)) {
        return std::wstring(L".");
    }
    std::wstring text(path, length);
    size_t slash = text.find_last_of(L"\\/");
    return slash == std::wstring::npos ? std::wstring(L".") : text.substr(0, slash);
}

struct ModuleScan {
    bool foundPython = false;
    bool bridgeLoaded = false;
};

bool ScanModules(DWORD pid, const std::wstring& bridgePath, ModuleScan& out) {
    HANDLE snapshot = ::CreateToolhelp32Snapshot(TH32CS_SNAPMODULE | TH32CS_SNAPMODULE32, pid);
    if (snapshot == INVALID_HANDLE_VALUE) {
        return false;
    }
    MODULEENTRY32W entry{};
    entry.dwSize = sizeof(entry);
    bool ok = false;
    if (::Module32FirstW(snapshot, &entry)) {
        ok = true;
        do {
            const wchar_t* name = entry.szModule;
            size_t length = wcslen(name);
            if (length > 4 && _wcsnicmp(name, L"python3", 7) == 0 &&
                _wcsicmp(name + length - 4, L".dll") == 0) {
                out.foundPython = true;
            }
            if (_wcsicmp(entry.szExePath, bridgePath.c_str()) == 0) {
                out.bridgeLoaded = true;
            }
        } while (::Module32NextW(snapshot, &entry));
    }
    ::CloseHandle(snapshot);
    return ok;
}

void* WriteRemote(HANDLE process, const void* data, size_t bytes) {
    void* remote = ::VirtualAllocEx(process, nullptr, bytes, MEM_COMMIT | MEM_RESERVE,
                                    PAGE_READWRITE);
    if (remote == nullptr) {
        return nullptr;
    }
    SIZE_T written = 0;
    if (!::WriteProcessMemory(process, remote, data, bytes, &written) || written != bytes) {
        ::VirtualFreeEx(process, remote, 0, MEM_RELEASE);
        return nullptr;
    }
    return remote;
}

// 经命名管道控制面下发启动指令；服务器 PID 必须等于显式目标，核对通过之前
// 不发送令牌。回执是 DLL 侧 StartBridgeImpl 的真实结果码。
bool SendStartOverPipe(DWORD targetPid, uint32_t port, const std::string& token,
                       uint32_t& resultCode) {
    const std::wstring pipeName = March7thPipeNameForPid(targetPid);
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
    March7thControlRequest request{};
    request.magic = kMarch7thCtrlMagicRequest;
    request.abiVersion = MARCH7TH_BRIDGE_ABI_VERSION;
    request.command = kMarch7thCtrlCmdStart;
    request.reserved = 0;
    request.config.abiVersion = MARCH7TH_BRIDGE_ABI_VERSION;
    request.config.port = port;
    request.config.reserved = 0;
    size_t copied = 0;
    for (; copied < token.size() && copied < MARCH7TH_BRIDGE_TOKEN_CHARS - 1; ++copied) {
        request.config.token[copied] = static_cast<wchar_t>(token[copied]);
    }
    request.config.token[copied] = L'\0';
    request.config.tokenChars = static_cast<uint32_t>(copied);
    DWORD written = 0;
    if (!::WriteFile(pipe, &request, sizeof(request), &written, nullptr) ||
        written != sizeof(request)) {
        ::CloseHandle(pipe);
        resultCode = 143;
        return false;
    }
    March7thControlReply reply{};
    DWORD readBytes = 0;
    if (!::ReadFile(pipe, &reply, sizeof(reply), &readBytes, nullptr) ||
        readBytes != sizeof(reply) || reply.magic != kMarch7thCtrlMagicReply) {
        ::CloseHandle(pipe);
        resultCode = 144;
        return false;
    }
    ::CloseHandle(pipe);
    resultCode = reply.result;
    return reply.result == 0;
}

} // namespace

int wmain(int argc, wchar_t** argv) {
    DWORD pid = 0;
    std::wstring tokenEnv = L"MARCH7TH_BRIDGE_TOKEN";
    unsigned long port = 23111;
    std::wstring dllPath;

    for (int i = 1; i < argc; ++i) {
        std::wstring arg = argv[i];
        auto next = [&]() -> std::wstring {
            if (i + 1 >= argc) {
                Fail(kExitUsage, "missing value for argument");
            }
            return argv[++i];
        };
        if (arg == L"--pid") {
            pid = DWORD(std::wcstoul(next().c_str(), nullptr, 10));
        } else if (arg == L"--token-env") {
            tokenEnv = next();
        } else if (arg == L"--port") {
            port = std::wcstoul(next().c_str(), nullptr, 10);
        } else if (arg == L"--dll") {
            dllPath = next();
        } else {
            Fail(kExitUsage, "unknown argument (expected --pid/--token-env/--port/--dll)");
        }
    }
    if (pid == 0) {
        Fail(kExitUsage, "--pid <existing process id> is required; this tool never starts a target");
    }
    if (pid == ::GetCurrentProcessId()) {
        Fail(kExitUsage, "refusing to attach to the injector itself");
    }
    if (port == 0 || port > 65535) {
        Fail(kExitUsage, "--port must be 1..65535");
    }

    // Token: read from this process environment; validate; never print it.
    wchar_t tokenBuffer[512]{};
    DWORD tokenLength = ::GetEnvironmentVariableW(tokenEnv.c_str(), tokenBuffer,
                                                  DWORD(std::size(tokenBuffer)));
    if (tokenLength == 0 || tokenLength >= std::size(tokenBuffer)) {
        std::fprintf(stderr,
                     "injector: environment variable %ls is not set; generate a random ASCII "
                     "string of 32+ characters (see plugin README)\n",
                     tokenEnv.c_str());
        return int(kExitToken);
    }
    std::string token;
    for (DWORD i = 0; i < tokenLength; ++i) {
        unsigned int c = unsigned int(tokenBuffer[i]);
        if (c < 0x21 || c > 0x7E) {
            Fail(kExitToken, "token must be printable ASCII");
        }
        token.push_back(char(c));
    }
    if (token.size() < 16) {
        Fail(kExitToken, "token too short (minimum 16 printable ASCII chars; 32+ recommended)");
    }

    if (dllPath.empty()) {
        dllPath = ModuleDirectoryOfSelf() + L"\\march7th_bridge.dll";
    }
    wchar_t absoluteDll[MAX_PATH * 2]{};
    if (!::GetFullPathNameW(dllPath.c_str(), DWORD(std::size(absoluteDll)), absoluteDll,
                            nullptr)) {
        std::fprintf(stderr, "injector: cannot resolve DLL path: %ls\n", dllPath.c_str());
        return int(kExitDll);
    }
    dllPath = absoluteDll;
    DWORD dllAttributes = ::GetFileAttributesW(dllPath.c_str());
    if (dllAttributes == INVALID_FILE_ATTRIBUTES || (dllAttributes & FILE_ATTRIBUTE_DIRECTORY)) {
        std::fprintf(stderr, "injector: bridge DLL not found: %ls\n", dllPath.c_str());
        return int(kExitDll);
    }

    HANDLE process = ::OpenProcess(
        PROCESS_CREATE_THREAD | PROCESS_QUERY_INFORMATION | PROCESS_VM_OPERATION |
            PROCESS_VM_WRITE | PROCESS_VM_READ,
        FALSE, pid);
    if (process == nullptr) {
        std::fprintf(stderr, "injector: OpenProcess(%lu) failed with error %lu\n", pid,
                     ::GetLastError());
        return int(kExitOpen);
    }

    // Architecture: this injector is built x64; reject WOW64 targets.
    BOOL wow64 = FALSE;
    if (!::IsWow64Process(process, &wow64)) {
        std::fprintf(stderr, "injector: IsWow64Process failed with error %lu\n", ::GetLastError());
        return int(kExitArch);
    }
    if (wow64) {
        std::fprintf(stderr, "injector: target %lu is a 32-bit process; x64 required\n", pid);
        return int(kExitArch);
    }

    ModuleScan scan;
    if (!ScanModules(pid, dllPath, scan)) {
        std::fprintf(stderr, "injector: cannot enumerate modules of target %lu\n", pid);
        return int(kExitPython);
    }
    if (!scan.foundPython) {
        std::fprintf(stderr,
                     "injector: target %lu has no python3*.dll loaded; it is not a running "
                     "March7th GUI (or CPython API is not visible). Fail closed.\n",
                     pid);
        return int(kExitPython);
    }
    std::printf("injector: target %lu confirmed x64 with CPython loaded\n", pid);
    if (scan.bridgeLoaded) {
        std::printf("injector: bridge DLL already loaded in target; skipping remote load\n");
    }

    if (!scan.bridgeLoaded) {
        // The ONLY remote thread in this injector, targeting the system
        // LoadLibraryW. The DLL starts its control-plane worker from DllMain.
        std::wstring widePath = dllPath;
        void* remotePath =
            WriteRemote(process, widePath.c_str(), (widePath.size() + 1) * sizeof(wchar_t));
        if (remotePath == nullptr) {
            std::fprintf(stderr, "injector: WriteProcessMemory(dll path) failed (error %lu)\n",
                         ::GetLastError());
            return int(kExitRemote);
        }
        auto loadLibrary = reinterpret_cast<LPTHREAD_START_ROUTINE>(
            ::GetProcAddress(::GetModuleHandleW(L"kernel32.dll"), "LoadLibraryW"));
        if (loadLibrary == nullptr) {
            ::VirtualFreeEx(process, remotePath, 0, MEM_RELEASE);
            Fail(kExitRemote, "LoadLibraryW not found in kernel32");
        }
        HANDLE loadThread =
            ::CreateRemoteThread(process, nullptr, 0, loadLibrary, remotePath, 0, nullptr);
        if (loadThread == nullptr) {
            ::VirtualFreeEx(process, remotePath, 0, MEM_RELEASE);
            std::fprintf(stderr,
                         "injector: CreateRemoteThread(LoadLibraryW) failed (error %lu)\n",
                         ::GetLastError());
            return int(kExitRemote);
        }
        DWORD wait = ::WaitForSingleObject(loadThread, 30000);
        DWORD loadExit = 0;
        ::GetExitCodeThread(loadThread, &loadExit);
        ::CloseHandle(loadThread);
        if (wait != WAIT_OBJECT_0) {
            // Outcome unknown: the remote thread may still be running. Never
            // terminate it and never free the path buffer it may still read;
            // leave the allocation to be reclaimed when the target exits.
            std::fprintf(stderr,
                         "injector: remote LoadLibraryW timed out; thread not terminated, "
                         "path buffer left allocated until target exits. Do not retry "
                         "blindly - check the target first.\n");
            return int(kExitRemote);
        }
        ::VirtualFreeEx(process, remotePath, 0, MEM_RELEASE);
        ModuleScan after{};
        if (!ScanModules(pid, dllPath, after)) {
            std::fprintf(stderr, "injector: cannot enumerate modules of target %lu\n", pid);
            return int(kExitRemote);
        }
        if (!after.bridgeLoaded || loadExit == 0) {
            std::fprintf(stderr,
                         "injector: remote LoadLibraryW did not map the DLL (loader rejected "
                         "it or wrong architecture)\n");
            return int(kExitRemote);
        }
    }

    // Control plane: the pipe server PID must match the explicit target before
    // the token leaves this process; the reply carries the real bridge result.
    uint32_t resultCode = 0;
    if (!SendStartOverPipe(pid, uint32_t(port), token, resultCode)) {
        if (resultCode >= 140) {
            std::fprintf(stderr,
                         "injector: control plane failed (%lu: 140=pipe not ready, "
                         "141/143/144=connect or protocol failure, 142=server pid mismatch)\n",
                         unsigned long(resultCode));
            return int(kExitPipe);
        }
        std::fprintf(stderr,
                     "injector: StartBridge rejected with code 0x%02lX (see plugin README "
                     "error codes)\n",
                     unsigned long(resultCode));
        return int(kExitBridge);
    }
    std::printf("injector: bridge attached; listening on 127.0.0.1:%lu\n", port);
    return int(kExitOk);
}
