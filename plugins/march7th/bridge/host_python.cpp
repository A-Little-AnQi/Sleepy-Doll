// 三月七原生桥：CPython C API 动态绑定与宿主反射。
//
// 不依赖 Python 头文件：通过 GetModuleHandle 在宿主进程内找到 python3*.dll，
// GetProcAddress 绑定所需 C API。所有规则：
// - 绝不 import 三月七业务模块（app/module/utils.tasks 只从已加载的模块表里查找）；
//   gc 是解释器内置模块，允许兜底导入；
// - 绝不构造宿主对象，只在已存在的 MainWindow/logInterface/cfg 上反射；
// - 触碰 Qt 的操作（isTaskRunning/startTask/stopTask/toPlainText/set_value）
//   一律经宿主已有 Qt context 的 singleShot 队列派发到 GUI 线程执行；
// - 纯 Python 字典快照（module.config 的 config dict）在工作线程持 GIL 读取；
// - 不使用 PyRun/eval/exec，不嵌任何 Python 脚本文本。
#include "http_server.hpp"

#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>
#include <tlhelp32.h>

#include <atomic>
#include <cstdio>
#include <functional>
#include <memory>
#include <mutex>
#include <random>
#include <vector>

namespace sdm::march7th {

namespace {

using winrt::Windows::Data::Json::JsonValueType;

// ---- CPython C API 动态绑定（无头文件） --------------------------------------

struct PyObjectOpaque;
using PyObjectT = PyObjectOpaque;
using PySsizeT = int64_t;
using NativeCallable = PyObjectT* (*)(PyObjectT*, PyObjectT*);
struct NativeMethodDef {
    const char* name;
    NativeCallable method;
    int flags;
    const char* documentation;
};

struct PythonApi {
    HMODULE module = nullptr;
    int (*IsInitialized)() = nullptr;
    int (*IsFinalizing)() = nullptr;
    int (*GilStateEnsure)() = nullptr;
    void (*GilStateRelease)(int) = nullptr;
    int (*AddPendingCall)(int (*)(void*), void*) = nullptr;
    PyObjectT* (*ImportGetModuleDict)() = nullptr;
    PyObjectT* (*ImportImportModule)(const char*) = nullptr;
    PyObjectT* (*DictGetItemString)(PyObjectT*, const char*) = nullptr;
    PyObjectT* (*DictKeys)(PyObjectT*) = nullptr;
    PySsizeT (*ListSize)(PyObjectT*) = nullptr;
    PyObjectT* (*ListGetItem)(PyObjectT*, PySsizeT) = nullptr;
    PySsizeT (*TupleSize)(PyObjectT*) = nullptr;
    PyObjectT* (*TupleGetItem)(PyObjectT*, PySsizeT) = nullptr;
    PyObjectT* (*GetAttrString)(PyObjectT*, const char*) = nullptr;
    PyObjectT* (*CallNoArgs)(PyObjectT*) = nullptr;
    // 单参调用可能是 Vectorcall 或 CallFunctionObjArgs 兜底，用 std::function 承载。
    std::function<PyObjectT*(PyObjectT*, PyObjectT*)> CallOneArg;
    int (*CallableCheck)(PyObjectT*) = nullptr;
    int (*IsInstance)(PyObjectT*, PyObjectT*) = nullptr;
    PyObjectT* (*ObjectType)(PyObjectT*) = nullptr;
    PyObjectT* boolType = nullptr;
    PyObjectT* intType = nullptr;
    PyObjectT* floatType = nullptr;
    PyObjectT* strType = nullptr;
    PyObjectT* listType = nullptr;
    PyObjectT* tupleType = nullptr;
    PyObjectT* dictType = nullptr;
    PyObjectT* (*UnicodeFromString)(const char*) = nullptr;
    const char* (*UnicodeAsUtf8)(PyObjectT*) = nullptr;
    PyObjectT* (*LongFromLongLong)(long long) = nullptr;
    long long (*LongAsLongLong)(PyObjectT*) = nullptr;
    double (*FloatAsDouble)(PyObjectT*) = nullptr;
    PyObjectT* (*BoolFromLong)(long) = nullptr;
    int (*IsTrue)(PyObjectT*) = nullptr;
    void (*IncRef)(PyObjectT*) = nullptr;
    void (*DecRef)(PyObjectT*) = nullptr;
    PyObjectT* (*ErrOccurred)() = nullptr;
    void (*ErrClear)() = nullptr;
    PyObjectT* (*CFunctionNew)(NativeMethodDef*, PyObjectT*, PyObjectT*) = nullptr;
    PyObjectT* (*CapsuleNew)(void*, const char*, void (*)(PyObjectT*)) = nullptr;
    void* (*CapsulePointer)(PyObjectT*, const char*) = nullptr;
    PyObjectT* (*CallArgs)(PyObjectT*, ...) = nullptr;

