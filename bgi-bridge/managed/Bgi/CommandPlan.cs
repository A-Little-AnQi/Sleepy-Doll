using System.Collections;
using System.Globalization;
using System.Reflection;
using System.Security.Cryptography;
using System.Runtime.CompilerServices;
using System.Text.Json;
using System.Text.Json.Nodes;
using BgiBridge.Catalog;
using BgiBridge.Protocol;

namespace BgiBridge.Bgi;

internal sealed record CommandPlanContract(string Name, Type Owner, Type? Parameter,
    bool Game, bool Destructive, bool Dialog, bool Internal, string? Unavailable, string Fingerprint = "");

internal interface ICommandPlanHost
{
    void Begin() { }
    CommandPlanContract Describe(string name);
    object Owner(CommandPlanContract contract, IReadOnlyList<object> created);
    IReadOnlyList<object> Snapshot(object? root = null);
    IReadOnlyList<object> TakeCreated();
    Task<JsonElement?> Invoke(CommandPlanContract contract, object owner, object? argument, CancellationToken cancellation);
}

/// <summary>固定命令与有界选择条件持久化；原生对象仅在本次运行的宿主对象图中解析。</summary>
internal static class CommandPlan
{
    internal const int MaxSteps = 16;
    private static readonly HashSet<string> Transient = new(StringComparer.OrdinalIgnoreCase)
        { "objectId", "contextId", "controlId", "operationId", "jobId", "requestId", "instanceId", "catalogVersion" };

    private static JsonElement BaseSchema => JsonDocument.Parse("""
    {"type":"object","properties":{"steps":{"type":"array","minItems":1,"maxItems":16,"items":{"type":"object","properties":{"command":{"type":"string","pattern":"^cmd\\.[a-z0-9_]+\\.[a-z0-9_]+$","maxLength":200},"waitSeconds":{"type":"integer","minimum":1,"maximum":3600},"argument":{"type":["string","number","boolean","null"]},"resolveArgument":{"type":"object","properties":{"filters":{"type":"array","minItems":1,"maxItems":8,"items":{"type":"object","properties":{"property":{"type":"string","pattern":"^[A-Za-z_][A-Za-z0-9_]*$","maxLength":80},"op":{"enum":["equals","contains","notEmpty","todayOrLater"]},"value":{"type":["string","number","boolean","null"]}},"required":["property","op"],"additionalProperties":false}},"orderBy":{"type":"string","pattern":"^[A-Za-z_][A-Za-z0-9_]*$","maxLength":80},"descending":{"type":"boolean"}},"required":["filters"],"additionalProperties":false}},"oneOf":[{"required":["command"],"not":{"required":["waitSeconds"]}},{"required":["waitSeconds"],"not":{"anyOf":[{"required":["command"]},{"required":["argument"]},{"required":["resolveArgument"]}]}}],"not":{"required":["argument","resolveArgument"]},"additionalProperties":false}}},"required":["steps"],"additionalProperties":false}
    """).RootElement.Clone();
    internal static JsonElement Schema
    {
        get
        {
            var schema = JsonNode.Parse(BaseSchema.GetRawText())!;
            schema["properties"]!["expectedContractHash"] = JsonNode.Parse("""{"type":"string","pattern":"^[a-f0-9]{64}$","description":"只读计划校验返回的命令契约指纹；定时绑定固定此指纹，运行时变化则拒绝。"}""");
            schema["properties"]!["steps"]!["items"]!["properties"]!["title"] = JsonNode.Parse("""{"type":"string","minLength":1,"maxLength":80,"description":"来自当前命令用途的简短用户可读名称，仅用于展示。"}""");
            schema["properties"]!["steps"]!["items"]!["oneOf"]![0]!["required"] = JsonSerializer.SerializeToNode(new[]{"command","title"});
            var selector = schema["properties"]!["steps"]!["items"]!["properties"]!["resolveArgument"]!;
            schema["properties"]!["steps"]!["items"]!["properties"]!["argument"] = JsonNode.Parse("""{"anyOf":[{"type":"string"},{"type":"number"},{"type":"boolean"},{"type":"null"}]}""");
            selector["properties"]!["filters"]!["items"]!["properties"]!["value"] = JsonNode.Parse("""{"anyOf":[{"type":"string"},{"type":"number"},{"type":"boolean"},{"type":"null"}]}""");
            selector["properties"]!["filters"]!["items"]!["properties"]!["op"]!["enum"] = JsonSerializer.SerializeToNode(new[]{"equals","contains","notEmpty","todayOrLater","notBeforeNow","notBeforeNowOrUnknown","notAfterNow"});
            selector["properties"]!["fromStep"] = JsonNode.Parse("""{"type":"integer","minimum":0,"maximum":15,"description":"只选择该前置步骤在本次计划中创建或更新的对象；0为第一项。重新取回相同代码也以集合引用更新为依据。"}""");
            selector["properties"]!["collection"] = JsonNode.Parse("""{"type":"string","pattern":"^[A-Za-z_][A-Za-z0-9_]*$","maxLength":80,"description":"当前命令所属 ViewModel 的真实公开集合属性；不从其它窗口混选对象。"}""");
            selector["required"] = JsonSerializer.SerializeToNode(new[]{"collection","filters"});
            schema["properties"]!["steps"]!["items"]!["properties"]!["observe"] = selector.DeepClone();
            return JsonSerializer.SerializeToElement(schema);
        }
    }
    internal static string ContractHash(IReadOnlyList<CommandPlanContract> contracts) =>
        Convert.ToHexString(SHA256.HashData(JsonSerializer.SerializeToUtf8Bytes(contracts.Select(c => new
            { c.Name, owner = c.Owner.FullName, parameter = c.Parameter?.FullName, c.Game, c.Destructive, c.Dialog, c.Internal, c.Unavailable, c.Fingerprint })))).ToLowerInvariant();

