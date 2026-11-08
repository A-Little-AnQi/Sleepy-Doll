// 三月七原生桥纯逻辑测试：core + routing + FakeHost + AsyncCall（注入 fake 队列）。
//
// 只链接 core.cpp / routing.cpp（不含 host_python / http_server / injector），
// 不初始化 Python、不碰套接字、不启动任何进程、不做键鼠输入。
// 由主代理的隔离运行器（tests/plugins/run-native.ps1）统一编译检查与执行。
#include "core.hpp"

#include <atomic>
#include <chrono>
#include <cstdio>
#include <functional>
#include <stdexcept>
#include <string>
#include <thread>
#include <vector>

using namespace sdm::march7th;
using winrt::Windows::Data::Json::JsonObject;

namespace {

int g_failures = 0;
int g_checks = 0;
std::string g_fixtureInstance;
std::string g_fixtureCatalog;

void Expect(bool condition, const char* what) {
    ++g_checks;
    if (!condition) {
        ++g_failures;
        std::printf("FAIL: %s\n", what);
    }
}

// 阶段标记：测试以无缓冲 stdout 输出，崩溃时也能看到已完成到哪一步。
void Stage(const char* name) {
    std::printf("-- stage: %s\n", name);
    std::fflush(stdout);
}

JsonObject ParseBody(const HttpResponse& response) {
    return JsonObject::Parse(winrt::to_hstring(response.body));
}

std::string BodyString(const HttpResponse& response, const wchar_t* outer,
                       const wchar_t* inner) {
    JsonObject body = ParseBody(response);
    return winrt::to_string(body.GetNamedObject(outer).GetNamedString(inner));
}

HttpRequest MakeRequest(const std::string& path, const std::string& body,
                        const std::string& token = "unit-test-token-0123456789abcdef",
                        const char* method = "POST", bool includeIdentity = true) {
    HttpRequest request;
    request.method = method;
    request.rawPath = path;
    if (!token.empty()) {
        request.headers["x-sleepy-token"] = token;
    }
    request.body = body;
    if (includeIdentity && (path == "/config/write" || path == "/task/start" || path == "/task/stop")) {
        JsonObject data;
        if (JsonObject::TryParse(winrt::to_hstring(body), data)) {
            if (!data.HasKey(L"instanceId")) data.Insert(L"instanceId", JsonValue::CreateStringValue(winrt::to_hstring(g_fixtureInstance)));
            if (!data.HasKey(L"catalogVersion")) data.Insert(L"catalogVersion", JsonValue::CreateStringValue(winrt::to_hstring(g_fixtureCatalog)));
            request.body = winrt::to_string(data.Stringify());
        }
    }
    request.headers["content-length"] = std::to_string(request.body.size());
    request.declaredContentLength = int64_t(request.body.size());
    return request;
}

// ---- FakeHost -----------------------------------------------------------------

std::string CanonicalOf(const winrt::Windows::Data::Json::IJsonValue& value) {
    using winrt::Windows::Data::Json::JsonValueType;
    switch (value.ValueType()) {
        case JsonValueType::Boolean:
            return value.GetBoolean() ? "b:1" : "b:0";
        case JsonValueType::Number: {
            double number = value.GetNumber();
            if (number == double(long long(number))) {
                return "i:" + std::to_string(long long(number));
            }
            return "f:" + std::to_string(number);
        }
        case JsonValueType::String:
            return "s:" + std::to_string(winrt::to_string(value.GetString()).size()) + ":" +
                   winrt::to_string(value.GetString());
        case JsonValueType::Null:
            return "n";
        case JsonValueType::Array: {
            std::string text = "l(";
            for (uint32_t i = 0; i < value.GetArray().Size(); ++i) {
                text += CanonicalOf(value.GetArray().GetAt(i));
                text += ',';
            }
            return text + ")";
        }
        case JsonValueType::Object: {
            std::string text = "d(";
            for (const auto& pair : value.GetObject()) {
                text += winrt::to_string(pair.Key()) + "=" + CanonicalOf(pair.Value()) + ",";
            }
            return text + ")";
        }
        default:
            return "t:?";
    }
}

std::vector<std::pair<std::string, std::string>> CanonicalPairs(const JsonObject& values) {
    std::vector<std::pair<std::string, std::string>> pairs;
    for (const auto& pair : values) {
        pairs.emplace_back(winrt::to_string(pair.Key()), CanonicalOf(pair.Value()));
    }
    return pairs;
}

class FakeHost : public HostOps {
  public:
    FakeHost() {
        // 与真实三月七 config.yaml 同名键的精简样本：含敏感、危险、白名单、
        // 嵌套字典与数组内对象（脱敏必须覆盖数组）。
        values_ = JsonObject::Parse(
            LR"({
                "ui_language": "auto",
                "power_enable": true,
                "tp_before_instance": false,
                "instance_team_enable": false,
                "instance_type": "拟造花萼（金）",
                "pause_after_success": true,
                "python_exe_path": "C:\\Python311\\python.exe",
                "game_path": "C:\\Games\\StarRail.exe",
                "after_finish": "None",
                "scheduled_tasks": [],
                "locales": "zh_CN",
                "notify_telegram_token": "123456:SECRET",
                "notify_bark_cipherkey": "CIPHER-SECRET",
                "notify_custom_url": "https://example.invalid/PRIVATE-URL-KEY",
                "notify_matrix_user_id": "@user:matrix.org",
                "fight_allow_memory_token": "不配置",
                "notify_channels": [
                    {"name": "telegram", "bot_token": "SECRET-TOKEN", "chat_id": "42"},
                    {"name": "webhook", "url": "https://example.invalid/hook"}
                ],
                "webhook_registry": {"default": {"webhook_token": "SECRET", "level": "all"}},
                "nested_settings": {"default": {"access_token": "NESTED-SECRET", "level": "all"}},
                "log_level": "INFO"
            })");
    }

