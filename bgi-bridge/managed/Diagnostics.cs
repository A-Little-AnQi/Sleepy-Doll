using System.Text;

namespace BgiBridge;

/// <summary>
/// 日志落文件。不能用 Console：宿主是 WPF 应用，没有属于我们的控制台。
/// 写在数据根（安装目录的 user\，见 InstallPaths）的 log 下，不碰宿主目录，
/// 也不让安装目录里堆运行期产物。
/// </summary>
public static class Diagnostics
{
    private static readonly object Gate = new();
    private static string? _logPath;

    public static void Attach(string bridgeDir)
    {
        try
        {
            _logPath = Path.Combine(InstallPaths.LogDirectory(bridgeDir), "bridge.log");
        }
        catch
        {
            _logPath = null;
        }
    }

    /// <summary>Attach 之前就出事时的兜底路径。</summary>
    private static string FallbackPath
    {
        get
        {
            try
            {
                return Path.Combine(Path.GetTempPath(), "bgi-bridge.log");
            }
            catch
            {
                return "bgi-bridge.log";
            }
        }
    }

    public static void Write(string message)
    {
        var line = $"[{DateTime.Now:HH:mm:ss.fff}] {message}{Environment.NewLine}";
        foreach (var path in new[] { _logPath, FallbackPath })
        {
            if (path is null) continue;
            try
            {
                lock (Gate)
                {
                    File.AppendAllText(path, line, Encoding.UTF8);
                }
                return;  // 写成功一处就够了，不要两份重复
            }
            catch
            {
                // 换下一个候选；全都写不出去也不能成为新的故障源。
            }
        }
    }

    public static void Write(string message, Exception error) =>
        Write($"{message}: {error.GetType().Name}: {error.Message}\n{error.StackTrace}");
}
