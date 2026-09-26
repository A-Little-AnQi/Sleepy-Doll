using System.Collections.Concurrent;
using System.ComponentModel;
using System.Globalization;
using System.Reflection;
using System.Runtime.CompilerServices;
using System.Text.Json;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Controls.Primitives;
using System.Windows.Data;
using System.Windows.Media;
using System.Windows.Threading;
using BgiBridge.Catalog;
using BgiBridge.Protocol;

namespace BgiBridge.Bgi;

/// <summary>反射当前原生界面的实际绑定与操作。字段更新走 WPF 绑定，动作走真实控件，不猜私有业务方法。</summary>
public static class NativeUiSurface
{
    private sealed class Identity { public string Id {get;}=Guid.NewGuid().ToString("N"); }
    private sealed record Field(WeakReference<FrameworkElement> Target,DependencyProperty Property);
    private sealed class Operation
    {
        public string Id {get;}=Guid.NewGuid().ToString("N");
        public CancellationTokenSource Cancellation {get;}=new();
        public TaskCompletionSource<object?> Completion {get;}=new(TaskCreationOptions.RunContinuationsAsynchronously);
        public NativeDialogScope? Dialog;
        public DateTimeOffset Started {get;}=DateTimeOffset.UtcNow;
    }
    private static readonly ConditionalWeakTable<object,Identity> Ids=new();
    private static readonly Dictionary<string,WeakReference<FrameworkElement>> Controls=[];
    private static readonly Dictionary<string,Field> Fields=[];
    private static readonly ConcurrentDictionary<Type,DependencyProperty[]> PropertyCache=new();
    private static readonly ConcurrentDictionary<string,Operation> Operations=new();
    private static string Id(object value)=>Ids.GetValue(value,_=>new()).Id;
    private static IEnumerable<DependencyObject> Elements()
    {
        var pending=new Queue<DependencyObject>(Application.Current.Windows.Cast<Window>().Where(w=>w.IsVisible));
        foreach(var source in PresentationSource.CurrentSources.Cast<PresentationSource>())if(source.RootVisual is not null)pending.Enqueue(source.RootVisual);
        var seen=new HashSet<DependencyObject>(ReferenceEqualityComparer.Instance);
        while(pending.Count>0&&seen.Count<30000)
        {
            var node=pending.Dequeue();if(!seen.Add(node))continue;yield return node;
            foreach(var child in LogicalTreeHelper.GetChildren(node).OfType<DependencyObject>())pending.Enqueue(child);
            if(node is Visual||node is System.Windows.Media.Media3D.Visual3D)
                for(var i=0;i<VisualTreeHelper.GetChildrenCount(node);i++)pending.Enqueue(VisualTreeHelper.GetChild(node,i));
        }
    }
    private static DependencyProperty[] Properties(FrameworkElement target)
    {
        var properties=new HashSet<DependencyProperty>();var local=target.GetLocalValueEnumerator();while(local.MoveNext())properties.Add(local.Current.Property);
        foreach(var property in PropertyCache.GetOrAdd(target.GetType(),type=>
        {
            var declared=new HashSet<DependencyProperty>();
            for(var current=type;current is not null;current=current.BaseType)
                foreach(var field in current.GetFields(BindingFlags.Static|BindingFlags.Public|BindingFlags.NonPublic|BindingFlags.DeclaredOnly))
                    if(field.FieldType==typeof(DependencyProperty)&&field.GetValue(null) is DependencyProperty property)declared.Add(property);
            return declared.ToArray();
        }))properties.Add(property);
        return properties.ToArray();
    }
    private static bool Mutable(DependencyProperty property)=>!property.ReadOnly;
    private static bool Editable(FrameworkElement target,DependencyProperty property,BindingExpression binding)
    {
        var mode=binding.ParentBinding.Mode;
        if(mode==BindingMode.Default)mode=property.GetMetadata(target.GetType()) is FrameworkPropertyMetadata{BindsTwoWayByDefault:true}?BindingMode.TwoWay:BindingMode.OneWay;
        return !property.ReadOnly&&mode is BindingMode.TwoWay or BindingMode.OneWayToSource;
    }
    private static bool Sensitive(string? path)=>SettingsCatalog.IsSensitive((path??"").Replace("_","").Replace("-",""));
    private static string Caption(FrameworkElement target)
    {
        if(target is TextBlock block&&BindingOperations.GetBindingExpression(block,TextBlock.TextProperty) is {} textBinding&&Sensitive(textBinding.ParentBinding.Path?.Path))return "敏感信息";
        var text=target switch{ButtonBase b=>b.Content as string,HeaderedContentControl h=>h.Header as string,HeaderedItemsControl h=>h.Header as string,TextBlock t=>t.Text,_=>null};
        return (text??target.ToolTip as string??target.Name??"").Trim();
    }
    private static string Label(FrameworkElement target)
    {
        var own=Caption(target);if(own.Length>0)return own.Length>200?own[..200]:own;
        DependencyObject? parent=target;
        for(var i=0;i<4 && parent is not null;i++)
        {
            if(parent is FrameworkElement element && Caption(element) is {Length:>0} caption)return caption;
            parent=LogicalTreeHelper.GetParent(parent)??(parent is Visual?VisualTreeHelper.GetParent(parent):null);
        }
        return target.GetType().Name;
    }
    private static BindingExpression Binding(FrameworkElement target,DependencyProperty property)=>BindingOperations.GetBindingExpression(target,property)??throw BridgeException.InvalidArgument("字段不再具有原生绑定。");
    private static JsonElement Snapshot(object? value)
    {
        if(value is null)return ArgumentSchema.Parse("null");
        if(value is double number&&!double.IsFinite(number)||value is float real&&!float.IsFinite(real))return ArgumentSchema.Parse("null");
        if(value is string||value.GetType().IsPrimitive||value is decimal||value.GetType().IsEnum)return ValueContract.Snapshot(value,value.GetType());
        if(value.GetType().IsValueType&&value.GetType().Namespace?.StartsWith("System.Windows",StringComparison.Ordinal)==true)return JsonSerializer.SerializeToElement(TypeDescriptor.GetConverter(value.GetType()).ConvertToInvariantString(value));
        return JsonSerializer.SerializeToElement(new{objectId=Id(value),type=value.GetType().FullName});
    }
    private static string Version(FrameworkElement target,DependencyProperty property,BindingExpression binding)=>ValueContract.Version(JsonSerializer.SerializeToElement(new
    {owner=binding.ResolvedSource is null?null:Id(binding.ResolvedSource),path=binding.ResolvedSourcePropertyName,value=Snapshot(target.GetValue(property)),options=target is Selector selector?selector.Items.Cast<object>().Select(Id).ToArray():null}));
    private static object Describe(string key,FrameworkElement target,DependencyProperty property)
    {
        var binding=Binding(target,property);var owner=binding.ResolvedSource;var name=binding.ResolvedSourcePropertyName;
        var sourceProperty=owner?.GetType().GetProperty(name??"");var sensitive=Sensitive(binding.ParentBinding.Path?.Path??name??"")||target is PasswordBox;
        var value=Snapshot(target.GetValue(property));var choices=target is Selector selector ? selector.Items.Cast<object>().Take(80).Select((item,index)=>new{optionId=Id(item),index,label=item is string text?text:(Reflect.Get(item,"Name")??Reflect.Get(item,"DisplayName")??Reflect.Get(item,"EnumName")??item.ToString())?.ToString()}).ToArray():null;
        var schema=ValueContract.Schema(property.PropertyType)??(property.PropertyType.IsValueType&&property.PropertyType.Namespace?.StartsWith("System.Windows",StringComparison.Ordinal)==true?AgentSchemas.Text("原生 WPF 格式，例如颜色 #RRGGBB、边距或坐标的逗号格式。",16384):(JsonElement?)null);
        return new{fieldId=key,controlId=Id(target),controlName=target.Name,controlType=target.GetType().FullName,label=Label(target),path=binding.ParentBinding.Path?.Path,sourceType=owner?.GetType().FullName,sourceProperty=name,targetType=property.PropertyType.FullName,
            writable=Editable(target,property,binding)&&target.IsEnabled&&sourceProperty?.SetMethod?.IsPublic!=false,
            sensitive,currentValue=sensitive?(object)"***REDACTED***":value,version=Version(target,property,binding),valueSchema=schema,options=sensitive?null:choices,
            explanation=owner is null?null:SourceDocumentation.Find("P",owner.GetType(),name??""),validationErrors=Validation.GetErrors(target).Select(e=>e.ErrorContent?.ToString()).ToArray(),
            chain="ui.read 定位真实 fieldId/version → ui.write 通过控件绑定提交 → 读取原生结果；有保存按钮时继续 ui.invoke 保存。"};
    }
    public static object Read(string query,int offset,int limit)
    {
        var records=new List<object>();
        foreach(var target in Elements().OfType<FrameworkElement>().Where(e=>e.IsVisible))
        {
            var key=Id(target);Controls[key]=new(target);
            if(target is Window window)records.Add(new{kind="window",controlId=key,label=window.Title,type=window.GetType().FullName,enabled=window.IsEnabled});
            if(target is Expander||target is TreeViewItem||target is TabItem||target.ContextMenu is not null)records.Add(new{kind="container",controlId=key,label=Label(target),type=target.GetType().FullName,enabled=target.IsEnabled,chain="ui.invoke action=expand/collapse/contextMenu/focus；展开后 ui.read 查询实际字段与动作。"});
            if(target is ButtonBase||target is MenuItem)records.Add(new{kind="action",controlId=key,label=Label(target),type=target.GetType().FullName,enabled=target.IsEnabled,chain="ui.invoke 触发当前控件的真实 OnClick／ICommand；弹窗与长任务通过 operationId 续接。"});
            foreach(var property in Properties(target))
            {
                if(!Mutable(property)||BindingOperations.GetBindingExpression(target,property) is not { } binding||binding.ResolvedSource is null)continue;
                var fieldId=key+":"+property.Name;Fields[fieldId]=new(new(target),property);
                records.Add(Describe(fieldId,target,property));
            }
        }
        var searchJson=new JsonSerializerOptions{Encoder=System.Text.Encodings.Web.JavaScriptEncoder.UnsafeRelaxedJsonEscaping};
        var filtered=records.Where(record=>query.Length==0||JsonSerializer.Serialize(record,searchJson).Contains(query,StringComparison.OrdinalIgnoreCase)).ToArray();
        return new{total=filtered.Length,items=filtered.Skip(offset).Take(limit),nextOffset=offset+limit<filtered.Length?(int?)(offset+limit):null,
            operations=Operations.Values.Where(operation=>!operation.Completion.Task.IsCompleted).Take(8).Select(Status),note="只枚举当前真实界面；未展开的分组、菜单或页面先打开／展开再读。字段来自实际 WPF 绑定，包含全局、窗口和列表行设置。"};
    }
    private static FrameworkElement Target(string key)
    {
        if(!Controls.TryGetValue(key,out var weak)||!weak.TryGetTarget(out var target)||!target.IsVisible||!Elements().Contains(target))throw new BridgeException("STALE_TARGET","界面已变化，请重新 ui.read。",409);
        return target;
    }
    public static object Write(string fieldId,string expected,JsonElement value)
    {
        if(!Fields.TryGetValue(fieldId,out var field)||!field.Target.TryGetTarget(out var target))throw BridgeException.NotFound("字段不存在，先 ui.read。");
        Target(Id(target));var property=field.Property;var binding=Binding(target,property);
        if(!target.IsEnabled||!Editable(target,property,binding))throw BridgeException.InvalidArgument("当前原生字段不可编辑。");
        var old=target.GetValue(property);if(Version(target,property,binding)!=expected)throw new BridgeException("VERSION_CONFLICT","字段、绑定目标或选项已变化，未修改。",409);
        object? converted;
        if(target is Selector selector&&value.ValueKind==JsonValueKind.Object&&value.TryGetProperty("optionId",out var selected))
        {
            var matches=selector.Items.Cast<object>().Where(item=>Id(item)==selected.GetString()).ToArray();if(matches.Length!=1)throw new BridgeException("STALE_TARGET","选项已失效。",409);
            converted=matches[0];property=Selector.SelectedItemProperty;old=selector.SelectedItem;
        }
        else
        {
            var schema=ValueContract.Schema(property.PropertyType);
            if(schema is not null){ArgumentSchema.Validate(value,schema.Value);converted=JsonSerializer.Deserialize(value.GetRawText(),property.PropertyType,ValueContract.Json);}
            else if(value.ValueKind==JsonValueKind.String&&property.PropertyType.IsValueType&&property.PropertyType.Namespace?.StartsWith("System.Windows",StringComparison.Ordinal)==true)
                converted=TypeDescriptor.GetConverter(property.PropertyType).ConvertFromInvariantString(value.GetString()!);
            else throw BridgeException.InvalidArgument("复杂选择必须使用当前返回的 optionId；结构化 WPF 值使用原生字符串格式。");
        }
        var checkpoint=SettingsTransactions.Engine.Checkpoint("bgi.ui.write");
        try
        {
            target.SetCurrentValue(property,converted);binding.UpdateSource();binding.UpdateTarget();
            if(Validation.GetHasError(target))throw BridgeException.InvalidArgument("原生输入校验未通过："+string.Join("；",Validation.GetErrors(target).Select(e=>e.ErrorContent)));
            if(ValueContract.Canonical(Snapshot(converted))!=ValueContract.Canonical(Snapshot(old))&&ValueContract.Canonical(Snapshot(target.GetValue(property)))==ValueContract.Canonical(Snapshot(old)))
                throw BridgeException.InvalidArgument("原生绑定没有接受新值，未报告修改成功。");
        }
        catch
        {
            target.SetCurrentValue(property,old);binding.UpdateSource();binding.UpdateTarget();throw;
        }
        return new{applied=true,configurationCheckpoint=checkpoint,field=Describe(fieldId,target,field.Property),verificationRequired="检查返回的原生值及校验结果；需要保存／关闭应用的编辑器继续使用其真实按钮，不把控件输入当作落盘。"};
    }
    public static object Options(string fieldId,int offset,int limit)
    {
        if(!Fields.TryGetValue(fieldId,out var field)||!field.Target.TryGetTarget(out var target))throw BridgeException.NotFound("先 ui.read 定位字段。");
        Target(Id(target));if(target is not Selector selector)throw BridgeException.InvalidArgument("字段不是列表／下拉选择。");
        var binding=Binding(target,field.Property);var sensitive=Sensitive(binding.ParentBinding.Path?.Path);var all=selector.Items.Cast<object>().ToArray();
        return new{fieldId,version=Version(target,field.Property,binding),total=all.Length,options=all.Skip(offset).Take(limit).Select((item,index)=>new{optionId=Id(item),index=offset+index,label=sensitive?"***REDACTED***":(item is string text?text:(Reflect.Get(item,"Name")??Reflect.Get(item,"DisplayName")??Reflect.Get(item,"EnumName")??item.ToString())?.ToString())}),nextOffset=offset+limit<all.Length?(int?)(offset+limit):null};
    }
    public static object Reorder(string fieldId,string expected,int from,int to)
    {
        if(!Fields.TryGetValue(fieldId,out var field)||!field.Target.TryGetTarget(out var target))throw BridgeException.NotFound("先 ui.read 定位列表字段。");
        Target(Id(target));var binding=Binding(target,field.Property);
        if(!target.IsEnabled||Version(target,field.Property,binding)!=expected)throw new BridgeException("VERSION_CONFLICT","列表已变化，未排序。",409);
        if(target is not ItemsControl items||items.ItemsSource is not System.Collections.IList list)throw BridgeException.InvalidArgument("当前控件没有可排序的原生列表。");
        if(from<0||to<0||from>=list.Count||to>=list.Count)throw BridgeException.InvalidArgument("索引超出当前列表。");
        var move=list.GetType().GetMethod("Move",[typeof(int),typeof(int)])??throw BridgeException.InvalidArgument("列表未提供原生 Move，不能重建集合。");
        var checkpoint=SettingsTransactions.Engine.Checkpoint("bgi.ui.reorder");var item=list[from];move.Invoke(list,[from,to]);
        return new{moved=ReferenceEquals(list[to],item)||Equals(list[to],item),from,to,configurationCheckpoint=checkpoint,verificationRequired="原生集合通知与保存回调已触发；回读列表顺序和对应资源文件。"};
    }
    public static async Task<object> Begin(Func<CancellationToken,Task<object?>> action,JsonElement? input,string[]? titles,CancellationToken cancellation)
    {
        cancellation.ThrowIfCancellationRequested();
        foreach(var old in Operations.Values.Where(o=>o.Completion.Task.IsCompleted&&DateTimeOffset.UtcNow-o.Started>TimeSpan.FromMinutes(30)).ToArray())
            if(Operations.TryRemove(old.Id,out _))old.Cancellation.Dispose();
        if(Operations.Count>=128)throw BridgeException.Busy("界面操作记录过多，请完成或取消当前操作。");
        var operation=new Operation();Operations[operation.Id]=operation;
        _ = Application.Current.Dispatcher.BeginInvoke(DispatcherPriority.Normal,new Action(async()=>
        {
            try
            {
                operation.Dialog=new NativeDialogScope(input,titles,operation.Cancellation.Token);
                var result=await action(operation.Cancellation.Token);operation.Dialog.Verify();operation.Completion.TrySetResult(result);
            }
            catch(Exception error){if(operation.Cancellation.IsCancellationRequested)operation.Completion.TrySetCanceled();else operation.Completion.TrySetException(Reflect.Root(error));}
            finally{operation.Dialog?.Dispose();}
        }));
        await Task.WhenAny(operation.Completion.Task,Task.Delay(400,cancellation)).ConfigureAwait(false);
        if(cancellation.IsCancellationRequested){operation.Cancellation.Cancel();cancellation.ThrowIfCancellationRequested();}
        return Status(operation);
    }
    private static object Status(Operation operation)=>new{operationId=operation.Id,state=operation.Completion.Task.IsCanceled?"cancelled":operation.Completion.Task.IsFaulted?"failed":operation.Completion.Task.IsCompleted?"completed":"runningOrAwaitingInput",
        result=operation.Completion.Task.IsCompletedSuccessfully?operation.Completion.Task.Result:null,error=operation.Completion.Task.Exception?.GetBaseException().Message,
        resultMeaning="completed 仅表示原生命令／事件返回；弹窗、游戏任务与保存结果按原生证据核验。ui.read 查看窗口和字段，ui.respond 填本次弹窗，ui.invoke 继续保存／确认，ui.operation 读取最后结果。"};
    public static object OperationRead(string id)=>Operations.TryGetValue(id,out var operation)?Status(operation):throw BridgeException.NotFound("界面 operationId 不存在。");
    public static object Respond(string id,JsonElement input)
    {
        if(!Operations.TryGetValue(id,out var operation)||operation.Completion.Task.IsCompleted||operation.Dialog is null)throw BridgeException.InvalidArgument("操作已结束或还未出现可填写弹窗。");
        operation.Dialog.SupplyInput(input);return new{accepted=true,operationId=id,verificationRequired="随后 ui.read/ui.operation 核对实际窗口、输入与终态。"};
    }
    public static async Task<object> Invoke(string id,JsonElement? input,CancellationToken cancellation,string action="click")
    {
        var target=Target(id);if(!target.IsEnabled)throw BridgeException.InvalidArgument("当前控件禁用；不能绕过原生前置。");
        return await Begin(async token=>
        {
            token.ThrowIfCancellationRequested();
            if(action is "expand" or "collapse")
            {
                if(target is Expander expander)expander.SetCurrentValue(Expander.IsExpandedProperty,action=="expand");
                else if(target is TreeViewItem tree)tree.SetCurrentValue(TreeViewItem.IsExpandedProperty,action=="expand");
                else if(target is TabItem tab)tab.SetCurrentValue(TabItem.IsSelectedProperty,true);
                else throw BridgeException.InvalidArgument("当前控件不是可展开分组或页签。");
                return new{dispatched=true,controlId=id};
            }
            if(action=="contextMenu"){if(target.ContextMenu is null)throw BridgeException.InvalidArgument("该控件没有原生上下文菜单。");target.ContextMenu.PlacementTarget=target;target.ContextMenu.IsOpen=true;return new{dispatched=true,controlId=id};}
            if(action=="focus"){target.BringIntoView();target.Focus();return new{dispatched=true,controlId=id};}
            var command=target is ButtonBase button?button.Command:(target as MenuItem)?.Command;
            var parameter=target is ButtonBase b?b.CommandParameter:(target as MenuItem)?.CommandParameter;
            if(command is not null)
            {
                if(!command.CanExecute(parameter))throw BridgeException.InvalidArgument("原生命令 CanExecute=false。");
                var commandBinding=BindingOperations.GetBindingExpression(target,target is ButtonBase?ButtonBase.CommandProperty:MenuItem.CommandProperty);
                var game=commandBinding?.ResolvedSource is {} owner&&CommandDocumentation.RequiresGameReady(owner.GetType().Name,commandBinding.ResolvedSourcePropertyName);
                if(game&&!Host.CaptureReady)throw BridgeException.GameNotReady("当前原生动作需要游戏截图器就绪。");
                using var hostCancellation=game?new HostTaskCancellation(token):null;
                var click=target.GetType().GetMethod("OnClick",BindingFlags.Instance|BindingFlags.NonPublic,[]);
                if(click is null)throw BridgeException.InvalidArgument("控件没有原生点击入口。");
                click.Invoke(target,null);
                if(Reflect.Get(command,"ExecutionTask") is Task execution)await execution;
                if(hostCancellation is not null)await hostCancellation.CompleteAsync();
            }
            else
            {
                if(target is MenuItem{HasItems:true} menu){menu.SetCurrentValue(MenuItem.IsSubmenuOpenProperty,true);return new{dispatched=true,controlId=id};}
                var method=target.GetType().GetMethod("OnClick",BindingFlags.Instance|BindingFlags.NonPublic,[]);
                if(method is null)throw BridgeException.InvalidArgument("该控件不是可调用动作。");method.Invoke(target,null);
            }
            return new{dispatched=true,controlId=id,verificationRequired="按目标读取实际字段、产物、日志或游戏状态。"};
        },input,null,cancellation);
    }
    public static object Close(string id)
    {
        if(Target(id) is not Window window)throw BridgeException.InvalidArgument("只接受 ui.read 返回的窗口 ID。");
        window.Close();return new{closed=!window.IsVisible,verificationRequired="关闭可能触发宿主保存或关闭到托盘；按目标回读结果。"};
    }
    public static object Cancel(string id)
    {
        if(!Operations.TryGetValue(id,out var operation))throw BridgeException.NotFound("操作不存在。");
        operation.Cancellation.Cancel();operation.Dialog?.SupplyInput(JsonSerializer.SerializeToElement(new{confirm=false}));return new{cancellationRequested=true,operationId=id};
    }
}
