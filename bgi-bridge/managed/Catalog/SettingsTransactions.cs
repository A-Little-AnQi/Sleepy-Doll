using System.Reflection;
using System.Security.AccessControl;
using System.Security.Cryptography;
using System.Security.Principal;
using System.Text;
using System.Text.Json;
using System.Text.Json.Nodes;
using BgiBridge.Bgi;
using BgiBridge.Protocol;

namespace BgiBridge.Catalog;

public sealed record SettingChange(string Path, JsonElement Value, string ExpectedVersion);
public sealed record StoredSettingChange(string Path, JsonElement Before, JsonElement After,
    bool BeforeDiskPresent, JsonElement BeforeDisk, JsonElement AfterDisk, bool AfterDiskPresent = true);
public sealed class SettingChangeRecord
{
    public int Format { get; set; } = 1;
    public string ChangeId { get; set; } = "";
    public string ConfigPath { get; set; } = "";
    public string State { get; set; } = "prepared";
    public string CreatedAt { get; set; } = DateTimeOffset.UtcNow.ToString("O");
    public string? ParentChangeId { get; set; }
    public string BeforeFileBase64 { get; set; } = "";
    public List<StoredSettingChange> Changes { get; set; } = [];
    public string? Error { get; set; }
    public string? Operation { get; set; }
    public string? HostExecutable { get; set; }
    public string BackupDigest { get; set; } = "";
}

