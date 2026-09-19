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
}

/// <summary>命令目录：把已注册 ViewModel 的 ICommand 属性暴露成 cmd.&lt;view_model&gt;.&lt;command&gt;。</summary>
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

        var services = Host.Services();
        var marker = Reflect.FindType("BetterGenshinImpact.ViewModel.IViewModel");
        if (marker is null) return result;

        // 宿主容器已构建，IServiceCollection 注册表不可读；扫已加载程序集的实现，再用容器过滤。
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
                if (type.IsAbstract || type.IsInterface || !marker.IsAssignableFrom(type)) continue;
                if (Host.IsRegistered(services, type) == false) continue;

                foreach (var property in type.GetProperties(BindingFlags.Instance | BindingFlags.Public))
                {
                    if (!typeof(ICommand).IsAssignableFrom(property.PropertyType)) continue;
                    if (!property.Name.EndsWith("Command", StringComparison.Ordinal)) continue;

                    var viewModelName = TrimSuffix(type.Name, "ViewModel");
                    var commandName = TrimSuffix(property.Name, "Command");
                    var name = $"{ToSnakeCase(viewModelName)}.{ToSnakeCase(commandName)}";
                    if (!seen.Add(name)) continue;  // 重名保留第一个
                    var parameterType = FindParameterType(property.PropertyType);
                    var source = SourceDocumentation.Find("C", type, property.Name);
                    var purpose = CommandDocumentation.Purpose(type.Name, property.Name, source);
                    var title = CommandDocumentation.Title(type.Name, property.Name, source);
                    var parameterSchema = parameterType is null
                        ? (JsonElement?)null
                        : DescribeParameter(ValueContract.Schema(parameterType), title, parameterType);
                    var unavailable = source?.HasImplementation == false ? "当前 BetterGI 版本该命令为空实现。"
                        : purpose.Contains("不安排自动调用", StringComparison.Ordinal) ? "BetterGI 没有提供足以确定目标、副作用和结果的业务说明。"
                        : parameterType is not null && parameterSchema is null ? $"需要 BetterGI 的 {parameterType.Name} 对象，不能从任意 JSON 重建；应在 BetterGI 界面完成该交互。"
                        : CommandDocumentation.UnavailableReason(type.Name, property.Name);
                    var guide = new AgentGuide(
                        title, purpose,
                        unavailable is null
                            ? [$"需要执行“{title}”，且目标就是 {viewModelName} 当前界面上下文时。"]
                            : ["仅用于审计 BetterGI 行为或理解相关界面；当前接口不安排直接调用。"],
                        unavailable is null
                            ? ["命令可调用且 CanExecute=true。", "用途依赖当前选择时，已确认 BetterGI 界面选择就是目标对象。", parameterType is null ? "arguments 为空对象。" : $"argument 满足 parameterSchema（{parameterType.FullName}）。"]
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
                            FindParameterType(property.PropertyType)?.FullName,
                            IsAsyncCommand(property.PropertyType),
                            true) { Guide = guide, ParameterSchema = parameterSchema, UnavailableReason = unavailable,
                                IsDestructive = DangerousWords.Any(word => commandName.Contains(word, StringComparison.OrdinalIgnoreCase)),
                                RequiresGameReady = CommandDocumentation.RequiresGameReady(type.Name, property.Name) },
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
    public static async Task<object?> Invoke(string name, JsonElement? argument, CancellationToken cancellation)
    {
        if (_config is null || !_config.IsMethodEnabled($"cmd.{name}", "command"))
            throw BridgeException.Disabled("命令分组或该命令已禁用；通用入口不能绕过此限制。");
        var entry = Catalog.Value.FirstOrDefault(x =>
            string.Equals(x.Descriptor.Name, name, StringComparison.OrdinalIgnoreCase));
        if (entry.Descriptor is null) throw BridgeException.NotFound($"没有这个命令：{name}");
        if (entry.Descriptor.UnavailableReason is { } unavailable)
            throw new BridgeException("NOT_CALLABLE", unavailable, 409);
        if (entry.Descriptor.RequiresGameReady && !Host.CaptureReady)
            throw BridgeException.GameNotReady("该命令要求截图器和游戏环境已就绪。");

        var services = Host.Services()
            ?? throw BridgeException.Missing("拿不到 BetterGI 的服务容器。");

        // 参数转换在进 UI 线程之前完成。
        var parameter = ConvertArgument(argument, entry.Parameter);

        return await Ui.InvokeAsync(async () =>
        {
            cancellation.ThrowIfCancellationRequested();
            // 解析 ViewModel 可能在其构造函数里执行配置迁移。
            var checkpoint = SettingsTransactions.Engine.Checkpoint($"cmd.{name}");
            var viewModel = services.GetService(entry.ViewModel)
                ?? throw BridgeException.Missing($"ViewModel 未注册：{entry.ViewModel.Name}");
            if (entry.Property.GetValue(viewModel) is not ICommand command)
                throw BridgeException.Missing($"属性不是 ICommand：{entry.Property.Name}");

            if (!command.CanExecute(parameter))
                throw new BridgeException("INVALID_STATE",
                    $"命令「{name}」当前不可执行——检查页面状态和必填参数。", 409);
            cancellation.ThrowIfCancellationRequested();

            await Commands.RunAsync(command, parameter);

            return (object?)new { command = name, executed = true, configurationCheckpoint = checkpoint };
        }).ConfigureAwait(false);
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
