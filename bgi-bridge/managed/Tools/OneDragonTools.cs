using System.Diagnostics;
using System.Text.Json;
using BgiBridge.Bgi;
using BgiBridge.Catalog;
using BgiBridge.Protocol;

namespace BgiBridge.Tools;

/// <summary>BetterGI 的「一条龙」与退出游戏：宿主自己的一键流程，不经过脚本配置组。</summary>
public static class OneDragonTools
{
    private const string ViewModelType =
        "BetterGenshinImpact.ViewModel.Pages.OneDragonFlowViewModel";
    private const string SystemControlType =
        "BetterGenshinImpact.GameTask.SystemControl";

    public static void Register(MethodRegistry registry)
    {
        RegisterOneDragon(registry);
        RegisterExitGame(registry);
    }

    private static void RegisterOneDragon(MethodRegistry registry)
    {
        const string id = "bgi.run_one_dragon";
        var guide = new AgentGuide(
            "运行一条龙",
            "运行 BetterGI 的「一条龙」日常流程（领邮件、合成树脂、自动秘境、首领讨伐、幽境危战、地脉花、每日奖励、尘歌壶等，按该配置启用的任务执行）。执行完按配置可能自动退出游戏。",
            ["用户要求做每日/一条龙/清体力，且 BetterGI 的 User/OneDragon 里已有对应配置时。", "与 run_script_group 的区别：一条龙是宿主原生任务链，带自己的等待、领奖与收尾逻辑。"],
            ["游戏未就绪时接口会按真实状态自动准备：启动游戏或挂上截图器并阻塞等待至主界面（至多 120 秒）；非 16:9 或超时如实失败。", "窗口分辨率为 16:9。", "没有其他独立任务持锁。"],
            ["按所选一条龙配置串行执行启用的日常任务；过程中会自动传送、领取与点击游戏界面；配置开启时收尾自动退出游戏。"],
            "默认 waitForCompletion=false：accepted=true 表示宿主入口已派发且任务锁已被占用，交接后立即返回，不守护整条一条龙。只有用户明确要求等结果才设 true（等待宿主方法返回并核验任务锁释放）。",
            "普通运行请求用返回证据给一次最终总结并结束。不继续 job.get、查任务状态、日志或截图，不重跑；仅用户明确要求监控等结果才继续，查看当前状态与排障只是当前请求的一次读取。",
            "一条龙只做日常任务范围内的操作；已消耗的体力与已领取的奖励不可回退。需要中途停止时用任务停止操作。",
            [JsonSerializer.SerializeToElement(new { configName = "配置名，省略时用当前选中配置", waitForCompletion = false })],
            "bridge-stable-operation");
        registry.Register(
            id,
            "scheduler",
            "运行 BetterGI 一条龙日常流程。可选 configName 指定 User/OneDragon 里的配置；省略用当前选中的配置。",
            Invoke,
            readOnly: false,
            destructive: false,
            inputSchema: OneDragonSchema(),
            guide: guide,
            requiresGameReady: true,
            preparation: "ensureGameReady");
    }

    private static void RegisterExitGame(MethodRegistry registry)
    {
        const string id = "bgi.exit_game";
        var guide = new AgentGuide(
            "退出原神",
            "结束原神游戏进程。等价于 BetterGI 一条龙收尾的退出方式：先请求正常关闭，5 秒未退再结束进程。任务收尾、切换账号前使用。",
            ["用户要求退出/关闭原神，或一条龙与任务链结束后按约定收尾时。"],
            ["原神正在运行。"],
            ["结束原神进程；BetterGI 与本工具继续运行。未保存的钓鱼杆数量等游戏内状态由游戏自己落盘。"],
            "调用返回即已发出关闭；进程在 5 秒内未自行退出会被强制结束。",
            "调用后读一次 bgi.get_status：gameHandle 变 0 或 captureReady 回落即已退出；不要反复调用。",
            "只结束游戏进程，不改任何配置；重新进入用 bgi.start_game。",
            [JsonSerializer.SerializeToElement(new { })],
            "bridge-stable-operation");
        registry.Register(
            id,
            "scheduler",
            "退出原神（正常关闭，超时强结束）。返回前等待进程退出或被强制结束。",
            ExitInvoke,
            readOnly: false,
            destructive: false,
            inputSchema: JsonSerializer.SerializeToElement(new { type = "object", additionalProperties = false }),
            guide: guide,
            // 退出必须在登录/加载画面和非 16:9 时也能执行，不能要求主界面就绪。
            requiresGameReady: false);
    }

