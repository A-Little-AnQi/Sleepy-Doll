using System.Net;
using System.Reflection;
using System.Text;
using System.Text.Json;
using System.Collections.Concurrent;
using System.Security.Cryptography;
using BgiBridge.Bgi;
using BgiBridge.Catalog;
using BgiBridge.Jobs;
using BgiBridge.Protocol;

namespace BgiBridge.Hosting;

/// <summary>
/// /bridge/v1 的 HTTP 宿主。宿主是框架依赖应用，TPA 里没有 Microsoft.AspNetCore.*，
/// 只能用 Microsoft.NETCore.App 自带的 HttpListener。
/// </summary>
public sealed class BridgeHost(BridgeConfig config, MethodRegistry registry, JobStore jobs, string version)
{
    private readonly CancellationTokenSource _shutdown = new();
    private HttpListener? _listener;
    private int _enabled = 1;
    private readonly string _instanceId = Guid.NewGuid().ToString("N");
    private readonly string _catalogVersion = Convert.ToHexString(SHA256.HashData(
        JsonSerializer.SerializeToUtf8Bytes(registry.All.OrderBy(method => method.Id), JsonOptions))).ToLowerInvariant();
    private readonly object _requestsGate = new();
    private readonly Dictionary<string, (string Fingerprint, string JobId)> _requests = new(StringComparer.Ordinal);
    private readonly ConcurrentDictionary<string, CancellationTokenSource> _jobCancellation = new();

    public int Port { get; private set; }

    public void Start()
    {
        var (host, port) = config.ResolveEndpoint();
        Port = port;

        var listener = new HttpListener();
        listener.Prefixes.Add($"http://{host}:{port}/");
        listener.Start();
        _listener = listener;

        _ = Task.Run(LoopAsync);
        Log($"已监听 http://{host}:{port}/（{registry.Count} 个方法）");
    }

    public void Stop()
    {
        try
        {
            _shutdown.Cancel();
            _listener?.Stop();
            _listener?.Close();
        }
        catch
        {
            // 卸载路径上不允许抛。
        }
    }

    private async Task LoopAsync()
    {
        while (!_shutdown.IsCancellationRequested)
        {
            HttpListenerContext context;
            try
            {
                context = await _listener!.GetContextAsync().ConfigureAwait(false);
            }
            catch
            {
                // 停止时 GetContextAsync 会抛。正常退出路径。
                return;
            }

            // 每个请求独立处理。
            _ = Task.Run(() => HandleSafelyAsync(context));
        }
    }

    /// <summary>运行在宿主进程里，逃逸的异常会带崩宿主；这里吞掉一切。</summary>
    private async Task HandleSafelyAsync(HttpListenerContext context)
    {
        try
        {
            await HandleAsync(context).ConfigureAwait(false);
        }
        catch (BridgeException ex)
        {
            await WriteJsonAsync(context, ex.HttpStatus, new { code = ex.Code, message = ex.Message })
                .ConfigureAwait(false);
        }
        catch (Exception ex)
        {
            Log($"请求处理异常：{ex}");
            await WriteJsonAsync(context, 500, new { code = "INTERNAL", message = ex.Message })
                .ConfigureAwait(false);
        }
        finally
        {
            try
            {
                context.Response.Close();
            }
            catch
            {
                // 客户端可能已经断开。
            }
        }
    }