    internal static IReadOnlyList<CommandPlanContract> Validate(JsonElement arguments, bool game, ICommandPlanHost host)
    {
        ArgumentSchema.Validate(arguments, Schema);
        RejectTransient(arguments);
        var contracts = new List<CommandPlanContract>();
        var waits = 0;
        var stepIndex=0;
        foreach (var step in arguments.GetProperty("steps").EnumerateArray())
        {
            var currentIndex=stepIndex++;
            var hasCommand=step.TryGetProperty("command",out _);
            var hasWait=step.TryGetProperty("waitSeconds",out _);
            if(hasCommand==hasWait || step.TryGetProperty("argument",out _) && step.TryGetProperty("resolveArgument",out _)
                || hasWait && (step.TryGetProperty("argument",out _) || step.TryGetProperty("resolveArgument",out _) || step.TryGetProperty("observe",out _)))
                throw BridgeException.InvalidArgument("步骤只接受一个固定命令或等待，参数只能使用固定值或执行时选择条件之一。");
            if(hasCommand && (!step.TryGetProperty("title",out var title) || string.IsNullOrWhiteSpace(title.GetString())))
                throw BridgeException.InvalidArgument("命令步骤需要用户可读名称。");
            if (step.TryGetProperty("waitSeconds", out var seconds)) { waits += seconds.GetInt32(); continue; }
            var name = step.GetProperty("command").GetString()![4..];
            var contract = host.Describe(name);
            if (contract.Destructive || contract.Dialog || contract.Internal || contract.Unavailable is not null
                || contract.Owner.ContainsGenericParameters || contract.Parameter?.ContainsGenericParameters == true)
                throw BridgeException.InvalidArgument($"命令 cmd.{name} 不满足无交互固定计划条件；读取当前契约选择可等待、无需弹窗的替代入口。");
            if (contract.Game && !game) throw BridgeException.InvalidArgument("此计划包含游戏动作，须使用游戏命令计划入口。");
            if (step.TryGetProperty("resolveArgument", out var selector))
            {
                if (contract.Parameter is null || contract.Parameter == typeof(object) || contract.Parameter.IsValueType || contract.Parameter == typeof(string))
                    throw BridgeException.InvalidArgument("运行时对象选择只用于命令声明的具体原生参数类型。");
                var collection = Property(contract.Owner,selector.GetProperty("collection").GetString()!);
                if(collection.PropertyType == typeof(string) || !typeof(IEnumerable).IsAssignableFrom(collection.PropertyType)) throw BridgeException.InvalidArgument("运行时对象来源必须是当前命令上下文的集合。");
                ValidateFields(contract.Parameter,selector);
                if(selector.TryGetProperty("fromStep",out var from) && (from.GetInt32()>=currentIndex || !arguments.GetProperty("steps")[from.GetInt32()].TryGetProperty("command",out _))) throw BridgeException.InvalidArgument("运行期对象只能引用前面已执行的命令步骤。");
            }
            else if (contract.Parameter is not null)
            {
                if (!step.TryGetProperty("argument", out var value)) throw BridgeException.InvalidArgument("命令缺少明确参数或运行期选择条件。");
                ConvertScalar(value, contract.Parameter);
            }
            else if (step.TryGetProperty("argument", out _)) throw BridgeException.InvalidArgument("该命令不接受参数。");
            if(step.TryGetProperty("observe",out var observation)) {
                var element=ElementType(contract.Owner,observation);
                ValidateFields(element,observation);
                if(!observation.TryGetProperty("fromStep",out var from)||from.GetInt32()>currentIndex||!arguments.GetProperty("steps")[from.GetInt32()].TryGetProperty("command",out _))throw BridgeException.InvalidArgument("业务数据观察须指向本次计划已执行的命令步骤。");
            }
            contracts.Add(contract);
        }
        if (contracts.Count == 0 || waits > 3600) throw BridgeException.InvalidArgument("计划须含命令，总等待不能超过一小时。");
        if(game&&!contracts.Any(contract=>contract.Game))throw BridgeException.InvalidArgument("计划只有宿主读取/界面动作，请使用宿主命令计划，不要启动游戏。");
        if (arguments.TryGetProperty("expectedContractHash", out var expected) && expected.GetString() != ContractHash(contracts))
            throw new BridgeException("PLAN_CONTRACT_CHANGED", "计划中的原生命令契约已变化；未执行，请重新读取和确认绑定。", 409);
        return contracts;
    }

