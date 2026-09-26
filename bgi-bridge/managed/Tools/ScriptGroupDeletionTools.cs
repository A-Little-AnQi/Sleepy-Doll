using System.Collections;
using System.Collections.Specialized;
using System.ComponentModel;
using System.Reflection;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using BgiBridge.Bgi;
using BgiBridge.Catalog;
using BgiBridge.Protocol;

namespace BgiBridge.Tools;

/// <summary>绑定真实配置组对象并调用宿主删除方法。</summary>
public static class ScriptGroupDeletionTools
{
    private const string ViewModelType = "BetterGenshinImpact.ViewModel.Pages.ScriptControlViewModel";

    public static void Register(MethodRegistry registry)
    {
        const string id = "bgi.delete_script_group";
        var guide = new AgentGuide(
            "删除配置组", "按精确名称删除一个调度器配置组，不依赖界面当前选择；保留脚本、地图追踪路线和订阅。",
            ["用户要求删除一个已定位的配置组时；不是停用任务，也不是卸载路线。"],
            ["groupName 来自配置组的 name；expectedSha256 来自 bgi.user.read。", "没有独立任务正在执行；不需要启动游戏或截图器。"],
            ["保存目标文件备份后删除该组，更新组列表；其他配置组文件、路线、JS 和订阅不变。"],
            "deleted=true、verified=true 表示目标文件和宿主组列表均已移除；backup 是 User 下的恢复文件。",
            "返回已验证结果即完成，不再查生命周期、导航或游戏状态。",
            "需要撤销时读取 backup，用 bgi.user.write 在原 path 新建原始内容；已有同名文件时不得覆盖。",
            [JsonSerializer.SerializeToElement(new { groupName = "精确配置组名称", expectedSha256 = "bgi.user.read 返回的 SHA-256" })],
            "bridge-stable-operation");
        registry.Register(id, "scheduler", guide.Purpose, Delete,
            readOnly: false, destructive: true, inputSchema: AgentSchemas.Input(id), guide: guide);
    }

    private static Task<object?> Delete(JsonElement arguments, CancellationToken cancellation) =>
        Ui.InvokeAsync<object?>(() => DeleteOnUi(arguments, cancellation));

