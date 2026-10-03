// Production-core acceptance: real file flows for SyncCore / AggregateRouteRequirements /
// DescribeEffectiveStrategy / EffectiveFieldReport. Fixtures live under the registered
// target/.tmp/shortcut-runtime cache via %TEMP%; no deletion here (root cleans the cache).
using System.IO;
using System.Text.Json.Nodes;
using BgiBridge.Tools;

static void Require(bool value, string message) { if (!value) throw new Exception(message); }

var cache = Environment.ExpandEnvironmentVariables("%TEMP%");
var root = Path.Combine(cache, "production-core-fixtures");
Directory.CreateDirectory(root);
var id = Guid.NewGuid().ToString("N")[..8];
var userRoot = Path.Combine(root, $"user-{id}");
var groupDir = Path.Combine(userRoot, "ScriptGroup");
var routeDir = Path.Combine(userRoot, "AutoPathing", "兽怪暴徒");
Directory.CreateDirectory(groupDir);
Directory.CreateDirectory(routeDir);

// ---- requirements aggregation over real route files (production path) ----
File.WriteAllText(Path.Combine(routeDir, "01.json"), """
{"info":{"name":"01"},"positions":[
 {"action":"","type":"teleport","x":1,"y":2},
 {"action":"fight","type":"path","x":3,"y":4}
]}
""");
var requirements = PathingPreparationTools.AggregateRouteRequirements(userRoot, ["兽怪暴徒/01.json"]);
var serialized = System.Text.Json.JsonSerializer.SerializeToElement(requirements);
Require(serialized.GetProperty("battle").GetBoolean(), "fight action must aggregate as battle");
Require(serialized.GetProperty("partyConfirmationRequired").GetBoolean(), "battle route must require party confirmation");
Require(serialized.GetProperty("status").GetString() == "preconditionsFound", $"status: {serialized.GetProperty("status")}");

File.WriteAllText(Path.Combine(routeDir, "02.json"), """
{"positions":[{"action":"","type":"teleport"},{"action":"pick_around","type":"path"},{"action":"四叶印","type":"path"}]}
""");
var plain = System.Text.Json.JsonSerializer.SerializeToElement(
    PathingPreparationTools.AggregateRouteRequirements(userRoot, ["兽怪暴徒/02.json"]));
Require(plain.GetProperty("status").GetString() == "noPreconditions", "benign-only route must not require confirmation");
Require(!plain.GetProperty("partyConfirmationRequired").GetBoolean(), "benign route flagged as requiring confirmation");

File.WriteAllText(Path.Combine(routeDir, "03.json"), """
{"positions":[{"action":"some_new_mechanic","type":"path"}]}
""");
var odd = System.Text.Json.JsonSerializer.SerializeToElement(
    PathingPreparationTools.AggregateRouteRequirements(userRoot, ["兽怪暴徒/03.json"]));
Require(odd.GetProperty("status").GetString() == "unknown", "unrecognized action must be unknown, not benign");
Require(odd.GetProperty("partyConfirmationRequired").GetBoolean(), "unknown must still require confirmation (fail closed)");

var missing = System.Text.Json.JsonSerializer.SerializeToElement(
    PathingPreparationTools.AggregateRouteRequirements(userRoot, ["兽怪暴徒/不存在.json"]));
Require(missing.GetProperty("status").GetString() == "unknown" && missing.GetProperty("partyConfirmationRequired").GetBoolean(),
    "unreadable files must not be presented as no-preconditions");

// ---- SyncCore: real CAS file flow ----
var groupFile = Path.Combine(groupDir, "讨伐组.json");
File.WriteAllText(groupFile, """
{"name":"讨伐组","config":{"pathingConfig":{"enabled":true,"partyName":"精英","hurryOnAvatar":"玛薇卡",
"autoFightConfig":{"strategyName":"根据队伍自动选择","pickDropsAfterFightEnabled":true,"timeout":120},
"autoEatConfig":{"enabled":false}}},"projects":[]}
""");
byte[] GroupBytes() => System.Text.Encoding.UTF8.GetBytes(File.ReadAllText(groupFile));
string Sha() => Convert.ToHexString(System.Security.Cryptography.SHA256.HashData(GroupBytes())).ToLowerInvariant();
var expected = Sha();
var globalFight = JsonNode.Parse("""
{"strategyName":"万能战斗策略（萌新推荐）","pickDropsAfterFightEnabled":false,"timeout":60,"kazuhaPickupEnabled":true}
""")!;
var synced = System.Text.Json.JsonSerializer.SerializeToElement(
    PathingPreparationTools.SyncCore(userRoot, "讨伐组", expected,
        new Dictionary<string, JsonNode?> { ["AutoFightConfig"] = globalFight.DeepClone() }));
