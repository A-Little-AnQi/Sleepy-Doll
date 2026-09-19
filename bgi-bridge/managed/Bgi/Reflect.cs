using System.Collections.Concurrent;
using System.Reflection;
using BgiBridge.Protocol;

namespace BgiBridge.Bgi;

/// <summary>与宿主的唯一接触面，全部走反射。桥的程序集在隔离的 ALC 里，按类型标识取不到宿主类型，只能按已加载程序集定位。</summary>
public static class Reflect
{
    private static readonly ConcurrentDictionary<string, Type?> TypeCache = new(StringComparer.Ordinal);
    private static readonly ConcurrentDictionary<(Type, string), MemberInfo?> MemberCache = new();
    private static readonly ConcurrentDictionary<(Type, string, int), MethodInfo?> MethodCache = new();

    /// <summary>csproj 里是 &lt;AssemblyName&gt;BetterGI&lt;/AssemblyName&gt;，不是项目名。</summary>
    public const string HostAssembly = "BetterGI";

    // ---------- 程序集与类型 ----------

    /// <summary>按简单名在所有已加载程序集里查找（含其它 ALC 的）。</summary>
    public static Assembly? FindAssembly(string simpleName) =>
        AppDomain.CurrentDomain.GetAssemblies()
            .FirstOrDefault(a =>
                string.Equals(a.GetName().Name, simpleName, StringComparison.OrdinalIgnoreCase));

    /// <summary>按已知类型判断宿主是否就绪。</summary>
    public static bool HostReady => FindType("BetterGenshinImpact.GameTask.TaskContext") is not null;

    /// <summary>只按完整类型名匹配，不绑定同名的其他类型。</summary>
    public static Type? FindType(string fullName)
    {
        if (TypeCache.TryGetValue(fullName, out var cached) && cached is not null) return cached;
        var assemblies = AppDomain.CurrentDomain.GetAssemblies();
        foreach (var assembly in assemblies)
        {
            if (fullName.StartsWith("BetterGenshinImpact.", StringComparison.Ordinal)
                && assembly.GetName().Name != HostAssembly) continue;
            try
            {
                var match = assembly.GetType(fullName, throwOnError: false, ignoreCase: false);
                if (match is null) continue;
                TypeCache[fullName] = match;
                return match;
            }
            catch { }
        }
        // 不缓存缺失：程序集可能稍后才加载。
        return null;
    }

    /// <summary>按「类型上有这个方法」定位宿主类型，不依赖页面类的完整名称。</summary>
    public static Type? FindHostTypeWithMethod(
        string method,
        params Type[] parameterTypes)
    {
        var assembly = FindAssembly(HostAssembly);
        if (assembly is null) return null;
        Type[] types;
        try
        {
            types = assembly.GetTypes();
        }
        catch (ReflectionTypeLoadException ex)
        {
            types = ex.Types.Where(type => type is not null).Cast<Type>().ToArray();
        }

        const BindingFlags Flags = BindingFlags.Public | BindingFlags.Instance;
        return types.FirstOrDefault(type => type.GetMethods(Flags).Any(candidate =>
        {
            if (!string.Equals(candidate.Name, method, StringComparison.Ordinal)) return false;
            var parameters = candidate.GetParameters();
            return parameters.Length == parameterTypes.Length
                   && parameters.Select(parameter => parameter.ParameterType)
                       .SequenceEqual(parameterTypes);
        }));
    }

    /// <summary>取宿主类型，缺失时抛出提示版本不匹配的错误。</summary>
    public static Type RequireType(string fullName) =>
        FindType(fullName) ?? throw BridgeException.Missing(
            $"BetterGI 里找不到类型 {fullName}。通常是 BetterGI 版本与桥不匹配。");

    // ---------- 成员查找 ----------

    private static MemberInfo? FindMember(Type type, string name) =>
        MemberCache.GetOrAdd((type, name), static key =>
        {
            var (t, n) = key;
            const BindingFlags Flags = BindingFlags.Public | BindingFlags.NonPublic
                                     | BindingFlags.Instance | BindingFlags.Static
                                     | BindingFlags.FlattenHierarchy;
            const BindingFlags Loose = Flags | BindingFlags.IgnoreCase;

            // 顺序：属性优先于字段，精确大小写优先于忽略大小写。
            if (FindProperty(t, n, Flags) is { } property) return property;
            if (t.GetField(n, Flags) is { } field) return field;
            if (FindProperty(t, n, Loose) is { } looseProperty) return looseProperty;
            if (t.GetField(n, Loose) is { } looseField) return looseField;
            return null;
        });

    /// <summary>容忍同名重载（宿主的 App.ServiceProvider 就是这种）：GetProperty(name, flags) 遇多个同名会抛 AmbiguousMatchException，退化为优先静态、其次可读。</summary>
    private static PropertyInfo? FindProperty(Type type, string name, BindingFlags flags)
    {
        try
        {
            return type.GetProperty(name, flags);
        }
        catch (AmbiguousMatchException)
        {
            // 落到下面的多候选挑选。
        }
        catch
        {
            return null;
        }

        var matches = type.GetProperties(flags)
            .Where(p => string.Equals(p.Name, name, StringComparison.Ordinal))
            .ToArray();
        if (matches.Length == 0) return null;

        return matches.FirstOrDefault(p => p.GetMethod?.IsStatic == true)
            ?? matches.FirstOrDefault(p => p.GetMethod is not null)
            ?? matches[0];
    }

