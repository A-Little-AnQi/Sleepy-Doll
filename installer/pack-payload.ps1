<#
.SYNOPSIS
    Packs dist\Sleepy-Doll into the two files the installer embeds.

.DESCRIPTION
    Writes target\setup\payload.bin (every file's bytes concatenated in path
    order, compressed as a raw DEFLATE stream) and target\setup\payload.json
    (the manifest, same order). sleepy-doll-setup reads both with
    include_bytes!, inflates the stream with flate2 and cuts it up by the
    manifest.

    user\ is left out on purpose: a development machine keeps real model keys
    there. *.log is left out as well. A bridge.config.json in the delivery
    folder holds the token issued on this machine, so it is replaced by
    bgi-bridge\bridge.config.example.json.

    Comments are ASCII only: Windows PowerShell reads a .ps1 without a BOM
    using the system ANSI code page, which would garble anything else.

.PARAMETER Source
    Product folder to pack. Defaults to ..\dist\Sleepy-Doll.

.PARAMETER Output
    Folder for payload.bin and payload.json. Defaults to ..\target\setup.
#>
[CmdletBinding()]
param(
    [string]$Source,
    [string]$Output
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# Defaults are resolved here, not in the param block: $PSScriptRoot is not set
# yet while parameters are being bound.
if (-not $Source) { $Source = Join-Path $PSScriptRoot '..\dist\Sleepy-Doll' }
if (-not $Output) { $Output = Join-Path $PSScriptRoot '..\target\setup' }

$bridgeFiles = @(
    'BgiBridge.Injector.exe',
    'BgiBridge.Bootstrap.dll',
    'BgiBridge.dll',
    'BgiBridge.runtimeconfig.json',
    'BgiBridge.deps.json',
    'BgiBridge.Recovery.exe',
    'BgiBridge.Recovery.dll',
    'BgiBridge.Recovery.runtimeconfig.json',
    'BgiBridge.Recovery.deps.json'
)
$pluginRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\plugins\bgi'))
$pluginPrefix = $pluginRoot.TrimEnd('\', '/') + [System.IO.Path]::DirectorySeparatorChar
$pluginFiles = @(
    Get-ChildItem -LiteralPath $pluginRoot -Recurse -File |
        ForEach-Object { 'plugins/bgi/' + $_.FullName.Substring($pluginPrefix.Length).Replace('\', '/') }
)

function Test-Included([string]$relative) {
    if ($relative -eq 'sleepy-doll.exe') { return $true }
    if ($relative -like 'plugins/bgi/*') { return $relative -in $pluginFiles }
    if ($relative -like 'bridge/*') {
        return $relative.Substring('bridge/'.Length) -in $bridgeFiles
    }
    # Runtime data (user/, bridge-cache/, WebView2/) must never enter setup.
    return $false
}

$source = (Resolve-Path -LiteralPath $Source).Path
$output = [System.IO.Path]::GetFullPath($Output)
[System.IO.Directory]::CreateDirectory($output) | Out-Null

# Relative paths use forward slashes; the sort keeps repeated runs identical.
$prefix = $source.TrimEnd('\', '/') + [System.IO.Path]::DirectorySeparatorChar
$files = @(
    Get-ChildItem -LiteralPath $source -Recurse -File |
        ForEach-Object {
            [pscustomobject]@{
                Path = $_.FullName.Substring($prefix.Length).Replace('\', '/')
                Full = $_.FullName
                Size = $_.Length
            }
        } |
        Where-Object { Test-Included $_.Path } |
        Sort-Object -Property Path
)

if ($files.Count -eq 0) {
    throw "nothing to pack: $source is empty"
}

# The payload carries the template, never the token issued on this machine.
$example = [System.IO.Path]::GetFullPath(
    (Join-Path $PSScriptRoot '..\bgi-bridge\bridge.config.example.json'))
if (-not (Test-Path -LiteralPath $example -PathType Leaf)) {
    throw "missing bridge config example: $example"
}
if ($files.Path -notcontains 'bridge/bridge.config.example.json') {
    $files = @(
        @($files) + [pscustomobject]@{
            Path = 'bridge/bridge.config.example.json'
            Full = $example
            Size = (Get-Item -LiteralPath $example).Length
        } | Sort-Object -Property Path
    )
}

$payload = Join-Path $output 'payload.bin'
$stream = [System.IO.File]::Create($payload)
try {
    # CompressionMode::Compress over the raw file stream is a bare DEFLATE
    # stream: no zip container and no gzip header.
    $deflate = [System.IO.Compression.DeflateStream]::new(
        $stream, [System.IO.Compression.CompressionMode]::Compress, $true)
    try {
        foreach ($file in $files) {
            $input = [System.IO.File]::OpenRead($file.Full)
            try { $input.CopyTo($deflate) } finally { $input.Dispose() }
        }
    }
    finally { $deflate.Dispose() }
}
finally { $stream.Dispose() }

$json = [System.Text.StringBuilder]::new()
[void]$json.Append('[')
for ($index = 0; $index -lt $files.Count; $index++) {
    if ($index -gt 0) { [void]$json.Append(',') }
    $escaped = $files[$index].Path.Replace('\', '\\').Replace('"', '\"')
    [void]$json.Append('{"path":"' + $escaped + '","size":' + $files[$index].Size + '}')
}
[void]$json.Append(']')

$manifest = Join-Path $output 'payload.json'
[System.IO.File]::WriteAllText($manifest, $json.ToString(), [System.Text.UTF8Encoding]::new($false))

$raw = ($files | Measure-Object -Property Size -Sum).Sum
$packed = (Get-Item -LiteralPath $payload).Length
Write-Host ("packed {0} files: {1:N0} bytes -> {2:N0} bytes" -f $files.Count, $raw, $packed)
Write-Host "  $payload"
Write-Host "  $manifest"
