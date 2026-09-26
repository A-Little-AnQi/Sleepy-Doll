using System.Collections.ObjectModel;
using System.Collections.Specialized;
using System.ComponentModel;
using System.Dynamic;
using System.IO;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using System.Windows;
using System.Windows.Threading;
using BgiBridge.Bgi;
using BgiBridge.Catalog;
using BgiBridge.Protocol;
using BgiBridge.Tools;
using Group = BetterGenshinImpact.Core.Script.Group.ScriptGroup;
using GroupProject = BetterGenshinImpact.Core.Script.Group.ScriptGroupProject;
using Vm = BetterGenshinImpact.ViewModel.Pages.ScriptControlViewModel;

class CoverageChecks
{
    sealed class DialogFields { public string Name { get; set; } = "原值"; }
    public static readonly string Root = Path.Combine(AppContext.BaseDirectory, "User");
    public static readonly JsonSerializerOptions Json = new() { PropertyNamingPolicy = JsonNamingPolicy.CamelCase };
    private static int passed;
    static void Check(bool value, string detail) { if (!value) throw new Exception(detail); passed++; }
    static string Hash(byte[] bytes) => Convert.ToHexString(SHA256.HashData(bytes)).ToLowerInvariant();
    static async Task<JsonElement> Call(MethodRegistry registry, string id, object args)
    {
        Check(registry.TryGet(id, out var descriptor, out var handler), "未注册接口 " + id);
        var input = JsonSerializer.SerializeToElement(args);
        ArgumentSchema.Validate(input, descriptor.InputSchema);
        var result = JsonSerializer.SerializeToElement(await handler(input, CancellationToken.None));
        ArgumentSchema.Validate(result, descriptor.OutputSchema);
        return result;
    }
    static async Task Reject(Func<Task> action, string code)
    {
        try { await action(); } catch (BridgeException error) { Check(error.Code == code, error.ToString()); return; }
        throw new Exception("没有拒绝 " + code);
    }

    [STAThread]
    static void Main()
    {
        Directory.CreateDirectory(Root);
        var app = new Application { ShutdownMode = ShutdownMode.OnExplicitShutdown };
        var window = new Window { Left = -32000, Top = -32000, Width = 100, Height = 100, ShowInTaskbar = false, ShowActivated = false };
        app.MainWindow = window;
        Exception? failure = null;
        app.Startup += async (_, _) =>
        {
            try { await Run(); } catch (Exception error) { failure = error; }
            finally { app.Shutdown(); }
        };
        app.Run();
        if (failure is not null) throw failure;
        Console.WriteLine(JsonSerializer.Serialize(new { passed, realUserDataTouched = false }));
    }

