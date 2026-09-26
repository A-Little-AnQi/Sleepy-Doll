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
        File.WriteAllText(Path.Combine(folder, "manifest.json"), "{\"name\":\"只在本机的脚本\",\"main\":\"main.js\",\"settingsUi\":\"options.json\"}");
        File.WriteAllText(Path.Combine(folder, "options.json"), "[{\"name\":\"team\",\"type\":\"input-text\",\"default\":\"默认队伍\"},{\"name\":\"mode\",\"type\":\"select\",\"options\":[\"A\",\"B\"],\"default\":\"A\"}]");
        var result = await Call(registry, "bgi.prepare_js_group", new { folderName = "本机JS", settings = new { team = "采集" } });
        var file = Path.Combine(Root, "ScriptGroup", result.GetProperty("groupName").GetString() + ".json");
        using var saved = JsonDocument.Parse(File.ReadAllText(file));
        Check(saved.RootElement.GetProperty("projects")[0].GetProperty("jsScriptSettingsObject").GetProperty("team").GetString() == "采集", "JS参数未保存");
        Check((await Call(registry, "bgi.prepare_js_group", new { folderName = "本机JS", settings = new { team = "采集" } })).GetProperty("reused").GetBoolean(), "重复准备不复用");
        await Reject(() => Call(registry, "bgi.prepare_js_group", new { folderName = "本机JS", settings = new { bad = true } }), "INVALID_ARGUMENT");
        await Reject(() => Call(registry, "bgi.prepare_js_group", new { folderName = "本机JS", settings = new { mode = "C" } }), "INVALID_ARGUMENT");
    }
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
        public object? GetService(Type type) => type == typeof(Vm) ? Groups : type == typeof(Wpf.Ui.INavigationService) ? Navigation : null;
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
        public ScriptProject(string folder) { FolderName=folder; Manifest=JsonSerializer.Deserialize<Manifest>(File.ReadAllText(Path.Combine(CoverageChecks.Root,"JsScript",folder,"manifest.json")),new JsonSerializerOptions { PropertyNameCaseInsensitive=true })!; }
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
    public class AllConfig { public Action? OnAnyChangedAction { get; set; } public GameTask.AutoBoss.AutoBossConfig AutoBossConfig { get; set; } = new(); }
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
