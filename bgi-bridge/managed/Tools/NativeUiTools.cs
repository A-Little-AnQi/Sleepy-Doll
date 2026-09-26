using BgiBridge.Bgi;
using BgiBridge.Catalog;

namespace BgiBridge.Tools;

public static class NativeUiTools
{
    public static void Register(MethodRegistry registry)
    {
        AgentGuide Guide(string title,string purpose,bool write)=>new(title,purpose,["当前可见功能没有合适稳定入口，或正在处理原生编辑器／列表行／弹窗时。"],
            ["先打开目标页面，ui.read 定位真实控件、字段、选项及版本；不能猜 ID。"],write?["触发宿主原生控件、绑定、保存或关闭事件；可能改变配置／资源／游戏。"]:["只读实际界面绑定和操作状态；敏感值遮蔽。"],
            "控件或原生命令阶段结果；runningOrAwaitingInput 需续接，completed 不代表用户目标完成。",
            "通过 ui.read 取得实际字段、按钮和窗口；填写后执行真实保存／确认，再 ui.operation 与资源／设置／日志核验。","输入前保存配置检查点；已执行的原生事件及游戏操作不能自动撤销。",[],"native-wpf-bindings-and-continuation");
        var text=AgentSchemas.Text("来自当前 ui.read 或执行结果的真实 ID。",256);
        var offset=ArgumentSchema.Parse("""{"type":"integer","minimum":0}""");var limit=ArgumentSchema.Parse("""{"type":"integer","minimum":1,"maximum":100}""");
        registry.Register("bgi.ui.read","ui","读取实际可见功能、字段、选项、操作与窗口",(a,_)=>Ui.InvokeAsync<object?>(()=>NativeUiSurface.Read(a.TryGetProperty("query",out var q)?q.GetString()!:"",a.TryGetProperty("offset",out var o)?o.GetInt32():0,a.TryGetProperty("limit",out var l)?l.GetInt32():30)),
            inputSchema:AgentSchemas.Object(("query",ArgumentSchema.Parse("""{"type":"string","maxLength":200}"""),false),("offset",offset,false),("limit",limit,false)),guide:Guide("读取原生可见界面","反射当前实际 WPF 绑定、列表行、弹窗、按钮及选项；包括不属于 AllConfig 的可编辑设置。",false));
        registry.Register("bgi.ui.write","ui","通过真实控件绑定修改一个可见字段",(a,_)=>Ui.InvokeAsync<object?>(()=>NativeUiSurface.Write(a.GetProperty("fieldId").GetString()!,a.GetProperty("expectedVersion").GetString()!,a.GetProperty("value"))),readOnly:false,
            inputSchema:AgentSchemas.Object(("fieldId",text,true),("expectedVersion",text,true),("value",ArgumentSchema.Parse("""{"description":"满足当前字段 valueSchema；选项使用 ui.read 返回的 {optionId}，不会从 JSON 伪造宿主对象。"}"""),true)),guide:Guide("修改原生可见字段","通过 WPF 原生绑定与校验修改字段，保留选择变更和控件事件；需要保存的编辑器继续点击保存。",true));
        registry.Register("bgi.ui.options","ui","分页读取原生字段全部选项",(a,_)=>Ui.InvokeAsync<object?>(()=>NativeUiSurface.Options(a.GetProperty("fieldId").GetString()!,a.TryGetProperty("offset",out var o)?o.GetInt32():0,a.TryGetProperty("limit",out var l)?l.GetInt32():40)),
            inputSchema:AgentSchemas.Object(("fieldId",text,true),("offset",offset,false),("limit",limit,false)),guide:Guide("读取全部原生选项","ui.read 只附带首批选项；用当前 fieldId 与分页游标读取后续真实 optionId。",false));
        var index=ArgumentSchema.Parse("""{"type":"integer","minimum":0}""");
        registry.Register("bgi.ui.reorder","ui","按当前原生列表顺序移动项目",(a,_)=>Ui.InvokeAsync<object?>(()=>NativeUiSurface.Reorder(a.GetProperty("fieldId").GetString()!,a.GetProperty("expectedVersion").GetString()!,a.GetProperty("from").GetInt32(),a.GetProperty("to").GetInt32())),readOnly:false,
            inputSchema:AgentSchemas.Object(("fieldId",text,true),("expectedVersion",text,true),("from",index,true),("to",index,true)),guide:Guide("排序原生列表","使用原生 ObservableCollection.Move 触发当前界面与保存回调，不复制或重建宿主集合；索引来自当前 options。",true));
        registry.Register("bgi.ui.invoke","ui","执行真实可见控件动作并续接弹窗",(a,c)=>Ui.InvokeAsync<object?>(async()=>await NativeUiSurface.Invoke(a.GetProperty("controlId").GetString()!,a.TryGetProperty("dialogInput",out var d)?d:null,c,a.TryGetProperty("action",out var mode)?mode.GetString()!:"click")),readOnly:false,
            inputSchema:AgentSchemas.Object(("controlId",text,true),("action",ArgumentSchema.Parse("""{"type":"string","enum":["click","expand","collapse","contextMenu","focus"]}"""),false),("dialogInput",NativeDialogScope.Schema,false)),guide:Guide("执行原生界面动作","执行当前真实控件 ICommand 或 OnClick；弹窗和异步动作返回 operationId，不能把初始阶段误报为完成。",true));
        registry.Register("bgi.ui.operation","ui","读取原生界面操作最后结果",(a,_)=>Task.FromResult<object?>(NativeUiSurface.OperationRead(a.GetProperty("operationId").GetString()!)),
            inputSchema:AgentSchemas.Object(("operationId",text,true)),guide:Guide("读取界面操作结果","读取原生命令的完成、异常或取消；如果仍在等待输入，用 ui.read/ respond / invoke 续接。",false));
        registry.Register("bgi.ui.respond","ui","向本次尚未结束的原生弹窗提供输入",(a,_)=>Ui.InvokeAsync<object?>(()=>NativeUiSurface.Respond(a.GetProperty("operationId").GetString()!,a.GetProperty("dialogInput"))),readOnly:false,
            inputSchema:AgentSchemas.Object(("operationId",text,true),("dialogInput",NativeDialogScope.Schema,true)),guide:Guide("填写本次原生弹窗","将 text、filePath、selectedValues 或确认信息交给该 operation 的原始窗口作用域；不排除已经打开的本次弹窗。",true));
        registry.Register("bgi.ui.close","ui","关闭指定原生窗口并执行宿主关闭逻辑",(a,_)=>Ui.InvokeAsync<object?>(()=>NativeUiSurface.Close(a.GetProperty("controlId").GetString()!)),readOnly:false,
            inputSchema:AgentSchemas.Object(("controlId",text,true)),guide:Guide("关闭原生窗口","对 ui.read 的真实窗口调用 Close；可能触发关闭保存或主窗口关闭到托盘，优先使用明确保存／取消按钮。",true));
        registry.Register("bgi.ui.cancel","ui","请求取消本次原生界面操作",(a,_)=>Ui.InvokeAsync<object?>(()=>NativeUiSurface.Cancel(a.GetProperty("operationId").GetString()!)),readOnly:false,
            inputSchema:AgentSchemas.Object(("operationId",text,true)),guide:Guide("取消界面操作","请求取消该 operation 的输入／命令；仍开放的编辑窗口用真实取消按钮或 ui.close 收尾，再核对终态。",true));
    }
}