    static async Task Run()
    {
        var registry = new MethodRegistry();
        ScriptGroupDeletionTools.Register(registry);
        TaskStopTools.Register(registry);
        NavigationTools.Register(registry);
        JavaScriptPreparationTools.Register(registry);
        var vm = BetterGenshinImpact.App.Services.Groups;
        Directory.CreateDirectory(vm.ScriptGroupPath);
        var other = new Group { Name = "保留组" }; vm.ScriptGroups.Add(other); vm.SelectedScriptGroup = other;
        var otherFile = Path.Combine(vm.ScriptGroupPath, other.Name + ".json");
        File.WriteAllText(otherFile, "{\"name\":\"保留组\",\"futureField\":123}");
        var otherBytes = File.ReadAllBytes(otherFile);
        var routes = Path.Combine(Root, "AutoPathing", "路线.json"); Directory.CreateDirectory(Path.GetDirectoryName(routes)!); File.WriteAllText(routes, "route bytes");
        var routeBytes = File.ReadAllBytes(routes);
        var group = new Group { Name = "删除目标" }; vm.ScriptGroups.Add(group);
        var file = Path.Combine(vm.ScriptGroupPath, group.Name + ".json");
        File.WriteAllText(otherFile, Encoding.UTF8.GetString(otherBytes));
        var bytes = File.ReadAllBytes(file);
        var removed = await Call(registry, "bgi.delete_script_group", new { groupName = group.Name, expectedSha256 = Hash(bytes) });
        Check(removed.GetProperty("deleted").GetBoolean() && !File.Exists(file), "目标没有删除");
        Check(File.ReadAllBytes(Path.Combine(Root, removed.GetProperty("backup").GetString()!)).SequenceEqual(bytes), "备份不是原始字节");
        Check(File.ReadAllBytes(otherFile).SequenceEqual(otherBytes) && File.ReadAllBytes(routes).SequenceEqual(routeBytes), "其他组或路线被改变");
        Check(vm.SelectedScriptGroup == other && vm.ScriptGroups.Count == 1, "依赖了当前选择");
        vm.Reset(new Group { Name = "版本冲突" }); file = Path.Combine(vm.ScriptGroupPath, "版本冲突.json");
        await Reject(() => Call(registry, "bgi.delete_script_group", new { groupName = "版本冲突", expectedSha256 = new string('0', 64) }), "VERSION_CONFLICT");
        Check(File.Exists(file), "版本冲突仍删除");
        var current = File.ReadAllBytes(file); vm.FailDelete = true;
        await Reject(() => Call(registry, "bgi.delete_script_group", new { groupName = "版本冲突", expectedSha256 = Hash(current) }), "EXECUTION_FAILED");
        Check(File.Exists(file) && vm.ScriptGroups.Count == 1, "原生删除失败未恢复列表"); vm.FailDelete = false;
        var saveCount = vm.SaveCount; vm.ScriptGroups.Add(new Group { Name = "回调仍有效" }); Check(vm.SaveCount > saveCount, "保存回调未恢复");
        vm.Reset(new Group { Name = "同名目标" }); file = Path.Combine(vm.ScriptGroupPath, "同名目标.json");
        File.WriteAllText(Path.Combine(vm.ScriptGroupPath, "重复.json"), File.ReadAllText(file));
        await Reject(() => Call(registry, "bgi.delete_script_group", new { groupName = "同名目标", expectedSha256 = Hash(File.ReadAllBytes(file)) }), "AMBIGUOUS_TARGET");
        File.Delete(Path.Combine(vm.ScriptGroupPath, "重复.json"));
        await BetterGenshinImpact.GameTask.Common.TaskControl.TaskSemaphore.WaitAsync();
        await Reject(() => Call(registry, "bgi.delete_script_group", new { groupName = "同名目标", expectedSha256 = Hash(File.ReadAllBytes(file)) }), "BUSY");
        BetterGenshinImpact.GameTask.Common.TaskControl.TaskSemaphore.Release();
        var navigation = await Call(registry, "bgi.open_page", new { page = "scheduler" });
        Check(navigation.GetProperty("verified").GetBoolean(), "导航未核验"); Application.Current.MainWindow.Hide();
        await Reject(() => Call(registry, "bgi.open_page", new { page = "bad-page" }), "INVALID_ARGUMENT");
        var dict = ValueContract.Schema(typeof(Dictionary<string, bool>))!.Value;
        ArgumentSchema.Validate(JsonSerializer.SerializeToElement(new Dictionary<string, bool> { ["fps"] = true }), dict);
        await Reject(() => { ArgumentSchema.Validate(JsonSerializer.SerializeToElement(new { fps = "true" }), dict); return Task.CompletedTask; }, "INVALID_ARGUMENT");
        await Reject(() => { ArgumentSchema.Validate(JsonSerializer.SerializeToElement(Enumerable.Range(0, 1001).ToDictionary(i => i.ToString(), _ => true)), dict); return Task.CompletedTask; }, "INVALID_ARGUMENT");
        await CancellationChecks();
        await SettingsChecks();
        await JavaScriptChecks(registry);
        await ReflectionChecks();
        await ImplementedChecks();
        foreach (var (path, boundary) in SettingMutationAdapters.Bounded)
        {
            var schema = ValueContract.Schema(typeof(double), path:path)!.Value;
            ArgumentSchema.Validate(JsonSerializer.SerializeToElement(boundary.Min), schema);
            ArgumentSchema.Validate(JsonSerializer.SerializeToElement(boundary.Max), schema);
            await Reject(() => { ArgumentSchema.Validate(JsonSerializer.SerializeToElement(boundary.Min-1), schema); return Task.CompletedTask; }, "INVALID_ARGUMENT");
        }
        foreach(var path in SettingMutationAdapters.Colors)
        {
            var schema = ValueContract.Schema(typeof(string), path:path)!.Value;
            ArgumentSchema.Validate(JsonSerializer.SerializeToElement("#DA4A23FF"), schema);
            await Reject(() => { ArgumentSchema.Validate(JsonSerializer.SerializeToElement("invalid"), schema); return Task.CompletedTask; }, "INVALID_ARGUMENT");
        }
        var bound = ScriptGroupTools.BindGroupsFromDisk(vm.ScriptGroupPath, "同名目标");
        Check(bound.Count == 1 && ((Group)bound[0]!).Name == "同名目标", "执行没有绑定磁盘目标");
        var dragon = new OneDragonFixture();
        OneDragonTools.BindConfiguration(dragon, "B");
        Check(dragon.SelectedConfig?.Name == "B" && dragon.TasksFrom == "B", "一条龙选择后仍使用旧任务");
    }