    bool ok() const {
        return module && IsInitialized && GilStateEnsure && GilStateRelease && AddPendingCall &&
               ImportGetModuleDict && ImportImportModule && DictGetItemString && DictKeys &&
               ListSize && ListGetItem && GetAttrString && CallNoArgs && CallOneArg &&
               CallableCheck && UnicodeFromString && UnicodeAsUtf8 && LongFromLongLong &&
               LongAsLongLong && FloatAsDouble && BoolFromLong && IsTrue && IncRef && DecRef &&
               ErrOccurred && ErrClear && CFunctionNew && CapsuleNew && CapsulePointer && CallArgs &&
               IsInstance && ObjectType && boolType && intType && floatType && strType && listType && tupleType && dictType;
    }
};

PythonApi g_py;

HMODULE FindPythonModule() {
    // 在本进程（即宿主进程）已加载模块中找 python3*.dll。
    HANDLE snapshot = ::CreateToolhelp32Snapshot(TH32CS_SNAPMODULE, ::GetCurrentProcessId());
    if (snapshot == INVALID_HANDLE_VALUE) {
        return nullptr;
    }
    HMODULE found = nullptr;
    HMODULE stableAbi = nullptr;
    MODULEENTRY32W entry{};
    entry.dwSize = sizeof(entry);
    if (::Module32FirstW(snapshot, &entry)) {
        do {
            const wchar_t* name = entry.szModule;
            size_t length = wcslen(name);
            if (length > 4 && _wcsnicmp(name, L"python3", 7) == 0 &&
                _wcsicmp(name + length - 4, L".dll") == 0) {
                if (_wcsicmp(name, L"python3.dll") == 0) {
                    stableAbi = entry.hModule;
                    continue;
                }
                found = entry.hModule;
                break;
            }
        } while (::Module32NextW(snapshot, &entry));
    }
    ::CloseHandle(snapshot);
    return found ? found : stableAbi;
}

template <typename Fn>
bool Bind(HMODULE module, const char* name, Fn& target) {
    target = reinterpret_cast<Fn>(::GetProcAddress(module, name));
    return target != nullptr;
}

bool BindPythonApi(std::string& error) {
    static bool bound = false;
    if (bound) {
        if (!g_py.IsInitialized() || (g_py.IsFinalizing && g_py.IsFinalizing())) {
            error = "宿主解释器未就绪或正在退出";
            return false;
        }
        return true;
    }
    g_py = PythonApi{};
    g_py.module = FindPythonModule();
    if (g_py.module == nullptr) {
        error = "宿主进程内未找到 python3*.dll（Python C API 不可见）";
        return false;
    }
    HMODULE m = g_py.module;
    Bind(m, "Py_IsFinalizing", g_py.IsFinalizing);
    if (!g_py.IsFinalizing) Bind(m, "_Py_IsFinalizing", g_py.IsFinalizing);
    bool ok = Bind(m, "Py_IsInitialized", g_py.IsInitialized) &&
              Bind(m, "PyGILState_Ensure", g_py.GilStateEnsure) &&
              Bind(m, "PyGILState_Release", g_py.GilStateRelease) &&
              Bind(m, "Py_AddPendingCall", g_py.AddPendingCall) &&
              Bind(m, "PyImport_GetModuleDict", g_py.ImportGetModuleDict) &&
              Bind(m, "PyImport_ImportModule", g_py.ImportImportModule) &&
              Bind(m, "PyDict_GetItemString", g_py.DictGetItemString) &&
              Bind(m, "PyDict_Keys", g_py.DictKeys) &&
              Bind(m, "PyList_Size", g_py.ListSize) &&
              Bind(m, "PyList_GetItem", g_py.ListGetItem) &&
              Bind(m, "PyObject_GetAttrString", g_py.GetAttrString) &&
              Bind(m, "PyObject_CallNoArgs", g_py.CallNoArgs) &&
              Bind(m, "PyCallable_Check", g_py.CallableCheck) &&
              Bind(m, "PyObject_IsInstance", g_py.IsInstance) &&
              Bind(m, "PyObject_Type", g_py.ObjectType) &&
              Bind(m, "PyUnicode_FromString", g_py.UnicodeFromString) &&
              Bind(m, "PyUnicode_AsUTF8", g_py.UnicodeAsUtf8) &&
              Bind(m, "PyLong_FromLongLong", g_py.LongFromLongLong) &&
              Bind(m, "PyLong_AsLongLong", g_py.LongAsLongLong) &&
              Bind(m, "PyFloat_AsDouble", g_py.FloatAsDouble) &&
              Bind(m, "PyBool_FromLong", g_py.BoolFromLong) &&
              Bind(m, "PyObject_IsTrue", g_py.IsTrue) &&
              Bind(m, "Py_IncRef", g_py.IncRef) &&
              Bind(m, "Py_DecRef", g_py.DecRef) &&
              Bind(m, "PyErr_Occurred", g_py.ErrOccurred) &&
              Bind(m, "PyErr_Clear", g_py.ErrClear) &&
              Bind(m, "PyCFunction_NewEx", g_py.CFunctionNew) &&
              Bind(m, "PyCapsule_New", g_py.CapsuleNew) &&
              Bind(m, "PyCapsule_GetPointer", g_py.CapsulePointer) &&
              Bind(m, "PyObject_CallFunctionObjArgs", g_py.CallArgs);
    if (!ok) {
        error = "python3*.dll 缺少所需 C API 导出";
        return false;
    }
    Bind(m, "PyTuple_Size", g_py.TupleSize);
    Bind(m, "PyTuple_GetItem", g_py.TupleGetItem);
    g_py.boolType = reinterpret_cast<PyObjectT*>(::GetProcAddress(m, "PyBool_Type"));
    g_py.intType = reinterpret_cast<PyObjectT*>(::GetProcAddress(m, "PyLong_Type"));
    g_py.floatType = reinterpret_cast<PyObjectT*>(::GetProcAddress(m, "PyFloat_Type"));
    g_py.strType = reinterpret_cast<PyObjectT*>(::GetProcAddress(m, "PyUnicode_Type"));
    g_py.listType = reinterpret_cast<PyObjectT*>(::GetProcAddress(m, "PyList_Type"));
    g_py.tupleType = reinterpret_cast<PyObjectT*>(::GetProcAddress(m, "PyTuple_Type"));
    g_py.dictType = reinterpret_cast<PyObjectT*>(::GetProcAddress(m, "PyDict_Type"));
    // 单参调用：优先 PyObject_Vectorcall(3.9+)，否则用 CallFunctionObjArgs 兜底。
    using VectorcallFn = PyObjectT* (*)(PyObjectT*, PyObjectT* const*, size_t, PyObjectT*);
    VectorcallFn vectorcall =
        reinterpret_cast<VectorcallFn>(::GetProcAddress(m, "PyObject_Vectorcall"));
    using VarArgsFn = PyObjectT* (*)(PyObjectT*, ...);
    VarArgsFn varArgs =
        reinterpret_cast<VarArgsFn>(::GetProcAddress(m, "PyObject_CallFunctionObjArgs"));
    if (vectorcall != nullptr) {
        g_py.CallOneArg = [vectorcall](PyObjectT* callable, PyObjectT* arg) -> PyObjectT* {
            PyObjectT* args[1] = {arg};
            return vectorcall(callable, args, 1, nullptr);
        };
    } else if (varArgs != nullptr) {
        g_py.CallOneArg = [varArgs](PyObjectT* callable, PyObjectT* arg) -> PyObjectT* {
            return varArgs(callable, arg, nullptr);
        };
    } else {
        error = "python3*.dll 缺少可用的调用入口";
        return false;
    }
    if (!g_py.ok()) {
        error = "python3*.dll C API 绑定不完整";
        return false;
    }
    if (!g_py.IsInitialized() || (g_py.IsFinalizing && g_py.IsFinalizing())) {
        error = "宿主解释器未就绪或正在退出";
        return false;
    }
    bound = true;
    return true;
}

// ---- 引用与基础助手 -----------------------------------------------------------

struct PyRef {
    PyObjectT* obj = nullptr;
    explicit PyRef(PyObjectT* o = nullptr) : obj(o) {}
    ~PyRef() { Reset(); }
    PyRef(const PyRef&) = delete;
    PyRef& operator=(const PyRef&) = delete;
    PyRef(PyRef&& other) noexcept : obj(other.obj) { other.obj = nullptr; }
    PyRef& operator=(PyRef&& other) noexcept {
        if (this != &other) {
            Reset();
            obj = other.obj;
            other.obj = nullptr;
        }
        return *this;
    }
    void Reset() {
        if (obj != nullptr) {
            g_py.DecRef(obj);
            obj = nullptr;
        }
    }
    PyObjectT* Get() const { return obj; }
    explicit operator bool() const { return obj != nullptr; }
};

struct GilGuard {
    int state = 0;
    explicit GilGuard() { state = g_py.GilStateEnsure(); }
    ~GilGuard() { g_py.GilStateRelease(state); }
};

PyRef Attr(PyObjectT* owner, const char* name) {
    return PyRef(g_py.GetAttrString(owner, name));
}

void ClearPythonError() {
    if (g_py.ErrOccurred() != nullptr) {
        g_py.ErrClear();
    }
}

std::string Utf8Of(PyObjectT* unicode) {
    if (unicode == nullptr) {
        return {};
    }
    const char* text = g_py.UnicodeAsUtf8(unicode);
    if (text == nullptr) {
        ClearPythonError();
        return {};
    }
    return text;
}

std::string TypeName(PyObjectT* obj) {
    if (obj == nullptr) {
        return "null";
    }
    // ruamel.yaml 的 CommentedMap/CommentedSeq/ScalarString 等继承内置类型。
    // 通过公共 C API 判断继承关系，不能把真实配置误拒绝为“不是 dict”。
    const std::pair<PyObjectT*, const char*> builtins[] = {
        {g_py.boolType, "bool"}, {g_py.intType, "int"}, {g_py.floatType, "float"},
        {g_py.strType, "str"}, {g_py.listType, "list"}, {g_py.tupleType, "tuple"},
        {g_py.dictType, "dict"},
    };
    for (const auto& type : builtins) {
        int matched = g_py.IsInstance(obj, type.first);
        if (matched == 1) return type.second;
        if (matched < 0) ClearPythonError();
    }
    PyRef cls(g_py.ObjectType(obj));
    if (!cls) {
        ClearPythonError();
        return "?";
    }
    PyRef name = Attr(cls.Get(), "__name__");
    if (!name) {
        ClearPythonError();
        return "?";
    }
    std::string result = Utf8Of(name.Get());
    return result.empty() ? "?" : result;
}

PyRef CallNoArgs(PyObjectT* callable) { return PyRef(g_py.CallNoArgs(callable)); }

PyRef CallOneArg(PyObjectT* callable, PyObjectT* arg) {
    return PyRef(g_py.CallOneArg(callable, arg));
}

// 绑定方法 __code__ 的参数元数据（等价于 inspect.signature 的最小替代）。
std::optional<int> MethodArgCount(PyObjectT* method) {
    PyRef code = Attr(method, "__code__");
    if (!code) {
        ClearPythonError();
        return std::nullopt;
    }
    PyRef count = Attr(code.Get(), "co_argcount");
    if (!count) {
        ClearPythonError();
        return std::nullopt;
    }
    long long value = g_py.LongAsLongLong(count.Get());
    if (g_py.ErrOccurred() != nullptr) {
        ClearPythonError();
        return std::nullopt;
    }
    return int(value);
}

std::optional<std::vector<std::string>> MethodParamNames(PyObjectT* method) {
    if (g_py.TupleSize == nullptr || g_py.TupleGetItem == nullptr) {
        return std::nullopt;
    }
    PyRef code = Attr(method, "__code__");
    if (!code) {
        ClearPythonError();
        return std::nullopt;
    }
    PyRef varnames = Attr(code.Get(), "co_varnames");
    if (!varnames) {
        ClearPythonError();
        return std::nullopt;
    }
    std::vector<std::string> names;
    PySsizeT size = g_py.TupleSize(varnames.Get());
    for (PySsizeT i = 0; i < size; ++i) {
        PyObjectT* item = g_py.TupleGetItem(varnames.Get(), i);
        if (item == nullptr) {
            ClearPythonError();
            return std::nullopt;
        }
        names.push_back(Utf8Of(item));
    }
    return names;
}

// ---- 主线程派发（Py_AddPendingCall + 取消/超时语义） -----------------------------

void EnsureTrampolineApartment() {
    // 主线程可能未初始化 COM 套间；失败（已初始化为其它套间）时忽略。
    thread_local bool initialized = [] {
        try {
            winrt::init_apartment(winrt::apartment_type::multi_threaded);
        } catch (...) {
        }
        return true;
    }();
    (void)initialized;
}

struct PendingClosure {
    std::function<void()> run;
};

constexpr const char* kQtCallbackCapsule = "SleepyDoll.March7th.Dispatch";
PyObjectT* g_qtContext = nullptr;
PyObjectT* g_qtSingleShot = nullptr;

void DestroyQtClosure(PyObjectT* capsule) noexcept {
    auto* closure = static_cast<PendingClosure*>(g_py.CapsulePointer(capsule, kQtCallbackCapsule));
    if (closure) delete closure;
    else ClearPythonError();
}

PyObjectT* QtClosureCallback(PyObjectT* capsule, PyObjectT*) noexcept {
    try {
        auto* closure = static_cast<PendingClosure*>(g_py.CapsulePointer(capsule, kQtCallbackCapsule));
        if (closure && closure->run) {
            auto run = std::move(closure->run);
            run();
        }
    } catch (...) {
    }
    ClearPythonError();
    return g_py.BoolFromLong(0); // 新引用；Qt 忽略回调返回值。
}

NativeMethodDef g_qtMethod{"_sleepy_march7th_dispatch", QtClosureCallback, 0x0004, nullptr};

// 调用方持 GIL；只取得现存 QApplication，不构造应用、不执行脚本文本。
bool EnsureQtPoster(std::string& error) {
    if (g_qtContext && g_qtSingleShot) return true;
    PyObjectT* modules = g_py.ImportGetModuleDict();
    PyObjectT* core = g_py.DictGetItemString(modules, "PySide6.QtCore");
    PyObjectT* widgets = g_py.DictGetItemString(modules, "PySide6.QtWidgets");
    if (!core || !widgets) {
        error = "宿主尚未加载 PySide6 GUI";
        return false;
    }
    PyRef timer = Attr(core, "QTimer");
    PyRef appClass = Attr(widgets, "QApplication");
    PyRef single = timer ? Attr(timer.Get(), "singleShot") : PyRef{};
    PyRef instanceMethod = appClass ? Attr(appClass.Get(), "instance") : PyRef{};
    PyRef app = instanceMethod ? PyRef(g_py.CallNoArgs(instanceMethod.Get())) : PyRef{};
    if (!single || !app || TypeName(app.Get()) == "NoneType") {
        ClearPythonError();
        error = "宿主没有可用的 QApplication 调度上下文";
        return false;
    }
    // 与已 pin 的 DLL 同寿命，退出阶段不从 CRT 析构器调用 Python。
    g_py.IncRef(app.Get());
    g_py.IncRef(single.Get());
    g_qtContext = app.Get();
    g_qtSingleShot = single.Get();
    return true;
}

AsyncCall::Poster MainThreadPoster(uint32_t budgetMs) {
    return [budgetMs](std::function<void()> run) -> bool {
        DWORD started = ::GetTickCount();
        try {
            if (!g_py.IsInitialized() || (g_py.IsFinalizing && g_py.IsFinalizing())) return false;
            GilGuard gil;
            if (::GetTickCount() - started >= budgetMs) return false;
            std::string error;
            if (!EnsureQtPoster(error)) return false;
            auto closure = std::make_unique<PendingClosure>(PendingClosure{std::move(run)});
            PyRef capsule(g_py.CapsuleNew(closure.get(), kQtCallbackCapsule, DestroyQtClosure));
            if (!capsule) { ClearPythonError(); return false; }
            closure.release(); // capsule 独占；Qt 丢弃回调时也会释放闭包。
            PyRef callback(g_py.CFunctionNew(&g_qtMethod, capsule.Get(), nullptr));
            PyRef zero(g_py.LongFromLongLong(0));
            if (!callback || !zero) { ClearPythonError(); return false; }
            PyRef posted(g_py.CallArgs(g_qtSingleShot, zero.Get(), g_qtContext, callback.Get(), nullptr));
            if (!posted) { ClearPythonError(); return false; }
            return true;
        } catch (...) {
            return false;
        }
    };
}

// 启动确认轮询参数：等待发生在 HTTP 工作线程，不阻塞 GUI 主线程。
constexpr int kStartConfirmAttempts = 6;
constexpr uint32_t kStartConfirmIntervalMs = 50;

// 把 body 派发到 CPython 主线程（= Qt GUI 线程）并等待。返回 true 仅当 body
// 完整执行且未抛异常；排队失败/超时/body 异常都填 err 并返回 false，绝不把
// 旧结果冒充成本次结果。body 内的错误经 RequestState 带回。
bool RunHostJob(std::function<void()> body, uint32_t timeoutMs, OpError& err) {
    DWORD started = ::GetTickCount();
    AsyncCall call([captured = std::move(body)] {
        EnsureTrampolineApartment();
        captured(); // 主线程已持 GIL，可直接调 C API
    });
    if (!call.Enqueue(MainThreadPoster(timeoutMs))) {
        err.httpStatus = 503;
        err.code = "dispatch_failed";
        err.message = "解释器忙碌，任务未能排入主线程";
        return false;
    }
    DWORD elapsed = ::GetTickCount() - started;
    DispatchStatus status = call.Wait(elapsed < timeoutMs ? timeoutMs - elapsed : 0);
    switch (status) {
        case DispatchStatus::Completed:
            if (!call.failure().empty()) {
                err.httpStatus = 500;
                err.code = "host_exception";
                err.message = "宿主主线程任务异常：" + call.failure();
                return false;
            }
            return true;
        case DispatchStatus::ScheduleFailed:
            err.httpStatus = 503;
            err.code = "dispatch_failed";
            err.message = "解释器忙碌，任务未能排入主线程";
            return false;
        case DispatchStatus::TimeoutNotStarted:
            err.httpStatus = 503;
            err.code = "dispatch_timeout";
            err.message = "宿主主线程在超时内未执行派发任务（已撤销，不会执行）";
            return false;
        case DispatchStatus::TimeoutStillRunning:
        default:
            err.httpStatus = 503;
            err.code = "dispatch_unknown";
            err.message = "宿主主线程仍在执行派发任务，结果未知；请稍后用 status 核实";
            return false;
    }
}

// ---- Python 值 → JSON + 规范标量 ----------------------------------------------

// Python 值 → JSON + 规范标量。buildJson=false 时只计算规范标量（用于
// 主线程闭包内的指纹复核，避免在 GUI 线程构造 WinRT 对象）。
bool PyValueToJson(PyObjectT* value, int depth, bool buildJson, JsonValue& out,
                   std::string& canonical) {
    using winrt::Windows::Data::Json::JsonValueType;
    std::string typeName = TypeName(value);
    if (typeName == "bool") {
        int truth = g_py.IsTrue(value);
        if (buildJson) {
            out = JsonValue::CreateBooleanValue(truth == 1);
        }
        canonical = truth == 1 ? "b:1" : "b:0";
        return true;
    }
    if (typeName == "int") {
        long long number = g_py.LongAsLongLong(value);
        if (g_py.ErrOccurred() != nullptr) {
            ClearPythonError();
            double wide = g_py.FloatAsDouble(value);
            ClearPythonError();
            if (buildJson) {
                out = JsonValue::CreateNumberValue(wide);
            }
            canonical = "f:" + std::to_string(wide);
            return true;
        }
        if (buildJson) {
            out = JsonValue::CreateNumberValue(double(number));
        }
        canonical = "i:" + std::to_string(number);
        return true;
    }
    if (typeName == "float") {
        double number = g_py.FloatAsDouble(value);
        if (g_py.ErrOccurred() != nullptr) {
            ClearPythonError();
            if (buildJson) {
                out = JsonValue::CreateNullValue();
            }
            canonical = "f:?";
            return true;
        }
        char buffer[64];
        snprintf(buffer, sizeof(buffer), "%.17g", number);
        if (buildJson) {
            out = JsonValue::CreateNumberValue(number);
        }
        canonical = std::string("f:") + buffer;
        return true;
    }
    if (typeName == "str") {
        std::string text = Utf8Of(value);
        if (buildJson) {
            out = JsonValue::CreateStringValue(winrt::to_hstring(text));
        }
        canonical = "s:" + std::to_string(text.size()) + ":" + text;
        return true;
    }
    if (typeName == "NoneType") {
        if (buildJson) {
            out = JsonValue::CreateNullValue();
        }
        canonical = "n";
        return true;
    }
    if (typeName == "list" && depth < 5) {
        PySsizeT size = g_py.ListSize(value);
        winrt::Windows::Data::Json::JsonArray array =
            buildJson ? winrt::Windows::Data::Json::JsonArray() : nullptr;
        canonical = "l(";
        PySsizeT limit = std::min<PySsizeT>(size, 200);
        for (PySsizeT i = 0; i < limit; ++i) {
            PyObjectT* item = g_py.ListGetItem(value, i);
            if (item == nullptr) {
                ClearPythonError();
                return false;
            }
            JsonValue element{nullptr};
            std::string elementCanonical;
            if (!PyValueToJson(item, depth + 1, buildJson, element, elementCanonical)) {
                return false;
            }
            if (buildJson) {
                array.Append(element);
            }
            canonical += elementCanonical;
            canonical += ',';
        }
        if (size > limit) {
            canonical += "+";
            canonical += std::to_string(size - limit);
            canonical += "more";
        }
        canonical += ")";
        if (buildJson) {
            out = JsonValue::Parse(array.Stringify());
        }
        return true;
    }
    if (typeName == "dict" && depth < 5) {
        PyRef keys(g_py.DictKeys(value));
        if (!keys) {
            ClearPythonError();
            return false;
        }
        JsonObject object = buildJson ? JsonObject() : nullptr;
        canonical = "d(";
        PySsizeT size = g_py.ListSize(keys.Get());
        PySsizeT limit = std::min<PySsizeT>(size, 200);
        for (PySsizeT i = 0; i < limit; ++i) {
            PyObjectT* key = g_py.ListGetItem(keys.Get(), i);
            if (key == nullptr) {
                ClearPythonError();
                return false;
            }
            if (TypeName(key) != "str") {
                continue; // 非字符串键跳过（规范化只覆盖标量键）
            }
            std::string keyText = Utf8Of(key);
            PyObjectT* item = g_py.DictGetItemString(value, keyText.c_str());
            if (item == nullptr) {
                continue;
            }
            JsonValue element{nullptr};
            std::string elementCanonical;
            if (!PyValueToJson(item, depth + 1, buildJson, element, elementCanonical)) {
                return false;
            }
            if (buildJson) {
                object.Insert(winrt::to_hstring(keyText), element);
            }
            canonical += keyText + "=" + elementCanonical + ",";
        }
        canonical += ")";
        if (buildJson) {
            out = JsonValue::Parse(object.Stringify());
        }
        return true;
    }
    // 其它类型（datetime 等）：只暴露类型名，不进入模型。
    if (buildJson) {
        JsonObject typed;
        typed.Insert(L"__type__", JsonValue::CreateStringValue(winrt::to_hstring(typeName)));
        out = JsonValue::Parse(typed.Stringify());
    }
    canonical = "t:" + typeName;
    return true;
}

// ---- 附着扫描 ------------------------------------------------------------------

struct AttachedHost {
    PyRef window;
    PyRef log;
    PyRef cfg;
};

std::string DescribeMainWindow(const std::vector<std::string>& labels) {
    std::string text = "app.main_window.MainWindow";
    if (!labels.empty()) {
        text += "（同时发现子类实例：";
        for (size_t i = 0; i < labels.size() && i < 3; ++i) {
            if (i > 0) {
                text += "、";
            }
            text += labels[i];
        }
        text += "）";
    }
    return text;
}

// 返回 0 成功；错误码见 README「错误码」一节。
unsigned long AttachHost(AttachedHost& host, HostSnapshot& snapshot, std::string& message) {
    if (g_py.IsInitialized() == 0) {
        message = "CPython 解释器尚未初始化";
        return 0x11;
    }
    GilGuard gil;
    PyObjectT* modules = g_py.ImportGetModuleDict();
    if (modules == nullptr) {
        ClearPythonError();
        message = "读取已加载模块表失败";
        return 0x12;
    }
    PyObjectT* configModule = g_py.DictGetItemString(modules, "module.config");
    if (configModule == nullptr) {
        message = "宿主未加载 module.config：目标不是三月七或界面尚未启动完成";
        return 0x20;
    }
    PyObjectT* appModule = g_py.DictGetItemString(modules, "app.main_window");
    if (appModule == nullptr) {
        message = "宿主未加载 app.main_window：界面尚未启动完成";
        return 0x21;
    }
    PyObjectT* tasksModule = g_py.DictGetItemString(modules, "utils.tasks");
    if (tasksModule == nullptr) {
        message = "宿主未加载 utils.tasks";
        return 0x25;
    }
    PyRef cfg = Attr(configModule, "cfg");
    if (!cfg) {
        ClearPythonError();
        message = "module.config 中没有 cfg 对象";
        return 0x26;
    }
    PyRef available = Attr(tasksModule, "AVAILABLE_TASKS");
    if (!available || TypeName(available.Get()) != "dict") {
        ClearPythonError();
        message = "utils.tasks.AVAILABLE_TASKS 不可用";
        return 0x27;
    }

    // gc.get_objects 定位唯一 MainWindow 实例（只找实例，不匹配类对象本身）。
    PyObjectT* gcModule = g_py.DictGetItemString(modules, "gc");
    PyRef gcRef;
    if (gcModule == nullptr) {
        gcRef = PyRef(g_py.ImportImportModule("gc")); // 解释器内置模块，非业务模块
        gcModule = gcRef.Get();
    }
    if (gcModule == nullptr) {
        ClearPythonError();
        message = "gc 模块不可用";
        return 0x28;
    }
    PyRef getObjects = Attr(gcModule, "get_objects");
    if (!getObjects) {
        ClearPythonError();
        message = "gc.get_objects 不可用";
        return 0x29;
    }
    PyRef objects = CallNoArgs(getObjects.Get());
    if (!objects) {
        ClearPythonError();
        message = "gc.get_objects() 调用失败";
        return 0x2A;
    }
    std::vector<PyRef> windows;
    std::vector<std::string> subclassLabels;
    PySsizeT count = g_py.ListSize(objects.Get());
    for (PySsizeT i = 0; i < count; ++i) {
        PyObjectT* item = g_py.ListGetItem(objects.Get(), i);
        if (item == nullptr) {
            ClearPythonError();
            continue;
        }
        PyRef cls(g_py.ObjectType(item));
        if (!cls) {
            ClearPythonError();
            continue;
        }
        PyRef className = Attr(cls.Get(), "__name__");
        if (!className) {
            ClearPythonError();
            continue;
        }
        if (Utf8Of(className.Get()) != "MainWindow") {
            continue;
        }
        PyRef moduleName = Attr(cls.Get(), "__module__");
        if (!moduleName || Utf8Of(moduleName.Get()) != "app.main_window") {
            ClearPythonError();
            continue;
        }
        g_py.IncRef(item);
        windows.emplace_back(item);
        if (windows.size() > 8) {
            break; // 明显异常，提前收手
        }
    }
    if (windows.empty()) {
        message = "未找到 app.main_window.MainWindow 实例：界面可能尚未创建或已关闭";
        return 0x22;
    }
    if (windows.size() > 1) {
        message = "发现多个 MainWindow 实例，拒绝附着（无法确定唯一宿主）";
        return 0x23;
    }
    PyRef window = std::move(windows[0]);
    PyRef log = Attr(window.Get(), "logInterface");
    if (!log) {
        ClearPythonError();
        message = "MainWindow 缺少 logInterface 成员";
        return 0x24;
    }

    // 成员与签名核对（__code__ 元数据，不导入 inspect）。
    struct MethodCheck {
        PyObjectT* owner;
        const char* name;
        int minArgs;           // 含 self
        const char* paramName; // 第一个真实参数名（self 之后）
        size_t paramIndex;     // 在参数表中的下标（self=0）
    };
    MethodCheck checks[] = {
        {log.Get(), "isTaskRunning", 1, nullptr, 0},
        {log.Get(), "startTask", 2, "command_or_task", 1},
        {log.Get(), "stopTask", 2, "user_initiated", 1},
        {cfg.Get(), "get_value", 3, "key", 1},
        {cfg.Get(), "set_value", 3, "key", 1},
    };
    for (const MethodCheck& check : checks) {
        PyRef method = Attr(check.owner, check.name);
        if (!method || g_py.CallableCheck(method.Get()) != 1) {
            ClearPythonError();
            message = std::string("宿主成员校验失败：") + check.name + " 不可调用";
            return 0x24;
        }
        auto argCount = MethodArgCount(method.Get());
        if (!argCount.has_value() || *argCount < check.minArgs) {
            message = std::string("宿主成员签名不符：") + check.name + " 参数过少";
            return 0x24;
        }
        if (check.paramName != nullptr) {
            auto names = MethodParamNames(method.Get());
            if (names.has_value() && names->size() > check.paramIndex) {
                if ((*names)[check.paramIndex] != check.paramName) {
                    message = std::string("宿主成员签名不符：") + check.name + " 的参数名不是 " +
                              check.paramName + "，宿主版本与桥已知契约不一致";
                    return 0x24;
                }
            }
        }
    }
    PyRef configDict = Attr(cfg.Get(), "config");
    if (!configDict || TypeName(configDict.Get()) != "dict") {
        ClearPythonError();
        message = "cfg.config 不是配置字典";
        return 0x24;
    }

    // 目录快照：白名单 ∩ 宿主 AVAILABLE_TASKS。
    for (const auto& [taskId, description] : AllowedTasks()) {
        PyObjectT* label = g_py.DictGetItemString(available.Get(), taskId.c_str());
        if (label == nullptr) {
            continue;
        }
        TaskEntry entry;
        entry.id = taskId;
        entry.label = Utf8Of(label);
        entry.description = description;
        snapshot.tasks.push_back(std::move(entry));
    }
    PyRef version = Attr(cfg.Get(), "version");
    snapshot.hostVersion = version ? Utf8Of(version.Get()) : "unknown";
    if (snapshot.hostVersion.empty()) {
        snapshot.hostVersion = "unknown";
    }
    host.window = std::move(window);
    host.log = std::move(log);
    host.cfg = std::move(cfg);
    if (!subclassLabels.empty()) {
        message = DescribeMainWindow(subclassLabels);
    }
    return 0;
}

// ---- HostOps 实现 ---------------------------------------------------------------

// 每次操作独立的请求状态：主线程闭包与调用方经 shared_ptr 共享，超时后迟到
// 回调写的是堆上状态而不是调用方栈引用；桥绝不使用跨请求共享的输出成员，
// 并发/迟到结果不会互相覆盖。闭包只捕获参数副本与 persistent this
//（PythonHostOps 故意不析构，见下方生命周期注释）。
TaskProcessState ReadTaskProcessState(PyObjectT* log) {
    PyRef external = Attr(log, "_external_task_active");
    if (!external) { ClearPythonError(); return TaskProcessState::Unknown; }
    int active = g_py.IsTrue(external.Get());
    if (active < 0) { ClearPythonError(); return TaskProcessState::Unknown; }
    if (active) return TaskProcessState::Running;
    PyRef process = Attr(log, "process");
    if (!process) { ClearPythonError(); return TaskProcessState::Unknown; }
    if (TypeName(process.Get()) == "NoneType") return TaskProcessState::Stopped;
    PyRef method = Attr(process.Get(), "state");
    PyRef state = method ? CallNoArgs(method.Get()) : PyRef{};
    if (!state) { ClearPythonError(); return TaskProcessState::Unknown; }
    PyRef enumValue = TypeName(state.Get()) == "int" ? PyRef{} : Attr(state.Get(), "value");
    PyObjectT* number = enumValue ? enumValue.Get() : state.Get();
    long long raw = g_py.LongAsLongLong(number);
    if (g_py.ErrOccurred()) { ClearPythonError(); return TaskProcessState::Unknown; }
    return ClassifyTaskProcessState(raw >= 0 && raw <= 2 ? int(raw) : -1, false);
}

struct StatusRequestState {
    OpError error;
    bool running = false;
    TaskProcessState processState = TaskProcessState::Unknown;
    std::string currentTask;
};
struct WriteRequestState {
    OpError error;
    WriteResultData result;
};
struct StartRequestState {
    OpError error;
    bool confirmed = false;
};
struct StopRequestState {
    OpError error;
    bool wasRunning = false;
    bool stopRequested = false;
    bool deferredStop = false;
    TaskProcessState processState = TaskProcessState::Unknown;
    std::string taskName;
};
struct LogsRequestState {
    OpError error;
    std::string text;
};

class PythonHostOps : public HostOps {
  public:
    explicit PythonHostOps(AttachedHost host) : host_(std::move(host)) {}