    // ---- 测试操纵 --------------------------------------------------------
    bool running = false;
    std::string currentTask;
    bool confirmStart = true;   // false = startTask 被调用但不进入运行态
    bool stubbornStop = false;  // true = stopTask 后仍报告运行
    std::vector<std::string> startCalls;
    std::vector<std::pair<std::string, std::string>> writes;

    void ExternalEdit(const std::string& key, const wchar_t* jsonValue) {
        values_.Insert(winrt::to_hstring(key), JsonValue::Parse(jsonValue));
    }

    // ---- HostOps ---------------------------------------------------------
    bool TaskStatus(TaskStatusData& out, OpError&) override {
        out.running = running;
        out.processState = ClassifyTaskProcessState(running ? 2 : 0, false);
        out.currentTask = currentTask;
        return true;
    }

    bool SnapshotConfig(ConfigSnapshotData& out, OpError&) override {
        out.values = values_;
        for (const auto& pair : values_) {
            out.allKeys.push_back(winrt::to_string(pair.Key()));
        }
        out.revision = ComputeRevision(CanonicalPairs(values_));
        return true;
    }

    bool WriteConfigField(const std::string& key, const IJsonValue& value,
                          const std::string& baseRevision, WriteResultData& out,
                          OpError& err) override {
        winrt::hstring wideKey = winrt::to_hstring(key);
        if (!values_.HasKey(wideKey)) {
            err = {422, "field_not_present",
                   "宿主配置中不存在字段 " + key + "，桥只写已存在字段"};
            return false;
        }
        std::string currentType = TypeNameOf(values_.Lookup(wideKey));
        std::string expectedType = TypeNameOf(value);
        if (currentType != expectedType) {
            err = {409, "host_state_changed", "字段 " + key + " 宿主类型与白名单不一致"};
            return false;
        }
        std::string revision = ComputeRevision(CanonicalPairs(values_));
        if (revision != baseRevision) {
            err = {409, "revision_conflict", "配置已被并发修改，请重新读取后再写"};
            return false;
        }
        values_.Insert(wideKey, value);
        writes.emplace_back(key, CanonicalOf(value));
        out.key = key;
        out.value = value;
        out.revision = ComputeRevision(CanonicalPairs(values_));
        return true;
    }

    bool StartTask(const std::string& taskId, StartTaskData& out, OpError& err) override {
        if (running) {
            err = {409, "task_running", "已有任务正在运行（" + currentTask + "）"};
            return false;
        }
        startCalls.push_back(taskId);
        if (confirmStart) {
            running = true;
            currentTask = taskId;
        }
        // 接纳证据只有真实运行态；current_task 与请求 ID 相同不算（与生产一致）。
        out.status = running ? "accepted" : "unconfirmed";
        out.taskId = taskId;
        return true;
    }

    bool StopTask(int graceMs, StopTaskData& out, OpError&) override {
        if (!running) {
            out.status = "not_running";
            out.taskId = currentTask;
            return true;
        }
        if (stubbornStop) {
            out.status = "unknown";
            out.taskId = currentTask;
            out.note = "stop requested but host still reports running (fake grace " +
                       std::to_string(graceMs) + "ms)";
            return true;
        }
        running = false;
        out.status = "stopped";
        out.taskId = currentTask;
        currentTask.clear();
        return true;
    }

    bool ReadLogs(int tailLines, LogsData& out, OpError&) override {
        logLines_.clear();
        for (int i = 1; i <= 40; ++i) {
            logLines_.push_back("line " + std::to_string(i));
        }
        out.totalLines = int32_t(logLines_.size());
        if (int32_t(logLines_.size()) > tailLines) {
            out.lines.assign(logLines_.end() - tailLines, logLines_.end());
        } else {
            out.lines = logLines_;
        }
        return true;
    }

