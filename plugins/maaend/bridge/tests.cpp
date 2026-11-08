// tests.cpp - maaend-bridge 核心逻辑测试（纯 stdlib 假传输，无真实网络）
//
// 只包含 maaend_bridge_core.h 与内置 FakeTransport；不链接 WinHTTP/WinSock，
// 不出现 connect/WinHttpConnect/OpenProcess/CreateProcess/SendInput 等导入。
// JSON fixture 字段名与 MaaEnd（MXU 内核）真实 snake_case 契约对齐。
// 由维护工作站的隔离 runner 执行；本文件不依赖仓库 test/ 目录。

#include "maaend_bridge_core.h"

#include <cstdio>
#include <map>
#include <stdexcept>
#include <string>

namespace {

int g_pass = 0;
int g_fail = 0;

#define CHECK(cond)                                                     \
  do {                                                                  \
    if (cond) {                                                         \
      ++g_pass;                                                         \
    } else {                                                            \
      ++g_fail;                                                         \
      std::printf("FAIL %s:%d %s\n", __FILE__, __LINE__, #cond);        \
    }                                                                   \
  } while (0)

using maaend::json::JsonObject;

// ---------------------------------------------------------------------------
// Fixtures：与真实 /api/interface、/api/maa/state、/api/config/snapshot 形状一致
// ---------------------------------------------------------------------------

const char* kInterfaceBody = R"INTERFACE({"interface":{"interface_version":2,"name":"MaaEnd","version":"v3.0.0-fixture","global_option":["GlobalOpt"],"controller":[{"name":"Win32-Front","type":"Win32","option":["CtrlOpt"]}],"resource":[{"name":"官服","path":["./resource"],"option":["ResOpt"]}],"task":[{"name":"SwitchTeam","entry":"SwitchTeam","label":"$task.SwitchTeam.label","option":["TeamOption"]},{"name":"AutoSell","entry":"AutoSell","option":["SellOption"]},{"name":"CreditShopping","entry":"CreditShopping","option":["PickOption","HotkeyOption"]},{"name":"VisitFriends","entry":"VisitFriendsEntry"},{"name":"PickOnly","entry":"PickOnlyEntry","option":["PickOption"]},{"name":"PcOnly","entry":"PcOnlyEntry","controller":["ADB"]},{"name":"WeirdTask","entry":"WeirdEntry","option":["WeirdOpt"]}],"option":{"TeamOption":{"type":"select","cases":[{"name":"A","label":"队伍A","pipeline_override":{"TeamMode":"A"},"option":["TeamNested"]},{"name":"B","pipeline_override":{"TeamMode":"B"}},{"name":"WinOnly","controller":["Win32-Front"],"pipeline_override":{"TeamMode":"Win"}}]},"SellOption":{"type":"input","pipeline_override":{"SellCount":"{count}","SellNote":"{note}","SellFlag":"{flag}","SellPassword":"{password}"},"inputs":[{"name":"count","pipeline_type":"int","verify":"^[0-9]+$"},{"name":"password","password":true},{"name":"image","input_type":"file"},{"name":"note"},{"name":"flag","pipeline_type":"bool","default":"yes"}]},"PickOption":{"type":"checkbox","cases":[{"name":"p1","pipeline_override":{"Pick":"p1"}},{"name":"p2","pipeline_override":{"Pick":"p2"}},{"name":"p3","pipeline_override":{"Pick":"p3"}}],"max_count":2},"HotkeyOption":{"type":"hotkey","hotkeys":[{"name":"key"}]},"GlobalOpt":{"type":"switch","cases":[{"name":"Yes","pipeline_override":{"Global":"on"}},{"name":"No","pipeline_override":{"Global":"off"}}]},"ResOpt":{"type":"switch","cases":[{"name":"Yes","pipeline_override":{"ResMode":"on"}},{"name":"No","pipeline_override":{"ResMode":"off"}}]},"CtrlOpt":{"type":"select","default_case":"Fast","cases":[{"name":"Fast","pipeline_override":{"CtrlSpeed":"fast"}},{"name":"Slow","pipeline_override":{"CtrlSpeed":"slow"}}]},"WeirdOpt":{"type":"orbital","cases":[]},"TeamNested":{"type":"switch","cases":[{"name":"Yes","pipeline_override":{"Nested":"on"}},{"name":"No","pipeline_override":{"Nested":"off"}}]}}},"translations":{"zh_cn":{"task.SwitchTeam.label":"切换队伍"},"en_us":{"task.SwitchTeam.label":"Switch Team"}},"basePath":"C:/FIXTURE/MaaEnd","dataPath":"C:/FIXTURE/data","webServerPort":12701,"backendOS":"windows","backendArch":"x86_64"})INTERFACE";

const char* kInterfaceWrongNameBody = R"({"interface":{"interface_version":2,"name":"OtherTool","version":"v9.9.9","task":[]},"translations":{},"basePath":"C:/X","dataPath":"C:/X","webServerPort":12701,"backendOS":"windows","backendArch":"x86_64"})";

const char* kConfigBody = R"({"version":"1.0","globalOptionValues":{"GlobalOpt":{"type":"switch","value":true}},"instances":[{"id":"inst-1","name":"一号机","controllerName":"Win32-Front","resourceName":"官服","savedDevice":{"adbDeviceAddress":"127.0.0.1:5555","windowName":"Endfield"},"isRunning":false,"selectedTasks":[{"id":"t1","taskName":"SwitchTeam","enabled":true,"expanded":true,"optionValues":{"TeamOption":{"type":"select","caseName":"A"},"TeamNested":{"type":"switch","value":true},"ResOpt":{"type":"switch","value":true},"CtrlOpt":{"type":"select","caseName":"Fast"}}},{"id":"t2","taskName":"AutoSell","enabled":false,"expanded":false,"optionValues":{"SellOption":{"type":"input","values":{"count":"5","password":"hunter2","note":"x"},"encryptedValues":{"password":"enk9x"}}}},{"id":"t3","taskName":"CreditShopping","enabled":true,"expanded":false,"optionValues":{"PickOption":{"type":"checkbox","caseNames":["p1","p2"]},"HotkeyOption":{"type":"hotkey","values":{"key":"Ctrl+Shift+A"}}}}],"preActions":[{"id":"pa1","enabled":true,"program":"C:/tools/launcher.exe","args":"","waitForExit":true,"skipIfRunning":true,"useCmd":false}],"schedulePolicies":[{"id":"sp1","name":"每日","enabled":true,"weekdays":[1],"times":["20:00"]}]},{"id":"inst-2","name":"二号机","selectedTasks":[{"id":"u1","taskName":"VisitFriends","enabled":true,"optionValues":{}}]},{"id":"inst-3","name":"三号机","controllerName":"Win32-Front","resourceName":"官服","selectedTasks":[{"id":"u3","taskName":"VisitFriends","enabled":true,"optionValues":{}},{"id":"u4","taskName":"VisitFriends","enabled":true,"optionValues":{}},{"id":"u5","taskName":"PickOnly","enabled":true,"optionValues":{"PickOption":{"type":"checkbox","caseNames":["p2","p3"]}}},{"id":"u6","taskName":"AutoSell","enabled":true,"optionValues":{"SellOption":{"type":"input","values":{"count":"7","password":"pw-legacy","note":"hi"}}}},{"id":"u7","taskName":"AutoSell","enabled":true,"optionValues":{"SellOption":{"type":"input","values":{"count":"2","password":"","note":"n"},"encryptedValues":{"password":"enc7"}}}},{"id":"u8","taskName":"WeirdTask","enabled":true,"optionValues":{}},{"id":"u9","taskName":"PcOnly","enabled":true,"optionValues":{}}]}],"settings":{"theme":"dark","backgroundImage":"C:/Users/me/bg.png","language":"zh_cn"}})";

std::string ReplaceAll(std::string s, const std::string& from, const std::string& to) {
  size_t pos = 0;
  while ((pos = s.find(from, pos)) != std::string::npos) {
    s.replace(pos, from.size(), to);
    pos += to.size();
  }
  return s;
}

std::string StateBody(bool inst1_running, bool with_instances = true) {
  std::string running = inst1_running ? "true" : "false";
  std::string inst1 = std::string(R"("inst-1":{"connected":true,"resource_loaded":true,"tasker_inited":true,"is_running":)") +
                      running +
                      R"(,"task_run_state":{"statuses":{"t1":"idle"},"mappings":{},"pending_task_ids":[],"current_task_index":0,"overall_status":null}})";
  std::string inst2 =
      R"("inst-2":{"connected":true,"resource_loaded":false,"tasker_inited":true,"is_running":false,"task_run_state":{"statuses":{},"mappings":{},"pending_task_ids":[],"current_task_index":0,"overall_status":null}})";
  std::string inst3 =
      R"("inst-3":{"connected":true,"resource_loaded":true,"tasker_inited":true,"is_running":false,"task_run_state":{"statuses":{},"mappings":{},"pending_task_ids":[],"current_task_index":0,"overall_status":null}})";
  if (!with_instances) {
    return R"({"cached_adb_devices":[],"cached_win32_windows":[]})";
  }
  return "{\"instances\":{" + inst1 + "," + inst2 + "," + inst3 +
         "},\"cached_adb_devices\":[],\"cached_win32_windows\":[]}";
}

// ---------------------------------------------------------------------------
// FakeTransport：模仿 MXU HTTP 后端的语义（含 revision CAS）
// ---------------------------------------------------------------------------

struct FakeUpstream final : public maaend::ITransport {
  bool wrong_name = false;
  bool interface_redirect = false;
  bool snapshot_404 = false;
  bool run_network_error = false;
  bool stop_network_error = false;
  bool save_network_error = false;
  bool drop_task_array = false;
  bool drop_state_instances = false;
  bool scalar_config = false;
  bool drop_selected_tasks = false;
  bool agent_interface = false;
  bool start_network_error = false;
  bool start_bad_receipt = false;
  bool run_bad_receipt = false;

  bool inst1_running = false;
  JsonObject config;
  std::wstring revision = L"rev-1";
  int rev_counter = 1;

  std::map<std::string, int> counts;
  std::string last_run_body;
  std::string last_start_body;
  std::string last_stop_path;
  std::string last_saved_body;

  FakeUpstream() { maaend::ParseJsonObject(kConfigBody, config); }

  int Count(const std::string& key) const {
    auto it = counts.find(key);
    return it == counts.end() ? 0 : it->second;
  }

  maaend::HttpReply Reply(int status, const std::string& body) {
    maaend::HttpReply r;
    r.status = status;
    r.body = body;
    return r;
  }
  maaend::HttpReply NetworkError(const char* reason) {
    maaend::HttpReply r;
    r.network_error = true;
    r.network_reason = reason;
    return r;
  }

  std::string InterfaceBody() const {
    if (wrong_name) return kInterfaceWrongNameBody;
    if (agent_interface) {
      // 用 JsonObject 结构注入 interface.agent（与真实 MaaEnd 声明同形）
      JsonObject root;
      if (maaend::ParseJsonObject(kInterfaceBody, root)) {
        auto iface = root.Lookup(L"interface").GetObject();
        maaend::json::JsonArray agents;
        JsonObject a1;
        maaend::InsertStrUtf8(a1, L"child_exec", "agent/go-service");
        JsonObject a2;
        maaend::InsertStrUtf8(a2, L"child_exec", "agent/cpp-algo");
        maaend::json::JsonArray no_args;
        a2.Insert(L"child_args", no_args);
        agents.Append(a1);
        agents.Append(a2);
        iface.Insert(L"agent", agents);
        root.Insert(L"interface", iface);
        return maaend::Stringify(root);
      }
    }
    if (drop_task_array) {
      JsonObject root;
      if (maaend::ParseJsonObject(kInterfaceBody, root)) {
        auto iface = root.Lookup(L"interface").GetObject();
        if (iface.HasKey(L"task")) iface.Remove(L"task");
        root.Insert(L"interface", iface);
        return maaend::Stringify(root);
      }
    }
    return kInterfaceBody;
  }

  // 当前生效配置 JSON（drift 开关同时作用于 snapshot 与只读回退）
  std::string EffectiveConfigJson() const {
    if (drop_selected_tasks) {
      JsonObject cfg;
      if (maaend::ParseJsonObject(maaend::Stringify(config), cfg)) {
        auto instances = cfg.Lookup(L"instances").GetArray();
        JsonObject first = instances.GetAt(0).GetObject();
        if (first.HasKey(L"selectedTasks")) first.Remove(L"selectedTasks");
        instances.SetAt(0, first);
        cfg.Insert(L"instances", instances);
        return maaend::Stringify(cfg);
      }
    }
    return maaend::Stringify(config);
  }

  std::string SnapshotBody() const {
    if (scalar_config) {
      return R"({"config":42,"revision":"rev-1"})";
    }
    JsonObject cfg;
    if (!maaend::ParseJsonObject(EffectiveConfigJson(), cfg)) {
      return R"({"config":{},"revision":"rev-1"})";
    }
    JsonObject root;
    root.Insert(L"config", cfg);
    root.Insert(L"revision", maaend::json::JsonValue::CreateStringValue(revision));
    return maaend::Stringify(root);
  }

  maaend::HttpReply Request(const maaend::HttpExchange& ex) override {
    std::string key = ex.method + " " + ex.path;
    ++counts[key];

    if (ex.path == "/api/heartbeat") return Reply(200, R"({"ok":true})");
    if (ex.path == "/api/maa/initialized") {
      return Reply(200, R"({"initialized":true,"version":"v4.14.0"})");
    }
    if (ex.path == "/api/maa/state") {
      return Reply(200, StateBody(inst1_running, !drop_state_instances));
    }
    if (ex.path == "/api/interface") {
      if (interface_redirect) {
        return Reply(302, "");
      }
      return Reply(200, InterfaceBody());
    }
    if (ex.path == "/api/config/snapshot") {
      if (snapshot_404) return Reply(404, R"({"error":"not found"})");
      return Reply(200, SnapshotBody());
    }
    if (ex.path == "/api/config" && ex.method == "GET") {
      return Reply(200, EffectiveConfigJson());
    }
    if (ex.path == "/api/config" && ex.method == "POST") {
      JsonObject body;
      if (!maaend::ParseJsonObject(ex.body, body)) {
        return Reply(400, R"({"code":"invalid_config","message":"bad json"})");
      }
      std::wstring client_id;
      if (!maaend::TryGetString(body, L"client_id", client_id) || client_id.empty()) {
        return Reply(400, R"({"code":"invalid_client_id","message":"client_id required"})");
      }
      if (!body.HasKey(L"revision") || body.Lookup(L"revision").ValueType() != maaend::json::JsonValueType::String) {
        return Reply(428, R"({"code":"revision_required","message":"revision required"})");
      }
      std::wstring given(body.Lookup(L"revision").GetString());
      if (given != revision) {
        return Reply(409, R"({"code":"conflict","message":"stale revision"})");
      }
      config = body.Lookup(L"config").GetObject();
      revision = L"rev-" + std::to_wstring(++rev_counter);
      last_saved_body = ex.body;
      if (save_network_error) {
        return NetworkError("timeout");  // 已保存但响应丢失
      }
      JsonObject root;
      root.Insert(L"config", config);
      root.Insert(L"revision", maaend::json::JsonValue::CreateStringValue(revision));
      return Reply(200, maaend::Stringify(root));
    }
    if (ex.path.find("/tasks/start") != std::string::npos) {
      if (start_network_error) return NetworkError("timeout");
      last_start_body = ex.body;
      inst1_running = true;
      if (start_bad_receipt) return Reply(200, R"({"taskIds":"nope"})");
      return Reply(200, R"({"taskIds":[201]})");
    }
    if (ex.path.find("/tasks/run") != std::string::npos) {
      if (run_network_error) return NetworkError("timeout");
      last_run_body = ex.body;
      inst1_running = true;
      if (run_bad_receipt) return Reply(200, R"({"taskIds":[]})");
      return Reply(200, R"({"taskIds":[101]})");
    }
    if (ex.path.find("/tasks/stop") != std::string::npos) {
      if (stop_network_error) return NetworkError("timeout");
      last_stop_path = ex.path;
      inst1_running = false;
      return Reply(200, R"({"ok":true})");
    }
    return Reply(404, R"({"error":"no route"})");
  }
};

// ---------------------------------------------------------------------------
// 断言辅助
// ---------------------------------------------------------------------------

std::string WideToUtf8(const std::wstring& s) { return maaend::ToUtf8(winrt::hstring(s)); }

JsonObject Call(maaend::BridgeContext& ctx, const char* path, const std::string& args_json) {
  JsonObject args;
  if (!maaend::ParseJsonObject(args_json.empty() ? "{}" : args_json, args)) {
    std::printf("Call parse failed on %s; json head: %.200s\n", path, args_json.c_str());
    throw std::runtime_error("bad test args json");
  }
  return maaend::Dispatch(ctx, path, args);
}

std::string ErrorCode(const JsonObject& response) {
  JsonObject err;
  std::wstring code;
  if (maaend::TryGetObject(response, L"error", err) && maaend::TryGetString(err, L"code", code)) {
    return WideToUtf8(code);
  }
  return "";
}

bool IsOk(const JsonObject& response) {
  bool ok = false;
  return maaend::TryGetBool(response, L"ok", ok) && ok;
}

std::string Str(const JsonObject& o, const wchar_t* key) {
  std::wstring v;
  return maaend::TryGetString(o, key, v) ? WideToUtf8(v) : "";
}

JsonObject Sub(const JsonObject& o, const wchar_t* key) {
  JsonObject v;
  if (!maaend::TryGetObject(o, key, v)) throw std::runtime_error("missing sub-object");
  return v;
}

std::string TestCatalogHash(FakeUpstream& fake) {
  return maaend::Sha256Hex(fake.InterfaceBody());
}

std::string CommonUpdateArgs(FakeUpstream& fake, const std::string& extra) {
  return std::string(R"({"instanceId":"inst-1","catalogHash":")") + TestCatalogHash(fake) +
         R"(","baseRevision":")" + WideToUtf8(fake.revision) + R"(")" + extra + "}";
}

