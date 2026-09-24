using System.Diagnostics;
using System.Windows.Input;
using BgiBridge.Bgi;
using BgiBridge.Catalog;
using BgiBridge.Protocol;

namespace BgiBridge.Tools;

/// <summary>状态与生命周期组。</summary>
public static class StatusTools
{
    public const string Group = "lifecycle";

    /// <summary>宿主启动原神的入口，名称对应 host-documentation.json。</summary>
    public const string StartViewModel = "BetterGenshinImpact.ViewModel.Pages.HomePageViewModel";
    public const string StartCommand = "StartTriggerCommand";

    /// <summary>启动动作最多等待的时间。原神从拉起窗口到能截图要几十秒，超过就先返回状态。</summary>
    private static readonly TimeSpan LaunchWait = TimeSpan.FromSeconds(20);

    private static readonly object LaunchGate = new();

    /// <summary>正在进行的启动；宿主命令发出后不可取消，重复调用只能等待。</summary>
    private static Task? Launching;

    public static void Register(MethodRegistry registry)
    {
        registry.Register(
            "bgi.ping",
            Group,
            "检查桥服务是否可用。",
            static (_, _) => Task.FromResult<object?>(new
            {
                ok = true,
                at = DateTimeOffset.UtcNow,
                hostLoaded = Host.HostLoaded,
            }));

        registry.Register(
            "bgi.probe",
            Group,
            "诊断：报告桥在 BetterGI 里实际能拿到的类型与单例。用于确认反射链是否通、以及 BGI 版本是否匹配。",
            static (_, _) => Task.FromResult<object?>(Host.Probe()));

        registry.Register(
            "bgi.get_status",
            Group,
            "获取 BetterGI、截图器、游戏窗口和独立任务的当前状态。只做被动观测，不改变游戏界面。",
            static (_, _) =>
            {
                var (ready, detail) = BridgeState.Capture();
                return Task.FromResult<object?>(new
                {
                    ready,
                    observedAt = DateTimeOffset.UtcNow,
                    runtime = detail,
                });
            });

        // 宿主按「联动启动」的配置拉起原神并开始截图，不需要用户回到界面点启动。
        registry.Register(
            "bgi.start_game",
            Group,
            "启动原神并让 BetterGI 开始截图，然后返回最新状态。游戏或截图器没就绪时先调用它，等 ready=true 再执行任务；"
                + "启动后仍在加载是正常状态，用 bgi.get_status 继续查看，不要把它当成失败。",
            async (_, cancellation) =>
            {
                cancellation.ThrowIfCancellationRequested();
                // 截图器就绪才算已经在运行。
                if (Host.CaptureReady)
                {
                    var (runningReady, runningDetail) = BridgeState.Capture();
                    return new
                    {
                        started = false,
                        alreadyRunning = true,
                        ready = runningReady,
                        stillLoading = false,
                        elapsedMs = 0L,
                        note = Host.CaptureReady && !Host.InMainUi()
                            ? "截图器在运行，但游戏还没进入主界面；用 bgi.get_status 等到 ready=true 再运行任务。"
                            : Host.GameSixteenToNine()
                                ? "截图器已经在运行，不需要重复启动。"
                                : ResolutionWarning(),
                        runtime = runningDetail,
                    };
                }

                var watch = Stopwatch.StartNew();
                Task launch;
                lock (LaunchGate)
                {
                    launch = Launching is { IsCompleted: false } running ? running : Launching = Launch();
                }
                // 取消只作用于本次等待：已经交给宿主的启动会继续执行完。
                var settled = await Task.WhenAny(launch, Task.Delay(LaunchWait, cancellation))
                    .ConfigureAwait(false);
                cancellation.ThrowIfCancellationRequested();
                if (settled == launch) await launch.ConfigureAwait(false);

                var (ready, detail) = BridgeState.Capture();
                if (!Host.CaptureReady && settled == launch)
                    throw BridgeException.Failed(
                        "BetterGI 的启动流程已经结束，但截图器仍未就绪：没有找到原神窗口，BetterGI 也就没有开始截图。"
                            + "请确认原神能正常启动，或让用户在 BetterGI 的启动页手动点击启动。");

                return new
                {
                    started = true,
                    alreadyRunning = false,
                    ready,
                    // 未等到启动流程结束表示仍在加载；这是过程状态。
                    stillLoading = settled != launch,
                    elapsedMs = watch.ElapsedMilliseconds,
                    note = ResolutionNote(ready),
                    runtime = detail,
                };
            },
            readOnly: false);
    }

