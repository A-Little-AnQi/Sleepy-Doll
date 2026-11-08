// tests.cpp - zzz 原生桥纯逻辑测试。
//
// 只链接 zzz_pure（纯逻辑单元 + 新 JobHub 状态机）；不包含生产 zzz_core.cpp
// 与 injector.cpp，不出现任何真实宿主接口（无 OpenProcess /
// CreateRemoteThread / WriteProcessMemory / WinHttpConnect / connect /
// SendInput / LoadLibrary 目标路径），不使用真实 CPython。
//
// job 状态机用伪 pending 回调队列驱动：覆盖同步执行、排队超时取消、
// 迟到回调不执行且持有对象直到回调发生、执行中超时（unknown），
// 全部以状态与生命周期计数断言，不靠响应字符串。

#include <winrt/Windows.Data.Json.h>
#include <winrt/Windows.Foundation.Collections.h>

#include "zzz_pure.hpp"

#include <chrono>
#include <cstdio>
#include <thread>

namespace {

int g_pass = 0;
int g_fail = 0;

void Check(bool condition, const char* what) {
    if (condition) {
        ++g_pass;
    } else {
        ++g_fail;
        std::printf("FAIL: %s\n", what);
    }
}

JVar MakeStr(const std::string& text) { return JVar{JVar::T::Str, false, 0, 0, text}; }
JVar MakeInt(long long value) { return JVar{JVar::T::Int, false, value}; }
JVar MakeBool(bool value) { return JVar{JVar::T::Bool, value}; }

// 伪 pending 回调队列：把 CPython 的排队语义做成测试可控的形式。
struct PendingQueue {
    struct Item {
        int (*func)(void*);
        void* arg;
    };
    std::mutex lock;
    std::vector<Item> items;