// 期望请求体提为具名常量：MSVC 传统预处理器不能把含 \" 的裸字符串作为被
// 字符串化的宏参（#cond 触发 C2017），常量名替代后无此问题。
const char* kExpectSwitchTeamPipeline =
    R"([{"Global":"on"},{"ResMode":"on"},{"CtrlSpeed":"fast"},{"TeamMode":"A"},{"Nested":"on"}])";
const char* kExpectSwitchTeamRunBody =
    R"([{"entry":"SwitchTeam","pipeline_override":"[{\"Global\":\"on\"},{\"ResMode\":\"on\"},{\"CtrlSpeed\":\"fast\"},{\"TeamMode\":\"A\"},{\"Nested\":\"on\"}]","selected_task_id":"t1","task_name":"SwitchTeam"}])";
const char* kExpectVisitRunBody =
    R"([{"entry":"VisitFriendsEntry","pipeline_override":"[{\"Global\":\"on\"},{\"ResMode\":\"on\"},{\"CtrlSpeed\":\"fast\"}]","selected_task_id":"u4","task_name":"VisitFriends"}])";
const char* kExpectSellRunBody =
    R"([{"entry":"AutoSell","pipeline_override":"[{\"Global\":\"on\"},{\"ResMode\":\"on\"},{\"CtrlSpeed\":\"fast\"},{\"SellCount\":7,\"SellNote\":\"hi\",\"SellFlag\":true,\"SellPassword\":\"pw-legacy\"}]","selected_task_id":"u6","task_name":"AutoSell"}])";