    bool TaskStatus(TaskStatusData& out, OpError& err) override {
        auto state = std::make_shared<StatusRequestState>();
        bool ok = RunHostJob(
            [this, state] {
                PyRef method = Attr(host_.log.Get(), "isTaskRunning");
                if (!method) {
                    ClearPythonError();
                    state->error = {500, "host_error", "宿主 isTaskRunning 不可用"};
                    return;
                }
                PyRef running = CallNoArgs(method.Get());
                if (!running) {
                    ClearPythonError();
                    state->error = {500, "host_error", "宿主 isTaskRunning 调用失败"};
                    return;
                }
                state->running = g_py.IsTrue(running.Get()) == 1;
                state->processState = ReadTaskProcessState(host_.log.Get());
                PyRef current = Attr(host_.log.Get(), "current_task");
                if (current && TypeName(current.Get()) == "str") {
                    state->currentTask = Utf8Of(current.Get());
                } else {
                    ClearPythonError();
                }
            },
            kDispatchTimeoutMs, err);
        if (!ok) {
            return false;
        }
        if (state->error.httpStatus != 0) {
            err = state->error;
            return false;
        }
        out.running = state->running;
        out.processState = state->processState;
        out.currentTask = std::move(state->currentTask);
        return true;
    }

    bool SnapshotConfig(ConfigSnapshotData& out, OpError& err) override {
        // 纯 Python 字典快照：工作线程持 GIL 读取（不触碰 Qt）。
        EnsureApartment();
        GilGuard gil;
        PyRef dict = Attr(host_.cfg.Get(), "config");
        if (!dict || TypeName(dict.Get()) != "dict") {
            ClearPythonError();
            err.httpStatus = 500;
            err.code = "host_error";
            err.message = "cfg.config 不可读";
            return false;
        }
        PyRef keys(g_py.DictKeys(dict.Get()));
        if (!keys) {
            ClearPythonError();
            err.httpStatus = 500;
            err.code = "host_error";
            err.message = "cfg.config 键列表不可读";
            return false;
        }
        JsonObject values;
        std::vector<std::pair<std::string, std::string>> pairs;
        std::vector<std::string> keyNames;
        PySsizeT count = g_py.ListSize(keys.Get());
        for (PySsizeT i = 0; i < count; ++i) {
            PyObjectT* key = g_py.ListGetItem(keys.Get(), i);
            if (key == nullptr) {
                ClearPythonError();
                continue;
            }
            if (TypeName(key) != "str") {
                continue;
            }
            std::string keyText = Utf8Of(key);
            PyObjectT* value = g_py.DictGetItemString(dict.Get(), keyText.c_str());
            if (value == nullptr) {
                continue;
            }
            JsonValue json{nullptr};
            std::string canonical;
            if (!PyValueToJson(value, 0, true, json, canonical)) {
                continue;
            }
            values.Insert(winrt::to_hstring(keyText), json);
            pairs.emplace_back(keyText, canonical);
            keyNames.push_back(keyText);
        }
        out.values = values;
        out.allKeys = std::move(keyNames);
        out.revision = ComputeRevision(pairs);
        return true;
    }