    private async Task HandleAsync(HttpListenerContext context)
    {
        var request = context.Request;
        var path = request.Url?.AbsolutePath ?? "/";

        // 本地控制面：只接受回环来源。
        if (!IPAddress.IsLoopback(request.RemoteEndPoint.Address))
            throw new BridgeException("FORBIDDEN", "只接受回环地址的请求。", 403);

        if (!Authorized(request))
            throw BridgeException.Unauthorized();

        if (path == "/bridge/v1/control" && request.HttpMethod == "POST")
        {
            using var body = await ReadJsonAsync(request).ConfigureAwait(false);
            var input = body.RootElement;
            if (input.ValueKind != JsonValueKind.Object || !input.TryGetProperty("enabled", out var enabled) ||
                enabled.ValueKind is not (JsonValueKind.True or JsonValueKind.False))
                throw BridgeException.InvalidArgument("enabled 必须是布尔值。");
            Interlocked.Exchange(ref _enabled, enabled.GetBoolean() ? 1 : 0);
            await WriteJsonAsync(context, 200, new { enabled = Volatile.Read(ref _enabled) != 0 })
                .ConfigureAwait(false);
            return;
        }

        if (path == "/bridge/v1/info" && request.HttpMethod == "GET")
        {
            await WriteJsonAsync(context, 200, Info()).ConfigureAwait(false);
            return;
        }

        if (Volatile.Read(ref _enabled) == 0 && !path.StartsWith("/bridge/v1/catalog", StringComparison.Ordinal))
            throw BridgeException.Disabled("BetterGI 连接已关闭，请在 Sleepy Doll 中开启。");

        if (path == "/bridge/v1/state" && request.HttpMethod == "GET")
        {
            await WriteJsonAsync(context, 200, await StateAsync().ConfigureAwait(false)).ConfigureAwait(false);
            return;
        }

        if (path == "/bridge/v1/host" && request.HttpMethod == "GET")
        {
            await WriteJsonAsync(context, 200, HostPaths()).ConfigureAwait(false);
            return;
        }

        if (path == "/bridge/v1/catalog" && request.HttpMethod == "GET")
        {
            var query = request.QueryString["q"];
            var limit = Math.Clamp(ParseInt(request.QueryString["limit"], 50), 1, 100);
            var offset = Math.Max(0, ParseInt(request.QueryString["offset"], 0));
            await WriteJsonAsync(context, 200, Catalog(query, limit, offset, request.QueryString["group"])).ConfigureAwait(false);
            return;
        }

        if (path.StartsWith("/bridge/v1/catalog/", StringComparison.Ordinal) && request.HttpMethod == "GET")
        {
            var id = Uri.UnescapeDataString(path["/bridge/v1/catalog/".Length..]);
            await WriteJsonAsync(context, 200, Describe(id)).ConfigureAwait(false);
            return;
        }

        if (path == "/bridge/v1/invoke" && request.HttpMethod == "POST")
        {
            await InvokeAsync(context).ConfigureAwait(false);
            return;
        }

        if (path.StartsWith("/bridge/v1/jobs/", StringComparison.Ordinal))
        {
            var rest = path["/bridge/v1/jobs/".Length..];
            var cancel = rest.EndsWith("/cancel", StringComparison.Ordinal);
            var id = cancel ? rest[..^"/cancel".Length] : rest;

            if (cancel && request.HttpMethod == "POST")
            {
                await WriteJsonAsync(context, 200, Cancel(id)).ConfigureAwait(false);
                return;
            }

            if (!cancel && request.HttpMethod == "GET")
            {
                var job = jobs.Get(id) ?? throw BridgeException.NotFound($"没有这个 Job：{id}");
                await WriteJsonAsync(context, 200, job.ToWire()).ConfigureAwait(false);
                return;
            }
        }

        throw BridgeException.NotFound($"未知端点：{request.HttpMethod} {path}");
    }

    private bool Authorized(HttpListenerRequest request)
    {
        if (string.IsNullOrEmpty(config.Token)) return false;
        var header = request.Headers["Authorization"];
        if (string.IsNullOrEmpty(header)) return false;
        const string Prefix = "Bearer ";
        if (!header.StartsWith(Prefix, StringComparison.Ordinal)) return false;
        var presented = header[Prefix.Length..].Trim();
        // 定长比较，避免按字符提前返回。
        return CryptographicEquals(presented, config.Token);
    }

    private static bool CryptographicEquals(string a, string b) =>
        System.Security.Cryptography.CryptographicOperations.FixedTimeEquals(
            Encoding.UTF8.GetBytes(a), Encoding.UTF8.GetBytes(b));

    private object Info() => new
    {
        protocol = "bridge/v1",
        protocolVersion = "1",
        bridge = version,
        catalogVersion = _catalogVersion,
        instanceId = _instanceId,
        processId = Environment.ProcessId,
        enabled = Volatile.Read(ref _enabled) != 0,
        features = new[] { "catalog", "invoke", "jobs", "state", "control", "idempotency", "agentGuides", "settingsTransactions" },
        methods = registry.Count,
        // 已知的能力边界。
        limitations = new[]
        {
            "detached-start-signal-approximate",
            "settings-xml-documentation-may-be-absent",
        },
    };