    static async Task CancellationChecks()
    {
        var context = BetterGenshinImpact.Core.Script.CancellationContext.Instance;
        context.Set();
        using var token = new CancellationTokenSource();
        using (var link = new HostTaskCancellation(token.Token))
        {
            token.Cancel();
            await BetterGenshinImpact.GameTask.Common.TaskControl.TaskSemaphore.WaitAsync();
            await Task.Delay(150);
            Check(context.Cts.IsCancellationRequested && context.IsManualStop, "取消没有传递给宿主");
            BetterGenshinImpact.GameTask.Common.TaskControl.TaskSemaphore.Release();
            try { await link.CompleteAsync(); } catch (OperationCanceledException) { }
        }
        context.Set(); await Task.Delay(100); Check(!context.Cts.IsCancellationRequested, "旧取消影响后续任务");
        using var lateToken = new CancellationTokenSource();
        using (var link = new HostTaskCancellation(lateToken.Token)) { await link.CompleteAsync(); }
        lateToken.Cancel(); await Task.Delay(100); Check(!context.Cts.IsCancellationRequested, "已结束作用域仍取消宿主");
        var registry = new MethodRegistry(); TaskStopTools.Register(registry);
        Check((await Call(registry, "bgi.stop_current_task", new { })).GetProperty("stopped").GetBoolean(), "空闲停止结果错误");
    }

    static Task SettingsChecks()
    {
        var config = new BetterGenshinImpact.Core.Config.AllConfig();
        var file = Path.Combine(Root, "config-fixture.json"); File.WriteAllText(file, JsonSerializer.Serialize(config, Json));
        var records = Path.Combine(Root, "records"); Directory.CreateDirectory(records);
        var engine = new SettingsTransactionEngine(() => config, () => file, () => Json, records);
        var entries = SettingsCatalog.Build(config);
        var count = entries.Single(item => item.Path == "autoBossConfig.runCount"); Check(count.Writable, "次数钩子仍不可写");
        var preview = JsonSerializer.SerializeToElement(engine.Preview([new(count.Path, JsonSerializer.SerializeToElement(2), count.ValueVersion)]));
        var committed = JsonSerializer.SerializeToElement(engine.Commit(preview.GetProperty("planId").GetString()!));
        Check(config.AutoBossConfig.RunCount == 2, "次数未修改");
        engine.Rollback(committed.GetProperty("changeId").GetString()!); Check(config.AutoBossConfig.RunCount == 1, "次数不能回退");
        count = SettingsCatalog.Build(config).Single(item => item.Path == "autoBossConfig.runCount");
        var mode = SettingsCatalog.Build(config).Single(item => item.Path == "autoBossConfig.specifyRunCount");
        var modePreview = JsonSerializer.SerializeToElement(engine.Preview([new(mode.Path, JsonSerializer.SerializeToElement(true), mode.ValueVersion)]));
        Check(modePreview.GetProperty("differences").GetArrayLength() == 3, "联动范围没有纳入预览");
        var modeChange = JsonSerializer.SerializeToElement(engine.Commit(modePreview.GetProperty("planId").GetString()!));
        Check(config.AutoBossConfig.SpecifyRunCount, "指定次数模式仍不可修改");
        engine.Rollback(modeChange.GetProperty("changeId").GetString()!); Check(!config.AutoBossConfig.SpecifyRunCount, "联动模式不能回退");
        config.AutoBossConfig.SpecifyRunCount=true; config.AutoBossConfig.UseFragileResin=true;
        mode=SettingsCatalog.Build(config).Single(item => item.Path=="autoBossConfig.specifyRunCount");
        modePreview=JsonSerializer.SerializeToElement(engine.Preview([new(mode.Path,JsonSerializer.SerializeToElement(false),mode.ValueVersion)]));
        modeChange=JsonSerializer.SerializeToElement(engine.Commit(modePreview.GetProperty("planId").GetString()!));
        Check(!config.AutoBossConfig.SpecifyRunCount && !config.AutoBossConfig.UseFragileResin,"模式关闭未联动清除树脂开关");
        engine.Rollback(modeChange.GetProperty("changeId").GetString()!); Check(config.AutoBossConfig.SpecifyRunCount && config.AutoBossConfig.UseFragileResin,"联动字段回退丢失原值");
        return Reject(() => { engine.Preview([new(count.Path, JsonSerializer.SerializeToElement(0), count.ValueVersion)]); return Task.CompletedTask; }, "INVALID_ARGUMENT");
    }

