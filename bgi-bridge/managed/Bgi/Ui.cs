using System.Reflection;
using BgiBridge.Protocol;

namespace BgiBridge.Bgi;

/// <summary>
/// 切到 WPF UI 线程。配置保存、绑在页面上的 ObservableObject / ObservableCollection、
/// 各 ViewModel 命令都只能在 UI 线程碰。
/// </summary>
public static class Ui
{
    private static object? Dispatcher
    {
        get
        {
            try
            {
                var application = Reflect.FindType("System.Windows.Application");
                if (application is null) return null;
                var current = Reflect.GetStatic(application, "Current");
                return current is null ? null : Reflect.Get(current, "Dispatcher");
            }
            catch { return null; }
        }
    }

    public static bool Available => Dispatcher is not null;

    public static Task InvokeAsync(Action action) => Dispatch(typeof(Action), action);

    public static async Task<T> InvokeAsync<T>(Func<T> func)
    {
        T result = default!;
        await InvokeAsync((Action)(() => result = func())).ConfigureAwait(false);
        return result;
    }

    public static async Task<T> InvokeAsync<T>(Func<Task<T>> func)
    {
        Task<T>? operation = null;
        await InvokeAsync((Action)(() => operation = func())).ConfigureAwait(false);
        return await operation!.ConfigureAwait(false);
    }

    private static async Task<object?> Dispatch(Type delegateType, Delegate callback)
    {
        var dispatcher = Dispatcher
            ?? throw BridgeException.Failed("拿不到 WPF Dispatcher，界面可能还没初始化。");

        var method = dispatcher.GetType().GetMethod("InvokeAsync", [delegateType])
            ?? throw BridgeException.Missing("当前 WPF 版本的 Dispatcher.InvokeAsync 签名与预期不符。");

        object? operation;
        try
        {
            operation = method.Invoke(dispatcher, [callback]);
        }
        catch (TargetInvocationException ex)
        {
            throw BridgeException.Failed($"切 UI 线程失败：{Reflect.Root(ex).Message}");
        }

        // DispatcherOperation.Task 才是真正代表委托完成的任务。不 await 它会变成
        // "调用返回了但命令还在跑"。
        if (operation?.GetType().GetProperty("Task")?.GetValue(operation) is Task task)
        {
            await task.ConfigureAwait(false);
            return task.GetType().IsGenericType
                ? task.GetType().GetProperty("Result")?.GetValue(task)
                : null;
        }

        return null;
    }
}