    private async Task<object> StateAsync()
    {
        var (ready, detail) = await Task.Run(Bgi.BridgeState.Capture).ConfigureAwait(false);
        return new
        {
            instanceId = _instanceId,
            snapshotId = Guid.NewGuid().ToString("N"),
            observedAt = DateTimeOffset.UtcNow,
            runtime = detail,
            bridgeReady = ready,
            host = HostPaths(),
        };
    }

    /// <summary>
    /// 宿主安装目录与用户目录：桥运行在 BetterGI 进程内，<c>AppContext.BaseDirectory</c>
    /// 就是安装目录，用户配置在 <c>User\</c> 下。
    /// </summary>
    private static object HostPaths()
    {
        var install = AppContext.BaseDirectory.TrimEnd(Path.DirectorySeparatorChar);
        return new
        {
            installPath = install,
            userPath = Path.GetFullPath(Path.Combine(install, "User")),
        };
    }

    private string? Unavailable(MethodDescriptor descriptor) =>
        Volatile.Read(ref _enabled) == 0 ? "桥连接开关已关闭。"
        : descriptor.UnavailableReason is { } contractReason
            ? contractReason + (!config.IsMethodEnabled(descriptor.Id, descriptor.Group)
                ? " 此外，当前策略已关闭该接口或所属分组。" : "")
        : !config.IsMethodEnabled(descriptor.Id, descriptor.Group) ? "接口所属分组或单接口开关已关闭。"
        : null;

    private object Catalog(string? query, int limit, int offset, string? group)
    {
        var all = registry.Search(query, int.MaxValue)
            .Where(method => string.IsNullOrEmpty(group) || method.Group.Equals(group, StringComparison.OrdinalIgnoreCase)).ToArray();
        var items = all.Skip(offset).Take(limit)
            .Select(method => method.Discovery(Unavailable(method) is null, Unavailable(method), _catalogVersion)).ToArray();
        // 空结果时提示调用方先确认证据源。
        var hint = all.Length == 0 && !string.IsNullOrWhiteSpace(query) && string.IsNullOrEmpty(group)
            ? "没有接口命中。先判断目标是否其实是 User 目录中的配置组、路线或脚本；若确定属于 BetterGI 接口，只用一个核心词重试一次。"
            : null;
        return new
        {
            catalogVersion = _catalogVersion, total = all.Length, offset,
            nextOffset = offset + items.Length < all.Length ? (int?)(offset + items.Length) : null,
            groups = registry.All.GroupBy(method => method.Group).OrderBy(group => group.Key)
                .Select(group => new { id = group.Key, count = group.Count() }).ToArray(),
            items,
            hint,
        };
    }

    private object Describe(string id)
    {
        if (!registry.TryGet(id, out var descriptor, out _)) throw BridgeException.NotFound($"没有这个方法：{id}");
        return new
        {
            methodId = descriptor.Id, displayName = descriptor.Guide.Title, group = descriptor.Group,
            instanceId = _instanceId,
            summary = descriptor.Guide.Purpose, guide = descriptor.Guide,
            inputSchema = descriptor.InputSchema, outputSchema = descriptor.OutputSchema,
            effect = descriptor.Effect,
            executionMode = descriptor.ReadOnly ? "inline" : "job",
            concurrency = descriptor.ReadOnly ? "parallel" : "gameExclusive",
            cancellation = !descriptor.ReadOnly && descriptor.Group != "settings" ? "waitOnly" : "cooperative",
            resultReliability = descriptor.Group == "settings" ? "readBackVerified" : descriptor.ReadOnly ? "immediate" : "completionOnly",
            requiresConfirmation = !descriptor.ReadOnly,
            callable = Unavailable(descriptor) is null, unavailableReason = Unavailable(descriptor),
            catalogVersion = _catalogVersion,
            errors = new[] { "UNAUTHORIZED", "NOT_CALLABLE", "INVALID_ARGUMENT", "INSTANCE_MISMATCH", "CATALOG_MISMATCH",
                "CONFIG_CONFLICT", "PLAN_EXPIRED", "COMMIT_REVERTED", "RECOVERY_REQUIRED", "INVALID_STATE", "TIMEOUT" },
        };
    }

