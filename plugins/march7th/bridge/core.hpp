// 三月七原生桥：纯逻辑层公共头。
// 只依赖 STL 与 C++/WinRT（Windows.Data.Json），不触碰 CPython、WinSock、
// 进程注入 API —— 这些在测试构建中必须被排除。
#pragma once

#include <winrt/Windows.Data.Json.h>
#include <winrt/Windows.Foundation.Collections.h>

#include <algorithm>
#include <atomic>
#include <cstdint>
#include <functional>
#include <map>
#include <optional>
#include <string>
#include <string_view>
#include <vector>

namespace sdm::march7th {

using winrt::Windows::Data::Json::IJsonValue;
using winrt::Windows::Data::Json::JsonObject;
using winrt::Windows::Data::Json::JsonValue;

// ---- 协议常量（与 plugin.json 清单保持一致） ------------------------------

inline constexpr const char* kProtocolVersion = "1";
inline constexpr uint16_t kDefaultPort = 23111;
inline constexpr const char* kTokenHeader = "x-sleepy-token";
inline constexpr const char* kTokenEnvName = "MARCH7TH_BRIDGE_TOKEN";
inline constexpr size_t kMaxBodyBytes = 64 * 1024;
inline constexpr uint32_t kDispatchTimeoutMs = 8000;
inline constexpr uint32_t kStopGraceMs = 3000;
inline constexpr uint32_t kStopPollIntervalMs = 200;
inline constexpr int kMaxLogLines = 2000;
inline constexpr const char* kPolicyRevision = "2026-11-07.2";

inline const std::vector<std::string>& EndpointList() {
    static const std::vector<std::string> endpoints = {
        "/status",       "/tasks/search", "/tasks/describe", "/config/read",
        "/config/describe", "/config/write", "/task/start", "/task/stop",
        "/logs/read",    "/shutdown",
    };
    return endpoints;
}

// ---- SHA-256（无外部依赖；修订号/目录版本指纹用） -------------------------

struct Sha256Digest {
    uint8_t bytes[32];
};

Sha256Digest Sha256(const void* data, size_t length);
std::string Sha256Hex(std::string_view text);
// 取前 16 个十六进制字符作为短指纹。
std::string ShortDigest(const Sha256Digest& digest);

// ---- 数据结构 ------------------------------------------------------------

struct TaskEntry {
    std::string id;
    std::string label;       // 宿主 AVAILABLE_TASKS 里的中文名（附着一 读）
    std::string description; // 白名单内置用途说明
};

struct HostSnapshot {
    std::string instanceId;      // 每次 StartBridge 随机生成
    std::string catalogVersion;  // 协议+生效白名单+可写策略指纹
    std::string hostVersion;     // 附着时 cfg.version
    std::vector<TaskEntry> tasks; // 白名单 ∩ 宿主目录
};

// 宿主配置快照：values 是完整（未脱敏）副本，仅存在于本进程内；
// 出协议前必须先过 ApplySensitiveMasking。
struct ConfigSnapshotData {
    JsonObject values;
    std::vector<std::string> allKeys;
    std::string revision;
};

struct OpError {
    int httpStatus = 0;
    std::string code;
    std::string message;
};

enum class TaskProcessState { Unknown, Stopped, Starting, Running };
TaskProcessState ClassifyTaskProcessState(int processState, bool externalActive);
const char* TaskProcessStateName(TaskProcessState state);

struct TaskStatusData {
    bool running = false;
    TaskProcessState processState = TaskProcessState::Unknown;
    std::string currentTask; // 空 = 未知/无
};

struct StartTaskData {
    std::string status; // "accepted" | "unconfirmed"
    std::string taskId;
};

struct StopTaskData {
    std::string status; // "stopped" | "not_running" | "unknown"
    std::string taskId;
    std::string note;
};

struct WriteResultData {
    std::string key;
    IJsonValue value{nullptr};
    std::string revision;
};

struct LogsData {
    std::vector<std::string> lines;
    int32_t totalLines = 0;
};

// ---- 宿主操作接口：DLL 内由 CPython 反射层实现，测试用 fake ---------------

class HostOps {
  public:
    virtual ~HostOps() = default;
    // 以下方法返回 false 时通过 err 给出 HTTP 错误；true 时 out 有效。
    virtual bool TaskStatus(TaskStatusData& out, OpError& err) = 0;
    virtual bool SnapshotConfig(ConfigSnapshotData& out, OpError& err) = 0;
    virtual bool WriteConfigField(const std::string& key, const IJsonValue& value,
                                  const std::string& baseRevision, WriteResultData& out,
                                  OpError& err) = 0;
    virtual bool StartTask(const std::string& taskId, StartTaskData& out, OpError& err) = 0;
    virtual bool StopTask(int graceMs, StopTaskData& out, OpError& err) = 0;
    virtual bool ReadLogs(int tailLines, LogsData& out, OpError& err) = 0;
};

// ---- HTTP 请求/响应（传输层解析，本层校验与路由） --------------------------

struct HttpRequest {
    std::string method;  // 大写
    std::string rawPath; // 含可能的查询串
    std::map<std::string, std::string> headers; // 键已转小写
    std::string body;    // 已按 Content-Length 读满（受上限约束）
    // 声明的 Content-Length（未声明为 -1；传输层拒绝非法值）
    int64_t declaredContentLength = -1;
};

struct HttpResponse {
    int status = 200;
    std::string body; // JSON 文本
};

// ---- 桥核心 ----------------------------------------------------------------

class Bridge {
  public:
    Bridge(std::string token, uint16_t port, HostSnapshot snapshot, HostOps* ops);

