using System.Diagnostics;
using System.Text.Json;
using BgiBridge.Bgi;
using BgiBridge.Catalog;
using BgiBridge.Protocol;

namespace BgiBridge.Tools;

/// <summary>多配置组统一内存计划：一次交给宿主按顺序执行，不拆散各组配置，可选完成后关闭游戏。</summary>
public static class ScriptGroupPlanTools
{
    private const string PreferredViewModelType =
        "BetterGenshinImpact.ViewModel.Pages.ScriptControlViewModel";
    private const string StartGroupsMethod = "StartGroups";
    private const string TaskProgressType =
        "BetterGenshinImpact.GameTask.TaskProgress.TaskProgress";
    private const string SystemControlType =
        "BetterGenshinImpact.GameTask.SystemControl";
    private const string CancellationContextType =
        "BetterGenshinImpact.Core.Script.CancellationContext";
    private const string RunnerContextType =
        "BetterGenshinImpact.GameTask.RunnerContext";
    private const string TaskContextType =
        "BetterGenshinImpact.GameTask.TaskContext";
    private const string ScriptGroupTypeFullName =
        "BetterGenshinImpact.Core.Script.Group.ScriptGroup";

    public static void Register(MethodRegistry registry)
    {
        const string id = "bgi.run_script_groups";
        var guide = new AgentGuide(
            "按顺序运行多个配置组",
            "把多个已存在的配置组按请求顺序合成一份宿主统一进度计划一次执行，各组配置保持原样不拆散；可选在调度进度全部成功核验后关闭游戏。",
            ["用户要求一次按顺序运行多个配置组时。"],
            ["战斗/特殊/未知前提路线（partyConfirmationRequired=true）：先完成缺配队 user.ask、inspect_group_effective 读实际生效配置与策略并按角色与策略要求适配回读，再运行。", "游戏未就绪时接口会按真实状态自动准备：启动游戏或挂上截图器并阻塞等待至主界面（至多 120 秒）；非 16:9 或超时如实失败。", "窗口分辨率为 16:9。", "没有其他独立任务持锁。", "groupNames 均来自 User/ScriptGroup 的真实 name。"],
            ["按顺序启动所有组内的游戏自动化、脚本、路线或 Shell 任务；closeGameAfter=true 且收尾证据齐全时会在结束后关闭游戏。"],
            "默认 waitForCompletion=false：accepted=true 表示宿主已接受整份计划并交接，立即返回，不守护执行；Sleepy-Doll 退出后宿主计划继续。只有用户明确要求等结果时才设 true。",
            "普通多组运行请求用返回证据总结本次已交接，并结束本轮。不轮询进度、不查任务状态或日志。",
            "已发送的游戏输入和脚本副作用不能自动撤销；closeGameAfter 只在整个 EndTime 存在、全部历史成功且未手动取消时才会关闭游戏，缺证一律不关。",
            [JsonSerializer.SerializeToElement(new { groupNames = new[] { "用户目录中读取到的精确配置组名称" } })],
            "bridge-stable-operation");
        registry.Register(
            id,
            "scheduler",
            guide.Purpose,
            Invoke,
            readOnly: false,
            destructive: true,
            inputSchema: AgentSchemas.Input(id),
            guide: guide,
            requiresGameReady: true,
            preparation: "ensureGameReady");
    }

