using System.Text.Json;
using BgiBridge.Bgi;
using BgiBridge.Catalog;
using BgiBridge.Protocol;
using Microsoft.Win32;

namespace BgiBridge.Tools;

/// <summary>原神显示分辨率的受控修改：游戏自己把显示设置记在注册表里，
/// 启动参数会被它覆盖——这是「启动参数不生效」的根因，必须改这里。</summary>
public static class GameResolutionTools
{
    private const string WidthName = "Screenmanager Resolution Width_h182942802";
    private const string HeightName = "Screenmanager Resolution Height_h2627698369";
    private const string FullscreenName = "Screenmanager Is Fullscreen mode_h409977534";

    public static void Register(MethodRegistry registry)
    {
        const string id = "bgi.set_game_resolution";
        var guide = new AgentGuide(
            "设置原神分辨率",
            "把原神保存的显示分辨率改为目标值（写入游戏自己的注册表记录，下次启动生效）。原神会记住上次的显示设置并覆盖 -screen-width 等启动参数；要让 16:9 生效必须改这里。",
            ["游戏当前分辨率非 16:9 导致任务无法运行，且原神已关闭（先用 bgi.exit_game）。", "用户明确要求改游戏分辨率。"],
            ["原神处于关闭状态——运行中修改会被拒绝且无效。"],
            ["只写当前用户注册表 HKCU 下原神的显示记录（宽/高/窗口模式）；不动其他任何配置。"],
            "written=true 表示三条记录已写入；下次 bgi.start_game 生效。远程桌面下窗口模式的最大尺寸受会话分辨率限制，必要时提示用户以 16:9 会话重连。",
            "下次启动后用 bgi.get_status 的 gameResolution 确认目标分辨率已生效。",
            "再次调用传回原值即可恢复。",
            [JsonSerializer.SerializeToElement(new { width = 1920, height = 1080, mode = "borderless" })],
            "bridge-stable-operation");
        registry.Register(
            id,
            "lifecycle",
            "修改原神显示分辨率（游戏需关闭；写入游戏注册表记录，下次启动生效）。",
            Invoke,
            readOnly: false,
            destructive: false,
            inputSchema: Schema(),
            guide: guide,
            requiresGameReady: false);
    }

    private static JsonElement Schema() =>
        JsonSerializer.SerializeToElement(new
        {
            type = "object",
            required = new[] { "width", "height" },
            properties = new
            {
                width = new { type = "integer", minimum = 640, maximum = 7680 },
                height = new { type = "integer", minimum = 480, maximum = 4320 },
                mode = new
                {
                    type = "string",
                    @enum = new[] { "windowed", "borderless", "fullscreen" },
                    description = "窗口模式，默认 borderless（无边框）",
                },
            },
            additionalProperties = false,
        });

    private static Task<object?> Invoke(JsonElement arguments, CancellationToken cancellation)
    {
        cancellation.ThrowIfCancellationRequested();
        if (Host.CaptureReady)
            throw BridgeException.GameNotReady(
                "原神正在运行，注册表修改不会生效。先用 bgi.exit_game 关闭游戏，再调用本接口。");
        var width = arguments.GetProperty("width").GetInt32();
        var height = arguments.GetProperty("height").GetInt32();
        var mode = arguments.TryGetProperty("mode", out var named)
            && named.GetString() is { Length: > 0 } text
            ? text.Trim().ToLowerInvariant()
            : "borderless";
        var fullscreenMode = mode switch
        {
            "fullscreen" => 1,
            "windowed" => 0,
            _ => 2,
        };

        var root = Registry.CurrentUser.OpenSubKey("Software\\miHoYo", writable: false);
        if (root is null)
            throw BridgeException.Missing(
                "注册表里没有 miHoYo 配置：原神从未在本机启动过。先启动一次游戏再改分辨率。");
        string[] subkeys;
        using (root)
        {
            subkeys = root.GetSubKeyNames();
        }
        // 国际服与国服键名不同，两边都可能存在；有显示记录的才算数。
        RegistryKey? target = null;
        foreach (var name in subkeys)
        {
            var candidate = Registry.CurrentUser.OpenSubKey($"Software\\miHoYo\\{name}", writable: true);
            if (candidate is null) continue;
            if (candidate.GetValue(WidthName) is not null)
            {
                target = candidate;
                break;
            }
            candidate.Dispose();
        }
        if (target is null)
            throw BridgeException.Missing(
                $"没有找到原神的显示记录（HKCU\\Software\\miHoYo 下无 Screenmanager 键）。子键：{string.Join("、", subkeys)}");

        using (target)
        {
            target.SetValue(WidthName, width, RegistryValueKind.DWord);
            target.SetValue(HeightName, height, RegistryValueKind.DWord);
            target.SetValue(FullscreenName, fullscreenMode, RegistryValueKind.DWord);
        }
        return Task.FromResult<object?>(new
        {
            written = true,
            width,
            height,
            mode,
            note = "已写入原神的显示记录，下次启动生效。远程桌面下窗口尺寸受会话分辨率限制。",
        });
    }
}