    // 处理一个已解析请求；纯逻辑，可离线测试。
    HttpResponse Handle(const HttpRequest& request);

    static HttpResponse Error(int status, std::string_view code, std::string_view message);

    // /shutdown 被合法触发后置位；传输层轮询后退出服务。
    bool ConsumeShutdown();
    bool ShutdownRequested() const { return shutdownRequested_.load(); }
    // 管道控制面 STOP 请求停止：语义与 /shutdown 相同（只停桥自建服务）。
    void RequestShutdown() { shutdownRequested_.store(true); }

    const std::string& token() const { return token_; }
    uint16_t port() const { return port_; }
    const HostSnapshot& snapshot() const { return snapshot_; }

  private:
    std::string token_;
    uint16_t port_;
    HostSnapshot snapshot_;
    HostOps* ops_;
    std::atomic<bool> shutdownRequested_{false};
};

// ---- 目录与策略（catalog） --------------------------------------------------

// 白名单内置任务：ID -> 用途说明。
const std::map<std::string, std::string>& AllowedTasks();
// 明确拒绝的内置任务：ID -> 拒绝原因（会进入模型可见的错误消息）。
const std::map<std::string, std::string>& DeniedTaskReasons();

struct WritableFieldSpec {
    std::string type;                  // boolean | integer | string
    std::vector<std::string> choices;  // 空 = 无枚举限制
    std::string description;
};
// 可写字段白名单（仅简单类型；必须同时已存在于宿主配置）。
const std::map<std::string, WritableFieldSpec>& WritableFields();

bool IsSensitiveKey(std::string_view key);
// 命中危险（可执行/路径/计划/自动启动等）模式时返回类别说明。
std::optional<std::string> DenyWriteReason(std::string_view key);

// 校验 (key, value)：返回 0 通过；否则返回 HTTP 状态码并填 code/message。
int CheckWritableField(std::string_view key, const IJsonValue& value, std::string& code,
                       std::string& message);

// 就地把敏感值替换为 {"redacted":true}，返回被脱敏的键列表。
std::vector<std::string> ApplySensitiveMasking(JsonObject& values, bool recursive);

// ---- 指纹 -------------------------------------------------------------------

// 修订号：对 (键, 规范标量) 对列表的有序指纹。敏感值参与哈希但不出进程。
std::string ComputeRevision(const std::vector<std::pair<std::string, std::string>>& pairs);
std::string ComputeCatalogVersion(const std::vector<TaskEntry>& tasks);

// ---- 主线程任务状态机（Py_AddPendingCall 派发的取消/超时语义） ---------------

// phase: 0=已排队 1=执行中 2=已完成 3=已取消
enum class DispatchStatus {
    Completed,          // 任务已在主线程执行完毕
    ScheduleFailed,     // 排队失败（解释器忙碌/退出中）
    TimeoutNotStarted,  // 超时且尚未开始执行，已撤销（不会再执行）
    TimeoutStillRunning // 超时但已开始执行，结果未知
};

// waitFn(timeoutMs) -> true 表示完成事件已触发。
DispatchStatus WaitDispatch(const std::function<bool(uint32_t)>& waitFn,
                            std::atomic<int>& phase, uint32_t timeoutMs);

// ---- 主线程异步调用内核 -------------------------------------------------------
//
// 生产环境经 Py_AddPendingCall 派发到 CPython 主线程；测试可直接调用
// RunOnTargetThread 模拟迟到回调。事件句柄由 State（shared_ptr 管理）独占，
// 蹦床与调用方共同持引用，最后一个释放者关闭句柄——不存在"SetEvent 后立即
// CloseHandle 而等待方尚未返回"的窗口。排队失败时调用方是唯一持有者，
// 析构即关闭。闭包内抛出的异常被捕获并记入 failure()，绝不跨回调 ABI。

class AsyncCall {
  public:
    explicit AsyncCall(std::function<void()> body);
    ~AsyncCall();
    AsyncCall(const AsyncCall&) = delete;
    AsyncCall& operator=(const AsyncCall&) = delete;

    // poster 负责把 RunOnTargetThread 的等价闭包排到目标线程；
    // 返回 false 表示排队失败（此后不会再有任何回调执行）。
    using Poster = std::function<bool(std::function<void()>)>;
    bool Enqueue(const Poster& poster);

    // 调用线程等待结果；完成后可读 failure()。
    DispatchStatus Wait(uint32_t timeoutMs);

    // 目标线程入口：已撤销（超时未执行）则绝不执行；异常转为 failure。
    void RunOnTargetThread() noexcept;

    const std::string& failure() const;

  private:
    struct State;
    // 目标线程执行体：撤销态绝不运行 body；异常转为 failure 后仍标记完成。
    static void RunCaptured(State* state) noexcept;
    std::shared_ptr<State> state_;
};

// 供执行侧（主线程蹦床）使用。
bool JobTryBeginRun(std::atomic<int>& phase);
void JobMarkFinished(std::atomic<int>& phase);

} // namespace sdm::march7th