    private static async Task<object?> Invoke(
        JsonElement arguments,
        CancellationToken cancellation)
    {
        var names = ParseNames(arguments);
        var closeGameAfter = arguments.TryGetProperty("closeGameAfter", out var close) && close.GetBoolean();
        var waitForCompletion = arguments.TryGetProperty("waitForCompletion", out var wait) && wait.GetBoolean();
        // 就绪准备由工具执行层按真实状态处理：无进程→宿主命令启动；有进程无截图→
        // 挂上截图器；有截图在加载→阻塞等主界面；非 16:9/超时→如实失败。
        await StatusTools.EnsureGameReady(cancellation).ConfigureAwait(false);
        // 脚本依赖 16:9 截图；非 16:9 时整组会逐条异常退出，宁可在这里拦下。
        if (!Host.GameSixteenToNine())
            throw BridgeException.GameNotReady(
                Host.GameClientSize() is { } size
                    ? $"游戏窗口分辨率 {size.Width}x{size.Height} 不是 16:9，配置组无法运行。请先把游戏或远程桌面会话调到 16:9（如 1920x1080）。"
                    : "游戏窗口尺寸未知，无法确认 16:9，配置组不执行。");
        if (Host.TaskSemaphoreCount() is not > 0)
            throw BridgeException.Busy("已有独立任务运行或任务锁状态未知。");
        RequireContinuousRunIdle();
        var gameHandle = Host.GameHandle;
        if (gameHandle == 0)
            throw BridgeException.GameNotReady("拿不到当前游戏窗口句柄，无法在收尾时防止误关新游戏，计划不启动。");

        return await Ui.InvokeAsync(async () =>
        {
            cancellation.ThrowIfCancellationRequested();
            var services = Host.Services()
                ?? throw BridgeException.Missing("拿不到 BetterGI 服务容器。");
            var progressType = Reflect.FindType(TaskProgressType)
                ?? throw BridgeException.Missing("宿主 TaskProgress 类型不可用，无法建立统一进度计划。");
            var viewModelType = Reflect.FindType(PreferredViewModelType)
                ?? throw BridgeException.Missing("当前 BetterGI 没有提供 StartGroups(List<ScriptGroup>, TaskProgress?, bool) 的调度服务类型。");
            if (!HasStartGroups(viewModelType))
                throw BridgeException.Missing("宿主调度服务没有公开 StartGroups(List<ScriptGroup>, TaskProgress?, bool) 三参签名。");
            if (closeGameAfter)
            {
                var systemControl = Reflect.FindType(SystemControlType);
                if (systemControl?.GetMethod("CloseGame") is null || systemControl.GetMethod("GetProcessByHandle") is null)
                    throw BridgeException.Missing("宿主缺少 SystemControl.CloseGame 或 GetProcessByHandle，无法锁定并安全关闭目标进程，计划不启动。");
            }
            var viewModel = services.GetService(viewModelType)
                ?? throw BridgeException.Missing($"包含 {StartGroupsMethod} 的 BetterGI 服务未注册。");

            var groups = BindGroupsInOrder(names);
            // 用本次计划自己的进度对象承载保序组名；显式落 Loop=false，不依赖宿主默认值。
            var progress = Activator.CreateInstance(progressType)
                ?? throw BridgeException.Missing("TaskProgress 构造失败。");
            Reflect.Set(progress, "ScriptGroupNames", new List<string>(names));
            Reflect.Set(progress, "Loop", false);
            cancellation.ThrowIfCancellationRequested();
            // 真正启动前在 UI 线程最后复核互斥与游戏实例；组间空档任务锁可能已释放但连续运行仍在。
            if (Host.TaskSemaphoreCount() is not > 0)
                throw BridgeException.Busy("已有独立任务运行或任务锁状态未知。");
            RequireContinuousRunIdle();
            if (Host.GameHandle != gameHandle)
                throw BridgeException.GameNotReady("启动前游戏窗口已切换，计划不执行。");
            // 需要关游戏时先锁定目标进程身份，收尾只认这份 PID+启动时间证据。
            var identity = closeGameAfter ? CaptureGameIdentity(gameHandle) : null;
            // 直接执行已绑定对象，一次传整份有序列表；不刷新会重写其他配置组的界面集合。
            // 登记 bridge 持有的当前计划（progress+组身份+取消标记）；计划结束注销。
            var plan = RegisterPlan(progress, groups, names.ToList(), gameHandle);
            var execution = Reflect.Await(Reflect.Call(viewModel, StartGroupsMethod, groups, progress, false));
            _ = execution.ContinueWith(_ => RemovePlan(plan.Id), TaskScheduler.Default);

            if (!waitForCompletion)
                // 启动交接：有限续接只观察异常并按需收尾关游戏，不绑定工具取消令牌。
                return await Handoff(execution,
                    task => _ = FollowUpAsync(task, progress, closeGameAfter, gameHandle, ProductionClose(identity), plan),
                    names, closeGameAfter).ConfigureAwait(false);

            return await WaitFinish(execution, progress, names, closeGameAfter, gameHandle, identity, cancellation, plan).ConfigureAwait(false);
        }).ConfigureAwait(false);
    }

