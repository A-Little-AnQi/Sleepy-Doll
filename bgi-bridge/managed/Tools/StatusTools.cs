using System.Text.Json;
using BgiBridge.Bgi;
using BgiBridge.Catalog;

namespace BgiBridge.Tools;

/// <summary>状态与生命周期组。目前只有打通验证需要的几个，其余按计划补齐。</summary>
public static class StatusTools
{
    public const string Group = "lifecycle";

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
    }
}