    private static JsonElement OneDragonSchema() =>
        JsonSerializer.SerializeToElement(new
        {
            type = "object",
            properties = new
            {
                configName = new { type = "string", description = "User/OneDragon 里的配置名；省略用当前选中的配置" },
                waitForCompletion = new { type = "boolean", @default = false, description = "默认 false：入口派发并核验任务锁被占用后即交接返回，不守护整条一条龙；true 时等宿主方法返回并核验任务锁释放。只有用户明确要求等结果才设 true。" },
            },
            additionalProperties = false,
        });

    private static async Task<object?> Invoke(
        JsonElement arguments,
        CancellationToken cancellation)
    {
        // 就绪准备由工具执行层按真实状态处理（与配置组运行入口同一状态机）：
        // 无进程→宿主命令启动；有进程无截图→挂截图器；加载→阻塞等主界面；
        // 非 16:9/超时→如实失败。
        await StatusTools.EnsureGameReady(cancellation).ConfigureAwait(false);
        if (!Host.GameSixteenToNine())
            throw BridgeException.GameNotReady(
                Host.GameClientSize() is { } size
                    ? $"游戏窗口分辨率 {size.Width}x{size.Height} 不是 16:9，一条龙无法运行。请先把游戏或远程桌面会话调到 16:9（如 1920x1080）。"
                    : "游戏窗口尺寸未知，无法确认 16:9，一条龙不执行。");
        if (Host.TaskSemaphoreCount() is not > 0)
            throw BridgeException.Busy("已有独立任务运行或任务锁状态未知。");

        return await Ui.InvokeAsync(async () =>
        {
            cancellation.ThrowIfCancellationRequested();
            var services = Host.Services()
                ?? throw BridgeException.Missing("拿不到 BetterGI 的服务容器。");
            var type = Reflect.FindType(ViewModelType)
                ?? throw BridgeException.Missing("当前 BetterGI 没有公开的一条龙 ViewModel。");
            var viewModel = services.GetService(type)
                ?? throw BridgeException.Missing("一条龙 ViewModel 未注册。");
            var requested = arguments.TryGetProperty("configName", out var named) ? named.GetString() : null;
            var waitForCompletion = arguments.TryGetProperty("waitForCompletion", out var wait) && wait.GetBoolean();
            var selected = BindConfiguration(viewModel, requested);
            cancellation.ThrowIfCancellationRequested();
            var execution = Reflect.Call(viewModel, "OnOneKeyExecute")
                as Task ?? throw BridgeException.Missing("一条龙执行入口不可调用。");
            var configName = Reflect.Get(selected, "Name") as string;
            if (!waitForCompletion)
            {
                // 启动交接：不守护整条一条龙。挂一个只观察异常的续接防止未观察异常，
                // 不向宿主传递取消。派发后等待"空闲→占用"的换锁沿或宿主方法已返回：
                // 正在占锁时不等，尚未占锁才等；立即故障如实透出，不把晚启动报成没启动。
                _ = execution.ContinueWith(static task => _ = task.Exception,
                    TaskScheduler.Default);
                if (execution.IsFaulted) await execution.ConfigureAwait(false);
                var deadline = DateTimeOffset.UtcNow.AddSeconds(5);
                while (Host.TaskSemaphoreCount() is not 0 && !execution.IsCompleted && DateTimeOffset.UtcNow < deadline)
                {
                    cancellation.ThrowIfCancellationRequested();
                    await Task.Delay(100, cancellation).ConfigureAwait(false);
                }
                var engaged = Host.TaskSemaphoreCount() is 0;
                var returned = execution.IsCompleted;
                var started = engaged || (returned && !execution.IsFaulted && !execution.IsCanceled);
                object launchResult = new
                {
                    started,
                    accepted = started,
                    configName,
                    executionMode = "launch",
                    lockEngaged = engaged,
                    hostReturned = returned,
                    verified = started,
                    verificationScope = "launch",
                    verificationReason = engaged
                        ? "只核验入口已派发且宿主任务锁被占用；不是一条龙业务完成的证据。"
                        : returned
                            ? "入口已派发且宿主方法已同步返回（快速完成路径）；未观察到锁占用，按宿主返回为准。"
                            : "入口已调用但 5 秒内未观察到任务锁被占用且宿主方法未返回；按未启动处理，请核对宿主状态。",
                };
                return launchResult;
            }
            await execution.ConfigureAwait(false);
            var released = Host.TaskSemaphoreCount() is > 0;
            object completionResult = new
            {
                started = true,
                accepted = true,
                configName,
                executionMode = "completion",
                verified = released,
                verificationScope = "executionReturn",
                verificationReason = released
                    ? "宿主方法已返回且任务锁已释放；这是执行链返回的证据，不代表每项业务成功。"
                    : "宿主方法已返回但任务锁仍被占用或状态未知；不能据此宣称一条龙完成。",
            };
            return completionResult;
        });
    }