    /// <summary>就绪提示按阶段区分：未到主界面、非 16:9 都如实说，别让上层以为能直接跑任务。</summary>
    private static string ResolutionNote(bool ready)
    {
        if (!ready)
            return Host.CaptureReady
                ? "截图器已就绪，但游戏还没进入主界面；用 bgi.get_status 等到 ready=true（以主界面为准）再运行任务。"
                : "原神仍在加载，用 bgi.get_status 继续查看。";
        return Host.GameSixteenToNine()
            ? "已进入游戏主界面，可以运行任务了。"
            : ResolutionWarning();
    }

    private static string ResolutionWarning() =>
        Host.GameClientSize() is { } size
            ? $"截图器已就绪，但游戏窗口分辨率 {size.Width}x{size.Height} 不是 16:9，BetterGI 的截图识别与脚本无法运行。"
                + "启动参数 -screen-width/-screen-height 对已初始化过的原神不生效；请改游戏内显示设置或把远程桌面会话调到 16:9（如 1920x1080）后再试。"
            : "截图器已就绪，但读不到游戏窗口尺寸。";

    /// <summary>在宿主 UI 线程上执行启动命令。返回的任务代表宿主启动流程：命令发出后不受调用方取消影响。</summary>
    private static Task Launch() => Ui.InvokeAsync(async () =>
    {
        var services = Host.Services()
            ?? throw BridgeException.Missing("拿不到 BetterGI 的服务容器。");
        var viewModel = Reflect.RequireType(StartViewModel);
        var home = services.GetService(viewModel)
            ?? throw BridgeException.Missing("启动页 ViewModel 未注册。");
        if (Reflect.Get(home, StartCommand) is not ICommand command)
            throw BridgeException.Missing($"{viewModel.Name}.{StartCommand} 不是可执行命令。");
        EnsureStartable(home);
        await Commands.RunAsync(command).ConfigureAwait(false);
    });

    /// <summary>
    /// 宿主在这些情况下只弹一个对话框就返回；注入进程里没人能点它，
    /// 先在动手前拦下并把缺的那一项说明给调用方。
    /// </summary>
    private static void EnsureStartable(object home)
    {
        if (Reflect.Get(home, "Config") is not { } config)
            throw BridgeException.Missing("启动页拿不到配置对象。");
        if (Reflect.Get(config, "TriggerInterval") is int interval and <= 0)
            throw BridgeException.InvalidArgument(
                "BetterGI 的触发器触发频率不大于 0，BetterGI 会拒绝启动截图器。请让用户先在 BetterGI 界面把它设为大于 0。");

        var systemControl = Reflect.RequireType("BetterGenshinImpact.GameTask.SystemControl");
        if (Reflect.CallStatic(systemControl, "FindGenshinImpactHandle") is IntPtr { } window
            && window != IntPtr.Zero)
        {
            return;  // 游戏已经在运行，宿主会直接开始截图
        }

        if (Reflect.Get(config, "GenshinStartConfig") is not { } start)
            throw BridgeException.Missing("拿不到原神启动配置。");
        if (Reflect.Get(start, "LinkedStartEnabled") is not true)
            throw BridgeException.InvalidArgument(
                "没有找到原神窗口，BetterGI 的「同时启动原神」也没有开启。请让用户先启动原神，"
                    + "或在 BetterGI 的启动页开启「同时启动原神」并配置原神安装路径。");
        if (Reflect.Get(start, "InstallPath") as string is not { Length: > 0 } path)
            throw BridgeException.InvalidArgument(
                "BetterGI 开启了「同时启动原神」，但没有配置原神安装路径。请让用户在启动页选择原神程序。");
        if (!File.Exists(path))
            throw BridgeException.InvalidArgument(
                "BetterGI 配置的原神安装路径在本机不存在。请让用户重新选择原神安装路径。");
    }
}
