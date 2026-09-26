using System.Text.Json;
using BgiBridge.Protocol;

namespace BgiBridge.Catalog;

/// <summary>为已核对的宿主联动字段补齐事务范围。</summary>
public static class SettingMutationAdapters
{
    public static readonly IReadOnlyDictionary<string, (double Min, double Max)> Bounded = new Dictionary<string, (double, double)>(StringComparer.Ordinal)
    {
        ["autoBossConfig.runCount"] = (1, int.MaxValue), ["autoBossConfig.reviveRetryCount"] = (0, int.MaxValue),
        ["maskWindowConfig.crosshairLineWidth"] = (1, 100), ["maskWindowConfig.crosshairSize"] = (1, 1000), ["maskWindowConfig.crosshairGap"] = (0, 1000),
        ["maskWindowConfig.logFontScale"] = (0.5, 3), ["maskWindowConfig.metricsFontScale"] = (0.5, 3),
        ["skillCdConfig.px"] = (0, 1920), ["skillCdConfig.py"] = (0, 1080), ["skillCdConfig.gap"] = (0, 200), ["skillCdConfig.scale"] = (0, 10),
        ["tpConfig.mapZoomInDistance"] = (200, 600), ["tpConfig.mapZoomOutDistance"] = (600, int.MaxValue),
        ["tpConfig.teleportOperationDelayMilliseconds"] = (2, 100), ["tpConfig.hpRestoreDuration"] = (1, 30)
    };
    public static readonly HashSet<string> Colors = ["skillCdConfig.backgroundNormalColor", "skillCdConfig.backgroundReadyColor", "skillCdConfig.textNormalColor", "skillCdConfig.textReadyColor"];
    public static bool ReviewedHook(string path) => Bounded.ContainsKey(path) || Colors.Contains(path) || path == "autoBossConfig.specifyRunCount";

    public static IReadOnlyList<SettingChange> Expand(IReadOnlyList<SettingChange> requested, Dictionary<string, SettingEntry> entries)
    {
        var changes = requested.ToList();
        var mode = changes.FirstOrDefault(change => change.Path.Equals("autoBossConfig.specifyRunCount", StringComparison.OrdinalIgnoreCase));
        if (mode is null) return changes;
        foreach (var path in new[] { "autoBossConfig.useTransientResin", "autoBossConfig.useFragileResin" })
        {
            var entry = entries.GetValueOrDefault(path) ?? throw BridgeException.Missing("首领模式联动字段不完整，未修改。");
            var supplied = changes.FirstOrDefault(change => change.Path.Equals(path, StringComparison.OrdinalIgnoreCase));
            if (mode.Value.ValueKind == JsonValueKind.False && supplied?.Value.ValueKind == JsonValueKind.True)
                throw BridgeException.InvalidArgument("树脂耗尽模式不能同时开启补充须臾／脆弱树脂。");
            if (supplied is null)
                changes.Add(new(path, mode.Value.ValueKind == JsonValueKind.False
                    ? JsonSerializer.SerializeToElement(false) : JsonSerializer.SerializeToElement(entry.CurrentValue), entry.ValueVersion));
        }
        if (changes.Count > 20) throw BridgeException.InvalidArgument("包含宿主联动后超过 20 项，未修改。");
        // 父模式在前，联动值在后，提交与回退都覆盖同一组字段。
        return changes.OrderBy(change => change.Path.Equals("autoBossConfig.specifyRunCount", StringComparison.OrdinalIgnoreCase) ? 0 : 1).ToArray();
    }
}
