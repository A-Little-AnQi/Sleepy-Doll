using System.Text.Json;
using BgiBridge.Tools;
using BgiBridge.Jobs;
using BgiBridge.Catalog;

static void Require(bool value, string message) { if (!value) throw new Exception(message); }
var native = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
using var caller = new CancellationTokenSource();
var handedOff = await ScriptLaunch.Finish(native.Task, "existing-group", false, caller.Token).WaitAsync(TimeSpan.FromSeconds(1));
var evidence = JsonSerializer.SerializeToElement(handedOff);
Require(!native.Task.IsCompleted, "Default launch waited for the long-running native task");
Require(evidence.GetProperty("accepted").GetBoolean() && !evidence.GetProperty("executed").GetBoolean(), "Launch was reported as business completion");
Require(evidence.GetProperty("verificationScope").GetString() == "launch", "Incorrect evidence scope");
var jobs = new JobStore();var job = jobs.Create("bgi.run_script_group");jobs.MarkRunning(job.JobId);
jobs.MarkCompleted(job.JobId, handedOff, true, evidence.GetProperty("verificationReason").GetString());
Require(jobs.Get(job.JobId)!.State==JobState.Completed&&!native.Task.IsCompleted, "Launch job guards the native lifetime");
caller.Cancel();Require(!native.Task.IsCanceled, "A completed launch retained its caller's cancellation");native.SetResult();

var supervised = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
var waiting = ScriptLaunch.Finish(supervised.Task,"explicit-wait",true,CancellationToken.None);
await Task.Delay(40);Require(!waiting.IsCompleted,"Explicit completion mode failed to wait");
supervised.SetResult();var completed=JsonSerializer.SerializeToElement(await waiting.WaitAsync(TimeSpan.FromSeconds(1)));
Require(completed.GetProperty("executed").GetBoolean()&&!completed.GetProperty("verified").GetBoolean(),"Execution return was claimed as verified business success");

try { await ScriptLaunch.Finish(Task.FromException(new InvalidOperationException("immediate failure")),"failed",false,CancellationToken.None);throw new Exception("Dispatch failure was swallowed"); }
catch(InvalidOperationException error) when(error.Message=="immediate failure"){}
var cancelled = new TaskCompletionSource();using(var stop=new CancellationTokenSource()) {
    var cancelWait=ScriptLaunch.Finish(cancelled.Task,"cancel-wait",true,stop.Token);stop.Cancel();
    try { await cancelWait;throw new Exception("Wait did not cancel"); }catch(OperationCanceledException){}
    cancelled.TrySetResult();
}
var schema=AgentSchemas.Input("bgi.run_script_group");
Require(schema.GetProperty("properties").GetProperty("waitForCompletion").GetProperty("default").ValueKind==JsonValueKind.False,"Launch must be the default contract");

// ---- bgi.run_script_groups 计划工具行为验收 ----
var groupNames = AgentSchemas.Input("bgi.run_script_groups").GetProperty("properties").GetProperty("groupNames");
Require(groupNames.TryGetProperty("uniqueItems", out _) is false, "groupNames must allow deliberate repeats");
Require(AgentSchemas.Input("bgi.run_script_groups").GetProperty("properties").GetProperty("closeGameAfter").GetProperty("default").ValueKind==JsonValueKind.False, "closeGameAfter must default false");

// 纯进度判定：成功、失败优先、缺证不猜。
ScriptGroupPlanTools.PlanVerdict V(bool ok,string s,string r)=>new(ok,s,r);
ScriptGroupPlanTools.PlanSnapshot Snap(bool end,bool stopKnown,bool stop,bool curIncomplete,params (int? s,bool? t,bool e)[] h)=>
    new(end,stopKnown,stop,curIncomplete,h.Select(x=>new ScriptGroupPlanTools.PlanHistoryEntry(x.s,x.t,x.e)).ToArray());
Require(ScriptGroupPlanTools.EvaluatePlan(Snap(true,true,false,false,(1,true,true),(1,true,true))) is { Succeeded: true }, "All-success plan must be succeeded");
Require(ScriptGroupPlanTools.EvaluatePlan(Snap(true,true,false,false,(2,true,true),(1,true,true))).Status=="failed", "Failure followed by success must stay failed");
Require(ScriptGroupPlanTools.EvaluatePlan(Snap(false,true,false,false,(1,true,true))).Status=="unknown", "Missing overall EndTime must be unknown");
Require(ScriptGroupPlanTools.EvaluatePlan(Snap(true,true,false,false)).Status=="unknown", "Empty history must be unknown");
Require(ScriptGroupPlanTools.EvaluatePlan(Snap(true,true,false,false,(3,true,true))).Status=="unknown", "Unknown Status=3 must not be guessed");
Require(ScriptGroupPlanTools.EvaluatePlan(Snap(true,true,false,false,(null,true,true))).Status=="unknown", "Missing Status must be unknown");
Require(ScriptGroupPlanTools.EvaluatePlan(Snap(true,true,false,false,(1,false,true))).Status=="unknown", "TaskEnd=false must be unknown");
Require(ScriptGroupPlanTools.EvaluatePlan(Snap(true,true,false,true,(1,true,true))).Status=="unknown", "Incomplete current project must be unknown");
Require(ScriptGroupPlanTools.EvaluatePlan(Snap(true,true,true,false,(1,true,true))).Status=="failed", "Manual cancel must be failed");
Require(ScriptGroupPlanTools.EvaluatePlan(Snap(true,true,false,false,(1,true,true))).Status=="succeeded", "Success must not depend on call order");

