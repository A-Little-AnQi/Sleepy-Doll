using System.Text;
using System.Text.Json;
using BgiBridge;
using BgiBridge.Catalog;
using BgiBridge.Protocol;
using BgiBridge.Bgi;

Console.OutputEncoding = new UTF8Encoding(false);
var options = new JsonSerializerOptions { PropertyNamingPolicy = JsonNamingPolicy.CamelCase };
// 本程序与桥组件同目录，所以从自己的目录读出同一个数据根。
try
{
    // 来源检查只读目标程序；不初始化恢复目录，也不读取用户配置。
    if (args.Length == 2 && args[0] == "origin")
    {
        Console.WriteLine(JsonSerializer.Serialize(new { ok = true, result = HostOrigin.Inspect(args[1]) }, options));
        return;
    }
    var bridgeDir = AppContext.BaseDirectory;
    InstallPaths.Ensure(bridgeDir);
    var records = InstallPaths.ChangeRecordDirectory(bridgeDir);
    object result = args.Length == 1 && args[0] == "list"
        ? SettingsRecovery.List(records)
        : args.Length == 4 && args[0] == "restore"
            ? SettingsRecovery.Restore(records, args[1], args[2], args[3])
            : throw new ArgumentException("Recovery list | restore <changeId> <recordVersion> <currentVersion>");
    Console.WriteLine(JsonSerializer.Serialize(new { ok = true, result }, options));
}
catch (Exception error)
{
    var message = error is BridgeException bridge ? bridge.Message : "恢复记录读取或写入失败，请核对路径、权限与记录完整性。";
    Console.WriteLine(JsonSerializer.Serialize(new { ok = false, error = new { message } }, options));
    Environment.ExitCode = 1;
}
