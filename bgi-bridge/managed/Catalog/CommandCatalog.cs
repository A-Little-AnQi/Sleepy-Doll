using System.Collections.Concurrent;
using System.Reflection;
using System.Text.Json;
using System.Text.Json.Nodes;
using System.Text.RegularExpressions;
using System.Windows.Input;
using BgiBridge.Bgi;
using BgiBridge.Protocol;

namespace BgiBridge.Catalog;

public sealed record CommandDescriptor(
    string Name,
    string ViewModel,
    string Command,
    string? ParameterType,
    bool IsAsync,
    bool RequiresConfirmation)
{
    public required AgentGuide Guide { get; init; }
    public JsonElement? ParameterSchema { get; init; }
    public string? UnavailableReason { get; init; }
    public bool IsDestructive { get; init; }
    public bool RequiresGameReady { get; init; }
    public required JsonElement SelectionSchema { get; init; }
    public bool NeedsDialogInput { get; init; }
    public bool InternalUiEvent { get; init; }
    public JsonElement? ImplementationInput { get; init; }
    public bool RequiresImplementationInput { get; init; }
}

/// <summary>命令目录：扫描宿主 ICommand 属性，按真实上下文暴露成 cmd.&lt;owner&gt;.&lt;command&gt;。</summary>
public static partial class CommandCatalog
{
    private static BridgeConfig? _config;
    public static void Configure(BridgeConfig config) => _config = config;
    /// <summary>命中这些词的命令标记为需要确认。</summary>
    private static readonly string[] DangerousWords =
    [
        "Delete", "Remove", "Clear", "Reset", "Exit", "Close", "Shutdown", "Restart",
        "Install", "Update", "Import", "Save", "Write", "Overwrite", "Uninstall",
    ];

    private static readonly Lazy<IReadOnlyList<(CommandDescriptor Descriptor, Type ViewModel, PropertyInfo Property, Type? Parameter)>> Catalog =
        new(Build);

    public static IReadOnlyList<CommandDescriptor> All => Catalog.Value.Select(x => x.Descriptor).ToList();