    static int Enqueue(int (*func)(void*), void* arg) {
        auto* queue = Instance();
        std::lock_guard<std::mutex> guard(queue->lock);
        queue->items.push_back({func, arg});
        return 0;  // 排队成功
    }
    static int Reject(int (*)(void*), void*) { return -1; }  // 队列满
    static PendingQueue* Instance() {
        static PendingQueue queue;
        return &queue;
    }
    size_t Count() {
        std::lock_guard<std::mutex> guard(lock);
        return items.size();
    }
    // 逐条触发已排队回调，模拟主线程处理 pending calls。
    void Drain() {
        std::vector<Item> pending;
        {
            std::lock_guard<std::mutex> guard(lock);
            pending.swap(items);
        }
        for (const auto& item : pending) item.func(item.arg);
    }
};

void TestKeyRules() {
    Check(IsSensitiveKey("FEISHU_WEBHOOK"), "sensitive webhook");
    Check(IsSensitiveKey("bark_DEVICE_KEY"), "sensitive key");
    Check(IsSensitiveKey("dingding_TOKEN"), "sensitive token");
    Check(IsSensitiveKey("USER_ID"), "sensitive user id");
    Check(!IsSensitiveKey("transport_point"), "point not sensitive");
    Check(!IsSensitiveKey("run_charge_plan_afterwards"), "plan flag not sensitive");
    Check(IsForbiddenWriteKey("script_path"), "path forbidden");
    Check(IsForbiddenWriteKey("pipeline"), "pipeline forbidden");
    Check(IsForbiddenWriteKey("startup_command"), "command forbidden");
    Check(IsForbiddenWriteKey("a/b"), "slash forbidden");
    Check(IsForbiddenWriteKey(".."), "dotdot forbidden");
    Check(!IsForbiddenWriteKey("transport_point"), "point writable");
    Check(!IsForbiddenWriteKey("hour_of_run"), "plain key writable");
    Check(IsValidGroupId("one_dragon"), "group ok");
    Check(IsValidGroupId("group-2_x"), "group ok 2");
    Check(!IsValidGroupId(""), "empty group");
    Check(!IsValidGroupId("../etc"), "traversal group");
    Check(!IsValidGroupId("a/b"), "slash group");
    Check(!IsValidGroupId("C:"), "drive group");
    Check(!IsValidGroupId("a\\b"), "backslash group");
    Check(IsValidAppId("coffee"), "appid ok");
    Check(!IsValidAppId("not allowed!"), "appid bad");
    Check(!IsValidAppId(""), "appid empty");
}

void TestTokenCompare() {
    Check(SecureTokenEqual("abcdef0123456789", "abcdef0123456789"), "token equal");
    Check(!SecureTokenEqual("abcdef0123456789", "abcdef0123456780"), "token differ");
    Check(!SecureTokenEqual("short", "shorter-token-value"), "token length");
}

void TestCanonicalHash() {
    JVar a{JVar::T::Obj};
    a.obj.emplace_back("x", MakeInt(1));
    a.obj.emplace_back("y", MakeBool(true));
    JVar b{JVar::T::Obj};
    b.obj.emplace_back("y", MakeBool(true));
    b.obj.emplace_back("x", MakeInt(1));
    Check(CanonicalHash(a) == CanonicalHash(b), "hash order independent");
    Check(CanonicalHash(a).size() == 64, "hash is sha256 hex");
    JVar c{JVar::T::Obj};
    c.obj.emplace_back("x", MakeInt(2));
    Check(CanonicalHash(a) != CanonicalHash(c), "hash differs on value");
    JVar nested{JVar::T::Obj};
    JVar inner{JVar::T::Obj};
    inner.obj.emplace_back("k", MakeStr("v"));
    nested.obj.emplace_back("in", std::move(inner));
    std::string feed;
    CanonicalFeed(nested, feed);
    Check(!feed.empty(), "canonical feed non-empty");
}

void TestSafeWriteValue() {
    std::string why;
    JVar nestedCommand{JVar::T::Obj};
    nestedCommand.obj.emplace_back("startup_command", MakeStr("do-not-run"));
    Check(!IsSafeWriteValue(nestedCommand, 0, why), "nested command cannot bypass key guards");
    Check(IsSensitiveKey("notify_callback_url"), "credential endpoints masked");
    Check(IsSafeWriteValue(MakeInt(42), 0, why), "int ok");
    Check(IsSafeWriteValue(MakeBool(false), 0, why), "bool ok");
    Check(!IsSafeWriteValue(MakeInt(9007199254740993LL), 0, why), "big int rejected");
    JVar real{JVar::T::Real};
    real.d = 1.5;
    Check(IsSafeWriteValue(real, 0, why), "real ok");
    JVar nan{JVar::T::Real};
    nan.d = std::nan("1");
    Check(!IsSafeWriteValue(nan, 0, why), "nan rejected");
    Check(IsSafeWriteValue(MakeStr(std::string(512, 'a')), 0, why), "512 str ok");
    Check(!IsSafeWriteValue(MakeStr(std::string(513, 'a')), 0, why), "513 str rejected");
    JVar list{JVar::T::Arr};
    list.arr.push_back(MakeInt(1));
    list.arr.push_back(MakeStr("x"));
    Check(IsSafeWriteValue(list, 0, why), "shallow list ok");
    JVar outer{JVar::T::Arr};
    JVar inner{JVar::T::Arr};
    inner.arr.push_back(MakeInt(1));
    outer.arr.push_back(std::move(inner));
    Check(IsSafeWriteValue(outer, 0, why), "depth-2 list ok");
    JVar deep{JVar::T::Arr};
    JVar mid{JVar::T::Arr};
    JVar innermost{JVar::T::Arr};
    mid.arr.push_back(std::move(innermost));
    deep.arr.push_back(std::move(mid));
    Check(!IsSafeWriteValue(deep, 0, why), "depth-3 rejected");
    JVar big{JVar::T::Arr};
    for (int i = 0; i < 33; ++i) big.arr.push_back(MakeInt(i));
    Check(!IsSafeWriteValue(big, 0, why), "33 items rejected");
}

void TestMaskSensitive() {
    JVar value{JVar::T::Obj};
    value.obj.emplace_back("FEISHU_WEBHOOK", MakeStr("https://secret"));
    value.obj.emplace_back("plain", MakeStr("ok"));
    JVar nested{JVar::T::Obj};
    nested.obj.emplace_back("bark_TOKEN", MakeStr("abc"));
    value.obj.emplace_back("inner", std::move(nested));
    MaskSensitive(value);
    const JVar* masked = nullptr;
    const JVar* plain = nullptr;
    const JVar* inner = nullptr;
    for (const auto& kv : value.obj) {
        if (kv.first == "FEISHU_WEBHOOK") masked = &kv.second;
        if (kv.first == "plain") plain = &kv.second;
        if (kv.first == "inner") inner = &kv.second;
    }
    Check(masked && masked->s == SensitiveMask(), "webhook masked");
    Check(plain && plain->s == "ok", "plain kept");
    Check(inner && inner->t == JVar::T::Obj && !inner->obj.empty() &&
              inner->obj[0].second.s == SensitiveMask(),
          "nested token masked");
}

void TestHttpRequestParsing() {
    HttpRequest request;
    std::string raw = "POST /status HTTP/1.1\r\n"
                      "Host: 127.0.0.1:23101\r\n"
                      "X-Sleepy-Token: abc123\r\n"
                      "Content-Length: 7\r\n"
                      "\r\n"
                      "{\"a\":1}";
    Check(ParseHttpRequest(raw, request), "request parsed");
    Check(request.method == "POST", "method");
    Check(request.path == "/status", "path");
    Check(request.host == "127.0.0.1:23101", "host");
    Check(request.token == "abc123", "token");
    Check(request.body == "{\"a\":1}", "body");
    Check(!request.tooLarge && !request.malformed, "clean flags");

    HttpRequest partial;
    Check(!ParseHttpRequest("POST /status HTTP/1.1\r\nHost: 127.0.0.1", partial) &&
              !partial.headerComplete,
          "incomplete header");

    HttpRequest origin;
    std::string withOrigin = "POST /status HTTP/1.1\r\nHost: 127.0.0.1:23101\r\n"
                             "Origin: http://evil.example\r\nContent-Length: 0\r\n\r\n";
    Check(ParseHttpRequest(withOrigin, origin), "origin request parsed");
    Check(origin.origin == "http://evil.example", "origin captured");

    HttpRequest tooLarge;
    std::string big = "POST /status HTTP/1.1\r\nHost: 127.0.0.1:23101\r\nContent-Length: 99999999\r\n\r\n";
    Check(!ParseHttpRequest(big, tooLarge) && tooLarge.tooLarge, "oversized body rejected");

    HttpRequest malformed;
    Check(!ParseHttpRequest("garbage", malformed), "garbage rejected");
}

void TestResponses() {
    std::string response = BuildHttpResponse(401, "{\"error\":{}}");
    Check(response.find("HTTP/1.1 401 Unauthorized") == 0, "status line");
    Check(response.find("Content-Length: 12\r\n") != std::string::npos, "content length");
    std::string unicodeResponse = BuildHttpResponse(200, "{\"错误\":{}}");
    Check(unicodeResponse.find("Content-Length: 13\r\n") != std::string::npos,
          "UTF-8 content length uses bytes");
    Check(response.find("Connection: close") != std::string::npos, "connection close");
    std::string error = ErrorJson("bad_host", "Host \u4e0d\u7b26 \"quoted\"");
    Check(error.find("\"code\":\"bad_host\"") != std::string::npos, "error code json");
    Check(error.find("\\\"quoted\\\"") != std::string::npos, "quote escaped");
    JVar root{JVar::T::Obj};
    root.obj.emplace_back("name", MakeStr("\u4e00\u9f99"));
    std::string serialized = SerializeJVar(root);
    Check(serialized.find("name") != std::string::npos, "serialized key");
    Check(std::strcmp(TypeNameOf(MakeInt(1)), "int") == 0, "type int");
    Check(std::strcmp(TypeNameOf(MakeBool(true)), "bool") == 0, "type bool");
    Check(std::strcmp(TypeNameOf(MakeStr("x")), "str") == 0, "type str");
}

void TestVerifyRequest() {
    HttpRequest ok;
    ok.host = "127.0.0.1:23101";
    ok.token = "token-value-0123456789";
    Check(VerifyHttpRequest(ok, 23101, "token-value-0123456789") == nullptr, "verify ok");
    HttpRequest badHost = ok;
    badHost.host = "evil.example:23101";
    Check(std::strcmp(VerifyHttpRequest(badHost, 23101, "token-value-0123456789"), "bad_host") == 0,
          "bad host");
    HttpRequest spoofed = ok;
    spoofed.origin = "http://127.0.0.1:23101";
    Check(std::strcmp(VerifyHttpRequest(spoofed, 23101, "token-value-0123456789"),
                      "browser_forbidden") == 0,
          "origin rejected");
    HttpRequest badToken = ok;
    badToken.token = "token-value-0123456788";
    Check(std::strcmp(VerifyHttpRequest(badToken, 23101, "token-value-0123456789"), "unauthorized") == 0,
          "bad token");
    HttpRequest otherPort = ok;
    otherPort.host = "127.0.0.1:23102";
    Check(std::strcmp(VerifyHttpRequest(otherPort, 23101, "token-value-0123456789"), "bad_host") == 0,
          "other port");
}

void TestJobDispatchStateMachine() {
    JobHub hub;

    // 1) 同步执行：排队即回调，结果完整带回。
    g_py.Py_AddPendingCall = [](int (*func)(void*), void* arg) { return func(arg); };
    bool executed = false;
    auto synced = hub.Dispatch([&executed](HostJob&) { executed = true; return true; }, 2000);
    Check(synced.outcome == DispatchOutcome::Executed && synced.ok && executed, "sync executed");

    // 2) 闭包失败：错误字段带出。
    auto failure = hub.Dispatch(
        [](HostJob& job) { return job.Fail(409, "config_hash_mismatch", "changed"); }, 2000);
    Check(failure.outcome == DispatchOutcome::Executed && !failure.ok && failure.failStatus == 409 &&
              failure.failCode == "config_hash_mismatch",
          "failure carried out");

    // 3) 闭包抛异常：回调内吞掉并转为失败，不跨 C 边界。
    auto thrown = hub.Dispatch([](HostJob&) -> bool { throw std::runtime_error("boom"); }, 2000);
    Check(thrown.outcome == DispatchOutcome::Executed && !thrown.ok &&
              thrown.failCode == "host_exception",
          "exception contained");

    // 4) 队列满：立即 QueueFull，不进入等待。
    g_py.Py_AddPendingCall = &PendingQueue::Reject;
    auto full = hub.Dispatch([](HostJob&) { return true; }, 2000);
    Check(full.outcome == DispatchOutcome::QueueFull, "queue full");

    // 5) 排队超时：CAS 取消，晚到回调不执行，且 ticket 持有的 job
    //    在回调真正发生前始终存活（weak 观测不提前释放）。
    g_py.Py_AddPendingCall = &PendingQueue::Enqueue;
    std::atomic<bool> lateWorkRan = false;
    auto* queue = PendingQueue::Instance();
    size_t queuedBefore = queue->Count();
    auto cancelled = hub.Dispatch([&lateWorkRan](HostJob&) { lateWorkRan = true; return true; }, 30);
    Check(cancelled.outcome == DispatchOutcome::QueueTimeout, "queue timeout cancelled");
    Check(queue->Count() == queuedBefore + 1, "ticket still queued");
    Check(hub.TrackedAlive() >= 1, "cancelled job still held by ticket");
    queue->Drain();  // 模拟迟到回调
    Check(!lateWorkRan, "late callback skipped cancelled work");
    hub.ForgetExpired();
    Check(hub.TrackedAlive() == 0, "late callback released job");

    // 6) 执行中超时：如 unknown，不重试；迟到完成后对象自然回收。
    g_py.Py_AddPendingCall = [](int (*func)(void*), void* arg) {
        std::thread([func, arg]() { func(arg); }).detach();  // 立即开始执行
        return 0;
    };
    std::atomic<bool> slowWorkFinished = false;
    auto running = hub.Dispatch(
        [&slowWorkFinished](HostJob&) {
            std::this_thread::sleep_for(std::chrono::milliseconds(250));
            slowWorkFinished = true;
            return true;
        },
        40);
    Check(running.outcome == DispatchOutcome::RunTimeout, "run timeout unknown");
    Check(!running.ok, "run timeout no fake success");
    // 等待迟到完成并确认对象回收（回调持有直至执行结束）。
    for (int i = 0; i < 60 && !slowWorkFinished; ++i) {
        std::this_thread::sleep_for(std::chrono::milliseconds(10));
    }
    Check(slowWorkFinished, "running job still completes after timeout");
    hub.ForgetExpired();
    Check(hub.TrackedAlive() == 0, "completed job released");
    g_py.Py_AddPendingCall = nullptr;
}

// 伪 Qt 队列 poster：模拟 QTimer.singleShot(0, context, callable) 的
// "稍后在 GUI 线程执行"、"context 销毁丢弃回调"与"投递失败"三类行为，
// 全部用工作执行标志与生命周期计数断言，不依赖响应字符串。
struct FakeQtQueue {
    static inline JobHub* cancelDuringPost = nullptr;
    struct Item {
        int (*func)(void*);
        void (*discard)(void*);
        void* arg;
    };
    std::mutex lock;
    std::vector<Item> items;