    internal static async Task<object> Run(JsonElement arguments, bool game, ICommandPlanHost host, CancellationToken cancellation)
    {
        var contracts = Validate(arguments, game, host);
        host.Begin();
        var created = new List<object>();
        var results = new List<object>();
        var touched = new Dictionary<int,HashSet<object>>();
        var business = new List<JsonElement?>();
        var observed=new List<object>();
        var at = 0;
        var index=0;
        foreach (var step in arguments.GetProperty("steps").EnumerateArray())
        {
            var currentIndex=index++;
            cancellation.ThrowIfCancellationRequested();
            if (step.TryGetProperty("waitSeconds", out var seconds))
            {
                await Task.Delay(TimeSpan.FromSeconds(seconds.GetInt32()), cancellation);
                results.Add(new { waitedSeconds = seconds.GetInt32() }); continue;
            }
            var contract = contracts[at++];
            var owner = host.Owner(contract, created);
            object? argument = step.TryGetProperty("resolveArgument", out var selector)
                ? Select(Collection(owner,selector).Where(item=>!selector.TryGetProperty("fromStep",out var from) || touched.TryGetValue(from.GetInt32(),out var changed) && changed.Contains(item)), contract.Parameter!, selector, DateTime.Now)
                : step.TryGetProperty("argument", out var scalar) ? ConvertScalar(scalar, contract.Parameter!) : null;
            var tracked=Tracked(host.Snapshot(owner),arguments,contracts);
            var proof=await host.Invoke(contract, owner, argument, cancellation);
            cancellation.ThrowIfCancellationRequested();
            var after=Tracked(host.Snapshot(owner),arguments,contracts);
            touched[currentIndex]=new HashSet<object>(after.Keys.Where(item=>!tracked.TryGetValue(item,out var prior)||prior!=after[item]),ReferenceEqualityComparer.Instance);
            foreach (var value in host.TakeCreated()) if (!created.Any(item => ReferenceEquals(item, value))) created.Add(value);
            if(step.TryGetProperty("observe",out var observation)) {
                var seen=Select(Collection(owner,observation).Where(item=>touched.TryGetValue(observation.GetProperty("fromStep").GetInt32(),out var changed)&&changed.Contains(item)),ElementType(contract.Owner,observation),observation,DateTime.Now);
                observed.Add(Project(seen,observation));
            }
            if(contract.Game)business.Add(proof);
            results.Add(new { command = $"cmd.{contract.Name}", handlerCompleted = true, resolvedAtExecution = selector.ValueKind == JsonValueKind.Object });
        }
        // 异步处理器的 Task 已结束，但原生 ICommand 不提供统一业务回执；不冒充兑换或其它目标完成。
        var statuses=business.Select(proof=>proof?.GetProperty("status").GetString()??"unknown").ToArray();
        var status=statuses.Contains("failed")?"failed":statuses.Length>0&&statuses.All(s=>s=="succeeded")||statuses.Length==0&&observed.Count>0?"succeeded":"unknown";
        var dataOnly=statuses.Length==0&&observed.Count>0;
        return new { steps = results, handlersCompleted = true, businessEvidence=business,observations=observed, verification = new { status, scope = dataOnly?"runtimeData":status=="unknown"?"commandHandlers":"nativeBusinessReceipt", reason = dataOnly?"已观察本次命令创建或更新的匹配数据；仅核验本次获取，不证明新发布、有效期或兑换完成。":status=="unknown"?"计划处理器已完成；尚无完整业务回执，不认定目标成功。":status=="failed"?"宿主本次业务回执包含失败。":"宿主本次业务回执已逐项核验。" } };
    }