const char* kExpectPickPipeline =
    R"("pipeline_override":"[{\"Global\":\"on\"},{\"ResMode\":\"on\"},{\"CtrlSpeed\":\"fast\"},{\"Pick\":\"p2\"},{\"Pick\":\"p3\"}]")";

// ---------------------------------------------------------------------------
// 测试用例
// ---------------------------------------------------------------------------

void TestInboundChecks() {
  auto decide = [](const std::string& method, const std::string& path,
                   const std::string& host, bool origin, const std::string& token,
                   bool configured, size_t length) {
    return maaend::CheckInbound(method, path, host, origin, token, configured,
                                "expected-token", length);
  };
  CHECK(decide("POST", "/status", "127.0.0.1:23121", false, "expected-token", true, 2).allowed);
  CHECK(decide("GET", "/health", "localhost", false, "expected-token", true, 0).allowed);
  CHECK(decide("POST", "/status", "127.0.0.1", false, "", true, 2).http_status == 401);
  CHECK(decide("POST", "/status", "127.0.0.1", false, "wrong", true, 2).http_status == 403);
  CHECK(decide("POST", "/status", "127.0.0.1", false, "expected-token", false, 2).http_status ==
        403);
  CHECK(decide("POST", "/status", "evil.example", false, "expected-token", true, 2).http_status ==
        403);
  CHECK(decide("POST", "/status", "127.0.0.1:23121", true, "expected-token", true, 2)
            .http_status == 403);
  CHECK(decide("POST", "/status", "127.0.0.1:23121", false, "expected-token", true,
               maaend::kMaxInboundBody + 1)
            .http_status == 413);
  CHECK(decide("POST", "/nope", "127.0.0.1", false, "expected-token", true, 2).http_status ==
        404);
  CHECK(decide("GET", "/status", "127.0.0.1", false, "expected-token", true, 0).http_status ==
        405);
}

void TestUpstreamTargetRules() {
  CHECK(maaend::AllowedUpstreamTarget("http", "127.0.0.1"));
  CHECK(!maaend::AllowedUpstreamTarget("http", "localhost"));
  CHECK(!maaend::AllowedUpstreamTarget("http", "192.168.1.5"));
  CHECK(!maaend::AllowedUpstreamTarget("https", "127.0.0.1"));
}

void TestStatusHappy(FakeUpstream& fake, maaend::BridgeContext& ctx) {
  auto out = Call(ctx, "/status", "{}");
  if (!IsOk(out)) std::printf("unexpected response: %.600s\n", maaend::Stringify(out).c_str());
  CHECK(IsOk(out));
  CHECK(Sub(out, L"upstream").Lookup(L"reachable").GetBoolean());
  JsonObject catalog;
  CHECK(maaend::TryGetObject(out, L"catalog", catalog));
  CHECK(maaend::ToUtf8(catalog.Lookup(L"catalogHash").GetString()) ==
        maaend::Sha256Hex(fake.InterfaceBody()));
  JsonObject instances;
  CHECK(maaend::TryGetObject(out, L"instances", instances));
  CHECK(instances.HasKey(L"inst-1") && instances.HasKey(L"inst-2"));
  JsonObject cap;
  CHECK(maaend::TryGetObject(out, L"config", cap));
  CHECK(cap.Lookup(L"snapshotSupported").GetBoolean());
}

void TestCatalog(FakeUpstream& fake, maaend::BridgeContext& ctx) {
  maaend::json::JsonArray matches;
  auto zh = Call(ctx, "/catalog/search", R"({"query":"队伍"})");
  CHECK(IsOk(zh));
  CHECK(maaend::TryGetArray(zh, L"matches", matches) && matches.Size() >= 1);  // 经 zh_cn 翻译命中
  auto sell = Call(ctx, "/catalog/search", R"({"query":"SellOption"})");
  CHECK(IsOk(sell));
  CHECK(maaend::TryGetArray(sell, L"matches", matches) && matches.Size() >= 1);

  auto task = Call(ctx, "/catalog/describe", R"({"name":"SwitchTeam"})");
  CHECK(IsOk(task));
  CHECK(Str(task, L"kind") == "task");
  CHECK(Str(task, L"entry") == "SwitchTeam");
  maaend::json::JsonArray task_options;
  CHECK(maaend::TryGetArray(task, L"options", task_options) && task_options.Size() == 1);

  auto opt = Call(ctx, "/catalog/describe", R"({"name":"TeamOption"})");
  CHECK(IsOk(opt));
  CHECK(Str(opt, L"kind") == "option");
  maaend::json::JsonArray cases;
  CHECK(maaend::TryGetArray(opt, L"cases", cases) && cases.Size() == 3);

  auto none = Call(ctx, "/catalog/describe", R"({"name":"NoSuchThing"})");
  CHECK(!IsOk(none));
  CHECK(ErrorCode(none) == "not_found");
}

void TestRead(FakeUpstream& fake, maaend::BridgeContext& ctx) {
  auto all = Call(ctx, "/read", "{}");
  CHECK(IsOk(all));
  CHECK(all.HasKey(L"maa"));
  JsonObject one = Call(ctx, "/read", R"({"instanceId":"inst-1"})");
  CHECK(IsOk(one));
  JsonObject instances = Sub(one, L"instances");
  CHECK(instances.Size() == 1 && instances.HasKey(L"inst-1"));
  JsonObject missing = Call(ctx, "/read", R"({"instanceId":"inst-x"})");
  CHECK(ErrorCode(missing) == "unknown_instance");
}

void TestInvoke(FakeUpstream& fake, maaend::BridgeContext& ctx) {
  std::string hash = TestCatalogHash(fake);
  std::string args = R"({"instanceId":"inst-1","taskName":"SwitchTeam","catalogHash":")" +
                     hash + R"("})";
  auto out = Call(ctx, "/invoke", args);
  if (!IsOk(out)) std::printf("unexpected response: %.600s\n", maaend::Stringify(out).c_str());
  CHECK(IsOk(out));
  CHECK(out.Lookup(L"accepted").GetBoolean());
  CHECK(out.GetNamedObject(L"verification").GetNamedString(L"status") == L"succeeded");
  CHECK(out.GetNamedObject(L"verification").GetNamedString(L"scope") == L"submission");
  CHECK(!out.Lookup(L"agentStarted").GetBoolean());  // 无 agent 声明 → /tasks/run
  maaend::json::JsonArray task_ids;
  CHECK(maaend::TryGetArray(out, L"taskIds", task_ids) && task_ids.Size() == 1);
  CHECK(task_ids.GetAt(0).ValueType() == maaend::json::JsonValueType::Number);
  // 真实契约形状：TaskConfig 无 serde 重命名，键为 snake_case；pipeline_override 按
  // GUI 算法从已保存选项生成（全局→资源→控制器→任务，含嵌套选项）
  CHECK(fake.last_run_body == kExpectSwitchTeamRunBody);

  auto unknown_task = Call(ctx, "/invoke",
                           ReplaceAll(args, "SwitchTeam", "NotDeclaredTask"));
  CHECK(ErrorCode(unknown_task) == "unknown_task");
  CHECK(fake.Count("POST /api/maa/instances/inst-1/tasks/run") == 1);

  auto stale_hash = Call(ctx, "/invoke", ReplaceAll(args, hash, std::string(64, '0')));
  CHECK(ErrorCode(stale_hash) == "catalog_conflict");

  auto not_ready = Call(ctx, "/invoke", ReplaceAll(args, "inst-1", "inst-2"));
  CHECK(ErrorCode(not_ready) == "instance_not_ready");
  CHECK(fake.Count("POST /api/maa/instances/inst-2/tasks/run") == 0);

  fake.inst1_running = true;
  auto busy = Call(ctx, "/invoke", args);
  CHECK(ErrorCode(busy) == "instance_busy");
  fake.inst1_running = false;

  // 上游提交回执丢失：unknown 且不重试
  fake.run_network_error = true;
  fake.Count("POST /api/maa/instances/inst-1/tasks/run");  // 重置基线读取
  int before = fake.Count("POST /api/maa/instances/inst-1/tasks/run");
  auto unknown = Call(ctx, "/invoke", args);
  CHECK(!IsOk(unknown));
  CHECK(ErrorCode(unknown) == "outcome_unknown");
  CHECK(fake.Count("POST /api/maa/instances/inst-1/tasks/run") == before + 1);
  JsonObject err = Sub(unknown, L"error");
  CHECK(err.HasKey(L"retriable") && !err.Lookup(L"retriable").GetBoolean());
  fake.run_network_error = false;
  fake.inst1_running = false;
}