    /// <summary>桥启动的宿主统一进度计划登记：progress 对象 + 组名身份 + 独立取消标记。
    /// 停止当前任务时按计划身份撤未执行队列，而不是反复打全局 ManualCancel。</summary>
    public sealed class PlanRegistration
    {
        public required string Id;
        public required object Progress;
        /// <summary>实际传给 StartGroups 的执行列表引用（GetNextScriptGroups 只检查
        /// NextFlag 后原样返回它；从头运行时 NextFlag=false，同一引用就是执行队列）。
        /// TaskProgress.ScriptGroupNames 只是显示名，不是队列。</summary>
        public required System.Collections.IList ExecutionQueue;
        public List<string> DisplayNames { get; init; } = [];
        public long GameHandle;
        public volatile bool Cancelled;
    }

    private static readonly System.Collections.Concurrent.ConcurrentDictionary<string, PlanRegistration> ActivePlans = new();

    /// <summary>计划启动时登记；计划结束的续接里注销。</summary>
    internal static PlanRegistration RegisterPlan(
        object progress, System.Collections.IList executionQueue, List<string> displayNames, long gameHandle)
    {
        var plan = new PlanRegistration
        {
            Id = Guid.NewGuid().ToString("N"),
            Progress = progress,
            ExecutionQueue = executionQueue,
            DisplayNames = displayNames,
            GameHandle = gameHandle,
        };
        ActivePlans[plan.Id] = plan;
        return plan;
    }

    internal static void RemovePlan(string planId) => ActivePlans.TryRemove(planId, out _);

    /// <summary>当前唯一活动计划；没有则 null（单组运行不在登记内）。</summary>
    internal static PlanRegistration? SingleActivePlan() =>
        ActivePlans.Count == 1 ? ActivePlans.Values.First() : null;

    /// <summary>计划级取消核心（UI 线程调用）：先核对本计划身份（当前游戏句柄
    /// 仍是启动时那个），然后清空实际传给 StartGroups 的执行列表——原生
    /// foreach(List) 因版本变化在当前组结束/下一次 MoveNext 退出，后续组不再
    /// 启动；StartGroups 的 catch/finally Reset 走完后计划结束。清显示名不是
    /// 证明，真正生效的是执行列表清空。</summary>
    internal static bool CancelPlanQueue(PlanRegistration plan, long currentGameHandle) =>
        CancelPlanQueue(plan, currentGameHandle, CurrentRunner());

    private static object? CurrentRunner()
    {
        var runnerType = Reflect.FindType(RunnerContextType);
        return runnerType is null ? null : Reflect.Singleton(runnerType);
    }

    /// <summary>核心（runner 可注入供同形态替身测试）：句柄 0 一律拒绝（无法确认
    /// 身份不得 fail-open）；游戏句柄不匹配拒绝；RunnerContext.taskProgress 与登记
    /// Progress 引用不一致拒绝——防过期登记清旧队列却把 ManualCancel 打到新任务。</summary>
    internal static bool CancelPlanQueue(PlanRegistration plan, long currentGameHandle, object? runner)
    {
        if (currentGameHandle == 0 || plan.GameHandle == 0) return false;
        if (plan.GameHandle != currentGameHandle) return false;
        var current = runner?.GetType().GetProperty("taskProgress")?.GetValue(runner);
        if (!ReferenceEquals(current, plan.Progress)) return false;
        plan.Cancelled = true;
        plan.ExecutionQueue.Clear();
        return plan.ExecutionQueue.Count == 0;
    }