/// <summary>只处理设置事务；界面命令不进入此引擎。</summary>
public sealed class SettingsTransactionEngine(
    Func<object> rootProvider, Func<string> configPathProvider,
    Func<JsonSerializerOptions> optionsProvider, string recordDirectory,
    Func<string?>? executableProvider = null, Action? validateMutation = null)
{
    private sealed record Plan(string Id, DateTimeOffset Expires, string RootVersion, string FileVersion, List<SettingChange> Changes);
    private readonly Dictionary<string, Plan> _plans = new(StringComparer.Ordinal);
    private readonly object _gate = new();
    private static readonly JsonSerializerOptions RecordJson = new() { PropertyNamingPolicy = JsonNamingPolicy.CamelCase, WriteIndented = true };

    private string ConfigPath => Path.GetFullPath(configPathProvider());
    private JsonElement RootSnapshot(object root) => ValueContract.SerializeBounded(root, root.GetType(), optionsProvider(), 16 * 1024 * 1024);
    private static string Digest(byte[] bytes) => System.Convert.ToHexString(SHA256.HashData(bytes));
    private static byte[] DiskBytes(string path)
    {
        if (!File.Exists(path)) return [];
        if (new FileInfo(path).Length > 16 * 1024 * 1024) throw new BridgeException("CONFIG_TOO_LARGE", "配置文件超过安全处理大小，拒绝自动修改。", 409);
        var bytes = File.ReadAllBytes(path);
        if (bytes.Length == 0) throw new BridgeException("INVALID_CONFIG_FILE", "BetterGI 配置文件为空，拒绝覆盖。请先人工核对。", 409);
        return bytes;
    }
    private static bool Equal(JsonElement left, JsonElement right) => ValueContract.Canonical(left) == ValueContract.Canonical(right);
    private static JsonObject ObjectFrom(byte[] bytes, JsonElement fallback) => bytes.Length == 0
        ? JsonNode.Parse(fallback.GetRawText())!.AsObject()
        : JsonNode.Parse(bytes, documentOptions: new JsonDocumentOptions { AllowTrailingCommas = true, CommentHandling = JsonCommentHandling.Skip })!.AsObject();
    private static JsonElement NodeValue(JsonNode? node) => node is null ? ArgumentSchema.Parse("null") : JsonSerializer.SerializeToElement(node);

    public object Preview(IReadOnlyList<SettingChange> requested)
    {
        lock (_gate)
        {
            if (requested.Count is < 1 or > 20) throw BridgeException.InvalidArgument("每次必须提交 1 到 20 项变更。");
            var root = rootProvider();
            var entries = SettingsCatalog.Build(root).ToDictionary(item => item.Path, StringComparer.OrdinalIgnoreCase);
            var seen = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
            var changes = new List<SettingChange>();
            foreach (var change in requested)
            {
                if (!entries.TryGetValue(change.Path, out var entry)) throw BridgeException.NotFound($"配置目录中没有 {change.Path}。");
                if (!entry.Writable) throw BridgeException.InvalidArgument(entry.WriteRestriction ?? "该配置只读。");
                if (!seen.Add(entry.Path)) throw BridgeException.InvalidArgument($"重复设置路径：{entry.Path}。");
                if (entry.ValueVersion != change.ExpectedVersion) throw new BridgeException("CONFIG_CONFLICT", $"设置 {entry.Path} 已变化，请重新读取。", 409);
                if (entry.Sensitive && change.Value.ValueKind == JsonValueKind.String && change.Value.GetString() == "***REDACTED***")
                    throw BridgeException.InvalidArgument("不能提交遮蔽占位值，请提供用户授权的新值。");
                var (owner, property) = SettingsCatalog.Resolve(root, entry.Path);
                var converted = ValueContract.Convert(change.Value, property, entry.Path);
                changes.Add(change with { Path = entry.Path, Value = ValueContract.Snapshot(converted, property.PropertyType) });
            }
            var snapshot = RootSnapshot(root);
            var disk = DiskBytes(ConfigPath);
            _ = ObjectFrom(disk, snapshot); // 磁盘配置不合法时在修改前抛出。
            var plan = new Plan(Guid.NewGuid().ToString("N"), DateTimeOffset.UtcNow.AddMinutes(10),
                ValueContract.Version(snapshot), Digest(disk), changes);
            foreach (var expired in _plans.Values.Where(plan => plan.Expires < DateTimeOffset.UtcNow).ToList()) _plans.Remove(expired.Id);
            if (_plans.Count >= 200) throw new BridgeException("QUEUE_FULL", "待提交预览过多，请稍后重试。", 429);
            _plans.Add(plan.Id, plan);
            return new
            {
                planId = plan.Id, expiresAt = plan.Expires, changesConfig = false,
                differences = changes.Select(change => new
                {
                    path = change.Path, before = entries[change.Path].CurrentValue,
                    after = SettingsCatalog.IsSensitive(change.Path) ? (object)"***REDACTED***" : change.Value,
                    rollback = "提交后可按 changeId 回退；目标项被其他来源修改时拒绝覆盖。",
                }),
            };
        }
    }

    public object Commit(string planId, CancellationToken cancellation = default)
    {
        lock (_gate)
        {
            if (File.Exists(RecordPath(planId)))
            {
                var previous = ReadRecord(planId);
                if (previous.State == "committed")
                {
                    VerifyRecordedValues(previous, after: true);
                    return Public(previous);
                }
                throw new BridgeException("RECOVERY_REQUIRED", "该计划已有未完成或回退记录，请先核对变更历史。", 409);
            }
            if (!_plans.TryGetValue(planId, out var plan) || plan.Expires < DateTimeOffset.UtcNow)
                throw new BridgeException("PLAN_EXPIRED", "预览计划不存在或已过期，请重新预览。", 409);
            cancellation.ThrowIfCancellationRequested();
            validateMutation?.Invoke();
            var root = rootProvider();
            if (ValueContract.Version(RootSnapshot(root)) != plan.RootVersion || Digest(DiskBytes(ConfigPath)) != plan.FileVersion)
                throw new BridgeException("CONFIG_CONFLICT", "预览后配置已变化，未提交任何修改。", 409);
            var result = Apply(plan.Id, root, plan.Changes, null);
            _plans.Remove(plan.Id);
            return Public(result);
        }
    }

    public object Rollback(string id, CancellationToken cancellation = default)
    {
        lock (_gate)
        {
            var record = ReadRecord(id);
            if (record.State == "rolledBack")
            {
                VerifyRecordedValues(record, after: false);
                return Public(record);
            }
            if (record.State is not ("committed" or "prepared" or "recoveryRequired"))
                throw new BridgeException("INVALID_STATE", "该变更没有可以回退的已提交内容。", 409);
            cancellation.ThrowIfCancellationRequested();
            validateMutation?.Invoke();
            var root = rootProvider();
            var disk = ObjectFrom(DiskBytes(ConfigPath), RootSnapshot(root));
            var changes = new List<SettingChange>();
            var entries = SettingsCatalog.Build(root).ToDictionary(item => item.Path, StringComparer.Ordinal);
            foreach (var saved in record.Changes)
            {
                var entry = entries.GetValueOrDefault(saved.Path)
                    ?? throw BridgeException.NotFound($"当前 BetterGI 不再提供 {saved.Path}。");
                var (owner, property) = SettingsCatalog.Resolve(root, saved.Path);
                var current = ValueContract.Snapshot(property.GetValue(owner), property.PropertyType);
                var present = TryGet(disk, saved.Path, out var node);
                if (!Equal(current, saved.After) || present != saved.AfterDiskPresent || present && !Equal(NodeValue(node), saved.AfterDisk))
                    throw new BridgeException("CONFIG_CONFLICT", $"设置 {saved.Path} 在提交后被另行修改，回退不会覆盖它。", 409);
                changes.Add(new(saved.Path, saved.Before, entry.ValueVersion));
            }
            var inverse = Apply(Guid.NewGuid().ToString("N"), root, changes, record);
            record.State = "rolledBack";
            SaveRecord(record);
            return new { changeId = record.ChangeId, rollbackChangeId = inverse.ChangeId, verified = true, state = "rolledBack" };
        }
    }

    public object History() => new
    {
        changes = Directory.EnumerateFiles(recordDirectory, "config-change-*.json")
            .OrderByDescending(File.GetLastWriteTimeUtc).Take(100)
            .Select(path => { try { return ReadRecord(Path.GetFileNameWithoutExtension(path)["config-change-".Length..]); } catch { return null; } })
            .Where(record => record is not null).Select(record => Public(record!)).ToArray(),
    };
    public object Describe(string id) => Public(ReadRecord(id));

    public object Checkpoint(string operation)
    {
        lock (_gate)
        {
            var snapshot = RootSnapshot(rootProvider());
            var bytes = DiskBytes(ConfigPath);
            _ = ObjectFrom(bytes, snapshot);
            var record = new SettingChangeRecord
            {
                ChangeId = Guid.NewGuid().ToString("N"), ConfigPath = ConfigPath,
                State = "commandCheckpoint", Operation = operation, HostExecutable = executableProvider?.Invoke(),
                BeforeFileBase64 = System.Convert.ToBase64String(bytes.Length == 0 ? Encoding.UTF8.GetBytes(snapshot.GetRawText()) : bytes),
            };
            SaveRecord(record, create: true);
            return Public(record);
        }
    }

    private SettingChangeRecord Apply(string id, object root, IReadOnlyList<SettingChange> changes, SettingChangeRecord? inverse)
    {
        var path = ConfigPath;
        var beforeSnapshot = RootSnapshot(root);
        var beforeFile = DiskBytes(path);
        var disk = ObjectFrom(beforeFile, beforeSnapshot);
        var prepared = new List<(object Owner, PropertyInfo Property, object? Before, object? After, string Path)>();
        var entries = SettingsCatalog.Build(root).ToDictionary(item => item.Path, StringComparer.Ordinal);
        var record = new SettingChangeRecord { ChangeId = id, ConfigPath = path, BeforeFileBase64 = System.Convert.ToBase64String(beforeFile.Length == 0 ? Encoding.UTF8.GetBytes(beforeSnapshot.GetRawText()) : beforeFile), ParentChangeId = inverse?.ChangeId, HostExecutable = executableProvider?.Invoke() };
        foreach (var change in changes)
        {
            var entry = entries.GetValueOrDefault(change.Path)
                ?? throw BridgeException.NotFound($"设置不存在：{change.Path}");
            if (!entry.Writable) throw BridgeException.InvalidArgument(entry.WriteRestriction ?? "设置只读。");
            var (owner, property) = SettingsCatalog.Resolve(root, change.Path);
            var old = ValueContract.Snapshot(property.GetValue(owner), property.PropertyType);
            if (ValueContract.Version(old) != change.ExpectedVersion) throw new BridgeException("CONFIG_CONFLICT", $"设置 {change.Path} 已变化。", 409);
            var next = inverse is null ? ValueContract.Convert(change.Value, property, change.Path)
                : JsonSerializer.Deserialize(change.Value.GetRawText(), property.PropertyType, ValueContract.Json);
            var existed = TryGet(disk, change.Path, out var oldDisk);
            record.Changes.Add(new(change.Path, old, ValueContract.Snapshot(next, property.PropertyType), existed, NodeValue(oldDisk), ArgumentSchema.Parse("null")));
            prepared.Add((owner, property, JsonSerializer.Deserialize(old.GetRawText(), property.PropertyType, ValueContract.Json), next, change.Path));
        }
        var callback = root.GetType().GetProperty("OnAnyChangedAction");
        if (callback?.CanRead != true || !callback.CanWrite)
            throw new BridgeException("UNSUPPORTED_HOST", "BetterGI 没有可暂停的配置自动保存回调，拒绝修改。", 409);
        var previousSave = callback.GetValue(root);
        SaveRecord(record, create: true); // 第一个 setter 调用前先落盘恢复记录。
        var changed = new List<(object Owner, PropertyInfo Property, object? Before)>();
        string? writtenHash = null;
        try
        {
            callback.SetValue(root, null);
            foreach (var change in prepared)
            {
                // 先登记再调用：setter 可能改完值再抛异常。
                changed.Add((change.Owner, change.Property, change.Before));
                change.Property.SetValue(change.Owner, change.After);
                if (!Equal(ValueContract.Snapshot(change.Property.GetValue(change.Owner), change.Property.PropertyType), record.Changes.Single(item => item.Path == change.Path).After))
                    throw new InvalidOperationException($"BetterGI 未接受 {change.Path} 的请求值。");
            }
            var serialized = JsonNode.Parse(RootSnapshot(root).GetRawText())!.AsObject();
            for (var index = 0; index < record.Changes.Count; index++)
            {
                var item = record.Changes[index];
                if (!TryGet(serialized, item.Path, out var newValue)) throw new InvalidOperationException($"BetterGI 序列化中没有 {item.Path}。");
                record.Changes[index] = item with { AfterDisk = NodeValue(newValue) };
                var restore = inverse?.Changes.Single(change => change.Path == item.Path);
                if (restore is { BeforeDiskPresent: false })
                {
                    Remove(disk, item.Path);
                    record.Changes[index] = record.Changes[index] with { AfterDiskPresent = false };
                }
                else Set(disk, item.Path, restore is null ? newValue?.DeepClone() : JsonNode.Parse(restore.BeforeDisk.GetRawText()));
            }
            SaveRecord(record); // 落盘目标值，用于恢复中断的提交。
            if (Digest(DiskBytes(path)) != Digest(beforeFile)) throw new InvalidOperationException("提交过程中配置文件被其他来源修改。");
            var bytes = Encoding.UTF8.GetBytes(disk.ToJsonString(new JsonSerializerOptions { WriteIndented = true }));
            writtenHash = Digest(bytes);
            AtomicWrite(path, bytes);
            if (Digest(DiskBytes(path)) != writtenHash) throw new IOException("配置落盘核验失败。");
            foreach (var change in prepared)
                if (!Equal(ValueContract.Snapshot(change.Property.GetValue(change.Owner), change.Property.PropertyType), record.Changes.Single(item => item.Path == change.Path).After))
                    throw new InvalidOperationException($"提交后 {change.Path} 内存值发生变化。");
            record.State = "committed";
            SaveRecord(record);
            return record;
        }
        catch (Exception failure)
        {
            var recovered = true;
            foreach (var change in changed.AsEnumerable().Reverse())
                try
                {
                    change.Property.SetValue(change.Owner, change.Before);
                    if (!Equal(ValueContract.Snapshot(change.Property.GetValue(change.Owner), change.Property.PropertyType),
                        ValueContract.Snapshot(change.Before, change.Property.PropertyType))) recovered = false;
                }
                catch { recovered = false; }
            if (writtenHash is not null)
            {
                try
                {
                    if (Digest(DiskBytes(path)) == writtenHash)
                        AtomicWrite(path, beforeFile.Length == 0 ? Encoding.UTF8.GetBytes(beforeSnapshot.GetRawText()) : beforeFile);
                    else if (Digest(DiskBytes(path)) != Digest(beforeFile)) recovered = false;
                }
                catch { recovered = false; }
            }
            record.State = recovered ? "revertedAfterFailure" : "recoveryRequired";
            record.Error = recovered ? "提交失败，已恢复修改前的字段和文件。" : "自动恢复未完全确认，请保留恢复记录并人工核对。";
            try { SaveRecord(record); } catch { }
            throw new BridgeException(recovered ? "COMMIT_REVERTED" : "RECOVERY_REQUIRED", $"{record.Error} changeId={record.ChangeId}；原因类型：{Reflect.Root(failure).GetType().Name}", 409);
        }
        finally
        {
            try { callback.SetValue(root, previousSave); }
            catch
            {
                record.State = "recoveryRequired";
                record.Error = "无法恢复 BetterGI 自动保存回调，需要人工核对。";
                try { SaveRecord(record); } catch { }
                throw new BridgeException("RECOVERY_REQUIRED", $"自动保存回调恢复失败。changeId={record.ChangeId}", 409);
            }
        }
    }

    private string RecordPath(string id)
    {
        if (!Guid.TryParseExact(id, "N", out var guid)) throw BridgeException.InvalidArgument("事务 ID 格式错误。");
        return Path.Combine(recordDirectory, $"config-change-{guid:N}.json");
    }
    private void VerifyRecordedValues(SettingChangeRecord record, bool after)
    {
        var root = rootProvider();
        var disk = ObjectFrom(DiskBytes(ConfigPath), RootSnapshot(root));
        foreach (var item in record.Changes)
        {
            var (owner, property) = SettingsCatalog.Resolve(root, item.Path);
            var current = ValueContract.Snapshot(property.GetValue(owner), property.PropertyType);
            var expected = after ? item.After : item.Before;
            var present = TryGet(disk, item.Path, out var node);
            if (!Equal(current, expected) || present != (after ? item.AfterDiskPresent : item.BeforeDiskPresent)
                || present && !Equal(NodeValue(node), after ? item.AfterDisk : item.BeforeDisk))
                throw new BridgeException("CONFIG_CONFLICT", $"事务已处理，但 {item.Path} 随后发生变化；不会重新应用旧请求。", 409);
        }
    }
    private SettingChangeRecord ReadRecord(string id)
    {
        var path = RecordPath(id);
        if (!File.Exists(path)) throw BridgeException.NotFound("没有该配置事务。");
        if (new FileInfo(path).Length > 32 * 1024 * 1024) throw BridgeException.InvalidArgument("恢复记录过大。");
        if ((File.GetAttributes(path) & FileAttributes.ReparsePoint) != 0) throw BridgeException.InvalidArgument("恢复记录不能是重解析链接。");
        var record = JsonSerializer.Deserialize<SettingChangeRecord>(File.ReadAllText(path), RecordJson)
            ?? throw BridgeException.InvalidArgument("恢复记录无效。");
        if (record.Format != 1 || record.ChangeId != id || record.ConfigPath != ConfigPath)
            throw BridgeException.InvalidArgument("恢复记录与当前 BetterGI 配置不匹配。");
        if (record.BackupDigest != Digest(System.Convert.FromBase64String(record.BeforeFileBase64)))
            throw BridgeException.InvalidArgument("恢复记录校验失败。");
        return record;
    }
    private void SaveRecord(SettingChangeRecord record, bool create = false)
    {
        if (create) record.BackupDigest = Digest(System.Convert.FromBase64String(record.BeforeFileBase64));
        var path = RecordPath(record.ChangeId);
        var bytes = JsonSerializer.SerializeToUtf8Bytes(record, RecordJson);
        if (!create) { AtomicWrite(path, bytes, privateFile: true); return; }
        using var file = new FileStream(path, FileMode.CreateNew, FileAccess.Write, FileShare.None);
        Restrict(path);
        file.Write(bytes);
        file.Flush(true);
    }
    private object Public(SettingChangeRecord record) => new
    {
        changeId = record.ChangeId, state = record.State, createdAt = record.CreatedAt,
        operation = record.Operation,
        verified = record.State is "committed" or "rolledBack", error = record.Error,
        backupFile = RecordPath(record.ChangeId),
        differences = record.Changes.Select(change => new
        {
            path = change.Path,
            before = SettingsCatalog.IsSensitive(change.Path) ? (object)"***REDACTED***" : change.Before,
            after = SettingsCatalog.IsSensitive(change.Path) ? (object)"***REDACTED***" : change.After,
        }),
    };
    internal static void AtomicWrite(string path, byte[] bytes, bool privateFile = false)
    {
        var temporary = path + "." + Guid.NewGuid().ToString("N") + ".tmp";
        try
        {
            using (var file = new FileStream(temporary, FileMode.CreateNew, FileAccess.Write, FileShare.None))
            {
                if (privateFile) Restrict(temporary);
                file.Write(bytes);
                file.Flush(true);
            }
            if (File.Exists(path)) File.Replace(temporary, path, null);
            else File.Move(temporary, path);
        }
        finally { if (File.Exists(temporary)) File.Delete(temporary); }
    }
    internal static void Restrict(string path)
    {
        if (!OperatingSystem.IsWindows()) return;
        var security = new FileSecurity();
        security.SetAccessRuleProtection(true, false);
        security.AddAccessRule(new FileSystemAccessRule(WindowsIdentity.GetCurrent().User!, FileSystemRights.FullControl, AccessControlType.Allow));
        new FileInfo(path).SetAccessControl(security);
    }
    private static bool TryGet(JsonObject root, string path, out JsonNode? value)
    {
        value = root;
        foreach (var part in path.Split('.'))
            if (value is not JsonObject obj || !obj.TryGetPropertyValue(part, out value)) { value = null; return false; }
        return true;
    }
    private static void Set(JsonObject root, string path, JsonNode? value)
    {
        var parts = path.Split('.');
        var owner = root;
        foreach (var part in parts[..^1])
        {
            if (owner[part] is null) owner[part] = new JsonObject();
            owner = owner[part] as JsonObject ?? throw new InvalidOperationException($"磁盘配置路径 {path} 的父级不是对象。");
        }
        owner[parts[^1]] = value;
    }
    private static void Remove(JsonObject root, string path)
    {
        var parts = path.Split('.');
        JsonNode? owner = root;
        foreach (var part in parts[..^1]) owner = owner?[part];
        (owner as JsonObject)?.Remove(parts[^1]);
    }
}

public static class SettingsTransactions
{
    public static SettingsTransactionEngine Engine { get; private set; } = null!;
    public static void Configure(string directory) => Engine = new(
        () => Host.AllConfigInstance() ?? throw BridgeException.Missing("BetterGI 配置尚未初始化。"),
        () => (string)(Reflect.CallStatic(Reflect.RequireType("BetterGenshinImpact.Core.Config.Global"), "Absolute", "User/config.json")
            ?? throw BridgeException.Missing("无法解析 BetterGI 配置位置。")),
        () => (JsonSerializerOptions)(Reflect.GetStatic(Reflect.RequireType("BetterGenshinImpact.Service.ConfigService"), "JsonOptions")
            ?? throw BridgeException.Missing("BetterGI 序列化规则不可用。")), directory, () => Environment.ProcessPath,
        () => { if (Host.TaskSemaphoreCount() is not > 0) throw BridgeException.Busy("存在运行中的独立任务或任务状态未知，暂不修改配置。"); });
}
