using System.Reflection;
using System.IO;
using System.Runtime.Loader;
using System.Text.Json;

static (string[] Api, HashSet<string> Private, string[] Native) Inspect(string path)
{
    var context = new AssemblyLoadContext(Guid.NewGuid().ToString(), true);
    context.Resolving += (_, name) => File.Exists(Path.Combine(Path.GetDirectoryName(path)!, name.Name + ".dll")) ? context.LoadFromAssemblyPath(Path.Combine(Path.GetDirectoryName(path)!, name.Name + ".dll")) : null;
    var assembly = context.LoadFromAssemblyPath(Path.GetFullPath(path));
    var types = assembly.GetTypes();
    foreach (var type in types.Where(t => t.GetProperties(BindingFlags.Public | BindingFlags.Instance).Length > 0))
        foreach (var constructor in type.GetConstructors())
            if (constructor.GetParameters().Any(p => string.IsNullOrEmpty(p.Name)))
                throw new Exception(path + ": JSON record constructor parameter metadata lost");
    foreach (var type in types.Where(t => !t.ContainsGenericParameters
        && t.GetProperty("EqualityContract", BindingFlags.NonPublic | BindingFlags.Instance) is not null))
    {
        if (type.GetProperties(BindingFlags.Public | BindingFlags.Instance).Any(p =>
            p.GetMethod?.CustomAttributes.Any(a => a.AttributeType.FullName == "System.Runtime.CompilerServices.CompilerGeneratedAttribute") != true)) continue;
        var constructor = type.GetConstructors().FirstOrDefault();
        if (constructor is null) continue;
        if (constructor.GetParameters().Any(p => !(p.ParameterType == typeof(string)
            || p.ParameterType == typeof(JsonElement) || p.ParameterType.IsPrimitive
            || p.ParameterType.IsEnum || p.ParameterType.IsArray))) continue;
        var value = constructor.Invoke(constructor.GetParameters().Select(p =>
            p.ParameterType == typeof(string) ? (object)"wire-value" :
            p.ParameterType == typeof(JsonElement) ? JsonSerializer.SerializeToElement<object?>(null) :
            p.ParameterType.IsArray ? Array.CreateInstance(p.ParameterType.GetElementType()!, 0) :
            p.ParameterType.IsValueType ? Activator.CreateInstance(p.ParameterType) : null).ToArray());
        foreach (var property in type.GetProperties(BindingFlags.Public | BindingFlags.Instance)
            .Where(p => p.CanWrite && p.PropertyType == typeof(JsonElement)))
            property.SetValue(value, JsonSerializer.SerializeToElement<object?>(null));
        var json = JsonSerializer.SerializeToElement(value, type);
        foreach (var property in type.GetProperties(BindingFlags.Public | BindingFlags.Instance))
            if (!json.TryGetProperty(property.Name, out _))
                throw new Exception(path + ": JSON record property lost: " + property.Name);
    }
    // Detect by metadata and shape: broken builds have already renamed these types.
    foreach (var original in types.Where(t => !t.IsVisible
        && t.CustomAttributes.Any(a => a.AttributeType.FullName == "System.Runtime.CompilerServices.CompilerGeneratedAttribute")
        && t.GetConstructors().Any(c => c.GetParameters().Length > 0)
        && t.GetProperties(BindingFlags.Public | BindingFlags.Instance).Length > 0))
    {
        var type = original.IsGenericTypeDefinition
            ? original.MakeGenericType(Enumerable.Repeat(typeof(string), original.GetGenericArguments().Length).ToArray())
            : original;
        var constructor = type.GetConstructors().Single();
        var parameters = constructor.GetParameters();
        if (parameters.Any(p => string.IsNullOrEmpty(p.Name)))
            throw new Exception(path + ": JSON constructor parameter metadata lost");
        var value = constructor.Invoke(parameters.Select(_ => (object)"wire-value").ToArray());
        var json = JsonSerializer.SerializeToElement(value, type);
        foreach (var parameter in parameters)
            if (json.GetProperty(parameter.Name!).GetString() != "wire-value")
                throw new Exception(path + ": JSON anonymous wire contract changed");
    }
    var api = types.Where(t => t.IsVisible).SelectMany(t => new[] { t.FullName! }.Concat(t.GetMembers(BindingFlags.Public | BindingFlags.Instance | BindingFlags.Static | BindingFlags.DeclaredOnly).Select(m => t.FullName + ":" + m))).Order().ToArray();
    var privateNames = types.SelectMany(t => t.GetMethods(BindingFlags.NonPublic | BindingFlags.Instance | BindingFlags.Static | BindingFlags.DeclaredOnly)).Where(m => !m.IsSpecialName).Select(m => m.Name).ToHashSet();
    var native = types.SelectMany(t => t.GetMethods(BindingFlags.Public | BindingFlags.NonPublic | BindingFlags.Static)).Where(m => m.CustomAttributes.Any(a => a.AttributeType.FullName == "System.Runtime.InteropServices.UnmanagedCallersOnlyAttribute")).Select(m => m.DeclaringType!.FullName + ":" + m).Order().ToArray();
    context.Unload();
    return (api, privateNames, native);
}
var report = new List<object>();
foreach (var file in new[] { "BgiBridge.dll", "BgiBridge.Recovery.dll" })
{
    var before = Inspect(Path.Combine(args[0], file));
    var after = Inspect(Path.Combine(args[1], file));
    if (!before.Api.SequenceEqual(after.Api)) throw new Exception(file + ": public API changed");
    if (!before.Native.SequenceEqual(after.Native)) throw new Exception(file + ": native entry changed");
    var changed = before.Private.Except(after.Private).Count();
    if (changed == 0) throw new Exception(file + ": no private methods protected");
    report.Add(new { file, publicApi = after.Api.Length, renamedPrivateMethods = changed, nativeEntries = after.Native.Length });
}
Console.WriteLine(JsonSerializer.Serialize(report));
