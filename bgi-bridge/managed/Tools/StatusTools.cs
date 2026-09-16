using System.Diagnostics;
using System.Windows.Input;
using BgiBridge.Bgi;
using BgiBridge.Catalog;
using BgiBridge.Protocol;

namespace BgiBridge.Tools;

/// <summary>状态与生命周期组。目前只有打通验证需要的几个，其余按计划补齐。</summary>
public static class StatusTools
{
    public const string Group = "lifecycle";

    /// <summary>宿主启动原神的入口。契约测试按 host-documentation.json 核对这两个名字。</summary>
    public const string StartViewModel = "BetterGenshinImpact.ViewModel.Pages.HomePageViewModel";
    public const string StartCommand = "StartTriggerCommand";

    /// <summary>
    /// 启动动作最多等这么久。原神从拉起窗口到能截图要几十秒，超过就先回执，
    /// 让调用方用 bgi.get_status 续等 —— 一次调用不该挂在那里等加载完。
    /// </summary>
    private static readonly TimeSpan LaunchWait = TimeSpan.FromSeconds(20);

    private static readonly object LaunchGate = new();

    /// <summary>
    /// 正在进行的启动。宿主命令发出后不可取消，重复调用只能等它，不能二次拉起原神。
    /// </summary>
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
            "诊断：报告桥在宿主里实际能拿到的类型与单例。用于确认反射链是否通、以及 BGI 版本是否匹配。",
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

        // 用户要求跑游戏任务而游戏或截图器没开时，这就是那一步。宿主自己会按
        // 「联动启动」的配置拉起原神并开始截图，不需要用户回到界面点启动。
        registry.Register(
            "bgi.start_game",
            Group,
            "启动原神并让 BetterGI 开始截图，然后返回最新状态。游戏或截图器没就绪时先调用它，等 ready=true 再跑任务；"
                + "启动后仍在加载是正常状态，用 bgi.get_status 继续查看，不要把它当成失败。",
            async (_, cancellation) =>
            {
                cancellation.ThrowIfCancellationRequested();
                // 截图器就绪才是「已经在跑」的判据：启动按钮的可执行性与它无关。
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
                        note = "截图器已经在运行，不需要重复启动。",
                        runtime = runningDetail,
                    };
                }

                var watch = Stopwatch.StartNew();
                Task launch;
                lock (LaunchGate)
                {
                    launch = Launching is { IsCompleted: false } running ? running : Launching = Launch();
                }
                // 取消只作用于本次等待：已经交给宿主的启动会继续跑完。
                var settled = await Task.WhenAny(launch, Task.Delay(LaunchWait, cancellation))
                    .ConfigureAwait(false);
                cancellation.ThrowIfCancellationRequested();
                if (settled == launch) await launch.ConfigureAwait(false);

                var (ready, detail) = BridgeState.Capture();
                if (!ready && settled == launch)
                    throw BridgeException.Failed(
                        "宿主的启动流程已经结束，但截图器仍未就绪：没有找到原神窗口，宿主也就没有开始截图。"
                            + "请确认原神能正常启动，或让用户在 BetterGI 的启动页手动点击启动。");

                return new
                {
                    started = true,
                    alreadyRunning = false,
                    ready,
                    // 没等到启动流程结束就说明还在加载：这是过程状态，不是失败。
                    stillLoading = settled != launch,
                    elapsedMs = watch.ElapsedMilliseconds,
                    note = ready ? "截图器已就绪，可以运行任务了。" : "原神仍在加载，用 bgi.get_status 继续查看。",
                    runtime = detail,
                };
            },
            readOnly: false);
    }

    /// <summary>
    /// 在宿主 UI 线程上执行启动命令。返回的任务代表宿主启动流程本身：命令一旦发出，
    /// 就与调用方的取消无关，宿主会把它跑完。
    /// </summary>
    private static Task Launch() => Ui.InvokeAsync(async () =>
    {
        var services = Host.Services()
            ?? throw BridgeException.Missing("拿不到宿主的服务容器。");
        var viewModel = Reflect.RequireType(StartViewModel);
        var home = services.GetService(viewModel)
            ?? throw BridgeException.Missing("启动页 ViewModel 未注册。");
        if (Reflect.Get(home, StartCommand) is not ICommand command)
            throw BridgeException.Missing($"{viewModel.Name}.{StartCommand} 不是可执行命令。");
        EnsureStartable(home);
        await Commands.RunAsync(command).ConfigureAwait(false);
    });

    /// <summary>
    /// 宿主在这些情况下只会弹一个对话框就返回。界面对话框在注入进程里没人点，
    /// 而且在动手前拦下来，才能把缺的那一项直接告诉调用方。
    /// </summary>
    private static void EnsureStartable(object home)
    {
        if (Reflect.Get(home, "Config") is not { } config)
            throw BridgeException.Missing("启动页拿不到配置对象。");
        if (Reflect.Get(config, "TriggerInterval") is int interval and <= 0)
            throw BridgeException.InvalidArgument(
                "BetterGI 的触发器触发频率不大于 0，宿主会拒绝启动截图器。请让用户先在 BetterGI 界面把它设为大于 0。");

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
