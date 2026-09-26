using System.Reflection;
using System.IO;
using System.Runtime.Loader;
using System.Text.Json;
using BgiBridge.Catalog;

var host = Path.GetFullPath(args[0]);
var directory = Path.GetDirectoryName(host)!;
AssemblyLoadContext.Default.Resolving += (_, name) => File.Exists(Path.Combine(directory, name.Name + ".dll"))
    ? AssemblyLoadContext.Default.LoadFromAssemblyPath(Path.Combine(directory, name.Name + ".dll")) : null;
var assembly = AssemblyLoadContext.Default.LoadFromAssemblyPath(host);
var commands = CommandCatalog.All.ToDictionary(c => "cmd." + c.Name, StringComparer.Ordinal);
using var audit = JsonDocument.Parse(File.ReadAllText(args[1]));
var absent = audit.RootElement.GetProperty("commands").EnumerateArray().Where(c => c.GetProperty("methodId").GetString() is not ("cmd.task_settings_page.switch_auto_track" or "cmd.task_settings_page.go_to_auto_track_url")
    && !commands.ContainsKey(c.GetProperty("methodId").GetString()!)).ToArray();
var missing = absent.Where(c => assembly.GetType(c.GetProperty("owner").GetString()!)?.GetProperty(c.GetProperty("name").GetString()!) is not null)
    .Select(c => c.GetProperty("methodId").GetString()!).ToArray();
var missingFromHostVersion = absent.Where(c => assembly.GetType(c.GetProperty("owner").GetString()!)?.GetProperty(c.GetProperty("name").GetString()!) is null)
    .Select(c => c.GetProperty("methodId").GetString()!).ToArray();
var blocked = commands.Where(c => c.Value.UnavailableReason is not null).Select(c => new { methodId = c.Key, reason = c.Value.UnavailableReason }).ToArray();
var malformed = commands.Where(c => c.Value.ParameterType is not null && c.Value.ParameterSchema is null).Select(c => c.Key).ToArray();
Console.WriteLine(JsonSerializer.Serialize(new { sourceAssembly = host, discovered = commands.Count, missing, missingFromHostVersion, blocked, malformed,
    hostConstructed = false, gameStarted = false }, new JsonSerializerOptions { WriteIndented = true }));
if (missing.Length > 0 || malformed.Length > 0) Environment.ExitCode = 1;
