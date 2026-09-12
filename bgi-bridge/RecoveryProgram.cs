using System.Text;
using System.Text.Json;
using BgiBridge.Catalog;
using BgiBridge.Protocol;

Console.OutputEncoding = new UTF8Encoding(false);
var options = new JsonSerializerOptions { PropertyNamingPolicy = JsonNamingPolicy.CamelCase };
try
{
    object result = args.Length == 1 && args[0] == "list"
        ? SettingsRecovery.List(AppContext.BaseDirectory)
        : args.Length == 4 && args[0] == "restore"
            ? SettingsRecovery.Restore(AppContext.BaseDirectory, args[1], args[2], args[3])
            : throw new ArgumentException("Recovery list | restore <changeId> <recordVersion> <currentVersion>");
    Console.WriteLine(JsonSerializer.Serialize(new { ok = true, result }, options));
}
catch (Exception error)
{
    var message = error is BridgeException bridge ? bridge.Message : "恢复记录读取或写入失败，请核对路径、权限与记录完整性。";
    Console.WriteLine(JsonSerializer.Serialize(new { ok = false, error = new { message } }, options));
    Environment.ExitCode = 1;
}