    internal static object Select(IEnumerable<object> graph, Type expected, JsonElement selector, DateTime now)
    {
        var candidates = graph.Where(item => expected.IsInstanceOfType(item)).Distinct(ReferenceEqualityComparer.Instance).Take(513).ToList();
        if (candidates.Count > 512) throw BridgeException.InvalidArgument("本次对象集合超过选择上限，须缩小目标范围。");
        foreach (var filter in selector.GetProperty("filters").EnumerateArray())
        {
            var property = Property(expected, filter.GetProperty("property").GetString()!);
            candidates = candidates.Where(item => Matches(property.GetValue(item), filter, now)).ToList();
        }
        if (candidates.Count == 0) throw BridgeException.NotFound("本次运行没有符合条件的对象；未使用旧对象、历史数据或伪造参数。");
        if (selector.TryGetProperty("orderBy", out var order))
        {
            var property = Property(expected, order.GetString()!);
            var sorted = candidates.Select(item => (Item: item, Key: property.GetValue(item))).ToList();
            sorted.Sort((a, b) => Compare(a.Key, b.Key));
            if (!selector.TryGetProperty("descending", out var descending) || descending.GetBoolean()) sorted.Reverse();
            if (sorted.Count > 1 && Compare(sorted[0].Key, sorted[1].Key) == 0) throw new BridgeException("AMBIGUOUS_TARGET", "排序后目标仍不唯一，未猜选对象。", 409);
            return sorted[0].Item;
        }
        if (candidates.Count != 1) throw new BridgeException("AMBIGUOUS_TARGET", "条件匹配多个当前对象，须补充条件或明确排序。", 409);
        return candidates[0];
    }

