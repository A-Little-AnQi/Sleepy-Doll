using System.Text.Json;

namespace BgiBridge.Catalog;

public static class AgentSchemas
{
    public static JsonElement Object(params (string Name, JsonElement Schema, bool Required)[] fields) =>
        JsonSerializer.SerializeToElement(new
        {
            type = "object",
            properties = fields.ToDictionary(field => field.Name, field => field.Schema),
            required = fields.Where(field => field.Required).Select(field => field.Name).ToArray(),
            additionalProperties = false,
        });
    public static JsonElement Text(string description, int max = 256) =>
        JsonSerializer.SerializeToElement(new { type = "string", minLength = 1, maxLength = max, description });
    public static JsonElement Flag(string description) =>
        JsonSerializer.SerializeToElement(new { type = "boolean", description });
    private static JsonElement Limit => ArgumentSchema.Parse("""{"type":"integer","minimum":1,"maximum":100,"default":30,"description":"最多返回条数；默认 30，上限 100。"}""");
    private static JsonElement Path => Text("从设置目录取得的精确点分路径，例如 triggerInterval。");
    private static JsonElement Version => Text("目标设置当前的 valueVersion；不允许省略或猜测。");
    private static JsonElement Value => ArgumentSchema.Parse("""{"description":"新值，必须符合 get_setting 返回的 valueSchema；不得将遮蔽值当作原值提交。"}""");
    public static JsonElement Input(string id) => id switch
    {
        "bgi.ping" or "bgi.probe" or "bgi.get_status" or "bgi.list_setting_sections" or "bgi.list_setting_changes" => ArgumentSchema.Empty,
        "bgi.search_settings" => Object(
            ("terms", ArgumentSchema.Parse("""{"type":"array","maxItems":16,"items":{"type":"string","maxLength":100},"description":"搜索词数组，所有词均需匹配路径或用途说明；空数组表示不限定。"}"""), false),
            ("section", Text("分区名，来自 list_setting_sections。"), false),
            ("valueType", Text("按目录 valueType 过滤，例如 boolean、integer、string。"), false),
            ("writableOnly", Flag("为 true 时仅返回可由事务接口修改的设置。"), false),
            ("limit", Limit, false)),
        "bgi.get_setting" => Object(("path", Path, true)),
        "bgi.set_setting" => Object(("path", Path, true), ("value", Value, true), ("expectedVersion", Version, true)),
        "bgi.preview_settings" => Object(("changes", JsonSerializer.SerializeToElement(new
        {
            type = "array", minItems = 1, maxItems = 20,
            description = "待修改设置，不得包含重复路径。",
            items = Object(("path", Path, true), ("value", Value, true), ("expectedVersion", Version, true)),
        }), true)),
        "bgi.commit_settings" => Object(("planId", Text("preview_settings 返回且尚未过期的计划 ID。"), true)),
        "bgi.get_setting_change" or "bgi.rollback_settings" => Object(("changeId", Text("由 commit_settings、set_setting 或变更记录返回的事务 ID。"), true)),
        "bgi.list_commands" => Object(("filter", Text("命令名或用途关键词。"), false), ("includeDangerous", Flag("是否同时列出有破坏性副作用的命令；不影响调用权限。"), false)),
        "bgi.invoke_command" => Object(("command", Text("来自命令目录的精确 name。"), true), ("argument", ArgumentSchema.Parse("""{"description":"必须符合该命令的 parameterSchema；无参命令省略此字段。"}"""), false)),
        _ => throw new InvalidOperationException($"接口 {id} 缺少参数契约。"),
    };

