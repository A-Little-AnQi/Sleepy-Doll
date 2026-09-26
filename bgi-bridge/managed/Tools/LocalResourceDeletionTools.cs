using System.IO;
using System.Security.Cryptography;
using System.Text.Json;
using BgiBridge.Bgi;
using BgiBridge.Catalog;
using BgiBridge.Protocol;

namespace BgiBridge.Tools;

/// <summary>按已定位的 User 资源路径检查、删除与恢复；不依赖可见页面或懒加载树。</summary>
public static class LocalResourceDeletionTools
{
    private static readonly string[] Roots=["AutoPathing","JsScript","KeyMouseScript","AutoFight","AutoGeniusInvokation"];
    private sealed record FileEntry(string Path,long Bytes,string Sha256);
    private sealed record Reference(string Group,string Project,string File,string Version);
    private sealed record Inventory(string Path,bool Directory,FileEntry[] Files,string[] Directories,Reference[] References,string[] Subscriptions,string Version);
    public static void Register(MethodRegistry registry)
    {
        var path=AgentSchemas.Text("已定位的 User 相对资源路径，例如 AutoPathing/地方特产/稻妻/血斛 或 JsScript/脚本目录；不能删除分类根目录。",1024);
        var version=ArgumentSchema.Parse("""{"type":"string","pattern":"^[0-9a-fA-F]{64}$"}""");
        AgentGuide Guide(string title,string purpose,bool write)=>new(title,purpose,["用户要求删除或恢复已定位的路线、JS、键鼠、战斗或七圣资源；只删配置组仍使用 delete_script_group。"],
            ["路径来自 user.list/read/resolve；删除前 inspect_local_resource 获取实际文件、引用与版本。","资源删除不需要游戏、页面、选中项或 UI 树；任务运行时不删除。"],write?["将精确资源移入 User 的恢复区或移回；其他资源、配置组及订阅保留。"]:["只读文件清单、引用、订阅覆盖与版本。"],
            "deleted/restored=true 且 verified=true 才完成；backupId 可恢复。订阅保留，未来更新可能重新导入，结果明确列出覆盖订阅。",
            "已验证结果返回后停止；不再搜索删除命令、构造上下文、导航、展开树或枚举选项。","用 restore_local_resource 恢复同一备份；原路径已存在则拒绝覆盖。",[],"bridge-stable-resource-lifecycle");
        var offset=ArgumentSchema.Parse("""{"type":"integer","minimum":0}""");var limit=ArgumentSchema.Parse("""{"type":"integer","minimum":1,"maximum":100}""");
        registry.Register("bgi.inspect_local_resource","resources","检查路线或脚本资源的删除范围、引用与版本",(a,_)=>Task.FromResult<object?>(Inspect(Path.Combine(AppContext.BaseDirectory,"User"),a.GetProperty("path").GetString()!,a.TryGetProperty("offset",out var o)?o.GetInt32():0,a.TryGetProperty("limit",out var l)?l.GetInt32():30)),
            inputSchema:AgentSchemas.Object(("path",path,true),("offset",offset,false),("limit",limit,false)),guide:Guide("检查资源删除范围","按 User 相对路径直接核对完整目录／单文件、文件 SHA、配置组引用及订阅覆盖；不操作懒加载界面树。",false));
        registry.Register("bgi.delete_local_resource","resources","按精确路径删除本机路线或脚本资源并保留恢复备份",async(a,c)=>
        {
            if(!Host.IsIdle)throw BridgeException.Busy("有任务正在运行，资源未删除。");
            using var gate=await RepoGate(c);
            var result=Delete(Path.Combine(AppContext.BaseDirectory,"User"),a.GetProperty("path").GetString()!,a.GetProperty("expectedVersion").GetString()!,a.TryGetProperty("allowBrokenReferences",out var r)&&r.GetBoolean(),c);
            var refresh=await Ui.InvokeAsync(()=>RefreshLoaded());return new{result,refresh};
        },readOnly:false,destructive:true,inputSchema:AgentSchemas.Object(("path",path,true),("expectedVersion",version,true),("allowBrokenReferences",AgentSchemas.Flag("默认 false；只有用户明确允许保留指向缺失资源的其他配置组时才为 true。"),false)),
            guide:Guide("删除路线或脚本资源","按已经找到的真实路径直接删除完整作者包／材料目录或单文件；保留同卷恢复备份、版本核对和范围证据，不要求界面选中。",true));
        registry.Register("bgi.restore_local_resource","resources","恢复由资源删除入口生成的原始备份",async(a,c)=>
        {
            if(!Host.IsIdle)throw BridgeException.Busy("有任务正在运行，资源未恢复。");
            using var gate=await RepoGate(c);
            var result=Restore(Path.Combine(AppContext.BaseDirectory,"User"),a.GetProperty("backupId").GetString()!,c);var refresh=await Ui.InvokeAsync(()=>RefreshLoaded());return new{result,refresh};
        },readOnly:false,inputSchema:AgentSchemas.Object(("backupId",ArgumentSchema.Parse("""{"type":"string","pattern":"^[0-9a-f]{32}$"}"""),true)),guide:Guide("恢复被删除资源","按返回的 backupId 恢复原始资源字节与相对位置；目标已存在时拒绝覆盖。",true));
    }
    private sealed class LockLease(SemaphoreSlim? semaphore):IDisposable{public void Dispose()=>semaphore?.Release();}
    private static async Task<IDisposable> RepoGate(CancellationToken cancellation)
    {
        var type=Reflect.FindType("BetterGenshinImpact.Core.Script.ScriptRepoUpdater");
        var instance=type?.BaseType?.GetField("_instance",System.Reflection.BindingFlags.Static|System.Reflection.BindingFlags.NonPublic)?.GetValue(null);
        if(instance is null)return new LockLease(null);
        if(Reflect.Get(instance,"_repoWriteLock") is not SemaphoreSlim semaphore)throw BridgeException.Missing("仓库写锁不能识别，资源未修改。");
        if(!await semaphore.WaitAsync(0,cancellation))throw BridgeException.Busy("仓库正在更新／导入，资源未修改。");
        return new LockLease(semaphore);
    }
    private static string Normalize(string raw)
    {
        var path=raw.Replace('\\','/');
        var aliases=new Dictionary<string,string>{{"pathing","AutoPathing"},{"js","JsScript"},{"combat","AutoFight"},{"tcg","AutoGeniusInvokation"}};
        var parts=path.Split('/');if(parts.Length>1&&aliases.TryGetValue(parts[0],out var root))parts[0]=root;
        if(parts.Length<2||!Roots.Contains(parts[0])||parts.Any(p=>p is "" or "." or ".."||p.Any(char.IsControl)||p.Contains(':')||p.EndsWith('.')||p.EndsWith(' ')))
            throw BridgeException.InvalidArgument("资源必须是允许分类下的精确相对路径，不能是根目录、绝对路径或 ..。");
        return string.Join('/',parts);
    }
    private static string Resolve(string user,string relative)
    {
        user=Path.GetFullPath(user);var path=Path.GetFullPath(Path.Combine(user,relative.Replace('/',Path.DirectorySeparatorChar)));
        if(!path.StartsWith(user+Path.DirectorySeparatorChar,StringComparison.OrdinalIgnoreCase))throw BridgeException.InvalidArgument("路径越出 User。");
        var current=user;CheckLink(current);
        foreach(var part in Path.GetRelativePath(user,path).Split(Path.DirectorySeparatorChar)){current=Path.Combine(current,part);CheckLink(current);}
        return path;
    }
    private static void CheckLink(string path)
    {if((File.Exists(path)||Directory.Exists(path))&&(File.GetAttributes(path)&FileAttributes.ReparsePoint)!=0)throw BridgeException.InvalidArgument("资源范围包含符号链接／目录联接，未操作。");}
    private static Inventory InventoryAt(string user,string relative)
    {
        relative=Normalize(relative);var target=Resolve(user,relative);var directory=Directory.Exists(target);
        if(!directory&&!File.Exists(target))throw BridgeException.NotFound("本机资源路径不存在。");
        var files=new List<FileEntry>();var dirs=new List<string>();var pending=new Queue<string>();if(directory)pending.Enqueue(target);
        void Add(string file)
        {
            CheckLink(file);if(files.Count>=20000)throw BridgeException.InvalidArgument("资源文件过多，请定位更具体目标。");
            using var input=File.OpenRead(file);files.Add(new(Path.GetRelativePath(target,file).Replace('\\','/'),input.Length,Convert.ToHexString(SHA256.HashData(input)).ToLowerInvariant()));
        }
        if(!directory)Add(target);
        while(pending.TryDequeue(out var dir))
        {
            foreach(var entry in Directory.EnumerateFileSystemEntries(dir).Order(StringComparer.Ordinal))
            {
                CheckLink(entry);if(Directory.Exists(entry)){dirs.Add(Path.GetRelativePath(target,entry).Replace('\\','/'));pending.Enqueue(entry);}else Add(entry);
                if(dirs.Count>20000)throw BridgeException.InvalidArgument("资源目录过多，请缩小范围。");
            }
        }
        var references=References(user,relative);var subscriptions=Subscriptions(user,relative);
        var ordered=files.OrderBy(f=>f.Path,StringComparer.Ordinal).ToArray();var sortedDirs=dirs.Order(StringComparer.Ordinal).ToArray();
        var version=Convert.ToHexString(SHA256.HashData(JsonSerializer.SerializeToUtf8Bytes(new{path=relative,directory,files=ordered,directories=sortedDirs,references,subscriptions}))).ToLowerInvariant();
        return new(relative,directory,ordered,sortedDirs,references,subscriptions,version);
    }
    private static Reference[] References(string user,string target)
    {
        var groups=Resolve(user,"ScriptGroup");if(!Directory.Exists(groups))return [];
        var found=new List<Reference>();
        foreach(var file in Directory.EnumerateFiles(groups,"*.json").Order(StringComparer.Ordinal))
        {
            CheckLink(file);var bytes=File.ReadAllBytes(file);using var json=JsonDocument.Parse(bytes);
            if(!json.RootElement.TryGetProperty("projects",out var projects)||projects.ValueKind!=JsonValueKind.Array)continue;
            foreach(var project in projects.EnumerateArray())
            {
                var kind=project.TryGetProperty("type",out var type)?type.GetString():null;var root=kind switch{"Pathing"=>"AutoPathing","Javascript"=>"JsScript","KeyMouse"=>"KeyMouseScript",_=>null};
                if(root is null||!project.TryGetProperty("folderName",out var folder)||folder.ValueKind!=JsonValueKind.String)continue;
                var name=project.TryGetProperty("name",out var title)?title.GetString()??"":"";var reference=root+"/"+(folder.GetString()??"").Replace('\\','/').Trim('/');
                if(root is "AutoPathing" or "KeyMouseScript")reference+="/"+(name.EndsWith(".json",StringComparison.OrdinalIgnoreCase)?name:name+".json");
                if(reference.Equals(target,StringComparison.OrdinalIgnoreCase)||reference.StartsWith(target+"/",StringComparison.OrdinalIgnoreCase))
                    found.Add(new(json.RootElement.GetProperty("name").GetString()??Path.GetFileName(file),name,Path.GetRelativePath(user,file).Replace('\\','/'),Convert.ToHexString(SHA256.HashData(bytes)).ToLowerInvariant()));
            }
        }
        return found.ToArray();
    }
    private static string[] Subscriptions(string user,string target)
    {
        var directory=Resolve(user,"Subscriptions");if(!Directory.Exists(directory))return [];
        var alias=target.Split('/')[0] switch{"AutoPathing"=>"pathing","JsScript"=>"js","AutoFight"=>"combat","AutoGeniusInvokation"=>"tcg",_=>null};if(alias is null)return [];
        var resource=alias+target[target.IndexOf('/')..];var found=new HashSet<string>();
        foreach(var file in Directory.EnumerateFiles(directory,"*.json")){CheckLink(file);using var json=JsonDocument.Parse(File.ReadAllBytes(file));if(json.RootElement.ValueKind!=JsonValueKind.Array)continue;
            foreach(var item in json.RootElement.EnumerateArray().Where(item=>item.ValueKind==JsonValueKind.String))
            {var path=item.GetString()!.Replace('\\','/').TrimEnd('/');if(path==resource||path.StartsWith(resource+"/",StringComparison.OrdinalIgnoreCase)||resource.StartsWith(path+"/",StringComparison.OrdinalIgnoreCase))found.Add(path);}}
        return found.Order(StringComparer.Ordinal).ToArray();
    }
    public static object Inspect(string user,string path,int offset=0,int limit=30)
    {
        var inventory=InventoryAt(user,path);
        return new{path=inventory.Path,directory=inventory.Directory,version=inventory.Version,totalFiles=inventory.Files.Length,files=inventory.Files.Skip(offset).Take(limit),nextOffset=offset+limit<inventory.Files.Length?(int?)(offset+limit):null,
            references=inventory.References,coveringSubscriptions=inventory.Subscriptions,next="describe/invoke bgi.delete_local_resource，path 与 expectedVersion 使用本次返回值；不再找 UI 节点。"};
    }
    public static object Delete(string user,string path,string expected,bool allowBrokenReferences,CancellationToken cancellation)
    {
        var inventory=InventoryAt(user,path);
        if(!inventory.Version.Equals(expected,StringComparison.OrdinalIgnoreCase))throw new BridgeException("VERSION_CONFLICT","资源、引用或订阅在检查后变化，未删除。",409);
        if(inventory.References.Length>0&&!allowBrokenReferences)throw new BridgeException("RESOURCE_IN_USE","还有其他配置组引用资源；先按用户范围处理这些组，或明确允许保留失效引用。",409);
        var target=Resolve(user,inventory.Path);var id=Guid.NewGuid().ToString("N");var backup=Resolve(user,".sleepy-doll/deleted-resources/"+id);cancellation.ThrowIfCancellationRequested();
        Directory.CreateDirectory(backup);var payload=Path.Combine(backup,"payload");var manifest=Path.Combine(backup,"manifest.json");
        try
        {
            File.WriteAllText(manifest,JsonSerializer.Serialize(inventory));
            var current=InventoryAt(user,path);if(current.Version!=inventory.Version)throw new BridgeException("VERSION_CONFLICT","资源在落盘前变化，未删除。",409);
            cancellation.ThrowIfCancellationRequested();
            if(inventory.Directory)Directory.Move(target,payload);else File.Move(target,payload);
            if(File.Exists(target)||Directory.Exists(target))throw BridgeException.Failed("目标仍存在，未确认删除。");
        }
        catch
        {
            if(!File.Exists(payload)&&!Directory.Exists(payload)){if(File.Exists(manifest))File.Delete(manifest);Directory.Delete(backup);}
            throw;
        }
        return new{path=inventory.Path,deleted=true,verified=true,backupId=id,deletedFiles=inventory.Files.Length,brokenReferences=inventory.References,subscriptionsPreserved=true,coveringSubscriptions=inventory.Subscriptions,
            note=inventory.Subscriptions.Length==0?"已从本机移除；其他资源和配置组保留。":"已删除本机资源；订阅保留，后续订阅更新可能重新导入。"};
    }
    public static object Restore(string user,string id,CancellationToken cancellation)
    {
        if(!Guid.TryParseExact(id,"N",out _))throw BridgeException.InvalidArgument("backupId 必须来自删除结果。");
        var backup=Resolve(user,".sleepy-doll/deleted-resources/"+id);var manifest=Path.Combine(backup,"manifest.json");CheckLink(manifest);
        var inventory=JsonSerializer.Deserialize<Inventory>(File.ReadAllText(manifest))??throw BridgeException.Failed("资源备份记录无效。");var target=Resolve(user,Normalize(inventory.Path));var payload=Resolve(user,Path.GetRelativePath(user,Path.Combine(backup,"payload")));
        if(File.Exists(target)||Directory.Exists(target))throw new BridgeException("RESOURCE_CONFLICT","原位置已有资源，未覆盖。",409);
        if(inventory.Directory)
        {
            var storedFiles=new List<string>();var storedDirectories=new List<string>();var folders=new Queue<string>();folders.Enqueue(payload);
            while(folders.TryDequeue(out var folder))foreach(var entry in Directory.EnumerateFileSystemEntries(folder))
            {
                CheckLink(entry);var relative=Path.GetRelativePath(payload,entry).Replace('\\','/');
                if(Directory.Exists(entry)){storedDirectories.Add(relative);folders.Enqueue(entry);}else storedFiles.Add(relative);
            }
            if(!storedFiles.Order(StringComparer.Ordinal).SequenceEqual(inventory.Files.Select(file=>file.Path))||!storedDirectories.Order(StringComparer.Ordinal).SequenceEqual(inventory.Directories))
                throw new BridgeException("VERSION_CONFLICT","备份范围已变化，未恢复。",409);
        }
        foreach(var file in inventory.Files)
        {
            var stored=inventory.Directory?Path.GetFullPath(Path.Combine(payload,file.Path.Replace('/',Path.DirectorySeparatorChar))):payload;
            if(inventory.Directory&&!stored.StartsWith(payload+Path.DirectorySeparatorChar,StringComparison.OrdinalIgnoreCase))throw BridgeException.InvalidArgument("备份文件范围无效。");
            CheckLink(stored);using var input=File.OpenRead(stored);
            if(input.Length!=file.Bytes||Convert.ToHexString(SHA256.HashData(input)).ToLowerInvariant()!=file.Sha256)throw new BridgeException("VERSION_CONFLICT","备份内容发生变化，未恢复。",409);
        }
        cancellation.ThrowIfCancellationRequested();Directory.CreateDirectory(Path.GetDirectoryName(target)!);if(inventory.Directory)Directory.Move(payload,target);else File.Move(payload,target);
        var restored=InventoryAt(user,inventory.Path);
        if(!restored.Files.SequenceEqual(inventory.Files)||!restored.Directories.SequenceEqual(inventory.Directories))throw BridgeException.Failed("恢复内容与原清单不一致，请核对。");
        return new{path=inventory.Path,restored=true,verified=true,restoredFiles=restored.Files.Length,backupId=id};
    }
    private static object RefreshLoaded()
    {
        var refreshed=new List<string>();var failures=new List<string>();
        foreach(var view in CommandTargets.Snapshot().Where(value=>value.GetType().FullName is "BetterGenshinImpact.ViewModel.Pages.MapPathingViewModel" or "BetterGenshinImpact.ViewModel.Pages.JsListViewModel" or "BetterGenshinImpact.ViewModel.Pages.KeyMouseRecordPageViewModel"))
            try{Reflect.Call(view,"OnRefresh");refreshed.Add(view.GetType().Name);}catch{failures.Add(view.GetType().Name);}
        return new{refreshed,refreshFailed=failures,note="仅刷新已加载的资源页面；不创建页面，不影响已完成的文件删除核验。"};
    }
}
