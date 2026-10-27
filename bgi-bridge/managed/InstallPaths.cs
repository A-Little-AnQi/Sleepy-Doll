using System.Text.Json;

namespace BgiBridge;

/// <summary>运行期数据的落点。日志和配置回滚记录归 user\，更新保留，卸载按产品流程清理。</summary>
public static class InstallPaths
{
    /// <summary>
    /// 数据根：bridge.config.json 的 userDirectory，缺省是桥目录下的 user\。
    /// 交付形态里桥组件在 &lt;安装目录&gt;\bridge 下，数据根是 &lt;安装目录&gt;\user。
    /// </summary>
    /// <param name="bridgeDir">桥组件所在目录，不是数据目录。</param>
    public static string UserRoot(string bridgeDir) =>
        ConfiguredRoot(bridgeDir) ?? Path.Combine(bridgeDir, "user");

    public static string LogDirectory(string bridgeDir) => Path.Combine(UserRoot(bridgeDir), "log");

    /// <summary>宿主配置改动记录，放在 user\ 下。</summary>
    public static string ChangeRecordDirectory(string bridgeDir) =>
        Path.Combine(DataDirectory(bridgeDir), "config-changes");

    public static string DataDirectory(string bridgeDir)
    {
        var root = UserRoot(bridgeDir);
        var previous = Path.Combine(root, ".sleepy-doll");
        var destination = Path.Combine(root, "data");
        if (ExistingAttributes(previous) is not null)
        {
            using var migration = new FileStream(Path.Combine(root, "data-migration.lock"),
                FileMode.OpenOrCreate, FileAccess.ReadWrite, FileShare.ReadWrite | FileShare.Delete);
            LockMigration(migration);
            try
            {
                // 主程序可能已在取得锁之前完成迁移；不能沿用锁外读到的目录状态。
                if (ExistingAttributes(previous) is not { } attributes) return destination;
                if (!OrdinaryDirectory(attributes))
                    throw new IOException("旧数据目录不是普通目录，未迁移。");
                if (ExistingAttributes(destination) is { } current)
                {
                    // 与主程序 data_directory::migrate 的兼容规则一致，不合并或覆盖文件。
                    if (OrdinaryDirectory(current) && CanonicalDatabaseExists(root, destination)
                        && LegacyHasOnlyAuxiliaryState(previous)) return destination;
                    throw new IOException("发现 .sleepy-doll 与 data 两份数据目录，未覆盖任何内容。");
                }
                using (var guard = new FileStream(Path.Combine(previous, "sleepy-doll.runtime.lock"),
                    FileMode.OpenOrCreate, FileAccess.ReadWrite, FileShare.ReadWrite | FileShare.Delete))
                { guard.Lock(0, 1); guard.Unlock(0, 1); }
                Directory.Move(previous, destination);
            }
            finally { migration.Unlock(0, 1); }
        }
        return destination;
    }

    private static bool OrdinaryDirectory(FileAttributes attributes) =>
        (attributes & FileAttributes.Directory) != 0 && (attributes & FileAttributes.ReparsePoint) == 0;

    private static bool CanonicalDatabaseExists(string root, string destination)
    {
        var database = Path.Combine(destination, "sleepy-doll.db");
        var config = Path.Combine(root, "config.json");
        if (File.Exists(config))
        {
            using var document = ReadConfig(config);
            if (document.RootElement.TryGetProperty("storage", out var storage)
                && storage.TryGetProperty("database", out var value))
            {
                var configured = value.GetString();
                if (configured is null) return false;
                if (configured.StartsWith("${ENV:", StringComparison.Ordinal) && configured.EndsWith('}'))
                    configured = Environment.GetEnvironmentVariable(configured[6..^1]);
                if (string.IsNullOrWhiteSpace(configured)) return false;
                database = Path.GetFullPath(configured, root);
            }
        }
        var relative = Path.GetRelativePath(destination, database);
        return !Path.IsPathRooted(relative) && relative != ".."
            && !relative.StartsWith(".." + Path.DirectorySeparatorChar, StringComparison.Ordinal)
            && File.Exists(database);
    }

    private static bool LegacyHasOnlyAuxiliaryState(string directory)
    {
        foreach (var entry in Directory.EnumerateFileSystemEntries(directory))
        {
            var attributes = File.GetAttributes(entry);
            if ((attributes & FileAttributes.ReparsePoint) != 0) return false;
            var name = Path.GetFileName(entry);
            if ((attributes & FileAttributes.Directory) != 0
                ? name is not ("webview2" or "updates" or "config-changes")
                : name != "window-placement.json") return false;
        }
        return true;
    }

    private static void LockMigration(FileStream migration)
    {
        for (var attempt = 0; ; attempt++)
        {
            try { migration.Lock(0, 1); return; }
            catch (IOException error) when ((error.HResult & 0xffff) == 33 && attempt < 79)
            { System.Threading.Thread.Sleep(25); }
        }
    }

    private static FileAttributes? ExistingAttributes(string path)
    {
        try { return File.GetAttributes(path); }
        catch (FileNotFoundException) { return null; }
        catch (DirectoryNotFoundException) { return null; }
    }

    private static JsonDocument ReadConfig(string path)
    {
        using var stream = new FileStream(path, FileMode.Open, FileAccess.Read,
            FileShare.ReadWrite | FileShare.Delete);
        using var reader = new StreamReader(stream, System.Text.Encoding.UTF8, true);
        return JsonDocument.Parse(reader.ReadToEnd());
    }

    /// <summary>只准备引导日志；数据迁移须在桥配置通过校验后执行。</summary>
    public static void Ensure(string bridgeDir)
    {
        if (string.IsNullOrWhiteSpace(bridgeDir)) return;
        try
        {
            Directory.CreateDirectory(LogDirectory(bridgeDir));
        }
        catch
        {
        }
    }

    /// <summary>缺少旧版可选字段时沿用缺省；显式配置损坏时不能悄悄创建另一份用户数据。</summary>
    private static string? ConfiguredRoot(string bridgeDir)
    {
        if (string.IsNullOrWhiteSpace(bridgeDir)) return null;
        try
        {
            var path = Path.Combine(bridgeDir, "bridge.config.json");
            using var document = ReadConfig(path);
            var root = document.RootElement.TryGetProperty("userDirectory", out var value)
                ? value.GetString()
                : null;
            if (string.IsNullOrWhiteSpace(root)) return null;
            if (!Path.IsPathFullyQualified(root))
                throw new IOException("连接配置中的数据目录必须是完整绝对路径，请重新连接 BetterGI。");
            return Path.GetFullPath(root);
        }
        catch (FileNotFoundException) { return null; }
        catch (DirectoryNotFoundException) { return null; }
    }
}
