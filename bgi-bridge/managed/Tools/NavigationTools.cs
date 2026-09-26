using System.Reflection;
using System.Text.Json;
using System.Windows;
using BgiBridge.Bgi;
using BgiBridge.Catalog;
using BgiBridge.Protocol;

namespace BgiBridge.Tools;

public static class NavigationTools
{
    public static readonly (string Id, string Title, string Type)[] Pages = [
        ("home", "启动", "HomePage"), ("triggers", "实时触发", "TriggerSettingsPage"),
        ("tasks", "独立任务", "TaskSettingsPage"), ("scheduler", "调度器", "ScriptControlPage"),
        ("oneDragon", "一条龙", "OneDragonFlowPage"), ("javascript", "JS 脚本", "JsListPage"),
        ("pathing", "地图追踪", "MapPathingPage"), ("keyMouse", "录制回放", "KeyMouseRecordPage"),
        ("macros", "操控辅助", "MacroSettingsPage"), ("music", "音乐", "MusicPage"),
        ("hotkeys", "快捷键", "HotKeyPage"), ("keyBindings", "按键绑定", "KeyBindingsSettingsPage"),
        ("notifications", "通知", "NotificationSettingsPage"), ("settings", "通用设置", "CommonSettingsPage")
    ];

    public static void Register(MethodRegistry registry)
    {
        foreach (var id in new[] { "bgi.list_pages", "bgi.open_page" })
        {
            var read = id == "bgi.list_pages";
            var guide = new AgentGuide(read ? "列出页面" : "打开页面", read ? "列出 BetterGI 主窗口的明确页面标识与名称。" : "按明确页面标识打开 BetterGI 页面并核验导航选择。",
                ["用户要求打开、切换或显示某个 BetterGI 页面时；打开页面不代替配置或任务执行。"],
                ["桥已连接；不要求游戏或截图器。"], read ? ["无写入副作用。"] : ["显示 BetterGI 主窗口并切换页面，不启动游戏任务。"],
                read ? "pages 返回可用页面。" : "opened=true、verified=true 表示主窗口可见且导航选择为请求页面。",
                "使用返回的页面标识；不查生命周期命令，不猜类名。", "可再打开另一页面。", [], "bridge-stable-operation");
            registry.Register(id, "navigation", guide.Purpose, read ? List : Open, readOnly: read, guide: guide);
        }
    }

    private static Task<object?> List(JsonElement arguments, CancellationToken cancellation) => Task.FromResult<object?>(new
    {
        pages = Pages.Select(page => new { id = page.Id, title = page.Title, available = PageType(page.Type) is not null })
    });

    private static Type? PageType(string name) => Reflect.FindType("BetterGenshinImpact.View.Pages." + name);

    private static Task<object?> Open(JsonElement arguments, CancellationToken cancellation) => Ui.InvokeAsync<object?>(() =>
    {
        cancellation.ThrowIfCancellationRequested();
        var page = Pages.Single(item => item.Id == arguments.GetProperty("page").GetString());
        var target = PageType(page.Type) ?? throw BridgeException.Missing($"当前宿主没有「{page.Title}」页面。");
        var serviceType = Reflect.FindType("Wpf.Ui.INavigationService") ?? throw BridgeException.Missing("导航服务类型不可用。");
        var service = Host.Services()?.GetService(serviceType) ?? throw BridgeException.Missing("导航服务未注册。");
        var navigate = service.GetType().GetMethod("Navigate", [typeof(Type)]) ?? throw BridgeException.Missing("宿主没有按页面类型导航的入口。");
        try { navigate.Invoke(service, [target]); }
        catch (TargetInvocationException error) { throw BridgeException.Failed(Reflect.Root(error).Message); }
        var navigation = Reflect.Call(service, "GetNavigationControl") ?? throw BridgeException.Failed("导航控件不可用。");
        var selected = Reflect.Get(navigation, "SelectedItem");
        if (selected is null || Reflect.Get(selected, "TargetPageType") as Type != target)
            throw BridgeException.Failed("导航选择未切换到目标页面，未确认打开。");
        var window = Application.Current?.MainWindow ?? throw BridgeException.Missing("宿主主窗口不可用。");
        window.Show();
        if (window.WindowState == WindowState.Minimized) window.WindowState = WindowState.Normal;
        window.Activate();
        return new { page = page.Id, title = page.Title, opened = window.IsVisible, verified = window.IsVisible };
    });
}
