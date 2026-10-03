using System.Text.Json;
using BgiBridge.Bgi;
using BgiBridge.Catalog;

namespace BgiBridge.Tools;

public static class TaskStopTools
{
    public static void Register(MethodRegistry registry)
    {
        const string id = "bgi.stop_current_task";
        var guide = new AgentGuide("停止当前任务", "向 BetterGI 当前独立任务的取消上下文发送停止请求，并以宿主任务锁是否释放为权威核验。",
            ["用户要求停止当前执行的配置组、一条龙、路线或独立任务；没有桥 Job ID 也可调用。"],
            ["桥已连接；不要求游戏主界面、截图器或 16:9。"], ["取消当前宿主独立任务，不删除配置与路线。"],
            "stopped=true 且 verified=true 表示已真实观察到任务锁释放（alreadyIdle 为调用前已空闲）；stopped=false、outcome=timeout 表示停止请求已发出但未观察到锁释放，是未核验的 unknown，不能报告已经停止。",
            "verified 只在观察到宿主任务锁空闲时为 true；有桥 Job 时继续跟踪原 Job，请求发出或 cancellationRequested 不能证明停止。", "已执行的游戏输入不回退，配置保持不变。",
            [JsonSerializer.SerializeToElement(new { })], "bridge-stable-operation");
        registry.Register(id, "lifecycle", guide.Purpose, Stop, readOnly: false, guide: guide);
    }

    private static async Task<object?> Stop(JsonElement arguments, CancellationToken cancellation)
    {
        // 核验以任务锁 + 连续运行状态为权威证据：StartGroups 组间间隙任务锁
        // 已释放但 IsContinuousRunGroup 仍为 true、后续组还会继续，不算停止。
        // verified 只在两者都真实观察到空闲时为 true。
        if (Host.TaskSemaphoreCount() is > 0 && ScriptGroupPlanTools.ContinuousRunIdle())
            return new
            {
                stopped = true,
                outcome = "alreadyIdle",
                verified = true,
                verificationScope = "taskLockAndPlan",
                verificationReason = "调用前已观察到宿主任务锁空闲且无连续运行计划；没有正在运行的独立任务。",
            };
        // 桥持有统一进度计划时按计划取消：标记取消并清空进度对象上的剩余组
        // 队列（宿主 foreach 在当前组结束后自然退出，后续组不启动），当前组只
        // 发一次停止请求；不反复打全局 ManualCancel（RunMulti 每组会重置它，
        // 重复全局取消可能波及别的新任务）。
        var plan = ScriptGroupPlanTools.SingleActivePlan();
        var queueCleared = false;
        var currentCancelIssued = false;
        if (plan is not null)
        {
            // UI 线程上先核对本计划的游戏身份，再清实际执行列表；只有当前确有
            // 组在跑（任务锁被占）才发一次当前 scope 的停止请求——空档期不发，
            // 不误伤已切换的新任务/新计划。
            await Ui.InvokeAsync(() =>
            {
                queueCleared = ScriptGroupPlanTools.CancelPlanQueue(plan!, Host.GameHandle);
                if (queueCleared && Host.TaskSemaphoreCount() == 0)
                {
                    HostTaskCancellation.RequestStop();
                    currentCancelIssued = true;
                }
            }).ConfigureAwait(false);
        }
        else
        {
            await Ui.InvokeAsync(HostTaskCancellation.RequestStop).ConfigureAwait(false);
            currentCancelIssued = true;
        }
        var deadline = DateTimeOffset.UtcNow.AddSeconds(10);
        while (DateTimeOffset.UtcNow < deadline)
        {
            cancellation.ThrowIfCancellationRequested();
            if (Host.TaskSemaphoreCount() is > 0 && ScriptGroupPlanTools.ContinuousRunIdle())
                return new
                {
                    stopped = true,
                    outcome = "stopped",
                    verified = true,
                    verificationScope = "taskLockAndPlan",
                    verificationReason = plan is null
                        ? "停止请求发出后观察到宿主任务锁已释放且连续运行计划已退出。"
                        : $"计划级取消（执行队列已清空={queueCleared}，当前组停止请求={(currentCancelIssued ? "已发一次" : "空档未发")}）后观察到任务锁已释放且连续运行计划已退出。",
                };
            await Task.Delay(100, cancellation).ConfigureAwait(false);
        }
        // 超时不能记核验成功：可能仍在组间过渡或状态不可读，保持真实 unknown。
        return new
        {
            stopped = false,
            outcome = "timeout",
            verified = false,
            verificationScope = "taskLockAndPlan",
            verificationReason = "停止请求已发出，但 10 秒内未同时观察到任务锁释放与连续运行计划退出；不得报告已停止。",
        };
    }
}
