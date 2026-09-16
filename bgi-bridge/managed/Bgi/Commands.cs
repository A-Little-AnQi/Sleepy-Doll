using System.Reflection;
using System.Windows.Input;

namespace BgiBridge.Bgi;

/// <summary>
/// 执行宿主的 ICommand 并等它真正结束。
///
/// CommunityToolkit 的异步命令把工作放在 ExecuteAsync 返回的 Task 里，而 ICommand.Execute
/// 是 void —— 只看 Execute 会把「已经返回」当成「已经完成」。桥零第三方依赖，按签名探测。
/// </summary>
public static class Commands
{
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
            // 命令同步抛出时别把反射包装交给调用方。
            throw Reflect.Translate(Reflect.Root(error));
        }
    }
}