    private static object DeleteOnUi(JsonElement arguments, CancellationToken cancellation)
    {
        cancellation.ThrowIfCancellationRequested();
        if (!Host.IsIdle) throw BridgeException.Busy("有独立任务正在运行，配置组未删除。");
        var name = arguments.GetProperty("groupName").GetString()!;
        var expected = arguments.GetProperty("expectedSha256").GetString()!;
        var type = Reflect.FindType(ViewModelType) ?? throw BridgeException.Missing("当前宿主没有配置组管理对象。");
        var viewModel = Host.Services()?.GetService(type) ?? throw BridgeException.Missing("配置组服务未注册。");
        var directory = Reflect.Get(viewModel, "ScriptGroupPath") as string ?? throw BridgeException.Missing("配置组目录不可用。");
        directory = Path.GetFullPath(directory);
        if (!Directory.Exists(directory)) throw BridgeException.NotFound("本机没有配置组目录。");
        if ((File.GetAttributes(directory) & FileAttributes.ReparsePoint) != 0)
            throw BridgeException.InvalidArgument("配置组目录是重解析点，未删除。");
        var matches = new List<(string Path, byte[] Bytes)>();
        foreach (var file in Directory.EnumerateFiles(directory, "*.json"))
        {
            if ((File.GetAttributes(file) & FileAttributes.ReparsePoint) != 0) continue;
            var bytes = File.ReadAllBytes(file);
            try
            {
                using var document = JsonDocument.Parse(Encoding.UTF8.GetString(bytes).TrimStart('\uFEFF'));
                if (document.RootElement.TryGetProperty("name", out var title) && title.ValueKind == JsonValueKind.String
                    && string.Equals(title.GetString(), name, StringComparison.OrdinalIgnoreCase)) matches.Add((file, bytes));
            }
            catch (JsonException) { }
        }
        if (matches.Count == 0) throw BridgeException.NotFound($"配置组不存在：{name}");
        if (matches.Count > 1) throw new BridgeException("AMBIGUOUS_TARGET", $"同名配置组不唯一：{name}；未删除。", 409);
        var (path, content) = matches[0];
        if (!string.Equals(Path.GetFileNameWithoutExtension(path), name, StringComparison.OrdinalIgnoreCase))
            throw new BridgeException("GROUP_CONFLICT", "配置组名称与文件名不一致，宿主删除会作用于不同文件；未删除。", 409);
        var hash = Convert.ToHexString(SHA256.HashData(content)).ToLowerInvariant();
        if (!string.Equals(expected, hash, StringComparison.OrdinalIgnoreCase))
            throw new BridgeException("VERSION_CONFLICT", "配置组在读取后发生变化，未删除；请重新读取。", 409);
        var groups = Reflect.Get(viewModel, "ScriptGroups") as IList ?? throw BridgeException.Missing("配置组列表不可用。");
        var cached = groups.Cast<object>().Where(group => string.Equals(Reflect.Get(group, "Name") as string, name, StringComparison.OrdinalIgnoreCase)).ToArray();
        if (cached.Length > 1) throw new BridgeException("AMBIGUOUS_TARGET", "宿主缓存存在多个同名组，未删除。", 409);
        var groupType = Reflect.FindType("BetterGenshinImpact.Core.Script.Group.ScriptGroup") ?? throw BridgeException.Missing("宿主配置组类型不可用。");
        var target = cached.FirstOrDefault() ?? Reflect.CallStatic(groupType, "FromJson", Encoding.UTF8.GetString(content).TrimStart('\uFEFF'))!;
        if (Reflect.FindMethod(type, "OnDeleteScriptGroup", 1) is null) throw BridgeException.Missing("宿主没有删除配置组的方法。");
        var autosave = Reflect.FindMethod(type, "ScriptGroupsCollectionChanged", 2) ?? throw BridgeException.Missing("宿主组列表保存逻辑不可识别，未删除。");
        var changed = groups.GetType().GetEvent("CollectionChanged") ?? throw BridgeException.Missing("宿主组列表事件不可识别。");
        var handler = autosave.CreateDelegate(typeof(NotifyCollectionChangedEventHandler), viewModel);
        var previousSelection = Reflect.Get(viewModel, "SelectedScriptGroup");
        var backup = path + ".sleepy-doll." + Guid.NewGuid().ToString("N") + ".bak";
        cancellation.ThrowIfCancellationRequested();
        using (var output = new FileStream(backup, FileMode.CreateNew, FileAccess.Write, FileShare.None))
        { output.Write(content); output.Flush(true); }
        // 只暂停宿主的整表保存回调，WPF 的集合通知保持有效。
        changed.RemoveEventHandler(groups, handler);
        try
        {
            Reflect.Call(viewModel, "OnDeleteScriptGroup", target);
            if (File.Exists(path)) throw BridgeException.Failed($"宿主没有删除目标文件；备份为 {Path.GetFileName(backup)}。");
            if (groups.Cast<object>().Any(group => string.Equals(Reflect.Get(group, "Name") as string, name, StringComparison.OrdinalIgnoreCase)))
                throw BridgeException.Failed("目标文件已删除，但宿主组列表仍有该组；请核对结果。");
            if (previousSelection is not null && string.Equals(Reflect.Get(previousSelection, "Name") as string, name, StringComparison.OrdinalIgnoreCase))
                Reflect.Set(viewModel, "SelectedScriptGroup", null);
            DetachProjectHandlers(viewModel, target);
        }
        catch
        {
            if (File.Exists(path) && cached.Length == 1 && !groups.Contains(target)) groups.Add(target);
            if (File.Exists(path) && previousSelection is not null) Reflect.Set(viewModel, "SelectedScriptGroup", previousSelection);
            throw;
        }
        finally { changed.AddEventHandler(groups, handler); }
        var user = Directory.GetParent(directory)!.FullName;
        return new { groupName = Reflect.Get(target, "Name"), deleted = true, verified = true,
            path = Path.GetRelativePath(user, path).Replace('\\', '/'),
            backup = Path.GetRelativePath(user, backup).Replace('\\', '/'), previousSha256 = hash,
            resourcesPreserved = true };
    }

    private static void DetachProjectHandlers(object viewModel, object group)
    {
        if (Reflect.Get(group, "Projects") is not IEnumerable projects) return;
        var type = viewModel.GetType();
        if (Reflect.FindMethod(type, "ScriptProjectsCollectionChanged", 2) is { } changed)
            projects.GetType().GetEvent("CollectionChanged")?.RemoveEventHandler(projects,
                changed.CreateDelegate(typeof(NotifyCollectionChangedEventHandler), viewModel));
        if (Reflect.FindMethod(type, "ScriptProjectsPChanged", 2) is { } properties)
            foreach (var project in projects)
                project?.GetType().GetEvent("PropertyChanged")?.RemoveEventHandler(project,
                    properties.CreateDelegate(typeof(PropertyChangedEventHandler), viewModel));
    }
}