    private async Task InvokeAsync(HttpListenerContext context)
    {
        using var body = await ReadJsonAsync(context.Request).ConfigureAwait(false);
        var root = body.RootElement;
        if (root.ValueKind != JsonValueKind.Object) throw BridgeException.InvalidArgument("请求体必须是对象。");
        foreach (var name in new[] { "instanceId", "catalogVersion", "requestId" })
            if (root.TryGetProperty(name, out var field) && field.ValueKind != JsonValueKind.String)
                throw BridgeException.InvalidArgument($"{name} 必须是字符串。");
        var methodId = root.TryGetProperty("methodId", out var method) && method.ValueKind == JsonValueKind.String ? method.GetString() : null;
        if (string.IsNullOrWhiteSpace(methodId)) throw BridgeException.InvalidArgument("请求体缺少 methodId。");
        if (!registry.TryGet(methodId, out var descriptor, out var handler)) throw BridgeException.NotFound($"没有这个方法：{methodId}");
        if (Unavailable(descriptor) is { } unavailable) throw new BridgeException("NOT_CALLABLE", unavailable, 409);
        var arguments = root.TryGetProperty("arguments", out var supplied) ? supplied.Clone() : ArgumentSchema.Parse("{}");
        ArgumentSchema.Validate(arguments, descriptor.InputSchema);
        if (root.TryGetProperty("instanceId", out var instance) && instance.ValueKind == JsonValueKind.String && instance.GetString() != _instanceId)
            throw new BridgeException("INSTANCE_MISMATCH", "BetterGI 实例已变化，请重新读取目录。", 409);
        if (root.TryGetProperty("catalogVersion", out var catalog) && catalog.ValueKind == JsonValueKind.String && catalog.GetString() != _catalogVersion)
            throw new BridgeException("CATALOG_MISMATCH", "接口契约版本已变化，请重新读取说明。", 409);

        if (descriptor.ReadOnly)
        {
            var result = await handler(arguments, _shutdown.Token).ConfigureAwait(false);
            await WriteJsonAsync(context, 200, new { methodId, result }).ConfigureAwait(false);
            return;
        }
        if (!root.TryGetProperty("instanceId", out instance) || instance.GetString() != _instanceId
            || !root.TryGetProperty("catalogVersion", out catalog) || catalog.GetString() != _catalogVersion)
            throw new BridgeException("CATALOG_MISMATCH", "写接口要求当前 instanceId 与 catalogVersion。", 409);
        var key = context.Request.Headers["Idempotency-Key"]
            ?? (root.TryGetProperty("requestId", out var requestId) ? requestId.GetString() : null);
        if (string.IsNullOrWhiteSpace(key) || key.Length > 128) throw BridgeException.InvalidArgument("写接口要求不超过 128 字符的 requestId 或 Idempotency-Key。");
        var fingerprint = ValueContract.Version(JsonSerializer.SerializeToElement(new { methodId, arguments, instanceId = _instanceId, catalogVersion = _catalogVersion }));
        JobSnapshot job;
        bool created;
        lock (_requestsGate)
        {
            if (_requests.TryGetValue(key, out var prior))
            {
                if (prior.Fingerprint != fingerprint) throw new BridgeException("IDEMPOTENCY_CONFLICT", "同一请求标识不能用于不同内容。", 409);
                job = jobs.Get(prior.JobId) ?? throw new BridgeException("JOB_EXPIRED", "原请求记录已过期，不会重新执行。", 410);
                created = false;
            }
            else
            {
                if (_requests.Count >= 10000) throw new BridgeException("QUEUE_FULL", "当前实例请求记录已达上限，请核对并重启 BetterGI。", 429);
                job = jobs.Create(methodId);
                _requests.Add(key, (fingerprint, job.JobId));
                created = true;
            }
        }
        if (created)
        {
            var cancellation = CancellationTokenSource.CreateLinkedTokenSource(_shutdown.Token);
            _jobCancellation[job.JobId] = cancellation;
            _ = Task.Run(async () =>
            {
                try
                {
                    cancellation.Token.ThrowIfCancellationRequested();
                    jobs.MarkRunning(job.JobId);
                    var result = await handler(arguments, cancellation.Token).ConfigureAwait(false);
                    var verified = descriptor.Group == "settings" && JsonSerializer.SerializeToElement(result).TryGetProperty("verified", out var check) && check.ValueKind == JsonValueKind.True;
                    jobs.MarkCompleted(job.JobId, result, verified);
                }
                catch (OperationCanceledException) { jobs.MarkCancelled(job.JobId); }
                catch (Exception error) { jobs.MarkFailed(job.JobId, BridgeExceptionTranslator.Describe(error)); }
                finally { _jobCancellation.TryRemove(job.JobId, out _); cancellation.Dispose(); }
            });
        }
        await WriteJsonAsync(context, 202, new { requestId = key, jobId = job.JobId, state = job.State, replayed = !created }).ConfigureAwait(false);
    }