    bool WriteConfigField(const std::string& key, const IJsonValue& value,
                          const std::string& baseRevision, WriteResultData& out,
                          OpError& err) override {
        // 请求校验层已保证类型合法；主线程闭包内完成复核快照 → revision →
        // set_value → 回读。标量先拷贝进闭包，不捕获请求参数的引用。
        bool boolValue = false;
        double numberValue = 0;
        std::string stringValue;
        JsonValueType type = value.ValueType();
        if (type == JsonValueType::Boolean) {
            boolValue = value.GetBoolean();
        } else if (type == JsonValueType::Number) {
            numberValue = value.GetNumber();
        } else {
            stringValue = winrt::to_string(value.GetString());
        }
        std::string expectedType = type == JsonValueType::Boolean
                                       ? "bool"
                                       : (type == JsonValueType::Number ? "int" : "str");
        auto state = std::make_shared<WriteRequestState>();
        bool ok = RunHostJob(
            [this, state, key, baseRevision, boolValue, numberValue, stringValue, type,
             expectedType, jsonValue = IJsonValue(value)] {
                PyRef dict = Attr(host_.cfg.Get(), "config");
                if (!dict || TypeName(dict.Get()) != "dict") {
                    ClearPythonError();
                    state->error = {500, "host_error", "cfg.config 不可读"};
                    return;
                }
                PyObjectT* current = g_py.DictGetItemString(dict.Get(), key.c_str());
                if (current == nullptr) {
                    state->error = {422, "field_not_present",
                                    "宿主配置中不存在字段 " + key + "，桥只写已存在字段"};
                    return;
                }
                std::string currentType = TypeName(current);
                if (currentType != expectedType) {
                    state->error = {409, "host_state_changed",
                                    "字段 " + key + " 在宿主中的当前类型与白名单不一致，拒绝写入"};
                    return;
                }
                // 整份配置复核（含敏感值，仅在本进程内参与指纹）。
                std::vector<std::pair<std::string, std::string>> pairs;
                if (!BuildCanonicalPairs(dict.Get(), pairs)) {
                    state->error = {500, "host_error", "配置快照构建失败"};
                    return;
                }
                std::string revision = ComputeRevision(pairs);
                if (revision != baseRevision) {
                    state->error = {409, "revision_conflict",
                                    "配置已被并发修改，请重新读取后再写"};
                    return;
                }
                PyRef pyKey(g_py.UnicodeFromString(key.c_str()));
                PyRef pyValue = MakePythonValue(boolValue, numberValue, stringValue, type);
                if (!pyKey || !pyValue) {
                    ClearPythonError();
                    state->error = {500, "host_error", "构造 Python 值失败"};
                    return;
                }
                PyRef setter = Attr(host_.cfg.Get(), "set_value");
                if (!setter) {
                    ClearPythonError();
                    state->error = {500, "host_error", "cfg.set_value 不可用"};
                    return;
                }
                PyRef call = CallOneArg2(setter.Get(), pyKey.Get(), pyValue.Get());
                if (!call) {
                    ClearPythonError();
                    state->error = {500, "host_error", "cfg.set_value 调用失败"};
                    return;
                }
                // 回读核验。
                PyRef dictAfter = Attr(host_.cfg.Get(), "config");
                if (!dictAfter) {
                    ClearPythonError();
                    state->error = {500, "write_unverified", "写入后无法回读配置"};
                    return;
                }
                PyObjectT* readback = g_py.DictGetItemString(dictAfter.Get(), key.c_str());
                if (readback == nullptr || !ValueEquals(readback, expectedType, boolValue,
                                                        numberValue, stringValue)) {
                    state->error = {500, "write_unverified",
                                    "写入后回读不一致，字段 " + key + " 未确认生效"};
                    return;
                }
                std::vector<std::pair<std::string, std::string>> after;
                if (!BuildCanonicalPairs(dictAfter.Get(), after)) {
                    state->error = {500, "host_error", "写入后配置快照构建失败"};
                    return;
                }
                state->result.key = key;
                state->result.value = jsonValue;
                state->result.revision = ComputeRevision(after);
            },
            12000, err);
        if (!ok) {
            return false;
        }
        if (state->error.httpStatus != 0) {
            err = state->error;
            return false;
        }
        out = state->result;
        return true;
    }

