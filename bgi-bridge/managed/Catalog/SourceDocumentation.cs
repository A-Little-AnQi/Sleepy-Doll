using System.Text.Json;

namespace BgiBridge.Catalog;

public sealed record SourceEntry(string Summary, string? Label, string Kind, string? ValueType,
    string? Initial, string[]? Range, string Source, int Line, bool HasCustomChangeHook, bool HasImplementation = true,
    string DocumentationSource = "host-source");

public static class SourceDocumentation
{
    private static readonly JsonElement Data = Load();
    private static readonly JsonSerializerOptions Options = new() { PropertyNameCaseInsensitive = true };
    private static readonly Dictionary<string, SourceEntry> Entries = Data.TryGetProperty("entries", out var entries)
        ? entries.Deserialize<Dictionary<string, SourceEntry>>(Options) ?? [] : [];
    public static SourceEntry? Find(string prefix, Type owner, string name)
    {
        for (var type = owner; type is not null; type = type.BaseType)
            if (Entries.TryGetValue($"{prefix}:{type.FullName}.{name}", out var entry))
            {
                // This DTO is explicitly copied into AutoFightConfig by the host.
                if (entry.Summary.Length == 0 && type.FullName == "BetterGenshinImpact.GameTask.AutoLeyLineOutcrop.AutoLeyLineOutcropFightConfig+FightFinishDetectConfig"
                    && Entries.TryGetValue($"P:BetterGenshinImpact.GameTask.AutoFight.AutoFightConfig+FightFinishDetectConfig.{name}", out var shared))
                    return entry with { Summary = "地脉花战斗检查：" + shared.Summary, DocumentationSource = "host-shared-config" };
                return entry;
            }
        return null;
    }

    public static string[]? StringEnum(SourceEntry? entry)
    {
        if (entry?.Initial is not { } expression || !expression.EndsWith(".ToString()")) return null;
        var type = expression.Split('.')[0];
        return Data.TryGetProperty("enums", out var enums) && enums.TryGetProperty(type, out var values)
            ? values.Deserialize<string[]>() : null;
    }

    private static JsonElement Load()
    {
        var assembly = typeof(SourceDocumentation).Assembly;
        var name = assembly.GetManifestResourceNames().SingleOrDefault(n => n.EndsWith("host-documentation.json"));
        if (name is null) return ArgumentSchema.Parse("{}");
        using var stream = assembly.GetManifestResourceStream(name)!;
        return JsonSerializer.Deserialize<JsonElement>(stream);
    }
}
