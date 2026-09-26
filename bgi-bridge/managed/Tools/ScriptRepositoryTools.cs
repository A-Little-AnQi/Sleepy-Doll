using System.Collections;
using System.Text.Json;
using BgiBridge.Bgi;
using BgiBridge.Catalog;
using BgiBridge.Protocol;

namespace BgiBridge.Tools;

/// <summary>仓库更新入口：调用 BetterGI 的 ScriptRepoUpdater，不经过界面按钮。</summary>
public static class ScriptRepositoryTools
{
    private const string PreferredType = "BetterGenshinImpact.Core.Script.ScriptRepoUpdater";
    private const string ManualUpdateMethod = "ManualUpdateSubscribedScripts";

    public static void Register(MethodRegistry registry)
    {
        RegisterReader(registry, "bgi.search_script_repository", "检索中央脚本仓库",
            "按脚本标题或功能词搜索当前中央仓库索引，包括未订阅脚本；返回精确内容路径，不搜索宿主设置。", Search);
        RegisterReader(registry, "bgi.read_script_repository_file", "读取中央仓库文件",
            "直接读取中央 Git 仓库中的 README、settings、manifest、JS 与模块源码；未订阅文件同样可读，不检出、不订阅、不执行。支持行号分页和关键词上下文。", Read);
        const string subscribeId = "bgi.subscribe_script_resources";
        var subscribeGuide = new AgentGuide("订阅仓库资源", "按精确中央仓库路径导入路线或脚本，调用 BetterGI 的订阅器，不手工重写 JSON。",
            ["已从仓库结果选择完整父目录或脚本，需要安装到 User 后运行。"], ["桥已连接；paths 来自仓库实际结果。"],
            ["记录订阅并导入所选资源；未订阅的同路径本地资源不覆盖。"], "installed=true 且各目标已在 User 中存在才表示准备完成。",
            "订阅完成后准备配置组并运行；不把准备当作执行完成。", "不自动删除已有资源。", [], "bridge-stable-operation");
        registry.Register(subscribeId, "repository", subscribeGuide.Purpose, Subscribe,
            readOnly: false, destructive: true, inputSchema: AgentSchemas.Input(subscribeId), guide: subscribeGuide);
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

    private static void RegisterReader(MethodRegistry registry, string id, string title, string purpose, MethodHandler handler)
    {
        var guide = new AgentGuide(title, purpose,
            ["解释脚本参数、实际执行逻辑或排查源码时，包括脚本尚未订阅的情况。"],
            ["桥已连接；当前渠道的中央仓库已存在。"], ["无写入副作用。"],
            "返回中央仓库来源、精确路径、搜索候选或带行号文本；truncated=true 时按 nextLine 继续读取。",
            "脚本参数以 settings 与 JS 的实际读取、分支和调用为准；已安装版本可能与中央仓库版本不同。",
            "只读，不更改订阅和用户文件。", [], "bridge-stable-operation");
        registry.Register(id, "repository", purpose, handler, inputSchema: AgentSchemas.Input(id), guide: guide);
    }

    private static (Type Type, object Updater) Reader()
    {
        var type = Reflect.FindType(PreferredType)
            ?? throw BridgeException.Missing("当前 BetterGI 没有中央仓库读取器。");
        return (type, Reflect.Singleton(type)
            ?? throw BridgeException.Missing("无法取得中央仓库读取器实例。"));
    }

    private static Task<object?> Search(JsonElement arguments, CancellationToken cancellation)
    {
        cancellation.ThrowIfCancellationRequested();
        var (type, _) = Reader();
        var root = Reflect.CallStatic(type, "get_CenterRepoPath") as string
            ?? throw BridgeException.Missing("无法取得当前渠道的中央仓库路径。");
        var indexPath = Path.Combine(root, "repo.json");
        if (!File.Exists(indexPath))
            throw BridgeException.NotFound("中央仓库索引尚未下载；刷新中央仓库后再查询，不需要订阅脚本。");
        var result = ScriptRepositoryReader.Search(File.ReadAllText(indexPath), arguments.GetProperty("query").GetString()!,
            arguments.TryGetProperty("category", out var category) ? category.GetString()! : "all",
            arguments.TryGetProperty("offset", out var offset) ? offset.GetInt32() : 0,
            arguments.TryGetProperty("limit", out var limit) ? limit.GetInt32() : 8);
        return Task.FromResult<object?>(result);
    }

    private static Task<object?> Read(JsonElement arguments, CancellationToken cancellation)
    {
        cancellation.ThrowIfCancellationRequested();
        var path = ScriptRepositoryReader.NormalizePath(arguments.GetProperty("path").GetString()!);
        if (!new[] { ".json", ".js", ".ts", ".md", ".txt", ".yaml", ".yml" }.Contains(Path.GetExtension(path), StringComparer.OrdinalIgnoreCase))
            throw BridgeException.InvalidArgument("只读取 JSON、JS、TS、Markdown 和文本资料，不返回二进制资源。");
        var (_, updater) = Reader();
        // 宿主读取器从 Git Blob 或文件式仓库读取，不依赖订阅目录。
        var content = Reflect.Call(updater, "ReadFileFromCenterRepo", path) as string
            ?? throw BridgeException.NotFound($"中央仓库文件不存在或读取失败：{path}；不要改读安装目录。");
        cancellation.ThrowIfCancellationRequested();
        return Task.FromResult<object?>(ScriptRepositoryReader.Read(path, content,
            arguments.TryGetProperty("startLine", out var start) ? start.GetInt32() : 1,
            arguments.TryGetProperty("maxLines", out var count) ? count.GetInt32() : 160,
            arguments.TryGetProperty("contains", out var contains) ? contains.GetString() : null));
    }

    private static async Task<object?> Subscribe(JsonElement arguments, CancellationToken cancellation)
    {
        var (type, updater) = Reader();
        var repoRoot = Reflect.CallStatic(type, "get_CenterRepoPath") as string
            ?? throw BridgeException.Missing("无法取得中央仓库路径。");
        var index = File.ReadAllText(Path.Combine(repoRoot, "repo.json"));
        var paths = arguments.GetProperty("paths").EnumerateArray().Select(item =>
            ScriptRepositoryReader.NormalizePath(item.GetString()!)).Distinct(StringComparer.Ordinal).ToArray();
        var subscribed = CurrentSubscriptions(type);
        var missing = new List<string>();
        foreach (var path in paths)
        {
            cancellation.ThrowIfCancellationRequested();
            if (ScriptRepositoryReader.FindNode(index, path) is null)
                throw BridgeException.NotFound($"中央仓库没有该精确路径：{path}");
            var target = InstalledPath(path);
            var exists = File.Exists(target) || Directory.Exists(target);
            if (exists && !subscribed.Contains(path, StringComparer.OrdinalIgnoreCase))
                throw new BridgeException("LOCAL_RESOURCE_CONFLICT", $"本机已有未订阅的同路径资源，未覆盖：{path}", 409);
            if (!exists) missing.Add(path);
        }
        if (missing.Count > 0)
            await Ui.InvokeAsync(async () => {
                cancellation.ThrowIfCancellationRequested();
                await Reflect.Await(Reflect.Call(updater, "ImportScriptFromPathJson", JsonSerializer.Serialize(missing))).ConfigureAwait(true);
                return (object?)null;
            }).ConfigureAwait(false);
        var failures = paths.Where(path => !File.Exists(InstalledPath(path)) && !Directory.Exists(InstalledPath(path))).ToArray();
        if (failures.Length > 0) throw BridgeException.Failed($"订阅返回后目标仍缺失：{string.Join("、", failures)}");
        return new { installed = true, paths, importedPaths = missing.ToArray() };
    }

    internal static string InstalledPath(string path)
    {
        path = ScriptRepositoryReader.NormalizePath(path);
        var split = path.IndexOf('/');
        if (split < 0) throw BridgeException.InvalidArgument("必须选择分类下的具体资源。");
        var directory = path[..split] switch {
            "pathing" => "AutoPathing", "js" => "JsScript", "combat" => "AutoFight", "tcg" => "AutoGeniusInvokation", _ => throw BridgeException.InvalidArgument("不支持的资源分类。")
        };
        return Path.Combine(AppContext.BaseDirectory, "User", directory, path[(split + 1)..].Replace('/', Path.DirectorySeparatorChar));
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
        // ImportScriptFromPathJson 会直接弹 Toast 并捕获 WPF 同步上下文，只能在 UI 线程调用；
        // 在桥的 Job 线程上执行会以「调用线程无法访问此对象」失败。仍要等待它的异步操作完成。
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
