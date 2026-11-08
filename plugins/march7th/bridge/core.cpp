// 三月七原生桥：纯逻辑层实现（SHA-256、目录策略、脱敏、指纹、请求路由、
// 主线程异步调用内核）。本文件不包含 CPython / WinSock / 进程注入调用。
#include "core.hpp"

#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>
// windows.h 的 GDI 宏会把 WinRT 的 GetObject 方法改写为 GetObjectW。
#ifdef GetObject
#undef GetObject
#endif

#include <cctype>
#include <cmath>
#include <cstring>

namespace sdm::march7th {

TaskProcessState ClassifyTaskProcessState(int processState, bool externalActive) {
    if (externalActive) return TaskProcessState::Running;
    switch (processState) {
        case 0: return TaskProcessState::Stopped;
        case 1: return TaskProcessState::Starting;
        case 2: return TaskProcessState::Running;
        default: return TaskProcessState::Unknown;
    }
}

const char* TaskProcessStateName(TaskProcessState state) {
    switch (state) {
        case TaskProcessState::Stopped: return "stopped";
        case TaskProcessState::Starting: return "starting";
        case TaskProcessState::Running: return "running";
        default: return "unknown";
    }
}

// ---- SHA-256 ---------------------------------------------------------------

namespace {

struct Sha256Ctx {
    uint32_t state[8];
    uint64_t bitlen = 0;
    uint8_t buffer[64];
    size_t bufferLen = 0;
};

constexpr uint32_t kK[64] = {
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1,
    0x923f82a4, 0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3,
    0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786,
    0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147,
    0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
    0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
    0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a,
    0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208,
    0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
};

inline uint32_t Rotr(uint32_t value, unsigned bits) {
    return (value >> bits) | (value << (32 - bits));
}

void Sha256Transform(Sha256Ctx& ctx, const uint8_t block[64]) {
    uint32_t w[64];
    for (int i = 0; i < 16; ++i) {
        w[i] = (uint32_t(block[i * 4]) << 24) | (uint32_t(block[i * 4 + 1]) << 16) |
               (uint32_t(block[i * 4 + 2]) << 8) | uint32_t(block[i * 4 + 3]);
    }
    for (int i = 16; i < 64; ++i) {
        uint32_t s0 = Rotr(w[i - 15], 7) ^ Rotr(w[i - 15], 18) ^ (w[i - 15] >> 3);
        uint32_t s1 = Rotr(w[i - 2], 17) ^ Rotr(w[i - 2], 19) ^ (w[i - 2] >> 10);
        w[i] = w[i - 16] + s0 + w[i - 7] + s1;
    }
    uint32_t a = ctx.state[0], b = ctx.state[1], c = ctx.state[2], d = ctx.state[3];
    uint32_t e = ctx.state[4], f = ctx.state[5], g = ctx.state[6], h = ctx.state[7];
    for (int i = 0; i < 64; ++i) {
        uint32_t s1 = Rotr(e, 6) ^ Rotr(e, 11) ^ Rotr(e, 25);
        uint32_t ch = (e & f) ^ (~e & g);
        uint32_t temp1 = h + s1 + ch + kK[i] + w[i];
        uint32_t s0 = Rotr(a, 2) ^ Rotr(a, 13) ^ Rotr(a, 22);
        uint32_t maj = (a & b) ^ (a & c) ^ (b & c);
        uint32_t temp2 = s0 + maj;
        h = g;
        g = f;
        f = e;
        e = d + temp1;
        d = c;
        c = b;
        b = a;
        a = temp1 + temp2;
    }
    ctx.state[0] += a;
    ctx.state[1] += b;
    ctx.state[2] += c;
    ctx.state[3] += d;
    ctx.state[4] += e;
    ctx.state[5] += f;
    ctx.state[6] += g;
    ctx.state[7] += h;
}

void Sha256Update(Sha256Ctx& ctx, const uint8_t* data, size_t length) {
    ctx.bitlen += uint64_t(length) * 8;
    while (length > 0) {
        size_t take = std::min(length, size_t(64) - ctx.bufferLen);
        std::memcpy(ctx.buffer + ctx.bufferLen, data, take);
        ctx.bufferLen += take;
        data += take;
        length -= take;
        if (ctx.bufferLen == 64) {
            Sha256Transform(ctx, ctx.buffer);
            ctx.bufferLen = 0;
        }
    }
}

} // namespace

Sha256Digest Sha256(const void* data, size_t length) {
    Sha256Ctx ctx{};
    ctx.state[0] = 0x6a09e667;
    ctx.state[1] = 0xbb67ae85;
    ctx.state[2] = 0x3c6ef372;
    ctx.state[3] = 0xa54ff53a;
    ctx.state[4] = 0x510e527f;
    ctx.state[5] = 0x9b05688c;
    ctx.state[6] = 0x1f83d9ab;
    ctx.state[7] = 0x5be0cd19;
    Sha256Update(ctx, static_cast<const uint8_t*>(data), length);
    // 长度字段必须是原始报文位数：填充字节不能计入（保存后填充，再写 tail）。
    uint64_t bitlen = ctx.bitlen;
    uint8_t pad = 0x80;
    Sha256Update(ctx, &pad, 1);
    uint8_t zero = 0;
    while (ctx.bufferLen != 56) {
        Sha256Update(ctx, &zero, 1);
    }
    uint8_t tail[8];
    for (int i = 0; i < 8; ++i) {
        tail[i] = uint8_t(bitlen >> (56 - i * 8));
    }
    Sha256Update(ctx, tail, 8);
    Sha256Digest digest{};
    for (int i = 0; i < 8; ++i) {
        digest.bytes[i * 4] = uint8_t(ctx.state[i] >> 24);
        digest.bytes[i * 4 + 1] = uint8_t(ctx.state[i] >> 16);
        digest.bytes[i * 4 + 2] = uint8_t(ctx.state[i] >> 8);
        digest.bytes[i * 4 + 3] = uint8_t(ctx.state[i]);
    }
    return digest;
}

std::string Sha256Hex(std::string_view text) {
    static const char* hex = "0123456789abcdef";
    Sha256Digest digest = Sha256(text.data(), text.size());
    std::string out;
    out.reserve(64);
    for (uint8_t byte : digest.bytes) {
        out.push_back(hex[byte >> 4]);
        out.push_back(hex[byte & 0xf]);
    }
    return out;
}

std::string ShortDigest(const Sha256Digest& digest) {
    static const char* hex = "0123456789abcdef";
    std::string out;
    out.reserve(16);
    for (int i = 0; i < 8; ++i) {
        out.push_back(hex[digest.bytes[i] >> 4]);
        out.push_back(hex[digest.bytes[i] & 0xf]);
    }
    return out;
}

// ---- 目录与策略 --------------------------------------------------------------

const std::map<std::string, std::string>& AllowedTasks() {
    static const std::map<std::string, std::string> tasks = {
        {"main", "完整运行：按配置依次执行日常、体力、周本等全部启用项"},
        {"routine", "日常：每日实训、清体力、周常等日常组合"},
        {"daily", "每日实训：领取奖励与每日任务"},
        {"power", "清体力：按体力计划消耗开拓力"},
        {"currencywars", "货币战争：按宿主配置挑战该周常玩法并领取奖励"},
        {"divergent", "差分宇宙：一次性差分宇宙通关"},
        {"fight", "锄大地：大地图巡猎收集"},
        {"universe", "模拟宇宙：一次性模拟宇宙通关"},
        {"forgottenhall", "混沌回忆：深渊挑战"},
        {"purefiction", "虚构叙事：虚构叙事挑战"},
        {"apocalyptic", "末日幻影：末日幻影挑战"},
        {"game", "启动游戏：按宿主配置的游戏路径启动崩坏：星穹铁道"},
        {"character_export", "角色面板导出：导出角色养成数据"},
    };
    return tasks;
}

const std::map<std::string, std::string>& DeniedTaskReasons() {
    static const std::map<std::string, std::string> reasons = {
        {"app_update", "更新三月七小助手属于程序更新，不允许经桥触发"},
        {"game_update", "更新游戏属于程序更新，不允许经桥触发"},
        {"game_pre_download", "预下载游戏资源属于程序更新，不允许经桥触发"},
        {"universe_update", "模拟宇宙资源更新属于资源下载，不允许经桥触发"},
        {"fight_update", "锄大地资源更新属于资源下载，不允许经桥触发"},
        {"mobileui_update", "触屏模式资源更新属于资源下载，不允许经桥触发"},
        {"notify", "测试消息推送会携带通知凭据，不允许经桥触发"},
        {"redemption", "兑换码任务需要透传参数，为避免参数注入不接受启动"},
        {"screen_test", "界面可切换性测试是开发自检，不属于游戏任务"},
        {"currencywarsloop", "循环模式面向无人值守挂机，不允许经桥启动"},
        {"currencywarstemp", "中途接管面向本机交互使用，不允许经桥启动"},
        {"divergentloop", "循环模式面向无人值守挂机，不允许经桥启动"},
        {"divergenttemp", "中途接管面向本机交互使用，不允许经桥启动"},
        {"universe_gui", "原生界面模式面向本机交互使用，不允许经桥启动"},
        {"fight_gui", "原生界面模式面向本机交互使用，不允许经桥启动"},
    };
    return reasons;
}

const std::map<std::string, WritableFieldSpec>& WritableFields() {
    static const std::map<std::string, WritableFieldSpec> fields = {
        {"autoplot_skip_enable", {"boolean", {}, "对话出现跳过按钮时自动点击"}},
        {"autoplot_click_enable", {"boolean", {}, "自动选择对话选项"}},
        {"autoplot_battle_detect_enable", {"boolean", {}, "未自动战斗时按 V 开启自动战斗"}},
        {"autoplot_phone_detect_enable", {"boolean", {}, "自动处理手机短信页面"}},
        {"autoplot_hero_chronicle_enable", {"boolean", {}, "自动阅读翁法罗斯英雄纪"}},
        {"pause_after_success", {"boolean", {}, "任务成功后暂停程序等待确认"}},
        {"exit_after_failure", {"boolean", {}, "任务失败后直接退出而不是暂停"}},
        {"play_audio", {"boolean", {}, "任务过程中播放提示音"}},
        {"power_enable", {"boolean", {}, "清体力总开关（完整运行与日常中的清体力受它控制）"}},
        {"tp_before_instance", {"boolean", {}, "进入副本前先传送到任意锚点"}},
        {"instance_team_enable", {"boolean", {}, "打副本时自动切换队伍"}},
        {"break_down_level_four_relicset", {"boolean", {}, "每周所选日期清理四星及以下遗器"}},
        {"weekly_relic_smart_discard_enable", {"boolean", {}, "每周清理前先智能弃置五星遗器"}},
        {"instance_type",
         {"string",
          {"拟造花萼（金）", "拟造花萼（赤）", "凝滞虚影", "侵蚀隧洞", "饰品提取", "历战余响"},
          "清体力使用的副本类型"}},
        {"calyx_golden_preference",
         {"string", {"Jarilo-VI", "XianzhouLuofu", "Penacony"}, "拟造花萼（金）偏好地区"}},
    };
    return fields;
}

namespace {

bool ContainsLower(std::string_view key, std::string_view pattern) {
    if (key.size() < pattern.size()) {
        return false;
    }
    for (size_t i = 0; i + pattern.size() <= key.size(); ++i) {
        size_t j = 0;
        while (j < pattern.size() &&
               char(std::tolower(unsigned char(key[i + j]))) == pattern[j]) {
            ++j;
        }
        if (j == pattern.size()) {
            return true;
        }
    }
    return false;
}

} // namespace

bool IsSensitiveKey(std::string_view key) {
    static const std::vector<std::string_view> patterns = {
        "token",   "password", "passwd",   "secret",  "cookie", "webhook",
        "api_key", "apikey",   "access_key", "private_key", "credential",
        "userid",  "user_id",  "account",
        "key", "url", "authorization",
    };
    for (std::string_view pattern : patterns) {
        if (ContainsLower(key, pattern)) {
            return true;
        }
    }
    return false;
}

std::optional<std::string> DenyWriteReason(std::string_view key) {
    static const std::vector<std::pair<std::string_view, std::string>> patterns = {
        {"path", "路径字段"},     {"exe", "可执行文件字段"}, {"program", "外部程序字段"},
        {"script", "脚本字段"},   {"command", "命令字段"},   {"cmd", "命令字段"},
        {"shell", "Shell 字段"},  {"python", "解释器字段"},  {"url", "网络端点字段"},
        {"mirror", "下载镜像字段"}, {"autostart", "自动启动字段"}, {"auto_start", "自动启动字段"},
        {"schedule", "计划任务字段"}, {"plan", "计划字段"},   {"workflow", "自定义流程字段"},
        {"after_finish", "完成后系统操作字段"}, {"hotkey", "全局快捷键字段"},
        {"timeout", "超时字段"},
    };
    for (const auto& [pattern, label] : patterns) {
        if (ContainsLower(key, pattern)) {
            return label;
        }
    }
    return std::nullopt;
}

namespace {

bool JsonIsIntegralNumber(const IJsonValue& value) {
    using winrt::Windows::Data::Json::JsonValueType;
    if (value.ValueType() != JsonValueType::Number) {
        return false;
    }
    double number = value.GetNumber();
    return std::floor(number) == number && std::abs(number) < 9.0e15;
}

} // namespace

int CheckWritableField(std::string_view key, const IJsonValue& value, std::string& code,
                       std::string& message) {
    using winrt::Windows::Data::Json::JsonValueType;
    const auto& fields = WritableFields();
    auto it = fields.find(std::string(key));
    if (it == fields.end()) {
        auto reason = DenyWriteReason(key);
        if (reason.has_value()) {
            code = "field_dangerous";
            message = "拒绝写入 " + std::string(key) + "：" + *reason + "不在可写白名单内";
        } else {
            code = "field_not_writable";
            message = "字段 " + std::string(key) +
                      " 不在可写白名单内；可用字段见 config.describe";
        }
        return 403;
    }
    const WritableFieldSpec& spec = it->second;
    JsonValueType type = value.ValueType();
    if (spec.type == "boolean" && type != JsonValueType::Boolean) {
        code = "value_type_mismatch";
        message = "字段 " + std::string(key) + " 需要布尔值 true/false";
        return 422;
    }
    if (spec.type == "integer" && !JsonIsIntegralNumber(value)) {
        code = "value_type_mismatch";
        message = "字段 " + std::string(key) + " 需要整数";
        return 422;
    }
    if (spec.type == "string" && type != JsonValueType::String) {
        code = "value_type_mismatch";
        message = "字段 " + std::string(key) + " 需要字符串";
        return 422;
    }
    if (!spec.choices.empty()) {
        std::string text = winrt::to_string(value.GetString());
        bool allowed = false;
        for (const std::string& choice : spec.choices) {
            if (choice == text) {
                allowed = true;
                break;
            }
        }
        if (!allowed) {
            std::string joined;
            for (size_t i = 0; i < spec.choices.size(); ++i) {
                if (i > 0) {
                    joined += "、";
                }
                joined += spec.choices[i];
            }
            code = "value_not_allowed";
            message = "字段 " + std::string(key) + " 的取值必须是：" + joined;
            return 422;
        }
    }
    return 0;
}

// 数组内元素逐个脱敏：对象递归、数组再下钻；替换值经 Stringify→Parse 构造
// （WinRT 集合元素是 IJsonValue 接口，不能直接塞 JsonObject/JsonArray）。
std::vector<std::string> MaskArrayElements(winrt::Windows::Data::Json::JsonArray& array) {
    using winrt::Windows::Data::Json::JsonValueType;
    std::vector<std::string> redacted;
    for (uint32_t i = 0; i < array.Size(); ++i) {
        IJsonValue element = array.GetAt(i);
        if (element.ValueType() == JsonValueType::Object) {
            JsonObject nested = element.GetObject();
            std::vector<std::string> nestedRedacted = ApplySensitiveMasking(nested, true);
            if (!nestedRedacted.empty()) {
                array.SetAt(i, JsonValue::Parse(nested.Stringify()));
                for (const std::string& child : nestedRedacted) {
                    redacted.push_back("[" + std::to_string(i) + "]." + child);
                }
            }
        } else if (element.ValueType() == JsonValueType::Array) {
            winrt::Windows::Data::Json::JsonArray nested = element.GetArray();
            std::vector<std::string> nestedRedacted = MaskArrayElements(nested);
            if (!nestedRedacted.empty()) {
                array.SetAt(i, JsonValue::Parse(nested.Stringify()));
                for (const std::string& child : nestedRedacted) {
                    redacted.push_back("[" + std::to_string(i) + "]" + child);
                }
            }
        }
    }
    return redacted;
}

std::vector<std::string> ApplySensitiveMasking(JsonObject& values, bool recursive) {
    using winrt::Windows::Data::Json::JsonValueType;
    std::vector<std::string> redacted;
    // 先收集键再改写，避免迭代中修改集合。
    std::vector<std::string> keys;
    for (const auto& pair : values) {
        keys.push_back(winrt::to_string(pair.Key()));
    }
    for (const std::string& key : keys) {
        IJsonValue value = values.Lookup(winrt::to_hstring(key));
        if (IsSensitiveKey(key)) {
            values.Insert(winrt::to_hstring(key), JsonValue::Parse(L"{\"redacted\":true}"));
            redacted.push_back(key);
        } else if (recursive && value.ValueType() == JsonValueType::Object) {
            JsonObject nested = value.GetObject();
            std::vector<std::string> nestedRedacted = ApplySensitiveMasking(nested, true);
            if (!nestedRedacted.empty()) {
                values.Insert(winrt::to_hstring(key), JsonValue::Parse(nested.Stringify()));
                for (const std::string& child : nestedRedacted) {
                    redacted.push_back(key + "." + child);
                }
            }
        } else if (recursive && value.ValueType() == JsonValueType::Array) {
            // 数组里的对象同样要脱敏，不能只处理字典。
            winrt::Windows::Data::Json::JsonArray array = value.GetArray();
            std::vector<std::string> nestedRedacted = MaskArrayElements(array);
            if (!nestedRedacted.empty()) {
                values.Insert(winrt::to_hstring(key), JsonValue::Parse(array.Stringify()));
                for (const std::string& child : nestedRedacted) {
                    redacted.push_back(key + child);
                }
            }
        }
    }
    return redacted;
}

// ---- 指纹 --------------------------------------------------------------------

std::string ComputeRevision(const std::vector<std::pair<std::string, std::string>>& pairs) {
    std::vector<std::pair<std::string, std::string>> sorted = pairs;
    std::sort(sorted.begin(), sorted.end(),
              [](const auto& a, const auto& b) { return a.first < b.first; });
    std::string canonical;
    for (const auto& [key, scalar] : sorted) {
        canonical += key;
        canonical += '=';
        canonical += scalar;
        canonical += '\n';
    }
    return "r" + ShortDigest(Sha256(canonical.data(), canonical.size()));
}

std::string ComputeCatalogVersion(const std::vector<TaskEntry>& tasks) {
    std::vector<std::string> entries;
    entries.reserve(tasks.size());
    for (const TaskEntry& task : tasks) {
        entries.push_back(task.id + ":" + task.label);
    }
    std::sort(entries.begin(), entries.end());
    std::string canonical = std::string("policy=") + kPolicyRevision + ";protocol=" +
                            kProtocolVersion + ";tasks=";
    for (const std::string& entry : entries) {
        canonical += entry;
        canonical += ',';
    }
    canonical += ";writable=";
    for (const auto& [key, spec] : WritableFields()) {
        canonical += key;
        canonical += ':' + spec.type;
        canonical += ',';
    }
    return "c" + ShortDigest(Sha256(canonical.data(), canonical.size()));
}

// ---- 主线程任务状态机 ----------------------------------------------------------

bool JobTryBeginRun(std::atomic<int>& phase) {
    int expected = 0;
    return phase.compare_exchange_strong(expected, 1);
}

void JobMarkFinished(std::atomic<int>& phase) {
    phase.store(2);
}

DispatchStatus WaitDispatch(const std::function<bool(uint32_t)>& waitFn,
                            std::atomic<int>& phase, uint32_t timeoutMs) {
    if (waitFn(timeoutMs)) {
        return phase.load() == 2 ? DispatchStatus::Completed
                                 : DispatchStatus::TimeoutStillRunning;
    }
    for (int attempt = 0; attempt < 2; ++attempt) {
        int current = phase.load();
        if (current == 2) {
            return DispatchStatus::Completed;
        }
        if (current == 1) {
            return DispatchStatus::TimeoutStillRunning;
        }
        // current == 0：尚未开始。尝试撤销（0 -> 3）。
        int expected = 0;
        if (phase.compare_exchange_strong(expected, 3)) {
            return DispatchStatus::TimeoutNotStarted;
        }
    }
    return DispatchStatus::TimeoutStillRunning;
}

// ---- 主线程异步调用内核 ---------------------------------------------------------

struct AsyncCall::State {
    ~State() {
        // 事件句柄只经本析构关闭：蹦床与调用方共同持 shared_ptr 保活，
        // 最后一个释放者负责关闭——不存在"SetEvent 后等待方尚未返回句柄即失效"。
        if (done != nullptr) {
            ::CloseHandle(done);
        }
    }
    std::atomic<int> phase{0};
    HANDLE done = nullptr; // 手动复位事件
    std::function<void()> body;
    std::string failure; // body 抛出的异常文本；仅在 phase==2 后读
};

AsyncCall::AsyncCall(std::function<void()> body) : state_(std::make_shared<State>()) {
    state_->body = std::move(body);
    state_->done = ::CreateEventW(nullptr, TRUE, FALSE, nullptr);
}

AsyncCall::~AsyncCall() = default;

bool AsyncCall::Enqueue(const Poster& poster) {
    if (state_->done == nullptr || !poster) {
        return false;
    }
    // 蹦床闭包持 State 引用：调用方超时返回后闭包仍可能迟到执行，
    // State 必须独立于 AsyncCall 对象存活。
    std::shared_ptr<State> state = state_;
    std::function<void()> run = [state] { AsyncCall::RunCaptured(state.get()); };
    return poster(std::move(run));
}

DispatchStatus AsyncCall::Wait(uint32_t timeoutMs) {
    std::shared_ptr<State> state = state_;
    auto wait = [state](uint32_t ms) -> bool {
        return ::WaitForSingleObject(state->done, ms) == WAIT_OBJECT_0;
    };
    return WaitDispatch(wait, state_->phase, timeoutMs);
}

void AsyncCall::RunOnTargetThread() noexcept {
    RunCaptured(state_.get());
}

void AsyncCall::RunCaptured(State* state) noexcept {
    if (!JobTryBeginRun(state->phase)) {
        return; // 已撤销（超时未执行）或重复执行：body 绝不运行
    }
    try {
        state->body();
    } catch (const std::exception& error) {
        try {
            state->failure = error.what();
        } catch (...) {
            state->failure = "派发任务内部异常";
        }
    } catch (...) {
        state->failure = "派发任务内部异常";
    }
    JobMarkFinished(state->phase);
    ::SetEvent(state->done);
}

const std::string& AsyncCall::failure() const {
    return state_->failure;
}

} // namespace sdm::march7th
