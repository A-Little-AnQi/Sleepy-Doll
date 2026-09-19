using System.Text.Json;

namespace BgiBridge;

/// <summary>运行期数据的落点。日志、宿主配置回滚记录一律落在安装目录的 user\ 下：那里安装与卸载都不动。</summary>
public static class InstallPaths
{
    /// <summary>
    /// 数据根：bridge.config.json 的 userDirectory，缺省是桥目录下的 user\。
    /// 交付形态里桥组件在 &lt;安装目录&gt;\bridge 下，数据根是 &lt;安装目录&gt;\user。
    /// </summary>
    /// <param name="bridgeDir">桥组件所在目录，不是数据目录。</param>
    public static string UserRoot(string bridgeDir) =>
        ConfiguredRoot(bridgeDir) ?? Path.Combine(bridgeDir, "user");

    public static string LogDirectory(string bridgeDir) => Path.Combine(UserRoot(bridgeDir), "log");

    /// <summary>宿主配置改动记录，放在 user\ 下。</summary>
    public static string ChangeRecordDirectory(string bridgeDir) =>
        Path.Combine(UserRoot(bridgeDir), ".sleepy-doll", "config-changes");

    /// <summary>建好运行期目录。失败只吞掉：日志另有兜底路径，记录写不进去时调用方会自己报错。</summary>
    public static void Ensure(string bridgeDir)
    {
        if (string.IsNullOrWhiteSpace(bridgeDir)) return;
        try
        {
            Directory.CreateDirectory(LogDirectory(bridgeDir));
            Directory.CreateDirectory(ChangeRecordDirectory(bridgeDir));
        }
        catch
        {
        }
    }

    /// <summary>读 bridge.config.json 里的数据根。读不出来或字段不完整时返回 null，由调用方退回缺省。</summary>
    private static string? ConfiguredRoot(string bridgeDir)
    {
        if (string.IsNullOrWhiteSpace(bridgeDir)) return null;
        try
        {
            var path = Path.Combine(bridgeDir, "bridge.config.json");
            if (!File.Exists(path)) return null;
            using var document = JsonDocument.Parse(File.ReadAllText(path));
            var root = document.RootElement.TryGetProperty("userDirectory", out var value)
                ? value.GetString()
                : null;
            // 相对路径随宿主进程的当前目录变化。
            return !string.IsNullOrWhiteSpace(root) && Path.IsPathRooted(root) ? root : null;
        }
        catch
        {
            return null;
        }
    }
}