    bool StartTask(const std::string& taskId, StartTaskData& out, OpError& err) override {
        auto state = std::make_shared<StartRequestState>();
        // 第一步（主线程闭包）：占用检查 + startTask 调用 + 一次即时复核。
        // 不在 GUI 主线程里 Sleep 等待 Starting→Running 迁移。
        bool ok = RunHostJob(
            [this, state, taskId] {
                PyRef method = Attr(host_.log.Get(), "isTaskRunning");
                if (!method) {
                    ClearPythonError();
                    state->error = {500, "host_error", "宿主 isTaskRunning 不可用"};
                    return;
                }
                PyRef running = CallNoArgs(method.Get());
                if (!running) {
                    ClearPythonError();
                    state->error = {500, "host_error", "宿主 isTaskRunning 调用失败"};
                    return;
                }
                TaskProcessState processState = ReadTaskProcessState(host_.log.Get());
                if (processState == TaskProcessState::Unknown) {
                    state->error = {503, "host_state_unknown", "无法确认当前进程状态，本次未启动"};
                    return;
                }
                if (processState != TaskProcessState::Stopped) {
                    PyRef current = Attr(host_.log.Get(), "current_task");
                    std::string currentName;
                    if (current && TypeName(current.Get()) == "str") {
                        currentName = Utf8Of(current.Get());
                    } else {
                        ClearPythonError();
                    }
                    state->error = {409, "task_running",
                                    "已有任务正在运行（" +
                                        (currentName.empty() ? "未知" : currentName) +
                                        "），请先停止再启动"};
                    return;
                }
                PyRef start = Attr(host_.log.Get(), "startTask");
                if (!start) {
                    ClearPythonError();
                    state->error = {500, "host_error", "宿主 startTask 不可用"};
                    return;
                }
                PyRef pyTaskId(g_py.UnicodeFromString(taskId.c_str()));
                if (!pyTaskId) {
                    ClearPythonError();
                    state->error = {500, "host_error", "任务 ID 转换失败"};
                    return;
                }
                PyRef call = CallOneArg(start.Get(), pyTaskId.Get());
                if (!call) {
                    ClearPythonError();
                    state->error = {500, "host_error", "宿主 startTask 调用失败"};
                    return;
                }
                // startTask 返回 void 不算成功：接纳证据只有 isTaskRunning() 变为
                // 真（真实 QProcess Running 或外部任务活动）；current_task 与请求
                // ID 相同不算证据。
                PyRef check = CallNoArgs(method.Get());
                if (check) {
                    state->confirmed = g_py.IsTrue(check.Get()) == 1;
                } else {
                    ClearPythonError();
                }
            },
            kDispatchTimeoutMs + 1000, err);
        if (!ok) {
            return false;
        }
        if (state->error.httpStatus != 0) {
            err = state->error;
            return false;
        }
        // 第二步（HTTP 工作线程轮询）：Starting→Running 需要时间；等待发生在
        // 本线程，每轮重新派发只读检查，不阻塞 GUI 主线程。
        for (int attempt = 0; attempt < kStartConfirmAttempts && !state->confirmed; ++attempt) {
            ::Sleep(kStartConfirmIntervalMs);
            bool running = false;
            if (!RunningEvidence(running, err)) {
                return false;
            }
            state->confirmed = running;
        }
        out.status = state->confirmed ? "accepted" : "unconfirmed";
        out.taskId = taskId;
        return true;
    }

