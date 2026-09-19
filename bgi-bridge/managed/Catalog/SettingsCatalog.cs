using System.Collections;
using System.ComponentModel;
using System.Reflection;
using System.Text.Json;
using System.Text.Json.Serialization;
using System.Xml.Linq;
using BgiBridge.Bgi;

namespace BgiBridge.Catalog;

/// <summary>一个可读写的设置叶子项。</summary>
public sealed record SettingEntry(
    string Path,
    string Section,
    string Property,
    string ValueType,
    string Description,
    string DescriptionSource,
    bool Writable,
    bool Sensitive,
    bool Collection,
    string[]? AllowedValues,
    Dictionary<string, string>? AllowedValueDescriptions,
    object? CurrentValue,
    object? DefaultValue,
    Type ClrType)
{
    public JsonElement? ValueSchema { get; init; }
    public string ValueVersion { get; init; } = "";
    public string? WriteRestriction { get; init; }
    public string? SourceReference { get; init; }
    public bool DefaultValueKnown { get; init; }
    public string? ReadError { get; init; }
}

public sealed record SettingSection(string Name, string Description, int SettingCount, int WritableCount, int SensitiveCount);

/// <summary>设置目录：反射遍历 AllConfig 对象图，每个叶子暴露成 setting.&lt;路径&gt;。</summary>
public static class SettingsCatalog
{
    private const int MaxDepth = 14;

    /// <summary>路径里出现这些词就当敏感，读取时遮蔽。</summary>
    private static readonly string[] SensitiveWords =
    [
        "password", "token", "secret", "cookie", "authorization", "credential", "webhook",
        "accesskey", "privatekey", "devicekey", "sendkey", "apikey", "endpoint", "url",
    ];

    private static readonly Lazy<Dictionary<string, string>> XmlDocs = new(LoadXmlDocs);

    /// <summary>分区中文说明；查不到退回默认文案。</summary>
    private static readonly Dictionary<string, string> SectionDescriptions = new(StringComparer.OrdinalIgnoreCase)
    {
        ["autoArtifactSalvageConfig"] = "自动分解圣遗物的星级、套装过滤和脚本规则。",
        ["autoBossConfig"] = "自动首领讨伐的名称、次数、队伍和领奖设置。",
        ["autoCookConfig"] = "自动烹饪的配方与次数。",
        ["autoDomainConfig"] = "自动秘境的秘境选择、队伍与树脂使用。",
        ["autoEatConfig"] = "自动吃东西的触发条件与食物选择。",
        ["autoFightConfig"] = "自动战斗的策略名与战斗行为。",
        ["autoFishingConfig"] = "自动钓鱼的鱼饵、时间策略与识别参数。",
        ["autoGeniusInvokationConfig"] = "自动七圣召唤的策略与对局行为。",
        ["autoLeyLineOutcropConfig"] = "自动地脉花的国家、花型、次数与树脂。",
        ["autoMusicGameConfig"] = "自动音游与专辑的曲目与判定设置。",
        ["autoPickConfig"] = "自动拾取的开关、黑白名单与交互方式。",
        ["autoRedeemCodeConfig"] = "自动兑换码的启用与兑换行为。",
        ["autoSkipConfig"] = "自动跳过剧情的开关与选项。",
        ["autoStygianOnslaughtConfig"] = "自动幽境危战的难度、队伍与轮数。",
        ["autoWoodConfig"] = "自动伐木的轮数与每日上限。",
        ["childSessionConfig"] = "多实例分身会话的窗口与隔离设置。",
        ["commonConfig"] = "通用行为：截图保存、任务结束动作等。",
        ["devConfig"] = "开发调试开关。",
        ["genshinStartConfig"] = "原神启动路径与启动参数。",
        ["getGridIconsConfig"] = "背包网格图标采集与测试参数。",
        ["hardwareAccelerationConfig"] = "硬件加速与推理后端选择。",
        ["hotKeyConfig"] = "全局热键绑定。",
        ["keyBindingsConfig"] = "游戏内按键映射。",
        ["macroConfig"] = "宏录制与回放。",
        ["mapMaskConfig"] = "地图遮罩的显示与样式。",
        ["maskWindowConfig"] = "遮罩窗口的位置、透明度与显示项。",
        ["musicConfig"] = "背景音乐播放设置。",
        ["notificationConfig"] = "通知渠道与触发事件。",
        ["otherConfig"] = "其他杂项：失去焦点时恢复、日志等。",
        ["pathingConditionConfig"] = "地图追踪的条件判断与超时。",
        ["quickTeleportConfig"] = "快速传送的候选与确认行为。",
        ["recordConfig"] = "录制相关设置。",
        ["scriptConfig"] = "脚本调度：自动更新订阅、脚本仓库等。",
        ["skillCdConfig"] = "技能冷却提示的显示。",
        ["tpConfig"] = "传送的行为参数。",
        ["root"] = "顶层设置。",
    };