    /// <summary>按名字与参数个数取方法：先精确匹配，再忽略大小写。</summary>
    public static MethodInfo? FindMethod(Type type, string name, int argCount) =>
        MethodCache.GetOrAdd((type, name, argCount), static key =>
        {
            var (t, n, count) = key;
            const BindingFlags Flags = BindingFlags.Public | BindingFlags.NonPublic
                                     | BindingFlags.Instance | BindingFlags.Static
                                     | BindingFlags.FlattenHierarchy;
            return t.GetMethods(Flags)
                .FirstOrDefault(m => string.Equals(m.Name, n, StringComparison.Ordinal)
                                     && m.GetParameters().Length == count)
                ?? t.GetMethods(Flags)
                    .FirstOrDefault(m => string.Equals(m.Name, n, StringComparison.OrdinalIgnoreCase)
                                         && m.GetParameters().Length == count);
        });

    // ---------- 读写 ----------

    public static object? GetStatic(Type type, string member)
    {
        var info = FindMember(type, member) ?? throw BridgeException.Missing(
            $"{type.Name}.{member} 在当前 BetterGI 版本里不存在。");
        try
        {
            return info switch
            {
                PropertyInfo p => p.GetValue(null),
                FieldInfo f => f.GetValue(null),
                _ => throw BridgeException.Missing($"{type.Name}.{member} 不是字段或属性。"),
            };
        }
        catch (TargetInvocationException ex)
        {
            throw BridgeException.Failed($"{type.Name}.{member} 取值抛异常：{Root(ex).Message}");
        }
    }

    public static object? Get(object? target, string member)
    {
        if (target is null) return null;
        var info = FindMember(target.GetType(), member) ?? throw BridgeException.Missing(
            $"{target.GetType().Name}.{member} 在当前 BetterGI 版本里不存在。");
        try
        {
            return info switch
            {
                PropertyInfo p => p.GetValue(target),
                FieldInfo f => f.GetValue(target),
                _ => throw BridgeException.Missing($"{target.GetType().Name}.{member} 不是字段或属性。"),
            };
        }
        catch (TargetInvocationException ex)
        {
            throw BridgeException.Failed($"{target.GetType().Name}.{member} 取值抛异常：{Root(ex).Message}");
        }
    }

    /// <summary>取 BGI 的静态单例：<c>Type.Instance()</c> 或 <c>Type.Instance</c> 两种写法都试。</summary>
    public static object? Singleton(string typeFullName)
    {
        var type = FindType(typeFullName);
        return type is null ? null : Singleton(type);
    }

    /// <summary>按已发现的宿主类型取得静态 Instance 属性或 Instance() 方法。</summary>
    public static object? Singleton(Type type)
    {
        var property = FindMember(type, "Instance");
        if (property is not null)
        {
            try
            {
                return property switch
                {
                    PropertyInfo p => p.GetValue(null),
                    FieldInfo f => f.GetValue(null),
                    _ => null,
                };
            }
            catch
            {
                return null;
            }
        }

        var method = FindMethod(type, "Instance", 0);
        if (method is null) return null;
        try
        {
            return method.Invoke(null, null);
        }
        catch
        {
            return null;
        }
    }

    // ---------- 调用 ----------

    public static object? Call(object? target, string method, params object?[] args)
    {
        if (target is null) throw BridgeException.Missing($"目标为 null，无法调用 {method}。");
        return Invoke(target.GetType(), target, method, args);
    }

    public static object? CallStatic(Type type, string method, params object?[] args) =>
        Invoke(type, null, method, args);

    private static object? Invoke(Type type, object? target, string method, object?[] args)
    {
        var info = FindMethod(type, method, args.Length)
            ?? type.GetMethods(BindingFlags.Public | BindingFlags.NonPublic | BindingFlags.Instance | BindingFlags.Static | BindingFlags.FlattenHierarchy)
                   .FirstOrDefault(m => string.Equals(m.Name, method, StringComparison.OrdinalIgnoreCase)
                                        && m.GetParameters().Length == args.Length)
            ?? throw BridgeException.Missing(
                $"{type.Name}.{method}({args.Length} 参数) 在当前 BetterGI 版本里不存在。");
        try
        {
            return info.Invoke(target, args);
        }
        catch (TargetInvocationException ex)
        {
            // 宿主内部的异常翻译成桥的错误。
            throw Translate(Root(ex));
        }
    }

    /// <summary>await 一个可能是 Task / Task&lt;T&gt; / ValueTask 的返回值。</summary>
    public static async Task<object?> Await(object? value)
    {
        switch (value)
        {
            case null:
                return null;
            case Task task:
                await task.ConfigureAwait(false);
                return task.GetType().IsGenericType
                    ? task.GetType().GetProperty("Result")?.GetValue(task)
                    : null;
            case ValueTask valueTask:
                await valueTask.ConfigureAwait(false);
                return null;
            default:
                return value;
        }
    }

    public static Exception Root(Exception ex) =>
        ex is TargetInvocationException { InnerException: { } inner } ? Root(inner) : ex;

    /// <summary>把宿主抛出的异常映射到桥的错误码。</summary>
    public static BridgeException Translate(Exception ex) => ex switch
    {
        BridgeException bridge => bridge,
        OperationCanceledException => new BridgeException("CANCELLED", "操作已取消。"),
        UnauthorizedAccessException => new BridgeException("FORBIDDEN", ex.Message, 403),
        ArgumentException => BridgeException.InvalidArgument(ex.Message),
        InvalidOperationException => new BridgeException("INVALID_STATE", ex.Message, 409),
        TimeoutException => new BridgeException("TIMEOUT", ex.Message, 504),
        _ => BridgeException.Failed($"{ex.GetType().Name}: {ex.Message}"),
    };
}