    private static IReadOnlyList<(CommandDescriptor, Type, PropertyInfo, Type?)> Build()
    {
        var result = new List<(CommandDescriptor, Type, PropertyInfo, Type?)>();
        var seen = new HashSet<string>(StringComparer.OrdinalIgnoreCase);


        // 目录扫描不创建服务或窗口；调用时再定位宿主当前上下文。
        foreach (var assembly in AppDomain.CurrentDomain.GetAssemblies())
        {
            Type[] types;
            try
            {
                types = assembly.GetTypes();
            }
            catch (ReflectionTypeLoadException error)
            {
                types = error.Types.OfType<Type>().ToArray();
            }
            catch
            {
                continue;  // 反射型程序集读不了，跳过
            }

            foreach (var type in types)
            {
                if (type.IsInterface || type.FullName?.StartsWith("BetterGenshinImpact.", StringComparison.Ordinal) != true
                    || type.Assembly.GetName().Name != Reflect.HostAssembly) continue;

                foreach (var property in type.GetProperties(BindingFlags.Instance | BindingFlags.Public))
                {
                    if (!property.Name.EndsWith("Command", StringComparison.Ordinal)) continue;
                    if (!typeof(ICommand).IsAssignableFrom(property.PropertyType)) continue;

                    var viewModelName = TrimSuffix(type.Name.Split('`')[0], "ViewModel");
                    var commandName = TrimSuffix(property.Name, "Command");
                    var name = $"{ToSnakeCase(viewModelName)}.{ToSnakeCase(commandName)}";
                    // 用户明确要求：遗留剧情跟踪暂不开放，目录与通用调用入口一起隐藏。
                    if (name is "task_settings_page.switch_auto_track" or "task_settings_page.go_to_auto_track_url") continue;
                    if (!seen.Add(name)) continue;  // 重名保留第一个
                    var parameterType = FindParameterType(property.PropertyType);
                    var source = SourceDocumentation.Find("C", type, property.Name);
                    var purpose = CommandDocumentation.Purpose(type.Name, property.Name, source);
                    if (purpose.Contains("不安排自动调用", StringComparison.Ordinal))
                        purpose = $"宿主反射命令 {type.FullName}.{property.Name}；需当前上下文、原生参数与操作后的业务证据，不从处理器返回推断用户目标完成。";
                    var title = CommandDocumentation.Title(type.Name, property.Name, source);
                    var parameterSchema = parameterType is null
                        ? (JsonElement?)null
                        : DescribeParameter(CommandTargets.Schema(parameterType), title, parameterType);
                    var implemented = ImplementedCommands.Handles(name);
                    var unavailable = source?.HasImplementation == false && !implemented ? "当前 BetterGI 版本该命令为空实现。"
                        : source?.AsyncVoid == true && source.HasAwait ? "宿主使用包含 await 的 async void，需稳定可等待的替代入口。"
                        : null;
                    var internalUi = CommandDocumentation.UnavailableReason(type.Name, property.Name) is not null;
                    var guide = new AgentGuide(
                        title, purpose,
                        unavailable is null
                            ? [$"需要执行“{title}”，且目标就是 {viewModelName} 当前界面上下文时。"]
                            : ["仅用于审计 BetterGI 行为或理解相关界面；当前接口不安排直接调用。"],
                        unavailable is null
                            ? ["命令可调用且 CanExecute=true。", "list_command_targets 定位上下文；多实例／泛型使用 contextId，复杂参数使用真实 objectId。", parameterType is null ? "没有 argument 参数；可传 contextId。" : $"argument 满足 parameterSchema（{parameterType.FullName}）。"]
                            : [$"不可调用：{unavailable}"],
                        CommandDocumentation.SideEffects(type.Name, property.Name),
                        unavailable is null
                            ? "executed=true 表示命令处理器已返回；异步命令已等待其 Task。configurationCheckpoint 是执行前配置备份。"
                            : "当前版本不会执行此接口；callable=false 与 unavailableReason 是权威结果。",
                        CommandDocumentation.Verification(type.Name, property.Name),
                        CommandDocumentation.Rollback(type.Name, property.Name),
                        unavailable is not null ? []
                            : parameterType is not null && parameterSchema is null ? []
                            : [parameterType is null ? ArgumentSchema.Parse("{}") : JsonSerializer.SerializeToElement(new { argument = Example(parameterSchema) })],
                        source?.Summary.Length > 0 ? "host-source-and-ui" : "bridge-domain-guide",
                        source is null ? null : $"{source.Source}:{source.Line}");

                    result.Add((
                        new CommandDescriptor(
                            name,
                            type.Name,
                            property.Name,
                            FindParameterType(property.PropertyType)?.FullName ?? FindParameterType(property.PropertyType)?.Name,
                            IsAsyncCommand(property.PropertyType),
                            true) { Guide = guide, ParameterSchema = parameterSchema, UnavailableReason = unavailable,
                                IsDestructive = DangerousWords.Any(word => commandName.Contains(word, StringComparison.OrdinalIgnoreCase)),
                                RequiresGameReady = CommandDocumentation.RequiresGameReady(type.Name, property.Name) || name == "task_settings_page.switch_auto_track_path",
                                SelectionSchema = CommandTargets.SelectionSchema(type), NeedsDialogInput = source?.NeedsDialogInput == true, InternalUiEvent = internalUi,
                                ImplementationInput = implemented ? ImplementedCommands.Input(name) : null, RequiresImplementationInput = ImplementedCommands.NeedsInput(name) },
                        type,
                        property,
                        FindParameterType(property.PropertyType)));
                }
            }
        }

        result.Sort((a, b) => string.Compare(a.Item1.Name, b.Item1.Name, StringComparison.Ordinal));
        return result;
    }