// 选项生成细节：重复任务名精确 ID、input 占位符与默认值、checkbox 合并顺序、
// 未选中/停用/不兼容在 POST 前拒绝
void TestInvokePipelineOptions(FakeUpstream& fake, maaend::BridgeContext& ctx) {
  std::string hash = TestCatalogHash(fake);
  auto with_sel = [](std::string args, const char* id) {
    return args.substr(0, args.size() - 1) + std::string(",\"selectedTaskId\":\"") + id + "\"}";
  };

  // 同名多条：不给 selectedTaskId 拒绝（不默选第一条）；给了精确选中
  std::string visit = R"({"instanceId":"inst-3","taskName":"VisitFriends","catalogHash":")" +
                      hash + "\"}";
  auto ambiguous = Call(ctx, "/invoke", visit);
  CHECK(ErrorCode(ambiguous) == "ambiguous_task");
  auto u4 = Call(ctx, "/invoke", with_sel(visit, "u4"));
  CHECK(IsOk(u4));
  CHECK(Str(u4, L"selectedTaskId") == "u4");
  // 未保存任何选项 → 全部回退默认值（GUI createDefaultOptionValue 口径）
  CHECK(fake.last_run_body == kExpectVisitRunBody);
  auto wrong_id = Call(ctx, "/invoke", with_sel(visit, "nope"));
  CHECK(ErrorCode(wrong_id) == "unknown_selected_task");
  fake.inst1_running = false;

  // input 占位符：int 裸数字、bool 默认值 true、字符串带引号、遗留明文密码按 GUI 口径进入
  std::string sell = R"({"instanceId":"inst-3","taskName":"AutoSell","catalogHash":")" +
                     hash + "\"}";
  auto sell_ok = Call(ctx, "/invoke", with_sel(sell, "u6"));
  CHECK(IsOk(sell_ok));
  CHECK(fake.last_run_body == kExpectSellRunBody);
  fake.inst1_running = false;

  // 仅密文的密码字段：桥无法解密，POST 前明确拒绝
  int before_u7 = fake.Count("POST /api/maa/instances/inst-3/tasks/run");
  auto cred = Call(ctx, "/invoke", with_sel(sell, "u7"));
  CHECK(ErrorCode(cred) == "unsupported_option");
  CHECK(fake.Count("POST /api/maa/instances/inst-3/tasks/run") == before_u7);

  // checkbox 按 cases 声明顺序合并已选 case
  std::string pick = R"({"instanceId":"inst-3","taskName":"PickOnly","catalogHash":")" +
                     hash + "\"}";
  auto pick_ok = Call(ctx, "/invoke", pick);  // 唯一条目，无需 selectedTaskId
  CHECK(IsOk(pick_ok));
  CHECK(Str(pick_ok, L"selectedTaskId") == "u5");
  CHECK(fake.last_run_body.find(kExpectPickPipeline) != std::string::npos);
  fake.inst1_running = false;

  // 未选中 / 停用 / 控制器不兼容：POST 前拒绝
  int before1 = fake.Count("POST /api/maa/instances/inst-1/tasks/run");
  int before3 = fake.Count("POST /api/maa/instances/inst-3/tasks/run");
  CHECK(ErrorCode(Call(ctx, "/invoke",
                       R"({"instanceId":"inst-3","taskName":"SwitchTeam","catalogHash":")" +
                           hash + R"("})")) == "task_not_selected");
  std::string disabled_args = with_sel(
      R"({"instanceId":"inst-1","taskName":"AutoSell","catalogHash":")" + hash + "\"}", "t2");
  CHECK(ErrorCode(Call(ctx, "/invoke", disabled_args)) == "task_disabled");
  CHECK(ErrorCode(Call(ctx, "/invoke",
                       R"({"instanceId":"inst-3","taskName":"PcOnly","catalogHash":")" +
                           hash + R"("})")) == "task_not_compatible");
  CHECK(fake.Count("POST /api/maa/instances/inst-1/tasks/run") == before1);
  CHECK(fake.Count("POST /api/maa/instances/inst-3/tasks/run") == before3);
}

// 热键与未知选项类型：不猜键码、不静默跳过，POST 前明确拒绝
// Official v3.0.0 global keymap declaration, metadata fixture only.
const char* kOfficialGlobalKeymap = R"KEYMAP({"type":"hotkey","label":"$option.KeymapGeneral.label","hotkeys":[{"name":"KeymapUseCurrentQuickTool","label":"$option.KeymapGeneral.inputs.KeymapUseCurrentQuickTool.label","default":"R"},{"name":"KeymapInteract","label":"$option.KeymapGeneral.inputs.KeymapInteract.label","default":"F"},{"name":"KeymapTrackCurrentMission","label":"$option.KeymapGeneral.inputs.KeymapTrackCurrentMission.label","default":"V"}],"pipeline_override":{"EnterCameraModeLongPress":{"key":"{KeymapUseCurrentQuickTool.primary}"},"EnterCameraModeKeyUp":{"key":"{KeymapUseCurrentQuickTool.primary}"},"AutoCollectClickKey":{"key":"{KeymapInteract.primary}"},"MapNavigatorInteract":{"key":"{KeymapInteract.primary}"},"AutoSellPressStockRedistribution":{"key":"{KeymapInteract.primary}"},"AutoPickFalls":{"key":"{KeymapInteract.primary}"},"AutoPickInteractive":{"key":"{KeymapInteract.primary}"},"ProtocolSpaceTouchExitTouch":{"key":"{KeymapInteract.primary}"},"ProtocolSpaceTouchExitTouchRetry":{"key":"{KeymapInteract.primary}"},"AutoMissionNpcProxyTrackCurrentMission":{"key":"{KeymapTrackCurrentMission.primary}"}}})KEYMAP";

