using BgiBridge.Protocol;

namespace BgiBridge.Bgi;

/// <summary>在当前桥任务执行期间将取消传递给宿主任务，不取消之后的任务。</summary>
public sealed class HostTaskCancellation : IDisposable
{
    private readonly CancellationTokenRegistration _registration;
    private readonly CancellationToken _token;
    private readonly object _gate = new();
    private Task _stop = Task.CompletedTask;
    private int _active = 1;
    private int _returned;

    public HostTaskCancellation(CancellationToken token)
    {
        _token = token;
        _registration = token.Register(() =>
        {
            lock (_gate)
                _stop = StopWhenRunningAsync();
        });
    }

    private async Task StopWhenRunningAsync()
    {
        while (Volatile.Read(ref _active) != 0 && Volatile.Read(ref _returned) == 0)
        {
            var stopped = await Ui.InvokeAsync(() =>
            {
                if (Volatile.Read(ref _active) == 0 || Volatile.Read(ref _returned) != 0) return true;
                if (Host.TaskSemaphoreCount() != 0) return false;
                RequestStop();
                return true;
            }).ConfigureAwait(false);
            if (stopped) return;
            await Task.Delay(20).ConfigureAwait(false);
        }
    }

    public static void RequestStop()
    {
        var context = Reflect.Singleton("BetterGenshinImpact.Core.Script.CancellationContext")
            ?? throw BridgeException.Missing("当前宿主没有独立任务取消上下文。");
        Reflect.Call(context, "ManualCancel");
    }

    public async Task CompleteAsync()
    {
        Interlocked.Exchange(ref _returned, 1);
        Task stop;
        lock (_gate) stop = _stop;
        await stop.ConfigureAwait(false);
        _token.ThrowIfCancellationRequested();
    }

    public void Dispose()
    {
        Interlocked.Exchange(ref _active, 0);
        _registration.Dispose();
    }
}