    /// <summary>在 UI 线程执行；这些命令操作绑定到界面的对象。</summary>
    public static async Task<object?> Invoke(string name, JsonElement? argument, CancellationToken cancellation, string? contextId = null, JsonElement? selection = null, JsonElement? dialogInput = null, JsonElement? implementationInput = null)
    {
        if (_config is null || !_config.IsMethodEnabled($"cmd.{name}", "command"))
            throw BridgeException.Disabled("命令分组或该命令已禁用；通用入口不能绕过此限制。");
        var entry = Catalog.Value.FirstOrDefault(x =>
            string.Equals(x.Descriptor.Name, name, StringComparison.OrdinalIgnoreCase));
        if (entry.Descriptor is null) throw BridgeException.NotFound($"没有这个命令：{name}");
        if (entry.Descriptor.UnavailableReason is { } unavailable)
            throw new BridgeException("NOT_CALLABLE", unavailable, 409);
        if (entry.Descriptor.NeedsDialogInput && !dialogInput.HasValue)
            throw BridgeException.InvalidArgument("命令需要明确 dialogInput；不能用通用入口省略弹窗输入。");
        if (entry.Descriptor.RequiresImplementationInput && !implementationInput.HasValue)
            throw BridgeException.InvalidArgument("补充实现需要 implementationInput；按当前接口 Schema 提供业务输入。");
        if (entry.Descriptor.RequiresGameReady && !Host.CaptureReady)
            throw BridgeException.GameNotReady("该命令要求截图器和游戏环境已就绪。");

        return await Ui.InvokeAsync(async () =>
        {
            cancellation.ThrowIfCancellationRequested();
            // 解析 ViewModel 可能在其构造函数里执行配置迁移。
            var checkpoint = SettingsTransactions.Engine.Checkpoint($"cmd.{name}");
            var viewModel = CommandTargets.Owner(entry.ViewModel, contextId);
            if (ImplementedCommands.Handles(name))
            {
                var result = await ImplementedCommands.Run(name,viewModel,implementationInput ?? ArgumentSchema.Parse("{}"),cancellation);
                return (object?)new { command = name, executed = true, configurationCheckpoint = checkpoint, implementation = "bgiPlugin", result };
            }
            if (name == "map_pathing.open_settings")
            {
                var vmType = Reflect.RequireType("BetterGenshinImpact.ViewModel.Pages.View.PathingConfigViewModel");
                var vm = Host.Services()?.GetService(vmType) ?? throw BridgeException.Missing("地图配置上下文不可用。");
                var windowType = Reflect.RequireType("BetterGenshinImpact.View.Pages.View.PathingConfigView");
                var window = Activator.CreateInstance(windowType, vm)!;
                Reflect.Call(window, "Show");
                return (object?)new { command = name, executed = true, configurationCheckpoint = checkpoint, opened = Reflect.Get(window,"IsVisible") is true,
                    interaction = "modelessEditor", verification = "只核验窗口打开；配置修改需后续读取和提交。" };
            }
            var property = viewModel.GetType().GetProperty(entry.Property.Name)!;
            var parameterType = FindParameterType(property.PropertyType);
            var parameter = parameterType is null ? ConvertArgument(argument, null)
                : argument.HasValue ? CommandTargets.Argument(argument.Value, parameterType) : ConvertArgument(null, parameterType);
            if (selection.HasValue) CommandTargets.Select(viewModel, selection.Value);
            if (property.GetValue(viewModel) is not ICommand command)
                throw BridgeException.Missing($"属性不是 ICommand：{entry.Property.Name}");

            if (!command.CanExecute(parameter))
                throw new BridgeException("INVALID_STATE",
                    $"命令「{name}」当前不可执行——检查页面状态和必填参数。", 409);
            cancellation.ThrowIfCancellationRequested();
            var source = SourceDocumentation.Find("C", entry.ViewModel, entry.Property.Name);
            using var dialog = new NativeDialogScope(dialogInput, source?.DialogTitles, cancellation);
            await Commands.RunAsync(command, parameter);
            dialog.Verify();

            return (object?)new { command = name, executed = true, configurationCheckpoint = checkpoint, dialogsHandled = dialog.Handled };
        }).ConfigureAwait(false);
    }

    public static object Targets(string name, string query = "", int offset = 0, int limit = 20)
    {
        var entry = Catalog.Value.FirstOrDefault(x => x.Descriptor.Name.Equals(name, StringComparison.OrdinalIgnoreCase));
        if (entry.Descriptor is null) throw BridgeException.NotFound("命令不存在。");
        return CommandTargets.List(entry.ViewModel, entry.Parameter, query, offset, limit);
    }

    public static object CreateTarget(string name, JsonElement arguments)
    {
        if (_config is null || !_config.IsMethodEnabled($"cmd.{name}", "command")) throw BridgeException.Disabled("目标命令已禁用，不能通过创建上下文绕过。");
        var entry = Catalog.Value.FirstOrDefault(x => x.Descriptor.Name.Equals(name, StringComparison.OrdinalIgnoreCase));
        if (entry.Descriptor is null) throw BridgeException.NotFound("命令不存在。");
        return CommandTargets.Create(entry.ViewModel, arguments);
    }

