using System.Collections;
using System.Reflection;
using System.Runtime.CompilerServices;
using System.Text.Json;
using System.Windows.Input;
using BgiBridge.Catalog;
using BgiBridge.Protocol;

namespace BgiBridge.Bgi;

/// <summary>命令使用真实宿主实例。枚举不构造窗口；构造是单独的写操作。</summary>
public static class CommandTargets
{
    private sealed record Handle(WeakReference<object> Value, bool Owned);
    private static readonly Dictionary<string, Handle> Handles = new();
    private static readonly Dictionary<string, object> Owned = new();
    private static readonly Dictionary<string, WeakReference<object>> Borrowed = new();
    private static readonly ConditionalWeakTable<object, Identity> Identities = new();
    private sealed class Identity { public string Id { get; } = Guid.NewGuid().ToString("N"); }
    private const BindingFlags Public = BindingFlags.Instance | BindingFlags.Public;
    private const BindingFlags Fields = BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic;

    public static IEnumerable<Type> Types()
    {
        var assembly = Reflect.FindAssembly(Reflect.HostAssembly);
        if (assembly is null) return [];
        try { return assembly.GetTypes().Where(type => type.FullName?.StartsWith("BetterGenshinImpact.", StringComparison.Ordinal) == true); }
        catch (ReflectionTypeLoadException error) { return error.Types.OfType<Type>(); }
    }

    public static bool Matches(Type expected, Type actual)
    {
        if (expected.IsAssignableFrom(actual)) return true;
        for (var type = actual; type is not null; type = type.BaseType)
            if (type.IsGenericType && type.GetGenericTypeDefinition() == expected) return true;
        return false;
    }

    private static bool HostObject(object value) => value.GetType().Assembly.GetName().Name == Reflect.HostAssembly;

    // Windows/DataContext、宿主对象的真实字段和集合构成绑定图；不调用 getter 来创建服务。
    public static IReadOnlyList<object> Snapshot()
    {
        var queue = new Queue<object>();
        var application = Reflect.FindType("System.Windows.Application");
        var app = application is null ? null : Reflect.GetStatic(application, "Current");
        if (app is not null && Reflect.Get(app, "Windows") is IEnumerable windows)
            foreach (var window in windows) if (window is not null) queue.Enqueue(window);
        foreach (var value in Owned.Values) queue.Enqueue(value);
        foreach (var value in Borrowed.Values) if (value.TryGetTarget(out var service)) queue.Enqueue(service);
        var visited = new HashSet<object>(ReferenceEqualityComparer.Instance);
        var result = new List<object>();
        while (queue.Count > 0 && visited.Count < 4096)
        {
            var value = queue.Dequeue();
            if (!visited.Add(value)) continue;
            result.Add(value);
            if (value is IEnumerable sequence && value is not string)
            {
                foreach (var item in sequence.Cast<object?>().Take(512)) if (item is not null && !item.GetType().IsValueType && item is not string) queue.Enqueue(item);
                continue;
            }
            foreach (var name in new[] { "DataContext", "Content", "Child", "Children" })
            {
                try { if (Reflect.Get(value, name) is { } child && child is not string) queue.Enqueue(child); } catch { }
            }
            if (!HostObject(value)) continue;
            for (var type = value.GetType(); type is not null && type.Assembly.GetName().Name == Reflect.HostAssembly; type = type.BaseType)
                foreach (var field in type.GetFields(Fields | BindingFlags.DeclaredOnly))
                {
                    if (field.IsStatic || typeof(Delegate).IsAssignableFrom(field.FieldType)) continue;
                    try
                    {
                        var child = field.GetValue(value);
                        if (child is not null && child is not string && !child.GetType().IsValueType
                            && child is not ICommand && (HostObject(child) || child is IEnumerable || field.FieldType.FullName is "OpenCvSharp.Mat" or "System.Globalization.CultureInfo")) queue.Enqueue(child);
                    }
                    catch { }
                }
        }
        return result;
    }

    private static string Register(object value, bool owned = false)
    {
        var id = Identities.GetValue(value, _ => new Identity()).Id;
        Handles[id] = new(new(value), owned);
        if (owned) Owned[id] = value;
        return id;
    }

    private static object Describe(object value) => new
    {
        objectId = Register(value, Owned.Values.Any(item => ReferenceEquals(item, value))),
        type = value.GetType().FullName,
        name = SafeName(value),
    };

