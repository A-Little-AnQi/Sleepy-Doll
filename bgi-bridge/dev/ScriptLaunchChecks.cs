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
Console.WriteLine(JsonSerializer.Serialize(new {defaultLaunchReturnsBeforeNativeCompletion=true,explicitWaitRetained=true,launchJobDoesNotGuardLifetime=true,launchEvidenceNotBusinessSuccess=true,immediateFailureSurfaced=true,waitCancellationWorks=true,realGameActions=0}));
