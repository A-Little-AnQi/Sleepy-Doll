# Requires an initialized local signer, Windows SDK, timestamp access and a built bridge.
[CmdletBinding()]
param()
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$sign = Join-Path $PSScriptRoot 'sign-windows.ps1'
$scratch = Join-Path $root 'target/.tmp/windows-signing'
$fixture = Join-Path $root 'target/bridge/BgiBridge.Recovery.exe'
$env:TEMP = Join-Path $root 'target/tmp'
$env:TMP = $env:TEMP
$env:TMPDIR = $env:TEMP

function Assert-Rejected([scriptblock]$Operation, [string]$Label) {
    $rejected = $false
    try { & $Operation } catch { $rejected = $true }
    if (-not $rejected) { throw "Unexpectedly accepted: $Label" }
    Write-Host "PASS: $Label rejected"
}

if (Test-Path -LiteralPath $scratch) { throw "Test directory already exists: $scratch" }
if (-not (Test-Path -LiteralPath $fixture -PathType Leaf)) { throw 'Build the bridge before running signing checks' }
& $sign -Action Check
[IO.Directory]::CreateDirectory($scratch) | Out-Null
try {
    $signed = Join-Path $scratch 'signed.exe'
    Copy-Item -LiteralPath $fixture -Destination $signed
    & $sign -Action Sign -Path $signed
    & $sign -Action Verify -Path $signed
    Write-Host 'PASS: real PE signature and timestamp verified'

    $tampered = Join-Path $scratch 'tampered.exe'
    $bytes = [IO.File]::ReadAllBytes($signed)
    $bytes[0x40] = $bytes[0x40] -bxor 1
    [IO.File]::WriteAllBytes($tampered, $bytes)
    Assert-Rejected { & $sign -Action Verify -Path $tampered } 'Modified PE digest'

    Assert-Rejected { & $sign -Action Verify -Path $fixture } 'Unsigned build input'
    Assert-Rejected { & $sign -Action Check -Optional -CertificateThumbprint ('0' * 40) } 'Explicit unavailable signer'
    Assert-Rejected { & $sign -Action Sign -Path (Join-Path $root 'Cargo.toml') } 'Source path'
} finally {
    # Fixed owned directory only. The local test is not a dependency of CI.
    & E:\tools\Remove-Directory.ps1 -Path $scratch -Json
    if ($LASTEXITCODE -ne 0 -or (Test-Path -LiteralPath $scratch)) { throw "Signing test cleanup failed: $scratch" }
}