  private:
    static std::string TypeNameOf(const IJsonValue& value) {
        using winrt::Windows::Data::Json::JsonValueType;
        switch (value.ValueType()) {
            case JsonValueType::Boolean: return "bool";
            case JsonValueType::Number: return "int";
            case JsonValueType::String: return "str";
            default: return "other";
        }
    }

    JsonObject values_;
    std::vector<std::string> logLines_;
};

HostSnapshot MakeSnapshot(FakeHost* fake = nullptr) {
    HostSnapshot snapshot;
    snapshot.instanceId = "instance-0001";
    snapshot.hostVersion = "5.7.0-test";
    for (const auto& [id, description] : AllowedTasks()) {
        // 故意让宿主目录缺少 divergent，覆盖“白名单内但宿主没有”的路径。
        if (id == "divergent") {
            continue;
        }
        TaskEntry entry;
        entry.id = id;
        entry.label = id == "main" ? "完整运行" : ("任务-" + id);
        entry.description = description;
        snapshot.tasks.push_back(entry);
    }
    (void)fake;
    snapshot.catalogVersion = ComputeCatalogVersion(snapshot.tasks);
    return snapshot;
}

std::vector<std::string> TaskIds(const HostSnapshot& snapshot) {
    std::vector<std::string> ids;
    for (const TaskEntry& task : snapshot.tasks) {
        ids.push_back(task.id);
    }
    return ids;
}

} // namespace

