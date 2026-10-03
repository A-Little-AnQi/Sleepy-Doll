# Live validation launcher for the bgi-agent-check file-transport driver.
# Intended to be started elevated by root:
#   Start-Process pwsh -Verb RunAs -WindowStyle Hidden -ArgumentList '-NoProfile','-File','E:\BetterGIProject\Sleepy-Doll\bgi-bridge\dev\Run-LiveValidation.ps1'
#
# Owned resources (created if missing, left in place for root to clean via the
# approved E:\tools\Remove-Directory.ps1 - do NOT delete unknown content here):
#   target\.tmp\shortcut-runtime\live-commands.jsonl   commands tailed by the driver
#   target\.tmp\shortcut-runtime\live-responses.jsonl  sanitized replies + events
#   target\.tmp\shortcut-runtime\live-driver.pid       driver process id
#   target\.tmp\shortcut-runtime\live-driver.exit      driver exit code
# Uses the original dist user config in place; no config/key copies are made and
# no credentials are written anywhere by this script.

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)   # repo root
$cache = Join-Path $root 'target\.tmp\shortcut-runtime'
$driver = Join-Path $root 'target\debug\bgi-agent-check.exe'
$config = Join-Path $root 'dist\Sleepy-Doll\user\config.json'

if (-not (Test-Path $driver)) { throw "driver not built: $driver" }
if (-not (Test-Path $config)) { throw "user config missing: $config" }
New-Item -ItemType Directory -Force -Path $cache | Out-Null

$commands  = Join-Path $cache 'live-commands.jsonl'
$responses = Join-Path $cache 'live-responses.jsonl'
$pidFile   = Join-Path $cache 'live-driver.pid'
$exitFile  = Join-Path $cache 'live-driver.exit'

# Root owns the command file; create it empty on first launch so the driver can tail it.
if (-not (Test-Path $commands)) { New-Item -ItemType File -Path $commands | Out-Null }
if (-not (Test-Path $responses)) { New-Item -ItemType File -Path $responses | Out-Null }

# All child temp state stays inside the registered cache directory.
$env:TEMP = $cache
$env:TMP  = $cache
$env:DOTNET_BUNDLE_EXTRACT_BASE_DIR = $cache

$process = Start-Process -FilePath $driver -ArgumentList @(
    "`"$config`"", '--commands', "`"$commands`"", '--responses', "`"$responses`""
) -WindowStyle Hidden -PassThru
Set-Content -Path $pidFile -Value $process.Id
$process.WaitForExit()
Set-Content -Path $exitFile -Value $process.ExitCode