    static FakeQtQueue* Instance() {
        static FakeQtQueue queue;
        return &queue;
    }
    // 模拟带 context 的 singleShot：入队成功（回调稍后在"GUI 线程"执行）。
    static int Post(int (*func)(void*), void (*discard)(void*), void* arg, DWORD) {
        auto* queue = Instance();
        std::lock_guard<std::mutex> guard(queue->lock);
        queue->items.push_back({func, discard, arg});
        return 0;
    }
    // 模拟投递失败（无 context / 已超请求期限）：post 内部经 discard 释放。
    static int PostFail(int (*)(void*), void (*discard)(void*), void* arg, DWORD) {
        discard(arg);
        return -1;
    }
    static int PostThenStop(int (*func)(void*), void (*discard)(void*), void* arg, DWORD budget) {
        Post(func, discard, arg, budget);
        cancelDuringPost->CancelQueued();
        return 0;
    }
    // GUI 线程取空队列逐条执行。
    void RunAll() {
        std::vector<Item> pending;
        {
            std::lock_guard<std::mutex> guard(lock);
            pending.swap(items);
        }
        for (const auto& item : pending) item.func(item.arg);
    }
    // 模拟 Qt context 销毁：未执行回调全部经 discard 丢弃。
    void DropAll() {
        std::vector<Item> pending;
        {
            std::lock_guard<std::mutex> guard(lock);
            pending.swap(items);
        }
        for (const auto& item : pending) item.discard(item.arg);
    }
    size_t Count() {
        std::lock_guard<std::mutex> guard(lock);
        return items.size();
    }
};

void TestQtPosterDispatch() {
    JobHub hub;
    auto* qt = FakeQtQueue::Instance();

    // 1) 队列稍后执行：投递入队，另一"GUI 线程"RunAll，等待方拿到 Executed。
    g_postCall = &FakeQtQueue::Post;
    std::atomic<bool> ran = false;
    std::thread guiThread([&hub, &ran]() {
        std::this_thread::sleep_for(std::chrono::milliseconds(80));
        FakeQtQueue::Instance()->RunAll();
    });
    auto executed = hub.Dispatch([&ran](HostJob&) { ran = true; return true; }, 2000);
    guiThread.join();
    Check(executed.outcome == DispatchOutcome::Executed && executed.ok && ran,
          "qt poster delivers and executes");

    // 2) 等待超时后队列才执行（迟到回调）：取消生效，工作不执行，
    //    且回调发生前 ticket 一直持有对象（weak 观测）。
    std::atomic<bool> lateRan = false;
    auto late = hub.Dispatch([&lateRan](HostJob&) { lateRan = true; return true; }, 40);
    Check(late.outcome == DispatchOutcome::QueueTimeout, "qt queue timeout cancels");
    Check(qt->Count() == 1, "qt late callback still queued");
    Check(hub.TrackedAlive() >= 1, "qt cancelled job held until callback");
    qt->RunAll();  // 迟到回调到达
    Check(!lateRan, "qt late callback skips cancelled work");
    hub.ForgetExpired();
    Check(hub.TrackedAlive() == 0, "qt late callback releases job");

    // 3) Qt context 销毁丢弃回调：未执行的工作被取消释放，等待方如实收到
    //    超时（非伪造成功），对象生命周期归零。
    std::atomic<bool> droppedRan = false;
    auto dropped = hub.Dispatch([&droppedRan](HostJob&) { droppedRan = true; return true; }, 40);
    qt->DropAll();  // context 销毁：丢弃发生在等待窗口内
    Check(dropped.outcome == DispatchOutcome::QueueTimeout, "qt context drop reports timeout");
    Check(!droppedRan, "qt context drop never runs work");
    hub.ForgetExpired();
    Check(hub.TrackedAlive() == 0, "qt context drop releases job");
    Check(qt->Count() == 0, "qt queue empty after drop");

    // 4) 投递失败（无 context / 超期限）：立即失败且对象被 discard 释放，
    //    不静默降级、不悬挂。
    g_postCall = &FakeQtQueue::PostFail;
    std::atomic<bool> failRan = false;
    auto failed = hub.Dispatch([&failRan](HostJob&) { failRan = true; return true; }, 2000);
    Check(failed.outcome == DispatchOutcome::QueueFull, "qt post failure is explicit");
    Check(!failRan, "qt post failure never runs work");
    hub.ForgetExpired();
    Check(hub.TrackedAlive() == 0, "qt post failure releases job");

    g_postCall = &FakeQtQueue::PostThenStop;
    FakeQtQueue::cancelDuringPost = &hub;
    std::atomic<bool> stoppedWorkRan{false};
    auto stopped = hub.Dispatch([&stoppedWorkRan](HostJob&) {
        stoppedWorkRan = true;
        return true;
    }, 2000);
    Check(stopped.outcome == DispatchOutcome::QueueTimeout, "bridge stop cancels queued work immediately");
    qt->RunAll();
    Check(!stoppedWorkRan, "late callback after stop never executes work");
    hub.ForgetExpired();
    Check(hub.TrackedAlive() == 0, "stopped queued callback releases its job");
    FakeQtQueue::cancelDuringPost = nullptr;
    g_postCall = nullptr;
}

void TestJsonParsing() {
    // Windows.Data.Json 请求解析路径（WinRT apartment 已在 main 初始化）。
    JVar parsed;
    try {
        namespace W = winrt::Windows::Data::Json;
        W::JsonValue value =
            W::JsonValue::Parse(winrt::to_hstring("{\"appId\":\"coffee\",\"instanceIdx\":2,"
                                                   "\"flag\":true,\"ratio\":1.5,"
                                                   "\"list\":[1,\"two\"],\"none\":null}"));
        Check(JsonToJVar(value, parsed) && parsed.t == JVar::T::Obj, "json parsed");
    } catch (...) {
        Check(false, "json parse threw");
        return;
    }
    std::string appId;
    long long idx = -1;
    for (const auto& kv : parsed.obj) {
        if (kv.first == "appId") appId = kv.second.s;
        if (kv.first == "instanceIdx") idx = kv.second.i;
    }
    Check(appId == "coffee", "json string field");
    Check(idx == 2, "json int field");
}

}  // namespace

int main() {
    try {
        winrt::init_apartment(winrt::apartment_type::multi_threaded);
    } catch (...) {
    }
    TestKeyRules();
    TestTokenCompare();
    TestCanonicalHash();
    TestSafeWriteValue();
    TestMaskSensitive();
    TestHttpRequestParsing();
    TestResponses();
    TestVerifyRequest();
    TestJobDispatchStateMachine();
    TestQtPosterDispatch();
    TestJsonParsing();
    std::printf("zzz bridge pure tests: %d passed, %d failed\n", g_pass, g_fail);
    return g_fail == 0 ? 0 : 1;
}
