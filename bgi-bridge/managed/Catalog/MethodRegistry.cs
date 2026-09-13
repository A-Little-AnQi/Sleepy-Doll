using System.Collections.Concurrent;
using System.Text.Json;
using BgiBridge.Protocol;

namespace BgiBridge.Catalog;

/// <summary>方法处理器；返回值序列化成 invoke 或 Job 的结果。</summary>
public delegate Task<object?> MethodHandler(JsonElement arguments, CancellationToken cancellation);

/// <summary>
/// 方法声明。Destructive 会映射成 catalog 的 effect:gameWrite + requiresConfirmation，
/// 交给宿主侧的审批闸门。
/// </summary>
public sealed record MethodDescriptor(
    string Id,
    string Group,
    string Summary,
    bool ReadOnly,
    bool Destructive,
    JsonElement InputSchema)
{
    public required AgentGuide Guide { get; init; }
    public string? UnavailableReason { get; init; }
    public bool RequiresGameReady { get; init; }
    public string Effect => ReadOnly ? "readOnly" : Group == "settings" ? "configurationWrite"
        : RequiresGameReady ? "gameWrite" : "hostCommand";
    public JsonElement OutputSchema => AgentSchemas.Output(Id, Guide.ResultMeaning);

    // Discovery must explain the API before the agent chooses which detail to load.
    public object Discovery(bool callable, string? unavailableReason, string catalogVersion) => new
    {
        methodId = Id, displayName = Guide.Title, group = Group, summary = Guide.Purpose,
        whenToUse = Guide.WhenToUse, parameters = Parameters(), sideEffects = Guide.SideEffects,
        effect = Effect, callable, unavailableReason, catalogVersion,
        executionMode = ReadOnly ? "inline" : "job", requiresConfirmation = !ReadOnly,
        documentationSource = Guide.DocumentationSource,
    };

    private object[] Parameters()
    {
        if (!InputSchema.TryGetProperty("properties", out var properties)) return [];
        var required = InputSchema.TryGetProperty("required", out var names)
            ? names.EnumerateArray().Select(name => name.GetString()).ToHashSet() : [];
        return properties.EnumerateObject().Select(property => (object)new
        {
            name = property.Name,
            required = required.Contains(property.Name),
            type = property.Value.TryGetProperty("type", out var type) ? type.GetString() : "见详细契约",
            description = property.Value.TryGetProperty("description", out var description)
                ? description.GetString() : "参数约束见此接口的完整说明。",
        }).ToArray();
    }
}

/// <summary>catalog 前缀分区：bgi. 显式方法、cmd. 命令、setting. 设置项。</summary>
public sealed class MethodRegistry
{
    private readonly ConcurrentDictionary<string, (MethodDescriptor Descriptor, MethodHandler Handler)> _methods =
        new(StringComparer.OrdinalIgnoreCase);

    public void Register(
        string id,
        string group,
        string summary,
        MethodHandler handler,
        bool readOnly = true,
        bool destructive = false,
        JsonElement? inputSchema = null,
        AgentGuide? guide = null,
        string? unavailableReason = null,
        bool requiresGameReady = false)
    {
        if (string.IsNullOrWhiteSpace(id)) throw new ArgumentException("方法 id 不能为空。", nameof(id));
        var documentation = guide ?? AgentGuides.For(id);
        var descriptor = new MethodDescriptor(id, group, documentation.Purpose, readOnly, destructive,
            inputSchema ?? AgentSchemas.Input(id)) { Guide = documentation, UnavailableReason = unavailableReason, RequiresGameReady = requiresGameReady };
        if (!_methods.TryAdd(id, (descriptor, handler)))
            throw new InvalidOperationException($"方法 id 重复：{id}");
    }

    public bool TryGet(string id, out MethodDescriptor descriptor, out MethodHandler handler)
    {
        if (_methods.TryGetValue(id, out var entry))
        {
            (descriptor, handler) = (entry.Descriptor, entry.Handler);
            return true;
        }
        (descriptor, handler) = (null!, null!);
        return false;
    }

    public IEnumerable<MethodDescriptor> All => _methods.Values.Select(x => x.Descriptor);

