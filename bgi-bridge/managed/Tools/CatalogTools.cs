using System.Text.Json;
using BgiBridge.Bgi;
using BgiBridge.Catalog;
using BgiBridge.Protocol;

namespace BgiBridge.Tools;

public static class CatalogTools
{
    public static void RegisterDiscovered(MethodRegistry registry, BridgeConfig config)
    {
        CommandCatalog.Configure(config);
        foreach (var command in CommandCatalog.All)
        {
            var item = command;
            var context = ("contextId", ArgumentSchema.Parse("""{"type":"string","minLength":1,"maxLength":64,"description":"list_command_targets 返回的当前上下文引用；多实例或泛型命令必须指定。"}"""), false);
            var selection = ("selection", item.SelectionSchema, false);
            var dialog = ("dialogInput", NativeDialogScope.Schema, item.NeedsDialogInput);
            var schema = item.ParameterType is null ? AgentSchemas.Object(context, selection, dialog)
                : AgentSchemas.Object(context, selection, dialog, ("argument", item.ParameterSchema!.Value, true));
            registry.Register($"cmd.{item.Name}", "command", item.Guide.Purpose,
                (arguments, cancellation) => CommandCatalog.Invoke(item.Name,
                    arguments.TryGetProperty("argument", out var value) ? value : null, cancellation,
                    arguments.TryGetProperty("contextId", out var contextId) ? contextId.GetString() : null,
                    arguments.TryGetProperty("selection", out var selected) ? selected : null,
                    arguments.TryGetProperty("dialogInput", out var input) ? input : null),
                readOnly: false, destructive: true, inputSchema: schema, guide: item.Guide,
                unavailableReason: item.UnavailableReason, requiresGameReady: item.RequiresGameReady);
        }
        foreach (var setting in SettingsCatalog.Build())
        {
            var path = setting.Path;
            var guide = new AgentGuide(
                $"{setting.Section} · {path}", setting.Description,
                ["查询此设置，或准备修改此设置时。"],
                ["桥已连接；arguments 为空对象。"],
                ["无写入副作用；敏感值返回遮蔽标记。"],
                "返回当前值、值类型、写入 Schema、valueVersion、可写状态及限制原因。",
                "修改时使用刚返回的 valueVersion；defaultValueKnown=false 表示目录没有可靠默认值。",
                "只读。", [ArgumentSchema.Parse("{}")], setting.DescriptionSource, setting.SourceReference);
            registry.Register($"setting.{path}", "settings", guide.Purpose,
                (_, _) => Ui.InvokeAsync<object?>(() => DescribeOne(Find(path))),
                inputSchema: ArgumentSchema.Empty, guide: guide);
        }
    }

    public static object DescribeOne(SettingEntry item) => new
    {
        path = item.Path, section = item.Section, valueType = item.ValueType,
        description = item.Description, descriptionSource = item.DescriptionSource,
        sourceReference = item.SourceReference, writable = item.Writable,
        writeRestriction = item.WriteRestriction, sensitive = item.Sensitive,
        readable = item.ReadError is null, readError = item.ReadError,
        currentValue = item.Sensitive ? (object)"***REDACTED***" : item.CurrentValue,
        valueSchema = item.ValueSchema, valueVersion = item.ValueVersion,
        defaultValue = item.DefaultValue, defaultValueKnown = item.DefaultValueKnown,
        allowedValues = item.AllowedValues, allowedValueDescriptions = item.AllowedValueDescriptions,
    };

    private static SettingEntry Find(string path) => SettingsCatalog.Build().FirstOrDefault(item =>
        item.Path.Equals(path, StringComparison.OrdinalIgnoreCase))
        ?? throw BridgeException.NotFound($"配置目录中没有 {path}。");

