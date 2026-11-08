# Export registrations from a freshly built bridge without invoking handlers.
param([string]$AssemblyPath, [switch]$Check)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$temporary = Join-Path $projectRoot 'target/tmp'
if (-not (Test-Path -LiteralPath $temporary)) { New-Item -ItemType Directory -Path $temporary | Out-Null }
$currentTemporary = if ($env:TEMP) { [IO.Path]::GetFullPath($env:TEMP) } else { '' }
if (-not $currentTemporary.StartsWith(([IO.Path]::GetFullPath($temporary) + [IO.Path]::DirectorySeparatorChar), [StringComparison]::OrdinalIgnoreCase)) {
    $env:TEMP = $temporary; $env:TMP = $temporary; $env:TMPDIR = $temporary
}
$env:TMP = $env:TEMP; $env:TMPDIR = $env:TEMP
$mode = if ($Check) { '--check-interfaces' } else { '--export-interfaces' }
$destination = Join-Path $PSScriptRoot 'bgi-method-metadata.json'
$arguments = @('run', '--project', (Join-Path $PSScriptRoot 'ProductionCoreChecks.csproj'), '--no-launch-profile', '-p:UseSharedCompilation=false', '--', $mode, $destination)
if ($AssemblyPath) { $arguments += (Resolve-Path -LiteralPath $AssemblyPath).Path }
& dotnet @arguments
if ($LASTEXITCODE -ne 0) { throw 'Bridge interface snapshot verification failed.' }