Require(synced.GetProperty("synced").GetBoolean() && synced.GetProperty("otherFieldsPreserved").GetBoolean(),
    "sync must verify replaced fields and preserved tail");
var after = JsonNode.Parse(File.ReadAllText(groupFile))!;
Require(after["config"]!["pathingConfig"]!["autoFightConfig"]!["pickDropsAfterFightEnabled"]!.GetValue<bool>() is false,
    "global pickDrops=false must survive into the group (effective behavior)");
Require(after["config"]!["pathingConfig"]!["autoFightConfig"]!["strategyName"]!.GetValue<string>().Contains("万能"),
    "global strategy name must be inherited");
Require(after["config"]!["pathingConfig"]!["partyName"]!.GetValue<string>() == "精英", "partyName must stay untouched");
Require(after["config"]!["pathingConfig"]!["hurryOnAvatar"]!.GetValue<string>() == "玛薇卡", "hurry avatar must stay untouched");
// 旧 SHA 再同步：CAS 拒绝。
var conflict = false;
try { PathingPreparationTools.SyncCore(userRoot, "讨伐组", expected,
    new Dictionary<string, JsonNode?> { ["AutoFightConfig"] = globalFight.DeepClone() }); }
catch (BgiBridge.Protocol.BridgeException error) { conflict = error.Message.Contains("不一致"); }
Require(conflict, "stale expectedSha256 must be rejected with VERSION_CONFLICT");

// ---- EffectiveFieldReport: source classification over real JSON views ----
var pathingView = after["config"]!["pathingConfig"]!.AsObject();
var globalView = new JsonObject
{
    ["AutoFightConfig"] = globalFight.DeepClone(),
    ["PartyName"] = JsonValue.Create("别队"),
};
var report = System.Text.Json.JsonSerializer.SerializeToElement(
    PathingPreparationTools.EffectiveFieldReport(pathingView, globalView,
        ["AutoFightConfig", "PartyName", "AutoEatConfig"], groupFightActive: true));
var entries = report.EnumerateArray().ToArray();
Require(entries[0].GetProperty("source").GetString() == "globalEqual", "synced field must read as globalEqual");
Require(entries[0].TryGetProperty("globalValue", out _), "field report must expose real globalValue");
Require(entries[0].GetProperty("inEffect").GetBoolean(), "fight field with group active must be in effect");
var inactive = System.Text.Json.JsonSerializer.SerializeToElement(
    PathingPreparationTools.EffectiveFieldReport(pathingView, globalView, ["AutoFightConfig"], groupFightActive: false));
Require(!inactive.EnumerateArray().First().GetProperty("inEffect").GetBoolean(),
    "group fight switch off must mark fight field not in effect (global wins)");
Require(entries[1].GetProperty("source").GetString() == "groupOverride", "partyName must read as groupOverride (explicit user setting)");

// ---- DescribeEffectiveStrategy: auto vs file forms through real reflection shapes ----
var typedAutoStrategy = System.Text.Json.JsonSerializer.SerializeToElement(PathingPreparationTools.DescribeEffectiveStrategy(
    new { Config = new { PathingConfig = new { AutoFightConfig = new { StrategyName = "根据队伍自动选择" } } } }, userRoot));
Require(typedAutoStrategy.GetProperty("type").GetString() == "auto", "auto strategy must not be treated as missing txt");
Directory.CreateDirectory(Path.Combine(userRoot, "AutoFight"));
File.WriteAllText(Path.Combine(userRoot, "AutoFight", "万能战斗策略（萌新推荐）.txt"),
    "第1行：通用要求\n第41行：万叶/琴聚集拾取时关闭扫描\n");
var fileStrategy = System.Text.Json.JsonSerializer.SerializeToElement(PathingPreparationTools.DescribeEffectiveStrategy(
    new { Config = new { PathingConfig = new { AutoFightConfig = new { StrategyName = "万能战斗策略（萌新推荐）" } } } }, userRoot));
Require(fileStrategy.GetProperty("type").GetString() == "file:txt", "txt strategy type");
Require(fileStrategy.GetProperty("version").GetString()!.Length == 64, "strategy version is content sha256");
Require(fileStrategy.GetProperty("requirementsExcerpt").GetString()!.Contains("关闭扫描"), "excerpt carries original requirements");