// 保序组合：fake group 引用与字段原样保留，不碰真实用户配置。
var cfg=new object(); var info=new object(); var settings=new object();
var g1=new FakeGroup{Name="a",Config=cfg,GroupInfo=info,RunNum=7,Settings=settings};
var g2=new FakeGroup{Name="b",Config=new object(),GroupInfo=new object(),RunNum=3,Settings=new object()};
var merged=ScriptGroupPlanTools.CombineGroups(new System.Collections.IList[]{new List<FakeGroup>{g1},new List<FakeGroup>{g2}});
Require(merged.Count==2&&ReferenceEquals(merged[0],g1)&&ReferenceEquals(merged[1],g2),"CombineGroups must preserve order and references");
Require(ReferenceEquals(g1.Config,cfg)&&ReferenceEquals(g1.GroupInfo,info)&&ReferenceEquals(g1.Settings,settings)&&g1.RunNum==7,"CombineGroups must not mutate groups");

// 默认交接在原生 Task 完成前返回；source token 在交接后不被持有。
var planTask=new TaskCompletionSource(TaskCompletionSourceCreationOptions());
using var planCaller=new CancellationTokenSource();
var followDone=new TaskCompletionSource(TaskCompletionSourceCreationOptions());
var planHandoff=await ScriptGroupPlanTools.Handoff(planTask.Task,
    t=>_=ScriptGroupPlanTools.FollowUpAsync(t,new FakeProgress(),false,0,_=>Task.FromResult(new ScriptGroupPlanTools.CloseResult(true,"unused"))).ContinueWith(_=>followDone.SetResult()),
    new List<string>{"a","b"},false).WaitAsync(TimeSpan.FromSeconds(1));
var handoffJson=JsonSerializer.SerializeToElement(planHandoff);
Require(!planTask.Task.IsCompleted,"Plan handoff waited for native completion");
Require(handoffJson.GetProperty("accepted").GetBoolean()&&handoffJson.GetProperty("executionMode").GetString()=="launch"&&handoffJson.GetProperty("verified").GetBoolean(),"Launch handoff evidence wrong");
Require(handoffJson.GetProperty("verificationScope").GetString()=="launch"&&handoffJson.GetProperty("verification").GetProperty("status").GetString()=="succeeded","Launch verification must be launch scope succeeded");
Require(handoffJson.GetProperty("verification").GetProperty("status").GetString()=="succeeded"&&!handoffJson.GetProperty("executed").GetBoolean(),"Launch must not claim execution");
planCaller.Cancel();
planTask.SetResult();
await followDone.Task.WaitAsync(TimeSpan.FromSeconds(1));
Require(!planTask.Task.IsCanceled,"Follow-up must not retain caller's cancellation token");

// 收尾决策（真实 SettleAsync 路径）：成功只关一次且保持 succeeded；失败/unknown 零关闭调用；关闭未核验整体降级 unknown。
var closeCalls=0;
var okSettle=await ScriptGroupPlanTools.SettleAsync(Task.CompletedTask,()=>V(true,"succeeded","调度全部成功。"),true,42,
    _=>{closeCalls++;return Task.FromResult(new ScriptGroupPlanTools.CloseResult(true,"目标进程已退出，关闭已核验。"));});
Require(closeCalls==1&&okSettle.Verdict.Succeeded&&okSettle.Verdict.Status=="succeeded"&&okSettle.Close!.Verified,"Success must close exactly once and stay succeeded");
var failCalls=0;
var failSettle=await ScriptGroupPlanTools.SettleAsync(Task.CompletedTask,()=>V(false,"failed","存在失败"),true,42,
    _=>{failCalls++;return Task.FromResult(new ScriptGroupPlanTools.CloseResult(true,"x"));});
Require(failCalls==0&&!failSettle.Verdict.Succeeded&&failSettle.Verdict.Status=="failed"&&failSettle.Close!.Outcome.Contains("存在失败"),"Failure must not close and must carry reason");
var unknownCalls=0;
var unknownSettle=await ScriptGroupPlanTools.SettleAsync(Task.CompletedTask,()=>V(false,"unknown","缺证"),true,42,
    _=>{unknownCalls++;return Task.FromResult(new ScriptGroupPlanTools.CloseResult(true,"x"));});
Require(unknownCalls==0&&!unknownSettle.Verdict.Succeeded&&unknownSettle.Verdict.Status=="unknown","Unknown verdict must not close");
var unverifiedClose=await ScriptGroupPlanTools.SettleAsync(Task.CompletedTask,()=>V(true,"succeeded","调度全部成功。"),true,42,
    _=>Task.FromResult(new ScriptGroupPlanTools.CloseResult(false,"未关闭：目标进程仍在运行")));
