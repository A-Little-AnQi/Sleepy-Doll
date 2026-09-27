using System.Diagnostics;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using System.Text.Json.Nodes;
using BgiBridge.Protocol;

namespace BgiBridge.Catalog;

public static class SettingsRecovery
{
    private static readonly JsonSerializerOptions Json = new() { PropertyNamingPolicy = JsonNamingPolicy.CamelCase, WriteIndented = true };
    private static string Hash(byte[] bytes) => Convert.ToHexString(SHA256.HashData(bytes));
    internal static string RecordPath(string directory, string id) {
        if (!Guid.TryParseExact(id,"N",out var guid)) throw BridgeException.InvalidArgument("恢复记录 ID 无效。");
        return Path.Combine(directory,$"config-change-{guid:N}.json");
    }
    public static bool HostRunning(string executable) {
        var processes=Process.GetProcessesByName("BetterGI");
        try { foreach(var process in processes) {
            try { if(string.Equals(process.MainModule?.FileName,executable,StringComparison.OrdinalIgnoreCase))return true; }
            catch { return true; } // Cannot prove that an inaccessible process is a different installation.
        } return false; } finally {foreach(var process in processes)process.Dispose();}
    }
    public static object Status(IEnumerable<string> targets) => new { runningTargets=targets.Distinct(StringComparer.OrdinalIgnoreCase).Where(HostRunning).ToArray() };
    public static string Label(string path) {
        var document=SourceDocumentation.ConfigPath(path);
        var label=document?.Label??document?.Summary;
        return string.IsNullOrWhiteSpace(label)?path:label.Split('。')[0].Trim();
    }
    public static object? SafeValue(string path,JsonElement value) {
        bool Sensitive(JsonElement item)=>item.ValueKind switch {
            JsonValueKind.Object=>item.EnumerateObject().Any(property=>SettingsCatalog.IsSensitive(property.Name)||Sensitive(property.Value)),
            JsonValueKind.Array=>item.EnumerateArray().Any(Sensitive),_=>false};
        if(SettingsCatalog.IsSensitive(path)||Sensitive(value))return "已遮蔽的敏感值";
        if(value.ValueKind==JsonValueKind.Undefined)return "未设置";
        if(value.ValueKind==JsonValueKind.String) {var text=value.GetString()!;return text.Length>1024?text[..1024]+"…（预览已缩略，恢复使用完整值）":text;}
        var raw=value.GetRawText();return raw.Length>1024?raw[..1024]+"…（预览已缩略）":(object)value;
    }
    internal static (SettingChangeRecord Record,string Version,byte[] Backup) Read(string directory,string id) {
        var path=RecordPath(directory,id);
        if(!File.Exists(path)||new FileInfo(path).Length>32*1024*1024||(File.GetAttributes(path)&FileAttributes.ReparsePoint)!=0)
            throw BridgeException.InvalidArgument("备份文件缺失、过大或为文件链接。");
        var bytes=File.ReadAllBytes(path);
        var record=JsonSerializer.Deserialize<SettingChangeRecord>(bytes,Json)??throw BridgeException.InvalidArgument("备份记录无法读取。");
        if(record.Format!=1||record.ChangeId!=id||string.IsNullOrWhiteSpace(record.HostExecutable))throw BridgeException.InvalidArgument("备份记录不完整。");
        var executable=Path.GetFullPath(record.HostExecutable);
        if(!Path.GetFileName(executable).Equals("BetterGI.exe",StringComparison.OrdinalIgnoreCase))throw BridgeException.InvalidArgument("备份不属于 BetterGI。");
        var userRoot=Path.GetFullPath(Path.Combine(Path.GetDirectoryName(executable)!,"User"));
        var target=Path.GetFullPath(record.ConfigPath);
        if(!target.StartsWith(userRoot+Path.DirectorySeparatorChar,StringComparison.OrdinalIgnoreCase)||!Path.GetFileName(target).Equals("config.json",StringComparison.OrdinalIgnoreCase))
            throw BridgeException.InvalidArgument("备份目标不是 BetterGI 的配置文件。");
        for(var candidate=target;candidate.Length>=userRoot.Length;candidate=Path.GetDirectoryName(candidate)??"")
            if((File.Exists(candidate)||Directory.Exists(candidate))&&(File.GetAttributes(candidate)&FileAttributes.ReparsePoint)!=0)
                throw BridgeException.InvalidArgument("配置路径包含文件或目录链接，不能自动恢复。");
        var backup=Convert.FromBase64String(record.BeforeFileBase64);
        if(backup.Length>16*1024*1024||!Hash(backup).Equals(record.BackupDigest,StringComparison.OrdinalIgnoreCase))throw BridgeException.InvalidArgument("备份校验失败。");
        _=Parse(backup);
        return(record,Hash(bytes),backup);
    }
    private static JsonObject Parse(byte[] bytes) {
        using var reader=new StreamReader(new MemoryStream(bytes),Encoding.UTF8,true);
        return JsonNode.Parse(reader.ReadToEnd(),documentOptions:new JsonDocumentOptions{AllowTrailingCommas=true,CommentHandling=JsonCommentHandling.Skip}) as JsonObject
            ??throw BridgeException.InvalidArgument("配置不是有效的 JSON 对象。");
    }
    private static byte[] CurrentBytes(string path) {
        if(!File.Exists(path))return [];
        if(new FileInfo(path).Length>16*1024*1024)throw BridgeException.InvalidArgument("当前配置过大，不能自动恢复。");
        return File.ReadAllBytes(path);
    }
    private static string Version(string path)=>File.Exists(path)?Hash(CurrentBytes(path)):"missing";
    private static bool TryGet(JsonObject root,string path,out JsonNode? value) {
        value=root;
        foreach(var part in path.Split('.'))if(value is not JsonObject owner||!owner.TryGetPropertyValue(part,out value)){value=null;return false;}
        return true;
    }
    private static JsonElement Element(JsonNode? value)=>JsonSerializer.SerializeToElement(value);
    private static bool Compatible(JsonElement a,JsonElement b)=>a.ValueKind==b.ValueKind || a.ValueKind==JsonValueKind.Null || b.ValueKind==JsonValueKind.Null || ((a.ValueKind is JsonValueKind.True or JsonValueKind.False)&&(b.ValueKind is JsonValueKind.True or JsonValueKind.False));
    private static bool Equal(JsonElement a,JsonElement b)=>ValueContract.Canonical(a)==ValueContract.Canonical(b);
    internal static List<StoredSettingChange> SelectChanges(SettingChangeRecord record,IReadOnlyList<string> paths) {
        if(paths.Count is <1 or >20||paths.Distinct(StringComparer.Ordinal).Count()!=paths.Count)throw BridgeException.InvalidArgument("请选择 1 到 20 项不同的设置。");
        return paths.Select(path=>record.Changes.SingleOrDefault(change=>change.Path==path)
            ??throw BridgeException.InvalidArgument("选择项不属于这份变更记录。")).ToList();
    }
    public static object List(string directory) {
        var rows=new List<object>();var targets=new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        foreach(var file in (Directory.Exists(directory)?Directory.EnumerateFiles(directory,"config-change-*.json"):[]).OrderByDescending(File.GetLastWriteTimeUtc).Take(100)) {
            var id=Path.GetFileNameWithoutExtension(file)["config-change-".Length..];
            try {
                var (record,version,backup)=Read(directory,id);targets.Add(record.HostExecutable!);
                var fields=record.Changes.Select(change=>new {path=change.Path,label=change.DisplayName??Label(change.Path)}).ToArray();
                rows.Add(new {changeId=id,recordVersion=version,currentVersion=Version(record.ConfigPath),state=record.State,createdAt=record.CreatedAt,
                    operation=record.Operation,configPath=record.ConfigPath,hostExecutable=record.HostExecutable,paths=fields.Select(field=>field.path).ToArray(),fields,
                    kind=fields.Length>0?"change":"snapshot",snapshotDigest=Hash(backup),canPreview=true,canRestore=!HostRunning(record.HostExecutable!),parentChangeId=record.ParentChangeId});
            }catch(Exception error) {
                rows.Add(new {changeId=id,paths=Array.Empty<string>(),fields=Array.Empty<object>(),kind="unavailable",canPreview=false,canRestore=false,reason=error is BridgeException?error.Message:"备份内容损坏或无法读取。"});
            }
        }
        var running=targets.Where(HostRunning).ToArray();return new {hostRunning=running.Length>0,runningTargets=running,records=rows};
    }
    public static object Preview(string directory,string id,string recordVersion,string mode,IReadOnlyList<string> paths,Func<string,bool>? hostState = null) {
        var (record,version,backup)=Read(directory,id);
        if(version!=recordVersion)throw new BridgeException("CONFIG_CONFLICT","备份记录已变化，请刷新列表。",409);
        if(mode is not("fields" or "full"))throw BridgeException.InvalidArgument("恢复方式无效。");
        var running=(hostState??HostRunning)(record.HostExecutable!);var bytes=CurrentBytes(record.ConfigPath);JsonObject? current=null;
        try {current=Parse(bytes);}catch{}
        var differences=new List<object>();string? reason=null;
        if(mode=="fields") {
            foreach(var saved in SelectChanges(record,paths)) {
                JsonNode? node=null;var present=current is not null&&TryGet(current,saved.Path,out node);var value=Element(node);
                var compatible=present&&(node is null||Compatible(saved.Before,value));
                if(!compatible)reason="当前配置缺少所选设置或类型已变化；请连接 BetterGI 确认，或使用完整备份恢复。";
                differences.Add(new {path=saved.Path,label=saved.DisplayName??Label(saved.Path),current=present?SafeValue(saved.Path,value):"当前文件未设置此项",restore=SafeValue(saved.Path,saved.Before),
                    changed=!present||!Equal(value,saved.Before),laterChanged=present&&!Equal(value,saved.After),related=false});
            }
        } else {
            var prior=Parse(backup);
            void Walk(JsonNode? left,JsonNode? right,string prefix) {
                if(left is JsonObject l&&right is JsonObject r) {
                    foreach(var name in l.Select(item=>item.Key).Union(r.Select(item=>item.Key)))Walk(l[name],r[name],prefix.Length==0?name:prefix+"."+name);
                    return;
                }
                if(JsonNode.DeepEquals(left,right))return;
                differences.Add(new {path=prefix,label=Label(prefix),current=SafeValue(prefix,Element(left)),restore=SafeValue(prefix,Element(right)),changed=true,laterChanged=false,related=false});
            }
            if(current is null)differences.Add(new {path="config",label="完整配置文件",current=bytes.Length==0?"配置文件缺失或为空":"当前配置无法解析",restore="这份有效的完整备份",changed=true,laterChanged=false,related=false});
            else Walk(current,prior,"");
        }
        var changed=differences.Any(item=>JsonSerializer.SerializeToElement(item).GetProperty("changed").GetBoolean());
        if(!changed)reason="当前已是这份记录中的值，无需恢复。";
        if(running)reason=mode=="full"?"完整恢复需要退出 BetterGI，避免运行中的配置自动保存覆盖恢复结果。":"BetterGI 正在运行；逐项恢复需要通过当前连接完成。";
        return new {mode,online=false,changeId=id,recordVersion=version,currentVersion=Version(record.ConfigPath),configPath=record.ConfigPath,hostExecutable=record.HostExecutable,hostRunning=running,
            paths=paths.ToArray(),canApply=!running&&changed&&reason is null,requiresExit=mode=="full"&&running,reason,differences};
    }
    public static object Restore(string directory,string id,string expectedRecordVersion,string expectedCurrentVersion)=>Restore(directory,id,expectedRecordVersion,expectedCurrentVersion,"full",[]);
    public static object Restore(string directory,string id,string expectedRecordVersion,string expectedCurrentVersion,string mode,IReadOnlyList<string> paths,Func<string,bool>? hostState = null) {
        var (record,version,backup)=Read(directory,id);
        if((hostState??HostRunning)(record.HostExecutable!))throw new BridgeException("HOST_RUNNING","请先退出这份配置对应的 BetterGI；运行中的设置请用逐项恢复。",409);
        if(version!=expectedRecordVersion)throw new BridgeException("CONFIG_CONFLICT","备份已变化，请重新预览。",409);
        if(mode is not("full" or "fields"))throw BridgeException.InvalidArgument("恢复方式无效。");
        using var mutex=new Mutex(false,"Local\\SleepyDollRecovery-"+Hash(Encoding.UTF8.GetBytes(record.ConfigPath.ToLowerInvariant())));var acquired=false;
        try {
            try{acquired=mutex.WaitOne(0);}catch(AbandonedMutexException){acquired=true;}
            if(!acquired)throw new BridgeException("RECOVERY_BUSY","已有恢复正在进行。",409);
            if(Version(record.ConfigPath)!=expectedCurrentVersion)throw new BridgeException("CONFIG_CONFLICT","预览后配置已变化，没有覆盖。请重新预览。",409);
            var before=CurrentBytes(record.ConfigPath);var changes=new List<StoredSettingChange>();
            if(mode=="fields") {
                var current=Parse(before);
                foreach(var saved in SelectChanges(record,paths)) {
                    if(!TryGet(current,saved.Path,out var old)||old is not null&&!Compatible(Element(old),saved.Before))throw BridgeException.InvalidArgument("所选设置缺失或类型变化，没有恢复。");
                    var owner=current;var parts=saved.Path.Split('.');foreach(var part in parts[..^1])owner=owner[part]!.AsObject();
                    changes.Add(new(saved.Path,Element(old),saved.Before,true,Element(old),saved.Before,DisplayName:saved.DisplayName??Label(saved.Path)));
                    owner[parts[^1]]=JsonNode.Parse(saved.Before.GetRawText());
                }
                backup=Encoding.UTF8.GetBytes(current.ToJsonString(Json));
            }
            var recovery=new SettingChangeRecord {ChangeId=Guid.NewGuid().ToString("N"),ConfigPath=record.ConfigPath,HostExecutable=record.HostExecutable,
                State="offlineRestorePrepared",ParentChangeId=id,Operation=mode=="fields"?"setting-restore":"offline-restore",Changes=changes,
                BeforeFileBase64=Convert.ToBase64String(before),BackupDigest=Hash(before)};
            var recoveryPath=RecordPath(directory,recovery.ChangeId);
            using(var file=new FileStream(recoveryPath,FileMode.CreateNew,FileAccess.Write,FileShare.None)) {SettingsTransactionEngine.Restrict(recoveryPath);file.Write(JsonSerializer.SerializeToUtf8Bytes(recovery,Json));file.Flush(true);}
            if((hostState??HostRunning)(record.HostExecutable!)||Version(record.ConfigPath)!=expectedCurrentVersion)throw new BridgeException("CONFIG_CONFLICT","BetterGI 已启动或配置变化，没有覆盖。",409);
            SettingsTransactionEngine.AtomicWrite(record.ConfigPath,backup);
            if(Version(record.ConfigPath)!=Hash(backup))throw new BridgeException("RECOVERY_REQUIRED","恢复后核对失败，请不要启动 BetterGI。",409);
            recovery.State="offlineRestored";SettingsTransactionEngine.AtomicWrite(recoveryPath,JsonSerializer.SerializeToUtf8Bytes(recovery,Json),privateFile:true);
            return new {restored=true,online=false,recoveryChangeId=recovery.ChangeId,configPath=record.ConfigPath};
        }finally{if(acquired)mutex.ReleaseMutex();}
    }
}