    static async Task JavaScriptChecks(MethodRegistry registry)
    {
        var folder = Path.Combine(Root, "JsScript", "本机JS"); Directory.CreateDirectory(folder);
        File.WriteAllText(Path.Combine(folder, "main.js"), "log('fixture');");
        File.WriteAllText(Path.Combine(folder, "manifest.json"), "{\"name\":\"只在本机的脚本\",\"main\":\"main.js\",\"settings_ui\":\"options.json\"}");
        File.WriteAllText(Path.Combine(folder, "options.json"), "[{\"name\":\"team\",\"type\":\"input-text\",\"default\":\"默认队伍\"},{\"name\":\"mode\",\"type\":\"select\",\"options\":[\"A\",\"B\"],\"default\":\"A\"}]");
        var result = await Call(registry, "bgi.prepare_js_group", new { folderName = "本机JS", settings = new { team = "采集" } });
        var file = Path.Combine(Root, "ScriptGroup", result.GetProperty("groupName").GetString() + ".json");
        using var saved = JsonDocument.Parse(File.ReadAllText(file));
        Check(saved.RootElement.GetProperty("projects")[0].GetProperty("jsScriptSettingsObject").GetProperty("team").GetString() == "采集", "JS参数未保存");
        Check((await Call(registry, "bgi.prepare_js_group", new { folderName = "本机JS", settings = new { team = "采集" } })).GetProperty("reused").GetBoolean(), "重复准备不复用");
        await Reject(() => Call(registry, "bgi.prepare_js_group", new { folderName = "本机JS", settings = new { bad = true } }), "INVALID_ARGUMENT");
        await Reject(() => Call(registry, "bgi.prepare_js_group", new { folderName = "本机JS", settings = new { mode = "C" } }), "INVALID_ARGUMENT");
    }

