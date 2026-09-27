using System.Text.Json;
using BgiBridge.Bgi;
using BgiBridge.Catalog;
using BgiBridge.Protocol;

namespace BgiBridge.Tools;

/// <summary>按名称调用的稳定操作，不依赖界面命令。</summary>
public static class ScriptGroupTools
{
    private const string PreferredViewModelType =
        "BetterGenshinImpact.ViewModel.Pages.ScriptControlViewModel";
    private const string RunMethod = "OnStartMultiScriptGroupWithNamesAsync";

    public static void Register(MethodRegistry registry)
    {
        const string id = "bgi.run_script_group";
        var guide = new AgentGuide(
            "按名称运行配置组",
            "调用 BetterGI 自带的按名称执行入口，运行指定配置组中的已启用任务。无需用户预先在脚本调度页选中目标。",
            ["用户要求运行一个已从 User/ScriptGroup 确认存在的配置组时。"],
            ["截图器就绪且已进入游戏主界面。", "窗口分辨率为 16:9。", "没有其他独立任务持锁。", "groupName 来自 User/ScriptGroup 的真实 name。"],
            ["启动配置组中的游戏自动化、脚本、路线或 Shell 任务；具体影响由组内已启用任务决定。"],
            "默认 waitForCompletion=false：accepted=true 表示宿主已接受运行，交接后立即返回，不守护整组。只有用户明确要求等结果、后续步骤或跟踪进度时才设 true。verified 的 launch 范围只核验启动交接，不是业务完成。",
            "普通运行请求用返回证据总结本次已提交或执行方法已返回，并结束本轮。不继续 job.get、查任务状态、日志或截图，不重跑。只有用户明确要求等待结果、后续步骤、查看进度或排错时才继续。",
            "已经发送的游戏输入和脚本副作用不能自动撤销；需要停止时使用对应停止操作并核验终态。",
            [JsonSerializer.SerializeToElement(new { groupName = "用户目录中读取到的精确配置组名称" })],
            "bridge-stable-operation");
        registry.Register(
            id,
            "scheduler",
            guide.Purpose,
            Invoke,
            readOnly: false,
            destructive: true,
            inputSchema: AgentSchemas.Input(id),
            guide: guide,
            requiresGameReady: true);
    }

    private static async Task<object?> Invoke(
        JsonElement arguments,
        CancellationToken cancellation)
    {
        var requested = arguments.GetProperty("groupName").GetString()!.Trim();
        var waitForCompletion = arguments.TryGetProperty("waitForCompletion", out var wait) && wait.GetBoolean();
        if (!Host.CaptureReady)
            throw BridgeException.GameNotReady("截图器或游戏窗口尚未就绪。");
        // 对齐 BetterGI 原生调度前置：主界面才算在游戏里；登录/加载画面跑配置组必然失败。
        if (!Host.InMainUi())
            throw BridgeException.GameNotReady(
                "游戏还没进入主界面（登录或加载画面）。用 bgi.wait_ready 阻塞等待 ready=true，不让模型循环查状态。");
        // 脚本依赖 16:9 截图；非 16:9 时整组会逐条异常退出，宁可在这里拦下。
        if (!Host.GameSixteenToNine())
            throw BridgeException.GameNotReady(
                Host.GameClientSize() is { } size
                    ? $"游戏窗口分辨率 {size.Width}x{size.Height} 不是 16:9，配置组无法运行。请先把游戏或远程桌面会话调到 16:9（如 1920x1080）。"
                    : "游戏窗口尺寸未知，无法确认 16:9，配置组不执行。");
        if (Host.TaskSemaphoreCount() is not > 0)
            throw BridgeException.Busy("已有独立任务运行或任务锁状态未知。");

        return await Ui.InvokeAsync(async () =>
        {
            cancellation.ThrowIfCancellationRequested();
            var services = Host.Services()
                ?? throw BridgeException.Missing("拿不到 BetterGI 服务容器。");
            var type = Reflect.FindType(PreferredViewModelType)
                ?? Reflect.FindHostTypeWithMethod(RunMethod, typeof(string[]))
                ?? throw BridgeException.Missing($"当前 BetterGI 没有公开的 {RunMethod}(string[])。");
            var viewModel = services.GetService(type)
                ?? throw BridgeException.Missing($"包含 {RunMethod} 的 BetterGI 服务未注册。");
            var matches = ResolveGroupsFromDisk(requested);
            if (matches.Length == 0)
            {
                var available = ResolveGroupsFromDisk(null).Take(30);
                throw BridgeException.NotFound(
                    $"配置组不存在：{requested}。当前可用：{string.Join("、", available)}");
            }
            if (matches.Length != 1)
                throw new BridgeException(
                    "AMBIGUOUS_TARGET",
                    $"存在多个同名配置组：{requested}；未执行。",
                    409);

            var resolved = matches[0];
            var groups = BindGroupsFromDisk(Path.Combine(AppContext.BaseDirectory, "User", "ScriptGroup"), resolved);
            cancellation.ThrowIfCancellationRequested();
            // 直接执行已绑定对象，不刷新会重写其他配置组的界面集合。
            var execution = Reflect.Call(viewModel, "StartGroups", groups, null, false);
            return await ScriptLaunch.Finish(Reflect.Await(execution), resolved, waitForCompletion, cancellation);
        }).ConfigureAwait(false);
    }

    /// <summary>构造指定配置组的真实宿主对象，不修改页面缓存或其他组文件。</summary>
    public static System.Collections.IList BindGroupsFromDisk(string directory, string name)
    {
        var files = Directory.EnumerateFiles(directory, "*.json").Where(file =>
        {
            try { using var json = JsonDocument.Parse(File.ReadAllText(file)); return json.RootElement.GetProperty("name").GetString() == name; }
            catch (JsonException) { return false; }
        }).ToArray();
        if (files.Length != 1) throw new BridgeException("AMBIGUOUS_TARGET", "配置组文件未唯一定位，未执行。", 409);
        var type = Reflect.FindType("BetterGenshinImpact.Core.Script.Group.ScriptGroup") ?? throw BridgeException.Missing("宿主配置组类型不可用。");
        var group = Reflect.CallStatic(type, "FromJson", File.ReadAllText(files[0])) ?? throw BridgeException.Failed("配置组解析失败。");
        var groups = (System.Collections.IList)Activator.CreateInstance(typeof(List<>).MakeGenericType(type))!;
        groups.Add(group);
        return groups;
    }

    private static string[] ResolveGroupsFromDisk(string? requested)
    {
        var directory = Path.Combine(AppContext.BaseDirectory, "User", "ScriptGroup");
        if (!Directory.Exists(directory)) return [];
        var names = new List<string>();
        foreach (var file in Directory.EnumerateFiles(directory, "*.json"))
        {
            try
            {
                using var document = JsonDocument.Parse(File.ReadAllText(file));
                if (document.RootElement.TryGetProperty("name", out var property)
                    && property.ValueKind == JsonValueKind.String
                    && property.GetString() is { } name
                    && !string.IsNullOrWhiteSpace(name)
                    && (requested is null || string.Equals(name, requested, StringComparison.OrdinalIgnoreCase)))
                {
                    names.Add(name);
                }
            }
            catch (JsonException)
            {
                // 单个损坏的配置文件不影响其它组。
            }
            catch (IOException)
            {
                // 文件可能正被 BetterGI 原子替换。
            }
        }
        return names.Distinct(StringComparer.Ordinal).Order(StringComparer.Ordinal).ToArray();
    }
}
