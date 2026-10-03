// Static acceptance checks for the readiness state machine and the party/hurry merge.
// ASCII-only comments; run via ReadinessPartyChecks.csproj.
using System.Text.Json.Nodes;
using BgiBridge.Catalog;
using BgiBridge.Tools;
using S = BgiBridge.Tools.StatusTools.ReadinessStep;

static void Require(bool value, string message) { if (!value) throw new Exception(message); }
var limit = TimeSpan.FromSeconds(120);

// ---- readiness state machine: all observable states classify to the right step ----
Require(StatusTools.ClassifyReadiness(false, false, false, false, false, TimeSpan.Zero, limit) == S.AttachCapturer,
    "no process and no capturer must attach the capturer (host command launches)");
Require(StatusTools.ClassifyReadiness(false, true, false, false, false, TimeSpan.Zero, limit) == S.AttachCapturer,
    "game process running without capturer must still attach the capturer, never idle-wait");
Require(StatusTools.ClassifyReadiness(true, true, false, false, false, TimeSpan.FromSeconds(3), limit) == S.WaitForMainUi,
    "capturer ready while loading must wait for the main UI");
Require(StatusTools.ClassifyReadiness(true, true, true, false, false, TimeSpan.Zero, limit) == S.Ready,
    "capturer ready and in main UI is ready");
Require(StatusTools.ClassifyReadiness(true, true, true, true, false, TimeSpan.Zero, limit) == S.FailResolution,
    "non-16:9 window must fail with the resolution branch even in main UI");
Require(StatusTools.ClassifyReadiness(false, false, false, false, false, TimeSpan.FromSeconds(6), limit) == S.FailNotRunning,
    "no process after the grace window must fail as not running, not loop");
Require(StatusTools.ClassifyReadiness(true, true, false, false, false, limit + TimeSpan.FromSeconds(1), limit) == S.FailTimeout,
    "loading past the limit must fail as timeout");
Require(StatusTools.ClassifyReadiness(false, true, false, false, false, limit + TimeSpan.FromSeconds(1), limit) == S.FailTimeout,
    "capturer never attaching past the limit must fail as timeout");

// ---- hurry avatar resolution: data mapping against the supported list ----
var supported = new[] { "", "自动", "玛薇卡", "闲云", "恰斯卡", "流浪者" };
Require(PathingPreparationTools.ResolveHurryAvatar("玛薇卡", supported) == "玛薇卡", "canonical names pass through");
Require(PathingPreparationTools.ResolveHurryAvatar("火神", supported) == "玛薇卡", "alias maps to canonical");
Require(PathingPreparationTools.ResolveHurryAvatar("自动", supported) == "自动", "auto passes through");
Require(PathingPreparationTools.ResolveHurryAvatar("", supported) == "", "empty keeps no-skill rushing");
Require(PathingPreparationTools.ResolveHurryAvatar("不赶路", supported) == "", "explicit no-rush alias maps to empty");
try { PathingPreparationTools.ResolveHurryAvatar("不存在角色", supported); throw new Exception("unknown alias was accepted"); }
catch (BgiBridge.Protocol.BridgeException error) { Require(error.Message.Contains("玛薇卡"), "rejection must list supported names"); }
try { PathingPreparationTools.ResolveHurryAvatar("火神", new[] { "", "自动", "闲云" }); throw new Exception("alias to unsupported host option was accepted"); }
catch (BgiBridge.Protocol.BridgeException) { }

// ---- party merge: only target fields change, everything else preserved ----
var pathing = JsonNode.Parse("""
{"enabled":false,"partyName":"旧队","hurryOnAvatar":"","autoFightConfig":{"name":"默认","enabled":false},"collectTimeout":20}
""")!.AsObject();
PathingPreparationTools.MergeParty(pathing, "新队", "玛薇卡");
Require(pathing["partyName"]!.GetValue<string>() == "新队", "partyName updated");
Require(pathing["enabled"]!.GetValue<bool>(), "partyName write must force enabled=true");
Require(pathing["hurryOnAvatar"]!.GetValue<string>() == "玛薇卡", "hurryOnAvatar updated");
Require(pathing["autoFightConfig"]!["name"]!.GetValue<string>() == "默认", "autoFightConfig preserved");
Require(pathing["autoFightConfig"]!["enabled"]!.GetValue<bool>() is false, "autoFightConfig enabled preserved");
Require(pathing["collectTimeout"]!.GetValue<int>() == 20, "collectTimeout preserved");
PathingPreparationTools.MergeParty(pathing, null, "");
Require(pathing["hurryOnAvatar"]!.GetValue<string>() == "", "empty hurry clears skill rushing");
Require(pathing["partyName"]!.GetValue<string>() == "新队" && pathing["enabled"]!.GetValue<bool>(), "hurry-only write leaves party untouched");