    /// <summary>closeGameAfter 判定用登记的取消标记，不依赖会被 RunMulti 每组
    /// 重置的宿主 ManualStop。</summary>
    internal static bool PlanCancelled(PlanRegistration? plan) => plan?.Cancelled == true;

    /// <summary>组间空档重入检查：宿主 StartGroups 组间 Task.Delay 时任务锁已释放，但 RunnerContext.IsContinuousRunGroup 仍为 true；
    /// TaskRunner.Clear 不复位它，只有 StartGroups.finally.Reset 才置 false。缺证按占用处理，不假定空闲。</summary>
    public static void RequireContinuousRunIdle()
    {
        if (ReadContinuousRun() is not false)
            throw BridgeException.Busy(ReadContinuousRunState()
                ? "上一份配置组计划仍在组间过渡，未完全退出，请稍后再试。"
                : "无法读取宿主连续运行状态（RunnerContext.IsContinuousRunGroup），缺证不启动。");
    }

    /// <summary>整份计划是否真正退出：任务锁空闲不足以判定——组间间隙锁已释放
    /// 但 IsContinuousRunGroup 仍为 true，后续组还会继续。供停止核验使用。</summary>
    internal static bool ContinuousRunIdle() => ReadContinuousRun() == false;

    private static bool? ReadContinuousRun()
    {
        var runner = Reflect.Singleton(RunnerContextType);
        return runner is null ? null : AsBool(Reflect.Get(runner, "IsContinuousRunGroup"));
    }

    private static bool ReadContinuousRunState() => ReadContinuousRun() == true;

    /// <summary>启动交接：挂上有限续接后立即返回证据；若原生 Task 已完成则暴露即时失败。</summary>
    public static async Task<object> Handoff(Task execution, Action<Task> attachFollowUp, List<string> names, bool closeGameAfter)
    {
        attachFollowUp(execution);
        if (execution.IsCompleted)
            await execution.ConfigureAwait(false); // Surface immediate dispatch failures.
        return new
        {
            groupNames = names.ToArray(),
            closeGameAfter,
            resolved = true,
            accepted = true,
            executed = execution.IsCompletedSuccessfully,
            executionMode = "launch",
            verified = true,
            verificationScope = "launch",
            verificationReason = "配置组均已唯一定位，宿主已接受整份有序计划并交接；launch 范围只核验交接，不表示业务完成。",
            verification = new
            {
                status = "succeeded",
                reason = "启动交接已核验：全部配置组定位成功并交给宿主按顺序执行。",
            },
        };
    }

    /// <summary>显式等待：等同一宿主 Task 返回后核对调度进度；判定与收尾统一走 SettleAsync，收尾未核验时整体降级为 unknown。</summary>
    private static async Task<object> WaitFinish(
        Task execution,
        object progress,
        List<string> names,
        bool closeGameAfter,
        long gameHandle,
        GameIdentity? identity,
        CancellationToken cancellation,
        PlanRegistration? plan = null)
    {
        await execution.WaitAsync(cancellation).ConfigureAwait(false);
        var snapshot = SnapshotFromProgress(progress, ReadManualStop() == true || PlanCancelled(plan));
        var settle = await SettleAsync(execution, () => EvaluatePlan(snapshot), closeGameAfter, gameHandle,
            ProductionClose(identity), cancellation).ConfigureAwait(false);
        var verdict = settle.Verdict;
        return new
        {
            groupNames = names.ToArray(),
            closeGameAfter,
            resolved = true,
            accepted = true,
            executed = true,
            executionMode = "completion",
            verified = verdict.Succeeded,
            verificationScope = "schedulerProgress",
            verificationReason = verdict.Reason,
            verification = new { status = verdict.Status, reason = verdict.Reason },
            manualStopKnown = snapshot.ManualStopKnown,
            manualStop = snapshot.ManualStop,
            overallEndTimePresent = snapshot.OverallEndTimePresent,
            historyCount = snapshot.History.Count,
            closeGame = settle.Close?.Outcome,
        };
    }

