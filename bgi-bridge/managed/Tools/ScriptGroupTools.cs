using System.Text.Json;
using BgiBridge.Bgi;
using BgiBridge.Catalog;
using BgiBridge.Protocol;

namespace BgiBridge.Tools;

/// <summary>Stable object-oriented operations that the WPF command surface cannot express safely.</summary>
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
            ["截图器和游戏窗口已就绪。", "没有其他独立任务持锁。", "groupName 来自 User/ScriptGroup 的真实 name。"],
            ["启动配置组中的游戏自动化、脚本、路线或 Shell 任务；具体影响由组内已启用任务决定。"],
            "返回 resolved=true 和 executed=true 只表示目标组已解析且宿主执行方法已返回；任务业务结果仍需继续读取 Job 和运行状态。",
            "等待桥 Job 终态，并按配置组任务产物或 BetterGI 运行状态核验；不得只凭 executed=true 报告任务完成。",
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
        if (!Host.CaptureReady)
            throw BridgeException.GameNotReady("截图器或游戏窗口尚未就绪。");
        if (Host.TaskSemaphoreCount() is not > 0)
            throw BridgeException.Busy("已有独立任务运行或任务锁状态未知。");

        return await Ui.InvokeAsync(async () =>
        {
            cancellation.ThrowIfCancellationRequested();
            var services = Host.Services()
                ?? throw BridgeException.Missing("拿不到宿主服务容器。");
            var type = Reflect.FindType(PreferredViewModelType)
                ?? Reflect.FindHostTypeWithMethod(RunMethod, typeof(string[]))
                ?? throw BridgeException.Missing($"当前 BetterGI 没有公开的 {RunMethod}(string[])。");
            var viewModel = services.GetService(type)
                ?? throw BridgeException.Missing($"包含 {RunMethod} 的宿主服务未注册。");
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
            // 新建配置组可能尚未进入页面 ViewModel 的内存集合。该刷新是兼容增强，
            // 不是硬依赖：未来版本若移除私有刷新函数，公开按名称执行入口仍可调用。
            try
            {
                Reflect.Call(viewModel, "ReadScriptGroup");
            }
            catch (BridgeException ex) when (ex.Code == "HOST_CAPABILITY_MISSING")
            {
            }
            cancellation.ThrowIfCancellationRequested();
            var execution = Reflect.Call(
                viewModel,
                RunMethod,
                (object)new[] { resolved });
            await Reflect.Await(execution);
            return new
            {
                groupName = resolved,
                resolved = true,
                executed = true,
            };
        }).ConfigureAwait(false);
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
                // 一个损坏的配置文件不应阻止其它有效组被解析。
            }
            catch (IOException)
            {
                // 文件可能正被 BetterGI 原子替换；本次只忽略该文件。
            }
        }
        return names.Distinct(StringComparer.Ordinal).Order(StringComparer.Ordinal).ToArray();
    }
}