// ---- plan registry: 实际执行队列取消（display 名单与执行列表分离的真枚举） ----
// 镜像宿主 StartGroups：foreach 实际执行列表，第一组运行中在组间边界取消。
var executionQueue = new List<object> { new GroupStub("组A"), new GroupStub("组B") };
var displayNames = new List<string> { "组A", "组B" }; // 与执行列表不同对象，仅显示
var planProgress = new object();
var planCheck = new BgiBridge.Tools.ScriptGroupPlanTools.PlanRegistration
{
    Id = "plan-test", Progress = planProgress, ExecutionQueue = executionQueue,
    DisplayNames = displayNames, GameHandle = 77,
};
var secondGroupRuns = 0;
var closeInvocations = 0;
var loop = Task.Run(async () =>
{
    try
    {
        foreach (var group in executionQueue) // List 枚举器带版本：清空后 MoveNext 抛
        {
            await Task.Delay(50); // “第一组运行中/组间边界”
            if (BgiBridge.Tools.ScriptGroupPlanTools.PlanCancelled(planCheck)) break;
            if (ReferenceEquals(group, executionQueue.Count > 1 ? executionQueue[1] : null)) secondGroupRuns++;
        }
    }
    catch (InvalidOperationException)
    {
        // 宿主 catch/finally Reset 同路径：计划结束
    }
});
await Task.Delay(20); // 保证枚举已开始并进入第一组等待
var runnerShape = new RunnerContextShape { taskProgress = planProgress };
Require(BgiBridge.Tools.ScriptGroupPlanTools.CancelPlanQueue(planCheck, 77, runnerShape), "cancel must clear the actual execution queue via runner taskProgress identity");
var otherRunner = new RunnerContextShape { taskProgress = new object() };
var planTwo = new BgiBridge.Tools.ScriptGroupPlanTools.PlanRegistration
{
    Id = "plan-two", Progress = planProgress, ExecutionQueue = new List<object> { new GroupStub("A"), new GroupStub("B") }, GameHandle = 77,
};
Require(!BgiBridge.Tools.ScriptGroupPlanTools.CancelPlanQueue(planTwo, 77, otherRunner),
    "taskProgress reference mismatch must refuse (stale registration cannot clear a newer plan)");
Require(executionQueue.Count == 0, "execution queue itself must be emptied (not display names)");
Require(BgiBridge.Tools.ScriptGroupPlanTools.PlanCancelled(planCheck), "cancel marker independent of host ManualStop");
await loop;
Require(secondGroupRuns == 0, "second group must not start after queue cancellation");
// close 判定按取消标记：取消后不调用关游戏回调。
if (!BgiBridge.Tools.ScriptGroupPlanTools.PlanCancelled(planCheck)) closeInvocations++;
Require(closeInvocations == 0, "cancelled plan must not invoke closeGame callback");
// 身份不符（游戏实例已切换）拒绝清队列：
var otherQueue = new List<object> { new GroupStub("X") };
var otherPlan = new BgiBridge.Tools.ScriptGroupPlanTools.PlanRegistration
{
    Id = "plan-other", Progress = new object(), ExecutionQueue = otherQueue, GameHandle = 99,
};
Require(!BgiBridge.Tools.ScriptGroupPlanTools.CancelPlanQueue(otherPlan, 77), "identity mismatch must refuse");
Require(otherQueue.Count == 1, "foreign plan queue must stay untouched");
Require(!BgiBridge.Tools.ScriptGroupPlanTools.PlanCancelled(otherPlan), "foreign plan must not be marked cancelled");

// ---- PlanQueue 防过期：句柄 0 拒绝（不 fail-open） ----
var stalePlan = new BgiBridge.Tools.ScriptGroupPlanTools.PlanRegistration
{
    Id = "plan-stale", Progress = new object(),
    ExecutionQueue = new List<object> { new GroupStub("A") }, GameHandle = 0,
};
Require(!BgiBridge.Tools.ScriptGroupPlanTools.CancelPlanQueue(stalePlan, 0),
    "handle 0 must refuse (cannot verify identity, never fail-open)");
Require(!BgiBridge.Tools.ScriptGroupPlanTools.CancelPlanQueue(stalePlan, 55),
    "plan handle 0 must refuse against any current handle");
Require(((List<object>)stalePlan.ExecutionQueue).Count == 1 && !BgiBridge.Tools.ScriptGroupPlanTools.PlanCancelled(stalePlan),
    "refused plan keeps queue and marker untouched");

