// BgiBridge 引导 DLL：被注入后用 hostfxr 接上宿主已有的 .NET 运行时，
// 载入托管控制面。
//
// 入口必须在 DllMain 起的线程里，不能靠注入器远程调导出函数——
// 详见 injector.cpp 顶部的说明。
//
// 只用 kernel32。

#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <cstdint>

namespace {

// 本 DLL 自己的模块句柄与目录，DllMain 里填。
// 不能用 GetModuleFileName(nullptr)：那拿到的是宿主进程 exe 的路径。
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

// ---------- 数据根 ----------

// 运行期数据的落点。交付包里本 DLL 住在 <安装目录>\bridge 下，
// 数据根是 <安装目录>\user，由 bridge.config.json 的 userDirectory 指出
// （Sleepy Doll 每次连接时写入）；没有这个字段就用桥目录下的 user\，
// 开发构建与旧版平铺包正是这样。
WCHAR g_dataRoot[MAX_PATH]{};

// bridge.config.json 是 UTF-8，只用 kernel32，所以按字节扫这一个键。
// 路径里只会出现 \\ \" \/ 三种转义，其余原样搬。
bool FindJsonString(const char* text, DWORD size, const char* key, WCHAR* out, int capacity) {
    const int keyLength = lstrlenA(key);
    for (DWORD at = 0; at + static_cast<DWORD>(keyLength) < size; ++at) {
        bool match = true;
        for (int i = 0; i < keyLength; ++i) {
            if (text[at + i] != key[i]) {
                match = false;
                break;
            }
        }
        if (!match) continue;

        DWORD cursor = at + static_cast<DWORD>(keyLength);
        while (cursor < size && (text[cursor] == ' ' || text[cursor] == '\t')) ++cursor;
        if (cursor >= size || text[cursor] != ':') continue;
        ++cursor;
        while (cursor < size && (text[cursor] == ' ' || text[cursor] == '\t')) ++cursor;
        if (cursor >= size || text[cursor] != '"') continue;
        ++cursor;

        char value[MAX_PATH * 4]{};
        int length = 0;
        for (; cursor < size && text[cursor] != '"'; ++cursor) {
            if (length >= static_cast<int>(sizeof(value)) - 1) return false;
            if (text[cursor] == '\\' && cursor + 1 < size) {
                const char escaped = text[cursor + 1];
                if (escaped == '\\' || escaped == '"' || escaped == '/') {
                    value[length++] = escaped;
                    ++cursor;
                    continue;
                }
            }
            value[length++] = text[cursor];
        }
        if (cursor >= size || length == 0) return false;
        // 显式长度不写结束符，自己补一个：后面全靠 C 字符串处理。
        const int written = ::MultiByteToWideChar(CP_UTF8, 0, value, length, out, capacity - 1);
        if (written <= 0) return false;
        out[written] = 0;
        return true;
    }
    return false;
}

// 相对路径随宿主进程的当前目录变化。
bool AbsolutePath(const WCHAR* path) {
    if (path[0] == 0) return false;
    if (path[0] == L'\\' && path[1] == L'\\') return true;
    return path[1] == L':' && (path[2] == L'\\' || path[2] == L'/');
}

// 读不到就返回 false，由调用方用缺省。
bool ReadConfiguredDataRoot(WCHAR* out, int capacity) {
    WCHAR path[MAX_PATH + 32]{};
    lstrcpynW(path, g_bridgeDir, MAX_PATH);
    AppendText(path, L"\\bridge.config.json");

    // 允许删除共享：Sleepy Doll 重写这份配置时是整份替换。
    HANDLE file = ::CreateFileW(path, GENERIC_READ,
                                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE, nullptr,
                                OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, nullptr);
    if (file == INVALID_HANDLE_VALUE) return false;

    // 这份配置只有几 KB；读不满说明它大得反常，退回缺省。
    static char text[32768];
    DWORD size = 0;
    const BOOL read = ::ReadFile(file, text, static_cast<DWORD>(sizeof(text)), &size, nullptr);
    ::CloseHandle(file);
    if (!read || size == 0) return false;

    return FindJsonString(text, size, "\"userDirectory\"", out, capacity) && AbsolutePath(out);
}

// 只在 Worker 线程里调用：DllMain 里不能做文件 I/O。
void ResolveDataRoot() {
    if (g_bridgeDir[0] == 0) return;
    if (ReadConfiguredDataRoot(g_dataRoot, MAX_PATH)) return;

    lstrcpynW(g_dataRoot, g_bridgeDir, MAX_PATH);
    AppendText(g_dataRoot, L"\\user");
}

// ---------- 日志 ----------

// 日志落在数据根的 log 下。只用 kernel32，所以逐级建目录。
void EnsureLogDirectory() {
    static bool created = false;
    if (created) return;
    created = true;

    ::CreateDirectoryW(g_dataRoot, nullptr);
    WCHAR path[MAX_PATH + 32]{};
    lstrcpynW(path, g_dataRoot, MAX_PATH);
    AppendText(path, L"\\log");
    ::CreateDirectoryW(path, nullptr);
}

bool AppendLine(const WCHAR* path, const WCHAR* line) {
    HANDLE file = ::CreateFileW(path, FILE_APPEND_DATA, FILE_SHARE_READ | FILE_SHARE_WRITE,
                                nullptr, OPEN_ALWAYS, FILE_ATTRIBUTE_NORMAL, nullptr);
    if (file == INVALID_HANDLE_VALUE) return false;

    DWORD written = 0;
    const BOOL ok =
        ::WriteFile(file, line, static_cast<DWORD>(lstrlenW(line)) * sizeof(WCHAR), &written, nullptr);
    ::CloseHandle(file);
    return ok != 0;
}

void LogLine(const WCHAR* text) {
    if (g_dataRoot[0] == 0) return;

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

    EnsureLogDirectory();
    WCHAR path[MAX_PATH + 32]{};
    lstrcpynW(path, g_dataRoot, MAX_PATH);
    AppendText(path, L"\\log\\bootstrap.log");
    if (AppendLine(path, line)) return;

    // 数据根写不出去（只读安装等）时退回 %TEMP%。
    WCHAR temp[MAX_PATH]{};
    const DWORD length = ::GetTempPathW(MAX_PATH, temp);
    if (length == 0 || length >= MAX_PATH) return;
    lstrcpynW(path, temp, MAX_PATH);
    AppendText(path, L"bgi-bridge-bootstrap.log");
    AppendLine(path, line);
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

// hostfxr_delegate_type 里只用这一个
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

// 返回 0 成功。bridgeDir 是桥目录，不是 BetterGI 的。
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
    // 2=Success_DifferentRuntimeProperties。复用宿主已有的运行时走的是 1。
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
    // 定下数据根。文件 I/O 只能在这里做：DllMain 里会卡在 loader lock 上。
    ResolveDataRoot();

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

    // 只传桥目录这个纯路径。
    LogLine(L"调用托管 Entry.Start");
    const int32_t startStatus = reinterpret_cast<bridge_start_fn>(entry)(bridgeDir);
    LogWithCode(L"托管 Entry.Start 返回", startStatus);
    LogLine(L"--- 引导结束 ---");
    return startStatus == 0 ? 0 : 3;
}

}  // namespace

// 入口就在这里。不要改成由注入器直接 CreateRemoteThread 调导出函数：
// 在启用了 CFG 的 .NET 宿主（BetterGI 就是）里，那种间接调用会以
// FAST_FAIL_GUARD_ICALL_CHECK_FAILURE (0xC0000409 / 参数 0xA) 失败并杀掉宿主进程。
// DllMain 是加载器调用的，不受这条限制。实测：同样的 DLL，DllMain 可用，导出调用必崩。
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

// 保留导出：供宿主内其它代码显式释放端口。注入器不再远程调用它（原因见 injector.cpp）。
extern "C" __declspec(dllexport) DWORD WINAPI BgiBridgeShutdown(LPVOID) {
    if (g_bridgeDir[0] == 0) return 1;

    WCHAR bridgeDir[MAX_PATH]{};
    lstrcpynW(bridgeDir, g_bridgeDir, MAX_PATH);

    void* entry = nullptr;
    if (ResolveEntry(bridgeDir, L"Stop", &entry) != 0 || !entry) return 1;
    return static_cast<DWORD>(reinterpret_cast<bridge_stop_fn>(entry)());
}
