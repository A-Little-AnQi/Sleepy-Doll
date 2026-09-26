using System.Collections;
using System.Reflection;
using System.IO;
using System.Text.Json;
using BgiBridge.Catalog;
using BgiBridge.Protocol;

namespace BgiBridge.Bgi;

/// <summary>补齐宿主遗留空入口，业务代码留在 BGI 提供方。</summary>
public static class ImplementedCommands
{
    public static bool Handles(string name) => name is "common_settings_page.switch_mask_enabled" or "common_settings_page.switch_taken_screenshot_enabled"
        or "home_page.test" or "task_settings_page.switch_auto_track_path" or "form.edit_at" or "form.save"
        or "map_pathing_dev.drop_down_changed" or "auto_pick_black_list.edit_at" or "auto_pick_white_list.edit_at" or "auto_pick_black_list.save" or "auto_pick_white_list.save";

    public static JsonElement Input(string name) => name switch
    {
        "form.edit_at" or "auto_pick_black_list.edit_at" or "auto_pick_white_list.edit_at" => ArgumentSchema.Parse("""{"type":"object","properties":{"index":{"type":"integer","minimum":0},"value":{"description":"符合真实 List 元素类型；复杂元素用 objectId。"}},"required":["index","value"],"additionalProperties":false}"""),
        "task_settings_page.switch_auto_track_path" => ArgumentSchema.Parse("""{"type":"object","properties":{"path":{"type":"string","minLength":1,"maxLength":1024}},"required":["path"],"additionalProperties":false,"description":"User/AutoPathing 下的实际路线，不能使用旧版硬编码 way2.json。"}"""),
        _ => ArgumentSchema.Empty,
    };

    public static bool NeedsInput(string name) => name.EndsWith(".edit_at", StringComparison.Ordinal) || name == "task_settings_page.switch_auto_track_path";

