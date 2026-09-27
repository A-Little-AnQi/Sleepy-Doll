using System.Text.Json;
using BgiBridge.Bgi;
using System.Diagnostics;
using BgiBridge.Protocol;

var checks = 0;
void Check(bool condition, string message) { if (!condition) throw new Exception(message); checks++; }
foreach (var repository in new[] { "babalae/better-genshin-impact", "Bedrockx/better-genshin-impact", "kaedelcb/better-genshin-impact" })
{
    foreach (var commit in new[] { "main", "preview", new string('a', 40) })
    {
        var json = JsonSerializer.SerializeToUtf8Bytes(new { documents = new Dictionary<string, string> { ["/_/*"] = $"https://raw.githubusercontent.com/{repository}/{commit}/*" } });
        var result = HostOrigin.InspectSourceLink(json);
        Check(result.State == (repository.StartsWith("babalae/") ? "official" : "nonOfficial"), repository + "/" + commit);
    }
}
Check(HostOrigin.InspectSourceLink("{\"documents\":{}}"u8).State == "unknown", "empty mapping");
Check(HostOrigin.InspectSourceLink("{\"documents\":{\"/_/*\":\"https://raw.githubusercontent.com.evil.example/babalae/better-genshin-impact/main/*\"}}"u8).State == "nonOfficial", "lookalike host");
Check(HostOrigin.InspectSourceLink("{\"documents\":{\"/different/*\":\"https://raw.githubusercontent.com/babalae/better-genshin-impact/main/*\"}}"u8, ["/actual/App.cs"]).State == "unknown", "unrelated source mapping");
try { HostOrigin.RequireOfficial(); throw new Exception("probe process must not start a bridge"); }
catch (BridgeException error) { Check(error.Code == "HOST_ORIGIN_REJECTED" && error.Message == HostOrigin.RejectedMessage, "bridge gate message"); }
foreach (var path in args)
{
    var assessment = HostOrigin.Inspect(path);
    Check(assessment.State == "official", "actual official executable rejected: " + assessment.Evidence);
    Console.WriteLine(JsonSerializer.Serialize(new { path, assessment }));
}
var fixtureRoot = Path.GetFullPath(Path.Combine(Path.GetTempPath(), "sleepy-doll-bgi-origin-validation"));
if (Directory.Exists(fixtureRoot)) throw new Exception("Owned fixture directory already exists; do not overwrite");
Console.WriteLine(JsonSerializer.Serialize(new { resource = fixtureRoot, cleanup = "finally removes owned fixture tree; child compilers have build servers disabled" }));
Directory.CreateDirectory(fixtureRoot);
try
{
    File.WriteAllText(Path.Combine(fixtureRoot, "Fixture.cs"), "namespace BetterGenshinImpact; public sealed class App { public string Title => \"BetterGI\"; }");
    File.WriteAllText(Path.Combine(fixtureRoot, "Fixture.csproj"), """
        <Project Sdk="Microsoft.NET.Sdk">
          <PropertyGroup><TargetFramework>net8.0</TargetFramework><AssemblyName>BetterGI</AssemblyName><DebugType>embedded</DebugType><EnableSourceControlManagerQueries>false</EnableSourceControlManagerQueries><UseSharedCompilation>false</UseSharedCompilation></PropertyGroup>
          <Target Name="FixtureSourceLink" BeforeTargets="CoreCompile"><PropertyGroup><SourceLink>$(MSBuildProjectDirectory)/source-link.json</SourceLink></PropertyGroup></Target>
        </Project>
        """);
    foreach (var (repository, version, expected) in new[] {
        ("babalae/better-genshin-impact", "0.66.0", "official"),
        ("babalae/better-genshin-impact", "9.123.0-alpha.999", "official"),
        ("Bedrockx/better-genshin-impact", "0.66.0", "nonOfficial"),
        ("kaedelcb/better-genshin-impact", "9.123.0-alpha.999", "nonOfficial") })
    {
        File.WriteAllText(Path.Combine(fixtureRoot, "source-link.json"), JsonSerializer.Serialize(new { documents = new Dictionary<string, string> { [fixtureRoot + Path.DirectorySeparatorChar + "*"] = $"https://raw.githubusercontent.com/{repository}/{new string('b', 40)}/*" } }));
        var start = new ProcessStartInfo("dotnet") { WorkingDirectory = fixtureRoot, RedirectStandardOutput = true, RedirectStandardError = true, CreateNoWindow = true };
        foreach (var argument in new[] { "build", "Fixture.csproj", "--disable-build-servers", "--nologo", "-v", "quiet", "-p:Version=" + version }) start.ArgumentList.Add(argument);
        using var process = Process.Start(start)!;
        var output = process.StandardOutput.ReadToEndAsync(); var error = process.StandardError.ReadToEndAsync();
        if (!process.WaitForExit(30000)) { process.Kill(entireProcessTree: true); process.WaitForExit(); throw new Exception("fixture compiler timed out"); }
        if (process.ExitCode != 0) throw new Exception("fixture compilation failed: " + output.Result + error.Result);
        var result = HostOrigin.Inspect(Path.Combine(fixtureRoot, "bin/Debug/net8.0/BetterGI.dll"));
        Check(result.State == expected, repository + " " + version + ": " + result.Evidence);
    }
    File.WriteAllText(Path.Combine(fixtureRoot, "invalid.exe"), "not a PE image");
    Check(HostOrigin.Inspect(Path.Combine(fixtureRoot, "invalid.exe")).State == "unknown", "invalid executable");
}
finally
{
    if (Directory.GetParent(fixtureRoot)?.FullName == Path.GetFullPath(Path.GetTempPath()).TrimEnd(Path.DirectorySeparatorChar)) Directory.Delete(fixtureRoot, recursive: true);
}
Console.WriteLine(JsonSerializer.Serialize(new { checks, offline = true, versionWhitelist = false, executingTargetCode = false }));
