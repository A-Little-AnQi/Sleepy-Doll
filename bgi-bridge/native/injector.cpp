// BgiBridge 注入器：把引导 DLL 送进宿主进程。一次 LoadLibraryW，没有别的。
//
// 两个不能改的地方：
//   1. 不要再用第二个远程线程去调 DLL 的导出函数。宿主是启用 CFG 的 .NET 程序，
//      CreateRemoteThread 指向刚映射进来的模块会以 0xC0000409/0xA 杀掉宿主。
//      DllMain 是加载器调的，不受这条限制，所以入口放在那里。
//   2. 目标是提权运行的 BetterGI 时，本进程也必须提权，否则写不进去。
//      刻意不自己 runas —— 开关是常规操作，不该每次弹 UAC。
//
// 不向宿主目录写任何文件。

#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <tlhelp32.h>
#include <cstdio>
#include <cstring>
#include <string>
#include <vector>

namespace {

constexpr wchar_t kBootstrapDll[] = L"BgiBridge.Bootstrap.dll";

std::wstring DirectoryOf(const std::wstring& path);

// 提权后的实例在另一个控制台里跑，父进程看不到它的输出。
// 所以同时写一份日志到自己目录下——排查问题时这是唯一的线索。
// 注意：写的是注入器自己的目录，不是 BetterGI 的。
void Log(bool toStderr, const std::wstring& message) {
    FILE* console = toStderr ? stderr : stdout;
    fwprintf(console, L"%ls\n", message.c_str());
    fflush(console);

    wchar_t exe[MAX_PATH]{};
    if (::GetModuleFileNameW(nullptr, exe, MAX_PATH) == 0) return;

    const std::wstring log = DirectoryOf(exe) + L"\\injector.log";
    FILE* file = nullptr;
    if (_wfopen_s(&file, log.c_str(), L"a, ccs=UTF-8") != 0 || !file) return;

    SYSTEMTIME now{};
    ::GetLocalTime(&now);
    fwprintf(file, L"[%02d:%02d:%02d] %ls\n", now.wHour, now.wMinute, now.wSecond, message.c_str());
    fclose(file);
}

void Say(const std::wstring& message) { Log(false, message); }
void Fail(const std::wstring& message) { Log(true, message); }

std::wstring DirectoryOf(const std::wstring& path) {
    const auto slash = path.find_last_of(L"\\/");
    return slash == std::wstring::npos ? std::wstring() : path.substr(0, slash);
}

std::wstring ModulePath(HMODULE module) {
    std::vector<wchar_t> buffer(MAX_PATH);
    for (;;) {
        const DWORD length =
            ::GetModuleFileNameW(module, buffer.data(), static_cast<DWORD>(buffer.size()));
        if (length == 0) return {};
        if (length < buffer.size() - 1) return std::wstring(buffer.data(), length);
        buffer.resize(buffer.size() * 2);
    }
}

bool IsElevated() {
    HANDLE token = nullptr;
    if (!::OpenProcessToken(::GetCurrentProcess(), TOKEN_QUERY, &token)) return false;
    TOKEN_ELEVATION elevation{};
    DWORD size = sizeof(elevation);
    const BOOL ok = ::GetTokenInformation(token, TokenElevation, &elevation, size, &size);
    ::CloseHandle(token);
    return ok && elevation.TokenIsElevated;
}

bool ProcessIsElevated(HANDLE process) {
    HANDLE token = nullptr;
    if (!::OpenProcessToken(process, TOKEN_QUERY, &token)) return false;
    TOKEN_ELEVATION elevation{};
    DWORD size = sizeof(elevation);
    const BOOL ok = ::GetTokenInformation(token, TokenElevation, &elevation, size, &size);
    ::CloseHandle(token);
    return ok && elevation.TokenIsElevated != 0;
}

LARGE_INTEGER ProcessStartTime(HANDLE process) {
    FILETIME created{}, exited{}, kernel{}, user{};
    if (!::GetProcessTimes(process, &created, &exited, &kernel, &user)) return {};
    LARGE_INTEGER value;
    value.HighPart = static_cast<LONG>(created.dwHighDateTime);
    value.LowPart = created.dwLowDateTime;
    return value;
}

struct Target {
    DWORD pid = 0;
    HANDLE process = nullptr;
    bool elevated = false;
};

struct Candidate {
    DWORD pid = 0;
    HANDLE process = nullptr;
    LARGE_INTEGER started{};
    bool elevated = false;
};

// 多实例必须明确选 PID，避免把连接挂到错误的宿主。
bool FindTarget(const std::wstring& processName, DWORD requestedPid, Target& target, std::wstring& error) {
    HANDLE snapshot = ::CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
    if (snapshot == INVALID_HANDLE_VALUE) {
        error = L"无法枚举进程。";
        return false;
    }

    constexpr DWORD kAccess = PROCESS_CREATE_THREAD | PROCESS_QUERY_INFORMATION |
                              PROCESS_VM_OPERATION | PROCESS_VM_WRITE | PROCESS_VM_READ;

    std::vector<Candidate> candidates;
    PROCESSENTRY32W entry{};
    entry.dwSize = sizeof(entry);

    if (::Process32FirstW(snapshot, &entry)) {
        do {
            if (_wcsicmp(entry.szExeFile, processName.c_str()) != 0) continue;
            if (requestedPid && entry.th32ProcessID != requestedPid) continue;

            HANDLE process = ::OpenProcess(kAccess, FALSE, entry.th32ProcessID);
            if (!process) continue;  // 更高完整性级别，或者已经退出

            Candidate candidate;
            candidate.pid = entry.th32ProcessID;
            candidate.process = process;
            candidate.started = ProcessStartTime(process);
            candidate.elevated = ProcessIsElevated(process);
            candidates.push_back(candidate);
        } while (::Process32NextW(snapshot, &entry));
    }

    ::CloseHandle(snapshot);

    if (candidates.empty()) {
        error = L"没有找到 " + processName + L" 进程，或者它跑在更高的完整性级别上。";
        return false;
    }

    if (candidates.size() > 1) {
        for (auto& candidate : candidates) ::CloseHandle(candidate.process);
        error = L"找到多个目标进程，请使用 --pid 明确选择，或关闭多余实例。";
        return false;
    }
    const Candidate* chosen = nullptr;
    for (const auto& candidate : candidates) {
        if (!candidate.elevated) continue;
        if (!chosen || candidate.started.QuadPart > chosen->started.QuadPart) chosen = &candidate;
    }

    bool fallback = false;
    if (!chosen) {
        for (const auto& candidate : candidates) {
            if (!chosen || candidate.started.QuadPart > chosen->started.QuadPart) chosen = &candidate;
        }
        fallback = true;
    }

    for (auto& candidate : candidates) {
        if (&candidate == chosen) continue;
        ::CloseHandle(candidate.process);
    }

    target.pid = chosen->pid;
    target.process = chosen->process;
    target.elevated = chosen->elevated;

    if (fallback) {
        Say(L"注意：没有找到提权的 " + processName + L" 实例，回退到非提权的那个。"
            L"如果这是 BetterGI，它可能正处于提权重启前的短暂窗口，注进去不会有任何效果。");
    }
    return true;
}

void* WriteRemoteString(HANDLE process, const std::wstring& text) {
    const SIZE_T bytes = (text.size() + 1) * sizeof(wchar_t);
    void* remote = ::VirtualAllocEx(process, nullptr, bytes, MEM_COMMIT | MEM_RESERVE,
                                    PAGE_READWRITE);
    if (!remote) return nullptr;
    if (!::WriteProcessMemory(process, remote, text.c_str(), bytes, nullptr)) {
        ::VirtualFreeEx(process, remote, 0, MEM_RELEASE);
        return nullptr;
    }
    return remote;
}

// -1: could not inspect, 0: absent, 1: already loaded. A second LoadLibrary
// only increments the reference count and cannot rerun DllMain.
int BootstrapLoaded(DWORD pid) {
    HANDLE snapshot = ::CreateToolhelp32Snapshot(TH32CS_SNAPMODULE, pid);
    if (snapshot == INVALID_HANDLE_VALUE) return -1;
    MODULEENTRY32W module{};
    module.dwSize = sizeof(module);
    int result = 0;
    if (::Module32FirstW(snapshot, &module)) {
        do {
            if (_wcsicmp(module.szModule, kBootstrapDll) == 0) { result = 1; break; }
        } while (::Module32NextW(snapshot, &module));
    } else { result = -1; }
    ::CloseHandle(snapshot);
    return result;
}

std::wstring Usage() {
    return L"用法：BgiBridge.Injector.exe [--process <名称>] [--pid <PID>] [--bridge <桥目录>] [--list]\n"
           L"\n"
           L"  --process   目标进程名，默认 BetterGI.exe\n"
           L"  --bridge    桥所在目录（含 BgiBridge.Bootstrap.dll 与 bridge.config.json），\n"
           L"              默认是注入器自己所在的目录\n"
           L"  --list      只列出匹配的进程，不注入\n"
           L"\n"
           L"注入后桥在目标进程内监听 bridge.config.json 里配的地址。\n"
           L"卸载：关掉 BetterGI 即可——桥不向 BGI 目录写任何文件，也不改它的配置。\n";
}

int Inject(const std::wstring& processName, DWORD requestedPid, const std::wstring& bridgeDir) {
    const std::wstring dllPath = bridgeDir + L"\\" + kBootstrapDll;
    if (::GetFileAttributesW(dllPath.c_str()) == INVALID_FILE_ATTRIBUTES) {
        Fail(L"找不到引导 DLL：" + dllPath);
        return 3;
    }

    const std::wstring configPath = bridgeDir + L"\\bridge.config.json";
    if (::GetFileAttributesW(configPath.c_str()) == INVALID_FILE_ATTRIBUTES) {
        Fail(L"找不到配置文件：" + configPath);
        return 4;
    }

    Target target;
    std::wstring error;
    if (!FindTarget(processName, requestedPid, target, error)) {
        Fail(error);
        return 5;
    }
    Say(L"目标进程 PID " + std::to_wstring(target.pid));

    int result = 0;
    do {
        BOOL wow64 = FALSE;
        if (!::IsWow64Process(target.process, &wow64) || wow64) {
            Fail(L"目标不是兼容的 64 位进程，拒绝加载 x64 桥。");
            result = 12;
            break;
        }
        const int loaded = BootstrapLoaded(target.pid);
        if (loaded != 0) {
            Fail(loaded > 0 ? L"桥已加载；请通过连接开关启用。若连接失败，请检查 token 或重启 BetterGI。"
                            : L"无法检查目标模块，请确认目标为 x64 并检查权限。");
            result = 11;
            break;
        }
        void* remotePath = WriteRemoteString(target.process, dllPath);
        if (!remotePath) {
            Fail(L"无法在目标进程里写入 DLL 路径（可能被更高完整性级别拦下）。");
            result = 6;
            break;
        }

        // kernel32 在同一台机器上各进程地址一致，直接拿本机的 LoadLibraryW 地址。
        // GetProcAddress 只有 ANSI 版本，所以这里必须是窄字符串。
        const auto loadLibrary = reinterpret_cast<LPTHREAD_START_ROUTINE>(
            ::GetProcAddress(::GetModuleHandleW(L"kernel32.dll"), "LoadLibraryW"));
        if (!loadLibrary) {
            ::VirtualFreeEx(target.process, remotePath, 0, MEM_RELEASE);
            Fail(L"拿不到 LoadLibraryW 的地址。");
            result = 7;
            break;
        }

        HANDLE thread = ::CreateRemoteThread(target.process, nullptr, 0, loadLibrary, remotePath,
                                             0, nullptr);
        if (!thread) {
            ::VirtualFreeEx(target.process, remotePath, 0, MEM_RELEASE);
            Fail(L"CreateRemoteThread 失败（错误码 " + std::to_wstring(::GetLastError()) + L"）。");
            result = 8;
            break;
        }

        // 必须等远程线程读完参数再释放那块内存。
        // CreateRemoteThread 只是创建线程，它还没开始跑；提前 VirtualFreeEx 会让
        // LoadLibraryW 读到已解映射的页——访问违例，直接崩掉宿主。
        const DWORD wait = ::WaitForSingleObject(thread, 60'000);
        if (wait != WAIT_OBJECT_0) {
            // The loader may still be reading this memory. Leave it allocated
            // until process exit instead of turning a timeout into a crash.
            ::CloseHandle(thread);
            Fail(L"等待加载超时或失败，结果尚未确认。请检查桥日志，勿重复注入。");
            result = 10;
            break;
        }
        DWORD moduleBase = 0;
        const BOOL gotExit = ::GetExitCodeThread(thread, &moduleBase);
        ::CloseHandle(thread);
        ::VirtualFreeEx(target.process, remotePath, 0, MEM_RELEASE);

        if (!gotExit || BootstrapLoaded(target.pid) != 1) {
            Fail(L"加载线程结束，但目标进程中没有检测到引导 DLL。"
                 L"确认它与目标同为 x64。");
            result = 9;
            break;
        }

        Say(L"加载线程已结束；桥是否就绪须由 /bridge/v1/info 确认。");
        Say(L"运行日志：" + bridgeDir + L"\\bootstrap.log");
    } while (false);

    ::CloseHandle(target.process);
    return result;
}

int List(const std::wstring& processName) {
    HANDLE snapshot = ::CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
    if (snapshot == INVALID_HANDLE_VALUE) return 1;

    PROCESSENTRY32W entry{};
    entry.dwSize = sizeof(entry);
    int count = 0;

    if (::Process32FirstW(snapshot, &entry)) {
        do {
            if (_wcsicmp(entry.szExeFile, processName.c_str()) != 0) continue;
            ++count;
            HANDLE process = ::OpenProcess(PROCESS_QUERY_INFORMATION, FALSE, entry.th32ProcessID);
            const bool elevated = process && ProcessIsElevated(process);
            if (process) ::CloseHandle(process);
            Say(L"  PID " + std::to_wstring(entry.th32ProcessID)
                + (elevated ? L"  [已提权]" : L"  [普通权限]"));
        } while (::Process32NextW(snapshot, &entry));
    }

    ::CloseHandle(snapshot);
    if (count == 0) Say(L"没有找到 " + processName + L"。");
    return 0;
}

}  // namespace

int wmain(int argc, wchar_t** argv) {
    std::wstring processName = L"BetterGI.exe";
    std::wstring bridgeDir;
    bool listOnly = false;
    DWORD requestedPid = 0;

    for (int i = 1; i < argc; ++i) {
        const std::wstring argument = argv[i];
        if (argument == L"--help" || argument == L"-h") {
            Say(Usage());
            return 0;
        }
        if (argument == L"--list") {
            listOnly = true;
            continue;
        }
        if (argument == L"--process" && i + 1 < argc) {
            processName = argv[++i];
            continue;
        }
        if (argument == L"--pid" && i + 1 < argc) {
            wchar_t* end = nullptr;
            const auto value = wcstoul(argv[++i], &end, 10);
            if (!value || !end || *end || argv[i][0] == L'-') return 1;
            requestedPid = value;
            continue;
        }
        if (argument == L"--bridge" && i + 1 < argc) {
            bridgeDir = argv[++i];
            continue;
        }
        Fail(L"无法识别的参数：" + argument + L"\n\n" + Usage());
        return 1;
    }

    if (bridgeDir.empty()) bridgeDir = DirectoryOf(ModulePath(nullptr));
    wchar_t absolute[MAX_PATH]{};
    const DWORD length = ::GetFullPathNameW(bridgeDir.c_str(), MAX_PATH, absolute, nullptr);
    if (!length || length + 40 >= MAX_PATH) {
        Fail(L"桥目录路径过长或无效，请使用较短的绝对路径。");
        return 1;
    }
    bridgeDir = absolute;

    if (listOnly) return List(processName);

    // 刻意**不**自己提权。
    //
    // 自己 runas 会让用户每注入一次就点一次 UAC，而注入是开关的常规操作，
    // 不该有这种打断。提权由调用方负责——最终封装 Sleepy Doll 的那层软件
    // 以管理员身份运行，这里就天然有权限了。
    //
    // 没有权限时给出可操作的错误，而不是弹一个窗。
    if (!IsElevated()) {
        Fail(L"需要管理员权限才能注入：BetterGI 自身以管理员身份运行，"
             L"低权限进程无法向它写入。请以管理员身份运行本程序，"
             L"或把它交给同样以管理员身份运行的宿主调用。");
        return 2;
    }

    return Inject(processName, requestedPid, bridgeDir);
}
