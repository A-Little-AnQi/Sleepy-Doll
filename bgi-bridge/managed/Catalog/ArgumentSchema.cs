using System.Text.Json;
using BgiBridge.Protocol;

namespace BgiBridge.Catalog;

/// <summary>派发前校验桥自己产出的 JSON Schema 子集。</summary>
public static class ArgumentSchema
{
    public static JsonElement Parse(string json) => JsonSerializer.Deserialize<JsonElement>(json);
    public static readonly JsonElement Empty = Parse("""{"type":"object","properties":{},"additionalProperties":false}""");

    public static void Validate(JsonElement value, JsonElement schema, string path = "arguments")
    {
        if (schema.TryGetProperty("anyOf", out var alternatives))
        {
            foreach (var alternative in alternatives.EnumerateArray())
                try { Validate(value, alternative, path); return; } catch (BridgeException) { }
            throw BridgeException.InvalidArgument($"{path} 的值不符合允许的类型。");
        }
        if (schema.TryGetProperty("type", out var type))
        {
            var matches = type.GetString() switch
            {
                "object" => value.ValueKind == JsonValueKind.Object,
                "array" => value.ValueKind == JsonValueKind.Array,
                "string" => value.ValueKind == JsonValueKind.String,
                "boolean" => value.ValueKind is JsonValueKind.True or JsonValueKind.False,
                "integer" => value.ValueKind == JsonValueKind.Number && value.TryGetDecimal(out var integer) && decimal.Truncate(integer) == integer,
                "number" => value.ValueKind == JsonValueKind.Number && value.TryGetDouble(out var number) && double.IsFinite(number),
                "null" => value.ValueKind == JsonValueKind.Null,
                _ => false,
            };
            if (!matches) throw BridgeException.InvalidArgument($"{path} 必须为 {type.GetString()}。");
        }
        if (schema.TryGetProperty("enum", out var allowed) && !allowed.EnumerateArray().Any(item => item.ToString() == value.ToString()))
            throw BridgeException.InvalidArgument($"{path} 不在允许值列表中。");
        if (value.ValueKind == JsonValueKind.Object)
        {
            if (schema.TryGetProperty("required", out var required))
                foreach (var item in required.EnumerateArray())
                    if (!value.TryGetProperty(item.GetString()!, out _)) throw BridgeException.InvalidArgument($"{path} 缺少 {item.GetString()}。");
            schema.TryGetProperty("properties", out var properties);
            foreach (var property in value.EnumerateObject())
            {
                if (properties.ValueKind == JsonValueKind.Object && properties.TryGetProperty(property.Name, out var child)) Validate(property.Value, child, $"{path}.{property.Name}");
                else if (schema.TryGetProperty("additionalProperties", out var extra) && extra.ValueKind == JsonValueKind.False)
                    throw BridgeException.InvalidArgument($"{path} 不接受参数 {property.Name}。");
            }
        }
        if (value.ValueKind == JsonValueKind.Array)
        {
            CheckBound(value.GetArrayLength(), schema, "minItems", "maxItems", path);
            if (schema.TryGetProperty("items", out var itemSchema))
                foreach (var item in value.EnumerateArray()) Validate(item, itemSchema, $"{path}[]");
        }
        if (value.ValueKind == JsonValueKind.String) CheckBound(value.GetString()!.Length, schema, "minLength", "maxLength", path);
        if (value.ValueKind == JsonValueKind.Number && value.TryGetDouble(out var n)) CheckBound(n, schema, "minimum", "maximum", path);
    }

    private static void CheckBound(double value, JsonElement schema, string min, string max, string path)
    {
        if (schema.TryGetProperty(min, out var lower) && value < lower.GetDouble()
            || schema.TryGetProperty(max, out var upper) && value > upper.GetDouble())
            throw BridgeException.InvalidArgument($"{path} 超出允许范围。");
    }
}
