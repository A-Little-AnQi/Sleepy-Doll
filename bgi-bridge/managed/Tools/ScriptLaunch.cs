using System.Diagnostics;

namespace BgiBridge.Tools;

/// <summary>普通运行请求只交接给宿主，显式等待才绑定整个执行生命周期。</summary>
public static class ScriptLaunch
{
    public static async Task<object> Finish(Task execution, string groupName, bool waitForCompletion, CancellationToken cancellation)
    {
        cancellation.ThrowIfCancellationRequested();
        if (waitForCompletion)
            await execution.WaitAsync(cancellation).ConfigureAwait(false);
        else if (execution.IsCompleted)
            await execution.ConfigureAwait(false); // Surface immediate dispatch failures.
        else
            _ = execution.ContinueWith(task => Trace.TraceError("配置组 {0} 后台执行异常：{1}", groupName, task.Exception),
                CancellationToken.None, TaskContinuationOptions.OnlyOnFaulted | TaskContinuationOptions.ExecuteSynchronously, TaskScheduler.Default);
        return new
        {
            groupName,
            resolved = true,
            accepted = true,
            executed = execution.IsCompletedSuccessfully,
            executionMode = waitForCompletion ? "completion" : "launch",
            verified = !waitForCompletion,
            verificationScope = waitForCompletion ? "executionReturn" : "launch",
            verificationReason = "配置组已唯一定位，宿主已接受本次运行交接；未核验脚本业务完成。",
        };
    }
}