    public static void Register(MethodRegistry registry)
    {
        CommandTargetTools.Register(registry);
        ScriptApiTools.Register(registry);
        NativeUiTools.Register(registry);
        LocalResourceDeletionTools.Register(registry);
        ScriptGroupTools.Register(registry);
        ScriptGroupDeletionTools.Register(registry);
        TaskStopTools.Register(registry);
        NavigationTools.Register(registry);
        JavaScriptPreparationTools.Register(registry);
        OneDragonTools.Register(registry);
        GameResolutionTools.Register(registry);
        ScriptRepositoryTools.Register(registry);
        PathingPreparationTools.Register(registry);
        registry.Register("bgi.list_setting_sections", "settings", "",
            (_, _) => Ui.InvokeAsync<object?>(() => new { sections = SettingsCatalog.BuildSections() }));
        registry.Register("bgi.search_settings", "settings", "", (arguments, _) => Ui.InvokeAsync<object?>(() =>
        {
            var terms = arguments.TryGetProperty("terms", out var t) ? t.EnumerateArray().Select(item => item.GetString()!).ToArray() : [];
            var section = arguments.TryGetProperty("section", out var s) ? s.GetString() : null;
            var type = arguments.TryGetProperty("valueType", out var v) ? v.GetString() : null;
            var writable = arguments.TryGetProperty("writableOnly", out var w) && w.GetBoolean();
            var limit = arguments.TryGetProperty("limit", out var l) ? l.GetInt32() : 30;
            var hits = SettingsCatalog.Build().Where(item =>
                (section is null || item.Section.Equals(section, StringComparison.OrdinalIgnoreCase))
                && (type is null || item.ValueType.Equals(type, StringComparison.OrdinalIgnoreCase))
                && (!writable || item.Writable)
                && terms.All(term => (item.Path + " " + item.Description).Contains(term, StringComparison.OrdinalIgnoreCase))).ToList();
            return new { total = hits.Count, items = hits.Take(limit).Select(DescribeOne).ToArray() };
        }));
        registry.Register("bgi.get_setting", "settings", "",
            (arguments, _) => Ui.InvokeAsync<object?>(() => DescribeOne(Find(arguments.GetProperty("path").GetString()!))));
        registry.Register("bgi.preview_settings", "settings", "", (arguments, cancellation) => Ui.InvokeAsync<object?>(() =>
        {
            cancellation.ThrowIfCancellationRequested();
            return SettingsTransactions.Engine.Preview(ReadChanges(arguments.GetProperty("changes")));
        }));
        registry.Register("bgi.commit_settings", "settings", "",
            (arguments, cancellation) => Ui.InvokeAsync<object?>(() => SettingsTransactions.Engine.Commit(arguments.GetProperty("planId").GetString()!, cancellation)),
            readOnly: false, destructive: true);
        registry.Register("bgi.set_setting", "settings", "", async (arguments, cancellation) =>
        {
            var preview = await Ui.InvokeAsync(() => SettingsTransactions.Engine.Preview([
                new(arguments.GetProperty("path").GetString()!, arguments.GetProperty("value").Clone(), arguments.GetProperty("expectedVersion").GetString()!)
            ])).ConfigureAwait(false);
            var planId = JsonSerializer.SerializeToElement(preview).GetProperty("planId").GetString()!;
            return await Ui.InvokeAsync(() => SettingsTransactions.Engine.Commit(planId, cancellation)).ConfigureAwait(false);
        }, readOnly: false, destructive: true);
        registry.Register("bgi.list_setting_changes", "settings", "",
            (_, _) => Ui.InvokeAsync<object?>(() => SettingsTransactions.Engine.History()));
        registry.Register("bgi.get_setting_change", "settings", "",
            (arguments, _) => Ui.InvokeAsync<object?>(() => SettingsTransactions.Engine.Describe(arguments.GetProperty("changeId").GetString()!)));
        registry.Register("bgi.rollback_settings", "settings", "",
            (arguments, cancellation) => Ui.InvokeAsync<object?>(() => SettingsTransactions.Engine.Rollback(arguments.GetProperty("changeId").GetString()!, cancellation)),
            readOnly: false, destructive: true);
        registry.Register("bgi.preview_setting_restore", "settings", "从历史变更中恢复所选设置的预览", (arguments, cancellation) => Ui.InvokeAsync<object?>(() =>
        {
            cancellation.ThrowIfCancellationRequested();
            var preview = SettingsTransactions.Engine.PreviewRestore(arguments.GetProperty("changeId").GetString()!,
                arguments.GetProperty("recordVersion").GetString()!, arguments.GetProperty("paths").EnumerateArray().Select(path => path.GetString()!).ToArray());
            var wire = JsonSerializer.SerializeToNode(preview)!.AsObject();
            var idle = Host.TaskSemaphoreCount();
            var changed = wire["differences"]!.AsArray().Any(row => row!["changed"]!.GetValue<bool>());
            wire["canApply"] = idle > 0 && changed;
            wire["reason"] = !changed ? "当前已经是这些值，无需恢复。" : idle is null ? "无法确认当前任务状态，请重新连接或退出 BetterGI 后恢复。" : idle == 0 ? "BetterGI 正在执行任务，请先停止任务再恢复设置。" : null;
            wire["hostRunning"] = true;
            return wire;
        }), inputSchema: AgentSchemas.Object(("changeId",AgentSchemas.Text("历史变更 ID。"),true),
            ("recordVersion",AgentSchemas.Text("变更记录的当前版本。"),true),
            ("paths",ArgumentSchema.Parse("""{"type":"array","minItems":1,"maxItems":20,"uniqueItems":true,"items":{"type":"string"},"description":"明确选择这份记录中的设置路径。"}"""),true)),
            guide: new AgentGuide("预览恢复所选设置", "读取历史记录的修改前值，生成当前设置到目标值的恢复预览；不会立即恢复，不覆盖其他设置。",
                ["用户明确要求把特定设置恢复到某次历史记录中的值。"], ["记录属于当前 BetterGI；路径是记录中明确选中的项。"], ["只建立十分钟有效的预览计划。"],
                "canApply=true 才可确认；differences 包含当前值、恢复值、联动项以及此项后来是否改过。敏感内容不显示。",
                "确认后使用 commit_settings(planId)，核验变更后的内存值与文件；目标项变化会拒绝恢复，不影响无关后续修改。",
                "恢复本身产生新变更记录，可再次选择其修改前值撤销。", [], "bridge-setting-recovery"));
        registry.Register("bgi.list_commands", "command", "", (arguments, _) =>
        {
            var includeDangerous = arguments.TryGetProperty("includeDangerous", out var d) && d.GetBoolean();
            var filter = arguments.TryGetProperty("filter", out var f) ? f.GetString() : null;
            var items = CommandCatalog.All
                .Where(item => includeDangerous || !item.IsDestructive)
                .Where(item => string.IsNullOrWhiteSpace(filter) || (item.Name + " " + item.Guide.Purpose).Contains(filter, StringComparison.OrdinalIgnoreCase)).ToArray();
            return Task.FromResult<object?>(new { count = items.Length, commands = items });
        });
        registry.Register("bgi.invoke_command", "command", "", (arguments, cancellation) =>
            CommandCatalog.Invoke(arguments.GetProperty("command").GetString()!,
                arguments.TryGetProperty("argument", out var argument) ? argument : null, cancellation,
                arguments.TryGetProperty("contextId", out var context) ? context.GetString() : null,
                arguments.TryGetProperty("selection", out var selection) ? selection : null,
                arguments.TryGetProperty("dialogInput", out var dialog) ? dialog : null),
            readOnly: false, destructive: true);
    }

    private static SettingChange[] ReadChanges(JsonElement changes) => changes.EnumerateArray()
        .Select(change => new SettingChange(change.GetProperty("path").GetString()!,
            change.GetProperty("value").Clone(), change.GetProperty("expectedVersion").GetString()!)).ToArray();
}
