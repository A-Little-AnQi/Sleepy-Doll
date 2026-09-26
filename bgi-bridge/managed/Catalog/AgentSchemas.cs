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
    private static JsonElement Date => Text("宿主日志的日期，yyyyMMdd 或 yyyy-MM-dd；省略为当天。");
    private static JsonElement Version => Text("目标设置当前的 valueVersion；不允许省略或猜测。");
    private static JsonElement Value => ArgumentSchema.Parse("""{"description":"新值，必须符合 get_setting 返回的 valueSchema；不得将遮蔽值当作原值提交。"}""");
    public static JsonElement Input(string id) => id switch
    {
        "bgi.ping" or "bgi.probe" or "bgi.get_status" or "bgi.start_game" or "bgi.stop_current_task" or "bgi.list_pages" or "bgi.list_setting_sections" or "bgi.list_setting_changes" => ArgumentSchema.Empty,
        "bgi.open_page" => Object(("page", JsonSerializer.SerializeToElement(new { type = "string", @enum = Tools.NavigationTools.Pages.Select(page => page.Id).ToArray(), description = "来自 bgi.list_pages 的页面标识。" }), true)),
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
        "bgi.run_script_group" => Object(("groupName", Text("从 User/ScriptGroup 配置文件读取到的精确 name。"), true)),
        "bgi.delete_script_group" => Object(("groupName", Text("从 User/ScriptGroup 读取到的精确 name；只删除该配置组。"), true),
            ("expectedSha256", ArgumentSchema.Parse("""{"type":"string","pattern":"^[0-9a-fA-F]{64}$","description":"bgi.user.read 返回的目标配置组 SHA-256，不允许猜测。"}"""), true)),
        "bgi.subscribe_script_resources" => Object(("paths", ArgumentSchema.Parse("""{"type":"array","minItems":1,"maxItems":8,"uniqueItems":true,"items":{"type":"string","minLength":1,"maxLength":1024},"description":"来自仓库结果的精确完整目录或脚本路径，如 pathing/地方特产/稻妻/血斛/作者包。"}"""), true)),
        "bgi.prepare_pathing_group" => Object(("path", Text("已安装的完整父目录，保留 pathing/ 前缀。", 1024), true), ("groupName", Text("可选稳定名称；留空由桥生成，不覆盖已有不同内容配置组。", 160), false)),
        "bgi.prepare_js_group" => Object(("folderName", Text("已安装 JsScript 的单层目录名。"), true),
            ("groupName", Text("可选稳定配置组名称。",160), false), ("settings", ArgumentSchema.Parse("""{"type":"object","maxProperties":100,"description":"来自脚本 settingsUi 定义的参数值；省略时沿用脚本默认值。"}"""), false)),
        "bgi.update_subscribed_scripts" => ArgumentSchema.Parse("""{"type":"object","properties":{"mode":{"type":"string","enum":["repositoryOnly","selected","all"],"description":"repositoryOnly 只刷新中央仓库；selected 只更新 paths；all 更新全部当前订阅。"},"paths":{"type":"array","minItems":1,"maxItems":100,"uniqueItems":true,"items":{"type":"string","minLength":1,"maxLength":512},"description":"mode=selected 时必填；值来自 User/Subscriptions，例如 js/AutoHoeingOneDragon。"}},"required":["mode"],"additionalProperties":false}"""),
        "bgi.search_script_repository" => Object(
            ("query", Text("脚本标题或一个功能词；可包含原始问题中的脚本名称。", 200), true),
            ("category", ArgumentSchema.Parse("""{"type":"string","enum":["all","js","pathing","combat","tcg"],"default":"all"}"""), false),
            ("offset", ArgumentSchema.Parse("""{"type":"integer","minimum":0,"default":0}"""), false),
            ("limit", ArgumentSchema.Parse("""{"type":"integer","minimum":1,"maximum":20,"default":8}"""), false)),
        "bgi.read_script_repository_file" => Object(
            ("path", Text("仓库内容相对路径，来自搜索结果，如 js/脚本目录/settings.json。", 1024), true),
            ("startLine", ArgumentSchema.Parse("""{"type":"integer","minimum":1,"default":1}"""), false),
            ("maxLines", ArgumentSchema.Parse("""{"type":"integer","minimum":1,"maximum":240,"default":160}"""), false),
            ("contains", Text("定位参数、函数或导入模块，返回命中行与周围上下文。", 200), false)),
        "bgi.read_host_log" => Object(
            ("date", Date, false),
            ("level", Text("日志级别，例如 ERR、WRN、INF、DBG；省略为全部级别。"), false),
            ("logger", Text("记录器名的一部分，例如 ScriptService；省略为全部记录器。"), false),
            ("contains", Text("消息正文包含的文本，用报错原文。"), false),
            ("thread", Text("线程号，例如 T1789426568302 或其中的数字；用于只看一次运行。"), false),
            ("limit", Limit, false)),
        "bgi.get_script_errors" => Object(("date", Date, false), ("limit", Limit, false)),
        "bgi.list_commands" => Object(("filter", Text("命令名或用途关键词。"), false), ("includeDangerous", Flag("是否同时列出有破坏性副作用的命令；不影响调用权限。"), false)),
        "bgi.invoke_command" => Object(("command", Text("来自命令目录的精确 name。"), true), ("argument", ArgumentSchema.Parse("""{"description":"必须符合该命令的 parameterSchema；无参命令省略此字段。"}"""), false),
            ("contextId",Text("真实上下文引用。",64),false), ("selection",ArgumentSchema.Parse("""{"type":"object","maxProperties":32}"""),false), ("dialogInput", Bgi.NativeDialogScope.Schema,false)),
        _ => throw new InvalidOperationException($"接口 {id} 缺少参数契约。"),
    };

    private static JsonElement Any(string description) => JsonSerializer.SerializeToElement(new { description });
    private static JsonElement Integer(string description) =>
        JsonSerializer.SerializeToElement(new { type = "integer", description });
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
            "bgi.ping" => ResultObject(description, ("ok", Flag("桥请求处理成功。"), true), ("hostLoaded", Flag("BetterGI 程序集可见。"), true), ("at", Text("观测时间，ISO 8601。"), true)),
            "bgi.get_status" => ResultObject(description, ("ready", Flag("当前状态检查是否就绪。"), true), ("runtime", Any("截图、窗口和独立任务状态；未能观测的项必须保留未知。"), true), ("observedAt", Text("ISO 8601 观测时间。"), true)),
            "bgi.stop_current_task" => ResultObject(description, ("stopped", Flag("宿主任务锁是否已释放。"), true), ("outcome", Text("stopped、alreadyIdle 或 timeout。"), true)),
            "bgi.list_pages" => ResultObject(description, ("pages", ArrayOf(Any("id、title、available。"), "主窗口页面。"), true)),
            "bgi.open_page" => ResultObject(description, ("page", Text("目标页面标识。"), true), ("opened", Flag("窗口是否可见且导航已选择目标页面。"), true), ("verified", Flag("导航选择和窗口可见性已复查。"), true)),
            "bgi.wait_ready" => ResultObject(description, ("ready", Flag("游戏是否已在主界面就绪。"), true), ("outcome", Text("ready、resolution、notRunning 或 timeout。"), true), ("elapsedMs", ArgumentSchema.Parse("""{"type":"integer"}"""), true), ("observedAt", Text("ISO 8601 观测时间。"), true), ("runtime", Any("本次观测到的游戏与截图器状态。"), true)),
            "bgi.set_game_resolution" => ResultObject(description, ("written", Flag("目标游戏的显示记录是否写入。"), true), ("width", ArgumentSchema.Parse("""{"type":"integer"}"""), true), ("height", ArgumentSchema.Parse("""{"type":"integer"}"""), true), ("mode", Text("写入的窗口模式。"), true), ("game", Text("实际修改的国服或国际服记录。"), true)),
            "bgi.start_game" => ResultObject(description, ("started", Flag("本次是否发出了启动。"), true), ("alreadyRunning", Flag("BetterGI 已经在运行，未重复启动。"), true), ("ready", Flag("返回时是否已可截图并执行游戏动作。"), true), ("stillLoading", Flag("为 true 表示原神仍在加载，用 get_status 继续等，不是失败。"), true), ("elapsedMs", ArgumentSchema.Parse("""{"type":"integer","description":"本次等待的毫秒数。"}"""), true), ("note", Text("需要用户知道的一句话说明。"), true), ("runtime", Any("启动后的截图、窗口和独立任务状态。"), true)),
            "bgi.probe" => ResultObject(description, ("hostLoaded", Flag("BetterGI 程序集已加载。"), true), ("uiDispatcherAvailable", Flag("能否在 BetterGI 界面线程调度。"), true), ("serviceProviderReachable", Flag("服务容器是否可达。"), true)),
            "bgi.search_settings" => ResultObject(description, ("total", ArgumentSchema.Parse("""{"type":"integer","description":"匹配总数。"}"""), true), ("items", ArrayOf(Setting, "匹配的设置契约。"), true)),
            "bgi.list_setting_sections" => ResultObject(description, ("sections", ArrayOf(Any("name、description、settingCount、writableCount、sensitiveCount。"), "设置分区摘要。"), true)),
            "bgi.preview_settings" => ResultObject(description, ("planId", Text("绑定当前值和文件版本的限时计划。"), true), ("expiresAt", Text("ISO 8601 到期时间。"), true), ("changesConfig", Flag("始终为 false，预览未修改配置。"), true), ("differences", ArrayOf(Any("path、before、after、rollback；敏感值遮蔽。"), "预期差异。"), true)),
            "bgi.commit_settings" or "bgi.set_setting" or "bgi.rollback_settings" or "bgi.get_setting_change" => ResultObject(description, ("changeId", Text("变更记录 ID。"), true), ("state", Text("事务状态。"), true), ("verified", Flag("本次操作的配置核验结果，不等于未来状态保持不变。"), true), ("backupFile", Text("受保护恢复记录文件路径。", 32767), false), ("differences", ArrayOf(Any("path、before、after。"), "脱敏差异。"), false)),
            "bgi.list_setting_changes" => ResultObject(description, ("changes", ArrayOf(Any("changeId、state、createdAt、verified、backupFile、differences。"), "最近的配置变更和命令前检查点。"), true)),
            "bgi.subscribe_script_resources" => ResultObject(description, ("installed", Flag("目标资源已存在。"), true), ("paths", ArrayOf(Text("精确仓库路径。",1024),"所选资源。"),true)),
            "bgi.prepare_pathing_group" => ResultObject(description, ("prepared", Flag("配置组已保存。"),true), ("groupName",Text("运行入口参数。"),true), ("executionScope",Text("directoryRecursive。"),true), ("routeCount",Integer("完整目录实际路线数，不是模型挑选条数。"),true)),
            "bgi.prepare_js_group" => ResultObject(description, ("prepared", Flag("宿主 JS 配置组已保存。"),true), ("groupName",Text("实际运行入口参数。"),true), ("reused",Flag("是否复用相同内容配置组。"),true)),
            "bgi.search_script_repository" => ResultObject(description,
                ("source", Text("centralRepository；不是 User 中已订阅副本。"), true),
                ("total", Integer("匹配数。"), true), ("items", ArrayOf(Any("path、name、version、description、namedInQuery。"), "候选脚本。"), true),
                ("nextOffset", Any("下一页位置，无下一页为 null。"), false)),
            "bgi.read_script_repository_file" => ResultObject(description,
                ("source", Text("centralRepository。"), true), ("path", Text("实际读取路径。", 1024), true),
                ("sha256", Text("完整文本的 SHA-256。"), true), ("totalLines", Integer("文件总行数。"), true),
                ("lines", ArrayOf(Any("line 为 1 起始的行号，text 为该行完整内容。"), "有序文本行。"), true),
                ("truncated", Flag("是否还有未返回内容。"), true), ("nextLine", Any("下一段起始行，无下一段为 null。"), false)),
            "bgi.read_host_log" => ResultObject(description,
                ("date", Text("实际读取的日志日期。"), true),
                ("exists", Flag("该日期是否存在日志文件。"), true),
                ("file", Text("日志文件路径。", 512), true),
                ("fileBytes", Integer("日志文件总字节数。"), true),
                ("scannedBytes", Integer("本次实际读取的尾部字节数。"), true),
                ("headTruncated", Flag("为 true 时只读取了文件尾部，更早的记录未读取。"), true),
                ("entriesSeen", Integer("读取范围内解析出的记录总数。"), true),
                ("matched", Integer("命中筛选条件的记录数。"), true),
                ("entries", ArrayOf(Any("time、level、thread、logger、message、truncated。"), "命中的记录，按时间升序。"), true),
                ("availableDates", ArrayOf(Text("日志日期，yyyyMMdd。"), "本机已有的宿主日志日期，最新在前。"), true),
                ("note", Any("需要调用方知道的一句话说明，可为 null。"), false)),
            "bgi.get_script_errors" => ResultObject(description,
                ("date", Text("实际读取的日志日期。"), true),
                ("exists", Flag("该日期是否存在日志文件。"), true),
                ("file", Text("日志文件路径。", 512), true),
                ("fileBytes", Integer("日志文件总字节数。"), true),
                ("headTruncated", Flag("为 true 时只读取了文件尾部，更早的记录未读取。"), true),
                ("count", Integer("该日期记录的脚本失败总数。"), true),
                ("failures", ArrayOf(Any("time、level、thread、script、error、jsError、locations、context。"), "脚本失败，按时间升序。"), true),
                ("availableDates", ArrayOf(Text("日志日期，yyyyMMdd。"), "本机已有的宿主日志日期，最新在前。"), true),
                ("note", Any("需要调用方知道的一句话说明，可为 null。"), false)),
            "bgi.list_commands" => ResultObject(description, ("count", ArgumentSchema.Parse("""{"type":"integer"}"""), true), ("commands", ArrayOf(Any("name、guide、parameterSchema、unavailableReason、requiresConfirmation、isDestructive。"), "界面命令契约。"), true)),
            "bgi.run_script_group" => ResultObject(description, ("groupName", Text("从磁盘配置解析并交给 BetterGI 的准确名称。"), true), ("resolved", Flag("是否唯一定位到目标配置组。"), true), ("executed", Flag("BetterGI 按名称执行方法是否已返回；不是整组任务完成标记。"), true)),
            "bgi.delete_script_group" => ResultObject(description, ("groupName", Text("已删除配置组的精确名称。"), true), ("deleted", Flag("文件与宿主列表均已删除。"), true),
                ("verified", Flag("删除结果已复查。"), true), ("path", Text("User 下的原配置组文件路径。", 1024), true), ("backup", Text("User 下的原始内容备份，脚本与路线保留。", 1024), true),
                ("previousSha256", Text("删除前文件的 SHA-256。"), true), ("resourcesPreserved", Flag("脚本、路线、订阅资源未删除。"), true)),
            "bgi.update_subscribed_scripts" => ResultObject(description, ("mode", Text("实际执行的更新范围。"), true), ("repositoryChanged", Any("中央仓库是否拉到新内容；all 模式可能无法单独报告。"), false), ("updatedPaths", ArrayOf(Text("交给 BetterGI 更新的精确订阅路径。", 512), "selected/all 模式涉及的订阅路径。"), true), ("completed", Flag("BetterGI 更新函数已返回；仍需回读目标文件验证内容。"), true)),
            "bgi.list_command_targets" => ResultObject(description, ("ownerType", Text("原生上下文类型。"), true), ("contexts", ArrayOf(Any("当前 contextId/objectId、类型和显示名称。"), "上下文。"), true), ("arguments", ArrayOf(Any("真实参数对象引用。"), "参数对象。"), true), ("constructors", ArrayOf(Any("构造参数契约。"), "构造器。"), true)),
            "bgi.create_command_target" or "bgi.create_command_argument" => ResultObject(description, ("created", Flag("对象已构造，不表示命令已执行。"), true), ("target", Any("真实 objectId。"), true), ("verified", Flag("上下文建立已核验。"), true)),
            "bgi.release_command_target" => ResultObject(description, ("released", Flag("桥创建的上下文已释放。"), true), ("verified", Flag("已核验。"), true)),
            _ => ResultObject(description, ("command", Text("实际命令名。"), true), ("executed", Flag("命令处理器已返回；不是业务成功标记。"), true), ("configurationCheckpoint", Any("执行前的配置备份记录，可用于离线恢复。"), true)),
        };
    }
}