    public static object CreateArgument(string name, JsonElement arguments, string? contextId)
    {
        if (_config is null || !_config.IsMethodEnabled($"cmd.{name}", "command")) throw BridgeException.Disabled("目标命令已禁用。");
        var entry = Catalog.Value.FirstOrDefault(x => x.Descriptor.Name.Equals(name, StringComparison.OrdinalIgnoreCase));
        if (entry.Descriptor is null || entry.Parameter is null) throw BridgeException.InvalidArgument("此命令没有参数对象。");
        var parameter = entry.Parameter;
        if (parameter.ContainsGenericParameters)
        {
            var owner = CommandTargets.Owner(entry.ViewModel, contextId);
            parameter = FindParameterType(owner.GetType().GetProperty(entry.Property.Name)!.PropertyType)!;
        }
        if (parameter == typeof(object)) throw BridgeException.InvalidArgument("object 参数需从当前宿主目标中选择具体对象；不能构造无业务类型的 object。");
        return CommandTargets.Create(parameter, arguments);
    }

    private static object? ConvertArgument(JsonElement? argument, Type? parameterType)
    {
        if (parameterType is null)
        {
            if (argument is { ValueKind: not (JsonValueKind.Null or JsonValueKind.Undefined) })
                throw BridgeException.InvalidArgument("该命令不接受参数。");
            return null;
        }
        if (argument is null || argument.Value.ValueKind is JsonValueKind.Null or JsonValueKind.Undefined)
        {
            if (parameterType.IsValueType && Nullable.GetUnderlyingType(parameterType) is null)
                throw BridgeException.InvalidArgument($"命令需要 {parameterType.Name} 类型的参数。");
            return null;
        }
        try
        {
            var schema = ValueContract.Schema(parameterType) ?? throw BridgeException.InvalidArgument("该命令参数尚无安全序列化契约。");
            ArgumentSchema.Validate(argument.Value, schema, "argument");
            return JsonSerializer.Deserialize(argument.Value.GetRawText(), parameterType, ValueContract.Json)
                ?? (parameterType.IsValueType ? Activator.CreateInstance(parameterType) : null);
        }
        catch (JsonException ex)
        {
            throw BridgeException.InvalidArgument($"参数无法转换为 {parameterType.Name}：{ex.Message}");
        }
    }

    private static JsonElement Example(JsonElement? schema)
    {
        if (schema is null) return ArgumentSchema.Parse("null");
        var value = schema.Value;
        if (value.TryGetProperty("anyOf", out var alternatives)) return Example(alternatives[0]);
        if (value.TryGetProperty("enum", out var choices)) return choices[0].Clone();
        return value.GetProperty("type").GetString() switch
        {
            "boolean" => ArgumentSchema.Parse("false"),
            "integer" or "number" => JsonSerializer.SerializeToElement(value.TryGetProperty("minimum", out var minimum) ? Math.Max(0, minimum.GetDouble()) : 0),
            "array" => ArgumentSchema.Parse("[]"),
            "string" => JsonSerializer.SerializeToElement("依据接口说明替换为实际值"),
            "object" => JsonSerializer.SerializeToElement(new { objectId = "list_command_targets 返回的 objectId" }),
            _ => ArgumentSchema.Parse("null"),
        };
    }

    private static JsonElement? DescribeParameter(JsonElement? schema, string title, Type parameterType)
    {
        if (schema is null) return null;
        var node = JsonNode.Parse(schema.Value.GetRawText())!.AsObject();
        node["description"] = $"“{title}”的 {parameterType.Name} 参数。取值业务含义见用途说明。";
        return JsonSerializer.SerializeToElement(node);
    }

    /// <summary>按接口名判断是否为异步命令。</summary>
    private static bool IsAsyncCommand(Type commandType) =>
        commandType.GetInterfaces().Append(commandType).Any(i =>
            i.Name is "IAsyncRelayCommand" or "IAsyncRelayCommand`1")
        || commandType.GetMethod("ExecuteAsync", [typeof(object)]) is not null;

    private static Type? FindParameterType(Type commandType)
    {
        var generic = commandType.GetInterfaces().Append(commandType).FirstOrDefault(x =>
            x.IsGenericType
            && (x.GetGenericTypeDefinition().Name.StartsWith("IRelayCommand", StringComparison.Ordinal)
                || x.GetGenericTypeDefinition().Name.StartsWith("IAsyncRelayCommand", StringComparison.Ordinal)));
        return generic?.GetGenericArguments().FirstOrDefault();
    }

    private static string TrimSuffix(string value, string suffix) =>
        value.EndsWith(suffix, StringComparison.Ordinal) ? value[..^suffix.Length] : value;

    /// <summary>驼峰转下划线。</summary>
    private static string ToSnakeCase(string value) =>
        SnakeBoundary().Replace(value, "$1_$2").ToLowerInvariant();

    [GeneratedRegex("([a-z0-9])([A-Z])")]
    private static partial Regex SnakeBoundary();
}
