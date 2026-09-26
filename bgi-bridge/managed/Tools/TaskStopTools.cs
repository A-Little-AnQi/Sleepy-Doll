using System.Text.Json;
using BgiBridge.Bgi;
using BgiBridge.Catalog;

namespace BgiBridge.Tools;

public static class TaskStopTools
{
    public static void Register(MethodRegistry registry)
    {
        const string id = "bgi.stop_current_task";
        var guide = new AgentGuide("停止当前任务", "向 BetterGI 当前独立任务的取消上下文发送停止请求，并检查宿主任务锁是否释放。",
            ["用户要求停止当前执行的配置组、一条龙、路线或独立任务；没有桥 Job ID 也可调用。"],
            ["桥已连接；不要求游戏主界面、截图器或 16:9。"], ["取消当前宿主独立任务，不删除配置与路线。"],
            "stopped=true 表示任务锁已释放；stopped=false、outcome=timeout 表示仍在停止，不能报告已经停止。",
            "有桥 Job 时继续跟踪原 Job；只有请求发出或 cancellationRequested 不能证明停止。", "已执行的游戏输入不回退，配置保持不变。",
            [JsonSerializer.SerializeToElement(new { })], "bridge-stable-operation");
        registry.Register(id, "lifecycle", guide.Purpose, Stop, readOnly: false, guide: guide);
    }

    private static async Task<object?> Stop(JsonElement arguments, CancellationToken cancellation)
    {
        if (Host.TaskSemaphoreCount() is > 0) return new { stopped = true, outcome = "alreadyIdle" };
        await Ui.InvokeAsync(HostTaskCancellation.RequestStop).ConfigureAwait(false);
        var deadline = DateTimeOffset.UtcNow.AddSeconds(10);
        while (DateTimeOffset.UtcNow < deadline)
        {
            cancellation.ThrowIfCancellationRequested();
            if (Host.TaskSemaphoreCount() is > 0) return new { stopped = true, outcome = "stopped" };
            await Task.Delay(100, cancellation).ConfigureAwait(false);
        }
        return new { stopped = false, outcome = "timeout" };
    }
}