// ---- optimistic-concurrency write-back verification: real diff, not hardcoded true ----
var beforeDoc = JsonNode.Parse("""
{"name":"挖矿讨伐","config":{"pathingConfig":{"enabled":false,"partyName":"旧队","hurryOnAvatar":"","autoFightConfig":{"name":"默认","enabled":false}}},"projects":[{"name":"路线A"}]}
""")!;
var afterDoc = beforeDoc.DeepClone();
PathingPreparationTools.MergeParty(afterDoc["config"]!["pathingConfig"]!.AsObject(), "新队", "玛薇卡");
var (preserved, changed) = PathingPreparationTools.VerifyMerge(beforeDoc, afterDoc, true, true);
Require(preserved, "clean merge must verify other fields preserved");
Require(changed.Length == 3 && changed.Contains("config.pathingConfig.hurryOnAvatar"), "changed fields list the exact targets");
// 外部改动：hurry 之外的 collectTimeout 被动过 -> 真实 diff 必须抓到。
var tampered = beforeDoc.DeepClone();
PathingPreparationTools.MergeParty(tampered["config"]!["pathingConfig"]!.AsObject(), "新队", null);
tampered["config"]!["pathingConfig"]!["collectTimeout"] = 999;
var (tamperedPreserved, _) = PathingPreparationTools.VerifyMerge(beforeDoc, tampered, true, false);
Require(!tamperedPreserved, "an unrelated field change must not be reported as preserved");
// hurry-only 变更不再剥离战斗字段：篡改 autoFightConfig 必须被发现。
var hurryOnly = beforeDoc.DeepClone();
PathingPreparationTools.MergeParty(hurryOnly["config"]!["pathingConfig"]!.AsObject(), null, "玛薇卡");
hurryOnly["config"]!["pathingConfig"]!["autoFightConfig"]!["timeout"] = 999;
var (hurryTampered, _) = PathingPreparationTools.VerifyMerge(beforeDoc, hurryOnly, false, true);
Require(!hurryTampered, "hurry-only write must still expose battle-field tampering");
// fightAligned=true 才剥离战斗字段（对齐场景）。
var alignedCase = beforeDoc.DeepClone();
PathingPreparationTools.MergeParty(alignedCase["config"]!["pathingConfig"]!.AsObject(), null, "玛薇卡");
alignedCase["config"]!["pathingConfig"]!["autoFightConfig"]!["timeout"] = 60;
var (alignedOk, _) = PathingPreparationTools.VerifyMerge(beforeDoc, alignedCase, false, true, fightAligned: true);
Require(alignedOk, "declared fight alignment strips only the fight field");

// ---- contract surface ----
var input = AgentSchemas.Input("bgi.set_pathing_party");
Require(input.GetProperty("required").EnumerateArray().Select(v => v.GetString()).OrderBy(s => s).SequenceEqual(["expectedSha256", "groupName"]), "groupName and expectedSha256 are required");
Require(input.GetProperty("properties").GetProperty("hurryAvatar").GetProperty("type").GetString() == "string", "hurryAvatar is a plain string slot");
// ---- ensure_game_ready result shape through the REAL job completion semantics ----
// BridgeHost 只在 evidence.verified==true 时 MarkCompleted(verified:true)；
// 用真实 handler 的返回体构造（ReadinessResult）过 JobStore，不假造 verifiedSucceeded 标签。
var readyDetail = new { captureReady = true, inMainUi = true };
var notReadyDetail = new { captureReady = true, inMainUi = false };
foreach (var (readyCase, detail, mustSucceed) in new[] {
    (true, readyDetail, true),   // 实测就绪：Job 核验成功，RootBridge 才放行
    (false, notReadyDetail, false), // 未就绪：核验不得成功（防御分支，handler 正常会抛错）
})
{
    var result = System.Text.Json.JsonSerializer.SerializeToElement(
        StatusTools.ReadinessResult(readyCase, detail, "note"));
    // 与 BridgeHost.InvokeAsync 相同的提取逻辑。
    var verified = result.TryGetProperty("verified", out var flag) && flag.ValueKind == System.Text.Json.JsonValueKind.True;
    var reason = result.TryGetProperty("verificationReason", out var explanation) && explanation.ValueKind == System.Text.Json.JsonValueKind.String
        ? explanation.GetString() : null;
    var jobs = new BgiBridge.Jobs.JobStore();
    var job = jobs.Create("bgi.ensure_game_ready");
    jobs.MarkRunning(job.JobId);
    jobs.MarkCompleted(job.JobId, result, verified, reason);
    var snapshot = jobs.Get(job.JobId)!;
    Require(snapshot.State == BgiBridge.Jobs.JobState.Completed, "readiness job must complete");
    Require(
        (snapshot.VerificationStatus == "succeeded") == mustSucceed,
        $"verified={verified} must map to verification {(mustSucceed ? "succeeded" : "not succeeded")} (got {snapshot.VerificationStatus})");
}

Console.WriteLine("readiness+party checks passed");
