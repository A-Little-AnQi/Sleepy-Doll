using BgiBridge.Protocol;

namespace BgiBridge.Bgi;

/// <summary>
/// 宿主 BetterGI 的入口点。类型一律按全名定位，缺失返回 null 而不抛——
/// 工具层据此把「这个版本没有该功能」变成明确错误。
/// </summary>
public static class Host
{
    // 只在第一次用到时解析，并缓存结果。
    private static readonly Lazy<Type?> TaskContextType = new(() => Reflect.FindType("BetterGenshinImpact.GameTask.TaskContext"));
    private static readonly Lazy<Type?> TaskControlType = new(() => Reflect.FindType("BetterGenshinImpact.GameTask.Common.TaskControl"));
    private static readonly Lazy<Type?> AppType = new(() => Reflect.FindType("BetterGenshinImpact.App"));

    /// <summary>按类型而非程序集名判断——宿主程序集叫 BetterGI，命名空间却是 BetterGenshinImpact.*。</summary>
    public static bool HostLoaded => Reflect.HostReady;

    /// <summary><c>TaskContext.Instance()</c>：截图器与游戏窗口状态。</summary>
    public static object? TaskContext()
    {
        if (TaskContextType.Value is null) return null;
        var instance = Reflect.Singleton("BetterGenshinImpact.GameTask.TaskContext");
        if (instance is null) return null;
        // TaskContext.Instance() 是懒加载，未初始化时拿到的是空壳。
        return instance;
    }

    /// <summary><c>TaskContext.Instance().IsInitialized</c> —— 截图器是否已启动。</summary>
    public static bool CaptureReady =>
        TaskContext() is { } context && Reflect.Get(context, "IsInitialized") is true;

    /// <summary>截图器没启动时是 <c>IntPtr.Zero</c>，此时前台判断不可信。</summary>
    public static long GameHandle
    {
        get
        {
            var handle = TaskContext() is { } context ? Reflect.Get(context, "GameHandle") : null;
            return handle is IntPtr pointer ? pointer.ToInt64() : 0;
        }
    }

    /// <summary><c>TaskControl.TaskSemaphore.CurrentCount == 0</c> 表示已有独立任务持锁。</summary>
    public static int? TaskSemaphoreCount()
    {
        if (TaskControlType.Value is null) return null;
        var semaphore = Reflect.GetStatic(TaskControlType.Value, "TaskSemaphore");
        return semaphore is null ? null : Reflect.Get(semaphore, "CurrentCount") as int?;
    }

    /// <summary>没有独立任务在跑。</summary>
    public static bool IsIdle => TaskSemaphoreCount() is not 0;

    /// <summary>
    /// 仅在持锁时可读：原版 TaskRunner.End() 在失败路径上不清空它，
    /// 空闲时读到非 null 是上次运行的残留。
    /// </summary>
    public static object? CurrentScriptProject
    {
        get
        {
            if (!IsIdle) return TaskContext() is { } context ? Reflect.Get(context, "CurrentScriptProject") : null;
            return null;
        }
    }

    /// <summary><c>App.ServiceProvider</c>，用来解析宿主 DI 里的服务。</summary>
    public static IServiceProvider? Services()
    {
        if (AppType.Value is null) return null;
        return Reflect.GetStatic(AppType.Value, "ServiceProvider") as IServiceProvider;
    }

    private static readonly Lazy<Type?> AllConfigTypeLazy =
        new(() => Reflect.FindType("BetterGenshinImpact.Core.Config.AllConfig"));

    /// <summary><c>AllConfig</c> 的类型。设置目录靠它反射整棵配置树。</summary>
    public static Type? AllConfigType() => AllConfigTypeLazy.Value;

    /// <summary>
    /// 当前生效的配置对象。**不要**去读写 User/config.json——
    /// ConfigService 缓存了对象并挂了自动保存回调，会把文件里的改动覆盖回去。
    /// </summary>
    public static object? AllConfigInstance()
    {
        // ConfigService.Config 是静态字段，最直接。
        var configService = Reflect.FindType("BetterGenshinImpact.Service.ConfigService");
        if (configService is not null)
        {
            var fromStatic = Reflect.GetStatic(configService, "Config");
            if (fromStatic is not null) return fromStatic;
        }

        // 退回 DI 里的 IConfigService。
        var service = Service("BetterGenshinImpact.Service.Interface.IConfigService");
        return service is null ? null : Reflect.Call(service, "Get");
    }

