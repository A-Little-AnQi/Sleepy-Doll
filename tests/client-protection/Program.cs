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
