using System.Collections;
using System.Collections.Concurrent;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using System.Text.Json.Nodes;
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
        var guide = new AgentGuide("准备地图追踪配置组", "用宿主原生构造器把所选父目录下全部路线包装成一个配置组，保留原文件名和目录；运行配置继承全局配置当前有效值（同名同类型字段深拷贝），并返回路线需求聚合与有效战斗策略面。",
            ["资源已订阅或本机已安装，需要为路线父目录建立可运行的配置组。"], ["path 是 pathing/ 开始的精确目录路径，对应 User/AutoPathing 下同名子目录（替换前缀，不在前面追加 AutoPathing）；目录不存在直接失败，无需先启动游戏。groupName 依据用户目标直接取简短自然名，如“挖矿讨伐”；不拼目录、作者、时间或参数长句，不为命名再次提问，缺省时自动生成。"],
            ["只创建一个稳定命名的配置组；已有同名但不同内容的组不覆盖，已有组的显式配置保持不动。新组的 autoFightConfig 等同名同类型字段继承全局配置当前有效值，不用类型默认冒充继承。"],
            "prepared=true 只表示完整配置组已落盘，不代表已经运行。requirements.partyConfirmationRequired=true（含战斗 fight/combat_script 或特殊机制动作）时，运行前必须完成当前任务的配队确认：用户原话已给队名/队员则用已给值；队员组成或策略适配所需角色信息缺失且无法从现场可靠观测时，用 user.ask 一次问清，未答复不得启动。策略文件列出的角色不是实际队伍名单。autoFightStrategy.requirementsExcerpt 是有效策略文件开头的原文要求，适配组配置以它为准。",
            "以返回的 inheritedSettings、requirements（含 sources）与 autoFightStrategy（exists/requirementsExcerpt）为准；队伍与赶路角色用 bgi.set_pathing_party 写入。只有用户明确要求运行时才运行；游戏未就绪时运行接口会按真实状态自行准备。",
            "删除该配置组后重新 prepare 即回到继承基线；不动路线与订阅。",
            [], "bridge-stable-operation");
        registry.Register(id, "scheduler", guide.Purpose, Prepare, readOnly: false, destructive: true,
            inputSchema: AgentSchemas.Input(id), guide: guide);

        const string partyId = "bgi.set_pathing_party";
        var partyGuide = new AgentGuide("设置配置组队伍与赶路角色", "把用户点名的队伍与赶路角色写入配置组的 pathingConfig；其他配置字段原样保留。",
            ["用户为现成配置组指定队伍、赶路/加速角色等运行偏好时；角色俗称由本接口做数据映射，不需要模型猜字段。"],
            ["groupName 来自 User/ScriptGroup 的真实 name，不需要启动游戏。", "expectedSha256 来自 bgi.user.read 的当前 SHA-256；内容不一致时拒绝写入。", "partyName 用用户原话中的队伍名。", "hurryAvatar 接受本机支持列表内的角色名、常见俗称或“自动”；空字符串表示不使用技能赶路。"],
            ["只修改该组 config.pathingConfig 的对应字段；写 partyName 时同时置 enabled=true，否则新队名不生效。文件其余字段不动。"],
            "configured=true 表示字段已写入并回读核验；不代表已运行任何脚本。",
            "以返回的 partyName、partySwitchEnabled、hurryOnAvatar 与回读核验为准。",
            "再次调用同一接口改回原值即可；不删除组、不动路线。",
            [JsonSerializer.SerializeToElement(new { groupName = "用户目录中读取到的精确配置组名称", partyName = "用户点名的队伍名", hurryAvatar = "玛薇卡" })],
            "bridge-stable-operation");
        registry.Register(partyId, "scheduler", partyGuide.Purpose, SetParty, readOnly: false, destructive: false,
            inputSchema: AgentSchemas.Input(partyId), guide: partyGuide);

        const string inspectId = "bgi.inspect_group_effective";
        var inspectGuide = new AgentGuide("核对组有效配置来源", "只读揭示一个配置组各运行字段实际生效的值与来源（组内覆盖 vs 与全局当前值一致），以及有效战斗策略的类型/文件/版本与开头要求原文。",
            ["运行前核对组配置是否满足用户目标、诊断组配置覆盖全局有效设置类问题时。"],
            ["groupName 来自 User/ScriptGroup 的真实 name；不需要启动游戏。"],
            ["只读；不修改任何文件，不启动游戏。"],
            "fields[] 给出每个关键字的组内值与来源；strategy 给出实际生效策略面。缺失/不可读字段如实标注，不猜。",
            "以 fields[].source 与 strategy.version 为准；source=groupOverride 表示该字段与全局不同，需判断是有意自定义还是历史默认覆盖。",
            "无需回滚。",
            [JsonSerializer.SerializeToElement(new { groupName = "精确配置组名称" })],
            "bridge-stable-operation");
        registry.Register(inspectId, "scheduler", inspectGuide.Purpose, Inspect, readOnly: true,
            inputSchema: AgentSchemas.Input(inspectId), guide: inspectGuide);

        const string syncId = "bgi.sync_group_effective_config";
        var syncGuide = new AgentGuide("按全局现值同步组字段", "把配置组中明确列出的运行字段替换为全局配置当前有效值（CAS：expectedSha256 + 每文件锁 + 原子替换 + 写后真实 diff），其余字段原样保留。用于修复创建时被类型默认覆盖的已证实错误配置；不用于无差别重置用户自定义。",
            ["已证实组内字段是错误默认（如旧版创建用类型默认覆盖了全局有效战斗配置）需要修复时；只列需要修复的字段。"],
            ["groupName 精确；expectedSha256 来自 bgi.user.read 当前值。", "fields 只允许同时存在于组 PathingConfig 与全局配置根且同类型的字段（本接口按此校验）。"],
            ["只替换 fields 列出的字段为全局当前值；其余字段与文件其他部分逐字段保留。"],
            "synced=true 且 otherFieldsPreserved=true 表示替换完成并回读核验；SHA 不匹配返回 VERSION_CONFLICT 未写入。",
            "回读 replacedFields 的值与全局当前值一致；其他字段与改前逐字段一致。",
            "再次调用同一接口换回原值即可；不删除组。",
            [JsonSerializer.SerializeToElement(new { groupName = "组名", expectedSha256 = "bgi.user.read 的 SHA-256", fields = new[] { "AutoFightConfig" } })],
            "bridge-stable-operation");
        registry.Register(syncId, "scheduler", syncGuide.Purpose, Sync, readOnly: false, destructive: false,
            inputSchema: AgentSchemas.Input(syncId), guide: syncGuide);
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
            : GroupNaming.Default(path.Split('/')[^1], "采集任务");
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
        // 组运行配置不从类型默认值开始：与全局配置根（HomePageViewModel.Config，
        // 即 AllConfig）同名同类型的字段用 JSON 深拷贝继承用户当前有效值，
        // 否则 AutoFightHandler 会拿组内默认覆盖用户的全局战斗策略。
        var inherited = InheritGlobalEffectiveSettings(group);
        // 关键有效战斗配置缺证即拒绝创建：不用类型默认冒充继承，让用户兜底。
        if (!inherited.Contains("AutoFightConfig"))
            throw BridgeException.Failed(
                "无法继承全局有效战斗配置（AutoFightConfig）：宿主全局配置根不可达或深拷贝失败。"
                + "已停止创建；请确认 BetterGI 已连接且全局配置可读后重试。");
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
        // 路线需求聚合（程序侧深读，聚合结果才返回）与有效策略面。
        var requirements = AggregateRouteRequirements(userRoot, routes);
        var strategy = DescribeEffectiveStrategy(group, userRoot);
        return Task.FromResult<object?>(new
        {
            prepared = true,
            reused = false,
            groupName = name,
            sourcePath = path,
            executionScope = "directoryRecursive",
            routeCount = routes.Length,
            inheritedSettings = inherited,
            inheritedHostSettings = inherited.Count > 0,
            inheritanceNote = inherited.Count > 0
                ? "同名同类型字段继承自全局配置当前有效值（深拷贝）。"
                : "未取得全局配置对象或无可继承字段；组配置保持类型默认，运行前应按用户目标核对。",
            requirements,
            autoFightStrategy = strategy,
        });
    }

    /// <summary>全局配置根（AllConfig）与组 PathingConfig 同名同类型字段的深拷贝继承。
    /// 反射真实现值，不经类型默认；任何字段失败即跳过并如实报告未继承。</summary>
    internal static List<string> InheritGlobalEffectiveSettings(object group)
    {
        var inherited = new List<string>();
        var config = Reflect.Get(group, "Config");
        var pathing = config is null ? null : Reflect.Get(config, "PathingConfig");
        var services = Host.Services();
        var homeType = Reflect.FindType("BetterGenshinImpact.ViewModel.Pages.HomePageViewModel");
        if (pathing is null || services is null || homeType is null) return inherited;
        var home = services.GetService(homeType);
        var global = home is null ? null : Reflect.Get(home, "Config");
        if (global is null) return inherited;
        var globalType = global.GetType();
        foreach (var property in pathing.GetType().GetProperties())
        {
            if (!property.CanWrite) continue;
            var match = globalType.GetProperty(property.Name);
            if (match?.PropertyType != property.PropertyType) continue;
            try
            {
                var current = match.GetValue(global);
                if (current is null) continue;
                // JSON 往返深拷贝：与组文件序列化同一通道，不手工逐字段复制。
                var json = JsonSerializer.Serialize(current, property.PropertyType);
                var clone = JsonSerializer.Deserialize(json, property.PropertyType);
                if (clone is null) continue;
                property.SetValue(pathing, clone);
                inherited.Add(property.Name);
            }
            catch (System.Text.Json.JsonException)
            {
                // 单字段失败不影响其它字段；未列入继承清单。
            }
        }
        return inherited;
    }

    /// <summary>路线需求聚合：深读路线文件的动作/类型，分类战斗与特殊机制，
    /// 只返回聚合事实与来源，不把叶子全文交给调用方。</summary>
    internal static object AggregateRouteRequirements(string userRoot, string[] routes)
    {
        const int limit = 60;
        var counts = new Dictionary<string, int>(StringComparer.Ordinal);
        var scanned = 0;
        var unreadable = 0;
        var sources = new List<string>();
        var root = Path.GetFullPath(Path.Combine(userRoot, "AutoPathing"));
        foreach (var route in routes.Take(limit))
        {
            string text;
            try { text = File.ReadAllText(Path.Combine(root, route)); }
            catch (IOException) { unreadable++; continue; }
            JsonDocument document;
            try { document = JsonDocument.Parse(text); }
            catch (JsonException) { unreadable++; continue; }
            using (document)
            {
                scanned++;
                if (sources.Count < 8) sources.Add(route.Replace('\\', '/'));
                foreach (var key in new[] { "positions", "waypoints" })
                    if (document.RootElement.TryGetProperty(key, out var points))
                        foreach (var point in points.EnumerateArray())
                            foreach (var field in new[] { "action", "type" })
                                if (point.TryGetProperty(field, out var value)
                                    && value.ValueKind == JsonValueKind.String
                                    && value.GetString() is { Length: > 0 } token)
                                    counts[token] = counts.GetValueOrDefault(token) + 1;
            }
        }
        string[] battleTokens = ["fight", "combat_script"];
        string[] specialTokens = ["up_down_grab_leaf", "linnea_mining"];
        string[] benignTokens = ["stop_flying", "mining", "path", "teleport", "target", "orientation", "pick_around", "force_tp", "log_output", "四叶印", "hydrogranum"];
        var battle = counts.Keys.Any(token => battleTokens.Contains(token));
        var special = counts.Where(pair => specialTokens.Contains(pair.Key))
            .Select(pair => (object)new { action = pair.Key, count = pair.Value })
            .ToArray();
        var unrecognized = counts.Where(pair => !battleTokens.Contains(pair.Key) && !specialTokens.Contains(pair.Key) && !benignTokens.Contains(pair.Key))
            .Select(pair => (object)new { action = pair.Key, count = pair.Value })
            .ToArray();
        var incomplete = unreadable > 0 || routes.Length > limit;
        var required = battle || special.Length > 0 || unrecognized.Length > 0 || incomplete;
        var status = battle || special.Length > 0 ? "preconditionsFound" : required ? "unknown" : "noPreconditions";
        return new
        {
            status,
            battle,
            specialActions = special,
            unrecognizedActions = unrecognized,
            partyConfirmationRequired = required,
            scannedFiles = scanned,
            unreadableFiles = unreadable,
            truncated = routes.Length > limit,
            sources,
            note = required
                ? "由路线文件动作聚合的程序侧事实；battle/special/unknown 任一成立都要求运行前用 user.ask 问清缺的配队信息（用户已给的队名/队员不重复问），未答复不得启动。"
                : "由路线文件动作聚合的程序侧事实；全部文件已读且未出现战斗/特殊/未识别动作。",
        };
    }

    /// <summary>组内实际生效的战斗策略视图。策略名可能是具体文件（txt/json）、
    /// "自动/根据队伍自动选择"（宿主运行时按队伍解析），都如实呈现；文件带
    /// 内容 SHA-256 作版本，摘录是开头的原文要求。按字符串/JsonNode 取值，
    /// 不对 JSON 做反射（JsonElement 无属性可反射，会误报宿主能力缺失）。</summary>
    internal static object StrategyView(string? name, string userRoot)
    {
        if (string.IsNullOrWhiteSpace(name))
            return new { name = (string?)null, type = (string?)null, file = (string?)null, exists = false, version = (string?)null, requirementsExcerpt = (string?)null };
        var trimmed = name.Trim();
        string[] autoNames = ["自动", "根据队伍自动选择"];
        if (autoNames.Contains(trimmed, StringComparer.Ordinal))
            return new
            {
                name = trimmed,
                type = "auto",
                file = (string?)null,
                exists = true,
                version = (string?)null,
                requirementsExcerpt = (string?)"宿主会在运行时按实际队伍自动选择策略；适配前应按用户确认的队员组成评估可用策略。",
            };
        foreach (var extension in new[] { ".txt", ".json" })
        {
            var file = Path.Combine(userRoot, "AutoFight", trimmed + extension);
            if (!File.Exists(file)) continue;
            string? excerpt = null;
            var head = File.ReadLines(file).Take(50).ToList();
            excerpt = string.Join(Environment.NewLine, head);
            if (excerpt.Length > 4000) excerpt = excerpt[..4000];
            var version = Convert.ToHexString(System.Security.Cryptography.SHA256.HashData(File.ReadAllBytes(file))).ToLowerInvariant();
            return new
            {
                name = trimmed,
                type = extension == ".txt" ? "file:txt" : "file:json",
                file = Path.GetRelativePath(userRoot, file).Replace('\\', '/'),
                exists = true,
                version,
                requirementsExcerpt = excerpt,
            };
        }
        return new { name = trimmed, type = "missing", file = (string?)null, exists = false, version = (string?)null, requirementsExcerpt = (string?)null };
    }

    /// <summary>typed 宿主组对象入口（Prepare 持真反射对象）。</summary>
    internal static object DescribeEffectiveStrategy(object group, string userRoot)
    {
        var config = Reflect.Get(group, "Config");
        var pathing = config is null ? null : Reflect.Get(config, "PathingConfig");
        var fight = pathing is null ? null : Reflect.Get(pathing, "AutoFightConfig");
        return StrategyView(fight is null ? null : Reflect.Get(fight, "StrategyName") as string, userRoot);
    }

    /// <summary>组文件 JSON 视图入口（Inspect 用）：按字段名取值，不做 JSON 反射。</summary>
    internal static object DescribeEffectiveStrategyFromJson(JsonObject pathing, string userRoot)
    {
        var fight = LookupKey(pathing, "AutoFightConfig") as JsonObject;
        var name = fight is null ? null : (LookupKey(fight, "StrategyName") as JsonValue)?.GetValue<string>();
        return StrategyView(name, userRoot);
    }

    /// <summary>赶路角色的常见俗称到本机支持名的数据映射；是知识数据，不是控制流。
    /// 支持列表以宿主 HurryOnAvatarList 为准（不同版本/未来角色会变化）。</summary>
    internal static readonly Dictionary<string, string> HurryAliases = new(StringComparer.Ordinal)
    {
        ["火神"] = "玛薇卡",
        ["战争之神"] = "玛薇卡",
        ["散兵"] = "流浪者",
        ["不赶路"] = "",
        ["不加速"] = "",
        ["普通跑图"] = "",
    };

    /// <summary>宿主原生支持列表读不到时的兜底；与 PathingPartyConfig.HurryOnAvatarList 默认值一致。</summary>
    internal static readonly string[] FallbackHurryOptions =
        ["", "自动", "玛薇卡", "闲云", "桑多涅", "恰斯卡", "流浪者", "伊法", "希诺宁", "法尔伽", "夜兰"];

    /// <summary>读取宿主当前支持的赶路角色选项；宿主类型不可用时用已知默认。</summary>
    internal static string[] HurryOptions()
    {
        var type = Reflect.FindType("BetterGenshinImpact.Core.Config.PathingPartyConfig");
        if (type is not null && Activator.CreateInstance(type) is { } instance
            && Reflect.Get(instance, "HurryOnAvatarList") is IEnumerable list)
        {
            var options = list.Cast<object?>()
                .Select(value => value?.ToString())
                .OfType<string>()
                .ToArray();
            if (options.Length > 0) return options;
        }
        return FallbackHurryOptions;
    }

    /// <summary>把用户/模型给出的赶路角色说法解析为本机支持名。先按支持列表直配，
    /// 再查俗称表；都不命中时按本机实际支持列表报错，不猜测。</summary>
    internal static string ResolveHurryAvatar(string requested, string[] supported)
    {
        var trimmed = requested.Trim();
        if (supported.Contains(trimmed, StringComparer.Ordinal)) return trimmed;
        if (HurryAliases.TryGetValue(trimmed, out var mapped) && supported.Contains(mapped, StringComparer.Ordinal))
            return mapped;
        var names = string.Join("、", supported.Where(option => option.Length > 0));
        throw BridgeException.InvalidArgument(
            $"不支持赶路角色「{trimmed}」。本机 BetterGI 支持：{names}、自动；空值表示不使用技能赶路。");
    }

    /// <summary>把队伍/赶路角色合并进 pathingConfig：只写目标字段（partyName 联动
    /// enabled=true），其余键原样保留。抽出来供静态验收直接测合并语义。</summary>
    internal static void MergeParty(JsonObject pathing, string? party, string? hurry)
    {
        if (party is not null)
        {
            pathing["partyName"] = party;
            pathing["enabled"] = true;
        }
        if (hurry is not null) pathing["hurryOnAvatar"] = hurry;
    }

    /// <summary>同进程内每个组文件一把锁：读-改-比-提交全程串行，防止并发互相覆盖。</summary>
    private static readonly ConcurrentDictionary<string, object> FileLocks = new(StringComparer.OrdinalIgnoreCase);

    /// <summary>写后核验：以改前文档为基准，剥离本次目标字段后逐字段深比较，
    /// 其余内容是否真的原样保留。返回事实结论，不预置 true。</summary>
    internal static (bool Preserved, string[] Changed) VerifyMerge(
        JsonNode? before, JsonNode? after, bool partyChanged, bool hurryChanged,
        bool fightAligned = false)
    {
        var expected = before?.DeepClone();
        var actual = after?.DeepClone();
        foreach (var node in new[] { expected, actual })
        {
            if (node?["config"]?["pathingConfig"] is not JsonObject pathing) continue;
            if (partyChanged)
            {
                pathing.Remove("partyName");
                pathing.Remove("enabled");
            }
            if (hurryChanged) pathing.Remove("hurryOnAvatar");
            // 只有真正发生战斗配置对齐时才剥离比较；其余未知字段一律参与逐字段比较。
            if (fightAligned) RemoveFieldIgnoreCase(pathing, "AutoFightConfig");
        }
        var preserved = JsonNode.DeepEquals(expected, actual);
        var changed = new List<string>();
        if (partyChanged) changed.AddRange(["config.pathingConfig.partyName", "config.pathingConfig.enabled"]);
        if (hurryChanged) changed.Add("config.pathingConfig.hurryOnAvatar");
        return (preserved, [.. changed]);
    }

    /// <summary>全局配置根（AllConfig）当前各字段序列化视图；null 表示宿主不可达。</summary>
    private static JsonObject? GlobalEffectiveView()
    {
        var services = Host.Services();
        var homeType = Reflect.FindType("BetterGenshinImpact.ViewModel.Pages.HomePageViewModel");
        if (services is null || homeType is null) return null;
        var home = services.GetService(homeType);
        var global = home is null ? null : Reflect.Get(home, "Config");
        if (global is null) return null;
        try
        {
            return JsonNode.Parse(JsonSerializer.Serialize(global, global.GetType()))?.AsObject();
        }
        catch (JsonException)
        {
            return null;
        }
    }

    /// <summary>组有效字段来源核对核心：组 pathingConfig 与全局视图逐字段比较。
    /// 纯函数（输入两个 JSON 视图），供离线验收直接测。</summary>
    internal static object EffectiveFieldReport(
        JsonObject pathing, JsonObject? global, string[] fields, bool groupFightActive)
    {
        var report = new List<object>();
        foreach (var field in fields)
        {
            // 组文件是 camelCase、全局 CLR 序列化是 PascalCase：按键大小写不敏感对齐。
            var groupValue = LookupKey(pathing, field)?.DeepClone();
            var globalValue = global is null ? null : LookupKey(global, field)?.DeepClone();
            string source;
            if (globalValue is null)
                source = groupValue is null ? "absent" : "groupOnly";
            else
                source = JsonNode.DeepEquals(groupValue, globalValue) ? "globalEqual" : "groupOverride";
            // 是否实际生效：AutoFightConfig 受 Enabled&&AutoFightEnabled 门控（组开关
            // 未开时全局值才生效）；其余字段组内存在即生效。
            var inEffect = string.Equals(field, "AutoFightConfig", StringComparison.OrdinalIgnoreCase)
                ? groupFightActive
                : groupValue is not null;
            report.Add(new { field, source, value = groupValue, globalValue, inEffect });
        }
        return report;
    }

    private static Task<object?> Inspect(JsonElement arguments, CancellationToken cancellation)
    {
        var requested = arguments.GetProperty("groupName").GetString()!.Trim();
        var userRoot = Path.Combine(AppContext.BaseDirectory, "User");
        var groupFile = ScriptGroupTools.RequireGroupFile(Path.Combine(userRoot, "ScriptGroup"), requested);
        var node = JsonNode.Parse(File.ReadAllText(groupFile)) ?? throw BridgeException.Failed("配置组文件解析失败。");
        var pathing = node["config"]?["pathingConfig"]?.AsObject()
            ?? throw BridgeException.Failed("配置组缺少 config.pathingConfig。");
        string[] keyFields = ["AutoFightConfig", "AutoEatConfig", "PartyName", "HurryOnAvatar", "Enabled", "AutoFightEnabled"];
        var global = GlobalEffectiveView();
        // AutoFightHandler 语义：pathingConfig.Enabled && AutoFightEnabled 才用组配置，
        // 否则实际生效的是全局；有效战斗策略取自胜出的一方。
        var groupFightActive = Truthy(pathing, "Enabled") && Truthy(pathing, "AutoFightEnabled");
        var fields = EffectiveFieldReport(pathing, global, keyFields, groupFightActive);
        var globalFight = global is null ? null : LookupKey(global, "AutoFightConfig")?.DeepClone();
        var effectiveFightSource = EffectiveFightSource(
            groupFightActive, LookupKey(pathing, "AutoFightConfig"), globalFight);
        var strategyPathing = groupFightActive || globalFight is null
            ? pathing.DeepClone().AsObject()
            : new JsonObject { ["autoFightConfig"] = globalFight!.DeepClone() };
        var strategy = DescribeEffectiveStrategyFromJson(strategyPathing, userRoot);
        return Task.FromResult<object?>(new
        {
            groupName = requested,
            globalReadable = global is not null,
            groupFightActive,
            effectiveFightSource,
            fields,
            strategy,
            note = global is null
                ? "宿主全局配置不可读：来源判定退化为 groupOnly/absent，不能据此断言与全局一致。"
                : "source=globalEqual 表示组内值与全局当前值一致；groupOverride 需判断是有意自定义还是历史默认覆盖。",
        });
    }

    /// <summary>把 fields 列出的组字段替换为给定克隆值：CAS（expectedSha256）、
    /// 每文件锁、原子替换、写后真实 diff。生产文件机制核心，离线可测。</summary>
    internal static object SyncCore(
        string userRoot, string groupName, string expectedSha256,
        IReadOnlyDictionary<string, JsonNode?> replacements)
    {
        var directory = Path.Combine(userRoot, "ScriptGroup");
        var groupFile = ScriptGroupTools.RequireGroupFile(directory, groupName);
        lock (FileLocks.GetOrAdd(groupFile, _ => new object()))
        {
            var content = File.ReadAllText(groupFile);
            var actual = Convert.ToHexString(System.Security.Cryptography.SHA256.HashData(Encoding.UTF8.GetBytes(content))).ToLowerInvariant();
            if (actual != expectedSha256.ToLowerInvariant())
                throw new BridgeException("VERSION_CONFLICT",
                    "配置组内容与 expectedSha256 不一致（可能已被其他操作修改）；请重新 bgi.user.read 取最新 SHA 后重试。未写入。", 409);
            var node = JsonNode.Parse(content) ?? throw BridgeException.Failed("配置组文件解析失败。");
            var before = node.DeepClone();
            var pathing = ((node["config"] ??= new JsonObject())["pathingConfig"] ??= new JsonObject()).AsObject();
            foreach (var (field, clone) in replacements)
            {
                // 沿用文件里已有的键名（组文件 camelCase / 全局 PascalCase），不留双键。
                var existing = pathing.FirstOrDefault(property =>
                    string.Equals(property.Key, field, StringComparison.OrdinalIgnoreCase)).Key ?? field;
                pathing.Remove(existing);
                pathing[existing] = clone?.DeepClone();
            }
            var temporary = groupFile + ".sync.tmp";
            try
            {
                File.WriteAllText(temporary, node.ToJsonString(new JsonSerializerOptions { WriteIndented = true }));
                if (Convert.ToHexString(System.Security.Cryptography.SHA256.HashData(Encoding.UTF8.GetBytes(File.ReadAllText(groupFile)))).ToLowerInvariant() != expectedSha256.ToLowerInvariant())
                    throw new BridgeException("VERSION_CONFLICT", "配置组文件在准备写入期间被修改；未写入，请重读后重试。", 409);
                File.Move(temporary, groupFile, true);
            }
            catch
            {
                try { File.Delete(temporary); } catch (IOException) { }
                throw;
            }
            // 写后双重核验：目标字段读回必须等于替换值本身，且其余字段与改前一致。
            // 任一不成立/读不回都保持未核验，不得当成成功。
            var written = JsonNode.Parse(File.ReadAllText(groupFile)) ?? throw BridgeException.Failed("写后回读解析失败。");
            var writtenPathing = written["config"]?["pathingConfig"] as JsonObject
                ?? throw BridgeException.Failed("写后回读缺少 pathingConfig；核验失败。");
            var replacedVerified = true;
            string? mismatchedField = null;
            foreach (var (field, clone) in replacements)
            {
                var readBack = LookupKey(writtenPathing, field);
                if (readBack is null || !JsonNode.DeepEquals(readBack, clone))
                {
                    replacedVerified = false;
                    mismatchedField ??= field;
                }
            }
            var expected = before.DeepClone();
            foreach (var field in replacements.Keys)
            {
                RemoveFieldIgnoreCase(expected["config"]?["pathingConfig"] as JsonObject, field);
                RemoveFieldIgnoreCase(written["config"]?["pathingConfig"] as JsonObject, field);
            }
            var preserved = JsonNode.DeepEquals(expected, written);
            var verified = preserved && replacedVerified;
            return new
            {
                synced = verified,
                verified,
                replacedFieldsVerified = replacedVerified,
                mismatchedField,
                groupName,
                replacedFields = replacements.Keys.ToArray(),
                otherFieldsPreserved = preserved,
                verificationScope = "configReadBack",
                verificationReason = verified
                    ? "目标字段读回等于替换值，且其余字段与改前逐字段一致。"
                    : !replacedVerified
                        ? $"目标字段读回与替换值不一致（{mismatchedField}）；未核验，请核对组文件。"
                        : "写后 diff 发现被替换字段之外的差异；未核验，请核对组文件。",
            };
        }
    }

    private static Task<object?> Sync(JsonElement arguments, CancellationToken cancellation)
    {
        var requested = arguments.GetProperty("groupName").GetString()!.Trim();
        var expectedSha = arguments.GetProperty("expectedSha256").GetString()!;
        var fields = arguments.TryGetProperty("fields", out var list)
            ? list.EnumerateArray().Select(value => value.GetString() ?? "").Where(name => name.Length > 0).Distinct(StringComparer.Ordinal).ToArray()
            : [];
        if (fields.Length == 0)
            throw BridgeException.InvalidArgument("fields 至少列一个要同步的字段。");
        var userRoot = Path.Combine(AppContext.BaseDirectory, "User");
        // 字段合法性：必须同时存在于组 PathingConfig 与全局配置根且同类型；
        // 克隆值只能来自全局当前值，调用方给不了值。
        var global = GlobalEffectiveView() ?? throw BridgeException.Failed("宿主全局配置不可读，无法按全局现值同步；未写入。");
        var pathingType = Reflect.FindType("BetterGenshinImpact.Core.Config.PathingPartyConfig")
            ?? throw BridgeException.Missing("宿主 PathingPartyConfig 类型不可用。");
        var replacements = new Dictionary<string, JsonNode?>(StringComparer.Ordinal);
        foreach (var field in fields)
        {
            var property = pathingType.GetProperty(field);
            if (property is null || !property.CanWrite)
                throw BridgeException.InvalidArgument($"字段不可同步（组配置没有可写属性 {field}）。");
            if (!global.TryGetPropertyValue(field, out var current) || current is null)
                throw BridgeException.InvalidArgument($"全局配置没有同名字段 {field}，不能同步。");
            replacements[field] = current.DeepClone();
        }
        var result = SyncCore(userRoot, requested, expectedSha, replacements);
        return Task.FromResult<object?>(result);
    }

    private static Task<object?> SetParty(JsonElement arguments, CancellationToken cancellation)
    {
        var requested = arguments.GetProperty("groupName").GetString()!.Trim();
        var expectedSha = arguments.GetProperty("expectedSha256").GetString()!;
        string? party = null, hurry = null;
        if (arguments.TryGetProperty("partyName", out var partyValue) && partyValue.GetString() is { Length: > 0 } named)
            party = named.Trim();
        if (arguments.TryGetProperty("hurryAvatar", out var hurryValue) && hurryValue.GetString() is { } raw)
            hurry = raw.Trim().Length == 0 ? "" : ResolveHurryAvatar(raw, HurryOptions());
        if (party is null && hurry is null)
            throw BridgeException.InvalidArgument("partyName 与 hurryAvatar 至少提供一项。");

        var userRoot = Path.Combine(AppContext.BaseDirectory, "User");
        var directory = Path.Combine(userRoot, "ScriptGroup");
        var groupFile = ScriptGroupTools.RequireGroupFile(directory, requested);
        lock (FileLocks.GetOrAdd(groupFile, _ => new object()))
        {
            cancellation.ThrowIfCancellationRequested();
            var content = File.ReadAllText(groupFile);
            var actual = Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(content))).ToLowerInvariant();
            if (actual != expectedSha.ToLowerInvariant())
                throw new BridgeException("VERSION_CONFLICT",
                    $"配置组内容与 expectedSha256 不一致（可能已被其他操作修改）；请重新 bgi.user.read 取最新 SHA 后重试。未写入。", 409);
            // JsonNode 就地改写：除目标字段外整份文档逐字段保留。
            var node = JsonNode.Parse(content)
                ?? throw BridgeException.Failed("配置组文件解析失败。");
            var before = node.DeepClone();
            var pathing = ((node["config"] ??= new JsonObject())["pathingConfig"] ??= new JsonObject()).AsObject();
            MergeParty(pathing, party, hurry);
            // 已证实的"类型默认遮盖全局"修正：用户显式配队/适配写入时，若组内战斗
            // 配置仍是原生构造默认（enabled 默认 true 的旧组同样命中）而全局已有不同
            // 的成熟配置，则同一 CAS 写入内把它对齐到全局当前有效值；非默认自定义与
            // 未知额外字段保持不动。不基于任何角色/关键词。
            string? fightAligned = null;
            if (party is not null || hurry is not null)
            {
                var globalView = GlobalEffectiveView();
                var globalFight = globalView is null ? null : LookupKey(globalView, "AutoFightConfig")?.DeepClone();
                var decision = FightAlignmentDecision(
                    LookupKey(pathing, "AutoFightConfig") as JsonObject, globalFight as JsonObject,
                    NativeAutoFightDefaultView());
                if (decision.Align && globalFight is not null)
                {
                    var key = pathing.FirstOrDefault(property =>
                        string.Equals(property.Key, "autoFightConfig", StringComparison.OrdinalIgnoreCase)).Key ?? "autoFightConfig";
                    pathing[key] = globalFight.DeepClone();
                    fightAligned = $"组内战斗配置是未配置的类型默认，已对齐全局当前有效值：{decision.Reason}";
                }
            }
            var serialized = node.ToJsonString(new JsonSerializerOptions { WriteIndented = true });
            // 提交前再核一次文件未变化（锁外的外部写入仍可能发生），原子替换落盘。
            if (Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(File.ReadAllText(groupFile)))).ToLowerInvariant() != expectedSha.ToLowerInvariant())
                throw new BridgeException("VERSION_CONFLICT", "配置组文件在准备写入期间被修改；未写入，请重读后重试。", 409);
            var temporary = groupFile + ".party.tmp";
            try
            {
                File.WriteAllText(temporary, serialized);
                // 临时文件落盘后、替换前是最后可无副作用取消的位置。
                cancellation.ThrowIfCancellationRequested();
                File.Move(temporary, groupFile, true);
            }
            catch
            {
                // 失败/取消时只清理本接口自己的临时文件，不动目标文件。
                try { File.Delete(temporary); } catch (IOException) { }
                throw;
            }

            // 写后回读 + 真实 diff：目标字段取回值，其余字段深比较给事实结论。
            var written = JsonNode.Parse(File.ReadAllText(groupFile))
                ?? throw BridgeException.Failed("配置组写后回读解析失败。");
            var applied = written["config"]?["pathingConfig"];
            string? ReadBack(string field) =>
                applied is JsonObject obj && obj.TryGetPropertyValue(field, out var value) ? value?.GetValue<string>() : null;
            bool ReadBackFlag(string field) =>
                applied is JsonObject obj && obj.TryGetPropertyValue(field, out var value) && value?.GetValue<bool>() == true;
            var (preserved, changed) = VerifyMerge(
                before, written, party is not null, hurry is not null, fightAligned is not null);
            var changedFields = changed.ToList();
            // 对齐字段的读回等值核验：写后的 autoFightConfig 必须与全局当前值精确相等。
            var alignedVerified = true;
            if (fightAligned is not null)
            {
                changedFields.Add("config.pathingConfig.autoFightConfig");
                var globalAfter = GlobalEffectiveView();
                var globalFightAfter = globalAfter is null ? null : LookupKey(globalAfter, "AutoFightConfig");
                alignedVerified = globalFightAfter is not null
                    && JsonNode.DeepEquals(LookupKey(written["config"]?["pathingConfig"]?.AsObject(), "AutoFightConfig"), globalFightAfter);
            }
            // 启用组配置后组内战斗配置即覆盖全局：若与全局不同，明确提示核对/同步，
            // 不默默把全局有效战斗策略切换成组内旧值（可能是历史类型默认）。
            string? effectiveFightWarning = null;
            if (party is not null && Truthy(pathing, "Enabled") && Truthy(pathing, "AutoFightEnabled"))
            {
                var globalView = GlobalEffectiveView();
                var globalFight = globalView is null ? null : LookupKey(globalView, "AutoFightConfig");
                if (globalFight is not null
                    && !JsonNode.DeepEquals(LookupKey(pathing, "AutoFightConfig"), globalFight))
                {
                    effectiveFightWarning =
                        "组配置已启用：运行时将使用组内 autoFightConfig 覆盖全局当前值，且两者不同"
                        + "（可能是历史创建写入的类型默认）。若这不是有意自定义，用 bgi.sync_group_effective_config(fields=[AutoFightConfig]) 按全局现值修复；已有显式自定义保持不动。";
                }
            }
            var valuesApplied = (party is null || ReadBack("partyName") == party)
                && (party is null || ReadBackFlag("enabled"))
                && (hurry is null || ReadBack("hurryOnAvatar") == hurry)
                && alignedVerified;
            return Task.FromResult<object?>(new
            {
                configured = valuesApplied && preserved,
                groupName = requested,
                partyName = ReadBack("partyName"),
                partySwitchEnabled = ReadBackFlag("enabled"),
                hurryOnAvatar = ReadBack("hurryOnAvatar"),
                changedFields = changedFields,
                otherFieldsPreserved = preserved,
                verified = valuesApplied && preserved,
                effectiveFightWarning,
                fightAlignedToGlobal = fightAligned is not null,
                effectiveFight = EffectiveFightFace(written["config"]?["pathingConfig"]?.AsObject(), userRoot),
                verificationScope = "configReadBack",
                verificationReason = alignedVerified
                    ? "目标字段与（如对齐的）战斗配置读回等于期望值，其余字段与改前逐字段一致；不代表已运行。"
                    : "对齐的战斗配置读回与全局当前值不一致；未核验。",
            });
        }
    }

    /// <summary>按键名大小写不敏感取值；组文件与全局序列化的命名风格不同。</summary>
    internal static JsonNode? LookupKey(JsonObject? source, string field)
    {
        if (source is null) return null;
        foreach (var (key, value) in source)
            if (string.Equals(key, field, StringComparison.OrdinalIgnoreCase))
                return value;
        return null;
    }

    /// <summary>宿主原生 AutoFightConfig 构造初值序列化视图（PascalCase）；
    /// 拿不到类型时为 null（识别退化为非默认，不误对齐）。</summary>
    private static readonly Lazy<JsonObject?> NativeAutoFightDefault = new(() =>
    {
        // 原生类型从真实全局对象的 AutoFightConfig 属性取（PropertyType 即宿主
        // 实际 CLR 类型，不猜命名空间）；构造初值按该类型序列化，与全局快照同一
        // 命名风格（PascalCase），比较在 FightIsTypeDefault 里做大小写规范化。
        var services = Host.Services();
        var homeType = Reflect.FindType("BetterGenshinImpact.ViewModel.Pages.HomePageViewModel");
        if (services is null || homeType is null) return null;
        var home = services.GetService(homeType);
        var global = home is null ? null : Reflect.Get(home, "Config");
        var type = global?.GetType().GetProperty("AutoFightConfig")?.PropertyType;
        if (type is null || type.IsInterface || type.GetConstructor(Type.EmptyTypes) is null) return null;
        try
        {
            var instance = Activator.CreateInstance(type);
            return instance is null ? null
                : JsonNode.Parse(JsonSerializer.Serialize(instance, type))?.AsObject();
        }
        catch (Exception)
        {
            return null;
        }
    });

    internal static JsonObject? NativeAutoFightDefaultView() => NativeAutoFightDefault.Value;

    /// <summary>识别"未配置的类型默认"：组内 autoFightConfig 的每个键都在原生默认里
    /// 同名存在（大小写不敏感）且值相等，且没有默认之外的额外字段——额外未知字段
    /// 视为用户自定义，必须保留、不对齐。空对象（未写战斗配置）同样算默认。</summary>
    internal static bool FightIsTypeDefault(JsonObject? groupFight, JsonObject? nativeDefault)
    {
        if (nativeDefault is null) return false;
        if (groupFight is null) return true;
        foreach (var (key, value) in groupFight)
        {
            var defaultValue = LookupKey(nativeDefault, key);
            if (defaultValue is null) return false; // 默认之外的额外字段=自定义
            if (!JsonNode.DeepEquals(value, defaultValue)) return false;
        }
        return true;
    }

    /// <summary>对齐决策核心：显式配队/适配写入时，只有"组战斗配置是未配置的类型
    /// 默认"且全局存在不同的成熟配置才对齐到全局当前有效值；非默认自定义保持。</summary>
    internal static (bool Align, string Reason) FightAlignmentDecision(
        JsonObject? groupFight, JsonObject? globalFight, JsonObject? nativeDefault)
    {
        if (globalFight is null) return (false, "全局战斗配置不可读，不对齐。");
        if (JsonNode.DeepEquals(groupFight, globalFight)) return (false, "已与全局一致。");
        if (!FightIsTypeDefault(groupFight, nativeDefault))
            return (false, "组内战斗配置含非默认自定义字段，保持不动。");
        return (true, "组内战斗配置是未配置的类型默认，对齐到全局当前有效值。");
    }

    /// <summary>写后实际生效的战斗面：按 Enabled&&AutoFightEnabled 判定组/全局胜出，
    /// 给出胜方策略（file 带版本与原文，auto 附带全局当前已选具体策略作为候选）。</summary>
    internal static object EffectiveFightFace(JsonObject? pathing, string userRoot)
    {
        var groupActive = pathing is not null && Truthy(pathing, "Enabled") && Truthy(pathing, "AutoFightEnabled");
        var globalView = GlobalEffectiveView();
        var globalFight = globalView is null ? null : LookupKey(globalView, "AutoFightConfig") as JsonObject;
        var winner = groupActive
            ? LookupKey(pathing!, "AutoFightConfig") as JsonObject
            : globalFight;
        var source = !groupActive ? "global" : globalFight is not null && JsonNode.DeepEquals(LookupKey(pathing!, "AutoFightConfig"), globalFight)
            ? "group(sameAsGlobal)" : "group";
        var name = winner is null ? null : (LookupKey(winner, "StrategyName") as JsonValue)?.GetValue<string>();
        var strategy = StrategyView(name, userRoot);
        object? autoCandidate = null;
        if (name is not null && (name.Trim() == "自动" || name.Trim() == "根据队伍自动选择"))
        {
            var globalName = globalFight is null ? null : (LookupKey(globalFight, "StrategyName") as JsonValue)?.GetValue<string>();
            if (globalName is not null && globalName.Trim() != "自动" && globalName.Trim() != "根据队伍自动选择")
                autoCandidate = StrategyView(globalName, userRoot);
        }
        return new { source, strategy, autoCandidate };
    }

    /// <summary>AutoFightHandler 语义的有效战斗配置来源：组开关未开 → 全局；
    /// 开启且与全局相同 → group(sameAsGlobal)；开启且不同 → group（覆盖）。</summary>
    internal static string EffectiveFightSource(bool groupFightActive, JsonNode? groupFight, JsonNode? globalFight)
    {
        if (!groupFightActive) return "global";
        if (globalFight is not null && JsonNode.DeepEquals(groupFight, globalFight)) return "group(sameAsGlobal)";
        return "group";
    }

    /// <summary>bool 字段读取（大小写不敏感；缺失按 false）。</summary>
    internal static bool Truthy(JsonObject source, string field) =>
        LookupKey(source, field) is { } value && value.GetValue<bool>();

    private static void RemoveFieldIgnoreCase(JsonObject? source, string field)
    {
        var existing = source?.FirstOrDefault(property =>
            string.Equals(property.Key, field, StringComparison.OrdinalIgnoreCase)).Key;
        if (existing is not null) source!.Remove(existing);
    }
}
