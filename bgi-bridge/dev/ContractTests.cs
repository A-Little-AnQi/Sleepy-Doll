using System.ComponentModel.DataAnnotations;
using System.IO;
using System.Text.Json;
using System.Text.Json.Nodes;
using System.Text.Json.Serialization;
using BgiBridge.Bgi;
using BgiBridge.Catalog;
using BgiBridge.Protocol;
using BgiBridge.Tools;
using BgiBridge.Hosting;
using System.Net;
using System.Net.Http;
using System.Net.Http.Json;

var temporary = Path.Combine(Path.GetTempPath(), "bgi-contract-tests-" + Guid.NewGuid().ToString("N"));
Directory.CreateDirectory(temporary);
try
{
    var registry = new MethodRegistry();
    StatusTools.Register(registry);
    CatalogTools.Register(registry);
    foreach (var descriptor in registry.All)
    {
        Check(descriptor.Guide.Purpose.Length > 12, "purpose missing: " + descriptor.Id);
        Check(descriptor.Guide.WhenToUse.Length > 0 && descriptor.Guide.Preconditions.Length > 0, "usage missing: " + descriptor.Id);
        Check(descriptor.Guide.Rollback.Length > 0 && descriptor.Guide.Verification.Length > 0, "recovery contract missing: " + descriptor.Id);
        foreach (var example in descriptor.Guide.Examples) ArgumentSchema.Validate(example, descriptor.InputSchema);
        var discovery = JsonSerializer.SerializeToElement(descriptor.Discovery(true, null, "test-version"));
        Check(discovery.GetProperty("summary").GetString() == descriptor.Guide.Purpose, "discovery hides purpose");
        Check(discovery.GetProperty("whenToUse").GetArrayLength() > 0, "discovery hides usage");
        Check(discovery.GetProperty("sideEffects").GetArrayLength() > 0, "discovery hides effects");
        Check(discovery.GetProperty("parameters").GetArrayLength() == descriptor.InputSchema.GetProperty("properties").EnumerateObject().Count(), "discovery hides parameters");
    }
    Check(registry.Count == 16, "all core APIs must have contracts");
    var runGroup = registry.All.Single(method => method.Id == "bgi.run_script_group");
    Check(runGroup.Effect == "gameWrite" && runGroup.RequiresGameReady,
        "named script-group execution must require game readiness");
    var updateScripts = registry.All.Single(method => method.Id == "bgi.update_subscribed_scripts");
    Check(updateScripts.Effect == "hostCommand" && !updateScripts.RequiresGameReady,
        "repository updates must not require a running game");
    Check(Reflect.Singleton(typeof(DerivedContractSingleton)) is DerivedContractSingleton,
        "inherited static singleton accessor is not discoverable");
    Console.WriteLine("PASS: all core method guides, schemas and examples");
    var activated = CommandDocumentation.Purpose("MainWindowViewModel", "ActivatedCommand", null);
    Check(activated.Contains("剪贴板") && activated.Contains("导入脚本") && activated.Contains("兑换码"), "Activated purpose hides its actual side effects");
    Check(CommandDocumentation.Title("MainWindowViewModel", "DismissRedeemCodeCommand", null) == "关闭兑换码更新提示", "ambiguous close title");
    Check(CommandDocumentation.Purpose("MusicPageViewModel", "SelectMusicFolderCommand", null).Contains("argument"), "music folder parameter purpose missing");
    Check(CommandDocumentation.UnavailableReason("MainWindowViewModel", "ActivatedCommand") is not null, "internal UI event exposed as standalone command");
    var undocumented = CommandDocumentation.Purpose("UnknownPanelViewModel", "DoSomethingCommand", null);
    Check(undocumented.Contains("不安排自动调用") && !undocumented.Contains("交互处理"),
        "undocumented commands still pretend to have safe business semantics");
    Check(!CommandDocumentation.RequiresGameReady("HomePageViewModel", "StartTriggerCommand")
        && CommandDocumentation.RequiresGameReady("MapPathingViewModel", "StartCommand"), "game readiness rules conflate capture startup and game actions");
    Console.WriteLine("PASS: command titles name their targets and UI events disclose real behavior");

    // 多词查询必须命中：调用方传的是「调度器 scheduler」这类中英混排查询。
    var searchRegistry = new MethodRegistry();
    var emptySchema = J("""{"type":"object","properties":{}}""");
    searchRegistry.Register("setting.schedulerFailureCount", "settings", "",
        (_, _) => Task.FromResult<object?>(null), readOnly: false, inputSchema: emptySchema,
        guide: TestGuide("调度器异常重启次数", "调度器任务连续异常退出几次任务自动重启。", "调整调度器容错时"));
    searchRegistry.Register("setting.schedulerLogCookie", "settings", "",
        (_, _) => Task.FromResult<object?>(null), readOnly: false, inputSchema: emptySchema,
        guide: TestGuide("调度器日志 Cookie", "与调度器日志处相互同步 cookie。", "排查调度器日志时"));
    searchRegistry.Register("cmd.auto_battle", "command", "",
        (_, _) => Task.FromResult<object?>(null), readOnly: false, inputSchema: emptySchema,
        guide: TestGuide("自动战斗", "启动自动战斗任务。", "需要自动战斗调度器时"));

    var mixed = searchRegistry.Search("调度器 scheduler", 50).Select(x => x.Id).ToArray();
    Check(mixed.Length == 3, "多词查询丢了条目：" + string.Join(",", mixed));
    Check(mixed[0].StartsWith("setting.scheduler") && mixed[1].StartsWith("setting.scheduler"),
        "命中两个词的条目应排在前：" + string.Join(",", mixed));
    Check(mixed[2] == "cmd.auto_battle", "只命中一个词的条目应排在最后：" + string.Join(",", mixed));
    Check(searchRegistry.Search("调度器", 50).Count() == 3, "单词查询行为改变");
    Check(!searchRegistry.Search("没有这个能力", 50).Any(), "无命中时必须返回空");
    // 调用方常直接传接口名，此时不该再要求它描述一遍。
    Check(searchRegistry.Search("cmd.auto_battle", 50).Single().Id == "cmd.auto_battle",
        "精确接口名未走快速通道");
    // `+词` 必须命中：排除只靠中文说明命中的条目。
    var requiredOnly = searchRegistry.Search("+scheduler", 50).Select(x => x.Id).ToArray();
    Check(requiredOnly.Length == 2 && !requiredOnly.Contains("cmd.auto_battle"), "+词未生效");
    // 名称命中权重高于说明命中：`scheduler` 在两条设置的 id 里。
    var byName = searchRegistry.Search("scheduler 重启", 50).ToArray();
    Check(byName.Length > 0 && byName[0].Id == "setting.schedulerFailureCount",
        "名称命中未排在说明命中之前：" + string.Join(",", byName.Select(x => x.Id)));
    Console.WriteLine("PASS: multi-term catalog search matches any term");

    var socket = new System.Net.Sockets.TcpListener(IPAddress.Loopback, 0);
    socket.Start();
    var port = ((IPEndPoint)socket.LocalEndpoint).Port;
    socket.Stop();
    var executions = 0;
    var httpRegistry = new MethodRegistry();
    httpRegistry.Register("bgi.commit_settings", "settings", "", (_, _) =>
    {
        Interlocked.Increment(ref executions);
        return Task.FromResult<object?>(new { verified = true });
    }, readOnly: false);
    var bridgeConfig = new BgiBridge.BridgeConfig { Listen = "127.0.0.1:" + port, Token = Guid.NewGuid().ToString("N") };
    var httpHost = new BridgeHost(bridgeConfig, httpRegistry, new BgiBridge.Jobs.JobStore(), "contract-test");
    httpHost.Start();
    try
    {
        using var http = new HttpClient { BaseAddress = new Uri("http://127.0.0.1:" + port), Timeout = TimeSpan.FromSeconds(5) };
        http.DefaultRequestHeaders.Authorization = new("Bearer", bridgeConfig.Token);
        var info = await http.GetFromJsonAsync<JsonElement>("/bridge/v1/info");
        // agent 侧靠这两个路径定位用户文件，缺一个文件服务就没法工作。
        var host = await http.GetFromJsonAsync<JsonElement>("/bridge/v1/host");
        Check(Path.IsPathFullyQualified(host.GetProperty("installPath").GetString()!), "host install path is not absolute");
        Check(Path.GetFileName(host.GetProperty("userPath").GetString()!) == "User"
            && Path.IsPathFullyQualified(host.GetProperty("userPath").GetString()!), "host user path is wrong");
        var firstPage = await http.GetFromJsonAsync<JsonElement>("/bridge/v1/catalog");
        var discovered = firstPage.GetProperty("items")[0];
        Check(discovered.GetProperty("parameters")[0].GetProperty("description").GetString()!.Contains("计划"), "HTTP catalog missing call guidance");
        var request = new { methodId = "bgi.commit_settings", arguments = new { planId = "test-plan" },
            instanceId = info.GetProperty("instanceId").GetString(), catalogVersion = info.GetProperty("catalogVersion").GetString(), requestId = "same-request" };
        using var accepted = await http.PostAsJsonAsync("/bridge/v1/invoke", request);
        Check(accepted.StatusCode == HttpStatusCode.Accepted, "write not accepted");
        var firstJob = await accepted.Content.ReadFromJsonAsync<JsonElement>();
        using var replay = await http.PostAsJsonAsync("/bridge/v1/invoke", request);
        var replayJob = await replay.Content.ReadFromJsonAsync<JsonElement>();
        Check(firstJob.GetProperty("jobId").GetString() == replayJob.GetProperty("jobId").GetString() && replayJob.GetProperty("replayed").GetBoolean(), "idempotent replay failed");
        for (var attempt = 0; attempt < 50 && executions == 0; attempt++) await Task.Delay(10);
        Check(executions == 1, "duplicate write executed twice");
        using var invalid = await http.PostAsJsonAsync("/bridge/v1/invoke", new { methodId = "bgi.commit_settings", arguments = new { planId = "x" }, instanceId = 123 });
        Check(invalid.StatusCode == HttpStatusCode.BadRequest, "wrong envelope type must be 400");
        using var disabled = await http.PostAsJsonAsync("/bridge/v1/control", new { enabled = false });
        var disabledPage = await http.GetFromJsonAsync<JsonElement>("/bridge/v1/catalog");
        Check(disabledPage.GetProperty("items").GetArrayLength() == 1 && !disabledPage.GetProperty("items")[0].GetProperty("callable").GetBoolean(), "disabled methods disappeared from docs");
        using var refused = await http.PostAsJsonAsync("/bridge/v1/invoke", request);
        Check(!refused.IsSuccessStatusCode && executions == 1, "disabled bridge executed a write");
    }
    finally { httpHost.Stop(); }
    Console.WriteLine("PASS: HTTP discovery, disabled docs, invalid envelope and idempotent execution");

    var root = new TestConfig();
    var configPath = Path.Combine(temporary, "config.json");
    var options = new JsonSerializerOptions { PropertyNamingPolicy = JsonNamingPolicy.CamelCase, WriteIndented = true };
    File.WriteAllText(configPath, JsonSerializer.Serialize(root, options));
    var withUnknown = JsonNode.Parse(File.ReadAllText(configPath))!.AsObject();
    withUnknown["futureHostField"] = "preserve";
    File.WriteAllText(configPath, withUnknown.ToJsonString());
    var saves = 0;
    root.OnAnyChangedAction = () => saves++;
    var engine = new SettingsTransactionEngine(() => root, () => configPath, () => options, temporary);
    string Version(string path) => SettingsCatalog.Build(root).Single(item => item.Path == path).ValueVersion;
    string Plan(params SettingChange[] changes) => JsonSerializer.SerializeToElement(engine.Preview(changes)).GetProperty("planId").GetString()!;
    var initial = File.ReadAllBytes(configPath);

    Reject(() => engine.Preview([new("onAnyChangedAction", J("null"), "x")]), "ignored members are not writable");
    Reject(() => engine.Preview([new("count", J("0"), Version("count"))]), "range violations rejected");
    Reject(() => engine.Preview([new("mode", J("\"Unknown\""), Version("mode"))]), "unknown enum rejected");
    Reject(() => engine.Preview([new("flag", J("\"true\""), Version("flag"))]), "wrong JSON type rejected");
    Check(File.ReadAllBytes(configPath).SequenceEqual(initial), "validation mutated disk");
    Console.WriteLine("PASS: paths, range, enum and JSON type validation before mutation");

    var plan = Plan(new SettingChange("flag", J("true"), Version("flag")));
    Check(!root.Flag && File.ReadAllBytes(configPath).SequenceEqual(initial), "preview changed state");
    var committed = JsonSerializer.SerializeToElement(engine.Commit(plan));
    Check(root.Flag && committed.GetProperty("verified").GetBoolean(), "commit not verified");
    Check(JsonNode.Parse(File.ReadAllText(configPath))!["flag"]!.GetValue<bool>(), "disk did not change");
    Check(JsonNode.Parse(File.ReadAllText(configPath))!["futureHostField"]!.GetValue<string>() == "preserve", "unknown field lost");
    Check(saves == 0 && root.OnAnyChangedAction is not null, "autosave was not suppressed/restored");
    engine.Commit(plan);
    var otherChange = JsonNode.Parse(File.ReadAllText(configPath))!.AsObject();
    root.Other = "manual";
    otherChange["other"] = "manual";
    File.WriteAllText(configPath, otherChange.ToJsonString());
    engine.Rollback(plan);
    Check(!root.Flag && root.Other == "manual", "rollback overwrote unrelated memory");
    var restored = JsonNode.Parse(File.ReadAllText(configPath))!;
    Check(!restored["flag"]!.GetValue<bool>() && restored["other"]!.GetValue<string>() == "manual", "rollback overwrote unrelated disk values");
    Check(restored["futureHostField"]!.GetValue<string>() == "preserve", "rollback lost unknown field");
    Console.WriteLine("PASS: preview, atomic commit, idempotent plan and field-scoped rollback");

    var beforeFailure = File.ReadAllBytes(configPath);
    var failing = Plan(new SettingChange("flag", J("true"), Version("flag")), new SettingChange("count", J("13"), Version("count")));
    Reject(() => engine.Commit(failing), "throwing setter must fail");
    Check(!root.Flag && root.Count == 3, "setter failure did not restore memory");
    Check(File.ReadAllBytes(configPath).SequenceEqual(beforeFailure), "setter failure changed disk");
    Check(root.OnAnyChangedAction is not null, "failure lost autosave callback");
    Console.WriteLine("PASS: setter failure compensates memory and keeps original file");

    var conflicting = Plan(new SettingChange("flag", J("true"), Version("flag")));
    root.Other = "changed-after-preview";
    Reject(() => engine.Commit(conflicting), "stale plan must conflict");
    Check(!root.Flag, "stale plan changed memory");
    var currentDisk = JsonNode.Parse(File.ReadAllText(configPath))!.AsObject();
    currentDisk["other"] = root.Other;
    File.WriteAllText(configPath, currentDisk.ToJsonString());
    var targetConflict = Plan(new SettingChange("flag", J("true"), Version("flag")));
    engine.Commit(targetConflict);
    root.Flag = false;
    var edited = JsonNode.Parse(File.ReadAllText(configPath))!.AsObject();
    edited["flag"] = false;
    File.WriteAllText(configPath, edited.ToJsonString());
    Reject(() => engine.Rollback(targetConflict), "rollback must not overwrite later target edits");
    Console.WriteLine("PASS: stale-plan and rollback compare-and-set conflicts");

    var secret = SettingsCatalog.Build(root).Single(item => item.Path == "apiToken");
    Check(secret.Sensitive && secret.CurrentValue?.ToString() == "***REDACTED***", "secret leaked through settings catalog");
    var secretPlan = Plan(new SettingChange("apiToken", J("\"new-private-token\""), secret.ValueVersion));
    var secretResult = JsonSerializer.Serialize(engine.Commit(secretPlan));
    Check(!secretResult.Contains("private-token") && !JsonSerializer.Serialize(engine.History()).Contains("private-token"), "secret leaked through public change records");
    engine.Rollback(secretPlan);
    Check(root.ApiToken == "original-private-token", "secret rollback failed");
    Console.WriteLine("PASS: sensitive values are redacted from public results and recoverable");

    // Only a disposable fake installation is touched; never restore the user's test host here.
    if (System.Diagnostics.Process.GetProcessesByName("BetterGI").Length == 0)
    {
        var offlineRoot = Path.Combine(temporary, "offline");
        Directory.CreateDirectory(Path.Combine(offlineRoot, "User"));
        var fakeHost = Path.Combine(offlineRoot, "BetterGI.exe");
        File.WriteAllBytes(fakeHost, []);
        var offlineConfig = Path.Combine(offlineRoot, "User", "config.json");
        File.WriteAllText(offlineConfig, JsonSerializer.Serialize(root, options));
        var originalOffline = File.ReadAllBytes(offlineConfig);
        var offlineEngine = new SettingsTransactionEngine(() => root, () => offlineConfig, () => options, offlineRoot, () => fakeHost);
        var offlinePlan = JsonSerializer.SerializeToElement(offlineEngine.Preview([new("other", J("\"changed-offline\""), Version("other"))])).GetProperty("planId").GetString()!;
        offlineEngine.Commit(offlinePlan);
        var record = JsonSerializer.SerializeToElement(SettingsRecovery.List(offlineRoot)).GetProperty("records")[0];
        Reject(() => SettingsRecovery.Restore(offlineRoot, offlinePlan, record.GetProperty("recordVersion").GetString()!, "stale"), "offline stale file accepted");
        SettingsRecovery.Restore(offlineRoot, offlinePlan, record.GetProperty("recordVersion").GetString()!, record.GetProperty("currentVersion").GetString()!);
        Check(File.ReadAllBytes(offlineConfig).SequenceEqual(originalOffline), "offline restore was not byte-exact");
        Console.WriteLine("PASS: offline recovery conflict detection and exact backup restore");
    }
    else Console.WriteLine("SKIP: offline recovery requires every BetterGI process to be stopped");

    var ready = new ManualResetEventSlim();
    System.Windows.Application? app = null;
    var thread = new Thread(() => { app = new System.Windows.Application(); ready.Set(); System.Windows.Threading.Dispatcher.Run(); });
    thread.SetApartmentState(ApartmentState.STA);
    thread.Start();
    ready.Wait();
    try
    {
        Check(await Ui.InvokeAsync(() => 42) == 42, "generic UI return broken");
        Check(await Ui.InvokeAsync(async () => { await Task.Delay(10); return 43; }) == 43, "nested task not unwrapped");
    }
    finally { app!.Dispatcher.InvokeShutdown(); thread.Join(); }
    Console.WriteLine("PASS: UI dispatcher generic and asynchronous delegates");
}
finally
{
    var full = Path.GetFullPath(temporary);
    var relative = Path.GetRelativePath(Path.GetTempPath(), full);
    if (!relative.StartsWith("..") && Path.GetFileName(full).StartsWith("bgi-contract-tests-")) Directory.Delete(full, true);
}