    /// <summary>每次调用重新反射，反映宿主当前的配置结构。</summary>
    public static List<SettingEntry> Build(object? suppliedRoot = null)
    {
        var current = suppliedRoot ?? Host.AllConfigInstance();
        if (current is null) return [];

        var result = new List<SettingEntry>();
        Walk(current, null, current.GetType(), "", "root", result, [], 0);
        result.Sort((a, b) => string.Compare(a.Path, b.Path, StringComparison.OrdinalIgnoreCase));
        return result;
    }

    private static void Walk(
        object? current, object? defaults, Type type, string prefix, string section,
        List<SettingEntry> result, HashSet<Type> ancestors, int depth)
    {
        // 深度上限 + 同一路径上的类型去重，防止配置图里出现环。
        if (depth > MaxDepth || !ancestors.Add(type)) return;

        foreach (var property in type.GetProperties(BindingFlags.Instance | BindingFlags.Public))
        {
            if (property.GetMethod is null) continue;                       // 只写的跳过
            if (property.GetIndexParameters().Length != 0) continue;        // 索引器跳过
            if (property.GetCustomAttribute<JsonIgnoreAttribute>() is not null) continue;
            if (typeof(Delegate).IsAssignableFrom(property.PropertyType)) continue;

            var jsonName = property.GetCustomAttribute<JsonPropertyNameAttribute>()?.Name
                           ?? JsonNamingPolicy.CamelCase.ConvertName(property.Name);
            var path = prefix.Length == 0 ? jsonName : $"{prefix}.{jsonName}";

            var underlying = Nullable.GetUnderlyingType(property.PropertyType) ?? property.PropertyType;
            // 只有顶层且非叶子的属性才开新分区，其余继承父级。
            var currentSection = prefix.Length == 0 && !IsLeaf(underlying) ? jsonName : section;

            if (!IsLeaf(underlying))
            {
                Walk(SafeGet(property, current), SafeGet(property, defaults), underlying, path,
                     currentSection, result, [.. ancestors], depth + 1);
                continue;
            }

            var (description, source) = Describe(type, property, currentSection, path);
            var sensitive = IsSensitive(path) && underlying != typeof(bool) && !underlying.IsEnum;
            var documentation = SourceDocumentation.Find("P", type, property.Name);
            var schema = ValueContract.Schema(property.PropertyType, property, path);
            object? currentValue = null;
            string? readError = current is null ? "父级配置对象尚未初始化。" : null;
            if (current is not null)
                try { currentValue = property.GetValue(current); }
                catch { readError = "BetterGI getter 读取失败，当前值未知。"; }
            JsonElement rawValue;
            try { rawValue = ValueContract.Snapshot(currentValue, property.PropertyType); }
            catch { rawValue = ArgumentSchema.Parse("null"); readError = "此值无法安全序列化，当前值未知。"; }
            sensitive |= ContainsSensitiveMembers(rawValue);
            object? defaultValue = null;
            var defaultKnown = false;
            if (documentation?.Initial is { } initial)
                try { defaultValue = ArgumentSchema.Parse(initial); defaultKnown = true; } catch { }
            // BetterGI 生成的变更钩子可能连带修改兄弟字段或文件。
            var customHook = documentation?.HasCustomChangeHook == true
                || property.DeclaringType?.GetMethods(BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic)
                    .Any(method => method.Name == $"On{property.Name}Changed") == true;
            var writable = property.SetMethod?.IsPublic == true && schema is not null && readError is null && !customHook;
            result.Add(new SettingEntry(
                path,
                currentSection,
                property.Name,
                FriendlyType(property.PropertyType),
                description,
                source,
                writable,
                sensitive,
                typeof(IEnumerable).IsAssignableFrom(underlying) && underlying != typeof(string),
                underlying.IsEnum ? Enum.GetNames(underlying) : SourceDocumentation.StringEnum(documentation),
                underlying.IsEnum ? EnumDescriptions(underlying) : null,
                sensitive ? "***REDACTED***" : rawValue,
                sensitive ? null : defaultValue,
                property.PropertyType)
            {
                ValueSchema = schema,
                ValueVersion = readError is null ? ValueContract.Version(rawValue) : "",
                WriteRestriction = readError ?? (writable ? null : customHook ? "BetterGI 包含联动变更处理器；尚无覆盖其连带副作用的事务适配，桥仅开放读取。"
                    : property.SetMethod?.IsPublic != true ? "BetterGI 未提供公共 setter，只能读取。" : "复合类型缺少安全 JSON 写入契约，只能读取；不得重建 BetterGI 对象。"),
                SourceReference = documentation is null ? null : $"{documentation.Source}:{documentation.Line}",
                DefaultValueKnown = defaultKnown,
                ReadError = readError,
            });
        }
    }

