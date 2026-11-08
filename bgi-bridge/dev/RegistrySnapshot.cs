using System.Collections;
using System.IO;
using System.Reflection;
using System.Runtime.Loader;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using System.Text.Json.Nodes;

internal static class RegistrySnapshot
{
    internal static void Run(string[] args)
    {
        var output = Path.GetFullPath(args[1]);
        var assembly = args.Length > 2
            ? new AssemblyLoadContext("interface-export", true).LoadFromAssemblyPath(Path.GetFullPath(args[2]))
            : typeof(BgiBridge.Catalog.MethodRegistry).Assembly;
        var registryType = assembly.GetType("BgiBridge.Catalog.MethodRegistry", true)!;
        var records = new SortedDictionary<string, (JsonObject Entry, string Owner)>(StringComparer.Ordinal);
        foreach (var type in assembly.GetTypes().Where(type => type.Namespace == "BgiBridge.Tools").OrderBy(type => type.FullName))
        {
            var register = type.GetMethods().SingleOrDefault(method => method.Name == "Register" && method.IsStatic
                && method.GetParameters() is [{ ParameterType: var parameter }] && parameter == registryType);
            if (register is null) continue;
            var registry = Activator.CreateInstance(registryType)!;
            register.Invoke(null, [registry]);
            foreach (var descriptor in (IEnumerable)registryType.GetProperty("All")!.GetValue(registry)!)
            {
                var id = (string)Read(descriptor, "Id")!;
                var entry = new JsonObject();
                foreach (var property in new[] { "Group", "Summary", "ReadOnly", "Destructive", "Effect", "RequiresGameReady", "Preparation", "OwnsHostCancellation", "InputSchema", "OutputSchema" })
                    entry[JsonNamingPolicy.CamelCase.ConvertName(property)] = JsonSerializer.SerializeToNode(Read(descriptor, property));
                entry["guide"] = JsonSerializer.SerializeToNode(Read(descriptor, "Guide"), new JsonSerializerOptions { PropertyNamingPolicy = JsonNamingPolicy.CamelCase });
                if (records.TryGetValue(id, out var previous))
                {
                    if (!JsonNode.DeepEquals(entry, previous.Entry)) throw new InvalidOperationException("重复且不一致的接口：" + id);
                    if (previous.Owner != "CatalogTools") continue;
                }
                records[id] = (entry, type.Name);
            }
        }
        var result = new JsonObject();
        foreach (var (id, value) in records)
        {
            value.Entry["source"] = new JsonObject { ["path"] = $"bgi-bridge/managed/Tools/{value.Owner}.cs", ["registrationType"] = $"BgiBridge.Tools.{value.Owner}" };
            result[id] = value.Entry;
        }
        if (args[0] == "--check-interfaces")
        {
            var expected = JsonNode.Parse(File.ReadAllText(output))!.AsObject();
            if (!JsonNode.DeepEquals(expected, result))
            {
                var changed = expected.Select(pair => pair.Key).Union(result.Select(pair => pair.Key))
                    .Where(id => !JsonNode.DeepEquals(expected[id], result[id])).Order(StringComparer.Ordinal);
                throw new InvalidOperationException("接口快照与当前编译结果不一致：" + string.Join(", ", changed));
            }
            var root = Path.GetFullPath(Path.Combine(Path.GetDirectoryName(output)!, "..", ".."));
            var indexFile = Path.Combine(root, "plugins", "bgi", "resources", "feature-index.json");
            if (File.Exists(indexFile))
            {
                using var hash = IncrementalHash.CreateHash(HashAlgorithmName.SHA256);
                foreach (var relative in new[] { "bgi-bridge/dev/fixtures/host-features.json", "bgi-bridge/dev/bgi-method-metadata.json", "bgi-bridge/managed/generated/host-documentation.json", "bgi-bridge/dev/fixtures/host-surface.json" })
                {
                    var text = File.ReadAllText(Path.Combine(root, relative)).Replace("\r\n", "\n", StringComparison.Ordinal).Replace('\r', '\n');
                    hash.AppendData(SHA256.HashData(Encoding.UTF8.GetBytes(text)));
                }
                var index = JsonNode.Parse(File.ReadAllText(indexFile))!;
                var actual = Convert.ToHexString(hash.GetHashAndReset()).ToLowerInvariant();
                if (index["indexInputsHash"]?.GetValue<string>() != actual)
                    throw new InvalidOperationException("功能索引与接口／源码输入不一致，请重新生成 feature-index.json");
            }
        }
        else
        {
            Directory.CreateDirectory(Path.GetDirectoryName(output)!);
            File.WriteAllText(output, result.ToJsonString(new JsonSerializerOptions { WriteIndented = true,
                Encoder = System.Text.Encodings.Web.JavaScriptEncoder.UnsafeRelaxedJsonEscaping }));
        }
        Console.WriteLine(JsonSerializer.Serialize(new { interfaces = result.Count, check = args[0] == "--check-interfaces" }));
    }

    private static object? Read(object value, string name) => value.GetType().GetProperty(name)?.GetValue(value);
}
