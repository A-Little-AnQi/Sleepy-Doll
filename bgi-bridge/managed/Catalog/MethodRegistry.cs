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

    /// <summary>按用途检索完整目录；禁用项仍保留说明。</summary>
    public IEnumerable<MethodDescriptor> Search(string? query, int limit)
    {
        IEnumerable<MethodDescriptor> source = All;
        if (!string.IsNullOrWhiteSpace(query))
        {
            var needle = query.Trim();
            source = source.Where(x =>
                x.Id.Contains(needle, StringComparison.OrdinalIgnoreCase)
                || x.Summary.Contains(needle, StringComparison.OrdinalIgnoreCase)
                || x.Guide.Title.Contains(needle, StringComparison.OrdinalIgnoreCase)
                || x.Guide.WhenToUse.Any(text => text.Contains(needle, StringComparison.OrdinalIgnoreCase)));
        }

        return source.OrderBy(x => x.Id, StringComparer.Ordinal).Take(limit);
    }

    public int Count => _methods.Count;
}