int RunAll() {
    try {
        winrt::init_apartment(winrt::apartment_type::multi_threaded);
    } catch (...) {
        // 已初始化其它套间也能继续。
    }

    // ---- SHA-256 向量 ------------------------------------------------------
    Stage("sha256");
    Expect(Sha256Hex("abc").rfind("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad", 0) == 0,
           "sha256(abc) vector");

    // ---- 修订号指纹 ---------------------------------------------------------
    Stage("revision");
    std::vector<std::pair<std::string, std::string>> pairsA = {
        {"alpha", "b:1"}, {"beta", "s:3:xyz"}};
    std::vector<std::pair<std::string, std::string>> pairsB = {
        {"beta", "s:3:xyz"}, {"alpha", "b:1"}};
    std::vector<std::pair<std::string, std::string>> pairsC = {
        {"alpha", "b:0"}, {"beta", "s:3:xyz"}};
    Expect(ComputeRevision(pairsA) == ComputeRevision(pairsB), "revision order independent");
    Expect(ComputeRevision(pairsA) != ComputeRevision(pairsC), "revision detects value change");

    // ---- 目录版本（实例/目录冲突语义） ---------------------------------------
    Stage("catalog-version");
    HostSnapshot snapshotA = MakeSnapshot();
    HostSnapshot snapshotB = MakeSnapshot();
    Expect(snapshotA.catalogVersion == snapshotB.catalogVersion,
           "same catalog produces same version");
    HostSnapshot snapshotC = snapshotA;
    TaskEntry extra;
    extra.id = "divergent";
    extra.label = "差分宇宙";
    snapshotC.tasks.push_back(extra);
    snapshotC.catalogVersion = ComputeCatalogVersion(snapshotC.tasks);
    Expect(snapshotA.catalogVersion != snapshotC.catalogVersion,
           "different host catalog produces different version");

    // ---- 派发状态机 -----------------------------------------------------------
    Stage("dispatch-machine");
    Expect(ClassifyTaskProcessState(0, false) == TaskProcessState::Stopped, "process stopped evidence");
    Expect(ClassifyTaskProcessState(1, false) == TaskProcessState::Starting, "starting never treated as stopped");
    Expect(ClassifyTaskProcessState(2, false) == TaskProcessState::Running, "process running evidence");
    Expect(ClassifyTaskProcessState(99, false) == TaskProcessState::Unknown, "unknown process state preserved");
    Expect(ClassifyTaskProcessState(0, true) == TaskProcessState::Running, "external activity remains running");
    {
        std::atomic<int> phase{0};
        Expect(JobTryBeginRun(phase), "queued job can begin");
        JobMarkFinished(phase);
        Expect(WaitDispatch([](uint32_t) { return true; }, phase, 10) == DispatchStatus::Completed,
               "signaled job reports completed");
    }
    {
        std::atomic<int> phase{0};
        DispatchStatus status = WaitDispatch([](uint32_t) { return false; }, phase, 1);
        Expect(status == DispatchStatus::TimeoutNotStarted, "unstarted job times out cancelled");
        Expect(phase.load() == 3, "cancelled job will never run");
        Expect(!JobTryBeginRun(phase), "cancelled job cannot begin run");
    }
    {
        std::atomic<int> phase{0};
        Expect(JobTryBeginRun(phase), "running job begins");
        DispatchStatus status = WaitDispatch([](uint32_t) { return false; }, phase, 1);
        Expect(status == DispatchStatus::TimeoutStillRunning, "running job times out unknown");
        Expect(phase.load() == 1, "running job keeps its phase");
    }

    // ---- AsyncCall（生产异步内核 + 注入 fake 队列） ----------------------------
    Stage("asynccall");
    {
        // 内联 fake 队列：排队即执行。
        bool ran = false;
        AsyncCall call([&ran] { ran = true; });
        Expect(call.Enqueue([](std::function<void()> job) { job(); return true; }),
               "inline poster accepts");
        Expect(call.Wait(1000) == DispatchStatus::Completed, "inline run completes");
        Expect(ran && call.failure().empty(), "inline run result without failure");
    }
    {
        // 迟到回调：超时未开始被撤销后，主线程很晚才处理 pending 时绝不运行 body。
        bool bodyRan = false;
        std::function<void()> parked;
        AsyncCall call([&bodyRan] { bodyRan = true; });
        Expect(call.Enqueue([&](std::function<void()> job) { parked = job; return true; }),
               "parking poster accepts");
        Expect(call.Wait(1) == DispatchStatus::TimeoutNotStarted,
               "unstarted call times out cancelled");
        if (parked) {
            parked(); // 模拟迟到的主线程回调
        }
        Expect(!bodyRan, "cancelled late callback never runs body");
    }
    {
        // 运行超时：body 仍在执行时 Wait 返回 unknown；放行后补完成且结果完整。
        auto gate = std::make_shared<std::atomic<bool>>(false);
        int observed = 0;
        AsyncCall call([gate, &observed] {
            while (!gate->load()) {
                std::this_thread::sleep_for(std::chrono::milliseconds(2));
            }
            observed = 7; // Completed 之后调用方才读（与生产拷回 out 同纪律）
        });
        std::thread runner;
        Expect(call.Enqueue([&](std::function<void()> job) { runner = std::thread(job); return true; }),
               "thread poster accepts");
        Expect(call.Wait(50) == DispatchStatus::TimeoutStillRunning,
               "still-running body times out unknown");
        gate->store(true);
        Expect(call.Wait(5000) == DispatchStatus::Completed, "gated body finishes");
        runner.join();
        Expect(observed == 7, "late completion result intact");
    }
    {
        // body 异常：作为失败传回，不吞异常冒充无错完成。
        AsyncCall call([] { throw std::runtime_error("kaboom"); });
        Expect(call.Enqueue([](std::function<void()> job) { job(); return true; }),
               "inline poster accepts throwing body");
        Expect(call.Wait(100) == DispatchStatus::Completed, "throwing body still completes");
        Expect(call.failure().find("kaboom") != std::string::npos,
               "exception recorded as failure");
    }
    {
        // 排队失败：此后不会再有回调执行。
        bool bodyRan = false;
        AsyncCall call([&bodyRan] { bodyRan = true; });
        Expect(!call.Enqueue([](std::function<void()>) { return false; }),
               "poster failure surfaces");
        Expect(!bodyRan, "failed enqueue never runs body");
        Expect(call.Wait(1) == DispatchStatus::TimeoutNotStarted,
               "failed call stays unstarted");
    }
    {
        // 并发独立结果：两个 AsyncCall 各自状态互不串扰。
        auto slotA = std::make_shared<std::atomic<int>>(0);
        auto slotB = std::make_shared<std::atomic<int>>(0);
        AsyncCall callA([slotA] { slotA->store(11); });
        AsyncCall callB([slotB] { slotB->store(22); });
        std::function<void()> parkedA;
        std::function<void()> parkedB;
        Expect(callA.Enqueue([&](std::function<void()> job) { parkedA = job; return true; }),
               "park A");
        Expect(callB.Enqueue([&](std::function<void()> job) { parkedB = job; return true; }),
               "park B");
        std::thread threadA(parkedA);
        std::thread threadB(parkedB);
        threadA.join();
        threadB.join();
        Expect(callA.Wait(2000) == DispatchStatus::Completed, "A completes");
        Expect(callB.Wait(2000) == DispatchStatus::Completed, "B completes");
        Expect(slotA->load() == 11 && slotB->load() == 22,
               "concurrent results stay independent");
    }

    // ---- 请求校验与路由 --------------------------------------------------------
    Stage("routing");
    FakeHost fake;
    HostSnapshot snapshot = MakeSnapshot();
    g_fixtureInstance = snapshot.instanceId;
    g_fixtureCatalog = snapshot.catalogVersion;
    Bridge bridge("unit-test-token-0123456789abcdef", 23111, snapshot, &fake);

    Expect(bridge.Handle(MakeRequest("/status", "")).status == 200, "valid token accepted");
    {
        HttpResponse response = bridge.Handle(MakeRequest("/status", "", ""));
        Expect(response.status == 401, "missing token rejected");
        Expect(BodyString(response, L"error", L"code") == "unauthorized", "401 code");
    }
    Expect(bridge.Handle(MakeRequest("/status", "", "wrong-token")).status == 401,
           "wrong token rejected");
    Expect(bridge.Handle(MakeRequest("/status", "", "x")).status == 401,
           "arbitrary bad token rejected");
    Expect(bridge.Handle(MakeRequest("/status", "", "unit-test-token-0123456789abcdeX")).status ==
               401,
           "token mismatch rejected");
    {
        HttpRequest request = MakeRequest("/status", "");
        request.method = "GET";
        Expect(bridge.Handle(request).status == 405, "GET rejected");
    }
    Expect(bridge.Handle(MakeRequest("/admin/shell", "{}")).status == 404, "unknown path 404");
    Expect(bridge.Handle(MakeRequest("/status/../etc", "{}")).status == 404,
           "traversal-style path 404");
    {
        HttpRequest request = MakeRequest("/status", "");
        request.headers["origin"] = "http://evil.example";
        Expect(bridge.Handle(request).status == 403, "browser origin rejected");
        request.headers["origin"] = "http://127.0.0.1:23111";
        Expect(bridge.Handle(request).status == 200, "loopback origin accepted");
        request.headers["origin"] = "null";
        Expect(bridge.Handle(request).status == 403, "null origin rejected");
    }
    {
        HttpRequest request = MakeRequest("/status", "");
        request.headers["host"] = "evil.example";
        Expect(bridge.Handle(request).status == 403, "non-loopback Host rejected");
        request.headers["host"] = "127.0.0.1:23111";
        Expect(bridge.Handle(request).status == 200, "loopback Host accepted");
        request.headers["host"] = "localhost";
        Expect(bridge.Handle(request).status == 200, "bare localhost Host accepted");
    }
    {
        HttpRequest request = MakeRequest("/status", "");
        request.declaredContentLength = -1;
        request.headers.erase("content-length");
        Expect(bridge.Handle(request).status == 411, "missing Content-Length 411");
        request.declaredContentLength = 70000;
        request.headers["content-length"] = "70000";
        Expect(bridge.Handle(request).status == 413, "oversized body 413");
    }
    {
        HttpRequest request = MakeRequest("/status", "not-json{");
        Expect(bridge.Handle(request).status == 400, "invalid JSON 400");
    }

    // ---- 状态载荷 ---------------------------------------------------------------
    Stage("status-payload");
    {
        HttpResponse response = bridge.Handle(MakeRequest("/status", ""));
        Expect(response.status == 200, "status ok");
        JsonObject body = ParseBody(response);
        Expect(winrt::to_string(body.GetNamedString(L"protocolVersion")) == "1",
               "status protocolVersion");
        Expect(winrt::to_string(body.GetNamedString(L"instanceId")) == "instance-0001",
               "status instanceId");
        Expect(winrt::to_string(body.GetNamedString(L"catalogVersion")) == snapshot.catalogVersion,
               "status catalogVersion");
        Expect(winrt::to_string(body.GetNamedString(L"hostVersion")) == "5.7.0-test",
               "status hostVersion");
        Expect(!body.GetNamedBoolean(L"taskRunning"), "status not running");
    }

    // ---- 任务目录 ----------------------------------------------------------------
    {
        for (const char* code : {"identity_conflict", "revision_conflict", "task_running"}) {
            Expect(ParseBody(Bridge::Error(409, code, "rejected")).GetNamedString(L"submissionStatus") == L"notSubmitted",
                   "pre-action rejection carries explicit non-submission receipt");
        }
        for (const char* code : {"host_error", "dispatch_unknown", "write_unverified"}) {
            Expect(!ParseBody(Bridge::Error(500, code, "unknown")).HasKey(L"submissionStatus"),
                   "potential post-action failure remains unknown");
        }
    }
    Stage("tasks-catalog");
    {
        HttpResponse response = bridge.Handle(MakeRequest("/tasks/search", "{}"));
        JsonObject body = ParseBody(response);
        auto ids = [&]() {
            std::vector<std::string> found;
            for (const auto& item : body.GetNamedArray(L"tasks")) {
                found.push_back(winrt::to_string(item.GetObject().GetNamedString(L"id")));
            }
            return found;
        }();
        bool hasMain = false;
        bool hasDivergent = false;
        bool hasDenied = false;
        for (const std::string& id : ids) {
            hasMain = hasMain || id == "main";
            hasDivergent = hasDivergent || id == "divergent";
            hasDenied = hasDenied || DeniedTaskReasons().count(id) > 0;
        }
        Expect(hasMain, "search lists main");
        Expect(!hasDivergent, "search omits task absent in host catalog");
        Expect(!hasDenied, "search omits denied tasks");
    }
    {
        HttpResponse response =
            bridge.Handle(MakeRequest("/tasks/search", R"({"query":"完整运行"})"));
        Expect(response.status == 200, "search query ok");
        Expect(ParseBody(response).GetNamedArray(L"tasks").Size() >= 1, "search query matches");
    }
    {
        HttpResponse response = bridge.Handle(MakeRequest("/tasks/describe", R"({"taskId":"power"})"));
        Expect(response.status == 200, "describe power ok");
        Expect(winrt::to_string(ParseBody(response).GetNamedString(L"label")) == "任务-power",
               "describe label from host catalog");
    }
    Expect(bridge.Handle(MakeRequest("/tasks/describe", R"({"taskId":"app_update"})")).status ==
               403,
           "describe denied update task 403");
    Expect(bridge.Handle(MakeRequest("/tasks/describe", R"({"taskId":"currencywarsloop"})"))
               .status == 403,
           "describe denied loop task 403");
    Expect(bridge.Handle(MakeRequest("/tasks/describe", R"({"taskId":"format_c"})")).status == 404,
           "describe unknown task 404");
    {
        HttpResponse debugResponse =
            bridge.Handle(MakeRequest("/tasks/describe", R"({"taskId":"divergent"})"));
        Expect(debugResponse.status == 404, "describe allowed-but-absent task 404");
    }
    Expect(bridge.Handle(MakeRequest("/tasks/describe", "{}")).status == 400,
           "describe without taskId 400");

    // ---- 配置读取与脱敏（含数组内对象） ---------------------------------------------
    Stage("config-read");
    {
        HttpResponse response = bridge.Handle(MakeRequest("/config/read", "{}"));
        Expect(response.status == 200, "config read ok");
        JsonObject body = ParseBody(response);
        JsonObject values = body.GetNamedObject(L"values");
        Expect(values.GetNamedObject(L"notify_telegram_token").GetNamedBoolean(L"redacted"),
               "top-level sensitive key masked");
        Expect(values.GetNamedObject(L"notify_bark_cipherkey").GetNamedBoolean(L"redacted"), "cipher keys masked");
        Expect(values.GetNamedObject(L"notify_custom_url").GetNamedBoolean(L"redacted"), "credential-bearing endpoints masked");
        Expect(response.body.find("CIPHER-SECRET") == std::string::npos && response.body.find("PRIVATE-URL-KEY") == std::string::npos,
               "notification credentials never leave snapshot");
        Expect(values.GetNamedObject(L"notify_matrix_user_id").GetNamedBoolean(L"redacted"),
               "user id masked");
        Expect(values.GetNamedObject(L"fight_allow_memory_token").GetNamedBoolean(L"redacted"),
               "token-named game key masked conservatively");
        auto channels = values.GetNamedArray(L"notify_channels");
        Expect(channels.Size() == 2, "array preserved");
        Expect(channels.GetAt(0).GetObject().GetNamedObject(L"bot_token").GetNamedBoolean(L"redacted"),
               "object inside array masked");
        Expect(!channels.GetAt(0).GetObject().HasKey(L"chat_id") ||
                   channels.GetAt(0).GetObject().GetNamedString(L"chat_id") == L"42",
               "non-sensitive sibling kept");
        Expect(values.GetNamedObject(L"webhook_registry").GetNamedBoolean(L"redacted"),
               "sensitive container masked as a whole");
        Expect(values.GetNamedObject(L"nested_settings").GetNamedObject(L"default").GetNamedObject(L"access_token").GetNamedBoolean(L"redacted"),
               "nested dictionary secret masked");
        Expect(values.GetNamedBoolean(L"power_enable"), "plain value kept");
        bool listedRedacted = false;
        for (const auto& item : body.GetNamedArray(L"redactedKeys")) {
            if (winrt::to_string(item.GetString()).find("notify_channels[0].bot_token") !=
                std::string::npos) {
                listedRedacted = true;
            }
        }
        Expect(listedRedacted, "array element redaction listed");
        Expect(!body.GetNamedString(L"revision").empty(), "revision returned");
    }
    {
        HttpResponse response =
            bridge.Handle(MakeRequest("/config/read", R"({"keys":["power_enable","no_such_key"]})"));
        JsonObject body = ParseBody(response);
        Expect(body.GetNamedObject(L"values").GetNamedBoolean(L"power_enable"),
               "selected key returned");
        Expect(body.GetNamedArray(L"missingKeys").Size() == 1, "missing key reported");
    }
    {
        HttpResponse response = bridge.Handle(MakeRequest("/config/describe", "{}"));
        JsonObject body = ParseBody(response);
        bool sawPresent = false;
        bool sawAbsent = false;
        for (const auto& item : body.GetNamedArray(L"fields")) {
            std::string key = winrt::to_string(item.GetObject().GetNamedString(L"key"));
            if (key == "power_enable") {
                sawPresent = item.GetObject().GetNamedBoolean(L"presentInHost");
            }
            if (key == "calyx_golden_preference") {
                sawAbsent = !item.GetObject().GetNamedBoolean(L"presentInHost");
            }
        }
        Expect(sawPresent, "describe marks present field");
        Expect(sawAbsent, "describe marks absent field");
    }

    // ---- 配置写入：revision 必填、白名单、类型、冲突 ------------------------------
    Stage("config-write");
    {
        HttpResponse response =
            bridge.Handle(MakeRequest("/config/write", R"({"key":"power_enable","value":false})"));
        Expect(response.status == 400, "write without revision rejected");
        Expect(BodyString(response, L"error", L"code") == "revision_required",
               "revision required code");
    }
    Expect(bridge.Handle(MakeRequest(
                           "/config/write",
                           R"({"key":"python_exe_path","value":"C:\\evil\\python.exe","revision":"r1"})"))
               .status == 403,
           "dangerous path field rejected");
    Expect(bridge.Handle(MakeRequest(
                           "/config/write",
                           R"({"key":"after_finish","value":"RunScript","revision":"r1"})"))
               .status == 403,
           "dangerous finish field rejected");
    Expect(bridge.Handle(MakeRequest(
                           "/config/write", R"({"key":"notify_telegram_token","value":"x","revision":"r1"})"))
               .status == 403,
           "sensitive field write rejected");
    Expect(
        bridge.Handle(MakeRequest("/config/write", R"({"key":"locales","value":"en_US","revision":"r1"})"))
            .status == 403,
        "non-whitelist field rejected");
    std::string revision;
    {
        HttpResponse response = bridge.Handle(MakeRequest("/config/read", "{}"));
        revision = winrt::to_string(ParseBody(response).GetNamedString(L"revision"));
        Expect(!revision.empty(), "read revision");
    }
    Expect(bridge.Handle(MakeRequest("/config/write",
                                     R"({"key":"power_enable","value":"false","revision":")" +
                                         revision + "\"}"))
               .status == 422,
           "boolean type mismatch rejected");
    Expect(bridge.Handle(MakeRequest("/config/write",
                                     R"({"key":"instance_type","value":"任意副本","revision":")" +
                                         revision + "\"}"))
               .status == 422,
           "enum choice rejected");
    Expect(bridge.Handle(MakeRequest(
                           "/config/write",
                           R"({"key":"calyx_golden_preference","value":"Penacony","revision":")" +
                               revision + "\"}"))
               .status == 422,
           "absent-in-host field rejected");
    {
        HttpResponse response = bridge.Handle(
            MakeRequest("/config/write",
                        R"({"key":"power_enable","value":false,"revision":")" + revision + "\"}"));
        Expect(response.status == 200, "valid write accepted");
        JsonObject body = ParseBody(response);
        std::string newRevision = winrt::to_string(body.GetNamedString(L"revision"));
        Expect(!newRevision.empty() && newRevision != revision, "write returns new revision");
        Expect(fake.writes.size() == 1 && fake.writes[0].first == "power_enable",
               "fake host recorded write");
    }
    {
        // 并发修改：旧 revision 必须冲突。
        HttpResponse response = bridge.Handle(
            MakeRequest("/config/write",
                        R"({"key":"tp_before_instance","value":true,"revision":")" + revision +
                            "\"}"));
        Expect(response.status == 409, "stale revision conflicts");
        Expect(BodyString(response, L"error", L"code") == "revision_conflict",
               "conflict code accurate");
    }
    {
        fake.ExternalEdit("tp_before_instance", L"true");
        HttpResponse response = bridge.Handle(MakeRequest("/config/read", "{}"));
        std::string fresh = winrt::to_string(ParseBody(response).GetNamedString(L"revision"));
        HttpResponse applied = bridge.Handle(
            MakeRequest("/config/write",
                        R"({"key":"tp_before_instance","value":false,"revision":")" + fresh +
                            "\"}"));
        Expect(applied.status == 200, "fresh revision writes after external edit");
    }

    // ---- 任务启动：接纳确认与未知 -------------------------------------------------
    Stage("task-start-stop");
    {
        auto missing = MakeRequest("/task/start", R"({"taskId":"main"})", "unit-test-token-0123456789abcdef", "POST", false);
        Expect(bridge.Handle(missing).status == 400, "write without current host identity rejected");
        auto stale = MakeRequest("/task/start", R"({"taskId":"main","instanceId":"previous-session"})");
        Expect(bridge.Handle(stale).status == 409, "stale host identity rejected");
        auto staleCatalog = MakeRequest("/task/start", R"({"taskId":"main","catalogVersion":"previous-catalog"})");
        Expect(bridge.Handle(staleCatalog).status == 409, "stale catalog rejected");
        Expect(fake.startCalls.empty(), "identity conflicts never reach host mutation");
    }
    {
        HttpResponse response = bridge.Handle(MakeRequest("/task/start", R"({"taskId":"main"})"));
        Expect(response.status == 200, "task start accepted");
        Expect(BodyString(response, L"verification", L"status") == "succeeded", "confirmed launch verified");
        Expect(BodyString(response, L"verification", L"scope") == "launch", "verification covers launch only");
        Expect(winrt::to_string(ParseBody(response).GetNamedString(L"status")) == "accepted",
               "start status accepted only when confirmed");
        Expect(fake.startCalls.size() == 1 && fake.startCalls[0] == "main",
               "start called with bare id");
    }
    {
        HttpResponse response = bridge.Handle(MakeRequest("/task/start", R"({"taskId":"daily"})"));
        Expect(response.status == 409, "start while running conflicts");
        Expect(BodyString(response, L"error", L"code") == "task_running", "running code");
    }
    Expect(bridge.Handle(MakeRequest("/task/stop", "{}")).status == 200, "stop ok");
    Expect(winrt::to_string(ParseBody(bridge.Handle(MakeRequest("/task/stop", "{}")))
                                .GetNamedString(L"status")) == "not_running",
           "stop when idle reports not_running");
    Expect(bridge.Handle(MakeRequest("/task/start", R"({"taskId":"app_update"})")).status == 403,
           "start denied update task");
    Expect(bridge.Handle(MakeRequest("/task/start", R"({"taskId":"rm_rf"})")).status == 404,
           "start unknown task");
    Expect(bridge.Handle(MakeRequest("/task/start", R"({"taskId":"divergent"})")).status == 404,
           "start allowed-but-absent task");
    {
        fake.confirmStart = false;
        HttpResponse response = bridge.Handle(MakeRequest("/task/start", R"({"taskId":"main"})"));
        Expect(response.status == 200, "unconfirmed start still 200 with explicit status");
        Expect(!ParseBody(response).HasKey(L"verification"), "unconfirmed launch never claims verified success");
        Expect(winrt::to_string(ParseBody(response).GetNamedString(L"status")) == "unconfirmed",
               "void return without running state is NOT accepted");
        fake.confirmStart = true;
    }
    {
        // 停止确认：正常停止
        fake.running = true;
        fake.currentTask = "fight";
        HttpResponse response = bridge.Handle(MakeRequest("/task/stop", R"({"graceMs":500})"));
        Expect(winrt::to_string(ParseBody(response).GetNamedString(L"status")) == "stopped",
               "stop confirmed via running state");
        Expect(!fake.running, "fake actually stopped");
    }
    {
        // 停止超时：宿主仍报告运行 → unknown，不冒充成功
        fake.running = true;
        fake.currentTask = "universe";
        fake.stubbornStop = true;
        HttpResponse response = bridge.Handle(MakeRequest("/task/stop", "{}"));
        Expect(winrt::to_string(ParseBody(response).GetNamedString(L"status")) == "unknown",
               "stubborn stop reports unknown");
        Expect(ParseBody(response).HasKey(L"note"), "unknown carries note");
        fake.stubbornStop = false;
        fake.running = false;
        fake.currentTask.clear();
    }
    Expect(bridge.Handle(MakeRequest("/task/stop", R"({"graceMs":"soon"})")).status == 400,
           "non-integer grace rejected");
    Expect(bridge.Handle(MakeRequest("/task/stop", R"({"graceMs":99999})")).status == 400,
           "out-of-range grace rejected");

    // ---- 日志 ------------------------------------------------------------------
    Stage("logs");
    {
        HttpResponse response = bridge.Handle(MakeRequest("/logs/read", R"({"tailLines":5})"));
        JsonObject body = ParseBody(response);
        Expect(body.GetNamedArray(L"lines").Size() == 5, "log tail lines");
        Expect(int(body.GetNamedNumber(L"totalLines")) == 40, "log total lines");
        Expect(winrt::to_string(body.GetNamedArray(L"lines").GetAt(4).GetString()) == "line 40",
               "log tail content");
    }
    Expect(bridge.Handle(MakeRequest("/logs/read", R"({"tailLines":0})")).status == 400,
           "log tail bounds rejected");

    // ---- 关桥：只停自建服务 ----------------------------------------------------------
    Stage("shutdown");
    {
        HttpResponse response = bridge.Handle(MakeRequest("/shutdown", "{}"));
        Expect(response.status == 400, "shutdown requires confirm");
        response = bridge.Handle(MakeRequest("/shutdown", R"({"confirm":true})"));
        Expect(response.status == 200, "shutdown accepted");
        Expect(bridge.ConsumeShutdown(), "shutdown flag consumed once");
        Expect(!bridge.ConsumeShutdown(), "shutdown flag not duplicated");
    }

    std::printf("checks=%d failures=%d\n", g_checks, g_failures);
    return g_failures == 0 ? 0 : 1;
}

int main() {
    // 无缓冲输出：进程意外终止时已完成阶段与 FAIL 行不随缓冲丢失。
    std::setvbuf(stdout, nullptr, _IONBF, 0);
    try {
        return RunAll();
    } catch (const winrt::hresult_error& error) {
        std::printf("FATAL: winrt error 0x%08lX: %ls\n",
                    static_cast<unsigned long>(error.code().value), error.message().c_str());
        return 2;
    } catch (const std::exception& error) {
        std::printf("FATAL: std::exception: %s\n", error.what());
        return 2;
    } catch (...) {
        std::printf("FATAL: unknown exception\n");
        return 2;
    }
}