    static async Task ReflectionChecks()
    {
        var registry = new MethodRegistry(); CommandTargetTools.Register(registry);
        CommandCatalog.Configure(new BgiBridge.BridgeConfig { });
        var config = new BetterGenshinImpact.Core.Config.AllConfig();
        var file = Path.Combine(Root, "reflection-config.json"); File.WriteAllText(file, JsonSerializer.Serialize(config, Json));
        Directory.CreateDirectory(Path.Combine(Root,"reflection-records"));
        typeof(SettingsTransactions).GetProperty("Engine")!.SetValue(null, new SettingsTransactionEngine(() => config, () => file, () => Json, Path.Combine(Root,"reflection-records")));
        Check(CommandCatalog.All.Any(c => c.Name == "reflection_fixture.apply" && c.ParameterSchema.HasValue && c.UnavailableReason is null), "未注册且无 IViewModel 的命令仍被过滤");
        var target = await Call(registry, "bgi.create_command_target", new { command = "reflection_fixture.apply", arguments = new { name = "原生记录" } });
        var context = target.GetProperty("target").GetProperty("objectId").GetString()!;
        var list = await Call(registry, "bgi.list_command_targets", new { command = "reflection_fixture.apply", query = "原生记录" });
        var argument = list.GetProperty("arguments")[0].GetProperty("objectId").GetString()!;
        var native = (BetterGenshinImpact.ViewModel.ReflectionFixtureViewModel)CommandTargets.Resolve(context, typeof(BetterGenshinImpact.ViewModel.ReflectionFixtureViewModel));
        await CommandCatalog.Invoke("reflection_fixture.apply", JsonSerializer.SerializeToElement(new { objectId = argument }), CancellationToken.None, context,
            JsonSerializer.SerializeToElement(new { SelectedItem = new { objectId = argument } }));
        Check(ReferenceEquals(native.Applied, native.Items[0]) && ReferenceEquals(native.SelectedItem, native.Items[0]), "对象参数被伪造／未绑定目标选择");
        await Reject(() => CommandCatalog.Invoke("reflection_fixture.apply", JsonSerializer.SerializeToElement(new { objectId = "forged" }), CancellationToken.None, context), "STALE_TARGET");
        native.Items.Clear(); native.SelectedItem = null; native.Applied = null;
        await Reject(() => CommandCatalog.Invoke("reflection_fixture.apply", JsonSerializer.SerializeToElement(new { objectId = argument }), CancellationToken.None, context), "STALE_TARGET");
        await CommandCatalog.Invoke("reflection_fixture.prompt", null, CancellationToken.None, context, null, JsonSerializer.SerializeToElement(new { text = "实际弹窗输入", confirm = true }));
        Check(native.Input == "实际弹窗输入", "弹窗填写没有进入原生处理器");
        var fields = new DialogFields();
        using (var scope = new NativeDialogScope(JsonSerializer.SerializeToElement(new { values = new { Name = "不应保存", Missing = "无效" } }), [], CancellationToken.None))
        {
            new BetterGenshinImpact.ViewModel.PromptDialog { DataContext = fields }.ShowDialog();
            await Reject(() => Ui.InvokeAsync(scope.Verify), "INVALID_ARGUMENT");
            Check(fields.Name == "原值", "后续字段无效却已修改前一个字段");
        }
        using (var scope = new NativeDialogScope(JsonSerializer.SerializeToElement(new { selectedValues = new[] { "不存在的选择" } }), [], CancellationToken.None))
        {
            new BetterGenshinImpact.ViewModel.PromptDialog().ShowDialog();
            await Reject(() => Ui.InvokeAsync(scope.Verify), "INVALID_ARGUMENT");
            Check(scope.Handled == 0, "选择完全不匹配仍确认了弹窗");
        }
        await Call(registry, "bgi.release_command_target", new { objectId = context });
        await Reject(() => Ui.InvokeAsync(() => CommandTargets.Resolve(context, typeof(BetterGenshinImpact.ViewModel.ReflectionFixtureViewModel))), "STALE_TARGET");
        Check(BetterGenshinImpact.ViewModel.ReflectionFixtureViewModel.Disposed, "桥创建的上下文未释放");
    }

    static async Task ImplementedChecks()
    {
        var config = BetterGenshinImpact.Service.ConfigService.Config;
        var owner = new CompletionFixture(config);
        var empty = ArgumentSchema.Parse("{}");
        config.MaskWindowConfig.MaskEnabled = true;
        var mask = JsonSerializer.SerializeToElement(await ImplementedCommands.Run("common_settings_page.switch_mask_enabled",owner,empty,CancellationToken.None));
        Check(mask.GetProperty("verified").GetBoolean() && BetterGenshinImpact.View.MaskWindow.Instance().IsVisible, "遮罩未显示");
        config.MaskWindowConfig.MaskEnabled = false;
        await ImplementedCommands.Run("common_settings_page.switch_mask_enabled",owner,empty,CancellationToken.None);
        Check(!BetterGenshinImpact.View.MaskWindow.Instance().IsVisible, "遮罩未隐藏");
        config.CommonConfig.ScreenshotEnabled = true;
        var screenshot = JsonSerializer.SerializeToElement(await ImplementedCommands.Run("common_settings_page.switch_taken_screenshot_enabled",owner,empty,CancellationToken.None));
        Check(screenshot.GetProperty("enabled").GetBoolean() && BetterGenshinImpact.Service.ConfigService.Saved, "截图开关未保存");
        var capture = JsonSerializer.SerializeToElement(await ImplementedCommands.Run("home_page.test",owner,empty,CancellationToken.None));
        Check(capture.GetProperty("verified").GetBoolean() && BetterGenshinImpact.View.CaptureTestWindow.Current?.Captured == true,"截图测试没有启动");
        BetterGenshinImpact.View.CaptureTestWindow.Current!.Close();
        var map = JsonSerializer.SerializeToElement(await ImplementedCommands.Run("map_pathing_dev.drop_down_changed",owner,empty,CancellationToken.None));
        Check(map.GetProperty("mapName").GetString() == "Teyvat" && map.GetProperty("verified").GetBoolean(),"录制地图未应用");
        await ImplementedCommands.Run("form.edit_at",owner,JsonSerializer.SerializeToElement(new { index=0,value="新条目" }),CancellationToken.None);
        Check(owner.List[0] == "新条目", "表单编辑没有写入元素");
        await ImplementedCommands.Run("form.save",owner,empty,CancellationToken.None);
        Check(owner.Saved, "表单保存未执行具体类型方法");
        Check(!CommandCatalog.All.Any(c => c.Name == "task_settings_page.switch_auto_track"), "剧情跟踪仍对外开放");
        var path = Path.Combine(Root,"AutoPathing","适配路线.json"); File.WriteAllText(path,"{\"positions\":[{\"id\":1}]}");
        var route = JsonSerializer.SerializeToElement(await ImplementedCommands.Run("task_settings_page.switch_auto_track_path",owner,JsonSerializer.SerializeToElement(new { path="适配路线.json" }),CancellationToken.None));
        Check(route.GetProperty("verified").GetBoolean(), "路线跟踪没有核验 SuccessEnd");
        Check(BetterGenshinImpact.GameTask.Common.TaskControl.TaskSemaphore.CurrentCount==1,"任务结束未释放宿主锁");
    }
}
public class CompletionFixture(BetterGenshinImpact.Core.Config.AllConfig config)
{
    public BetterGenshinImpact.Core.Config.AllConfig Config { get; } = config;
    public BetterGenshinImpact.Core.Config.DevConfig DevConfig => Config.DevConfig;
    public object[] MapTypeItems { get; } = [new { EnumName="Teyvat" }];
    public ObservableCollection<string> List { get; } = ["旧条目"]; public bool Saved;
    public void OnSave() => Saved=true;
}