    /// <summary>共享收尾：等原生 Task、判定进度；要求关闭时成功才调回调，关闭未核验则整体降级为 unknown，失败/缺证不调用回调。</summary>
    public static async Task<PlanSettle> SettleAsync(
        Task execution,
        Func<PlanVerdict> judge,
        bool closeGameAfter,
        long gameHandle,
        Func<long, Task<CloseResult>> tryClose,
        CancellationToken cancellation = default)
    {
        await execution.WaitAsync(cancellation).ConfigureAwait(false);
        var verdict = judge();
        if (!closeGameAfter)
            return new PlanSettle(verdict, null);
        if (!verdict.Succeeded)
            return new PlanSettle(verdict, new CloseResult(false, $"未关闭游戏：{verdict.Reason}"));
        var close = await tryClose(gameHandle).ConfigureAwait(false);
        return close.Verified
            ? new PlanSettle(new PlanVerdict(true, "succeeded", $"{verdict.Reason}{close.Outcome}"), close)
            : new PlanSettle(new PlanVerdict(false, "unknown", close.Outcome), close);
    }

    /// <summary>启动交接后的有限 async 收尾：只等已有 Task，自行观察异常；不建线程、计时器或轮询，不延长桥 Job。</summary>
    public static async Task FollowUpAsync(Task execution, object progress, bool closeGameAfter, long gameHandle, Func<long, Task<CloseResult>>? tryClose = null, PlanRegistration? plan = null)
    {
        try
        {
            var settle = await SettleAsync(execution, () => EvaluatePlan(SnapshotFromProgress(progress, (ReadManualStop() == true) || PlanCancelled(plan))),
                closeGameAfter, gameHandle, tryClose ?? ProductionClose(null)).ConfigureAwait(false);
            Trace.TraceInformation("配置组计划收尾：{0}；{1}", settle.Verdict.Reason, settle.Close?.Outcome);
        }
        catch (Exception ex)
        {
            Trace.TraceError("配置组计划后台收尾异常：{0}", ex);
        }
    }

    /// <summary>生产关闭包装：CloseGame 只能在 BetterGI UI 线程调用。</summary>
    private static Func<long, Task<CloseResult>> ProductionClose(GameIdentity? identity) =>
        handle => Ui.InvokeAsync(() => TryCloseGame(handle, identity));

    /// <summary>关闭前在 UI 线程复核未手动取消、同游戏句柄、目标进程身份一致、任务空闲且无新计划过渡、无会话内其它原神实例；缺信息 fail closed。</summary>
    private static CloseResult TryCloseGame(long capturedHandle, GameIdentity? identity)
    {
        if (identity is null)
            return new(false, "未关闭游戏：缺少启动时锁定的目标进程证据。");
        var context = Reflect.Singleton(CancellationContextType);
        if (context is null)
            return new(false, "未关闭游戏：无法读取宿主取消状态。");
        if (AsBool(Reflect.Get(context, "IsManualStop")) is not false)
            return new(false, "未关闭游戏：手动取消状态不可读或已被取消。");
        if (Host.GameHandle != capturedHandle || capturedHandle == 0)
            return new(false, "未关闭游戏：当前游戏窗口已不是启动计划时的实例。");
        if (Host.TaskSemaphoreCount() is not > 0)
            return new(false, "未关闭游戏：任务锁未释放或状态未知。");
        if (ReadContinuousRun() is not false)
            return new(false, "未关闭游戏：连续运行状态未知或仍有计划在组间过渡。");
        var systemControl = Reflect.FindType(SystemControlType);
        if (systemControl?.GetMethod("GetProcessByHandle") is null || !HasParameterlessStaticCloseGame())
            return new(false, "未关闭游戏：SystemControl.CloseGame 或 GetProcessByHandle 不可用。");
        // CloseGame 会关同会话全部原神进程；发现其它实例宁可放弃关闭。
        if (HasForeignGenshinInstance(identity))
            return new(false, "未关闭游戏：当前会话存在其它原神进程实例，宽范围关闭可能误伤，不执行。");
        var process = Reflect.CallStatic(systemControl, "GetProcessByHandle", new IntPtr(capturedHandle));
        if (process is null)
            return new(false, "未关闭游戏：目标进程已无法从窗口句柄定位。");
        var currentIdentity = ReadIdentity(process);
        (process as IDisposable)?.Dispose();
        if (currentIdentity != identity)
            return new(false, $"未关闭游戏：目标进程身份已变化（PID 或启动时间不一致），不调用宽范围关闭。");
        Reflect.CallStatic(systemControl, "CloseGame");
        try
        {
            using var leftover = Process.GetProcessById(identity.ProcessId);
            return new(false, "已调用宿主 CloseGame，但目标进程仍在运行，关闭未核验。");
        }
        catch (ArgumentException)
        {
            return new(true, $"目标进程 {identity.ProcessId} 已退出，关闭已核验。");
        }
        catch (Exception error)
        {
            return new(false, $"已调用宿主 CloseGame，目标进程退出核验失败：{error.Message}");
        }
    }