static JsonElement J(string value) => JsonSerializer.Deserialize<JsonElement>(value);
static AgentGuide TestGuide(string title, string purpose, string when) => new(
    title, purpose, [when], ["桥已连接。"], ["只读。"], "结果含义。", "核验方式。", "只读，无需回退。", [J("{}")]);
static void Check(bool condition, string message) { if (!condition) throw new Exception(message); }
static void Reject(Action action, string message)
{
    try { action(); } catch (BridgeException) { return; }
    throw new Exception(message);
}
public enum TestMode { First, Second }
public class ContractSingleton<T> where T : new()
{
    public static T Instance { get; } = new();
}
public sealed class DerivedContractSingleton : ContractSingleton<DerivedContractSingleton> { }
public sealed class TestConfig
{
    private bool _flag;
    private int _count = 3;
    public bool Flag { get => _flag; set { _flag = value; OnAnyChangedAction?.Invoke(); } }
    [Range(1,100)]
    public int Count { get => _count; set { _count = value; OnAnyChangedAction?.Invoke(); if (value == 13) throw new InvalidOperationException("simulated setter failure"); } }
    public TestMode Mode { get; set; }
    public string Other { get; set; } = "before";
    public string ApiToken { get; set; } = "original-private-token";
    [JsonIgnore]
    public Action? OnAnyChangedAction { get; set; }
}
