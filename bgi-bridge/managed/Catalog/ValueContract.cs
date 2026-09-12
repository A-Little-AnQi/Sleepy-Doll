using System.ComponentModel.DataAnnotations;
using System.Reflection;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using System.Text.Json.Nodes;
using System.Text.Json.Serialization;
using BgiBridge.Protocol;

namespace BgiBridge.Catalog;

public static class ValueContract
{
    private static readonly byte[] VersionKey = RandomNumberGenerator.GetBytes(32);
    public static readonly JsonSerializerOptions Json = new()
    {
        PropertyNamingPolicy = JsonNamingPolicy.CamelCase,
        Converters = { new JsonStringEnumConverter() },
    };

    public static JsonElement Snapshot(object? value, Type type) => SerializeBounded(value, type, Json, 256 * 1024);
    public static JsonElement SerializeBounded(object? value, Type type, JsonSerializerOptions options, int limit)
    {
        using var stream = new LimitedStream(limit);
        JsonSerializer.Serialize(stream, value, type, options);
        return JsonSerializer.Deserialize<JsonElement>(stream.ToArray());
    }
    private sealed class LimitedStream(int limit) : MemoryStream
    {
        public override void Write(byte[] buffer, int offset, int count)
        {
            if (Length + count > limit) throw new InvalidOperationException("配置序列化超过安全大小限制。");
            base.Write(buffer, offset, count);
        }
        public override void Write(ReadOnlySpan<byte> buffer)
        {
            if (Length + buffer.Length > limit) throw new InvalidOperationException("配置序列化超过安全大小限制。");
            base.Write(buffer);
        }
    }
    public static string Version(JsonElement value) => System.Convert.ToHexString(HMACSHA256.HashData(VersionKey, Encoding.UTF8.GetBytes(Canonical(value)))).ToLowerInvariant();
    public static string Canonical(JsonElement value) => value.ValueKind switch
    {
        JsonValueKind.Object => "{" + string.Join(",", value.EnumerateObject().OrderBy(p => p.Name, StringComparer.Ordinal)
            .Select(p => JsonSerializer.Serialize(p.Name) + ":" + Canonical(p.Value))) + "}",
        JsonValueKind.Array => "[" + string.Join(",", value.EnumerateArray().Select(Canonical)) + "]",
        _ => value.GetRawText(),
    };

    public static JsonElement? Schema(Type type, PropertyInfo? property = null, string? path = null)
    {
        var nullable = Nullable.GetUnderlyingType(type);
        if (nullable is not null)
        {
            var inner = Schema(nullable, property, path);
            return inner is null ? null : JsonSerializer.SerializeToElement(new { anyOf = new[] { inner.Value, ArgumentSchema.Parse("""{"type":"null"}""") } });
        }
        JsonObject? schema = null;
        if (type == typeof(bool)) schema = new() { ["type"] = "boolean" };
        else if (type.IsEnum) schema = new() { ["type"] = "string", ["enum"] = new JsonArray(Enum.GetNames(type).Select(n => (JsonNode?)JsonValue.Create(n)).ToArray()) };
        else if (type == typeof(string))
        {
            schema = new() { ["type"] = "string", ["maxLength"] = 16384 };
            var allowed = property is null ? null : SourceDocumentation.StringEnum(SourceDocumentation.Find("P", property.DeclaringType!, property.Name));
            if (allowed is not null) schema["enum"] = new JsonArray(allowed.Select(n => (JsonNode?)JsonValue.Create(n)).ToArray());
        }
        else if (type == typeof(int) || type == typeof(long) || type == typeof(short) || type == typeof(byte)
            || type == typeof(uint) || type == typeof(ulong))
        {
            schema = new() { ["type"] = "integer" };
            schema["minimum"] = type == typeof(byte) || type == typeof(uint) || type == typeof(ulong) ? 0
                : type == typeof(short) ? short.MinValue : type == typeof(int) ? int.MinValue : long.MinValue;
            schema["maximum"] = type == typeof(byte) ? byte.MaxValue : type == typeof(short) ? short.MaxValue
                : type == typeof(int) ? int.MaxValue : type == typeof(uint) ? uint.MaxValue : long.MaxValue;
        }
        else if (type == typeof(double) || type == typeof(float) || type == typeof(decimal)) schema = new() { ["type"] = "number" };
        else
        {
            var element = type.IsArray ? type.GetElementType() : type.IsGenericType
                && type.GetGenericTypeDefinition() is var generic
                && (generic == typeof(List<>) || generic == typeof(System.Collections.ObjectModel.ObservableCollection<>))
                ? type.GetGenericArguments()[0] : null;
            if (element is not null && Schema(element) is { } item)
                schema = new() { ["type"] = "array", ["maxItems"] = 1000, ["items"] = JsonNode.Parse(item.GetRawText()) };
        }
        if (schema is null) return null;
        if (property?.GetCustomAttribute<RangeAttribute>() is { } range)
        {
            if (double.TryParse(range.Minimum.ToString(), out var min)) schema["minimum"] = min;
            if (double.TryParse(range.Maximum.ToString(), out var max)) schema["maximum"] = max;
        }
        if (path == "triggerInterval") { schema["minimum"] = 1; schema["maximum"] = 10000; }
        if (schema["type"]?.ToString() is "integer" or "number"
            && property is not null && new[] { "Interval", "Timeout", "Delay", "Duration" }.Any(word => property.Name.Contains(word, StringComparison.OrdinalIgnoreCase)))
            schema["minimum"] = Math.Max(path == "triggerInterval" ? 1 : 0, double.TryParse(schema["minimum"]?.ToString(), out var existingMin) ? existingMin : 0);
        return JsonSerializer.SerializeToElement(schema);
    }

    public static object? Convert(JsonElement value, PropertyInfo property, string path)
    {
        var schema = Schema(property.PropertyType, property, path)
            ?? throw BridgeException.InvalidArgument($"设置 {path} 的复合类型尚无安全写入契约。");
        ArgumentSchema.Validate(value, schema, $"value({path})");
        object? converted;
        try { converted = JsonSerializer.Deserialize(value.GetRawText(), property.PropertyType, Json); }
        catch { throw BridgeException.InvalidArgument($"设置 {path} 的值无法转换为 {property.PropertyType.Name}。"); }
        foreach (var constraint in property.GetCustomAttributes<ValidationAttribute>())
            if (!constraint.IsValid(converted)) throw BridgeException.InvalidArgument($"设置 {path} 不满足 {constraint.GetType().Name} 约束。");
        return converted;
    }
}