    private static PropertyInfo Property(Type type, string name)
    {
        var property = type.GetProperty(name, BindingFlags.Instance | BindingFlags.Public);
        if (property is null || !property.CanRead || property.GetMethod?.IsPublic != true || property.GetIndexParameters().Length != 0)
            throw BridgeException.InvalidArgument($"参数类型没有可读取字段 {name}。");
        return property;
    }
    private static void ValidateFields(Type type,JsonElement selector)
    {
        foreach(var filter in selector.GetProperty("filters").EnumerateArray()) {
            var field=Property(type,filter.GetProperty("property").GetString()!);var op=filter.GetProperty("op").GetString();
            if(op is "equals" or "contains"&&!filter.TryGetProperty("value",out _))throw BridgeException.InvalidArgument("匹配条件缺少固定值。");
            if(op is "equals"&&!Scalar(field.PropertyType)||op is "contains"&&field.PropertyType!=typeof(string)
                ||op is "todayOrLater" or "notBeforeNow" or "notBeforeNowOrUnknown" or "notAfterNow"&&field.PropertyType!=typeof(string)&&field.PropertyType!=typeof(DateTime))throw BridgeException.InvalidArgument("选择条件与字段类型不匹配。");
        }
        if(selector.TryGetProperty("orderBy",out var order)&&!Scalar(Property(type,order.GetString()!).PropertyType))throw BridgeException.InvalidArgument("排序只接受标量字段。");
    }
    private static bool Scalar(Type type) => type.IsPrimitive || type.IsEnum || type == typeof(string) || type == typeof(decimal) || type == typeof(DateTime);
    private static IEnumerable<object> Collection(object owner,JsonElement selector)
    {
        if(Property(owner.GetType(),selector.GetProperty("collection").GetString()!).GetValue(owner) is not IEnumerable values) throw BridgeException.InvalidArgument("本次上下文集合不可读取。");
        var current=values.Cast<object>().Take(513).ToArray();
        if(current.Length>512)throw BridgeException.InvalidArgument("本次集合超过512项，须缩小目标范围。");
        return current;
    }
    private static Type ElementType(Type owner,JsonElement selector)
    {
        var type=Property(owner,selector.GetProperty("collection").GetString()!).PropertyType;
        return type.IsArray?type.GetElementType()!:type.GetInterfaces().Append(type).FirstOrDefault(t=>t.IsGenericType&&t.GetGenericTypeDefinition()==typeof(IEnumerable<>))?.GetGenericArguments()[0]??throw BridgeException.InvalidArgument("数据观察需要具有明确元素类型的公开集合。");
    }
    private static object Project(object item,JsonElement selector)
    {
        var values=new Dictionary<string,object?>();
        var names=selector.GetProperty("filters").EnumerateArray().Select(f=>f.GetProperty("property").GetString()!).Distinct().Take(8);
        foreach(var name in names) {
            var value=Property(item.GetType(),name).GetValue(item);
            values[name]=value is null?null:Scalar(value.GetType())?value is string text?text[..Math.Min(text.Length,1000)]:value:value is IEnumerable list?list.Cast<object>().Take(32).Where(v=>v is not null&&Scalar(v.GetType())).ToArray():null;
        }
        return values;
    }
    private static bool Matches(object? value, JsonElement filter, DateTime now) => filter.GetProperty("op").GetString() switch
    {
        "notEmpty" => value switch { string text => !string.IsNullOrWhiteSpace(text), ICollection collection => collection.Count > 0, _ => value is not null },
        "todayOrLater" => TryDate(value, out var date) && date.Date >= now.Date,
        "notBeforeNow" => TryDate(value,out var date) && (DateOnlyValue(value)?date.Date.AddDays(1).AddTicks(-1):date)>=now,
        "notBeforeNowOrUnknown" => !TryDate(value,out var date) || (DateOnlyValue(value)?date.Date.AddDays(1).AddTicks(-1):date)>=now,
        "notAfterNow" => TryDate(value,out var date) && date<=now,
        "contains" => value is string text && filter.TryGetProperty("value", out var part) && part.ValueKind == JsonValueKind.String && text.Contains(part.GetString()!, StringComparison.Ordinal),
        "equals" => filter.TryGetProperty("value", out var expected) && JsonSerializer.SerializeToElement(value).GetRawText() == expected.GetRawText(),
        _ => false,
    };
    private static bool TryDate(object? value, out DateTime date) => DateTime.TryParse(Convert.ToString(value, CultureInfo.InvariantCulture), CultureInfo.InvariantCulture, DateTimeStyles.AllowWhiteSpaces, out date);
    private static bool DateOnlyValue(object? value) => value is string text && System.Text.RegularExpressions.Regex.IsMatch(text,@"^\d{4}[-./]\d{1,2}[-./]\d{1,2}$",System.Text.RegularExpressions.RegexOptions.CultureInvariant,TimeSpan.FromSeconds(1));
    private static Dictionary<object,string> Tracked(IEnumerable<object> graph,JsonElement arguments,IReadOnlyList<CommandPlanContract> contracts)
    {
        var fields=new Dictionary<Type,HashSet<string>>();var at=0;
        foreach(var step in arguments.GetProperty("steps").EnumerateArray()) {
            if(!step.TryGetProperty("command",out _))continue;var contract=contracts[at++];
            foreach(var key in new[]{"resolveArgument","observe"}){
                if(!step.TryGetProperty(key,out var selector))continue;
                var type=key=="observe"?ElementType(contract.Owner,selector):contract.Parameter;
                if(type is null)continue;
                if(!fields.TryGetValue(type,out var names))fields[type]=names=[];
                foreach(var filter in selector.GetProperty("filters").EnumerateArray())names.Add(filter.GetProperty("property").GetString()!);
                if(selector.TryGetProperty("orderBy",out var order))names.Add(order.GetString()!);
            }
        }
        var result=new Dictionary<object,string>(ReferenceEqualityComparer.Instance);
        foreach(var item in graph.Take(4096))foreach(var pair in fields)if(pair.Key.IsInstanceOfType(item)) {
            result[item]=string.Join("|",pair.Value.Order().Select(name=>{
                var value=Property(pair.Key,name).GetValue(item);
                return value is null?"null":Scalar(value.GetType())?JsonSerializer.Serialize(value):$"ref:{RuntimeHelpers.GetHashCode(value)}:{(value is ICollection c?c.Count:0)}";
            }));
        }
        return result;
    }
    private static int Compare(object? left, object? right)
    {
        if (left is null || right is null) return left is null ? right is null ? 0 : -1 : 1;
        if (left is string a && right is string b) return TryDate(a, out var ad) && TryDate(b, out var bd) ? ad.CompareTo(bd) : string.CompareOrdinal(a,b);
        if (left is IComparable comparable && left.GetType() == right.GetType()) return comparable.CompareTo(right);
        throw BridgeException.InvalidArgument("排序字段必须是可比较的当前标量值。");
    }
    private static object? ConvertScalar(JsonElement value, Type expected)
    {
        if (expected != typeof(string) && expected != typeof(bool) && !expected.IsPrimitive && !expected.IsEnum)
            throw BridgeException.InvalidArgument("复杂参数必须在执行时从当前对象图解析。");
        try { return JsonSerializer.Deserialize(value.GetRawText(), expected); }
        catch (Exception error) when (error is JsonException or NotSupportedException) { throw BridgeException.InvalidArgument("命令参数类型不匹配。"); }
    }
    private static void RejectTransient(JsonElement value)
    {
        if (value.ValueKind == JsonValueKind.Object) foreach (var property in value.EnumerateObject()) { if (Transient.Contains(property.Name)) throw BridgeException.InvalidArgument("计划不能保存运行期对象或请求标识；使用执行时选择条件。"); RejectTransient(property.Value); }
        else if (value.ValueKind == JsonValueKind.Array) foreach (var child in value.EnumerateArray()) RejectTransient(child);
        else if (value.ValueKind == JsonValueKind.String && value.GetString()!.Contains("{{", StringComparison.Ordinal)) throw BridgeException.InvalidArgument("命令计划不接受自由参数模板。");
    }
}
