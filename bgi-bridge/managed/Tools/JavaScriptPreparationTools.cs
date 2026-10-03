using System.Collections;
using System.Dynamic;
using System.Text.Json;
using BgiBridge.Bgi;
using BgiBridge.Catalog;
using BgiBridge.Protocol;

namespace BgiBridge.Tools;

public static class JavaScriptPreparationTools
{
    public static void Register(MethodRegistry registry)
    {
        const string id = "bgi.prepare_js_group";
        var guide = new AgentGuide("准备 JS 配置组", "用已安装 JS 脚本的宿主构造器与真实设置定义建立单任务配置组，不执行脚本。",
            ["用户要求运行已安装或刚订阅的 JS，且没有可复用配置组时。"],
            ["folderName 来自 JsScript；先读取 manifest、settingsUi 和 README；settings 只填写已定义参数。groupName 依据用户目标直接取简短自然名，如“莉奈娅挖矿”“兽怪暴徒”；不拼目录、作者、时间或参数长句，不为命名再次提问，缺省时自动生成。"],
            ["新建一个配置组，已有不同内容的同名组不覆盖。"], "prepared=true 返回实际 groupName，之后使用 run_script_group。",
            "prepared 只表示配置准备完成，用户要求运行时继续执行。", "可删除生成的配置组，脚本保留。", [], "bridge-stable-operation");
        registry.Register(id, "scheduler", guide.Purpose, Prepare, readOnly: false, guide: guide);
    }

    private static Task<object?> Prepare(JsonElement arguments, CancellationToken cancellation) => Ui.InvokeAsync<object?>(() =>
    {
        cancellation.ThrowIfCancellationRequested();
        var folder = arguments.GetProperty("folderName").GetString()!;
        if (folder.Length == 0 || folder.IndexOfAny(Path.GetInvalidFileNameChars()) >= 0 || folder is "." or "..")
            throw BridgeException.InvalidArgument("folderName 必须是已安装脚本的单层目录名。");
        var root = Path.Combine(AppContext.BaseDirectory, "User");
        var scriptPath = Path.Combine(root, "JsScript", folder);
        if (!Directory.Exists(scriptPath) || (File.GetAttributes(scriptPath) & FileAttributes.ReparsePoint) != 0)
            throw BridgeException.NotFound("目标 JS 未安装或目录不可直接使用。");
        var projectType = Reflect.FindType("BetterGenshinImpact.Core.Script.Project.ScriptProject") ?? throw BridgeException.Missing("宿主 JS 构造器不可用。");
        var script = Activator.CreateInstance(projectType, folder)!;
        var manifest = Reflect.Get(script, "Manifest")!;
        var definitions = Reflect.Call(manifest, "LoadSettingItems", scriptPath) as IEnumerable ?? throw BridgeException.Missing("脚本设置定义不可用。");
        var fields = definitions.Cast<object>().Where(item => !string.IsNullOrWhiteSpace(Reflect.Get(item, "Name") as string))
            .ToDictionary(item => (string)Reflect.Get(item, "Name")!, StringComparer.Ordinal);
        IDictionary<string, object?> settings = new ExpandoObject();
        foreach (var (key, definition) in fields)
        {
            if (Reflect.Get(definition, "Default") is { } defaultValue)
            {
                var value = JsonSerializer.SerializeToElement(defaultValue);
                if (value.ValueKind != JsonValueKind.Null) settings[key] = SettingValue(definition, value);
            }
        }
        if (arguments.TryGetProperty("settings", out var supplied))
            foreach (var property in supplied.EnumerateObject())
            {
                if (!fields.TryGetValue(property.Name, out var definition)) throw BridgeException.InvalidArgument($"脚本没有定义参数 {property.Name}。");
                settings[property.Name] = SettingValue(definition, property.Value);
            }
        var groupType = Reflect.FindType("BetterGenshinImpact.Core.Script.Group.ScriptGroup") ?? throw BridgeException.Missing("配置组构造器不可用。");
        var taskType = Reflect.FindType("BetterGenshinImpact.Core.Script.Group.ScriptGroupProject") ?? throw BridgeException.Missing("配置组项目构造器不可用。");
        var group = Activator.CreateInstance(groupType)!;
        var manifestName = Reflect.Get(manifest, "Name") as string;
        var name = arguments.TryGetProperty("groupName", out var selected) ? selected.GetString()!
            : GroupNaming.Default(string.IsNullOrWhiteSpace(manifestName) ? folder : manifestName, "脚本任务");
        if (name.Length == 0 || name.Length > 160 || name.IndexOfAny(Path.GetInvalidFileNameChars()) >= 0)
            throw BridgeException.InvalidArgument("groupName 必须是有效文件名。");
        var file = Path.Combine(root, "ScriptGroup", name + ".json");
        if (File.Exists(file))
        {
            using var old = JsonDocument.Parse(File.ReadAllText(file));
            var projects = old.RootElement.GetProperty("projects").EnumerateArray().ToArray();
            if (projects.Length != 1 || projects[0].GetProperty("type").GetString() != "Javascript"
                || projects[0].GetProperty("folderName").GetString() != folder || projects[0].GetProperty("status").GetString() != "Enabled"
                || !projects[0].TryGetProperty("jsScriptSettingsObject", out var prior)
                || ValueContract.Canonical(prior) != ValueContract.Canonical(JsonSerializer.SerializeToElement(settings)))
                throw new BridgeException("GROUP_CONFLICT", "已有同名组内容不同，未覆盖。", 409);
            return new { prepared = true, reused = true, groupName = name };
        }
        Reflect.Set(group, "Name", name);
        var task = Activator.CreateInstance(taskType, script)!;
        Reflect.Set(task, "Index", 1);
        Reflect.Set(task, "JsScriptSettingsObject", settings);
        ((IList)Reflect.Get(group, "Projects")!).Add(task);
        cancellation.ThrowIfCancellationRequested();
        Reflect.Call(group, "WriteToFileAtomically", file);
        if (!File.Exists(file)) throw BridgeException.Failed("配置组未保存。");
        return new { prepared = true, reused = false, groupName = name };
    });

    private static object? SettingValue(object definition, JsonElement value)
    {
        var kind = Reflect.Get(definition, "Type") as string;
        var options = (Reflect.Get(definition, "Options") as IEnumerable)?.Cast<object>().Select(item => item.ToString()!).ToArray();
        if (kind == "checkbox" && value.ValueKind is JsonValueKind.True or JsonValueKind.False) return value.GetBoolean();
        if (kind == "input-text" && value.ValueKind is JsonValueKind.String or JsonValueKind.Number) return value.ToString();
        if (kind == "select" && value.ValueKind == JsonValueKind.String && (options is null || options.Contains(value.GetString()))) return value.GetString();
        if (kind == "multi-checkbox" && value.ValueKind == JsonValueKind.Array
            && value.EnumerateArray().All(item => item.ValueKind == JsonValueKind.String && (options is null || options.Contains(item.GetString()))))
            return value.EnumerateArray().Select(item => item.GetString()!).ToList();
        if (kind == "cascade-select" && value.ValueKind == JsonValueKind.String
            && Reflect.Get(definition, "CascadeOptions") is IDictionary cascade
            && cascade.Values.Cast<IEnumerable>().SelectMany(group => group.Cast<object>()).Any(option => option.ToString() == value.GetString()))
            return value.GetString();
        throw BridgeException.InvalidArgument($"参数 {Reflect.Get(definition, "Name")} 的值不符合脚本定义（{kind}）。");
    }
}
