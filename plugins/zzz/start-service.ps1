param(
    [Parameter(Mandatory = $true)][int]$TargetPid,
    [string]$TokenEnv = 'ZZZ_BRIDGE_TOKEN',
    [int]$Port = 23101,
    [switch]$Check
)

$ErrorActionPreference = 'Stop'

# Native zzz reflection bridge launcher: attach to an existing OneDragon GUI process.
# The legacy MCP service (port 23001) is retired; this entry never starts the target
# and never writes into the target install directory.

$bridgeDir = Join-Path $PSScriptRoot '..\..\..\target\bridge\zzz'
$injector = Join-Path $bridgeDir 'zzz_injector.exe'
if (-not (Test-Path -LiteralPath $injector -PathType Leaf)) {
    throw "Injector not found at $injector. Run plugins/zzz/bridge/build.cmd first."
}

$injectorArgs = @('--pid', "$TargetPid", '--token-env', $TokenEnv, '--port', "$Port")
if ($Check) { $injectorArgs += '--check' }

& $injector @injectorArgs
exit $LASTEXITCODE
