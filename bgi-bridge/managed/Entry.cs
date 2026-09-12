using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using System.Text;
using BgiBridge.Bgi;
using BgiBridge.Catalog;
using BgiBridge.Hosting;
using BgiBridge.Jobs;
using BgiBridge.Tools;

namespace BgiBridge;

/// <summary>
/// 原生引导的托管入口。由 hostfxr 以 UNMANAGEDCALLERSONLY_METHOD 解析，
/// 所以签名必须 blittable。
/// </summary>
public static class Entry
{
    public const string Version = "0.1.0";

    private static BridgeHost? _host;

    /// <summary>
    /// 参数是桥目录的 UTF-16 路径。0 表示成功，其余是错误码。
    /// **不能抛异常**：从原生代码调进来的，逃逸出去会终止宿主进程。
    /// </summary>
    [UnmanagedCallersOnly]
    public static int Start(IntPtr parameters)
    {
        try
        {
            // 原生侧传进来的是桥目录的纯路径。先接日志，后面每一步失败都看得见。
            var bridgeDir = Marshal.PtrToStringUni(parameters) ?? "";
            Diagnostics.Attach(bridgeDir);
            Diagnostics.Write($"Entry.Start 被调用。bridgeDir={bridgeDir}");

            if (string.IsNullOrWhiteSpace(bridgeDir))
            {
                Diagnostics.Write("桥目录为空，返回 2。");
                return 42;
            }

            var configPath = Path.Combine(bridgeDir, "bridge.config.json");
            if (!File.Exists(configPath))
            {
                Diagnostics.Write($"配置文件不存在：{configPath}，返回 2。");
                return 43;
            }

            var config = BridgeConfig.Parse(File.ReadAllText(configPath));

            // 总开关。关掉时明确拒绝启动，而不是起来之后每个请求都报错。
            if (!config.Enabled)
            {
                Diagnostics.Write("bridge.config.json 里 enabled=false，不启动。");
                return 3;
            }

            if (string.IsNullOrEmpty(config.Token))
            {
                Diagnostics.Write("bridge.config.json 里没有 token；本地控制面必须带鉴权，拒绝启动。");
                return 4;
            }

            // 后台线程上的未观察异常会让进程崩，必须兜住。
            TaskScheduler.UnobservedTaskException += (_, e) => e.SetObserved();
            AppDomain.CurrentDomain.UnhandledException += (_, e) =>
                Diagnostics.Write($"未处理异常（不会终止宿主，仅记录）：{e.ExceptionObject}");

            var registry = new MethodRegistry();
            SettingsTransactions.Configure(bridgeDir);
            StatusTools.Register(registry);
            CatalogTools.Register(registry);
            // 两组自动发现：命令与设置项数量随宿主版本变化，不写死。
            Ui.InvokeAsync(() => CatalogTools.RegisterDiscovered(registry, config)).GetAwaiter().GetResult();
            Diagnostics.Write($"方法注册完成，共 {registry.Count} 个。");

            _host = new BridgeHost(config, registry, new JobStore(), Version);
            _host.Start();

            Diagnostics.Write(
                $"启动完成。宿主程序集已加载={Host.HostLoaded}，UI 线程可用={Ui.Available}");
            return 0;
        }
        catch (Exception ex)
        {
            Diagnostics.Write("启动失败", ex);
            return 41;
        }
    }

    /// <summary>停止监听并释放端口。DLL 本身要宿主退出才会卸载。</summary>
    [UnmanagedCallersOnly]
    public static int Stop()
    {
        try
        {
            _host?.Stop();
            _host = null;
            return 0;
        }
        catch
        {
            return 1;
        }
    }

}