Require(!unverifiedClose.Verdict.Succeeded&&unverifiedClose.Verdict.Status=="unknown"&&unverifiedClose.Verdict.Reason.Contains("目标进程仍在运行")&&unverifiedClose.Close!.Outcome.Contains("目标进程仍在运行"),"Unverified close must downgrade the overall verdict to unknown");

// Settle 在原生 Task 完成前不得返回；取消等待后不得调用关闭回调。
var settleGate=new TaskCompletionSource(TaskCompletionSourceCreationOptions());
var pending=ScriptGroupPlanTools.SettleAsync(settleGate.Task,()=>V(true,"succeeded","ok"),true,7,
    _=>Task.FromResult(new ScriptGroupPlanTools.CloseResult(true,"closed")));
await Task.Delay(40);Require(!pending.IsCompleted,"Settle returned before native completion");
settleGate.SetResult();
Require((await pending.WaitAsync(TimeSpan.FromSeconds(1))).Verdict.Succeeded,"Settle must judge after native completion");
var cancelGate=new TaskCompletionSource(TaskCompletionSourceCreationOptions());
var closeOnCancel=0;
using(var stop=new CancellationTokenSource()){
    var cancelSettle=ScriptGroupPlanTools.SettleAsync(cancelGate.Task,()=>V(true,"succeeded","ok"),true,7,
        _=>{closeOnCancel++;return Task.FromResult(new ScriptGroupPlanTools.CloseResult(true,"x"));},stop.Token);
    stop.Cancel();
    try{await cancelSettle;throw new Exception("Settle did not honor wait cancellation");}catch(OperationCanceledException){}
}
Require(closeOnCancel==0,"Cancelled wait must never call the close callback");
cancelGate.SetResult();

// 真实 SnapshotFromProgress 路径：当前项目已完整收尾 → succeeded；未完整 → 拒绝。
var doneEntry=new FakeHistoryEntry{Status=1,TaskEnd=true,EndTime=new DateTime(2026,1,1,0,0,0,DateTimeKind.Utc)};
var doneProgress=new FakeProgress{EndTime=new DateTime(2026,1,1,1,0,0,DateTimeKind.Utc),History=new object[]{doneEntry},CurrentScriptGroupProjectInfo=doneEntry};
var doneSnap=ScriptGroupPlanTools.SnapshotFromProgress(doneProgress,false);
Require(doneSnap.ManualStopKnown&&!doneSnap.CurrentProjectIncomplete&&ScriptGroupPlanTools.EvaluatePlan(doneSnap) is { Succeeded:true, Status:"succeeded" },"Completed current project must yield a real succeeded snapshot");
var unfinished=new FakeHistoryEntry{Status=0,TaskEnd=false,EndTime=null};
var unfinishedProgress=new FakeProgress{EndTime=new DateTime(2026,1,1,1,0,0,DateTimeKind.Utc),History=new object[]{unfinished},CurrentScriptGroupProjectInfo=unfinished};
Require(!ScriptGroupPlanTools.EvaluatePlan(ScriptGroupPlanTools.SnapshotFromProgress(unfinishedProgress,false)).Succeeded,"Unfinished current project must be rejected on the real snapshot path");

// Reflect.Set 必须显式落 Loop=false（用 fake progress 验证写入路径）。
var loopFake=new FakeProgress();
BgiBridge.Bgi.Reflect.Set(loopFake,"Loop",false);
Require(loopFake.Loop==false,"Reflect.Set must write Loop explicitly");

Console.WriteLine(JsonSerializer.Serialize(new {defaultLaunchReturnsBeforeNativeCompletion=true,explicitWaitRetained=true,launchJobDoesNotGuardLifetime=true,launchEvidenceNotBusinessSuccess=true,immediateFailureSurfaced=true,waitCancellationWorks=true,planVerdictMatrix=true,orderedCombinePreservesGroups=true,handoffReturnsBeforeNative=true,handoffDoesNotHoldCallerToken=true,closeOnlyOnSuccessOnce=true,failUnknownNeverClose=true,unverifiedCloseDowngradesOverall=true,settleWaitsBeforeReturn=true,waitCancelNeverClose=true,realSnapshotCompletedAndUnfinishedCurrent=true}));
static System.Threading.Tasks.TaskCreationOptions TaskCompletionSourceCreationOptions()=>System.Threading.Tasks.TaskCreationOptions.RunContinuationsAsynchronously;

internal sealed class FakeGroup
{
    public string Name{get;set;}="";
    public object? Config{get;set;}
    public object? GroupInfo{get;set;}
    public int RunNum{get;set;}
    public object? Settings{get;set;}
}

internal sealed class FakeHistoryEntry
{
    public int? Status{get;set;}
    public bool? TaskEnd{get;set;}
    public DateTime? EndTime{get;set;}
}

internal sealed class FakeProgress
{
    public bool Loop{get;set;}=true;
    public List<string>? ScriptGroupNames{get;set;}
    public object? EndTime{get;set;}
    public System.Collections.IEnumerable? History{get;set;}
    public object? CurrentScriptGroupProjectInfo{get;set;}
}