    /// <summary>按宿主提供的真实进程名清单，检查当前会话是否还有目标之外的实例；清单不可读按存在实例处理（fail closed）。</summary>
    private static bool HasForeignGenshinInstance(GameIdentity identity)
    {
        var taskContext = Reflect.Singleton(TaskContextType);
        var method = taskContext?.GetType().GetMethod("GetGenshinGameProcessNameList");
        if (method?.Invoke(taskContext, null) is not System.Collections.IEnumerable names) return true;
        var session = Process.GetCurrentProcess().SessionId;
        foreach (var name in names)
        {
            if (name is not string processName || string.IsNullOrWhiteSpace(processName)) continue;
            foreach (var candidate in Process.GetProcessesByName(processName))
            {
                using (candidate)
                {
                    if (candidate.Id != identity.ProcessId && candidate.SessionId == session) return true;
                }
            }
        }
        return false;
    }

    /// <summary>启动前锁定目标进程 PID 与启动时间；读完立即释放句柄。</summary>
    private static GameIdentity CaptureGameIdentity(long handle)
    {
        var systemControl = Reflect.FindType(SystemControlType)
            ?? throw BridgeException.Missing("宿主 SystemControl 不可用。");
        var process = Reflect.CallStatic(systemControl, "GetProcessByHandle", new IntPtr(handle));
        if (process is null)
            throw BridgeException.GameNotReady("无法从游戏窗口句柄定位目标进程，closeGameAfter 计划不启动。");
        var identity = ReadIdentity(process);
        (process as IDisposable)?.Dispose();
        return identity ?? throw BridgeException.GameNotReady("无法读取目标进程 ID 或启动时间，closeGameAfter 计划不启动。");
    }

    private static GameIdentity? ReadIdentity(object process)
    {
        try
        {
            if (AsInt(Reflect.Get(process, "Id")) is not int pid)
                return null;
            var ticks = Reflect.Get(process, "StartTime") is DateTime start ? start.Ticks : -1;
            return ticks < 0 ? null : new GameIdentity(pid, ticks);
        }
        catch (Exception)
        {
            return null;
        }
    }

    /// <summary>精确匹配 public 实例 Task StartGroups(List&lt;ScriptGroup&gt;, TaskProgress?, bool)，不接受同名其他重载。</summary>
    private static bool HasStartGroups(Type type) => type.GetMethods().Any(method =>
        method.Name == StartGroupsMethod
        && method.IsPublic
        && !method.IsStatic
        && typeof(Task).IsAssignableFrom(method.ReturnType)
        && method.GetParameters() is { Length: 3 } parameters
        && parameters[0].ParameterType is { IsGenericType: true } listType
            && listType.GetGenericTypeDefinition() == typeof(List<>)
            && listType.GetGenericArguments()[0].FullName == ScriptGroupTypeFullName
        && parameters[1].ParameterType.FullName == TaskProgressType
        && parameters[2].ParameterType == typeof(bool));

