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
        if (failure is not null) System.Runtime.ExceptionServices.ExceptionDispatchInfo.Capture(failure).Throw();
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
        await NativeSurfaceChecks();
        await LocalDeletionChecks();

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
        Check(!entries.Any(item => item.Path.EndsWith("legacyFlag", StringComparison.Ordinal)), "弃用设置仍被公开");
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

    static async Task LocalDeletionChecks()
    {
        var registry=new MethodRegistry();LocalResourceDeletionTools.Register(registry);
        var oldReference=Path.Combine(Root,"ScriptGroup/其他组引用血斛.json");if(File.Exists(oldReference))File.Delete(oldReference);
        var path="AutoPathing/地方特产/稻妻/血斛/血斛@固定作者包";var target=Path.Combine(Root,path.Replace('/',Path.DirectorySeparatorChar));Directory.CreateDirectory(target);
        for(var i=1;i<=5;i++)File.WriteAllText(Path.Combine(target,$"路线{i}.json"),"{\"positions\":[{\"id\":"+i+"}]}");
        Directory.CreateDirectory(Path.Combine(target,"空目录"));
        var other=Path.Combine(Root,"AutoPathing/其他材料.json");File.WriteAllText(other,"keep-other-route");var otherBytes=File.ReadAllBytes(other);
        var subscription=Path.Combine(Root,"Subscriptions/bettergi-scripts-list.json");Directory.CreateDirectory(Path.GetDirectoryName(subscription)!);File.WriteAllText(subscription,"[\"pathing/地方特产/稻妻/血斛\",\"pathing/其他材料\"]");var subscriptionBytes=File.ReadAllBytes(subscription);
        var inspected=await Call(registry,"bgi.inspect_local_resource",new{path});Check(inspected.GetProperty("totalFiles").GetInt32()==5,"完整作者包范围未定位");
        var version=inspected.GetProperty("version").GetString()!;
        await Reject(()=>Call(registry,"bgi.delete_local_resource",new{path,expectedVersion=new string('0',64)}),"VERSION_CONFLICT");Check(Directory.Exists(target),"冲突仍移动了资源");
        var deleted=await Call(registry,"bgi.delete_local_resource",new{path,expectedVersion=version});var result=deleted.GetProperty("result");
        Check(result.GetProperty("verified").GetBoolean()&&!Directory.Exists(target)&&result.GetProperty("deletedFiles").GetInt32()==5,"路径删除没有完成");
        Check(File.ReadAllBytes(other).SequenceEqual(otherBytes)&&File.ReadAllBytes(subscription).SequenceEqual(subscriptionBytes),"删除影响其他路线或订阅");
        var backupId=result.GetProperty("backupId").GetString()!;var restored=await Call(registry,"bgi.restore_local_resource",new{backupId});Check(restored.GetProperty("result").GetProperty("verified").GetBoolean()&&Directory.Exists(Path.Combine(target,"空目录")),"未完整恢复目录与文件");
        await Reject(()=>Call(registry,"bgi.restore_local_resource",new{backupId}),"RESOURCE_CONFLICT");
        var reference=Path.Combine(Root,"ScriptGroup/其他组引用血斛.json");File.WriteAllText(reference,JsonSerializer.Serialize(new{name="其他组引用血斛",projects=new[]{new{name="路线1.json",type="Pathing",folderName="地方特产/稻妻/血斛/血斛@固定作者包"}}}));
        inspected=await Call(registry,"bgi.inspect_local_resource",new{path});Check(inspected.GetProperty("references").GetArrayLength()==1,"引用范围未被检查");
        await Reject(()=>Call(registry,"bgi.delete_local_resource",new{path,expectedVersion=inspected.GetProperty("version").GetString()}),"RESOURCE_IN_USE");
        await Reject(()=>Call(registry,"bgi.inspect_local_resource",new{path="AutoPathing/../ScriptGroup"}),"INVALID_ARGUMENT");
        await Reject(()=>Call(registry,"bgi.inspect_local_resource",new{path="AutoPathing"}),"INVALID_ARGUMENT");
        File.Delete(reference);
    }

    static async Task ReflectionChecks()
    {
        var registry = new MethodRegistry(); CommandTargetTools.Register(registry);
        CommandCatalog.Configure(new BgiBridge.BridgeConfig { });
        var config = new BetterGenshinImpact.Core.Config.AllConfig();
        var file = Path.Combine(Root, "reflection-config.json"); File.WriteAllText(file, JsonSerializer.Serialize(config, Json));
        Directory.CreateDirectory(Path.Combine(Root,"reflection-records"));
        typeof(SettingsTransactions).GetProperty("Engine")!.SetValue(null, new SettingsTransactionEngine(() => config, () => file, () => Json, Path.Combine(Root,"reflection-records")));
        var sourceEntries = (Dictionary<string, SourceEntry>)typeof(SourceDocumentation).GetField("Entries", System.Reflection.BindingFlags.Static | System.Reflection.BindingFlags.NonPublic)!.GetValue(null)!;
        var fixtureOwner = typeof(BetterGenshinImpact.ViewModel.ReflectionFixtureViewModel).FullName!;
        foreach (var member in new[] { "ApplyCommand", "PromptCommand" })
            sourceEntries["C:" + fixtureOwner + "." + member] = new SourceEntry("隔离检查原生动作", null, "command", null, null, null, "CoverageChecks.cs", 1, false);
        sourceEntries["C:" + fixtureOwner + ".RetiredCommand"] = new SourceEntry("隔离检查空入口", null, "command", null, null, null, "CoverageChecks.cs", 1, false, HasImplementation: false);
        Check(!CommandCatalog.All.Any(c => c.Name is "reflection_fixture.retired" or "reflection_fixture.unreviewed"), "空实现或未审查入口仍被登记");
        await Reject(() => CommandCatalog.Invoke("reflection_fixture.retired", null, CancellationToken.None), "METHOD_NOT_FOUND");
        var genericRegistry = new MethodRegistry(); CatalogTools.Register(genericRegistry);
        await Reject(() => Call(genericRegistry, "bgi.invoke_command", new { command = "reflection_fixture.retired" }), "METHOD_NOT_FOUND");
        await Reject(() => Call(registry, "bgi.create_command_target", new { command = "reflection_fixture.retired", arguments = new { } }), "METHOD_NOT_FOUND");
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

    static async Task NativeSurfaceChecks()
    {
        var registry=new MethodRegistry();NativeUiTools.Register(registry);ScriptApiTools.Register(registry);
        var model=new SurfaceFixture();var panel=new System.Windows.Controls.StackPanel();
        var name=new System.Windows.Controls.TextBox{Name="队伍名称"};name.SetBinding(System.Windows.Controls.TextBox.TextProperty,new System.Windows.Data.Binding(nameof(SurfaceFixture.Name)){Mode=System.Windows.Data.BindingMode.TwoWay});panel.Children.Add(name);
        var secret=new System.Windows.Controls.TextBox{Name="访问凭据"};secret.SetBinding(System.Windows.Controls.TextBox.TextProperty,new System.Windows.Data.Binding(nameof(SurfaceFixture.Api_Secret)){Mode=System.Windows.Data.BindingMode.TwoWay});panel.Children.Add(secret);
        var readOnly=new System.Windows.Controls.TextBlock{Name="只读说明"};readOnly.SetBinding(System.Windows.Controls.TextBlock.TextProperty,new System.Windows.Data.Binding(nameof(SurfaceFixture.Name)));panel.Children.Add(readOnly);
        var saved=false;var save=new System.Windows.Controls.Button{Content="保存到原生目标"};save.Click+=(_,_)=>saved=true;panel.Children.Add(save);
        var checkedValue=false;var check=new System.Windows.Controls.CheckBox{Content="原生勾选事件"};check.Click+=(_,_)=>checkedValue=check.IsChecked==true;panel.Children.Add(check);
        var order=new ObservableCollection<string>{"甲","乙","丙"};var reordered=false;order.CollectionChanged+=(_,args)=>reordered=args.Action==NotifyCollectionChangedAction.Move;
        var list=new System.Windows.Controls.ListBox{Name="排序列表",Height=60};list.SetBinding(System.Windows.Controls.ItemsControl.ItemsSourceProperty,new System.Windows.Data.Binding{Source=order});panel.Children.Add(list);
        var window=new Window{DataContext=model,Content=panel,Left=-32000,Top=-32000,Width=240,Height=240,Opacity=0,ShowInTaskbar=false,ShowActivated=false};window.Show();window.UpdateLayout();await Task.Delay(50);
        var state=await Call(registry,"bgi.ui.read",new{query="队伍名称"});var field=state.GetProperty("items").EnumerateArray().Single(item=>item.TryGetProperty("fieldId",out _));
        Check(field.GetProperty("writable").GetBoolean(),"可见双向字段不可编辑");
        await Call(registry,"bgi.ui.write",new{fieldId=field.GetProperty("fieldId").GetString(),expectedVersion=field.GetProperty("version").GetString(),value="新队伍"});
        Check(model.Name=="新队伍","未走原生绑定更新实际模型");
        await Reject(()=>Call(registry,"bgi.ui.write",new{fieldId=field.GetProperty("fieldId").GetString(),expectedVersion=field.GetProperty("version").GetString(),value="不能覆盖"}),"VERSION_CONFLICT");
        var privateFields=await Call(registry,"bgi.ui.read",new{query="Api_Secret"});Check(!privateFields.GetRawText().Contains(model.Api_Secret,StringComparison.Ordinal),"凭据从界面契约泄露");
        var readonlyState=await Call(registry,"bgi.ui.read",new{query="只读说明"});Check(!readonlyState.GetProperty("items")[0].GetProperty("writable").GetBoolean(),"默认单向显示被当作可写字段");
        var buttons=await Call(registry,"bgi.ui.read",new{query="保存到原生目标"});var button=buttons.GetProperty("items").EnumerateArray().Single(item=>item.TryGetProperty("kind",out var kind)&&kind.GetString()=="action");
        await Call(registry,"bgi.ui.invoke",new{controlId=button.GetProperty("controlId").GetString()});Check(saved,"没有调用真实原生 Click 处理器");
        var checkState=await Call(registry,"bgi.ui.read",new{query="原生勾选事件"});var checkAction=checkState.GetProperty("items").EnumerateArray().Single(item=>item.TryGetProperty("kind",out var kind)&&kind.GetString()=="action");
        await Call(registry,"bgi.ui.invoke",new{controlId=checkAction.GetProperty("controlId").GetString()});Check(checkedValue,"控件动作绕过了原生 Toggle 行为");
        var listState=await Call(registry,"bgi.ui.read",new{query="排序列表"});var listField=listState.GetProperty("items").EnumerateArray().Single(item=>item.TryGetProperty("fieldId",out _));
        var optionState=await Call(registry,"bgi.ui.options",new{fieldId=listField.GetProperty("fieldId").GetString(),offset=1,limit=1});Check(optionState.GetProperty("options")[0].GetProperty("label").GetString()=="乙","分页选项顺序不正确");
        await Call(registry,"bgi.ui.reorder",new{fieldId=listField.GetProperty("fieldId").GetString(),expectedVersion=listField.GetProperty("version").GetString(),from=0,to=2});Check(reordered&&order[2]=="甲","排序没有进入原生集合通知");
        var continuation=JsonSerializer.SerializeToElement(await NativeUiSurface.Begin(_=>
        {
            var dialog=new BetterGenshinImpact.ViewModel.PromptDialog();dialog.ShowDialog();return Task.FromResult<object?>(new{received=dialog.Box.Text});
        },null,null,CancellationToken.None));
        Check(continuation.GetProperty("state").GetString()=="runningOrAwaitingInput","模态窗口仍卡住初始调用");
        var operationId=continuation.GetProperty("operationId").GetString()!;
        await Call(registry,"bgi.ui.respond",new{operationId,dialogInput=new{text="续接输入",confirm=true}});
        await Task.Delay(120);
        var completed=await Call(registry,"bgi.ui.operation",new{operationId});Check(completed.GetProperty("state").GetString()=="completed"&&completed.GetProperty("result").GetProperty("received").GetString()=="续接输入","已打开的弹窗没有完成续接");
        var ocr=await Call(registry,"bgi.js_api.read",new{id="RecognitionObject",member="Ocr"});Check(ocr.GetProperty("total").GetInt32()>1,"OCR 契约／重载缺失");
        var inherited=await Call(registry,"bgi.js_api.read",new{id="ImageRegion",member="Click"});Check(inherited.GetProperty("total").GetInt32()>0,"图像区域的继承 API 缺失");
        window.Close();await Reject(()=>Call(registry,"bgi.ui.write",new{fieldId=field.GetProperty("fieldId").GetString(),expectedVersion=field.GetProperty("version").GetString(),value="已关闭"}),"STALE_TARGET");
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
        public object? GetService(Type type) => type == typeof(Vm) ? Groups : type == typeof(Wpf.Ui.INavigationService) ? Navigation : type==typeof(BetterGenshinImpact.Service.Interface.IConfigService) ? new BetterGenshinImpact.Service.ConfigService() : null;
    }


}
public class SurfaceFixture:INotifyPropertyChanged
{
    private string name="原队伍";public string Name{get=>name;set{name=value;PropertyChanged?.Invoke(this,new(nameof(Name)));}}
    public string Api_Secret{get;set;}="fixture-private-value";public event PropertyChangedEventHandler? PropertyChanged;
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
        public System.Windows.Input.ICommand RetiredCommand { get; } = new FixturePlainCommand(() => throw new Exception("不得运行已移除入口"));
        public System.Windows.Input.ICommand UnreviewedCommand { get; } = new FixturePlainCommand(() => throw new Exception("不得运行未审查入口"));
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
    public class AllConfig { public Action? OnAnyChangedAction { get; set; } public GameTask.AutoBoss.AutoBossConfig AutoBossConfig { get; set; } = new(); [Obsolete] public bool LegacyFlag { get; set; } }
}
namespace BetterGenshinImpact.Service.Interface { public interface IConfigService { void Save(); } }
namespace BetterGenshinImpact.Service { public class ConfigService : Interface.IConfigService { public static Core.Config.AllConfig Config {get;}=new(); public static bool Saved; public void Save()=>Saved=true; } }
namespace BetterGenshinImpact.GameTask
{
    public class TaskContext { public static TaskContext Instance()=>Current; public static TaskContext Current {get;}=new(); public bool IsInitialized {get;set;}=true; public IntPtr GameHandle {get;}=(IntPtr)1; }
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
