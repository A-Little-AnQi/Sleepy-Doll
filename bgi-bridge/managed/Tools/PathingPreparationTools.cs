using System.Collections;
using System.Text.Json;
using BgiBridge.Bgi;
using BgiBridge.Catalog;
using BgiBridge.Protocol;

namespace BgiBridge.Tools;

/// <summary>把选定父目录交给宿主构造完整配置组，不由模型复制路线内容。</summary>
public static class PathingPreparationTools
{
    public static void Register(MethodRegistry registry)
    {
        const string id = "bgi.prepare_pathing_group";
        var guide = new AgentGuide("准备地图追踪配置组", "使用所选父目录下全部路线建立配置组，保留原文件名和目录；配置使用宿主默认值，不拼接游戏设置。",
            ["资源已订阅或本机已安装，准备执行该完整目标或作者包。"], ["path 是 pathing/ 开始的精确目录路径；无需先启动游戏。"],
            ["只创建一个稳定命名的配置组；已有同名但不同内容的组不覆盖。"],
            "prepared=true 表示完整配置组已落盘；尚未运行。返回 groupName 是运行入口的参数。",
            "随后先确认游戏就绪，再直接 describe/invoke bgi.run_script_group；不要搜索 ViewModel 命令或程序集。",
            "保留现有路线和用户配置，不自动删除。", [], "bridge-stable-operation");
        registry.Register(id, "scheduler", guide.Purpose, Prepare, readOnly: false, destructive: true,
            inputSchema: AgentSchemas.Input(id), guide: guide);
    }

    public static string[] RouteFiles(string userRoot, string path)
    {
        path = ScriptRepositoryReader.NormalizePath(path);
        if (!path.StartsWith("pathing/", StringComparison.Ordinal)) throw BridgeException.InvalidArgument("只接受 pathing/ 父目录。");
        var root = Path.GetFullPath(Path.Combine(userRoot, "AutoPathing"));
        var folder = Path.GetFullPath(Path.Combine(root, path[8..].Replace('/', Path.DirectorySeparatorChar)));
        if (!folder.StartsWith(root + Path.DirectorySeparatorChar, StringComparison.OrdinalIgnoreCase) || !Directory.Exists(folder))
            throw BridgeException.NotFound("所选路线目录未安装，先订阅该父目录。");
        if ((File.GetAttributes(folder) & FileAttributes.ReparsePoint) != 0)
            throw BridgeException.InvalidArgument("路线目录不能是符号链接或目录联接。");
        var options = new EnumerationOptions { RecurseSubdirectories = true, AttributesToSkip = FileAttributes.ReparsePoint };
        var files = Directory.GetFiles(folder, "*.json", options).Order(StringComparer.Ordinal).ToArray();
        if (files.Length == 0) throw BridgeException.NotFound("所选父目录没有地图追踪文件。");
        return files.Select(file => Path.GetRelativePath(root, file)).ToArray();
    }

    private static Task<object?> Prepare(JsonElement arguments, CancellationToken cancellation)
    {
        var path = ScriptRepositoryReader.NormalizePath(arguments.GetProperty("path").GetString()!);
        var userRoot = Path.Combine(AppContext.BaseDirectory, "User");
        var routes = RouteFiles(userRoot, path);
        var name = arguments.TryGetProperty("groupName", out var supplied) ? supplied.GetString()!.Trim()
            : "Sleepy Doll-地图追踪-" + path.Split('/')[^1];
        if (name.Length == 0 || name.Length > 160 || name.IndexOfAny(Path.GetInvalidFileNameChars()) >= 0)
            throw BridgeException.InvalidArgument("配置组名称必须是有效文件名。");
        var groupFile = Path.Combine(userRoot, "ScriptGroup", name + ".json");
        cancellation.ThrowIfCancellationRequested();
        if (File.Exists(groupFile))
        {
            using var existing = JsonDocument.Parse(File.ReadAllText(groupFile));
            var old = existing.RootElement.GetProperty("projects").EnumerateArray().Select(project =>
                Path.Combine(project.GetProperty("folderName").GetString()!, project.GetProperty("name").GetString()!)).ToArray();
            if (!old.SequenceEqual(routes, StringComparer.OrdinalIgnoreCase)
                || existing.RootElement.GetProperty("projects").EnumerateArray().Any(project => project.GetProperty("type").GetString() != "Pathing" || project.GetProperty("status").GetString() != "Enabled"))
                throw new BridgeException("GROUP_CONFLICT", $"已有同名配置组内容不同，未覆盖：{name}", 409);
            return Task.FromResult<object?>(new { prepared = true, reused = true, groupName = name, sourcePath = path, executionScope = "directoryRecursive", routeCount = routes.Length });
        }
        var groupType = Reflect.FindType("BetterGenshinImpact.Core.Script.Group.ScriptGroup")
            ?? throw BridgeException.Missing("宿主没有配置组构造器。");
        var projectType = Reflect.FindType("BetterGenshinImpact.Core.Script.Group.ScriptGroupProject")
            ?? throw BridgeException.Missing("宿主没有地图追踪项目构造器。");
        var group = Activator.CreateInstance(groupType)!;
        Reflect.Set(group, "Name", name);
        var projects = Reflect.Get(group, "Projects") as IList
            ?? throw BridgeException.Missing("宿主配置组项目列表不可用。");
        var index = 0;
        foreach (var route in routes)
        {
            var project = Reflect.CallStatic(projectType, "BuildPathingProject", Path.GetFileName(route), Path.GetDirectoryName(route)!)
                ?? throw BridgeException.Missing("宿主未构造地图追踪项目。");
            Reflect.Set(project, "Index", ++index);
            projects.Add(project);
        }
        cancellation.ThrowIfCancellationRequested();
        Reflect.Call(group, "WriteToFileAtomically", groupFile);
        if (!File.Exists(groupFile)) throw BridgeException.Failed("宿主未保存配置组。");
        return Task.FromResult<object?>(new { prepared = true, reused = false, groupName = name, sourcePath = path, executionScope = "directoryRecursive", routeCount = routes.Length, inheritedHostSettings = true });
    }
}