void TestSavedHotkeyPipeline(FakeUpstream& fake, maaend::BridgeContext& ctx) {
  auto catalog = maaend::FetchCatalog(ctx);
  CHECK(catalog.ok);
  auto options = catalog.iface.GetNamedObject(L"option");
  options.Insert(L"KeymapGeneral", JsonObject::Parse(maaend::ToH(kOfficialGlobalKeymap)));
  auto global = catalog.iface.GetNamedArray(L"global_option");
  global.Append(maaend::json::JsonValue::CreateStringValue(L"KeymapGeneral"));
  auto instance = fake.config.GetNamedArray(L"instances").GetAt(0).GetObject();
  auto selected = instance.GetNamedArray(L"selectedTasks").GetAt(0).GetObject();
  auto task = maaend::FindTask(catalog, L"SwitchTeam");
  auto values = JsonObject::Parse(fake.config.GetNamedObject(L"globalOptionValues").Stringify());
  auto generate = [&]() { return maaend::GenerateTaskPipelineOverride(catalog, task.raw, selected, L"Win32-Front", L"官服", values); };
  auto findKey = [](const std::string& pipeline,const wchar_t* node) -> int {
    for(const auto& item:maaend::json::JsonArray::Parse(maaend::ToH(pipeline))) {
      if(item.ValueType()!=maaend::json::JsonValueType::Object) continue;
      auto object=item.GetObject();
      if(object.HasKey(node)) return static_cast<int>(object.GetNamedObject(node).GetNamedNumber(L"key"));
    }
    return -1;
  };
  auto defaults = generate();
  CHECK(defaults.ok);
  CHECK(findKey(defaults.pipeline,L"MapNavigatorInteract")==70);
  CHECK(findKey(defaults.pipeline,L"EnterCameraModeKeyUp")==82);
  CHECK(findKey(defaults.pipeline,L"AutoMissionNpcProxyTrackCurrentMission")==86);
  auto stored = JsonObject::Parse(LR"KEY({"type":"hotkey","values":{"KeymapInteract":"G"}})KEY");
  values.Insert(L"KeymapGeneral",stored);
  auto customized = generate();
  CHECK(customized.ok);
  CHECK(findKey(customized.pipeline,L"MapNavigatorInteract")==71);
  CHECK(findKey(customized.pipeline,L"EnterCameraModeKeyUp")==82);

  auto comboDef=JsonObject::Parse(LR"COMBO({"type":"hotkey","hotkeys":[{"name":"key","default":"A"}],"pipeline_override":{"ComboProbe":{"main":"{key.primary}","one":"{key.modifier1}","two":"{key.modifier2}"}}})COMBO");
  options.Insert(L"Combo",comboDef);
  auto taskDef=JsonObject::Parse(LR"TASK({"option":["Combo"]})TASK");
  auto selectedCombo=JsonObject::Parse(LR"TASK({"optionValues":{"Combo":{"type":"hotkey","values":{"key":"Ctrl+Shift+A"}}}})TASK");
  auto combo = maaend::GenerateTaskPipelineOverride(catalog,taskDef,selectedCombo,L"Win32-Front",L"官服",values);
  CHECK(combo.ok);
  bool found=false;
  for(const auto& item:maaend::json::JsonArray::Parse(maaend::ToH(combo.pipeline))) {
    auto object=item.GetObject();
    if(!object.HasKey(L"ComboProbe")) continue;
    auto keys=object.GetNamedObject(L"ComboProbe");
    CHECK(keys.GetNamedNumber(L"main")==65);
    CHECK(keys.GetNamedNumber(L"one")==17);
    CHECK(keys.GetNamedNumber(L"two")==16);
    found=true;
  }
  CHECK(found);
  CHECK(maaend_hotkeys::Code(L"a",L"MacOS").value()==0);
  CHECK(maaend_hotkeys::Code(L"A",L"Adb").value()==29);
  CHECK(maaend_hotkeys::Code(L"F",L"Linux").value()==33);
  auto controllers=catalog.iface.GetNamedArray(L"controller");
  auto controller=controllers.GetAt(0).GetObject();
  controller.Insert(L"type",maaend::json::JsonValue::CreateStringValue(L"Linux"));
  auto linuxOptions=JsonObject::Parse(L"{\"use_win32_vk_code\":true}");
  controller.Insert(L"linux",linuxOptions);
  CHECK(maaend::HotkeyControllerType(catalog,L"Win32-Front")==L"Win32");
  linuxOptions.Insert(L"use_win32_vk_code",maaend::json::JsonValue::CreateBooleanValue(false));
  CHECK(maaend::HotkeyControllerType(catalog,L"Win32-Front")==L"Linux");
  auto valid=JsonObject::Parse(LR"K({"type":"hotkey","values":{"key":"Ctrl+K"}})K");
  CHECK(maaend::CheckHotkeyValue(comboDef,valid).ok);
  auto invalid=JsonObject::Parse(LR"K({"type":"hotkey","values":{"key":"not-an-actual-key"}})K");
  CHECK(!maaend::CheckHotkeyValue(comboDef,invalid).ok);
}

void TestInvokeUnsupportedBeforePost(FakeUpstream& fake, maaend::BridgeContext& ctx) {
  std::string hash = TestCatalogHash(fake);
  int before1 = fake.Count("POST /api/maa/instances/inst-1/tasks/run");
  int before3 = fake.Count("POST /api/maa/instances/inst-3/tasks/run");
  auto hotkey = Call(ctx, "/invoke",
                     R"({"instanceId":"inst-1","taskName":"CreditShopping","catalogHash":")" +
                         hash + R"("})");
  CHECK(IsOk(hotkey));
  CHECK(fake.Count("POST /api/maa/instances/inst-1/tasks/run") == before1 + 1);
  fake.inst1_running = false;
  auto weird = Call(ctx, "/invoke",
                    R"({"instanceId":"inst-3","taskName":"WeirdTask","catalogHash":")" +
                        hash + R"("})");
  CHECK(ErrorCode(weird) == "unsupported_option");
  auto special = Call(ctx, "/invoke",
                      R"({"instanceId":"inst-1","taskName":"__MXU_SLEEP__","catalogHash":")" +
                          hash + R"("})");
  CHECK(ErrorCode(special) == "unsupported_task");
  CHECK(fake.Count("POST /api/maa/instances/inst-1/tasks/run") == before1 + 1);
  CHECK(fake.Count("POST /api/maa/instances/inst-3/tasks/run") == before3);
}

