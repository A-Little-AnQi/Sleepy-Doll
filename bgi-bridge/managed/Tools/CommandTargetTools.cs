using System.Text.Json;
using BgiBridge.Bgi;
using BgiBridge.Catalog;
using BgiBridge.Protocol;

namespace BgiBridge.Tools;

public static class CommandTargetTools
{
    public static void Register(MethodRegistry registry)
    {
        AgentGuide Guide(string title, string purpose, bool write) => new(title, purpose,
            ["已发现精确 cmd ID，需要绑定对象参数、窗口、子 ViewModel 或泛型实例。"],
            ["使用当前引用；多实例不猜目标。"], write ? ["可能构造宿主对象或释放桥创建的上下文。"] : ["只枚举当前对象与构造契约，不构造服务或窗口。"],
            "对象引用与构造／释放结果；不证明命令或游戏目标完成。", "继续绑定精确 contextId/objectId，执行后核对业务证据。",
            "只释放桥创建的上下文；用户已有对象保留。", [], "bridge-native-object-binding");
        var command = ArgumentSchema.Parse("""{"type":"string","minLength":1,"maxLength":200,"description":"cmd. 前缀可保留，使用目录返回的精确命令 ID。"}""");
        registry.Register("bgi.list_command_targets", "command", "读取真实命令上下文与参数对象",
            (a, _) => Ui.InvokeAsync<object?>(() => CommandCatalog.Targets(Name(a), a.TryGetProperty("query", out var query) ? query.GetString()! : "",
                a.TryGetProperty("offset", out var offset) ? offset.GetInt32() : 0, a.TryGetProperty("limit", out var limit) ? limit.GetInt32() : 20)),
            inputSchema: AgentSchemas.Object(("command", command, true), ("query", ArgumentSchema.Parse("""{"type":"string","maxLength":200}"""), false),
                ("offset", ArgumentSchema.Parse("""{"type":"integer","minimum":0}"""), false), ("limit", ArgumentSchema.Parse("""{"type":"integer","minimum":1,"maximum":20}"""), false)),
            guide: Guide("读取命令目标", "反射当前窗口、数据上下文和宿主集合；返回真实 objectId 与构造参数，不靠界面当前选中猜目标。", false));
        registry.Register("bgi.create_command_target", "command", "按构造契约建立命令上下文",
            (a, cancellation) => Ui.InvokeAsync<object?>(() =>
            {
                cancellation.ThrowIfCancellationRequested();
                SettingsTransactions.Engine.Checkpoint("bgi.create_command_target");
                return CommandCatalog.CreateTarget(Name(a), a.GetProperty("arguments"));
            }), readOnly: false,
            inputSchema: AgentSchemas.Object(("command", command, true), ("arguments", ArgumentSchema.Parse("""{"type":"object","maxProperties":32,"description":"来自当前构造契约的命名参数；复杂输入使用真实 objectId。"}"""), true)),
            guide: Guide("建立命令上下文", "显式构造未注册的宿主对象；服务依赖从原容器取得，未知／歧义参数拒绝。目录查询不偷偷实例化。", true));
        registry.Register("bgi.release_command_target", "command", "释放本次桥创建的上下文",
            (a, _) => Ui.InvokeAsync<object?>(() => CommandTargets.Release(a.GetProperty("objectId").GetString()!)), readOnly: false,
            inputSchema: AgentSchemas.Object(("objectId", ArgumentSchema.Parse("""{"type":"string","minLength":1,"maxLength":64}"""), true)), guide: Guide("释放命令上下文", "结束使用后释放由桥创建的对象，用户已有窗口和对象不能通过此入口释放。", true));
        registry.Register("bgi.create_command_argument", "command", "按当前命令参数类型创建原生参数对象",
            (a, cancellation) => Ui.InvokeAsync<object?>(() =>
            {
                cancellation.ThrowIfCancellationRequested();
                return CommandCatalog.CreateArgument(Name(a), a.GetProperty("arguments"), a.TryGetProperty("contextId",out var context) ? context.GetString() : null);
            }), readOnly: false,
            inputSchema: AgentSchemas.Object(("command",command,true), ("arguments", ArgumentSchema.Parse("""{"type":"object","maxProperties":32}"""), true),
                ("contextId", ArgumentSchema.Parse("""{"type":"string","minLength":1,"maxLength":64}"""), false)),
            guide: Guide("建立参数对象", "仅构造当前命令声明的参数类型；不能指定任意 CLR 类型。泛型从真实 contextId 取得具体类型，已有资源仍使用引用。", true));
    }
    private static string Name(JsonElement a) => a.GetProperty("command").GetString()!.Replace("cmd.", "", StringComparison.Ordinal);
}