public class OneDragonFixture
{
    public List<DragonConfig> ConfigList { get; } = []; public DragonConfig? SelectedConfig { get; set; }
    public string? TasksFrom { get; private set; }
    private void InitConfigList() { ConfigList.Clear(); ConfigList.AddRange([new("A"), new("B")]); SelectedConfig = ConfigList[0]; TasksFrom = "A"; }
    public void SetSomeSelectedConfig(DragonConfig config) { TasksFrom = config.Name; }
}
public record DragonConfig(string Name);

namespace BetterGenshinImpact
{
    public static class App { public static Services Services { get; } = new(); public static IServiceProvider ServiceProvider => Services; }
    public class Services : IServiceProvider
    {
        public Vm Groups { get; } = new(); public Wpf.Ui.NavigationService Navigation { get; } = new();
        public object? GetService(Type type) => type == typeof(Vm) ? Groups : type == typeof(Wpf.Ui.INavigationService) ? Navigation : type==typeof(BetterGenshinImpact.Service.Interface.IConfigService) ? new BetterGenshinImpact.Service.ConfigService() : null;
    }
}
namespace BetterGenshinImpact.GameTask.Common { public static class TaskControl { public static SemaphoreSlim TaskSemaphore { get; } = new(1,1); } }
namespace BetterGenshinImpact.Core.Script
{
    public class CancellationContext
    {
        public static CancellationContext Instance { get; } = new(); public CancellationTokenSource Cts { get; private set; } = new(); public bool IsManualStop { get; private set; }
        public void Set() { Cts = new(); IsManualStop = false; } public void ManualCancel() { IsManualStop = true; Cts.Cancel(); }
    }
}
namespace BetterGenshinImpact.ViewModel.Pages
{
    public class ScriptControlViewModel
    {
        public string ScriptGroupPath { get; } = Path.Combine(CoverageChecks.Root,"ScriptGroup");
        public ObservableCollection<Group> ScriptGroups { get; } = []; public Group? SelectedScriptGroup { get; set; }
        public bool FailDelete { get; set; } public int SaveCount { get; private set; }
        public ScriptControlViewModel() { ScriptGroups.CollectionChanged += ScriptGroupsCollectionChanged; }
        private void ScriptGroupsCollectionChanged(object? sender, NotifyCollectionChangedEventArgs args)
        { SaveCount++; foreach(var group in ScriptGroups)group.WriteToFileAtomically(Path.Combine(ScriptGroupPath,group.Name+".json")); }
        private void ScriptProjectsCollectionChanged(object? sender,NotifyCollectionChangedEventArgs args) { }
        private void ScriptProjectsPChanged(object? sender,PropertyChangedEventArgs args) { }
        public void OnDeleteScriptGroup(Group group) { ScriptGroups.Remove(group); if(!FailDelete)File.Delete(Path.Combine(ScriptGroupPath,group.Name+".json")); }
        public void Reset(Group group) { ScriptGroups.Clear(); ScriptGroups.Add(group); }
    }
}
namespace BetterGenshinImpact.ViewModel
{
    public sealed record ReflectionItem(string Name);
    public interface IRelayCommand<T> : System.Windows.Input.ICommand { }
    public sealed class FixtureCommand<T>(Action<T> action) : IRelayCommand<T>
    {
        public event EventHandler? CanExecuteChanged { add { } remove { } }
        public bool CanExecute(object? p) => true;
        public void Execute(object? p) => action((T)p!);
    }
    public sealed class FixturePlainCommand(Action action) : System.Windows.Input.ICommand
    {
        public event EventHandler? CanExecuteChanged { add { } remove { } }
        public bool CanExecute(object? p) => true;
        public void Execute(object? p) => action();
    }
    public sealed class ReflectionFixtureViewModel : IDisposable
    {
        public ObservableCollection<ReflectionItem> Items { get; } = [];
        public ReflectionItem? SelectedItem { get; set; }
        public ReflectionItem? Applied;
        public string? Input;
        public static bool Disposed;
        public IRelayCommand<ReflectionItem> ApplyCommand { get; }
        public System.Windows.Input.ICommand PromptCommand { get; }
        public ReflectionFixtureViewModel(string name)
        {
            Items.Add(new(name)); ApplyCommand = new FixtureCommand<ReflectionItem>(item => Applied = item);
            PromptCommand = new FixturePlainCommand(() => { var window = new PromptDialog(); window.ShowDialog(); Input = window.Box.Text; });
        }
        public void Dispose() => Disposed = true;
    }
    public sealed class PromptDialog : Window
    {
        public System.Windows.Controls.TextBox Box { get; } = new();
        public PromptDialog()
        {
            Left = -32000; Top = -32000; Width = 200; Height = 100; Opacity = 0; ShowInTaskbar = false; ShowActivated = false;
            var panel = new System.Windows.Controls.StackPanel(); panel.Children.Add(Box);
            var button = new System.Windows.Controls.Button { Content = "确定" }; button.Click += (_, _) => DialogResult = true; panel.Children.Add(button); Content = panel;
        }
    }
}
namespace BetterGenshinImpact.Core.Script.Group
{
    public class ScriptGroup
    {
        public string Name { get; set; } = ""; public ObservableCollection<ScriptGroupProject> Projects { get; set; } = [];
        public static ScriptGroup FromJson(string json) => JsonSerializer.Deserialize<ScriptGroup>(json,new JsonSerializerOptions { PropertyNameCaseInsensitive = true })!;
        public void WriteToFileAtomically(string file) { Directory.CreateDirectory(Path.GetDirectoryName(file)!); File.WriteAllText(file,JsonSerializer.Serialize(this,CoverageChecks.Json)); }
    }
    public class ScriptGroupProject : INotifyPropertyChanged
    {
        public event PropertyChangedEventHandler? PropertyChanged { add { } remove { } }
        public string Name { get; set; } = ""; public string FolderName { get; set; } = ""; public string Type { get; set; } = "Javascript";
        public string Status { get; set; } = "Enabled"; public int Index { get; set; } public ExpandoObject? JsScriptSettingsObject { get; set; }
        public ScriptGroupProject() { } public ScriptGroupProject(Project.ScriptProject project) { Name = project.Manifest.Name; FolderName = project.FolderName; }
    }
}
namespace BetterGenshinImpact.Core.Script.Project
{
    public class ScriptProject
    {
        public string FolderName { get; } public Manifest Manifest { get; }
        public ScriptProject(string folder) { FolderName=folder; Manifest=JsonSerializer.Deserialize<Manifest>(File.ReadAllText(Path.Combine(CoverageChecks.Root,"JsScript",folder,"manifest.json")),new JsonSerializerOptions { PropertyNamingPolicy=JsonNamingPolicy.SnakeCaseLower })!; }
    }
    public class Manifest
    {
        public string Name { get; set; } = ""; public string SettingsUi { get; set; } = "";
        public List<SettingItem> LoadSettingItems(string folder) => JsonSerializer.Deserialize<List<SettingItem>>(File.ReadAllText(Path.Combine(folder,SettingsUi)),new JsonSerializerOptions { PropertyNameCaseInsensitive=true })!;
    }
    public class SettingItem { public string Name { get; set; } = ""; public string Type { get; set; } = ""; public List<string>? Options { get; set; } public object? Default { get; set; } }
}
namespace BetterGenshinImpact.Core.Config
{
    public class AllConfig { public Action? OnAnyChangedAction { get; set; } public GameTask.AutoBoss.AutoBossConfig AutoBossConfig { get; set; } = new(); public MaskConfig MaskWindowConfig {get;set;}=new(); public CommonConfig CommonConfig {get;set;}=new(); public DevConfig DevConfig {get;set;}=new(); public string CaptureMode {get;set;}="BitBlt"; }
    public class MaskConfig { public bool MaskEnabled {get;set;} }
    public class CommonConfig { public bool ScreenshotEnabled {get;set;} }
    public class DevConfig { public string RecordMapName {get;set;}="Teyvat"; }
}
namespace BetterGenshinImpact.Service.Interface { public interface IConfigService { void Save(); } }
namespace BetterGenshinImpact.Service { public class ConfigService : Interface.IConfigService { public static Core.Config.AllConfig Config {get;}=new(); public static bool Saved; public void Save()=>Saved=true; } }
namespace BetterGenshinImpact.GameTask
{
    public class TaskContext { public static TaskContext Instance()=>Current; public static TaskContext Current {get;}=new(); public bool IsInitialized {get;set;}=true; public IntPtr GameHandle {get;}=(IntPtr)1; }
    public class TaskRunner
    {
        public async Task RunCurrentAsync(Func<Task> action,bool reset,bool clear)
        {
            if(!await Common.TaskControl.TaskSemaphore.WaitAsync(0))return;
            try { Core.Script.CancellationContext.Instance.Set(); await action(); } catch { }
            finally { Common.TaskControl.TaskSemaphore.Release(); }
        }
    }
}
namespace BetterGenshinImpact.GameTask.AutoSkip.Model { public class AutoTrackParam { } }
namespace BetterGenshinImpact.GameTask.AutoSkip
{
    public class AutoTrackTask { private CancellationToken _ct=CancellationToken.None; public static bool Called; public AutoTrackTask(Model.AutoTrackParam parameter){ ArgumentNullException.ThrowIfNull(parameter); } private void TrackMission() { _ct.ThrowIfCancellationRequested(); Called=true; } }
}
namespace BetterGenshinImpact.GameTask.AutoPathing.Model { public class PathingTask { public static PathingTask BuildFromJson(string json) { JsonDocument.Parse(json); return new(); } } }
namespace BetterGenshinImpact.GameTask.AutoPathing { public class PathExecutor(CancellationToken token) { public bool SuccessEnd; public Task Pathing(Model.PathingTask task) { token.ThrowIfCancellationRequested(); SuccessEnd=true; return Task.CompletedTask; } } }
namespace Fischless.GameCapture { public static class CaptureModeExtensions { public static string ToCaptureMode(string mode)=>mode; } }
namespace BetterGenshinImpact.View
{
    public class MaskWindow : Window { private static readonly MaskWindow Current=new(); public static MaskWindow Instance()=>Current; public MaskWindow(){ Left=-32000;Top=-32000;Width=100;Height=100;Opacity=0;ShowInTaskbar=false;ShowActivated=false; } }
    public class CaptureTestWindow : Window { public static CaptureTestWindow? Current; public bool Captured; public CaptureTestWindow(){ Current=this;Left=-32000;Top=-32000;Width=100;Height=100;Opacity=0;ShowInTaskbar=false;ShowActivated=false; } public void StartCapture(IntPtr handle,string mode)=>Captured=handle!=IntPtr.Zero; }
}
namespace BetterGenshinImpact.GameTask.AutoBoss
{
    public class AutoBossConfig
    {
        private int count=1; public int RunCount { get=>count; set=>count=Math.Max(1,value); }
        public int ReviveRetryCount { get; set; } = 3;
        private bool specified; public bool SpecifyRunCount { get=>specified; set { specified=value; if(!value) { UseTransientResin=false; UseFragileResin=false; } } }
        public bool UseTransientResin { get; set; } public bool UseFragileResin { get; set; }
    }
}
namespace BetterGenshinImpact.View.Pages { public class ScriptControlPage { } }
namespace Wpf.Ui
{
    public interface INavigationService { }
    public class NavigationService : INavigationService
    {
        public Control Control { get; } = new(); public bool Navigate(Type type) { Control.SelectedItem=new Item { TargetPageType=type }; return true; } public Control GetNavigationControl()=>Control;
    }
    public class Control { public Item? SelectedItem { get; set; } } public class Item { public Type? TargetPageType { get; set; } }
}