    bool StopTask(int graceMs, StopTaskData& out, OpError& err) override {
        auto state = std::make_shared<StopRequestState>();
        // 第一步（主线程闭包）：读运行证据与当前任务，必要时发出 stopTask。
        bool ok = RunHostJob(
            [this, state] {
                PyRef method = Attr(host_.log.Get(), "isTaskRunning");
                if (!method) {
                    ClearPythonError();
                    state->error = {500, "host_error", "宿主 isTaskRunning 不可用"};
                    return;
                }
                PyRef running = CallNoArgs(method.Get());
                if (!running) {
                    ClearPythonError();
                    state->error = {500, "host_error", "宿主 isTaskRunning 调用失败"};
                    return;
                }
                state->wasRunning = g_py.IsTrue(running.Get()) == 1;
                state->processState = ReadTaskProcessState(host_.log.Get());
                PyRef current = Attr(host_.log.Get(), "current_task");
                if (current && TypeName(current.Get()) == "str") {
                    state->taskName = Utf8Of(current.Get());
                } else {
                    ClearPythonError();
                }
                if (state->processState == TaskProcessState::Unknown) {
                    state->error = {503, "host_state_unknown", "无法确认当前进程状态，停止结果未知"};
                    return;
                }
                state->deferredStop = state->processState == TaskProcessState::Starting;
                // 运行中，或任务仍被选中（如 QProcess 处于 Starting、
                // isTaskRunning 尚为假）：都发出 stopTask。
                PyRef stop = Attr(host_.log.Get(), "stopTask");
                if (!stop) {
                    ClearPythonError();
                    state->error = {500, "host_error", "宿主 stopTask 不可用"};
                    return;
                }
                PyRef call = CallNoArgs(stop.Get()); // user_initiated 默认 True
                if (!call) {
                    ClearPythonError();
                    state->error = {500, "host_error", "宿主 stopTask 调用失败"};
                    return;
                }
                state->stopRequested = state->processState != TaskProcessState::Stopped;
            },
            kDispatchTimeoutMs, err);
        if (!ok) {
            return false;
        }
        if (state->error.httpStatus != 0) {
            err = state->error;
            return false;
        }
        if (!state->stopRequested) {
            out.status = "not_running";
            out.taskId = state->taskName;
            return true;
        }
        // 第二步（HTTP 工作线程轮询）：只有 isTaskRunning 变假才报 stopped；
        // 观察窗内仍运行或状态不可读一律 unknown，不冒充已停止。
        uint32_t deadline = ::GetTickCount() + uint32_t(graceMs);
        while (true) {
            ::Sleep(kStopPollIntervalMs);
            TaskStatusData observed;
            if (!TaskStatus(observed, err)) {
                out.status = "unknown";
                out.taskId = state->taskName;
                out.note = "停止后核对状态失败：" + err.message;
                return true;
            }
            if (observed.processState == TaskProcessState::Stopped) {
                out.status = "stopped";
                out.taskId = state->taskName;
                return true;
            }
            if (observed.processState == TaskProcessState::Unknown) {
                out.status = "unknown";
                out.taskId = state->taskName;
                out.note = "停止后未能确认实际进程状态";
                return true;
            }
            if (state->deferredStop && observed.processState == TaskProcessState::Running) {
                state->deferredStop = false;
                if (!RunHostJob([this, state] {
                    PyRef current = Attr(host_.log.Get(), "current_task");
                    if (!current || TypeName(current.Get()) != "str" || Utf8Of(current.Get()) != state->taskName) return;
                    PyRef stop = Attr(host_.log.Get(), "stopTask");
                    PyRef result = stop ? CallNoArgs(stop.Get()) : PyRef{};
                    if (!result) ClearPythonError();
                }, kDispatchTimeoutMs, err)) {
                    out.status = "unknown";
                    out.taskId = state->taskName;
                    return true;
                }
            }
            if (::GetTickCount() >= deadline) {
                out.status = "unknown";
                out.taskId = state->taskName;
                out.note =
                    "停止请求已发出，但在观察窗内宿主仍报告任务运行；真实状态未知，请用 status 核实";
                return true;
            }
        }
    }