    private object Cancel(string id)
    {
        var job = jobs.Get(id) ?? throw BridgeException.NotFound($"没有这个 Job：{id}");
        if (JobState.IsTerminal(job.State)) return new { jobId = id, state = job.State, cancelled = job.State == JobState.Cancelled };
        if (_jobCancellation.TryGetValue(id, out var cancellation))
        {
            try { cancellation.Cancel(); } catch (ObjectDisposedException) { }
        }
        return new
        {
            jobId = id, state = jobs.Get(id)?.State, cancelled = false, cancellationRequested = true,
            note = "请求仅作用于此桥 Job；已开始的 BetterGI 命令可能继续运行，必须继续查询终态。",
        };
    }

    // ---------- 编解码 ----------

    private async Task<JsonDocument> ReadJsonAsync(HttpListenerRequest request)
    {
        // 限制请求体大小。
        const int MaxBody = 4 * 1024 * 1024;
        using var buffer = new MemoryStream();
        var chunk = new byte[8192];
        int count;
        while ((count = await request.InputStream.ReadAsync(chunk).ConfigureAwait(false)) != 0)
        {
            if (buffer.Length + count > MaxBody) throw BridgeException.InvalidArgument("请求体过大。");
            buffer.Write(chunk, 0, count);
        }
        buffer.Position = 0;
        try
        {
            return await JsonDocument.ParseAsync(buffer).ConfigureAwait(false);
        }
        catch (JsonException ex)
        {
            throw BridgeException.InvalidArgument($"请求体不是合法 JSON：{ex.Message}");
        }
    }

    private static async Task WriteJsonAsync(HttpListenerContext context, int status, object payload)
    {
        var response = context.Response;
        response.StatusCode = status;
        response.ContentType = "application/json; charset=utf-8";
        var bytes = JsonSerializer.SerializeToUtf8Bytes(payload, JsonOptions);
        response.ContentLength64 = bytes.Length;
        await response.OutputStream.WriteAsync(bytes).ConfigureAwait(false);
    }

    private static int ParseInt(string? text, int fallback) =>
        int.TryParse(text, out var value) && value > 0 ? value : fallback;

    internal static readonly JsonSerializerOptions JsonOptions = new()
    {
        PropertyNamingPolicy = JsonNamingPolicy.CamelCase,
        DefaultIgnoreCondition = System.Text.Json.Serialization.JsonIgnoreCondition.WhenWritingNull,
    };

    /// <summary>只写 stdout，不写宿主日志文件。</summary>
    internal static void Log(string message)
    {
        try
        {
            Console.WriteLine($"[bgi-bridge] {message}");
        }
        catch
        {
            // 宿主可能没有控制台。
        }
    }
}

internal static class BridgeExceptionTranslator
{
    public static string Describe(Exception ex) => ex switch
    {
        BridgeException bridge => $"{bridge.Code}: {bridge.Message}",
        // 反射调用宿主命令时异常被包成 TargetInvocationException，报内层的原因。
        TargetInvocationException wrapped => Describe(Reflect.Root(wrapped)),
        _ => $"{ex.GetType().Name}: {ex.Message}",
    };
}
