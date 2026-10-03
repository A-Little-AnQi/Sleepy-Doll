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
                    note = ResolutionNote(ready),
                    runtime = detail,
                });
            });

        registry.Register(
            "bgi.wait_ready",
            Group,
            "默认阻塞等待 120 秒（可指定 1 到 180 秒），就绪、分辨率错误或游戏未运行时立即返回；不由模型轮询。",
            async (arguments, cancellation) =>
            {
                var seconds = arguments.TryGetProperty("timeoutSeconds", out var timeout) ? timeout.GetInt32() : 120;
                var watch = Stopwatch.StartNew();
                while (true)
                {
                    cancellation.ThrowIfCancellationRequested();
                    var (ready, detail) = BridgeState.Capture();
                    var outcome = ready ? "ready"
                        : Host.CaptureReady && Host.GameClientSize() is not null && !Host.GameSixteenToNine()
                            ? "resolution"
                        : watch.Elapsed >= TimeSpan.FromSeconds(5) && !Host.GameProcessRunning() && !Host.CaptureReady
                            ? "notRunning"
                        : watch.Elapsed >= TimeSpan.FromSeconds(seconds) ? "timeout" : null;
                    if (outcome is not null)
                        return new
                        {
                            ready,
                            outcome,
                            elapsedMs = watch.ElapsedMilliseconds,
                            observedAt = DateTimeOffset.UtcNow,
                            note = ResolutionNote(ready),
                            runtime = detail,
                        };
                    await Task.Delay(TimeSpan.FromSeconds(2), cancellation).ConfigureAwait(false);
                }
            },
            inputSchema: AgentSchemas.Object(("timeoutSeconds",ArgumentSchema.Parse("""{"type":"integer","minimum":1,"maximum":180,"default":120,"description":"等待主界面的最长秒数；默认 120，整个等待在工具内阻塞，不逐次调用模型。"}"""),false)),
            guide: new AgentGuide(
                "等待游戏就绪",
                "在游戏启动后默认阻塞等待最多 120 秒，返回最新状态；发现非 16:9 时不继续等主界面。",
                ["bgi.start_game 返回仍在加载，需要等待游戏进入主界面时。"],
                ["BetterGI 桥已连接。"],
                ["只观测状态，不操作游戏。"],
                "outcome=ready 才能继续游戏任务；resolution 应先修正分辨率；notRunning 表示游戏进程未运行；timeout 只表示本次限时内尚未就绪。",
                "以 ready 与 runtime.gameResolution 核对；timeout 后向用户报告仍未就绪，不继续模型轮询或 PowerShell 睡眠。",
                "只读。",
                [System.Text.Json.JsonSerializer.SerializeToElement(new { })]));

        // 统一的原生就绪准备入口：RootBridge 在已授权的写动作提交前按契约
        // preparation=ensureGameReady 通用调用；也可以被显式调用。只读与配置写
        // 不会经由它启动游戏。
        registry.Register(
            "bgi.ensure_game_ready",
            Group,
            "按真实状态做一次就绪准备并阻塞等待到主界面：需要时用宿主命令启动游戏或挂上截图器，最多等 120 秒。",
            async (_, cancellation) =>
            {
                await EnsureGameReady(cancellation).ConfigureAwait(false);
                var (ready, detail) = BridgeState.Capture();
                return ReadinessResult(ready, detail, ResolutionNote(ready));
            },
            readOnly: false,
            destructive: true,
            guide: new AgentGuide(
                "统一就绪准备",
                "按真实状态分类处理：无进程→宿主命令启动；有进程无截图→挂截图器；加载→阻塞等主界面；非 16:9/超时如实失败。",
                ["运行类接口在提交前需要就绪准备时（通常由运行时按契约自动执行）；排障时显式调用一次。"],
                ["BetterGI 桥已连接。"],
                ["可能启动原神进程或挂上截图器；不发送游戏输入，不改配置。"],
                "prepared=true 且 ready=true 表示就绪完成；失败时按返回的具体阻碍处理。",
                "以 ready 与 runtime 状态为准；超时/非 16:9 是失败不是就绪。",
                "就绪状态是观测事实，无需回滚；游戏可按需关闭。",
                [System.Text.Json.JsonSerializer.SerializeToElement(new { })],
                "bridge-stable-operation"));

        // 宿主按「联动启动」的配置拉起原神并开始截图，不需要用户回到界面点启动。
        registry.Register(
            "bgi.start_game",
            Group,
            "启动原神并让 BetterGI 开始截图，然后返回最新状态。游戏或截图器没就绪时先调用它，等 ready=true 再执行任务；"
                + "启动后仍在加载是正常状态，用 bgi.wait_ready 一次阻塞等待，不要把它当成失败。",
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
                        note = ResolutionNote(runningReady),
                        runtime = runningDetail,
                        verified = true,
                        verificationScope = "launch",
                        verificationReason = "已观察到运行中的游戏与截图器；未核验登录或脚本业务结果。",
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
                    verified = Host.CaptureReady || Host.GameProcessRunning(),
                    verificationScope = "launch",
                    verificationReason = "只核验已观察到游戏进程或截图器，未核验登录或脚本业务结果。",
                };
            },
            readOnly: false);
    }

    /// <summary>就绪准备状态机的迁移判定。纯函数，供静态验收覆盖全部状态。</summary>
    internal enum ReadinessStep
    {
        Ready,
        /// <summary>截图器未挂上；即使游戏进程已在运行，也要通过宿主启动命令挂截图器。</summary>
        AttachCapturer,
        WaitForMainUi,
        FailResolution,
        FailNotRunning,
        FailTimeout,
    }

    /// <summary>
    /// 按真实观测状态分类下一步：无进程、有进程无截图、有截图在加载、就绪、非 16:9、
    /// 失败超时。不依赖调用方提示词，工具执行层自己消化就绪准备。
    /// </summary>
    internal static ReadinessStep ClassifyReadiness(
        bool captureReady,
        bool gameProcessRunning,
        bool inMainUi,
        bool windowSizeKnown,
        bool sixteenToNine,
        TimeSpan waited,
        TimeSpan limit)
    {
        if (captureReady && windowSizeKnown && !sixteenToNine) return ReadinessStep.FailResolution;
        if (captureReady && inMainUi) return ReadinessStep.Ready;
        if (waited >= limit) return ReadinessStep.FailTimeout;
        if (!captureReady && !gameProcessRunning && waited >= TimeSpan.FromSeconds(5))
            return ReadinessStep.FailNotRunning;
        if (!captureReady) return ReadinessStep.AttachCapturer;
        return ReadinessStep.WaitForMainUi;
    }

    /// <summary>
    /// 运行前置的确定性就绪准备：先按状态机分类，需要时用与 bgi.start_game 相同的
    /// 宿主命令把截图器挂上（游戏已开时不会重复拉起进程），再按 bgi.wait_ready 的
    /// 语义一次阻塞等待到主界面、分辨率错误或限时。不新增守护线程；取消只作用于
    /// 等待，已交给宿主的启动命令由宿主执行完。
    /// </summary>
    internal static async Task EnsureGameReady(CancellationToken cancellation)
    {
        var watch = Stopwatch.StartNew();
        var launched = false;
        while (true)
        {
            cancellation.ThrowIfCancellationRequested();
            var size = Host.GameClientSize();
            switch (ClassifyReadiness(
                Host.CaptureReady,
                Host.GameProcessRunning(),
                Host.InMainUi(),
                size is not null,
                Host.GameSixteenToNine(),
                watch.Elapsed,
                TimeSpan.FromSeconds(120)))
            {
                case ReadinessStep.Ready:
                    return;
                case ReadinessStep.FailResolution:
                    throw BridgeException.GameNotReady(ResolutionWarning());
                case ReadinessStep.FailNotRunning:
                    throw BridgeException.GameNotReady(
                        "游戏进程未运行，就绪准备未完成。先用 bgi.start_game 启动，并按它返回的缺项处理。");
                case ReadinessStep.FailTimeout:
                    throw BridgeException.GameNotReady(
                        "等待 120 秒后游戏仍未进入主界面；如实向用户报告当前状态，不继续空等。");
                case ReadinessStep.AttachCapturer when !launched:
                    launched = true;
                    Task launch;
                    lock (LaunchGate)
                    {
                        launch = Launching is { IsCompleted: false } running ? running : Launching = Launch();
                    }
                    // 取消只作用于本次等待；宿主命令发出后继续执行完。
                    var settled = await Task.WhenAny(launch, Task.Delay(LaunchWait, cancellation))
                        .ConfigureAwait(false);
                    cancellation.ThrowIfCancellationRequested();
                    if (settled == launch)
                    {
                        await launch.ConfigureAwait(false);
                        if (!Host.CaptureReady)
                            throw BridgeException.Failed(
                                "BetterGI 的启动流程已经结束，但截图器仍未就绪：没有找到原神窗口，BetterGI 也就没有开始截图。"
                                    + "请确认原神能正常启动，或让用户在 BetterGI 的启动页手动点击启动。");
                    }
                    continue;
                default:
                    await Task.Delay(TimeSpan.FromSeconds(2), cancellation).ConfigureAwait(false);
                    break;
            }
        }
    }

    /// <summary>bgi.ensure_game_ready 的返回体。verified 与 ready 同值：只有截图器
    /// 与主界面实测就绪才为 true，宿主 Job 完成语义（result.verified==true 才记
    /// 核验成功）据此判定；未就绪的路径在 EnsureGameReady 内抛错，不会返回
    /// verified=false 的完成结果。</summary>
    internal static object ReadinessResult(bool ready, object runtime, string note) => new
    {
        prepared = true,
        ready,
        verified = ready,
        observedAt = DateTimeOffset.UtcNow,
        note,
        runtime,
        verificationScope = "readiness",
        verificationReason = "只核验截图器与主界面就绪；未核验登录账号或任何业务结果。",
    };

    /// <summary>就绪提示按阶段区分：未到主界面、非 16:9 都如实说，别让上层以为能直接跑任务。</summary>
    private static string ResolutionNote(bool ready)
    {
        if (Host.CaptureReady && Host.GameClientSize() is not null && !Host.GameSixteenToNine())
            return ResolutionWarning();
        if (!ready)
            return Host.CaptureReady
                ? "截图器已就绪，但游戏还没进入主界面；用 bgi.wait_ready 阻塞等到 ready=true（以主界面为准）再运行任务。"
                : Host.GameProcessRunning()
                    ? "原神进程已启动，但截图器尚未就绪；先调用 bgi.start_game（游戏已开时它会挂上截图器，不会重复拉起进程），再用 bgi.wait_ready 一次阻塞等待。"
                    : Host.DisplaySize() is { } display
                        ? $"原神进程未运行；当前桌面会话为 {display.Width}x{display.Height}。先确认它能容纳目标游戏分辨率，再启动。"
                        : "原神进程未运行；先确认所需分辨率，再用 bgi.start_game 启动。";
        return Host.GameSixteenToNine()
            ? "已进入游戏主界面，可以运行任务了。"
            : ResolutionWarning();
    }

    private static string ResolutionWarning() =>
        Host.GameClientSize() is { } size
            ? $"截图器已就绪，但游戏窗口分辨率 {size.Width}x{size.Height} 不是 16:9，BetterGI 的截图识别与脚本无法运行。"
                + (Host.DisplaySize() is { } display
                    ? $"当前桌面会话为 {display.Width}x{display.Height}；若目标分辨率超过它，先调整远程桌面会话，不能靠重写游戏配置解决。"
                    : "先核对当前桌面会话尺寸。")
                + "尺寸足够时再用 bgi.exit_game 关闭游戏、bgi.set_game_resolution 写入目标分辨率，然后 bgi.start_game 核对实际窗口。"
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
