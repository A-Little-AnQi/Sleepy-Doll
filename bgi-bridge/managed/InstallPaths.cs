using System.Text.Json;

namespace BgiBridge;

/// <summary>
/// 运行期数据的落点。安装目录里放的是产品文件，重装与分发包都会整包替换，
/// 所以日志、宿主配置回滚记录一律落在 user\ 下 —— 那是用户的目录，
/// 安装程序不覆盖，卸载也不删除。
/// </summary>
public static class InstallPaths
{
    /// <summary>
    /// 数据根。桥组件装在 &lt;安装目录&gt;\bridge 下，数据根仍是
    /// &lt;安装目录&gt;\user，由 bridge.config.json 的 userDirectory 指出。
    /// 没有这个字段就退回桥目录下的 user\ —— 开发构建与旧版平铺包都是这样，
    /// 那时桥目录就是安装目录。
    /// </summary>
    /// <param name="bridgeDir">桥组件所在目录，不是数据目录。</param>
    public static string UserRoot(string bridgeDir) =>
        ConfiguredRoot(bridgeDir) ?? Path.Combine(bridgeDir, "user");

    public static string LogDirectory(string bridgeDir) => Path.Combine(UserRoot(bridgeDir), "log");

    /// <summary>宿主配置改动记录。放在 user\ 下，重装之后仍然可以回滚。</summary>
    public static string ChangeRecordDirectory(string bridgeDir) =>
        Path.Combine(UserRoot(bridgeDir), ".sleepy-doll", "config-changes");

    /// <summary>
    /// 建好运行期目录。装到只读位置时它不能变成新的故障源，所以失败只吞掉：
    /// 日志另有兜底路径，记录写不进去时调用方会自己报错。
    /// </summary>
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

    /// <summary>
    /// 读 bridge.config.json 里的数据根。配置读不出来或字段不完整时返回 null，
    /// 由调用方退回缺省 —— 数据落点不能成为启动失败的原因。
    /// </summary>
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
            // 相对路径会跟着宿主进程的工作目录跑，那不可预期。
            return !string.IsNullOrWhiteSpace(root) && Path.IsPathRooted(root) ? root : null;
        }
        catch
        {
            return null;
        }
    }
}
