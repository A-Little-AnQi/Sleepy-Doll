// BgiBridge 引导 DLL：被注入后用 hostfxr 接上宿主**已有的** .NET 运行时，
// 载入托管控制面。
//
// 入口必须在 DllMain 起的线程里，不能靠注入器远程调导出函数——
// 详见 injector.cpp 顶部的说明。
//
// 只用 kernel32：宿主是别人的进程，少一个依赖少一处故障点。

#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <cstdint>

namespace {

// 本 DLL 自己的模块句柄与目录，DllMain 里填。
// 不能用 GetModuleFileName(nullptr) —— 那拿到的是 BetterGI.exe 的路径，
// 我们的程序集和配置都在自己目录下，用错了就什么都找不到。
HMODULE g_self = nullptr;
WCHAR g_bridgeDir[MAX_PATH]{};

// ---------- 字符串：只用 kernel32 ----------

void AppendText(WCHAR* buffer, const WCHAR* text) { lstrcatW(buffer, text); }

void AppendNumber(WCHAR* buffer, unsigned value) {
    WCHAR digits[12]{};
    int count = 0;
    if (value == 0) digits[count++] = L'0';
    while (value > 0 && count < 11) {
        digits[count++] = static_cast<WCHAR>(L'0' + (value % 10));
        value /= 10;
    }
    WCHAR* end = buffer + lstrlenW(buffer);
    while (count > 0) *end++ = digits[--count];
    *end = 0;
}

void AppendHex(WCHAR* buffer, unsigned value) {
    const WCHAR* table = L"0123456789ABCDEF";
    WCHAR digits[8]{};
    for (int i = 0; i < 8; ++i) {
        digits[i] = table[value & 0xF];
        value >>= 4;
    }
    WCHAR* end = buffer + lstrlenW(buffer);
    for (int i = 7; i >= 0; --i) *end++ = digits[i];
    *end = 0;
}

void AppendVersion(WCHAR* buffer, int major, int minor, int patch) {
    AppendNumber(buffer, static_cast<unsigned>(major));
    AppendText(buffer, L".");
    AppendNumber(buffer, static_cast<unsigned>(minor));
    AppendText(buffer, L".");
    AppendNumber(buffer, static_cast<unsigned>(patch));
}

// ---------- 日志 ----------

void LogLine(const WCHAR* text) {
    if (g_bridgeDir[0] == 0) return;

    WCHAR path[MAX_PATH]{};
    lstrcpynW(path, g_bridgeDir, MAX_PATH);
    AppendText(path, L"\\bootstrap.log");

    HANDLE file = ::CreateFileW(path, FILE_APPEND_DATA, FILE_SHARE_READ | FILE_SHARE_WRITE,
                                nullptr, OPEN_ALWAYS, FILE_ATTRIBUTE_NORMAL, nullptr);
    if (file == INVALID_HANDLE_VALUE) return;

    SYSTEMTIME now{};
    ::GetLocalTime(&now);
    WCHAR line[768]{};
    AppendText(line, L"[");
    AppendNumber(line, now.wHour);
    AppendText(line, L":");
    AppendNumber(line, now.wMinute);
    AppendText(line, L":");
    AppendNumber(line, now.wSecond);
    AppendText(line, L".") ;
    AppendNumber(line, now.wMilliseconds);
    AppendText(line, L"] ");
    lstrcpynW(line + lstrlenW(line), text, 512);
    AppendText(line, L"\r\n");

    DWORD written = 0;
    ::WriteFile(file, line, static_cast<DWORD>(lstrlenW(line)) * sizeof(WCHAR), &written, nullptr);
    ::CloseHandle(file);
}

void LogWithCode(const WCHAR* step, int32_t code) {
    WCHAR buffer[768]{};
    lstrcpynW(buffer, step, 512);
    AppendText(buffer, L" (0x");
    AppendHex(buffer, static_cast<unsigned>(code));
    AppendText(buffer, L")");
    LogLine(buffer);
}

// ---------- hostfxr 接口（只声明用到的部分） ----------

using hostfxr_handle = void*;

struct hostfxr_initialize_parameters {
    size_t size;
    const wchar_t* host_path;
    const wchar_t* dotnet_root;
};

// hostfxr_delegate_type 里我们只要这一个
constexpr int32_t hdt_load_assembly_and_get_function_pointer = 5;

// 传给 load_assembly_and_get_function_pointer 表示"目标方法是 [UnmanagedCallersOnly]"。
// 不能是 constexpr —— reinterpret_cast 不是常量表达式。
const wchar_t* const unmanaged_callers_only = reinterpret_cast<const wchar_t*>(-1);

using hostfxr_initialize_for_runtime_config_fn =
    int32_t (*)(const wchar_t*, const hostfxr_initialize_parameters*, hostfxr_handle*);
using hostfxr_get_runtime_delegate_fn = int32_t (*)(hostfxr_handle, int32_t, void**);
using hostfxr_close_fn = int32_t (*)(hostfxr_handle);
using load_assembly_and_get_function_pointer_fn =
    int32_t (*)(const wchar_t*, const wchar_t*, const wchar_t*, const wchar_t*, void*, void**);

// 托管入口的签名：int Start(const wchar_t*)
using bridge_start_fn = int32_t (*)(const wchar_t*);
using bridge_stop_fn = int32_t (*)();

// ---------- 版本比较 ----------

void ParseVersion(const WCHAR* text, int& major, int& minor, int& patch) {
    major = minor = patch = 0;
    int* target = &major;
    for (const WCHAR* p = text; *p; ++p) {
        if (*p >= L'0' && *p <= L'9') {
            *target = *target * 10 + (*p - L'0');
        } else if (*p == L'.') {
            if (target == &major) target = &minor;
            else if (target == &minor) target = &patch;
            else break;
        } else {
            break;
        }
    }
}

// ---------- 找 hostfxr ----------

HMODULE LoadHostfxr() {
    // 优先用进程里已经加载的那一份：它和宿主用的是同一个运行时，版本天然匹配。
    if (HMODULE loaded = ::GetModuleHandleW(L"hostfxr.dll")) {
        LogLine(L"hostfxr: 使用进程内已加载的那一份");
        return loaded;
    }

    WCHAR programFiles[MAX_PATH]{};
    if (::GetEnvironmentVariableW(L"ProgramFiles", programFiles, MAX_PATH) == 0) {
        LogLine(L"hostfxr: 拿不到 ProgramFiles");
        return nullptr;
    }

    WCHAR pattern[MAX_PATH]{};
    lstrcpynW(pattern, programFiles, MAX_PATH);
    AppendText(pattern, L"\\dotnet\\host\\fxr\\*");

    WIN32_FIND_DATAW entry{};
    HANDLE search = ::FindFirstFileW(pattern, &entry);
    if (search == INVALID_HANDLE_VALUE) {
        LogLine(L"hostfxr: 找不到 dotnet\\host\\fxr 目录");
        return nullptr;
    }

    // 只在 8.x 里挑，取最高补丁号：宿主目标框架是 net8.0。
    int bestMajor = -1, bestMinor = -1, bestPatch = -1;
    WCHAR bestName[MAX_PATH]{};
    do {
        if ((entry.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY) == 0) continue;
        int major, minor, patch;
        ParseVersion(entry.cFileName, major, minor, patch);
        if (major != 8) continue;
        const bool better = bestMajor < 0
            || (major != bestMajor ? major > bestMajor
                                   : (minor != bestMinor ? minor > bestMinor : patch > bestPatch));
        if (better) {
            bestMajor = major; bestMinor = minor; bestPatch = patch;
            lstrcpynW(bestName, entry.cFileName, MAX_PATH);
        }
    } while (::FindNextFileW(search, &entry));
    ::FindClose(search);

    if (bestMajor < 0) {
        LogLine(L"hostfxr: 机器上没有 8.x 的 hostfxr");
        return nullptr;
    }

    WCHAR full[MAX_PATH]{};
    lstrcpynW(full, programFiles, MAX_PATH);
    AppendText(full, L"\\dotnet\\host\\fxr\\");
    AppendText(full, bestName);
    AppendText(full, L"\\hostfxr.dll");

    HMODULE module = ::LoadLibraryW(full);
    if (module) {
        WCHAR note[MAX_PATH + 32]{};
        lstrcpynW(note, L"hostfxr: 从磁盘加载 ", 512);
        lstrcpynW(note + lstrlenW(note), full, MAX_PATH);
        LogLine(note);
    } else {
        LogLine(L"hostfxr: 从磁盘加载失败");
    }
    return module;
}

// ---------- 载入托管入口 ----------

// 返回 0 成功。bridgeDir 是我们的目录，不是 BetterGI 的。
int32_t ResolveEntry(const WCHAR* bridgeDir, const WCHAR* methodName, void** out) {
    *out = nullptr;

    HMODULE hostfxr = LoadHostfxr();
    if (!hostfxr) return -1;

    auto initialize = reinterpret_cast<hostfxr_initialize_for_runtime_config_fn>(
        ::GetProcAddress(hostfxr, "hostfxr_initialize_for_runtime_config"));
    auto getDelegate = reinterpret_cast<hostfxr_get_runtime_delegate_fn>(
        ::GetProcAddress(hostfxr, "hostfxr_get_runtime_delegate"));
    auto close = reinterpret_cast<hostfxr_close_fn>(
        ::GetProcAddress(hostfxr, "hostfxr_close"));
    if (!initialize || !getDelegate || !close) {
        LogLine(L"hostfxr: 缺少需要的导出函数");
        return -2;
    }
    LogLine(L"hostfxr: 导出函数已解析");

    WCHAR runtimeConfig[MAX_PATH]{};
    lstrcpynW(runtimeConfig, bridgeDir, MAX_PATH);
    AppendText(runtimeConfig, L"\\BgiBridge.runtimeconfig.json");
    if (::GetFileAttributesW(runtimeConfig) == INVALID_FILE_ATTRIBUTES) {
        LogLine(L"找不到 BgiBridge.runtimeconfig.json");
        return -3;
    }

    hostfxr_handle context = nullptr;
    // 返回值 >= 0 都算成功：0=Success，1=Success_HostAlreadyInitialized，
    // 2=Success_DifferentRuntimeProperties。我们要的正是 1 —— 复用宿主已有的运行时。
    const int32_t initStatus = initialize(runtimeConfig, nullptr, &context);
    LogWithCode(L"hostfxr_initialize_for_runtime_config", initStatus);
    if (initStatus < 0) {
        if (context) close(context);
        return -4;
    }

    void* load = nullptr;
    const int32_t delegateStatus =
        getDelegate(context, hdt_load_assembly_and_get_function_pointer, &load);
    LogWithCode(L"hostfxr_get_runtime_delegate", delegateStatus);
    if (delegateStatus < 0 || !load) {
        // 无论成败都必须关闭上下文：被遗弃的上下文会永久阻塞后续初始化。
        close(context);
        return -5;
    }

    WCHAR assembly[MAX_PATH]{};
    lstrcpynW(assembly, bridgeDir, MAX_PATH);
    AppendText(assembly, L"\\BgiBridge.dll");

    WCHAR typeName[128]{};
    lstrcpynW(typeName, L"BgiBridge.Entry, BgiBridge", 128);

    void* function = nullptr;
    const int32_t loadStatus = reinterpret_cast<load_assembly_and_get_function_pointer_fn>(load)(
        assembly, typeName, methodName, unmanaged_callers_only, nullptr, &function);

    // 托管入口已经解析出来了，运行时上下文可以关掉；不影响已加载的程序集。
    close(context);

    LogWithCode(L"load_assembly_and_get_function_pointer", loadStatus);
    if (loadStatus < 0 || !function) return -6;

    *out = function;
    return 0;
}

// ---------- 线程体 ----------

DWORD WINAPI Worker(LPVOID) {
    LogLine(L"--- 引导开始 ---");

    WCHAR bridgeDir[MAX_PATH]{};
    lstrcpynW(bridgeDir, g_bridgeDir, MAX_PATH);
    if (bridgeDir[0] == 0) {
        LogLine(L"拿不到桥目录，放弃");
        return 1;
    }

    void* entry = nullptr;
    const int32_t status = ResolveEntry(bridgeDir, L"Start", &entry);
    if (status != 0 || !entry) {
        LogWithCode(L"解析托管入口失败", status);
        LogLine(L"--- 引导结束（失败）---");
        return 2;
    }

    // 只传桥目录这个纯路径。不包 JSON —— 托管侧少一层解析就少一个失败点，
    // 而且日志能在解析之前就接上，出问题时才看得见原因。
    LogLine(L"调用托管 Entry.Start");
    const int32_t startStatus = reinterpret_cast<bridge_start_fn>(entry)(bridgeDir);
    LogWithCode(L"托管 Entry.Start 返回", startStatus);
    LogLine(L"--- 引导结束 ---");
    return startStatus == 0 ? 0 : 3;
}

}  // namespace

