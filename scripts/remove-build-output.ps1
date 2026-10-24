# Deletes only explicitly selected, regenerable workspace output.
[CmdletBinding(SupportsShouldProcess = $true)]
param(
    [Parameter(Mandatory = $true)][string[]]$Path,
    [switch]$Json
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..')).TrimEnd('\', '/')
$prefix = $root + [IO.Path]::DirectorySeparatorChar
if ($env:GITHUB_ACTIONS -eq 'true' -and
    (-not $env:GITHUB_WORKSPACE -or [IO.Path]::GetFullPath($env:GITHUB_WORKSPACE).TrimEnd('\', '/') -ne $root)) {
    throw 'Cleanup workspace does not match this checkout'
}

function Assert-NoLinks([string]$FullPath) {
    $cursor = $FullPath
    while ($cursor -and $cursor.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)) {
        if (Test-Path -LiteralPath $cursor) {
            $item = Get-Item -LiteralPath $cursor -Force
            if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) {
                throw "Refusing a redirected cleanup path: $cursor"
            }
        }
        $cursor = [IO.Path]::GetDirectoryName($cursor)
    }
}

$targets = @($Path | ForEach-Object { [IO.Path]::GetFullPath($_).TrimEnd('\', '/') } | Select-Object -Unique)
foreach ($target in $targets) {
    if (-not $target.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)) { throw "Cleanup escaped workspace: $target" }
    $relative = $target.Substring($prefix.Length).Replace('\', '/')
    $allowed = $relative -match '^target/\.tmp/[^/]+(?:/.*)?$' -or
        $relative -match '^target/tmp/frame/(dev|build|serve|setup|typecheck)(?:/.*)?$' -or
        $relative -match '^target/setup/(staging|payload\.bin|payload\.json)$' -or
        $relative -match '^target/(debug|release)/(incremental|deps|build|\.fingerprint|[^/]+\.(pdb|rlib|rmeta))$'
    if (-not $allowed) { throw "Not an approved build-output path: $relative" }
    Assert-NoLinks $target
}

# This workstation's mandatory channel takes precedence. A rejection is final.
$localChannel = 'E:\tools\Remove-Directory.ps1'
if ($env:GITHUB_ACTIONS -ne 'true' -and (Test-Path -LiteralPath $localChannel -PathType Leaf)) {
    & $localChannel -Path $targets -WhatIf:$WhatIfPreference -Json:$Json
    if ($LASTEXITCODE -ne 0) { throw 'Local deletion channel rejected or failed cleanup' }
} else {
    # Check the whole selection before deleting; never traverse a link.
    function Assert-Tree([string]$ItemPath) {
        $item = Get-Item -LiteralPath $ItemPath -Force
        if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "Refusing a link in build output: $ItemPath" }
        if ($item.PSIsContainer) {
            foreach ($child in Get-ChildItem -LiteralPath $ItemPath -Force) { Assert-Tree $child.FullName }
        }
    }
    function Remove-Tree([string]$ItemPath) {
        $item = Get-Item -LiteralPath $ItemPath -Force
        Assert-NoLinks $ItemPath
        if ($item.PSIsContainer) {
            foreach ($child in Get-ChildItem -LiteralPath $ItemPath -Force) { Remove-Tree $child.FullName }
            [IO.Directory]::Delete($ItemPath, $false)
        } else {
            $item.IsReadOnly = $false
            [IO.File]::Delete($ItemPath)
        }
    }
    foreach ($target in $targets) { if (Test-Path -LiteralPath $target) { Assert-Tree $target } }
    foreach ($target in $targets) {
        if ((Test-Path -LiteralPath $target) -and $PSCmdlet.ShouldProcess($target, 'Remove regenerable build output')) { Remove-Tree $target }
    }
    if ($Json) { [pscustomobject]@{ Paths = $targets; WhatIf = [bool]$WhatIfPreference } | ConvertTo-Json -Compress }
}
if (-not $WhatIfPreference) {
    foreach ($target in $targets) { if (Test-Path -LiteralPath $target) { throw "Cleanup incomplete: $target" } }
}
exit 0