    /// <summary>集合与 System.* 视为叶子，不展开成 arr.0.xxx 路径。</summary>
    private static bool IsLeaf(Type type) =>
        type.IsValueType
        || type == typeof(string)
        || typeof(IEnumerable).IsAssignableFrom(type)
        || type.Namespace?.StartsWith("System", StringComparison.Ordinal) == true;

    private static object? SafeGet(PropertyInfo property, object? owner)
    {
        if (owner is null) return null;
        try
        {
            return property.GetValue(owner);
        }
        catch
        {
            // getter 可能被其他线程访问；单项取不到不影响整张目录。
            return null;
        }
    }

    public static bool IsSensitive(string pathOrName) =>
        SensitiveWords.Any(word => pathOrName.Contains(word, StringComparison.OrdinalIgnoreCase));

    private static bool ContainsSensitiveMembers(JsonElement value) => value.ValueKind switch
    {
        JsonValueKind.Object => value.EnumerateObject().Any(property => IsSensitive(property.Name) || ContainsSensitiveMembers(property.Value)),
        JsonValueKind.Array => value.EnumerateArray().Any(ContainsSensitiveMembers),
        _ => false,
    };

    // ---------- 说明文字 ----------

    private static (string Text, string Source) Describe(Type owner, PropertyInfo property, string section, string path)
    {
        if (SettingDocumentation.Find(owner, property) is { } reviewed) return (reviewed, "bridge-reviewed-source");
        var documented = SourceDocumentation.Find("P", owner, property.Name);
        if (!string.IsNullOrWhiteSpace(documented?.Summary))
            return (documented.Summary, documented.DocumentationSource);
        var key = $"P:{owner.FullName}.{property.Name}";
        if (XmlDocs.Value.TryGetValue(key, out var fromXml) && fromXml.Length > 0)
            return (fromXml, "xml-summary");

        var attribute = property.GetCustomAttribute<DescriptionAttribute>()?.Description;
        if (!string.IsNullOrWhiteSpace(attribute)) return (attribute, "DescriptionAttribute");

        // 推断：分区用途 + 属性名。
        var sectionText = SectionDescriptions.GetValueOrDefault(section, $"{section} 设置分区。");
        return ($"读取 {section} 分区中的 {property.Name}（{FriendlyType(property.PropertyType)}）。分区用途：{sectionText}BetterGI 未提供该字段的独立业务说明，不能仅凭名称推断改变后的游戏效果。", "type-contract");
    }