// interface 声明 agent 时走 /tasks/start：参数全部来自声明与后端状态，模型不提供
void TestInvokeAgentStart(FakeUpstream& fake, maaend::BridgeContext& ctx) {
  fake.agent_interface = true;
  std::string hash = TestCatalogHash(fake);
  std::string args = R"({"instanceId":"inst-1","taskName":"SwitchTeam","catalogHash":")" +
                     hash + "\"}";
  int run_before = fake.Count("POST /api/maa/instances/inst-1/tasks/run");
  int start_before = fake.Count("POST /api/maa/instances/inst-1/tasks/start");
  auto out = Call(ctx, "/invoke", args);
  if (!IsOk(out)) std::printf("unexpected response: %.600s\n", maaend::Stringify(out).c_str());
  CHECK(IsOk(out));
  CHECK(out.Lookup(L"accepted").GetBoolean());
  CHECK(out.Lookup(L"agentStarted").GetBoolean());
  CHECK(fake.Count("POST /api/maa/instances/inst-1/tasks/run") == run_before);
  CHECK(fake.Count("POST /api/maa/instances/inst-1/tasks/start") == start_before + 1);
  CHECK(fake.inst1_running);
  // /start 请求体：外层 snake_case 与真实后端 StartTasksRequest 一致；cwd 省略由
  // 后端使用自身 basePath
  JsonObject body;
  CHECK(maaend::ParseJsonObject(fake.last_start_body, body));
  CHECK(!body.HasKey(L"cwd"));
  maaend::json::JsonArray tasks;
  CHECK(maaend::TryGetArray(body, L"tasks", tasks) && tasks.Size() == 1);
  JsonObject tc = tasks.GetAt(0).GetObject();
  CHECK(Str(tc, L"entry") == "SwitchTeam");
  CHECK(Str(tc, L"selected_task_id") == "t1");
  CHECK(Str(tc, L"task_name") == "SwitchTeam");
  CHECK(Str(tc, L"pipeline_override") == kExpectSwitchTeamPipeline);
  maaend::json::JsonArray agents;
  CHECK(maaend::TryGetArray(body, L"agent_configs", agents) && agents.Size() == 2);
  CHECK(Str(agents.GetAt(0).GetObject(), L"child_exec") == "agent/go-service");
  CHECK(Str(agents.GetAt(1).GetObject(), L"child_exec") == "agent/cpp-algo");
  CHECK(body.HasKey(L"tcp_compat_mode") && !body.Lookup(L"tcp_compat_mode").GetBoolean());
  JsonObject envs;
  CHECK(maaend::TryGetObject(body, L"pi_envs", envs));
  CHECK(Str(envs, L"PI_CLIENT_NAME") == "MXU");
  CHECK(Str(envs, L"PI_INTERFACE_VERSION") == "v2.11.0");
  CHECK(Str(envs, L"PI_VERSION") == "v3.0.0-fixture");
  CHECK(Str(envs, L"PI_CLIENT_MAAFW_VERSION") == "v4.14.0");
  CHECK(Str(envs, L"PI_CLIENT_LANGUAGE") == "en_us");  // settings.language 非法值回退 en_us
  CHECK(Str(envs, L"PI_CONTROLLER").find("Win32-Front") != std::string::npos);
  CHECK(Str(envs, L"PI_RESOURCE").find("resource") != std::string::npos);
  fake.inst1_running = false;

  // 模型不能夹带任何执行参数
  CHECK(ErrorCode(Call(ctx, "/invoke",
                       ReplaceAll(args, "}", R"(,"agent_configs":[]})"))) ==
        "invalid_request");
  CHECK(ErrorCode(Call(ctx, "/invoke",
                       ReplaceAll(args, "}", R"(,"pipeline_override":"{}"})"))) ==
        "invalid_request");
  CHECK(fake.Count("POST /api/maa/instances/inst-1/tasks/start") == start_before + 1);

  // /start 回执丢失与坏回执：unknown 且不重试
  fake.start_network_error = true;
  auto lost = Call(ctx, "/invoke", args);
  CHECK(ErrorCode(lost) == "outcome_unknown");
  JsonObject err = Sub(lost, L"error");
  CHECK(err.HasKey(L"retriable") && !err.Lookup(L"retriable").GetBoolean());
  CHECK(fake.Count("POST /api/maa/instances/inst-1/tasks/start") == start_before + 2);
  fake.start_network_error = false;
  fake.start_bad_receipt = true;
  auto bad = Call(ctx, "/invoke", args);
  CHECK(ErrorCode(bad) == "outcome_unknown");
  CHECK(fake.Count("POST /api/maa/instances/inst-1/tasks/start") == start_before + 3);
  fake.start_bad_receipt = false;
  fake.agent_interface = false;
  fake.inst1_running = false;
}

// /run 的 2xx 坏回执（taskIds 数量不符）：不假成功，unknown 且不重试
void TestInvokeBadRunReceipt(FakeUpstream& fake, maaend::BridgeContext& ctx) {
  fake.run_bad_receipt = true;
  std::string hash = TestCatalogHash(fake);
  int before = fake.Count("POST /api/maa/instances/inst-1/tasks/run");
  auto out = Call(ctx, "/invoke",
                  R"({"instanceId":"inst-1","taskName":"SwitchTeam","catalogHash":")" +
                      hash + "\"}");
  CHECK(ErrorCode(out) == "outcome_unknown");
  JsonObject err = Sub(out, L"error");
  CHECK(err.HasKey(L"retriable") && !err.Lookup(L"retriable").GetBoolean());
  CHECK(fake.Count("POST /api/maa/instances/inst-1/tasks/run") == before + 1);
  fake.run_bad_receipt = false;
  fake.inst1_running = false;
}

void TestStop(FakeUpstream& fake, maaend::BridgeContext& ctx) {
  fake.inst1_running = true;
  auto out = Call(ctx, "/task/stop", R"({"instanceId":"inst-1"})");
  if (!IsOk(out)) std::printf("unexpected response: %.600s\n", maaend::Stringify(out).c_str());
  CHECK(IsOk(out));
  CHECK(out.Lookup(L"stopped").GetBoolean());
  CHECK(out.Lookup(L"verified").GetBoolean());
  CHECK(out.HasKey(L"is_running") && !out.Lookup(L"is_running").GetBoolean());
  CHECK(out.HasKey(L"task_run_state"));  // 终态证据含 task_run_state
  CHECK(!fake.inst1_running);

  fake.stop_network_error = true;
  fake.inst1_running = true;
  int before = fake.Count("POST /api/maa/instances/inst-1/tasks/stop");
  auto unknown = Call(ctx, "/task/stop", R"({"instanceId":"inst-1"})");
  CHECK(ErrorCode(unknown) == "outcome_unknown");
  CHECK(fake.Count("POST /api/maa/instances/inst-1/tasks/stop") == before + 1);
  fake.stop_network_error = false;
  fake.inst1_running = false;
}

void TestConfigReadRedaction(FakeUpstream& fake, maaend::BridgeContext& ctx) {
  auto out = Call(ctx, "/config/read", R"({"instanceId":"inst-1"})");
  if (!IsOk(out)) std::printf("unexpected response: %.600s\n", maaend::Stringify(out).c_str());
  CHECK(IsOk(out));
  std::string body = maaend::Stringify(out);
  // 敏感信息不得出现在响应正文
  CHECK(body.find("hunter2") == std::string::npos);          // 密码明文
  CHECK(body.find("enk9x") == std::string::npos);            // 密码密文
  CHECK(body.find("C:/Users/me/bg.png") == std::string::npos);  // 设置中的路径
  CHECK(body.find("C:/tools/launcher.exe") == std::string::npos);  // 预执行程序
  CHECK(body.find("127.0.0.1:5555") == std::string::npos);   // 保存的设备地址
  CHECK(body.find("<redacted>") != std::string::npos);       // 密码字段被显式脱敏
  CHECK(out.HasKey(L"revision"));

  JsonObject cfg;
  CHECK(maaend::TryGetObject(out, L"config", cfg));
  maaend::json::JsonArray view_instances;  // 视图中的 instances 是数组
  CHECK(maaend::TryGetArray(cfg, L"instances", view_instances));
  CHECK(view_instances.Size() == 1);  // 实例过滤
  maaend::json::JsonArray settings_keys;
  CHECK(maaend::TryGetArray(cfg, L"settingKeys", settings_keys));
  CHECK(settings_keys.Size() == 3);  // 只回显键名
}

void TestConfigUpdateHappy(FakeUpstream& fake, maaend::BridgeContext& ctx) {
  JsonObject original;
  maaend::ParseJsonObject(maaend::Stringify(fake.config), original);
  std::string args = CommonUpdateArgs(
      fake,
      R"(,"taskOrder":["t2","t1","t3"],"taskEnabled":{"t1":false},)"
      R"("optionValues":{"t1":{"TeamOption":{"type":"select","caseName":"B"}}})");
  auto out = Call(ctx, "/config/update", args);
  if (!IsOk(out)) {
    std::printf("config.update failed: %s\n", ErrorCode(out).c_str());
  }
  if (!IsOk(out)) std::printf("unexpected response: %.600s\n", maaend::Stringify(out).c_str());
  CHECK(IsOk(out));
  CHECK(out.Lookup(L"applied").GetBoolean());
  CHECK(out.Lookup(L"verified").GetBoolean());  // 回读核实通过

  // 上游收到的最终配置：只改了允许字段，其它字段与其它实例原样保留
  JsonObject saved = fake.config;
  maaend::json::JsonArray saved_instances;
  CHECK(maaend::TryGetArray(saved, L"instances", saved_instances));
  JsonObject inst1 = saved_instances.GetAt(0).GetObject();
  JsonObject inst2 = saved_instances.GetAt(1).GetObject();
  maaend::json::JsonArray tasks;
  CHECK(maaend::TryGetArray(inst1, L"selectedTasks", tasks));
  CHECK(tasks.Size() == 3);
  CHECK(maaend::ToUtf8(tasks.GetAt(0).GetObject().Lookup(L"id").GetString()) == "t2");
  CHECK(maaend::ToUtf8(tasks.GetAt(1).GetObject().Lookup(L"id").GetString()) == "t1");
  CHECK(maaend::ToUtf8(tasks.GetAt(2).GetObject().Lookup(L"id").GetString()) == "t3");
  CHECK(!tasks.GetAt(1).GetObject().Lookup(L"enabled").GetBoolean());
  CHECK(maaend::ToUtf8(tasks.GetAt(1)
                           .GetObject()
                           .Lookup(L"optionValues")
                           .GetObject()
                           .Lookup(L"TeamOption")
                           .GetObject()
                           .Lookup(L"caseName")
                           .GetString()) == "B");
  // 未触碰字段与其它实例原样（实例隔离 + 未知字段保留）
  CHECK(maaend::DeepEqual(inst1.Lookup(L"savedDevice"),
                          original.Lookup(L"instances").GetArray().GetAt(0).GetObject().Lookup(L"savedDevice")));
  CHECK(maaend::DeepEqual(inst1.Lookup(L"preActions"),
                          original.Lookup(L"instances").GetArray().GetAt(0).GetObject().Lookup(L"preActions")));
  CHECK(maaend::DeepEqual(inst1.Lookup(L"schedulePolicies"),
                          original.Lookup(L"instances").GetArray().GetAt(0).GetObject().Lookup(L"schedulePolicies")));
  CHECK(maaend::DeepEqual(inst2, original.Lookup(L"instances").GetArray().GetAt(1).GetObject()));
  CHECK(maaend::DeepEqual(saved.Lookup(L"settings"), original.Lookup(L"settings")));
  // t2 的凭据密文仍在（写入不得丢失凭据字段）
  CHECK(maaend::Stringify(saved).find("enk9x") != std::string::npos);
}