// ---- 类型默认遮盖识别与对齐决策（生产核心，纯函数） ----
var nativeDefault = JsonNode.Parse("""
{"StrategyName":"根据队伍自动选择","PickDropsAfterFightEnabled":true,"KazuhaPickupEnabled":true,"Timeout":120}
""")!.AsObject();
var globalMature = JsonNode.Parse("""
{"StrategyName":"万能战斗策略（萌新推荐）","PickDropsAfterFightEnabled":false,"KazuhaPickupEnabled":true,"Timeout":60}
""")!.AsObject();
// 默认遮盖（enabled=true 的旧组同样命中）：组=camel 键的默认等值 → 对齐。
var defaultShadow = JsonNode.Parse("""
{"strategyName":"根据队伍自动选择","pickDropsAfterFightEnabled":true,"kazuhaPickupEnabled":true,"timeout":120}
""")!.AsObject();
var decision = BgiBridge.Tools.PathingPreparationTools.FightAlignmentDecision(defaultShadow, globalMature, nativeDefault);
Require(decision.Align, $"type-default shadow must align: {decision.Reason}");
// 显式自定义：值不同 → 保持。
var custom = JsonNode.Parse("""
{"strategyName":"火芙万白","pickDropsAfterFightEnabled":true,"kazuhaPickupEnabled":true,"timeout":120}
""")!.AsObject();
decision = BgiBridge.Tools.PathingPreparationTools.FightAlignmentDecision(custom, globalMature, nativeDefault);
Require(!decision.Align, "explicit custom must stay untouched");
// 未知额外字段：非默认 → 保持（不丢字段）。
var withExtra = JsonNode.Parse("""
{"strategyName":"根据队伍自动选择","pickDropsAfterFightEnabled":true,"kazuhaPickupEnabled":true,"timeout":120,"myCustomExtra":7}
""")!.AsObject();
decision = BgiBridge.Tools.PathingPreparationTools.FightAlignmentDecision(withExtra, globalMature, nativeDefault);
Require(!decision.Align, "unknown extra field means customized; never align/drop");
// 与全局等值 → 无需对齐。
decision = BgiBridge.Tools.PathingPreparationTools.FightAlignmentDecision(globalMature.DeepClone().AsObject(), globalMature, nativeDefault);
Require(!decision.Align, "already equal needs no alignment");
// 空战斗配置（未写）算默认。
decision = BgiBridge.Tools.PathingPreparationTools.FightAlignmentDecision(null, globalMature, nativeDefault);
Require(decision.Align, "absent fight config counts as unconfigured default");

// 对齐后目标等值：经 SyncCore 的 CAS 通道写入 global 克隆并核验（与 SetParty 同一文件机制）。
File.WriteAllText(groupFile, """
{"name":"讨伐组","config":{"pathingConfig":{"enabled":true,"partyName":"旧队","autoFightConfig":{"strategyName":"根据队伍自动选择","pickDropsAfterFightEnabled":true,"kazuhaPickupEnabled":true,"timeout":120}}},"projects":[]}
""");
var alignSha = Sha();
var alignedSync = System.Text.Json.JsonSerializer.SerializeToElement(
    BgiBridge.Tools.PathingPreparationTools.SyncCore(userRoot, "讨伐组", alignSha,
        new Dictionary<string, JsonNode?> { ["AutoFightConfig"] = globalMature.DeepClone() }));
Require(alignedSync.GetProperty("verified").GetBoolean() && alignedSync.GetProperty("replacedFieldsVerified").GetBoolean(),
    "aligned write must read back equal to replacement with precise diff");
var afterAlign = JsonNode.Parse(File.ReadAllText(groupFile))!;
var alignedFight = BgiBridge.Tools.PathingPreparationTools.LookupKey(
    afterAlign["config"]!["pathingConfig"]!.AsObject(), "autoFightConfig") as JsonObject;
Require((BgiBridge.Tools.PathingPreparationTools.LookupKey(alignedFight!, "pickDropsAfterFightEnabled") as JsonValue)!.GetValue<bool>() is false,
    "global scan=false must take effect after alignment (case-normalized read)");
Require(afterAlign["config"]!["pathingConfig"]!["partyName"]!.GetValue<string>() == "旧队",
    "non-target fields (partyName) preserved by alignment channel");

Console.WriteLine("production-core checks passed");

// ---- 真实宿主形态替身（成员在文件底部） ----

internal sealed class RunnerContextShape
{
    public object? taskProgress { get; set; }
}

internal sealed class GroupStub(string name)
{
    public string Name { get; } = name;
}


// SwitchPartyTask 真实形态：Name 只读 getter；唯一 Start(string, CancellationToken)。



// CombatScenes 真实形态：Avatars 私有字段；公共 GetAvatars()。