// 入口就在这里。**不要**改成由注入器直接 CreateRemoteThread 调导出函数：
// 在启用了 CFG 的 .NET 宿主（BetterGI 就是）里，那种间接调用会以
// FAST_FAIL_GUARD_ICALL_CHECK_FAILURE (0xC0000409 / 参数 0xA) 失败并杀掉宿主进程。
// DllMain 是加载器调的，不受这条限制。实测：同样的 DLL，DllMain 能跑，导出调用必崩。
//
// DllMain 里可以 CreateThread，但不能 LoadLibrary / 等待其他线程——loader lock 会死锁。
// 所以这里只起线程，真正的活交给 Worker。
BOOL APIENTRY DllMain(HMODULE module, DWORD reason, LPVOID) {
    if (reason == DLL_PROCESS_ATTACH) {
        g_self = module;
        ::DisableThreadLibraryCalls(module);

        const DWORD length = ::GetModuleFileNameW(module, g_bridgeDir, MAX_PATH);
        for (DWORD i = length; i > 0; --i) {
            if (g_bridgeDir[i - 1] == L'\\' || g_bridgeDir[i - 1] == L'/') {
                g_bridgeDir[i - 1] = 0;
                break;
            }
        }

        HANDLE thread = ::CreateThread(nullptr, 0, Worker, nullptr, 0, nullptr);
        if (thread) {
            ::CloseHandle(thread);
        } else {
            LogLine(L"CreateThread 失败，引导无法继续");
        }
    }
    return TRUE;
}

// 保留导出：供宿主内其它代码显式释放端口。注入器不再远程调用它（见上面的原因）。
extern "C" __declspec(dllexport) DWORD WINAPI BgiBridgeShutdown(LPVOID) {
    if (g_bridgeDir[0] == 0) return 1;

    WCHAR bridgeDir[MAX_PATH]{};
    lstrcpynW(bridgeDir, g_bridgeDir, MAX_PATH);

    void* entry = nullptr;
    if (ResolveEntry(bridgeDir, L"Stop", &entry) != 0 || !entry) return 1;
    return static_cast<DWORD>(reinterpret_cast<bridge_stop_fn>(entry)());
}