void TestConfigUpdateConflicts(FakeUpstream& fake, maaend::BridgeContext& ctx) {
  int before = fake.Count("POST /api/config");

  auto stale = Call(ctx, "/config/update",
                    ReplaceAll(CommonUpdateArgs(fake, R"(,"taskEnabled":{"t1":false})"),
                               WideToUtf8(fake.revision), "rev-stale"));
  CHECK(ErrorCode(stale) == "revision_conflict");

  auto wrong_hash =
      Call(ctx, "/config/update",
           ReplaceAll(CommonUpdateArgs(fake, R"(,"taskEnabled":{"t1":false})"),
                      TestCatalogHash(fake), std::string(64, '1')));
  CHECK(ErrorCode(wrong_hash) == "catalog_conflict");

  auto unknown_key = Call(ctx, "/config/update",
                          CommonUpdateArgs(fake, R"(,"shell":"cmd")"));
  CHECK(ErrorCode(unknown_key) == "invalid_request");

  CHECK(fake.Count("POST /api/config") == before);  // 冲突在写之前被拒绝
}

void TestConfigUpdateTaskRules(FakeUpstream& fake, maaend::BridgeContext& ctx) {
  auto drop_id = Call(ctx, "/config/update",
                      CommonUpdateArgs(fake, R"(,"taskOrder":["t1","t2"])"));
  CHECK(ErrorCode(drop_id) == "invalid_task_order");
  auto add_id = Call(ctx, "/config/update",
                     CommonUpdateArgs(fake, R"(,"taskOrder":["t1","t2","t3","t9"])"));
  CHECK(ErrorCode(add_id) == "invalid_task_order");
  auto unknown_enabled = Call(ctx, "/config/update",
                              CommonUpdateArgs(fake, R"(,"taskEnabled":{"t9":true})"));
  CHECK(ErrorCode(unknown_enabled) == "unknown_task");
  auto unknown_option_task = Call(ctx, "/config/update",
                                  CommonUpdateArgs(fake, R"(,"optionValues":{"t9":{"TeamOption":{"type":"select","caseName":"A"}}})"));
  CHECK(ErrorCode(unknown_option_task) == "unknown_task");
}

void TestConfigUpdateOptionSchema(FakeUpstream& fake, maaend::BridgeContext& ctx) {
  auto run = [&](const std::string& extra) {
    return Call(ctx, "/config/update", CommonUpdateArgs(fake, extra));
  };
  // 选项未声明对该任务可用（PickOption 只属于 CreditShopping）
  CHECK(ErrorCode(run(R"(,"optionValues":{"t1":{"PickOption":{"type":"checkbox","caseNames":["p1"]}}})")) ==
        "unknown_option");
  // select 未声明 case
  CHECK(ErrorCode(run(R"(,"optionValues":{"t1":{"TeamOption":{"type":"select","caseName":"Z"}}})")) ==
        "invalid_option_value");
  // input 整数字段违反 verify
  CHECK(ErrorCode(run(R"(,"optionValues":{"t2":{"SellOption":{"type":"input","values":{"count":"abc"}}}})")) ==
        "invalid_option_value");
  // 密码字段：凭据拒绝
  CHECK(ErrorCode(run(R"(,"optionValues":{"t2":{"SellOption":{"type":"input","values":{"password":"new-secret"}}}})")) ==
        "unsupported_option");
  // 文件路径字段拒绝
  CHECK(ErrorCode(run(R"(,"optionValues":{"t2":{"SellOption":{"type":"input","values":{"image":"C:/x.png"}}}})")) ==
        "unsupported_option");
  // 未声明输入字段
  CHECK(ErrorCode(run(R"(,"optionValues":{"t2":{"SellOption":{"type":"input","values":{"bogus":"1"}}}})")) ==
        "invalid_option_value");
  // checkbox 超过 max_count=2
  CHECK(ErrorCode(run(R"(,"optionValues":{"t3":{"PickOption":{"type":"checkbox","caseNames":["p1","p2","p3"]}}})")) ==
        "invalid_option_value");
  // 已声明、可映射的热键可以保存；未知按键仍需拒绝。
  const auto hotkeyUpdate = run(R"(,"optionValues":{"t3":{"HotkeyOption":{"type":"hotkey","values":{"key":"Ctrl+A"}}}})");
  CHECK(IsOk(hotkeyUpdate));
  const auto invalidHotkey = run(R"(,"optionValues":{"t3":{"HotkeyOption":{"type":"hotkey","values":{"key":"not-a-key"}}}})");
  CHECK(ErrorCode(invalidHotkey) == "invalid_option_value");
  // 正常整数 + 文本可通过
  auto good = run(R"(,"optionValues":{"t2":{"SellOption":{"type":"input","values":{"count":"9","note":"hello"}}}})");
  CHECK(IsOk(good));
  CHECK(good.Lookup(L"verified").GetBoolean());
}

void TestSnapshotMissing(FakeUpstream& fake, maaend::BridgeContext& ctx) {
  fake.snapshot_404 = true;
  int before = fake.Count("POST /api/config");
  auto update = Call(ctx, "/config/update",
                     CommonUpdateArgs(fake, R"(,"taskEnabled":{"t1":false})"));
  CHECK(ErrorCode(update) == "unsupported_versioned_write");  // 不退到无版本写
  CHECK(fake.Count("POST /api/config") == before);

  // 无快照也无法按已保存选项生成任务参数，invoke 拒绝
  int run_before = fake.Count("POST /api/maa/instances/inst-1/tasks/run");
  auto invoke = Call(ctx, "/invoke",
                     R"({"instanceId":"inst-1","taskName":"SwitchTeam","catalogHash":")" +
                         TestCatalogHash(fake) + "\"}");
  CHECK(ErrorCode(invoke) == "snapshot_unsupported");
  CHECK(fake.Count("POST /api/maa/instances/inst-1/tasks/run") == run_before);

  auto read = Call(ctx, "/config/read", "{}");
  CHECK(IsOk(read));
  CHECK(!read.Lookup(L"snapshotSupported").GetBoolean());  // 只读回退仍可用
  CHECK(read.HasKey(L"warning"));
  fake.snapshot_404 = false;
}

void TestSaveOutcomeUnknown(FakeUpstream& fake, maaend::BridgeContext& ctx) {
  fake.save_network_error = true;
  int before = fake.Count("POST /api/config");
  auto out = Call(ctx, "/config/update",
                  CommonUpdateArgs(fake, R"(,"taskEnabled":{"t1":false})"));
  CHECK(ErrorCode(out) == "outcome_unknown");
  CHECK(fake.Count("POST /api/config") == before + 1);  // 不重试
  JsonObject err = Sub(out, L"error");
  CHECK(err.HasKey(L"evidenceRevision"));  // 回读证据仍在
  CHECK(err.Lookup(L"possiblyApplied").GetBoolean());
  fake.save_network_error = false;
}