    /// <summary>从磁盘刷新配置并同步任务列表，避免只更换名称仍执行旧任务。</summary>
    public static object BindConfiguration(object viewModel, string? requested)
    {
        var prior = Reflect.Get(viewModel, "SelectedConfig");
        var name = requested?.Trim() ?? (prior is null ? null : Reflect.Get(prior, "Name") as string);
        Reflect.Call(viewModel, "InitConfigList");
        if (name is not null)
        {
            var list = Reflect.Get(viewModel, "ConfigList") as System.Collections.IEnumerable
                ?? throw BridgeException.Missing("一条龙配置列表不可读。");
            var matches = list.Cast<object>().Where(item => string.Equals(Reflect.Get(item, "Name") as string, name, StringComparison.OrdinalIgnoreCase)).ToArray();
            if (matches.Length == 0) throw BridgeException.NotFound($"一条龙配置不存在：{name}");
            if (matches.Length > 1) throw new BridgeException("AMBIGUOUS_TARGET", "同名一条龙配置不唯一，未执行。", 409);
            Reflect.Set(viewModel, "SelectedConfig", matches[0]);
        }
        var selected = Reflect.Get(viewModel, "SelectedConfig") ?? throw BridgeException.NotFound("没有可运行的一条龙配置。");
        Reflect.Call(viewModel, "SetSomeSelectedConfig", selected);
        return selected;
    }

    private static async Task<object?> ExitInvoke(
        JsonElement arguments,
        CancellationToken cancellation)
    {
        cancellation.ThrowIfCancellationRequested();
        if (!Host.CaptureReady && !Host.GameProcessRunning())
            throw BridgeException.GameNotReady("原神没有在运行，无需退出。");
        var systemControl = Reflect.FindType(SystemControlType)
            ?? throw BridgeException.Missing("当前 BetterGI 没有公开的 SystemControl。");
        // 关闭前先锁定本会话的原神进程身份，关闭后按 PID 复核进程确实消失；
        // closed/verified 只以进程消失为准，不盲信 CloseGame 返回。
        var before = SessionGenshinProcesses();
        // CloseGame 内部已处理等待与超时强杀，只能在 UI 线程同步执行。
        await Ui.InvokeAsync(() => Reflect.CallStatic(systemControl, "CloseGame")).ConfigureAwait(false);
        var deadline = DateTimeOffset.UtcNow.AddSeconds(10);
        while (before.Count > 0 && DateTimeOffset.UtcNow < deadline)
        {
            cancellation.ThrowIfCancellationRequested();
            before.RemoveAll(pid => !ProcessIsAlive(pid));
            if (before.Count == 0) break;
            await Task.Delay(200, cancellation).ConfigureAwait(false);
        }
        // 权威证据是本会话关闭前锁定的进程全部消失；Host.GameProcessRunning()
        // 不分会话，别让其它会话的原神挡住本次核验。
        var gone = before.Count == 0;
        return new
        {
            closed = gone,
            verified = gone,
            outcome = gone ? "closed" : "timeout",
            remainingProcessIds = gone ? [] : before.ToArray(),
            captureReady = Host.CaptureReady,
            verificationScope = "processExit",
            verificationReason = gone
                ? "已观察到关闭前锁定的全部原神进程退出。"
                : "CloseGame 已调用，但 10 秒内仍能观察到原神进程；关闭未核验，不得报告已退出。",
        };
    }

    /// <summary>按宿主使用的原神进程名清单收集当前会话的进程 ID；清单不可读时退回常见两个进程名。</summary>
    private static List<int> SessionGenshinProcesses()
    {
        var names = new[] { "YuanShen", "GenshinImpact" }.AsEnumerable();
        var context = Reflect.Singleton("BetterGenshinImpact.GameTask.TaskContext");
        if (context?.GetType().GetMethod("GetGenshinGameProcessNameList")?.Invoke(context, null)
            is System.Collections.IEnumerable list)
        {
            var collected = list.Cast<object?>().OfType<string>().Where(name => !string.IsNullOrWhiteSpace(name)).ToList();
            if (collected.Count > 0) names = collected;
        }
        var session = Process.GetCurrentProcess().SessionId;
        var result = new List<int>();
        foreach (var name in names)
            foreach (var process in Process.GetProcessesByName(name))
                using (process)
                {
                    try
                    {
                        if (!process.HasExited && process.SessionId == session) result.Add(process.Id);
                    }
                    catch (InvalidOperationException)
                    {
                        // 进程刚好退出。
                    }
                }
        return result;
    }

    private static bool ProcessIsAlive(int pid)
    {
        try
        {
            using var process = Process.GetProcessById(pid);
            return !process.HasExited;
        }
        catch (ArgumentException)
        {
            return false;
        }
    }
}
