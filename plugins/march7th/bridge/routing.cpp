// 三月七原生桥：请求校验与端点路由（纯逻辑，不含套接字/CPython）。
#include "core.hpp"

#include <cctype>
#include <cmath>

namespace sdm::march7th {

namespace {

bool ConstantTimeEqual(const std::string& a, const std::string& b) {
    volatile unsigned char diff = unsigned char(a.size() ^ b.size());
    size_t count = std::min(a.size(), b.size());
    for (size_t i = 0; i < count; ++i) {
        diff |= unsigned char(a[i]) ^ unsigned char(b[i]);
    }
    return diff == 0;
}

std::string ToLower(std::string text) {
    for (char& c : text) {
        c = char(std::tolower(unsigned char(c)));
    }
    return text;
}

uint16_t ParsePort(const char* begin, size_t length) {
    int value = 0;
    for (size_t i = 0; i < length; ++i) {
        if (begin[i] < '0' || begin[i] > '9') {
            return 0;
        }
        value = value * 10 + (begin[i] - '0');
        if (value > 65535) {
            return 0;
        }
    }
    return uint16_t(value);
}

bool LoopbackHost(std::string_view hostHeader, uint16_t port) {
    // hostHeader 形如 127.0.0.1:23111 / localhost / [::1]:23111
    std::string_view name = hostHeader;
    uint16_t parsedPort = 0;
    if (!name.empty() && name.front() == '[') {
        size_t close = name.find(']');
        if (close == std::string_view::npos) {
            return false;
        }
        std::string_view ipv6 = name.substr(1, close - 1);
        if (close + 2 < name.size() && name[close + 1] == ':') {
            parsedPort = ParsePort(name.data() + close + 2, name.size() - close - 2);
        } else if (close + 1 != name.size()) {
            return false;
        }
        name = ipv6;
    } else {
        size_t colon = name.rfind(':');
        if (colon != std::string_view::npos) {
            if (name.find(':') != colon) {
                return false; // 裸 IPv6 不带方括号，非法
            }
            parsedPort = ParsePort(name.data() + colon + 1, name.size() - colon - 1);
            name = name.substr(0, colon);
        }
    }
    if (name != "127.0.0.1" && name != "localhost" && name != "::1") {
        return false;
    }
    // 显式带端口时必须是自己；不带端口也接受（http/1.0 风格）。
    return parsedPort == 0 || parsedPort == port;
}

bool LoopbackOrigin(const std::string& origin, uint16_t port) {
    std::string lowered = ToLower(origin);
    while (!lowered.empty() && (lowered.back() == '/')) {
        lowered.pop_back();
    }
    return lowered == "http://127.0.0.1:" + std::to_string(port) ||
           lowered == "http://localhost:" + std::to_string(port) ||
           lowered == "http://[::1]:" + std::to_string(port);
}

bool ParseJsonObject(const std::string& body, JsonObject& out) {
    if (body.empty()) {
        out = JsonObject();
        return true;
    }
    try {
        out = JsonObject::Parse(winrt::to_hstring(body));
        return true;
    } catch (...) {
        return false;
    }
}

std::optional<std::string> GetStringField(const JsonObject& object, const wchar_t* name) {
    if (!object.HasKey(name)) {
        return std::nullopt;
    }
    IJsonValue value = object.GetNamedValue(name);
    if (value == nullptr || value.ValueType() != winrt::Windows::Data::Json::JsonValueType::String) {
        return std::nullopt;
    }
    return winrt::to_string(value.GetString());
}

struct ParsedRequestLine {
    std::string method;
    std::string path; // 去掉查询串与尾斜杠
};

std::string StatusToJsonString(const TaskStatusData& status, const HostSnapshot& snapshot) {
    JsonObject object;
    object.Insert(L"protocolVersion", JsonValue::CreateStringValue(winrt::to_hstring(kProtocolVersion)));
    object.Insert(L"instanceId", JsonValue::CreateStringValue(winrt::to_hstring(snapshot.instanceId)));
    object.Insert(L"catalogVersion", JsonValue::CreateStringValue(winrt::to_hstring(snapshot.catalogVersion)));
    object.Insert(L"policyRevision", JsonValue::CreateStringValue(winrt::to_hstring(kPolicyRevision)));
    object.Insert(L"hostVersion", JsonValue::CreateStringValue(winrt::to_hstring(snapshot.hostVersion)));
    object.Insert(L"taskRunning", JsonValue::CreateBooleanValue(status.running));
    object.Insert(L"taskState", JsonValue::CreateStringValue(winrt::to_hstring(TaskProcessStateName(status.processState))));
    object.Insert(
        L"currentTask",
        status.currentTask.empty() ? JsonValue::CreateNullValue()
                                   : JsonValue::CreateStringValue(winrt::to_hstring(status.currentTask)));
    return winrt::to_string(object.Stringify());
}

} // namespace

Bridge::Bridge(std::string token, uint16_t port, HostSnapshot snapshot, HostOps* ops)
    : token_(std::move(token)), port_(port), snapshot_(std::move(snapshot)), ops_(ops) {}

bool Bridge::ConsumeShutdown() {
    bool expected = true;
    return shutdownRequested_.compare_exchange_strong(expected, false);
}

HttpResponse Bridge::Error(int status, std::string_view code, std::string_view message) {
    JsonObject error;
    error.Insert(L"code", JsonValue::CreateStringValue(winrt::to_hstring(code)));
    error.Insert(L"message", JsonValue::CreateStringValue(winrt::to_hstring(message)));
    JsonObject body;
    body.Insert(L"error", error);
    // 仅这些分支已在调用宿主动作之前拒绝；不得根据 HTTP 状态推断未提交。
    // host_error、dispatch_unknown、write_unverified 等可能在动作之后发生，保留未知。
    static const std::vector<std::string_view> rejected = {
        "method_not_allowed", "unknown_endpoint", "unauthorized", "host_forbidden",
        "origin_forbidden", "length_required", "payload_too_large", "bad_request",
        "identity_required", "identity_conflict", "revision_required", "revision_conflict",
        "task_unknown", "task_denied", "task_running", "field_not_present", "host_state_changed"
    };
    if (std::find(rejected.begin(), rejected.end(), code) != rejected.end()) {
        body.Insert(L"submissionStatus", JsonValue::CreateStringValue(L"notSubmitted"));
    }
    HttpResponse response;
    response.status = status;
    response.body = winrt::to_string(body.Stringify());
    return response;
}

HttpResponse Bridge::Handle(const HttpRequest& request) {
    // 1) 方法
    if (request.method != "POST") {
        return Error(405, "method_not_allowed", "桥只接受 POST");
    }
    // 2) 路径白名单
    std::string path = request.rawPath;
    size_t query = path.find('?');
    if (query != std::string::npos) {
        path = path.substr(0, query);
    }
    while (path.size() > 1 && path.back() == '/') {
        path.pop_back();
    }
    if (path.empty()) {
        path = "/";
    }
    bool known = false;
    for (const std::string& endpoint : EndpointList()) {
        if (endpoint == path) {
            known = true;
            break;
        }
    }
    if (!known) {
        return Error(404, "unknown_endpoint", "未知端点 " + path);
    }
    // 3) 令牌（常数时间比较）
    auto tokenHeader = request.headers.find(kTokenHeader);
    if (tokenHeader == request.headers.end() ||
        !ConstantTimeEqual(tokenHeader->second, token_)) {
        return Error(401, "unauthorized", "缺少或错误的 X-Sleepy-Token");
    }
    // 4) Host 必须回环
    auto hostHeader = request.headers.find("host");
    if (hostHeader != request.headers.end() &&
        !LoopbackHost(hostHeader->second, port_)) {
        return Error(403, "host_forbidden", "Host 必须是回环地址");
    }
    // 5) Origin 存在时只接受本机回环来源
    auto originHeader = request.headers.find("origin");
    if (originHeader != request.headers.end() &&
        !LoopbackOrigin(originHeader->second, port_)) {
        return Error(403, "origin_forbidden", "Origin 不被接受");
    }
    // 6) 请求体上限
    if (request.declaredContentLength < 0) {
        return Error(411, "length_required", "缺少 Content-Length");
    }
    if (uint64_t(request.declaredContentLength) > kMaxBodyBytes) {
        return Error(413, "payload_too_large", "请求体超过上限");
    }
    // 7) JSON 解析
    JsonObject arguments;
    if (!ParseJsonObject(request.body, arguments)) {
        return Error(400, "bad_request", "请求体必须是 UTF-8 JSON 对象");
    }

    auto requireString = [&](const wchar_t* field, std::string& out) -> bool {
        auto value = GetStringField(arguments, field);
        if (!value.has_value() || value->empty()) {
            return false;
        }
        out = *value;
        return true;
    };

    OpError err;
    if (path == "/config/write" || path == "/task/start" || path == "/task/stop") {
        std::string instanceId, catalogVersion;
        if (!requireString(L"instanceId", instanceId) || !requireString(L"catalogVersion", catalogVersion)) {
            return Error(400, "identity_required", "写操作必须携带本次 status 返回的 instanceId 和 catalogVersion");
        }
        if (instanceId != snapshot_.instanceId || catalogVersion != snapshot_.catalogVersion) {
            return Error(409, "identity_conflict", "桥会话或目录已变化，本次未执行，请重新读取 status");
        }
    }
    if (path == "/status") {
        TaskStatusData data;
        if (!ops_->TaskStatus(data, err)) {
            return Error(err.httpStatus, err.code, err.message);
        }
        HttpResponse response;
        response.body = StatusToJsonString(data, snapshot_);
        return response;
    }
    if (path == "/tasks/search") {
        auto queryText = GetStringField(arguments, L"query").value_or("");
        std::string lowered = ToLower(queryText);
        winrt::Windows::Data::Json::JsonArray array;
        for (const TaskEntry& task : snapshot_.tasks) {
            std::string haystack = ToLower(task.id + " " + task.label + " " + task.description);
            if (!lowered.empty() && haystack.find(lowered) == std::string::npos) {
                continue;
            }
            JsonObject item;
            item.Insert(L"id", JsonValue::CreateStringValue(winrt::to_hstring(task.id)));
            item.Insert(L"label", JsonValue::CreateStringValue(winrt::to_hstring(task.label)));
            item.Insert(L"description",
                        JsonValue::CreateStringValue(winrt::to_hstring(task.description)));
            array.Append(item);
        }
        JsonObject body;
        body.Insert(L"tasks", array);
        body.Insert(L"total", JsonValue::CreateNumberValue(double(array.Size())));
        body.Insert(L"deniedCount",
                    JsonValue::CreateNumberValue(double(DeniedTaskReasons().size())));
        HttpResponse response;
        response.body = winrt::to_string(body.Stringify());
        return response;
    }
    if (path == "/tasks/describe") {
        std::string taskId;
        if (!requireString(L"taskId", taskId)) {
            return Error(400, "bad_request", "taskId 必须是非空字符串");
        }
        const auto& allowed = AllowedTasks();
        auto allowedIt = allowed.find(taskId);
        if (allowedIt != allowed.end()) {
            for (const TaskEntry& task : snapshot_.tasks) {
                if (task.id == taskId) {
                    JsonObject body;
                    body.Insert(L"id", JsonValue::CreateStringValue(winrt::to_hstring(task.id)));
                    body.Insert(L"label",
                                JsonValue::CreateStringValue(winrt::to_hstring(task.label)));
                    body.Insert(L"description",
                                JsonValue::CreateStringValue(winrt::to_hstring(allowedIt->second)));
                    body.Insert(L"startable", JsonValue::CreateBooleanValue(true));
                    body.Insert(L"note", JsonValue::CreateStringValue(
                        L"接纳并确认提交后才算启动；是否成功以 status 与日志为准"));
                    HttpResponse response;
                    response.body = winrt::to_string(body.Stringify());
                    return response;
                }
            }
            return Error(404, "task_unknown", "宿主目录中没有内置任务 " + taskId);
        }
        const auto& denied = DeniedTaskReasons();
        auto deniedIt = denied.find(taskId);
        if (deniedIt != denied.end()) {
            return Error(403, "task_denied", deniedIt->second);
        }
        return Error(404, "task_unknown", "未知任务 " + taskId);
    }
    if (path == "/config/read" || path == "/config/describe") {
        ConfigSnapshotData snapshotData;
        if (!ops_->SnapshotConfig(snapshotData, err)) {
            return Error(err.httpStatus, err.code, err.message);
        }
        if (path == "/config/describe") {
            winrt::Windows::Data::Json::JsonArray array;
            for (const auto& [key, spec] : WritableFields()) {
                JsonObject field;
                field.Insert(L"key", JsonValue::CreateStringValue(winrt::to_hstring(key)));
                field.Insert(L"type", JsonValue::CreateStringValue(winrt::to_hstring(spec.type)));
                field.Insert(L"description",
                             JsonValue::CreateStringValue(winrt::to_hstring(spec.description)));
                bool present = false;
                for (const std::string& existing : snapshotData.allKeys) {
                    if (existing == key) {
                        present = true;
                        break;
                    }
                }
                field.Insert(L"presentInHost", JsonValue::CreateBooleanValue(present));
                if (!spec.choices.empty()) {
                    winrt::Windows::Data::Json::JsonArray choices;
                    for (const std::string& choice : spec.choices) {
                        choices.Append(JsonValue::CreateStringValue(winrt::to_hstring(choice)));
                    }
                    field.Insert(L"choices", choices);
                }
                array.Append(field);
            }
            JsonObject body;
            body.Insert(L"fields", array);
            body.Insert(
                L"note",
                JsonValue::CreateStringValue(
                    L"仅 presentInHost 为 true 的字段可写；写入需携带 config.read 返回的 revision"));
            HttpResponse response;
            response.body = winrt::to_string(body.Stringify());
            return response;
        }
        // /config/read：先脱敏再按 keys 过滤；过滤路径也只取脱敏后的值。
        JsonObject values = snapshotData.values;
        std::vector<std::string> redacted = ApplySensitiveMasking(values, true);
        if (arguments.HasKey(L"keys")) {
            IJsonValue keysValue = arguments.GetNamedValue(L"keys");
            if (keysValue.ValueType() != winrt::Windows::Data::Json::JsonValueType::Array) {
                return Error(400, "bad_request", "keys 必须是字符串数组");
            }
            winrt::Windows::Data::Json::JsonArray keysArray = keysValue.GetArray();
            JsonObject filtered;
            winrt::Windows::Data::Json::JsonArray missing;
            for (uint32_t i = 0; i < keysArray.Size(); ++i) {
                IJsonValue item = keysArray.GetAt(i);
                if (item.ValueType() != winrt::Windows::Data::Json::JsonValueType::String) {
                    return Error(400, "bad_request", "keys 必须是字符串数组");
                }
                std::string key = winrt::to_string(item.GetString());
                bool present = false;
                for (const std::string& existing : snapshotData.allKeys) {
                    if (existing == key) {
                        present = true;
                        break;
                    }
                }
                if (present && values.HasKey(winrt::to_hstring(key))) {
                    filtered.Insert(winrt::to_hstring(key),
                                    values.Lookup(winrt::to_hstring(key)));
                } else if (!present) {
                    missing.Append(JsonValue::CreateStringValue(winrt::to_hstring(key)));
                }
            }
            values = filtered;
            JsonObject body;
            body.Insert(L"values", values);
            body.Insert(L"missingKeys", missing);
            winrt::Windows::Data::Json::JsonArray redactedArray;
            for (const std::string& key : redacted) {
                redactedArray.Append(JsonValue::CreateStringValue(winrt::to_hstring(key)));
            }
            body.Insert(L"redactedKeys", redactedArray);
            body.Insert(L"revision",
                        JsonValue::CreateStringValue(winrt::to_hstring(snapshotData.revision)));
            HttpResponse response;
            response.body = winrt::to_string(body.Stringify());
            return response;
        }
        JsonObject body;
        body.Insert(L"values", values);
        winrt::Windows::Data::Json::JsonArray redactedArray;
        for (const std::string& key : redacted) {
            redactedArray.Append(JsonValue::CreateStringValue(winrt::to_hstring(key)));
        }
        body.Insert(L"redactedKeys", redactedArray);
        winrt::Windows::Data::Json::JsonArray writable;
        for (const auto& [key, spec] : WritableFields()) {
            writable.Append(JsonValue::CreateStringValue(winrt::to_hstring(key)));
        }
        body.Insert(L"writableKeys", writable);
        body.Insert(L"revision",
                    JsonValue::CreateStringValue(winrt::to_hstring(snapshotData.revision)));
        HttpResponse response;
        response.body = winrt::to_string(body.Stringify());
        return response;
    }
    if (path == "/config/write") {
        std::string key;
        if (!requireString(L"key", key)) {
            return Error(400, "bad_request", "key 必须是非空字符串");
        }
        if (!arguments.HasKey(L"value")) {
            return Error(400, "bad_request", "缺少 value");
        }
        std::string revision;
        if (!requireString(L"revision", revision)) {
            return Error(400, "revision_required",
                         "写入必须携带 config.read 返回的 revision，用于并发校验");
        }
        IJsonValue value = arguments.GetNamedValue(L"value");
        std::string code;
        std::string message;
        int verdict = CheckWritableField(key, value, code, message);
        if (verdict != 0) {
            return Error(verdict, code, message);
        }
        WriteResultData result;
        if (!ops_->WriteConfigField(key, value, revision, result, err)) {
            return Error(err.httpStatus, err.code, err.message);
        }
        JsonObject body;
        body.Insert(L"key", JsonValue::CreateStringValue(winrt::to_hstring(result.key)));
        body.Insert(L"value", result.value);
        body.Insert(L"revision", JsonValue::CreateStringValue(winrt::to_hstring(result.revision)));
        HttpResponse response;
        response.body = winrt::to_string(body.Stringify());
        return response;
    }
    if (path == "/task/start") {
        std::string taskId;
        if (!requireString(L"taskId", taskId)) {
            return Error(400, "bad_request", "taskId 必须是非空字符串");
        }
        if (AllowedTasks().find(taskId) == AllowedTasks().end()) {
            auto denied = DeniedTaskReasons().find(taskId);
            if (denied != DeniedTaskReasons().end()) {
                return Error(403, "task_denied", denied->second);
            }
            return Error(404, "task_unknown", "未知任务 " + taskId);
        }
        bool effective = false;
        for (const TaskEntry& task : snapshot_.tasks) {
            if (task.id == taskId) {
                effective = true;
                break;
            }
        }
        if (!effective) {
            return Error(404, "task_unknown", "宿主目录中没有内置任务 " + taskId);
        }
        StartTaskData data;
        if (!ops_->StartTask(taskId, data, err)) {
            return Error(err.httpStatus, err.code, err.message);
        }
        JsonObject body;
        body.Insert(L"status", JsonValue::CreateStringValue(winrt::to_hstring(data.status)));
        body.Insert(L"taskId", JsonValue::CreateStringValue(winrt::to_hstring(data.taskId)));
        if (data.status == "accepted") {
            JsonObject verification;
            verification.Insert(L"status", JsonValue::CreateStringValue(L"succeeded"));
            verification.Insert(L"scope", JsonValue::CreateStringValue(L"launch"));
            body.Insert(L"verification", verification);
            body.Insert(L"note", JsonValue::CreateStringValue(
                L"宿主已接纳且确认进入运行状态；业务结果仍以 status 与日志为准"));
        } else {
            body.Insert(L"note", JsonValue::CreateStringValue(
                L"已调用宿主 startTask，但未确认任务进入运行状态；不要当作已启动，用 status 核实"));
        }
        HttpResponse response;
        response.body = winrt::to_string(body.Stringify());
        return response;
    }
    if (path == "/task/stop") {
        int graceMs = int(kStopGraceMs);
        if (arguments.HasKey(L"graceMs")) {
            IJsonValue value = arguments.GetNamedValue(L"graceMs");
            if (value.ValueType() != winrt::Windows::Data::Json::JsonValueType::Number) {
                return Error(400, "bad_request", "graceMs 必须是整数");
            }
            double number = value.GetNumber();
            if (std::floor(number) != number) {
                return Error(400, "bad_request", "graceMs 必须是整数");
            }
            graceMs = int(number);
            if (graceMs < 0 || graceMs > 15000) {
                return Error(400, "bad_request", "graceMs 取值范围 0..15000");
            }
        }
        StopTaskData data;
        if (!ops_->StopTask(graceMs, data, err)) {
            return Error(err.httpStatus, err.code, err.message);
        }
        JsonObject body;
        body.Insert(L"status", JsonValue::CreateStringValue(winrt::to_hstring(data.status)));
        if (!data.taskId.empty()) {
            body.Insert(L"taskId", JsonValue::CreateStringValue(winrt::to_hstring(data.taskId)));
        }
        if (!data.note.empty()) {
            body.Insert(L"note", JsonValue::CreateStringValue(winrt::to_hstring(data.note)));
        } else if (data.status == "unknown") {
            body.Insert(L"note", JsonValue::CreateStringValue(
                L"停止请求已发出但观察窗内宿主仍报告运行；真实状态未知，请用 status 核实"));
        }
        HttpResponse response;
        response.body = winrt::to_string(body.Stringify());
        return response;
    }
    if (path == "/logs/read") {
        int tailLines = 200;
        if (arguments.HasKey(L"tailLines")) {
            IJsonValue value = arguments.GetNamedValue(L"tailLines");
            if (value.ValueType() != winrt::Windows::Data::Json::JsonValueType::Number ||
                std::floor(value.GetNumber()) != value.GetNumber()) {
                return Error(400, "bad_request", "tailLines 必须是整数");
            }
            tailLines = int(value.GetNumber());
            if (tailLines < 1 || tailLines > kMaxLogLines) {
                return Error(400, "bad_request", "tailLines 取值范围 1..2000");
            }
        }
        LogsData data;
        if (!ops_->ReadLogs(tailLines, data, err)) {
            return Error(err.httpStatus, err.code, err.message);
        }
        winrt::Windows::Data::Json::JsonArray lines;
        for (const std::string& line : data.lines) {
            lines.Append(JsonValue::CreateStringValue(winrt::to_hstring(line)));
        }
        JsonObject body;
        body.Insert(L"lines", lines);
        body.Insert(L"returnedLines",
                    JsonValue::CreateNumberValue(double(data.lines.size())));
        body.Insert(L"totalLines", JsonValue::CreateNumberValue(double(data.totalLines)));
        HttpResponse response;
        response.body = winrt::to_string(body.Stringify());
        return response;
    }
    if (path == "/shutdown") {
        bool confirm = false;
        if (arguments.HasKey(L"confirm")) {
            IJsonValue value = arguments.GetNamedValue(L"confirm");
            confirm = value.ValueType() == winrt::Windows::Data::Json::JsonValueType::Boolean &&
                      value.GetBoolean();
        }
        if (!confirm) {
            return Error(400, "bad_request", "关闭桥需要在请求体携带 \"confirm\": true");
        }
        shutdownRequested_.store(true);
        JsonObject body;
        body.Insert(L"status", JsonValue::CreateStringValue(L"stopping"));
        body.Insert(L"note", JsonValue::CreateStringValue(
            L"仅停止桥自建的 HTTP 服务；不影响宿主图形界面与正在运行的任务"));
        HttpResponse response;
        response.body = winrt::to_string(body.Stringify());
        return response;
    }
    return Error(404, "unknown_endpoint", "未知端点 " + path);
}

} // namespace sdm::march7th
