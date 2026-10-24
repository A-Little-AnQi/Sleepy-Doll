param(
    [Parameter(Mandatory = $true)][string]$OneDragonPath,
    [ValidateRange(1, 65535)][int]$Port = 23001
)

$ErrorActionPreference = 'Stop'
$root = (Resolve-Path -LiteralPath $OneDragonPath).Path
if (-not (Test-Path -LiteralPath (Join-Path $root 'src/zzz_od/backend/entry/server.py') -PathType Leaf)) {
    throw 'ZZZ OneDragon MCP entry point was not found. Set -OneDragonPath to its source directory.'
}
$python = @(
    (Join-Path $root 'target/python-env/Scripts/python.exe'),
    (Join-Path $root '.venv/Scripts/python.exe')
) | Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } | Select-Object -First 1
if (-not $python) { throw 'ZZZ OneDragon Python environment was not found. Prepare target/python-env or .venv before starting the MCP service.' }
$env:PYTHONDONTWRITEBYTECODE = '1'
& $python -B (Join-Path $PSScriptRoot 'start-service.py') --one-dragon-dir $root --port $Port
exit $LASTEXITCODE
