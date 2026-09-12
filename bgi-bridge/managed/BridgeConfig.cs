using System.Text.Json;
using System.Text.Json.Serialization;

namespace BgiBridge;

/// <summary>桥自己的配置。属于本工具，不写进宿主目录。</summary>
public sealed class BridgeConfig
{
    /// <summary>启动开关。false 时托管入口拒绝启动；运行中的开关由鉴权 control 端点管理。</summary>
    [JsonPropertyName("enabled")]
    public bool Enabled { get; set; } = true;

    /// <summary>监听地址，只允许回环。</summary>
    [JsonPropertyName("listen")]
    public string Listen { get; set; } = "127.0.0.1:3499";

    /// <summary>Bearer token，须与 Sleepy Doll 的 bridge.token 一致。</summary>
    [JsonPropertyName("token")]
    public string Token { get; set; } = "";

    /// <summary>按组开关。关闭后仍可检索说明，但 invoke 拒绝执行。</summary>
    [JsonPropertyName("groups")]
    public Dictionary<string, bool> Groups { get; set; } = new(StringComparer.OrdinalIgnoreCase);

    /// <summary>单方法黑名单，优先级高于组开关。</summary>
    [JsonPropertyName("disabledMethods")]
    public List<string> DisabledMethods { get; set; } = [];

    /// <summary>写类方法是否带 requiresConfirmation 元数据，交给 Sleepy Doll 审批。</summary>
    [JsonPropertyName("requireConfirmation")]
    public bool RequireConfirmation { get; set; } = true;

    /// <summary>未列出的组默认启用——升级时静默丢能力比多暴露更难查。</summary>
    public bool IsGroupEnabled(string group) =>
        !Groups.Any(item => item.Key.Equals(group, StringComparison.OrdinalIgnoreCase) && !item.Value)
        && !(group == "settings" && Groups.Any(item => item.Key.Equals("setting", StringComparison.OrdinalIgnoreCase) && !item.Value));

    /// <summary>方法是否暴露。总开关 → 黑名单 → 组开关。</summary>
    public bool IsMethodEnabled(string methodId, string group) =>
        Enabled
        && !DisabledMethods.Contains(methodId, StringComparer.OrdinalIgnoreCase)
        && IsGroupEnabled(group);

    public static BridgeConfig Parse(string json) =>
        JsonSerializer.Deserialize<BridgeConfig>(json, SerializerOptions)
        ?? throw new InvalidOperationException("bridge.config.json 解析结果为空。");

    public static readonly JsonSerializerOptions SerializerOptions = new()
    {
        PropertyNameCaseInsensitive = true,
        ReadCommentHandling = JsonCommentHandling.Skip,
        AllowTrailingCommas = true,
        WriteIndented = true,
    };

    /// <summary>解析 "127.0.0.1:3499"，拒绝非回环地址。</summary>
    public (string Host, int Port) ResolveEndpoint()
    {
        var text = Listen?.Trim() ?? "";
        var separator = text.LastIndexOf(':');
        if (separator <= 0 || !int.TryParse(text[(separator + 1)..], out var port) || port is < 1 or > 65535)
            throw new InvalidOperationException($"listen 必须形如 127.0.0.1:3499，实际是「{Listen}」。");

        var host = text[..separator].Trim().Trim('[', ']');
        if (host is not ("127.0.0.1" or "localhost" or "::1"))
            throw new InvalidOperationException($"只允许监听回环地址，实际是「{host}」。");

        return (host == "localhost" ? "127.0.0.1" : host, port);
    }
}
