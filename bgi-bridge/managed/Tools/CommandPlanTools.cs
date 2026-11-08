using System.Text.Json;
using BgiBridge.Bgi;
using BgiBridge.Catalog;

namespace BgiBridge.Tools;

public static class CommandPlanTools
{
    public static void Register(MethodRegistry registry)
    {
        var validationSchema = AgentSchemas.Object(("plan",CommandPlan.Schema,true),("mode",ArgumentSchema.Parse("""{"type":"string","enum":["host","game"]}"""),true));
        registry.Register("bgi.validate_command_plan","command","只读校验固定命令计划和运行期对象选择条件",
            (a,_)=>Ui.InvokeAsync<object?>(()=>
            {
                var contracts=CommandPlan.Validate(a.GetProperty("plan"),a.GetProperty("mode").GetString()=="game",new NativePlanHost());
                return new { valid=true,contractHash=CommandPlan.ContractHash(contracts),requiresGameReady=contracts.Any(c=>c.Game),commands=contracts.Select(c=>new{command=$"cmd.{c.Name}",effect=c.Game?"gameWrite":"hostCommand",destructive=c.Destructive}),verificationScope="commandHandlers",executed=false };
            }),inputSchema:validationSchema,guide:Guide("校验命令计划","保存定时或快捷计划前，只读核对固定目录命令、参数类型、无弹窗前提与运行期选择条件，不构造窗口、不调用命令、不兑换。",false));
        foreach(var game in new[]{false,true})
        {
            var mode=game?"game":"host";
            registry.Register($"bgi.run_{mode}_command_plan","command",game?"执行含游戏动作的固定命令计划":"执行宿主固定命令计划并在运行时解析对象",
                (a,c)=>Ui.InvokeAsync<object?>(async()=>await CommandPlan.Run(a,game,new NativePlanHost(),c)),readOnly:false,destructive:game,inputSchema:CommandPlan.Schema,
                guide:Guide(game?"执行游戏命令计划":"执行宿主命令计划","按固定顺序执行当前目录中的已审查命令；异步命令等待 Task。resolveArgument 在本次 Job 的当前命令所属对象图中按条件选唯一对象，可按明确字段排序；不保存历史 objectId/contextId。没有匹配项、排序并列、契约变化或调用失败则停止，不用旧数据顶替。处理器完成不等于业务完成，有本次业务回执才核验，否则未确认。",true),requiresGameReady:game,preparation:game?"ensureGameReady":null,ownsHostCancellation:true);
        }
    }
    private static AgentGuide Guide(string title,string purpose,bool write)=>new(title,purpose,
        ["任务需要到运行时才读取最新数据、绑定当前原生对象，且各条命令和选择规则能提前固定时。"],
        ["先读当前命令契约，再用 validate_command_plan 校验；固定命令名，不保存运行期对象 ID。", "steps 最多16项，只有命令和固定等待；破坏性、内部事件、未审查、需弹窗或无法等待的命令拒绝。", "选择条件使用真实参数类型的属性；todayOrLater 是本机当天零时下限，获取时间不能冒充发布时间或有效期。"],
        write?["按计划执行当前命令的已声明副作用；游戏计划可能启动游戏、向游戏发送输入。"]:["仅读取命令目录和类型契约，不执行计划。"],
        "校验返回契约指纹；运行可限定核验本次获取的数据，或使用实际业务回执；缺证才保持未确认。","实际完成、已使用、过期或失败必须来自宿主业务结果；不得从处理器返回推断。",
        "取消停止后续步骤，已发生的输入或修改不自动回滚。",[],"bridge-bounded-native-command-plan");

    private sealed class NativePlanHost:ICommandPlanHost
    {
        private IReadOnlyList<object> created=[];
        private IReadOnlyList<object> initial=[];
        public void Begin()=>initial=CommandTargets.Snapshot().ToArray();
        public IReadOnlyList<object> TakeCreated(){var values=created;created=[];return values;}
        public CommandPlanContract Describe(string name)
        {
            var (d,owner,parameter)=CommandCatalog.PlanContract(name);
            var fingerprint=$"{owner.Assembly.ManifestModule.ModuleVersionId}:{typeof(CommandPlan).Assembly.ManifestModule.ModuleVersionId}:{d.ParameterSchema?.GetRawText()}:{d.Guide.Purpose}:{d.Guide.Verification}";
            return new(name,owner,parameter,d.RequiresGameReady,d.IsDestructive,d.NeedsDialogInput,d.InternalUiEvent,d.UnavailableReason,fingerprint);
        }
        public object Owner(CommandPlanContract contract,IReadOnlyList<object> created)
        {
            var own=created.Where(item=>CommandTargets.Matches(contract.Owner,item.GetType())).ToArray();
            if(own.Length>1)throw Protocol.BridgeException.InvalidArgument("本次计划创建了多个同类上下文，未猜选目标。");
            if(own.Length==1)return own[0];
            var existing=initial.Where(item=>CommandTargets.Matches(contract.Owner,item.GetType())).ToArray();
            if(existing.Length!=1)throw Protocol.BridgeException.InvalidArgument("计划开始时上下文不唯一，且没有本次同步创建的唯一上下文；未借用其它窗口。");
            return existing[0];
        }
        public IReadOnlyList<object> Snapshot(object? root=null)=>CommandTargets.Snapshot(root);
        public async Task<JsonElement?> Invoke(CommandPlanContract contract,object owner,object? argument,CancellationToken cancellation)
        {
            JsonElement? parameter=argument is null?null:argument.GetType().IsPrimitive||argument is string||argument.GetType().IsEnum
                ?JsonSerializer.SerializeToElement(argument):JsonSerializer.SerializeToElement(new{objectId=CommandTargets.RuntimeReference(argument)});
            if(contract.Game && Host.TaskSemaphoreCount()!=1)throw Protocol.BridgeException.Busy("宿主当前任务执行权不可确认空闲；未启动或取消其它任务。");
            var receipt=NativeCommandEvidence.Begin(contract.Name,argument);
            await CommandCatalog.Invoke(contract.Name,parameter,cancellation,CommandTargets.RuntimeReference(owner),ownCancellation:true,captureCreated:values=>created=values);
            return await receipt.Read(cancellation);
        }
    }
}