    private static bool HasParameterlessStaticCloseGame() =>
        Reflect.FindType(SystemControlType)?.GetMethods().Any(method =>
            method.Name == "CloseGame"
            && method.IsPublic
            && method.IsStatic
            && method.GetParameters().Length == 0) == true;

    /// <summary>按请求顺序绑定各组真实宿主对象；不写任何组文件。</summary>
    private static System.Collections.IList BindGroupsInOrder(List<string> names)
    {
        var directory = Path.Combine(AppContext.BaseDirectory, "User", "ScriptGroup");
        return CombineGroups(names.Select(name => ScriptGroupTools.BindGroupsFromDisk(directory, name)));
    }

    /// <summary>把各批已绑定组按批次顺序合成一个强类型有序列表；元素是原对象引用，不改任何配置。</summary>
    public static System.Collections.IList CombineGroups(IEnumerable<System.Collections.IList> batches)
    {
        System.Collections.IList? combined = null;
        foreach (var batch in batches)
        {
            if (combined is null)
            {
                var elementType = batch.GetType().GetElementType()
                    ?? batch.GetType().GetGenericArguments().FirstOrDefault()
                    ?? throw BridgeException.Missing("宿主配置组列表元素类型不可用。");
                combined = (System.Collections.IList)Activator.CreateInstance(typeof(List<>).MakeGenericType(elementType))!;
            }
            foreach (var group in batch) combined.Add(group);
        }
        return combined ?? throw new BridgeException("INVALID_ARGUMENT", "没有可合并的配置组。", 400);
    }

    private static List<string> ParseNames(JsonElement arguments)
    {
        if (!arguments.TryGetProperty("groupNames", out var element) || element.ValueKind != JsonValueKind.Array)
            throw new BridgeException("INVALID_ARGUMENT", "groupNames 必须是非空字符串数组。", 400);
        var names = element.EnumerateArray()
            .Select(item => item.GetString()?.Trim())
            .ToList();
        if (names.Count is < 1 or > 32)
            throw new BridgeException("INVALID_ARGUMENT", "groupNames 需要 1 到 32 个配置组名称。", 400);
        if (names.Any(string.IsNullOrWhiteSpace))
            throw new BridgeException("INVALID_ARGUMENT", "groupNames 不允许空白名称。", 400);
        return names!;
    }

    private static bool? ReadManualStop()
    {
        var context = Reflect.Singleton(CancellationContextType);
        return context is null ? null : AsBool(Reflect.Get(context, "IsManualStop"));
    }

    /// <summary>共享快照构造：宿主读取时注入真实手动取消状态，测试可显式传入；CurrentScriptGroupProjectInfo 按其自身状态判完整收尾。</summary>
    public static PlanSnapshot SnapshotFromProgress(object progress, bool? manualStop)
    {
        var history = new List<PlanHistoryEntry>();
        if (Reflect.Get(progress, "History") is System.Collections.IEnumerable items && items is not string)
        {
            foreach (var item in items)
            {
                if (item is null) continue;
                history.Add(new PlanHistoryEntry(
                    Status: AsInt(Reflect.Get(item, "Status")),
                    TaskEnd: AsBool(Reflect.Get(item, "TaskEnd")),
                    EndPresent: Reflect.Get(item, "EndTime") is not null));
            }
        }
        // 宿主完成后 CurrentScriptGroupProjectInfo 仍非 null，按其自身状态判断是否已完整收尾。
        var current = Reflect.Get(progress, "CurrentScriptGroupProjectInfo");
        return new PlanSnapshot(
            OverallEndTimePresent: Reflect.Get(progress, "EndTime") is not null,
            ManualStopKnown: manualStop is not null,
            ManualStop: manualStop ?? false,
            CurrentProjectIncomplete: current is not null && !CurrentEntryComplete(current),
            history);
    }