    private static string? SafeName(object value)
    {
        foreach (var name in new[] { "Name", "FolderName", "Title" })
            try { if (Reflect.Get(value, name) is string text && text.Length > 0) return text.Length <= 160 ? text : text[..160]; } catch { }
        return null;
    }

    public static object Resolve(string id, Type expected)
    {
        if (!Handles.TryGetValue(id, out var handle) || !handle.Value.TryGetTarget(out var value))
            throw new BridgeException("STALE_TARGET", "对象引用已失效，重新读取命令目标。", 409);
        if (!Matches(expected, value.GetType())) throw BridgeException.InvalidArgument("对象引用类型与当前命令不匹配。");
        if (!handle.Owned && !Snapshot().Any(item => ReferenceEquals(item, value)))
            throw new BridgeException("STALE_TARGET", "目标已不在当前宿主对象图中；不能执行旧选择。", 409);
        return value;
    }

    public static object Owner(Type type, string? contextId)
    {
        if (!string.IsNullOrEmpty(contextId)) return Resolve(contextId, type);
        var existing = Snapshot().Where(item => Matches(type, item.GetType())).ToArray();
        if (existing.Length == 1) return existing[0];
        if (existing.Length > 1) throw new BridgeException("AMBIGUOUS_TARGET", "存在多个命令上下文；读取目标并传 contextId。", 409);
        if (!type.IsAbstract && !type.ContainsGenericParameters && Host.Services()?.GetService(type) is { } service)
        { Borrowed[Register(service)] = new(service); return service; }
        throw new BridgeException("TARGET_REQUIRED", "命令已登记，但上下文尚未建立；读取构造契约后创建或打开目标上下文。", 409);
    }

    public static JsonElement ReferenceSchema => ArgumentSchema.Parse("""{"type":"object","properties":{"objectId":{"type":"string","minLength":1,"maxLength":64,"description":"list_command_targets 返回的真实对象引用，不接受类名、伪造字段或索引。"}},"required":["objectId"],"additionalProperties":false}""");

    public static JsonElement Schema(Type type) => type == typeof(object)
        ? JsonSerializer.SerializeToElement(new { anyOf = new[] { ReferenceSchema, ArgumentSchema.Parse("""{"type":"string","maxLength":4096}"""), ArgumentSchema.Parse("""{"type":"number"}"""), ArgumentSchema.Parse("""{"type":"boolean"}""") } })
        : ValueContract.Schema(type) ?? ReferenceSchema;

    public static JsonElement SelectionSchema(Type type) => AgentSchemas.Object(type.GetProperties(Public)
        .Where(p => p.Name.StartsWith("Selected", StringComparison.Ordinal) && p.CanWrite && p.SetMethod?.IsPublic == true && p.GetIndexParameters().Length == 0)
        .Select(p => (p.Name, Schema(p.PropertyType), false)).ToArray());

    public static void Select(object owner, JsonElement selection)
    {
        ArgumentSchema.Validate(selection, SelectionSchema(owner.GetType()));
        var changes = selection.EnumerateObject().Select(value =>
        {
            var property = owner.GetType().GetProperty(value.Name)!;
            return (Property: property, Value: Argument(value.Value, property.PropertyType), Prior: property.GetValue(owner));
        }).ToArray();
        try { foreach (var change in changes) change.Property.SetValue(owner, change.Value); }
        catch
        {
            foreach (var change in Enumerable.Reverse(changes)) change.Property.SetValue(owner, change.Prior);
            throw;
        }
    }

    public static object? Argument(JsonElement value, Type type)
    {
        if (value.ValueKind == JsonValueKind.Object && value.TryGetProperty("objectId", out var id))
        {
            ArgumentSchema.Validate(value, ReferenceSchema);
            return Resolve(id.GetString()!, type);
        }
        if (type == typeof(object))
        {
            ArgumentSchema.Validate(value, Schema(type));
            return value.ValueKind switch { JsonValueKind.String => value.GetString(), JsonValueKind.True => true, JsonValueKind.False => false,
                JsonValueKind.Number => value.TryGetInt64(out var integer) ? (object)integer : value.GetDouble(), _ => throw BridgeException.InvalidArgument("object 参数仅接受原生引用或原始标量。") };
        }
        var schema = ValueContract.Schema(type) ?? throw BridgeException.InvalidArgument("此参数必须引用当前宿主对象，先读取命令目标。");
        ArgumentSchema.Validate(value, schema);
        return JsonSerializer.Deserialize(value.GetRawText(), type, ValueContract.Json);
    }