    /// <summary>
    /// 宿主的 XML 注释文件，原版发布默认没有。路径必须用 AllConfig 所在程序集定位：
    /// 桥自己的 Location 在单文件宿主里是空串。
    /// </summary>
    private static Dictionary<string, string> LoadXmlDocs()
    {
        var result = new Dictionary<string, string>(StringComparer.Ordinal);
        try
        {
            var assembly = Host.AllConfigType()?.Assembly;
            var location = assembly?.Location;
            if (string.IsNullOrEmpty(location)) return result;

            var xmlPath = Path.ChangeExtension(location, ".xml");
            if (!File.Exists(xmlPath)) return result;

            var document = XDocument.Load(xmlPath);
            foreach (var member in document.Descendants("member"))
            {
                var name = member.Attribute("name")?.Value;
                if (name is null) continue;
                var summary = member.Element("summary")?.Value;
                if (string.IsNullOrWhiteSpace(summary)) continue;
                result[name] = string.Join(' ', summary.Split((char[]?)null, StringSplitOptions.RemoveEmptyEntries));
            }
        }
        catch
        {
            // 文件损坏或读不到都只是没有富说明，不影响目录。
        }
        return result;
    }

    private static Dictionary<string, string> EnumDescriptions(Type enumType) =>
        Enum.GetNames(enumType).ToDictionary(
            name => name,
            name => enumType.GetField(name)?.GetCustomAttribute<DescriptionAttribute>()?.Description ?? name,
            StringComparer.OrdinalIgnoreCase);

    private static string FriendlyType(Type type)
    {
        var underlying = Nullable.GetUnderlyingType(type);
        if (underlying is not null) return $"{FriendlyType(underlying)}?";
        if (type.IsEnum) return $"enum:{type.Name}";
        if (type.IsArray) return $"array<{FriendlyType(type.GetElementType()!)}>";
        if (typeof(IEnumerable).IsAssignableFrom(type) && type != typeof(string))
        {
            var element = type.IsGenericType ? type.GetGenericArguments()[0] : typeof(object);
            return $"array<{FriendlyType(element)}>";
        }
        return type.Name switch
        {
            "Boolean" => "boolean",
            "String" => "string",
            "Int32" => "integer",
            "Int64" => "integer64",
            "Single" or "Double" or "Decimal" => "number",
            _ => type.Name,
        };
    }

    // ---------- 路径读写 ----------

    /// <summary>按点分路径定位属性与其宿主对象，不区分大小写。</summary>
    public static (object Owner, PropertyInfo Property) Resolve(object root, string path)
    {
        if (string.IsNullOrWhiteSpace(path) || path.Length > 256 || path.Split('.').Any(part => string.IsNullOrWhiteSpace(part)))
            throw new ArgumentException("必须提供目录中的完整设置路径。", nameof(path));
        var parts = path.Split('.', StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries);
        if (parts.Length == 0) throw new ArgumentException("设置路径不能为空。", nameof(path));

        object owner = root;
        for (var index = 0; index < parts.Length; index++)
        {
            var property = owner.GetType()
                .GetProperties(BindingFlags.Instance | BindingFlags.Public)
                .FirstOrDefault(p =>
                    p.GetMethod?.IsPublic == true && p.GetIndexParameters().Length == 0
                    && p.GetCustomAttribute<JsonIgnoreAttribute>() is null
                    && !typeof(Delegate).IsAssignableFrom(p.PropertyType)
                    && ((p.GetCustomAttribute<JsonPropertyNameAttribute>()?.Name ?? JsonNamingPolicy.CamelCase.ConvertName(p.Name)).Equals(parts[index], StringComparison.OrdinalIgnoreCase)
                    || p.Name.Equals(parts[index], StringComparison.OrdinalIgnoreCase)))
                ?? throw new ArgumentException(
                    $"设置路径不存在：{string.Join('.', parts.Take(index + 1))}", nameof(path));

            if (index == parts.Length - 1) return (owner, property);

            owner = property.GetValue(owner)
                ?? throw new InvalidOperationException($"设置路径中间属性 {property.Name} 为 null。");
        }

        throw new InvalidOperationException("无法解析设置路径。");
    }

    public static List<SettingSection> BuildSections()
    {
        var entries = Build();
        return entries
            .GroupBy(x => x.Section, StringComparer.OrdinalIgnoreCase)
            .Select(group => new SettingSection(
                group.Key,
                SectionDescriptions.GetValueOrDefault(group.Key, $"{group.Key} 设置分区。"),
                group.Count(),
                group.Count(x => x.Writable),
                group.Count(x => x.Sensitive)))
            .OrderBy(x => x.Name, StringComparer.OrdinalIgnoreCase)
            .ToList();
    }
}