    bool ReadLogs(int tailLines, LogsData& out, OpError& err) override {
        auto state = std::make_shared<LogsRequestState>();
        bool ok = RunHostJob(
            [this, state] {
                PyRef widget = Attr(host_.log.Get(), "logTextEdit");
                if (!widget) {
                    ClearPythonError();
                    state->error = {500, "host_error", "宿主 logTextEdit 不可用"};
                    return;
                }
                // logTextEdit 是 QPlainTextEdit 控件：要先取 toPlainText 绑定方法
                // 再调用，不能把控件对象本身当 callable。
                PyRef toPlainText = Attr(widget.Get(), "toPlainText");
                if (!toPlainText) {
                    ClearPythonError();
                    state->error = {500, "host_error", "logTextEdit.toPlainText 不可用"};
                    return;
                }
                PyRef plain = CallNoArgs(toPlainText.Get());
                if (!plain || TypeName(plain.Get()) != "str") {
                    ClearPythonError();
                    state->error = {500, "host_error", "logTextEdit.toPlainText() 调用失败"};
                    return;
                }
                state->text = Utf8Of(plain.Get());
                PyRef buffered = Attr(host_.log.Get(), "_buffered_logs");
                if (buffered && TypeName(buffered.Get()) == "str") {
                    state->text += Utf8Of(buffered.Get());
                } else {
                    ClearPythonError(); // 可选内部缓冲：缺失或非字符串时跳过
                }
            },
            kDispatchTimeoutMs, err);
        if (!ok) {
            return false;
        }
        if (state->error.httpStatus != 0) {
            err = state->error;
            return false;
        }
        std::vector<std::string> lines;
        size_t start = 0;
        while (start <= state->text.size()) {
            size_t eol = state->text.find('\n', start);
            size_t end = eol == std::string::npos ? state->text.size() : eol;
            std::string line = state->text.substr(start, end - start);
            while (!line.empty() && (line.back() == '\r')) {
                line.pop_back();
            }
            lines.push_back(line);
            if (eol == std::string::npos) {
                break;
            }
            start = eol + 1;
        }
        int32_t total = int32_t(lines.size());
        if (int32_t(lines.size()) > tailLines) {
            lines.erase(lines.begin(), lines.end() - tailLines);
        }
        out.lines = std::move(lines);
        out.totalLines = total;
        return true;
    }

  private:
    static void EnsureApartment() {
        thread_local bool initialized = [] {
            try {
                winrt::init_apartment(winrt::apartment_type::multi_threaded);
            } catch (...) {
                // 目标线程可能已初始化其它套间；WinRT 调用仍可用。
            }
            return true;
        }();
        (void)initialized;
    }

    // 派发的单次 isTaskRunning 只读检查：接纳/停止确认共用。
    bool RunningEvidence(bool& running, OpError& err) {
        auto state = std::make_shared<StatusRequestState>();
        bool ok = RunHostJob(
            [this, state] {
                PyRef method = Attr(host_.log.Get(), "isTaskRunning");
                if (!method) {
                    ClearPythonError();
                    state->error = {500, "host_error", "宿主 isTaskRunning 不可用"};
                    return;
                }
                PyRef result = CallNoArgs(method.Get());
                if (!result) {
                    ClearPythonError();
                    state->error = {500, "host_error", "宿主 isTaskRunning 调用失败"};
                    return;
                }
                state->running = g_py.IsTrue(result.Get()) == 1;
            },
            kDispatchTimeoutMs, err);
        if (!ok) {
            return false;
        }
        if (state->error.httpStatus != 0) {
            err = state->error;
            return false;
        }
        running = state->running;
        return true;
    }

    PyRef CallOneArg2(PyObjectT* callable, PyObjectT* a, PyObjectT* b) {
        // 两参调用：Vectorcall 或 CallFunctionObjArgs 兜底。
        static Vectorcall2Fn vectorcall2 = ResolveVectorcall2();
        if (vectorcall2) {
            PyObjectT* args[2] = {a, b};
            return PyRef(vectorcall2(callable, args, 2, nullptr));
        }
        static VarArgs2Fn varArgs2 = ResolveVarArgs2();
        if (varArgs2) {
            return PyRef(varArgs2(callable, a, b, nullptr));
        }
        return PyRef(nullptr);
    }