    public static object List(Type owner, Type? parameter, string query = "", int offset = 0, int limit = 20)
    {
        var snapshot = Snapshot();
        var arguments = parameter is null ? [] : snapshot.Where(item => !item.GetType().IsValueType && Matches(parameter, item.GetType())
            && (query.Length == 0 || ((SafeName(item) ?? "") + " " + item.GetType().FullName).Contains(query, StringComparison.OrdinalIgnoreCase))).ToArray();
        return new {
        ownerType = owner.FullName,
        contexts = snapshot.Where(item => Matches(owner, item.GetType())).Take(32).Select(Describe).ToArray(),
        arguments = arguments.Skip(offset).Take(limit).Select(Describe).ToArray(),
        total = arguments.Length,
        nextOffset = offset + limit < arguments.Length ? (int?)(offset + limit) : null,
        constructors = owner.GetConstructors().Select(ctor => new
        {
            parameters = ctor.GetParameters().Select(p => new { name = p.Name, type = p.ParameterType.FullName, optional = p.IsOptional, schema = Schema(p.ParameterType), fromServices = Host.Services() is { } services && Host.IsRegistered(services, p.ParameterType) == true }).ToArray(),
        }).ToArray(),
        argumentType = parameter?.FullName ?? parameter?.Name,
        argumentConstructors = parameter?.GetConstructors().Select(ctor => new
        {
            parameters = ctor.GetParameters().Select(p => new { name = p.Name, type = p.ParameterType.FullName, optional = p.IsOptional, schema = Schema(p.ParameterType) }).ToArray(),
        }).ToArray(),
        requiresExistingGenericContext = owner.ContainsGenericParameters,
        selectionSchema = SelectionSchema(owner),
        next = "已有上下文使用 contextId；复杂参数使用真实 objectId。需要新上下文时 create_command_target。构造或打开页面不表示业务目标完成。",
        };
    }

    public static object Create(Type type, JsonElement values)
    {
        if (type.IsAbstract || type.ContainsGenericParameters) throw BridgeException.InvalidArgument("抽象或开放泛型必须绑定已有具体实例，不能猜测泛型类型。");
        if (!values.EnumerateObject().Any() && Host.Services() is { } services && Host.IsRegistered(services, type) == true && services.GetService(type) is { } reused)
        {
            Borrowed[Register(reused)] = new(reused);
            return new { created = false, reused = true, target = Describe(reused), verified = true };
        }
        if (Owned.Count >= 32) throw new BridgeException("TARGET_LIMIT", "释放已创建的无用命令上下文后再创建。", 409);
        var candidates = type.GetConstructors().Where(ctor =>
        {
            var parameters = ctor.GetParameters();
            return values.EnumerateObject().All(item => parameters.Any(p => p.Name == item.Name))
                && parameters.All(p => values.TryGetProperty(p.Name!, out _) || p.IsOptional || Host.Services() is { } services && Host.IsRegistered(services, p.ParameterType) == true);
        }).ToArray();
        if (candidates.Length != 1) throw BridgeException.InvalidArgument("构造参数未唯一匹配；按当前目标契约补齐参数，不能任意创建对象。");
        var ctor = candidates[0];
        var args = ctor.GetParameters().Select(p => values.TryGetProperty(p.Name!, out var value) ? Argument(value, p.ParameterType)
            : p.IsOptional ? p.DefaultValue : Host.Services()!.GetService(p.ParameterType)).ToArray();
        var instance = ctor.Invoke(args);
        Register(instance, true);
        return new { created = true, target = Describe(instance), verified = true };
    }

    public static object Release(string id)
    {
        if (!Owned.Remove(id, out var value)) throw BridgeException.InvalidArgument("只能释放由桥创建的上下文，不能关闭用户已有对象。");
        Handles.Remove(id);
        if (Reflect.FindType("System.Windows.Window")?.IsInstanceOfType(value) == true) Reflect.Call(value, "Close");
        if (value is IDisposable disposable) disposable.Dispose();
        return new { released = true, verified = true };
    }
}