    private static bool CurrentEntryComplete(object entry) =>
        AsInt(Reflect.Get(entry, "Status")) == 1
        && AsBool(Reflect.Get(entry, "TaskEnd")) == true
        && Reflect.Get(entry, "EndTime") is not null;

    private static int? AsInt(object? value) => value switch
    {
        null => null,
        int number => number,
        _ => Convert.ToInt32(value),
    };

    private static bool? AsBool(object? value) => value switch
    {
        null => null,
        bool flag => flag,
        _ => Convert.ToBoolean(value),
    };

    /// <summary>收尾只认启动时锁定的目标进程身份；GameHandle 在 CloseGame 后不变化，不能作为退出证据。</summary>
    public sealed record GameIdentity(int ProcessId, long StartTimeTicks);

    public sealed record PlanHistoryEntry(int? Status, bool? TaskEnd, bool EndPresent);

    public sealed record PlanSnapshot(
        bool OverallEndTimePresent,
        bool ManualStopKnown,
        bool ManualStop,
        bool CurrentProjectIncomplete,
        IReadOnlyList<PlanHistoryEntry> History);

    /// <summary>Status 只认 1 为成功、2 为失败；true 仅在全部成功且收尾核验通过时出现。</summary>
    public sealed record PlanVerdict(bool Succeeded, string Status, string Reason);

    public sealed record CloseResult(bool Verified, string Outcome);

    public sealed record PlanSettle(PlanVerdict Verdict, CloseResult? Close);

    /// <summary>
    /// 纯进度判定：已知的失败（手动取消、任一历史 Status=2）优先于缺证；
    /// Status 只认 1 为成功、2 为失败，其他值一律缺证 unknown，不猜测；
    /// 全部条目成功收尾且当前项目已完整、整体 EndTime 存在才算 succeeded。
    /// </summary>
    public static PlanVerdict EvaluatePlan(PlanSnapshot snapshot)
    {
        if (snapshot.ManualStopKnown && snapshot.ManualStop)
            return new(false, "failed", "检测到手动取消，计划未完整执行。");
        for (var index = 0; index < snapshot.History.Count; index++)
        {
            if (snapshot.History[index].Status == 2)
                return new(false, "failed", $"第 {index + 1} 条进度状态为 2（失败）；即使后续条目成功也不算全成功。");
        }
        if (!snapshot.ManualStopKnown)
            return new(false, "unknown", "无法读取宿主手动取消状态，缺证不判定。");
        if (!snapshot.OverallEndTimePresent)
            return new(false, "unknown", "宿主计划没有整体 EndTime，可能仍在运行或被取消。");
        if (snapshot.CurrentProjectIncomplete)
            return new(false, "unknown", "当前配置组项目未完整收尾（状态、结束标记或结束时间缺失）。");
        if (snapshot.History.Count == 0)
            return new(false, "unknown", "宿主进度没有历史记录，无法核验。");
        for (var index = 0; index < snapshot.History.Count; index++)
        {
            var entry = snapshot.History[index];
            var position = $"第 {index + 1} 条进度";
            if (entry.Status is null || (entry.Status != 1 && entry.Status != 2))
                return new(false, "unknown", $"{position}状态 {entry.Status?.ToString() ?? "缺失"} 无法识别，缺证不判定。");
            if (entry.TaskEnd is null)
                return new(false, "unknown", $"{position}缺少结束标记，缺证不判定。");
            if (!entry.TaskEnd.Value)
                return new(false, "unknown", $"{position}TaskEnd=false，收尾证据不完整。");
            if (!entry.EndPresent)
                return new(false, "unknown", $"{position}缺少结束时间，缺证不判定。");
        }
        return new(true, "succeeded", "整体 EndTime 存在，全部历史条目均成功收尾，当前项目已完整，未检测到手动取消。");
    }
}
