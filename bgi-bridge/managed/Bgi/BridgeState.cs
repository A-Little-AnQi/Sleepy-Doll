namespace BgiBridge.Bgi;

/// <summary>状态快照与停止请求。只做被动观测，不得改变游戏界面。</summary>
public static class BridgeState
{
    /// <summary>取一次状态快照。任何一项取不到都不抛。</summary>
    public static (bool Ready, object Detail) Capture()
    {
        var captureReady = false;
        var handle = 0L;
        int? semaphore = null;
        var hasProject = false;
        var hostLoaded = Host.HostLoaded;

        try
        {
            captureReady = Host.CaptureReady;
            handle = Host.GameHandle;
            semaphore = Host.TaskSemaphoreCount();
            hasProject = Host.CurrentScriptProject is not null;
        }
        catch
        {
            // 宿主可能正处于关闭过程中。
        }

        // 截图器没起来时 GameHandle 恒为 0，如实报出去。
        var windowActive = false;
        if (captureReady && handle != 0)
        {
            try
            {
                var systemControl = Reflect.FindType("BetterGenshinImpact.GameTask.SystemControl");
                if (systemControl is not null)
                    windowActive = Reflect.CallStatic(systemControl, "IsGenshinImpactActive") is true;
            }
            catch
            {
                // 同上。
            }
        }

        var detail = new
        {
            hostLoaded,
            captureReady,
            gameHandle = handle,
            windowActive,
            // CurrentCount == 0 表示有独立任务持锁。
            taskLockHeld = semaphore is 0,
            activeJobId = hasProject ? "script-project" : null,
            ui = new
            {
                value = "unknown",
                status = captureReady ? "notSampled" : "unavailable",
                reason = captureReady
                    ? "桥不识别界面内容，当前没有可读取界面类别的接口。"
                    : "截图器未启动。",
            },
            position = new
            {
                value = (object?)null,
                status = "unknown",
                reason = "桥不读画面，当前没有可读取游戏内定位的接口。",
            },
        };

        return (hostLoaded && captureReady, detail);
    }

    /// <summary>解除暂停 → 手动取消 → 释放模拟键。顺序不能调换。</summary>
    public static bool RequestCancel()
    {
        var requested = false;

        try
        {
            var runner = Reflect.Singleton("BetterGenshinImpact.GameTask.RunnerContext");
            if (runner is not null)
            {
                // 先解除协作式暂停：取消信号要到下一个安全检查点才生效。
                runner.GetType().GetProperty("IsSuspend")?.SetValue(runner, false);
                requested = true;
            }
        }
        catch
        {
            // 单例拿不到就跳过这一步。
        }

        try
        {
            var cancellation = Reflect.Singleton("BetterGenshinImpact.Core.Script.CancellationContext");
            if (cancellation is not null)
            {
                Reflect.Call(cancellation, "ManualCancel");
                requested = true;
            }
        }
        catch
        {
            // 同上。
        }

        try
        {
            var simulation = Reflect.FindType("BetterGenshinImpact.Core.Simulator.Simulation");
            if (simulation is not null) Reflect.CallStatic(simulation, "ReleaseAllKey");
        }
        catch
        {
            // 同上。
        }

        return requested;
    }
}
