using System.Reflection;
using System.Windows.Input;

namespace BgiBridge.Bgi;

/// <summary>执行宿主的 ICommand 并等它真正结束。CommunityToolkit 的异步命令把工作放在 ExecuteAsync 返回的 Task 里，ICommand.Execute 是 void，只看 Execute 会把「已经返回」当成「已经完成」。</summary>
public static class Commands
{
    internal static CancellationTokenRegistration LinkOwnCancellation(ICommand command,Task execution,CancellationToken token)
    {
        var task=command.GetType().GetProperty("ExecutionTask");
        var canCancel=command.GetType().GetProperty("CanBeCanceled");
        var cancel=Reflect.FindMethod(command.GetType(),"Cancel",0);
        if(task is null||canCancel is null||cancel is null)return default;
        return token.Register(()=>
        {
            // 只请求当前 ICommand 自己的取消，不对全局 CancellationContext 发停止。
            if(ReferenceEquals(task.GetValue(command),execution) && canCancel.GetValue(command) is true)
                cancel.Invoke(command,null);
        });
    }
    public static Task RunAsync(ICommand command, object? parameter = null)
    {
        if (Reflect.FindMethod(command.GetType(), "ExecuteAsync", 1) is not { } executeAsync)
        {
            command.Execute(parameter);
            return Task.CompletedTask;
        }
        try
        {
            return executeAsync.Invoke(command, [parameter]) as Task ?? Task.CompletedTask;
        }
        catch (TargetInvocationException error)
        {
            // 命令同步抛出时，不把反射包装抛给调用方。
            throw Reflect.Translate(Reflect.Root(error));
        }
    }
}
