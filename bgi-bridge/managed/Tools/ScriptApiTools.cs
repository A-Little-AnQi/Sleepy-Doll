using BgiBridge.Catalog;

namespace BgiBridge.Tools;

public static class ScriptApiTools
{
    public static void Register(MethodRegistry registry)
    {
        AgentGuide Guide(string title,string purpose)=>new(title,purpose,["用户要求编写／修改 JS，或需要确认 OCR、图像、输入、任务、模块 API 时。"],
            ["来自实际 EngineExtend 注入声明；不会创建脚本引擎或执行方法。"],["只读契约；不截图、不点击、不发送消息。"],"返回别名、CLR 类型、真实重载、参数默认值、返回类型、继承成员与源码；runtimeVerified 分别标注。",
            "从已注册别名开始读；按 nextOffset 补齐需要的成员与返回对象。写 JS 时用真实名称、参数与 await。","只读，无回退。",[],"js-engine-bindings-and-reflection");
        var offset=ArgumentSchema.Parse("""{"type":"integer","minimum":0}""");var limit=ArgumentSchema.Parse("""{"type":"integer","minimum":1,"maximum":60}""");
        registry.Register("bgi.js_api.search","javascript","检索实际 JS 注入的对象、类型和 OCR 等 API",(a,_)=>Task.FromResult<object?>(ScriptApiCatalog.Search(a.GetProperty("query").GetString()!,a.TryGetProperty("offset",out var o)?o.GetInt32():0,a.TryGetProperty("limit",out var l)?l.GetInt32():12)),
            inputSchema:AgentSchemas.Object(("query",ArgumentSchema.Parse("""{"type":"string","maxLength":200}"""),true),("offset",offset,false),("limit",limit,false)),guide:Guide("检索 JS 宿主 API","从当前脚本引擎实际注入的别名和可达类型发现 OCR、图像区域、BvPage/BvLocator、OpenCV、输入和任务 API。"));
        registry.Register("bgi.js_api.read","javascript","读取 JS 真实 API 契约",(a,_)=>Task.FromResult<object?>(ScriptApiCatalog.Read(a.GetProperty("id").GetString()!,a.TryGetProperty("member",out var m)?m.GetString()!:"",a.TryGetProperty("offset",out var o)?o.GetInt32():0,a.TryGetProperty("limit",out var l)?l.GetInt32():30)),
            inputSchema:AgentSchemas.Object(("id",AgentSchemas.Text("已注册 JS 别名、已返回的类型或 OpenCvSharp 完整类型名。"),true),("member",AgentSchemas.Text("可选成员名，包含所有匹配重载。"),false),("offset",offset,false),("limit",limit,false)),guide:Guide("读取 JS API","读取当前类型的构造器、静态／实例方法、继承成员、属性、枚举、默认参数与 Task→Promise 约定。"));
    }
}