    /// <summary>把配置落盘。属性 setter 本身会自动保存，这里是批量改完后手动触发的那一次。</summary>
    public static void SaveConfig()
    {
        var service = Service("BetterGenshinImpact.Service.Interface.IConfigService")
            ?? throw BridgeException.Missing("配置服务尚未初始化。");
        Reflect.Call(service, "Save");
    }

    /// <summary>批量改设置前摘掉自动保存，否则每改一项写一次盘。须与 RestoreAutoSave 配对。</summary>
    public static object? TakeAutoSave(object config)
    {
        var property = config.GetType().GetProperty("OnAnyChangedAction");
        if (property is null || !property.CanRead || !property.CanWrite) return null;
        var previous = property.GetValue(config);
        property.SetValue(config, null);
        return previous;
    }

    public static void RestoreAutoSave(object config, object? previous)
    {
        var property = config.GetType().GetProperty("OnAnyChangedAction");
        if (property is null || !property.CanWrite) return;
        property.SetValue(config, previous);
    }

    /// <summary>命令目录用它滤掉「程序集里有、宿主没注册」的死命令。</summary>
    public static bool? IsRegistered(IServiceProvider? services, Type type)
    {
        if (services is null) return null;
        try
        {
            var probeType = Reflect.FindType("Microsoft.Extensions.DependencyInjection.IServiceProviderIsService");
            var probe = probeType is null ? null : services.GetService(probeType);
            return probe is null ? null : Reflect.Call(probe, "IsService", type) as bool?;
        }
        catch
        {
            return false;
        }
    }

    /// <summary>解析宿主 DI 服务；拿不到返回 null。</summary>
    public static object? Service(string typeFullName)
    {
        var services = Services();
        var type = Reflect.FindType(typeFullName);
        if (services is null || type is null) return null;
        try
        {
            return services.GetService(type);
        }
        catch
        {
            return null;
        }
    }

    /// <summary>诊断：报告桥在宿主里实际能拿到的类型与单例。</summary>
    public static object Probe()
    {
        var report = new Dictionary<string, object?>
        {
            ["hostLoaded"] = HostLoaded,
            ["hostAssembly"] = Reflect.FindAssembly(Reflect.HostAssembly)?.GetName().Version?.ToString(),
            ["loadedAssemblies"] = AppDomain.CurrentDomain.GetAssemblies().Length,
            ["uiDispatcherAvailable"] = Ui.Available,
        };

        foreach (var (label, typeName) in Probes)
        {
            var type = Reflect.FindType(typeName);
            report[label] = type is null ? "缺失" : type.FullName;
        }

        // 静态单例是注入方案的核心价值：跨进程拿不到，同进程直接可读。
        report["taskContextReachable"] = TaskContext() is not null;
        report["captureReady"] = CaptureReady;
        report["gameHandle"] = GameHandle;
        report["taskSemaphoreCount"] = TaskSemaphoreCount();
        report["serviceProviderReachable"] = Services() is not null;

        if (!HostLoaded)
        {
            report["hint"] = "宿主程序集尚未加载。若刚注入，等 BetterGI 进入主界面后重试。";
        }

        return report;
    }

    private static readonly (string Label, string TypeName)[] Probes =
    [
        ("taskContext", "BetterGenshinImpact.GameTask.TaskContext"),
        ("taskControl", "BetterGenshinImpact.GameTask.Common.TaskControl"),
        ("taskTriggerDispatcher", "BetterGenshinImpact.GameTask.TaskTriggerDispatcher"),
        ("taskRunner", "BetterGenshinImpact.GameTask.TaskRunner"),
        ("cancellationContext", "BetterGenshinImpact.Core.Script.CancellationContext"),
        ("configService", "BetterGenshinImpact.Service.ConfigService"),
        ("allConfig", "BetterGenshinImpact.Core.Config.AllConfig"),
        ("bv", "BetterGenshinImpact.GameTask.Common.BgiVision.Bv"),
        ("genshin", "BetterGenshinImpact.Core.Script.Dependence.Genshin"),
        ("systemControl", "BetterGenshinImpact.GameTask.SystemControl"),
        ("scriptGroup", "BetterGenshinImpact.Core.Script.Group.ScriptGroup"),
        ("scriptGroupProject", "BetterGenshinImpact.Core.Script.Group.ScriptGroupProject"),
        ("scriptRepoUpdater", "BetterGenshinImpact.Core.Script.ScriptRepoUpdater"),
        ("iScriptService", "BetterGenshinImpact.Service.Interface.IScriptService"),
    ];
}
