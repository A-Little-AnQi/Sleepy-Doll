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
            throw BridgeException.InvalidArgument("恢复记录缺少宿主身份，不能自动恢复。");
        var executable = Path.GetFullPath(record.HostExecutable);
        if (!Path.GetFileName(executable).Equals("BetterGI.exe", StringComparison.OrdinalIgnoreCase) || !File.Exists(executable))
            throw BridgeException.InvalidArgument("无法验证备份对应的 BetterGI 安装位置。");
        var userRoot = Path.GetFullPath(Path.Combine(Path.GetDirectoryName(executable)!, "User")) + Path.DirectorySeparatorChar;
        var target = Path.GetFullPath(record.ConfigPath);
        if (!target.StartsWith(userRoot, StringComparison.OrdinalIgnoreCase) || !Path.GetFileName(target).Equals("config.json", StringComparison.OrdinalIgnoreCase))
            throw BridgeException.InvalidArgument("配置不在已验证的宿主 User 目录中；自定义目录需人工恢复。");
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
            catch (BridgeException error) { records.Add(new { changeId = id, canRestore = false, reason = error.Message, paths = Array.Empty<string>() }); }
            catch { records.Add(new { changeId = id, canRestore = false, reason = "记录无法校验。", paths = Array.Empty<string>() }); }
        }
        return new { hostRunning = running, records };
    }
    public static object Restore(string directory, string id, string expectedRecordVersion, string expectedCurrentVersion)
    {
        if (HostRunning()) throw new BridgeException("HOST_RUNNING", "必须先完全退出 BetterGI，不能在宿主运行时覆盖整个配置。", 409);
        var (record, version, backup) = Read(directory, id);
        if (version != expectedRecordVersion) throw new BridgeException("CONFIG_CONFLICT", "恢复记录已变化，请刷新后重新确认。", 409);
        using var mutex = new Mutex(false, "Local\\SleepyDollRecovery-" + Hash(Encoding.UTF8.GetBytes(record.ConfigPath.ToLowerInvariant())));
        var acquired = false;
        try
        {
            try { acquired = mutex.WaitOne(0); } catch (AbandonedMutexException) { acquired = true; }
            if (!acquired) throw new BridgeException("RECOVERY_BUSY", "已有恢复操作正在进行。", 409);
            if (CurrentVersion(record.ConfigPath) != expectedCurrentVersion)
                throw new BridgeException("CONFIG_CONFLICT", "当前配置在确认后发生变化，未覆盖。", 409);
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
                throw new BridgeException("CONFIG_CONFLICT", "宿主已启动或配置发生变化，未恢复。", 409);
            SettingsTransactionEngine.AtomicWrite(record.ConfigPath, backup);
            if (CurrentVersion(record.ConfigPath) != Hash(backup))
                throw new BridgeException("RECOVERY_REQUIRED", "恢复后校验失败，请保留恢复记录并人工核对。", 409);
            recovery.State = "offlineRestored";
            SettingsTransactionEngine.AtomicWrite(recoveryPath, JsonSerializer.SerializeToUtf8Bytes(recovery, Json), privateFile: true);
            return new { restored = true, recoveryChangeId = recovery.ChangeId, configPath = record.ConfigPath };
        }
        finally { if (acquired) mutex.ReleaseMutex(); }
    }
}