    /// <summary>词间分隔符。中英文混排的查询很常见，全角空格也在内。</summary>
    private static readonly char[] TermSeparators =
        [' ', '\t', '　', ',', '，', '、', '/', '|', ';', '；'];

    /// <summary>
    /// 按用途检索完整目录；禁用项仍保留说明。
    /// </summary>
    /// <remarks>
    /// <para>
    /// 查询先切词，命中任一词即入选，按字段加权排序。不做整串匹配：调用方传的
    /// 是「调度器 scheduler」这类多词查询，目录里不会有字段包含这个完整串。
    /// </para>
    /// <para>
    /// `+词` 表示必须命中，其余词只参与排序。与 Claude Code 的 ToolSearch 一致：
    /// 名称命中权重最高，说明文字最低 —— 名中最能确定一个接口是做什么的。
    /// </para>
    /// </remarks>
    public IEnumerable<MethodDescriptor> Search(string? query, int limit)
    {
        var ordered = All.OrderBy(x => x.Id, StringComparer.Ordinal);
        if (string.IsNullOrWhiteSpace(query))
        {
            return ordered.Take(limit);
        }

        var needle = query.Trim();
        // 调用方常直接传接口名，此时精确返回，不必让它再描述一遍。
        var exact = All.FirstOrDefault(x => x.Id.Equals(needle, StringComparison.OrdinalIgnoreCase));
        if (exact is not null)
        {
            return [exact];
        }

        var terms = needle
            .Split(TermSeparators, StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries)
            .Distinct(StringComparer.OrdinalIgnoreCase)
            .ToArray();
        if (terms.Length == 0)
        {
            return ordered.Take(limit);
        }
        var required = terms.Where(IsRequired).Select(term => term[1..]).ToArray();
        var scoring = required.Length > 0 ? [.. required, .. terms.Where(term => !IsRequired(term))] : terms;

        var candidates = required.Length > 0
            ? All.Where(method => required.All(term => Score(method, term) > 0))
            : All;

        return candidates
            .Select(method => (method, score: scoring.Sum(term => Score(method, term))))
            .Where(candidate => candidate.score > 0)
            .OrderByDescending(candidate => candidate.score)
            .ThenBy(candidate => candidate.method.Id, StringComparer.Ordinal)
            .Select(candidate => candidate.method)
            .Take(limit);
    }

    private static bool IsRequired(string term) => term.Length > 1 && term[0] == '+';

    /// <summary>命中得分。字段权重递减：名称 &gt; 标题 &gt; 用途 &gt; 使用时机。</summary>
    private static int Score(MethodDescriptor method, string term)
    {
        var score = 0;
        if (NameParts(method.Id).Any(part => part.Equals(term, StringComparison.OrdinalIgnoreCase)))
        {
            score += 10;
        }
        else if (method.Id.Contains(term, StringComparison.OrdinalIgnoreCase))
        {
            score += 5;
        }
        if (method.Guide.Title.Contains(term, StringComparison.OrdinalIgnoreCase))
        {
            score += 6;
        }
        if (method.Summary.Contains(term, StringComparison.OrdinalIgnoreCase))
        {
            score += 3;
        }
        if (method.Guide.WhenToUse.Any(text => text.Contains(term, StringComparison.OrdinalIgnoreCase)))
        {
            score += 1;
        }
        return score;
    }

    /// <summary>
    /// 把 id 切成可比词片：`setting.OtherConfig+AutoRestart.FailureCount` 切成
    /// setting / OtherConfig / AutoRestart / FailureCount。调用方按「重启」的
    /// 英文词找接口时，靠的就是这一步命中 AutoRestart。
    /// </summary>
    private static string[] NameParts(string id) => id
        .Split(['.', '_', '-', '+', '/', ':'], StringSplitOptions.RemoveEmptyEntries)
        .SelectMany(part => CamelBoundaries.Split(part))
        .Where(part => part.Length > 0)
        .ToArray();

    private static readonly System.Text.RegularExpressions.Regex CamelBoundaries =
        new(@"(?<=[a-z0-9])(?=[A-Z])", System.Text.RegularExpressions.RegexOptions.Compiled);

    public int Count => _methods.Count;
}