    using Vectorcall2Fn = PyObjectT* (*)(PyObjectT*, PyObjectT* const*, size_t, PyObjectT*);
    using VarArgs2Fn = PyObjectT* (*)(PyObjectT*, ...);
    static Vectorcall2Fn ResolveVectorcall2() {
        return reinterpret_cast<Vectorcall2Fn>(
            ::GetProcAddress(g_py.module, "PyObject_Vectorcall"));
    }
    static VarArgs2Fn ResolveVarArgs2() {
        return reinterpret_cast<VarArgs2Fn>(
            ::GetProcAddress(g_py.module, "PyObject_CallFunctionObjArgs"));
    }

    PyRef MakePythonValue(bool boolValue, double numberValue, const std::string& stringValue,
                          JsonValueType type) {
        if (type == JsonValueType::Boolean) {
            return PyRef(g_py.BoolFromLong(boolValue ? 1 : 0));
        }
        if (type == JsonValueType::Number) {
            return PyRef(g_py.LongFromLongLong(long long(numberValue)));
        }
        return PyRef(g_py.UnicodeFromString(stringValue.c_str()));
    }

    bool ValueEquals(PyObjectT* readback, const std::string& expectedType, bool boolValue,
                     double numberValue, const std::string& stringValue) {
        std::string type = TypeName(readback);
        if (type != expectedType) {
            return false;
        }
        if (expectedType == "bool") {
            return (g_py.IsTrue(readback) == 1) == boolValue;
        }
        if (expectedType == "int") {
            long long number = g_py.LongAsLongLong(readback);
            if (g_py.ErrOccurred() != nullptr) {
                ClearPythonError();
                return false;
            }
            return number == long long(numberValue);
        }
        return Utf8Of(readback) == stringValue;
    }

    bool BuildCanonicalPairs(PyObjectT* dict, std::vector<std::pair<std::string, std::string>>& pairs) {
        PyRef keys(g_py.DictKeys(dict));
        if (!keys) {
            ClearPythonError();
            return false;
        }
        PySsizeT count = g_py.ListSize(keys.Get());
        for (PySsizeT i = 0; i < count; ++i) {
            PyObjectT* key = g_py.ListGetItem(keys.Get(), i);
            if (key == nullptr || TypeName(key) != "str") {
                ClearPythonError();
                continue;
            }
            std::string keyText = Utf8Of(key);
            PyObjectT* value = g_py.DictGetItemString(dict, keyText.c_str());
            if (value == nullptr) {
                continue;
            }
            JsonValue json{nullptr};
            std::string canonical;
            if (!PyValueToJson(value, 0, false, json, canonical)) {
                continue;
            }
            pairs.emplace_back(keyText, canonical);
        }
        return true;
    }

    AttachedHost host_;
};

// ---- 桥生命周期 ------------------------------------------------------------------
//
// 桥对象（Bridge / PythonHostOps）在 StartBridge 成功后故意泄漏、永不析构：宿主
// 退出时 CRT 会先于 Python 终结销毁静态析构物，此时再 DecRef 宿主对象就是
// UAF/terminate。桥随宿主进程一起消亡；DLL 由控制面 worker 钉住（见
// dll_main.cpp），线程句柄不 join（HTTP 服务线程退出回调只复位状态，不做清理）。
//
// 停止与再启动：HTTP /shutdown 与控制面 STOP 都会让 HTTP 服务线程退出，其
// onStopped 回调把 running 复位为 false 并清空桥指针（旧桥对象保持泄漏）；
// 常驻 worker 随后可再次接受 START。

struct BridgeState {
    std::mutex mutex;          // 保护 bridge/ops/server 指针的赋值与复位
    Bridge* bridge = nullptr;  // new 后不 delete（见上）
    HostOps* ops = nullptr;    // 同上
    HttpServerHandle server{};
    std::atomic<bool> running{false};
};

BridgeState g_state;

std::string RandomInstanceId() {
    // 实例号只用于标识本次附着会话，不是秘密；用进程内随机源即可。
    static std::mt19937_64 engine{std::random_device{}()};
    uint64_t bytes[2] = {engine(), engine()};
    char text[33];
    snprintf(text, sizeof(text), "%016llx%016llx",
             (unsigned long long)bytes[0], (unsigned long long)bytes[1]);
    return std::string(text, 16);
}

bool ValidToken(const std::string& token) {
    if (token.size() < 16 || token.size() > 256) {
        return false;
    }
    for (unsigned char c : token) {
        if (c < 0x21 || c > 0x7E) {
            return false;
        }
    }
    return true;
}

} // namespace

unsigned long StartBridgeImpl(const wchar_t* argsJson, std::string& message) {
    bool expected = false;
    if (!g_state.running.compare_exchange_strong(expected, true)) {
        message = "桥已在运行";
        return 0x40;
    }
    try {
        winrt::init_apartment(winrt::apartment_type::multi_threaded);
    } catch (...) {
    }
    std::string token;
    uint16_t port = kDefaultPort;
    try {
        JsonObject args = JsonObject::Parse(argsJson != nullptr ? argsJson : L"{}");
        if (args.HasKey(L"token")) {
            token = winrt::to_string(args.GetNamedString(L"token"));
        }
        if (args.HasKey(L"port")) {
            port = uint16_t(args.GetNamedNumber(L"port"));
        }
    } catch (...) {
        g_state.running.store(false);
        message = "启动参数不是合法 JSON";
        return 0x41;
    }
    if (!ValidToken(token)) {
        g_state.running.store(false);
        message = "令牌缺失或不合规（需要 16..256 个可打印 ASCII 字符）";
        return 0x42;
    }
    if (port == 0) {
        g_state.running.store(false);
        message = "端口不合法";
        return 0x43;
    }
    std::string error;
    if (!BindPythonApi(error)) {
        g_state.running.store(false);
        message = error;
        return 0x10;
    }
    {
        GilGuard gil;
        if (!EnsureQtPoster(error)) {
            g_state.running.store(false);
            message = error;
            return 0x45;
        }
    }
    AttachedHost host;
    HostSnapshot snapshot;
    unsigned long code = AttachHost(host, snapshot, message);
    if (code != 0) {
        g_state.running.store(false);
        return code;
    }
    snapshot.instanceId = RandomInstanceId();
    snapshot.catalogVersion = ComputeCatalogVersion(snapshot.tasks);
    HostOps* ops = new PythonHostOps(std::move(host));
    Bridge* bridge = new Bridge(token, port, snapshot, ops);
    HttpServerHandle server;
    server.onStopped = [] {
        // HTTP 服务线程退出（/shutdown 或控制面 STOP）：复位生命周期状态，
        // 允许常驻 worker 再次 START。桥对象保持泄漏，不做清理。
        std::lock_guard<std::mutex> lock(g_state.mutex);
        if (g_state.server.thread != nullptr) {
            ::CloseHandle(static_cast<HANDLE>(g_state.server.thread));
            g_state.server.thread = nullptr;
        }
        g_state.bridge = nullptr;
        g_state.ops = nullptr;
        g_state.running.store(false);
    };
    if (!StartHttpServer(*bridge, port, server, error)) {
        g_state.running.store(false);
        message = error;
        return 0x44;
    }
    {
        std::lock_guard<std::mutex> lock(g_state.mutex);
        g_state.ops = ops;
        g_state.bridge = bridge;
        g_state.server = server;
    }
    return 0;
}

unsigned long StopBridgeImpl() {
    Bridge* bridge = nullptr;
    HttpServerHandle server{};
    {
        std::lock_guard<std::mutex> lock(g_state.mutex);
        if (g_state.bridge == nullptr) {
            // 从未启动或已停止：幂等成功。
            g_state.running.store(false);
            return 0;
        }
        bridge = g_state.bridge;
        server = g_state.server;
        // 句柄移交本次 join；先清空状态避免服务线程退出回调重复关闭。
        g_state.server = HttpServerHandle{};
        g_state.bridge = nullptr;
        g_state.ops = nullptr;
    }
    bridge->RequestShutdown();
    JoinHttpServer(server);
    g_state.running.store(false);
    return 0;
}

} // namespace sdm::march7th
