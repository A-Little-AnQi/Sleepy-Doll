// Only launches explicitly selected bridge test binaries, never a downloaded host.
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <filesystem>
#include <iostream>
#include <string>
#include <vector>

namespace fs = std::filesystem;

struct Handle {
    HANDLE value = nullptr;
    ~Handle() { if (value && value != INVALID_HANDLE_VALUE) CloseHandle(value); }
};

static std::wstring env(const wchar_t* name) {
    wchar_t buffer[32768]{};
    DWORD length = GetEnvironmentVariableW(name, buffer, 32768);
    if (!length || length >= 32768) throw std::runtime_error("missing environment");
    return {buffer, length};
}

static HANDLE duplicate(HANDLE original) {
    HANDLE copy = nullptr;
    if (!DuplicateHandle(GetCurrentProcess(), original, GetCurrentProcess(), &copy,
                         0, TRUE, DUPLICATE_SAME_ACCESS)) {
        throw std::runtime_error("cannot duplicate output handle");
    }
    return copy;
}

int wmain(int argc, wchar_t** argv) {
    SetErrorMode(SEM_FAILCRITICALERRORS | SEM_NOGPFAULTERRORBOX | SEM_NOOPENFILEERRORBOX);
    try {
        if (argc == 2 && std::wstring(argv[1]) == L"--isolation-probe") {
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION current{};
            if (!QueryInformationJobObject(nullptr, JobObjectExtendedLimitInformation,
                                           &current, sizeof(current), nullptr) ||
                current.BasicLimitInformation.ActiveProcessLimit != 1) {
                throw std::runtime_error("single-process Job is not active");
            }
            wchar_t desktop[256]{};
            DWORD needed = 0;
            if (!GetUserObjectInformationW(GetThreadDesktop(GetCurrentThreadId()), UOI_NAME,
                                            desktop, sizeof(desktop), &needed) ||
                std::wstring(desktop).find(L"SleepyDollAdapterValidation_") != 0) {
                throw std::runtime_error("test is not on the isolated desktop");
            }
            LPWCH block = GetEnvironmentStringsW();
            if (!block) throw std::runtime_error("cannot inspect test environment");
            bool clean = true;
            for (LPWCH item = block; *item; item += wcslen(item) + 1) {
                std::wstring entry(item);
                auto name = entry.substr(0, entry.find(L'='));
                if (name != L"SystemRoot" && name != L"WINDIR" && name != L"TEMP" && name != L"TMP") clean = false;
            }
            FreeEnvironmentStringsW(block);
            if (!clean) throw std::runtime_error("unexpected inherited environment variable");
            wchar_t self[32768]{};
            if (!GetModuleFileNameW(nullptr, self, 32768)) throw std::runtime_error("cannot find test helper");
            std::wstring command = L"\"" + std::wstring(self) + L"\" --child-must-be-blocked";
            STARTUPINFOW startup{sizeof(startup)};
            PROCESS_INFORMATION process{};
            if (CreateProcessW(self, command.data(), nullptr, nullptr, FALSE, CREATE_NO_WINDOW,
                               nullptr, nullptr, &startup, &process)) {
                TerminateProcess(process.hProcess, 2);
                WaitForSingleObject(process.hProcess, 5000);
                CloseHandle(process.hThread);
                CloseHandle(process.hProcess);
                throw std::runtime_error("Job permitted an extra process");
            }
            std::cout << "isolation probe passed: private desktop, clean environment, child process refused" << std::endl;
            return 0;
        }
        if (argc != 3) throw std::runtime_error("usage: native-runner <workspace> <test-exe>");
        bool selfTest = std::wstring(argv[1]) == L"--verify-isolation";
        fs::path root = fs::canonical(selfTest ? argv[2] : argv[1]);
        wchar_t self[32768]{};
        if (!GetModuleFileNameW(nullptr, self, 32768)) throw std::runtime_error("cannot locate runner");
        fs::path binary = fs::canonical(selfTest ? self : argv[2]);
        fs::path relative = binary.lexically_relative(root / L"target" / L"bridge");
        std::vector<std::wstring> parts;
        for (auto& part : relative) parts.push_back(part.native());
        if (!selfTest && (parts.size() != 2 || (parts[0] != L"zzz" && parts[0] != L"march7th" && parts[0] != L"maaend"))) {
            throw std::runtime_error("test binary outside bridge output allowlist");
        }
        std::wstring filename = binary.filename().native();
        for (auto& ch : filename) ch = static_cast<wchar_t>(towlower(ch));
        if (!selfTest && (filename.find(L"test") == std::wstring::npos || binary.extension() != L".exe" ||
            filename.find(L"inject") != std::wstring::npos)) {
            throw std::runtime_error("only test executables are allowed");
        }
        fs::path work = root / L"target" / L".tmp" / L"local-adapters";
        if (!fs::is_directory(work)) throw std::runtime_error("registered test directory missing");
        if (selfTest && binary != fs::canonical(work / L"native-runner.exe")) {
            throw std::runtime_error("self-test helper outside registered validation directory");
        }

        Handle job{CreateJobObjectW(nullptr, nullptr)};
        if (!job.value) throw std::runtime_error("CreateJobObject failed");
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION limits{};
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_ACTIVE_PROCESS |
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION |
            JOB_OBJECT_LIMIT_PROCESS_MEMORY;
        limits.BasicLimitInformation.ActiveProcessLimit = 1;
        limits.ProcessMemoryLimit = 512ull * 1024 * 1024;
        if (!SetInformationJobObject(job.value, JobObjectExtendedLimitInformation, &limits, sizeof(limits))) {
            throw std::runtime_error("cannot constrain job");
        }
        JOBOBJECT_BASIC_UI_RESTRICTIONS ui{};
        ui.UIRestrictionsClass = JOB_OBJECT_UILIMIT_HANDLES | JOB_OBJECT_UILIMIT_READCLIPBOARD |
            JOB_OBJECT_UILIMIT_WRITECLIPBOARD | JOB_OBJECT_UILIMIT_SYSTEMPARAMETERS |
            JOB_OBJECT_UILIMIT_DISPLAYSETTINGS | JOB_OBJECT_UILIMIT_GLOBALATOMS |
            JOB_OBJECT_UILIMIT_DESKTOP | JOB_OBJECT_UILIMIT_EXITWINDOWS;
        if (!SetInformationJobObject(job.value, JobObjectBasicUIRestrictions, &ui, sizeof(ui))) {
            throw std::runtime_error("cannot constrain job UI");
        }
        std::wstring desktopName = L"SleepyDollAdapterValidation_" + std::to_wstring(GetCurrentProcessId());
        HDESK desktop = CreateDesktopW(desktopName.c_str(), nullptr, nullptr, 0, GENERIC_ALL, nullptr);
        if (!desktop) throw std::runtime_error("cannot create isolated desktop");
        struct DesktopCleanup { HDESK h; ~DesktopCleanup() { CloseDesktop(h); } } desktopCleanup{desktop};

        std::wstring environment;
        auto add = [&](const std::wstring& key, const std::wstring& value) {
            environment += key + L"=" + value;
            environment.push_back(L'\0');
        };
        add(L"SystemRoot", env(L"SystemRoot"));
        add(L"TEMP", work.native());
        add(L"TMP", work.native());
        add(L"WINDIR", env(L"SystemRoot"));
        environment.push_back(L'\0');

        Handle output{duplicate(GetStdHandle(STD_OUTPUT_HANDLE))};
        Handle error{duplicate(GetStdHandle(STD_ERROR_HANDLE))};
        SECURITY_ATTRIBUTES security{sizeof(SECURITY_ATTRIBUTES), nullptr, TRUE};
        Handle input{CreateFileW(L"NUL", GENERIC_READ, FILE_SHARE_READ | FILE_SHARE_WRITE,
                                 &security, OPEN_EXISTING, 0, nullptr)};
        STARTUPINFOW startup{sizeof(startup)};
        startup.lpDesktop = desktopName.data();
        startup.dwFlags = STARTF_USESTDHANDLES;
        startup.hStdInput = input.value;
        startup.hStdOutput = output.value;
        startup.hStdError = error.value;
        PROCESS_INFORMATION process{};
        std::wstring command = L"\"" + binary.native() + L"\"";
        if (selfTest) command += L" --isolation-probe";
        if (!CreateProcessW(binary.c_str(), command.data(), nullptr, nullptr, TRUE,
                            CREATE_SUSPENDED | CREATE_NO_WINDOW | CREATE_UNICODE_ENVIRONMENT,
                            environment.data(), work.c_str(), &startup, &process)) {
            std::cerr << "CreateProcess error=" << GetLastError() << '\n';
            throw std::runtime_error("cannot launch isolated test");
        }
        Handle child{process.hProcess};
        Handle thread{process.hThread};
        if (!AssignProcessToJobObject(job.value, child.value)) {
            TerminateProcess(child.value, 2);
            WaitForSingleObject(child.value, 5000);
            throw std::runtime_error("cannot attach test to job");
        }
        std::cout << "isolated test pid=" << process.dwProcessId
                  << " desktop-owner-pid=" << GetCurrentProcessId() << std::endl;
        if (ResumeThread(thread.value) == static_cast<DWORD>(-1)) {
            throw std::runtime_error("cannot resume test");
        }
        DWORD wait = WaitForSingleObject(child.value, 60000);
        if (wait != WAIT_OBJECT_0) {
            TerminateJobObject(job.value, 2);
            WaitForSingleObject(child.value, 5000);
            throw std::runtime_error("test failed or exceeded 60 seconds");
        }
        DWORD code = 2;
        if (!GetExitCodeProcess(child.value, &code)) throw std::runtime_error("cannot read exit code");
        return static_cast<int>(code);
    } catch (const std::exception& error) {
        std::cerr << "native validation refused: " << error.what() << '\n';
        return 2;
    }
}
