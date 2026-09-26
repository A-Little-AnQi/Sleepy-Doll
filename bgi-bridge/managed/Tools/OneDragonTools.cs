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
            ["截图器就绪且已进入游戏主界面。", "窗口分辨率为 16:9。", "没有其他独立任务持锁。"],
            ["按所选一条龙配置串行执行启用的日常任务；过程中会自动传送、领取与点击游戏界面；配置开启时收尾自动退出游戏。"],
            "started=true 表示一条龙已开始执行，Job 到终态即整轮结束。",
            "Job 到终态后读一次 bgi.read_host_log（过滤「一条龙」或 ERR）确认各任务结果与是否已退出游戏；不要反复轮询。",
            "一条龙只做日常任务范围内的操作；已消耗的体力与已领取的奖励不可回退。需要中途停止时用任务停止操作。",
            [JsonSerializer.SerializeToElement(new { configName = "配置名，省略时用当前选中配置" })],
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
            requiresGameReady: true);
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
                configName = new { type = "string", description = "User/OneDragon 里的配置名；省略用当前选中配置" },
            },
            additionalProperties = false,
        });

    private static async Task<object?> Invoke(
        JsonElement arguments,
        CancellationToken cancellation)
    {
        if (!Host.CaptureReady)
            throw BridgeException.GameNotReady("截图器或游戏窗口尚未就绪。");
        if (!Host.InMainUi())
            throw BridgeException.GameNotReady(
                "游戏还没进入主界面（登录或加载画面）。用 bgi.get_status 等到 ready=true 后再运行一条龙。");
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
                ?? throw BridgeException.Missing("拿不到 BetterGI 服务容器。");
            var type = Reflect.FindType(ViewModelType)
                ?? throw BridgeException.Missing("当前 BetterGI 没有公开的一条龙 ViewModel。");
            var viewModel = services.GetService(type)
                ?? throw BridgeException.Missing("一条龙 ViewModel 未注册。");
            var requested = arguments.TryGetProperty("configName", out var named) ? named.GetString() : null;
            var selected = BindConfiguration(viewModel, requested);
            cancellation.ThrowIfCancellationRequested();
            await (Reflect.Call(viewModel, "OnOneKeyExecute")
                as Task ?? throw BridgeException.Missing("一条龙执行入口不可调用。"));
            return new
            {
                started = true,
                configName = Reflect.Get(selected, "Name") as string,
            };
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

    private static Task<object?> ExitInvoke(
        JsonElement arguments,
        CancellationToken cancellation)
    {
        cancellation.ThrowIfCancellationRequested();
        if (!Host.CaptureReady && !Host.GameProcessRunning())
            throw BridgeException.GameNotReady("原神没有在运行，无需退出。");
        var systemControl = Reflect.FindType(SystemControlType)
            ?? throw BridgeException.Missing("当前 BetterGI 没有公开的 SystemControl。");
        // CloseGame 内部已处理等待与超时强杀，同步执行。
        Reflect.CallStatic(systemControl, "CloseGame");
        return Task.FromResult<object?>(new { closed = true, captureReady = Host.CaptureReady });
    }
}
