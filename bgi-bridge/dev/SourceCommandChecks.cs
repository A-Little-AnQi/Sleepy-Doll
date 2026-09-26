using System.Reflection;
using System.IO;
using System.Runtime.Loader;
using System.Text.Json;
using BgiBridge.Catalog;
using BgiBridge.Tools;

var host = Path.GetFullPath(args[0]);
var directory = Path.GetDirectoryName(host)!;
AssemblyLoadContext.Default.Resolving += (_, name) => File.Exists(Path.Combine(directory, name.Name + ".dll"))
    ? AssemblyLoadContext.Default.LoadFromAssemblyPath(Path.Combine(directory, name.Name + ".dll")) : null;
var assembly = AssemblyLoadContext.Default.LoadFromAssemblyPath(host);
var commands = CommandCatalog.All.ToDictionary(c => "cmd." + c.Name, StringComparer.Ordinal);
using var audit = JsonDocument.Parse(File.ReadAllText(args[1]));
var sourceCommands = audit.RootElement.GetProperty("commands").EnumerateArray().ToArray();
var absent = sourceCommands.Where(c => c.GetProperty("publicExposure").GetBoolean()
    && !commands.ContainsKey(c.GetProperty("methodId").GetString()!)).ToArray();
var excludedLeaks = sourceCommands.Where(c => !c.GetProperty("publicExposure").GetBoolean() && commands.ContainsKey(c.GetProperty("methodId").GetString()!))
    .Select(c => c.GetProperty("methodId").GetString()).ToArray();
var missing = absent.Where(c => assembly.GetType(c.GetProperty("owner").GetString()!)?.GetProperty(c.GetProperty("name").GetString()!) is not null)
    .Select(c => c.GetProperty("methodId").GetString()!).ToArray();
var missingFromHostVersion = absent.Where(c => assembly.GetType(c.GetProperty("owner").GetString()!)?.GetProperty(c.GetProperty("name").GetString()!) is null)
    .Select(c => c.GetProperty("methodId").GetString()!).ToArray();
var blocked = commands.Where(c => c.Value.UnavailableReason is not null).Select(c => new { methodId = c.Key, reason = c.Value.UnavailableReason }).ToArray();
var malformed = commands.Where(c => c.Value.ParameterType is not null && c.Value.ParameterSchema is null).Select(c => c.Key).ToArray();
var stable = new MethodRegistry(); StatusTools.Register(stable); HostLogTools.Register(stable); CatalogTools.Register(stable);
var jsContracts=new List<JsonElement>();var jsErrors=new List<object>();
foreach(var binding in SourceDocumentation.ScriptBindings.EnumerateArray())
{
    var id=binding.GetProperty("alias").GetString()!;
    try{jsContracts.Add(JsonSerializer.SerializeToElement(ScriptApiCatalog.Read(id,"",0,60)));}
    catch(Exception error){jsErrors.Add(new{alias=id,error=error.GetType().Name,message=error.Message});}
}
if (args.Length > 2)
{
    var output = Path.GetFullPath(args[2]); Directory.CreateDirectory(Path.GetDirectoryName(output)!);
    File.WriteAllText(output, JsonSerializer.Serialize(new { sourceAssembly = host, commands = commands.Values.OrderBy(c => c.Name), stable = stable.All.OrderBy(m => m.Id),jsContracts }, new JsonSerializerOptions { WriteIndented = true }));
}
Console.WriteLine(JsonSerializer.Serialize(new { sourceAssembly = host, discovered = commands.Count, stable = stable.Count,jsBindings=jsContracts.Count,jsErrors, missing, missingFromHostVersion, blocked, malformed, excludedLeaks,
    hostConstructed = false, gameStarted = false }, new JsonSerializerOptions { WriteIndented = true }));
if (missing.Length > 0 || malformed.Length > 0 || excludedLeaks.Length > 0 || blocked.Length > 0||jsErrors.Count>0) Environment.ExitCode = 1;