    private static JsonElement Any(string description) => JsonSerializer.SerializeToElement(new { description });
    private static JsonElement ArrayOf(JsonElement items, string description) =>
        JsonSerializer.SerializeToElement(new { type = "array", items, description });
    private static JsonElement ResultObject(string description, params (string Name, JsonElement Schema, bool Required)[] fields) =>
        JsonSerializer.SerializeToElement(new
        {
            type = "object", description,
            properties = fields.ToDictionary(field => field.Name, field => field.Schema),
            required = fields.Where(field => field.Required).Select(field => field.Name).ToArray(),
            additionalProperties = true,
        });
    private static JsonElement Setting => ResultObject("当前设置值与写入契约",
        ("path", Text("精确配置路径。"), true),
        ("description", Text("字段用途。", 16384), true),
        ("valueType", Text("可读值类型。"), true),
        ("currentValue", Any("当前值；敏感字段为遮蔽字符串。"), true),
        ("valueSchema", Any("新值需满足的 JSON Schema；不支持安全写入时为 null。"), false),
        ("valueVersion", Text("当前桥实例内的值版本；重启后需重新读取。"), true),
        ("writable", Flag("是否允许使用事务接口修改。"), true),
        ("writeRestriction", Any("不可写的原因，可为 null。"), false),
        ("defaultValueKnown", Flag("是否存在可验证的字面量默认值。"), true),
        ("sensitive", Flag("当前值是否已遮蔽。"), true));
    public static JsonElement Output(string id, string description)
    {
        if (id.StartsWith("setting.") || id == "bgi.get_setting") return Setting;
        return id switch
        {
            "bgi.ping" => ResultObject(description, ("ok", Flag("桥请求处理成功。"), true), ("hostLoaded", Flag("宿主程序集可见。"), true), ("at", Text("观测时间，ISO 8601。"), true)),
            "bgi.get_status" => ResultObject(description, ("ready", Flag("当前状态检查是否就绪。"), true), ("runtime", Any("截图、窗口和独立任务状态；未能观测的项必须保留未知。"), true), ("observedAt", Text("ISO 8601 观测时间。"), true)),
            "bgi.probe" => ResultObject(description, ("hostLoaded", Flag("宿主程序集已加载。"), true), ("uiDispatcherAvailable", Flag("能否在宿主 UI 线程调度。"), true), ("serviceProviderReachable", Flag("服务容器是否可达。"), true)),
            "bgi.search_settings" => ResultObject(description, ("total", ArgumentSchema.Parse("""{"type":"integer","description":"匹配总数。"}"""), true), ("items", ArrayOf(Setting, "匹配的设置契约。"), true)),
            "bgi.list_setting_sections" => ResultObject(description, ("sections", ArrayOf(Any("name、description、settingCount、writableCount、sensitiveCount。"), "设置分区摘要。"), true)),
            "bgi.preview_settings" => ResultObject(description, ("planId", Text("绑定当前值和文件版本的限时计划。"), true), ("expiresAt", Text("ISO 8601 到期时间。"), true), ("changesConfig", Flag("始终为 false，预览未修改配置。"), true), ("differences", ArrayOf(Any("path、before、after、rollback；敏感值遮蔽。"), "预期差异。"), true)),
            "bgi.commit_settings" or "bgi.set_setting" or "bgi.rollback_settings" or "bgi.get_setting_change" => ResultObject(description, ("changeId", Text("变更记录 ID。"), true), ("state", Text("事务状态。"), true), ("verified", Flag("本次操作的配置核验结果，不等于未来状态保持不变。"), true), ("backupFile", Text("受保护恢复记录文件路径。", 32767), false), ("differences", ArrayOf(Any("path、before、after。"), "脱敏差异。"), false)),
            "bgi.list_setting_changes" => ResultObject(description, ("changes", ArrayOf(Any("changeId、state、createdAt、verified、backupFile、differences。"), "最近的配置变更和命令前检查点。"), true)),
            "bgi.list_commands" => ResultObject(description, ("count", ArgumentSchema.Parse("""{"type":"integer"}"""), true), ("commands", ArrayOf(Any("name、guide、parameterSchema、unavailableReason、requiresConfirmation、isDestructive。"), "宿主命令契约。"), true)),
            _ => ResultObject(description, ("command", Text("实际命令名。"), true), ("executed", Flag("命令处理器已返回；不是业务成功标记。"), true), ("configurationCheckpoint", Any("执行前的配置备份记录，可用于离线恢复。"), true)),
        };
    }
}
