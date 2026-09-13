using System.Collections;
using System.Text.Json;
using BgiBridge.Bgi;
using BgiBridge.Catalog;
using BgiBridge.Protocol;

namespace BgiBridge.Tools;

/// <summary>Stable repository operations backed by ScriptRepoUpdater rather than window buttons.</summary>
public static class ScriptRepositoryTools
{
    private const string PreferredType = "BetterGenshinImpact.Core.Script.ScriptRepoUpdater";
    private const string ManualUpdateMethod = "ManualUpdateSubscribedScripts";

    public static void Register(MethodRegistry registry)
    {
        const string id = "bgi.update_subscribed_scripts";
        var guide = new AgentGuide(
            "更新脚本仓库与订阅内容",
            "调用 BetterGI 自带仓库更新器。可只刷新中央仓库、只更新指定的已订阅路径，或更新全部当前订阅；不需要打开脚本仓库窗口。",
            ["用户要求立即刷新脚本仓库，或更新一个、多个、全部已订阅脚本和路线时。"],
            ["桥已连接。", "selected 模式的 paths 来自 User/Subscriptions 的真实当前订阅。", "当前没有另一项仓库导入或更新操作。"],
            ["访问当前配置的仓库渠道；selected/all 会覆盖订阅资源的程序文件，并由 BetterGI 保留其支持的脚本配置文件。"],
            "completed=true 表示 BetterGI 更新函数已返回；repositoryChanged 仅报告中央仓库是否拉到新内容，不证明每个目标文件符合预期。",
            "selected/all 完成后回读目标 manifest、版本或关键文件；repositoryOnly 可结合 repositoryChanged 与更新时间判断。",
            "中央仓库和脚本程序文件没有通用自动回退；BetterGI 只保留其更新器明确支持的脚本配置。更新前必须由运行时审批确认范围。",
            [JsonSerializer.SerializeToElement(new { mode = "selected", paths = new[] { "js/配置中读取到的真实目录" } })],
            "bridge-stable-operation");
        registry.Register(
            id,
            "repository",
            guide.Purpose,
            Invoke,
            readOnly: false,
            destructive: true,
            inputSchema: AgentSchemas.Input(id),
            guide: guide);
    }

    private static async Task<object?> Invoke(JsonElement arguments, CancellationToken cancellation)
    {
        var mode = arguments.GetProperty("mode").GetString()!;
        var type = Reflect.FindType(PreferredType)
            ?? Reflect.FindHostTypeWithMethod(ManualUpdateMethod)
            ?? throw BridgeException.Missing($"当前 BetterGI 没有公开的 {ManualUpdateMethod}()。 ");
        var updater = Reflect.Singleton(type)
            ?? throw BridgeException.Missing("无法取得 BetterGI 脚本仓库更新器实例。");
        var subscribed = CurrentSubscriptions(type);
        cancellation.ThrowIfCancellationRequested();

        if (mode == "all")
        {
            await Reflect.Await(Reflect.Call(updater, ManualUpdateMethod)).ConfigureAwait(false);
            return new { mode, repositoryChanged = (bool?)null, updatedPaths = subscribed, completed = true };
        }

        var repositoryChanged = await RefreshRepository(type, updater, cancellation).ConfigureAwait(false);
        if (mode == "repositoryOnly")
            return new { mode, repositoryChanged, updatedPaths = Array.Empty<string>(), completed = true };

        if (mode != "selected")
            throw BridgeException.InvalidArgument("mode 必须是 repositoryOnly、selected 或 all。");
        if (!arguments.TryGetProperty("paths", out var pathValue) || pathValue.ValueKind != JsonValueKind.Array)
            throw BridgeException.InvalidArgument("selected 模式必须提供 paths。");
        var requested = pathValue.EnumerateArray().Select(item => item.GetString()!.Trim()).ToArray();
        var canonical = requested.Select(path => subscribed.FirstOrDefault(item =>
                string.Equals(item, path, StringComparison.OrdinalIgnoreCase))
            ?? throw BridgeException.InvalidArgument($"路径不是当前订阅：{path}"))
            .Distinct(StringComparer.Ordinal)
            .ToArray();
        cancellation.ThrowIfCancellationRequested();
        // ImportScriptFromPathJson is also a UI entry point: it raises Toasts
        // directly and captures the WPF synchronization context. Starting it on
        // the bridge Job thread fails after the repository pull with
        // "the calling thread cannot access this object". Dispatch only this
        // phase to WPF and still await its complete async operation.
        await Ui.InvokeAsync(async () =>
        {
            cancellation.ThrowIfCancellationRequested();
            await Reflect.Await(Reflect.Call(
                updater,
                "ImportScriptFromPathJson",
                JsonSerializer.Serialize(canonical))).ConfigureAwait(true);
            return (object?)null;
        }).ConfigureAwait(false);
        return new { mode, repositoryChanged, updatedPaths = canonical, completed = true };
    }

    private static string[] CurrentSubscriptions(Type type)
    {
        var value = Reflect.CallStatic(type, "GetSubscribedPathsForCurrentRepo");
        if (value is not IEnumerable items)
            throw BridgeException.Missing("当前 BetterGI 无法读取脚本订阅清单。");
        return items.Cast<object>()
            .Select(item => item as string)
            .Where(item => !string.IsNullOrWhiteSpace(item))
            .Cast<string>()
            .Distinct(StringComparer.OrdinalIgnoreCase)
            .Order(StringComparer.OrdinalIgnoreCase)
            .ToArray();
    }

    private static async Task<bool?> RefreshRepository(
        Type type,
        object updater,
        CancellationToken cancellation)
    {
        var scriptConfig = Reflect.Get(Reflect.Get(Host.TaskContext(), "Config"), "ScriptConfig")
            ?? throw BridgeException.Missing("当前 BetterGI 的脚本仓库配置不可用。");
        var url = ResolveRepositoryUrl(type, scriptConfig);
        cancellation.ThrowIfCancellationRequested();
        var result = await Reflect.Await(Reflect.Call(
            updater,
            "UpdateCenterRepoByGit",
            url,
            null)).ConfigureAwait(false);
        return result is null ? null : Reflect.Get(result, "Item2") as bool?;
    }

    private static string ResolveRepositoryUrl(Type type, object scriptConfig)
    {
        try
        {
            if (Reflect.CallStatic(type, "ResolveRepoUrl", scriptConfig) is string resolved
                && !string.IsNullOrWhiteSpace(resolved))
                return resolved;
        }
        catch (BridgeException ex) when (ex.Code == "HOST_CAPABILITY_MISSING")
        {
        }

        var channel = Reflect.Get(scriptConfig, "SelectedChannelName") as string;
        if (string.IsNullOrWhiteSpace(channel) || channel == "CNB")
            return "https://cnb.cool/bettergi/bettergi-scripts-list";
        if (channel == "GitCode")
            return "https://gitcode.com/huiyadanli/bettergi-scripts-list";
        if (channel == "GitHub")
            return "https://github.com/babalae/bettergi-scripts-list";
        if (channel == "自定义"
            && Reflect.Get(scriptConfig, "CustomRepoUrl") is string custom
            && Uri.TryCreate(custom, UriKind.Absolute, out _)
            && custom != "https://example.com/custom-repo")
            return custom;
        throw BridgeException.InvalidArgument($"当前脚本仓库渠道无法解析：{channel ?? "(空)"}");
    }
}