    public static async Task<object?> Run(string name, object owner, JsonElement input, CancellationToken cancellation)
    {
        ArgumentSchema.Validate(input, Input(name));
        cancellation.ThrowIfCancellationRequested();
        switch (name)
        {
            case "common_settings_page.switch_mask_enabled":
            {
                var config = Reflect.Get(Reflect.Get(owner, "Config")!, "MaskWindowConfig")!;
                var enabled = Reflect.Get(config, "MaskEnabled") is true;
                var window = Reflect.Singleton("BetterGenshinImpact.View.MaskWindow") ?? throw BridgeException.Missing("遮罩窗口不可用。");
                Reflect.Call(window, enabled ? "Show" : "Hide"); Host.SaveConfig();
                return new { applied = true, enabled, visible = Reflect.Get(window,"IsVisible"), verified = (Reflect.Get(window,"IsVisible") is true) == enabled };
            }
            case "common_settings_page.switch_taken_screenshot_enabled":
            {
                var common = Reflect.Get(Reflect.Get(owner,"Config")!,"CommonConfig")!;
                Host.SaveConfig();
                return new { applied = true, enabled = Reflect.Get(common,"ScreenshotEnabled"), verified = true, meaning = "截图保存开关已持久化；没有截图目标时不生成伪造图片。" };
            }
            case "home_page.test":
            {
                if (!Host.CaptureReady) throw BridgeException.GameNotReady("图像测试需要先启动截图器。");
                var type = Reflect.RequireType("BetterGenshinImpact.View.CaptureTestWindow");
                var window = Activator.CreateInstance(type)!;
                var mode = Reflect.Get(Reflect.Get(owner,"Config")!,"CaptureMode")!;
                var converted = Reflect.CallStatic(Reflect.RequireType("Fischless.GameCapture.CaptureModeExtensions"),"ToCaptureMode",mode)
                    ?? throw BridgeException.Missing("当前截图模式转换不可用。");
                Reflect.Call(window,"StartCapture",(IntPtr)Host.GameHandle,converted); Reflect.Call(window,"Show");
                return new { opened = Reflect.Get(window,"IsVisible"), verified = Reflect.Get(window,"IsVisible") is true, gameHandle = Host.GameHandle };
            }
            case "map_pathing_dev.drop_down_changed":
            {
                var config = Reflect.Get(owner,"DevConfig")!;
                var map = Reflect.Get(config,"RecordMapName") as string ?? throw BridgeException.InvalidArgument("尚未选择地图。");
                var maps = Reflect.Get(owner,"MapTypeItems") as IEnumerable;
                if (maps is null || !maps.Cast<object>().Any(item => string.Equals(Reflect.Get(item,"EnumName")?.ToString(),map,StringComparison.Ordinal)))
                    throw BridgeException.InvalidArgument("地图名称不在当前原生选项中。");
                Host.SaveConfig();
                return new { applied = true, mapName = map, verified = true };
            }
            case "task_settings_page.switch_auto_track_path":
                return await RunTask(cancellation, async token =>
                {
                    var root = Path.GetFullPath(Path.Combine(AppContext.BaseDirectory,"User","AutoPathing"));
                    var path = Path.GetFullPath(Path.Combine(root,input.GetProperty("path").GetString()!));
                    if (!path.StartsWith(root+Path.DirectorySeparatorChar,StringComparison.OrdinalIgnoreCase) || !File.Exists(path)) throw BridgeException.InvalidArgument("路线不在 User/AutoPathing 或文件不存在。");
                    var modelType = Reflect.RequireType("BetterGenshinImpact.GameTask.AutoPathing.Model.PathingTask");
                    var model = Reflect.CallStatic(modelType,"BuildFromJson",File.ReadAllText(path))!;
                    var executor = Activator.CreateInstance(Reflect.RequireType("BetterGenshinImpact.GameTask.AutoPathing.PathExecutor"),token)!;
                    if (Reflect.Call(executor,"Pathing",model) is not Task run) throw BridgeException.Missing("地图执行器没有返回可等待任务。");
                    await run.ConfigureAwait(false); token.ThrowIfCancellationRequested();
                    var success = Reflect.Get(executor,"SuccessEnd") is true;
                    return new { finished = success, verified = success, path = Path.GetRelativePath(root,path), engine = "AutoPathing.PathExecutor" };
                }).ConfigureAwait(false);
            default:
                if (name.EndsWith(".edit_at",StringComparison.Ordinal))
                {
                    var list = Reflect.Get(owner,"List") as IList ?? throw BridgeException.Missing("表单列表不可用。");
                    var index = input.GetProperty("index").GetInt32();
                    if (index >= list.Count) throw new BridgeException("VERSION_CONFLICT","表单行已变化，请重新读取。",409);
                    var type = list.GetType().GetGenericArguments().Single();
                    var value = CommandTargets.Argument(input.GetProperty("value"),type);
                    list[index] = value;
                    return new { edited = true, index, verified = Equals(list[index],value) };
                }
                if (name.EndsWith(".save",StringComparison.Ordinal))
                {
                    var method = owner.GetType().GetMethods(BindingFlags.Public|BindingFlags.NonPublic|BindingFlags.Instance).FirstOrDefault(m => m.Name == "OnSave" && m.GetParameters().Length == 0 && !m.DeclaringType!.Name.StartsWith("FormViewModel",StringComparison.Ordinal));
                    if (method is null) throw BridgeException.InvalidArgument("通用表单需要具体保存目标；先绑定实际黑／白名单表单上下文。");
                    method.Invoke(owner,null);
                    return new { saved = true, concreteType = owner.GetType().FullName, verificationRequired = "回读对应列表资源，确认保存内容。" };
                }
                throw BridgeException.NotFound("补充实现不存在。");
        }
    }

    private static async Task<object?> RunTask(CancellationToken cancellation, Func<CancellationToken,Task<object?>> action)
    {
        if (!Host.CaptureReady) throw BridgeException.GameNotReady("运行追踪前先启动截图器与游戏。");
        var runner = Activator.CreateInstance(Reflect.RequireType("BetterGenshinImpact.GameTask.TaskRunner"))!;
        object? result = null; Exception? failure = null; bool started = false;
        Func<Task> body = async () =>
        {
            started = true;
            try
            {
                var context = Reflect.Singleton("BetterGenshinImpact.Core.Script.CancellationContext")!;
                var source = Reflect.Get(context,"Cts") as CancellationTokenSource ?? throw BridgeException.Missing("宿主取消上下文不可用。");
                using var linked = CancellationTokenSource.CreateLinkedTokenSource(cancellation,source.Token);
                result = await action(linked.Token).ConfigureAwait(false);
            }
            catch (Exception error) { failure = Reflect.Root(error); throw; }
        };
        if (Reflect.Call(runner,"RunCurrentAsync",body,true,false) is not Task run) throw BridgeException.Missing("宿主任务运行器不可等待。");
        await run.ConfigureAwait(false);
        if (!started) throw new BridgeException("BUSY","宿主任务锁已占用，未启动追踪。",409);
        if (failure is not null) throw failure;
        cancellation.ThrowIfCancellationRequested();
        return result;
    }
}
