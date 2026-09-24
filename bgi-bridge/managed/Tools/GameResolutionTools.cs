using System.Text.Json;
using BgiBridge.Bgi;
using BgiBridge.Catalog;
using BgiBridge.Protocol;
using Microsoft.Win32;

namespace BgiBridge.Tools;

/// <summary>只修改 BetterGI 当前要启动的原神显示记录；启动后仍需核对真实窗口。</summary>
public static class GameResolutionTools
{
    private const string WidthName = "Screenmanager Resolution Width_h182942802";
    private const string HeightName = "Screenmanager Resolution Height_h2627698369";
    private const string FullscreenName = "Screenmanager Is Fullscreen mode_h409977534";
    private const string ChinaHeightName = "Screenmanager Resolution Height_h2627697771";
    private const string ChinaFullscreenName = "Screenmanager Is Fullscreen mode_h3981298716";

    public static void Register(MethodRegistry registry)
    {
        const string id = "bgi.set_game_resolution";
        var guide = new AgentGuide(
            "设置原神分辨率",
            "把 BetterGI 当前要启动的原神显示记录改为目标值。仅写入注册表不保证启动后的实际窗口尺寸；远程桌面会话太小时游戏会覆盖它。",
            ["游戏当前分辨率非 16:9 导致任务无法运行，且原神已关闭（先用 bgi.exit_game）。", "用户明确要求改游戏分辨率。"],
            ["原神处于关闭状态；当前桌面会话能容纳目标客户区。"],
            ["只写当前用户注册表 HKCU 下原神的显示记录（宽/高/窗口模式）；不动其他任何配置。"],
            "written=true 只表示当前游戏的三条显示记录已写入；不能当作实际 16:9 成功，必须在下次 bgi.start_game 后读取 gameResolution。",
            "下次启动后用 bgi.get_status 的 gameResolution 确认目标分辨率已生效。",
            "再次调用传回原值即可恢复。",
            [JsonSerializer.SerializeToElement(new { width = 1920, height = 1080, mode = "borderless" })],
            "bridge-stable-operation");
        registry.Register(
            id,
            "lifecycle",
            "修改 BetterGI 当前游戏的显示记录（游戏需关闭且桌面能容纳目标）；启动后必须核对实际窗口。",
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
        if (Host.CaptureReady || Host.GameProcessRunning())
            throw BridgeException.GameNotReady(
                "原神正在运行，注册表修改不会生效。先用 bgi.exit_game 关闭游戏，再调用本接口。");
        var width = arguments.GetProperty("width").GetInt32();
        var height = arguments.GetProperty("height").GetInt32();
        if (Host.DisplaySize() is { } display && (width > display.Width || height > display.Height))
            throw BridgeException.InvalidArgument(
                $"当前桌面会话只有 {display.Width}x{display.Height}，放不下目标游戏客户区 {width}x{height}。"
                + "先把远程桌面会话调整到至少目标尺寸；不改用较低分辨率，也不写入注定无法生效的游戏配置。");
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
        // 同一 Windows 用户可能同时装国服与国际服。必须按 BetterGI 当前配置的
        // 启动程序选中对应记录；枚举到的第一条不是本次游戏的配置。
        var home = Host.Service(StatusTools.StartViewModel);
        var config = home is null ? null : Reflect.Get(home, "Config");
        var start = config is null ? null : Reflect.Get(config, "GenshinStartConfig");
        var executable = Path.GetFileName(start is null ? null : Reflect.Get(start, "InstallPath") as string);
        var preferred = executable?.ToLowerInvariant() switch
        {
            "yuanshen.exe" => "原神",
            "genshinimpact.exe" => "Genshin Impact",
            _ => null,
        };
        var available = new List<string>();
        foreach (var name in new[] { "原神", "Genshin Impact" })
        {
            if (!subkeys.Contains(name, StringComparer.OrdinalIgnoreCase)) continue;
            using var candidate = Registry.CurrentUser.OpenSubKey($"Software\\miHoYo\\{name}", writable: false);
            if (candidate?.GetValue(WidthName) is not null)
            {
                available.Add(name);
            }
        }
        var selected = preferred is not null && available.Contains(preferred, StringComparer.OrdinalIgnoreCase)
            ? preferred
            : preferred is null && available.Count == 1 ? available[0] : null;
        if (selected is null)
            throw BridgeException.Missing(
                $"无法确定当前原神的显示记录。BetterGI 启动程序：{executable ?? "未配置"}；可用记录：{string.Join("、", available)}。未修改任何注册表值。");

        using var target = Registry.CurrentUser.OpenSubKey($"Software\\miHoYo\\{selected}", writable: true)
            ?? throw BridgeException.Missing($"无法写入 {selected} 的显示记录。");
        var heightName = selected == "原神" ? ChinaHeightName : HeightName;
        var fullscreenName = selected == "原神" ? ChinaFullscreenName : FullscreenName;
        if (target.GetValue(heightName) is null || target.GetValue(fullscreenName) is null)
            throw BridgeException.Missing($"{selected} 的高度或窗口模式记录缺失，未修改任何注册表值。");

        target.SetValue(WidthName, width, RegistryValueKind.DWord);
        target.SetValue(heightName, height, RegistryValueKind.DWord);
        target.SetValue(fullscreenName, fullscreenMode, RegistryValueKind.DWord);
        return Task.FromResult<object?>(new
        {
            written = true,
            width,
            height,
            mode,
            game = selected,
            note = "显示记录已写入；下次启动后需用 bgi.get_status 核对实际分辨率。",
        });
    }
}
