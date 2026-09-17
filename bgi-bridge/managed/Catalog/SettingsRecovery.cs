using System.Diagnostics;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using System.Text.Json.Nodes;
using BgiBridge.Protocol;

namespace BgiBridge.Catalog;

/// <summary>Offline recovery runs in a separate helper, never inside a broken host.</summary>
public static class SettingsRecovery
{
    private static readonly JsonSerializerOptions Json = new() { PropertyNamingPolicy = JsonNamingPolicy.CamelCase, WriteIndented = true };
    private static string Hash(byte[] bytes) => Convert.ToHexString(SHA256.HashData(bytes));
    private static bool HostRunning()
    {
        var processes = Process.GetProcessesByName("BetterGI");
        var running = processes.Length > 0;
        foreach (var process in processes) process.Dispose();
        return running;
    }
    private static string RecordPath(string directory, string id)
    {
        if (!Guid.TryParseExact(id, "N", out var parsed)) throw BridgeException.InvalidArgument("恢复记录 ID 无效。");
        return Path.Combine(directory, $"config-change-{parsed:N}.json");
    }
    private static (SettingChangeRecord Record, string Version, byte[] Backup) Read(string directory, string id)
    {
        var path = RecordPath(directory, id);
        if (!File.Exists(path) || new FileInfo(path).Length > 32 * 1024 * 1024)
            throw BridgeException.InvalidArgument("恢复记录不存在或超过大小限制。");
        if ((File.GetAttributes(path) & FileAttributes.ReparsePoint) != 0)
            throw BridgeException.InvalidArgument("恢复记录不能是文件链接。");
        var bytes = File.ReadAllBytes(path);
        var record = JsonSerializer.Deserialize<SettingChangeRecord>(bytes, Json)
            ?? throw BridgeException.InvalidArgument("恢复记录无效。");
        if (record.Format != 1 || record.ChangeId != id || string.IsNullOrWhiteSpace(record.HostExecutable))
            throw BridgeException.InvalidArgument("这份备份不完整，无法自动恢复。");
        var executable = Path.GetFullPath(record.HostExecutable);
        if (!Path.GetFileName(executable).Equals("BetterGI.exe", StringComparison.OrdinalIgnoreCase) || !File.Exists(executable))
            throw BridgeException.InvalidArgument("找不到对应的 BetterGI，无法自动恢复。");
        var userRoot = Path.GetFullPath(Path.Combine(Path.GetDirectoryName(executable)!, "User")) + Path.DirectorySeparatorChar;
        var target = Path.GetFullPath(record.ConfigPath);
        if (!target.StartsWith(userRoot, StringComparison.OrdinalIgnoreCase) || !Path.GetFileName(target).Equals("config.json", StringComparison.OrdinalIgnoreCase))
            throw BridgeException.InvalidArgument("这份备份对应的配置不在 BetterGI 的默认位置，无法自动恢复。");
        var backup = Convert.FromBase64String(record.BeforeFileBase64);
        if (backup.Length > 16 * 1024 * 1024 || Hash(backup) != record.BackupDigest)
            throw BridgeException.InvalidArgument("备份内容校验失败。");
        using var reader = new StreamReader(new MemoryStream(backup), Encoding.UTF8, true);
        if (JsonNode.Parse(reader.ReadToEnd(), documentOptions: new JsonDocumentOptions { AllowTrailingCommas = true, CommentHandling = JsonCommentHandling.Skip }) is not JsonObject)
            throw BridgeException.InvalidArgument("备份不是有效的配置对象。");
        return (record, Hash(bytes), backup);
    }
    private static string CurrentVersion(string path)
    {
        if (!File.Exists(path)) return "missing";
        if (new FileInfo(path).Length > 16 * 1024 * 1024) throw BridgeException.InvalidArgument("当前配置过大，拒绝自动恢复。");
        return Hash(File.ReadAllBytes(path));
    }
    public static object List(string directory)
    {
        var running = HostRunning();
        var records = new List<object>();
        foreach (var path in Directory.EnumerateFiles(directory, "config-change-*.json").OrderByDescending(File.GetLastWriteTimeUtc).Take(100))
        {
            var id = Path.GetFileNameWithoutExtension(path)["config-change-".Length..];
            try
            {
                var (record, version, _) = Read(directory, id);
                records.Add(new
                {
                    changeId = id, recordVersion = version, currentVersion = CurrentVersion(record.ConfigPath),
                    state = record.State, createdAt = record.CreatedAt, operation = record.Operation,
                    configPath = record.ConfigPath,
                    paths = record.Changes.Select(change => change.Path).ToArray(),
                    canRestore = !running, reason = running ? "请先完全退出 BetterGI，再恢复配置。" : null,
                });
            }
            catch
            {
                // 损坏或无法校验的备份不能恢复，不进入列表。
            }
        }
        return new { hostRunning = running, records };
    }
    public static object Restore(string directory, string id, string expectedRecordVersion, string expectedCurrentVersion)
    {
        if (HostRunning()) throw new BridgeException("HOST_RUNNING", "请先退出 BetterGI，再恢复配置。", 409);
        var (record, version, backup) = Read(directory, id);
        if (version != expectedRecordVersion) throw new BridgeException("CONFIG_CONFLICT", "备份已更新。", 409);
        using var mutex = new Mutex(false, "Local\\SleepyDollRecovery-" + Hash(Encoding.UTF8.GetBytes(record.ConfigPath.ToLowerInvariant())));
        var acquired = false;
        try
        {
            try { acquired = mutex.WaitOne(0); } catch (AbandonedMutexException) { acquired = true; }
            if (!acquired) throw new BridgeException("RECOVERY_BUSY", "已有恢复正在进行。", 409);
            if (CurrentVersion(record.ConfigPath) != expectedCurrentVersion)
                throw new BridgeException("CONFIG_CONFLICT", "配置已变化，没有覆盖。", 409);
            var before = File.Exists(record.ConfigPath) ? File.ReadAllBytes(record.ConfigPath) : [];
            var recovery = new SettingChangeRecord
            {
                ChangeId = Guid.NewGuid().ToString("N"), ConfigPath = record.ConfigPath, HostExecutable = record.HostExecutable,
                State = "offlineRestorePrepared", ParentChangeId = id, Operation = "offline-restore",
                BeforeFileBase64 = Convert.ToBase64String(before), BackupDigest = Hash(before),
            };
            var recoveryPath = RecordPath(directory, recovery.ChangeId);
            using (var file = new FileStream(recoveryPath, FileMode.CreateNew, FileAccess.Write, FileShare.None))
            {
                SettingsTransactionEngine.Restrict(recoveryPath);
                file.Write(JsonSerializer.SerializeToUtf8Bytes(recovery, Json));
                file.Flush(true);
            }
            if (HostRunning() || CurrentVersion(record.ConfigPath) != expectedCurrentVersion)
                throw new BridgeException("CONFIG_CONFLICT", "BetterGI 已启动或配置已变化，没有覆盖。", 409);
            SettingsTransactionEngine.AtomicWrite(record.ConfigPath, backup);
            if (CurrentVersion(record.ConfigPath) != Hash(backup))
                throw new BridgeException("RECOVERY_REQUIRED", "恢复后核对失败。请先不要启动 BetterGI。", 409);
            recovery.State = "offlineRestored";
            SettingsTransactionEngine.AtomicWrite(recoveryPath, JsonSerializer.SerializeToUtf8Bytes(recovery, Json), privateFile: true);
            return new { restored = true, recoveryChangeId = recovery.ChangeId, configPath = record.ConfigPath };
        }
        finally { if (acquired) mutex.ReleaseMutex(); }
    }
}