void TestWrongBackend(FakeUpstream& fake, maaend::BridgeContext& ctx) {
  fake.wrong_name = true;
  auto status = Call(ctx, "/status", "{}");
  CHECK(IsOk(status));  // status 是诊断端点，返回总览而不是失败
  JsonObject catalog_error;
  CHECK(maaend::TryGetObject(status, L"catalogError", catalog_error));
  CHECK(maaend::ToUtf8(catalog_error.Lookup(L"code").GetString()) == "wrong_backend");

  std::string args = R"({"instanceId":"inst-1","taskName":"SwitchTeam","catalogHash":")" +
                     std::string(64, '0') + R"("})";
  CHECK(ErrorCode(Call(ctx, "/invoke", args)) == "wrong_backend");
  CHECK(ErrorCode(Call(ctx, "/config/read", "{}")) == "wrong_backend");
  fake.wrong_name = false;
}

void TestUpstreamRedirect(FakeUpstream& fake, maaend::BridgeContext& ctx) {
  fake.interface_redirect = true;
  int before = fake.Count("GET /api/interface");
  auto out = Call(ctx, "/config/read", "{}");
  CHECK(ErrorCode(out) == "upstream_redirect");  // 不跟随重定向
  CHECK(fake.Count("GET /api/interface") == before + 1);
  fake.interface_redirect = false;
}

void TestStructureDrift(FakeUpstream& fake, maaend::BridgeContext& ctx) {
  fake.drop_state_instances = true;
  CHECK(ErrorCode(Call(ctx, "/read", "{}")) == "bad_state_response");
  fake.drop_state_instances = false;

  fake.drop_task_array = true;
  auto none = Call(ctx, "/catalog/describe", R"({"name":"SwitchTeam"})");
  CHECK(ErrorCode(none) == "not_found");  // 不崩溃，明确找不到
  fake.drop_task_array = false;

  fake.scalar_config = true;
  CHECK(ErrorCode(Call(ctx, "/config/update",
                       CommonUpdateArgs(fake, R"(,"taskEnabled":{"t1":false})"))) ==
        "snapshot_invalid");
  fake.scalar_config = false;

  fake.drop_selected_tasks = true;
  CHECK(ErrorCode(Call(ctx, "/config/update",
                       CommonUpdateArgs(fake, R"(,"taskEnabled":{"t1":false})"))) ==
        "structure_drift");
  fake.drop_selected_tasks = false;
}

void TestOptionsParse() {
  char arg_exe[] = "exe";
  char arg_token_env[] = "--token-env";
  char arg_bad_name[] = "bad-name";
  char arg_upstream[] = "--upstream-port";
  char arg_12702[] = "12702";
  char arg_port[] = "--port";
  char arg_23199[] = "23199";
  char* argv0[] = {arg_exe};
  auto defaults = maaend::ParseOptions(1, argv0);
  CHECK(defaults && defaults->port == 23121 && defaults->upstream_port == 12701 &&
        defaults->token_env == "MAAEND_BRIDGE_TOKEN");
  char* argv_bad[] = {arg_exe, arg_token_env, arg_bad_name};
  CHECK(!maaend::ParseOptions(3, argv_bad));
  char* argv_ok[] = {arg_exe, arg_upstream, arg_12702, arg_port, arg_23199};
  auto custom = maaend::ParseOptions(5, argv_ok);
  CHECK(custom && custom->upstream_port == 12702 && custom->port == 23199);
}

}  // namespace

// 阶段包装：任何异常都计数为失败并打印来源，不允许把进程带崩（0xC0000409
// = UCRT abort，此前未捕获异常会吞掉全部缓冲输出）。
#define RUN_STAGE(name, call)                                                   \
  do {                                                                         \
    std::printf("[stage] %s begin\n", name);                                    \
    try {                                                                      \
      call;                                                                    \
      std::printf("[stage] %s ok\n", name);                                     \
    } catch (const winrt::hresult_error& e) {                                   \
      ++g_fail;                                                                \
      std::printf("[stage] %s THREW hresult_error 0x%08lX: %s\n", name,         \
                  static_cast<unsigned long>(e.code().value),                   \
                  maaend::ToUtf8(e.message()).c_str());                         \
    } catch (const std::exception& e) {                                         \
      ++g_fail;                                                                \
      std::printf("[stage] %s THREW std::exception: %s\n", name, e.what());     \
    } catch (...) {                                                             \
      ++g_fail;                                                                \
      std::printf("[stage] %s THREW unknown exception\n", name);                \
    }                                                                          \
  } while (0)

int main() {
  // 崩溃定位前提：关闭缓冲，任何 fastfail 之前的输出都保留。
  std::setvbuf(stdout, nullptr, _IONBF, 0);
  std::printf("maaend-bridge-tests start\n");
  try {
    std::printf("[stage] winrt-init begin\n");
    winrt::init_apartment(winrt::apartment_type::single_threaded);
    std::printf("[stage] winrt-init ok\n");
    // 仅 JSON 初始化确认：不调用任何业务函数，验证隔离桌面/Job 下激活可用。
    JsonObject probe;
    if (!maaend::ParseJsonObject(R"({"probe":true})", probe) || !probe.HasKey(L"probe")) {
      std::printf("json-probe FAILED\n");
      return 2;
    }
    std::printf("[stage] json-probe ok %s\n", maaend::Stringify(probe).c_str());

    std::printf("[stage] fixtures begin\n");
    FakeUpstream fake;
    maaend::BridgeOptions options;
    options.stop_poll_interval_ms = 1;
    options.stop_poll_attempts = 20;
    maaend::BridgeContext ctx{&fake, &options};
    std::printf("[stage] fixtures ok\n");

    RUN_STAGE("TestInboundChecks", TestInboundChecks());
    RUN_STAGE("TestUpstreamTargetRules", TestUpstreamTargetRules());
    RUN_STAGE("TestStatusHappy", TestStatusHappy(fake, ctx));
    RUN_STAGE("TestCatalog", TestCatalog(fake, ctx));
    RUN_STAGE("TestRead", TestRead(fake, ctx));
    RUN_STAGE("TestInvoke", TestInvoke(fake, ctx));
    RUN_STAGE("TestInvokePipelineOptions", TestInvokePipelineOptions(fake, ctx));
    RUN_STAGE("TestSavedHotkeyPipeline", TestSavedHotkeyPipeline(fake, ctx));
    RUN_STAGE("TestInvokeUnsupportedBeforePost", TestInvokeUnsupportedBeforePost(fake, ctx));
    RUN_STAGE("TestInvokeAgentStart", TestInvokeAgentStart(fake, ctx));
    RUN_STAGE("TestInvokeBadRunReceipt", TestInvokeBadRunReceipt(fake, ctx));
    RUN_STAGE("TestStop", TestStop(fake, ctx));
    RUN_STAGE("TestConfigReadRedaction", TestConfigReadRedaction(fake, ctx));
    RUN_STAGE("TestConfigUpdateHappy", TestConfigUpdateHappy(fake, ctx));
    RUN_STAGE("TestConfigUpdateConflicts", TestConfigUpdateConflicts(fake, ctx));
    RUN_STAGE("TestConfigUpdateTaskRules", TestConfigUpdateTaskRules(fake, ctx));
    RUN_STAGE("TestConfigUpdateOptionSchema", TestConfigUpdateOptionSchema(fake, ctx));
    RUN_STAGE("TestSnapshotMissing", TestSnapshotMissing(fake, ctx));
    RUN_STAGE("TestSaveOutcomeUnknown", TestSaveOutcomeUnknown(fake, ctx));
    RUN_STAGE("TestWrongBackend", TestWrongBackend(fake, ctx));
    RUN_STAGE("TestUpstreamRedirect", TestUpstreamRedirect(fake, ctx));
    RUN_STAGE("TestStructureDrift", TestStructureDrift(fake, ctx));
    RUN_STAGE("TestOptionsParse", TestOptionsParse());

    std::printf("maaend-bridge tests: %d passed, %d failed\n", g_pass, g_fail);
    return g_fail == 0 ? 0 : 1;
  } catch (const winrt::hresult_error& e) {
    std::printf("TOP-LEVEL hresult_error 0x%08lX: %s\n",
                static_cast<unsigned long>(e.code().value),
                maaend::ToUtf8(e.message()).c_str());
    return 2;
  } catch (const std::exception& e) {
    std::printf("TOP-LEVEL std::exception: %s\n", e.what());
    return 2;
  } catch (...) {
    std::printf("TOP-LEVEL unknown exception\n");
    return 2;
  }
}
