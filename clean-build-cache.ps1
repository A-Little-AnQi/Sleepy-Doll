<#
.SYNOPSIS
    Removes regenerable Rust caches while preserving executables and user data.
.DESCRIPTION
    Only cleans Cargo's debug/release cache directories and top-level Rust
    libraries/debug symbols. Does not use cargo clean, which also removes
    target/ui, target/bridge and development user directories.
    Run with -WhatIf to preview. The next Rust build recompiles dependencies.
#>
[CmdletBinding(SupportsShouldProcess = $true)]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$root = [System.IO.Path]::GetFullPath($PSScriptRoot)
$target = Join-Path $root 'target'
if (-not (Test-Path -LiteralPath $target)) {
    Write-Output 'No target directory to clean.'
    return
}

function Assert-OrdinaryDirectory([string]$Path) {
    $item = Get-Item -LiteralPath $Path -Force
    if (-not $item.PSIsContainer -or
        ($item.Attributes -band [System.IO.FileAttributes]::ReparsePoint)) {
        throw "Refusing a redirected or non-directory path: $Path"
    }
}

Assert-OrdinaryDirectory $target
$compilers = @(Get-Process -Name cargo, rustc -ErrorAction SilentlyContinue)
if ($compilers.Count -gt 0) {
    throw 'Cargo/rustc is running. Wait for compilation to finish before cleaning.'
}

$locks = [System.Collections.Generic.List[System.IO.FileStream]]::new()
try {
    $items = @()
    foreach ($profile in @('debug', 'release')) {
        $profileRoot = Join-Path $target $profile
        if (-not (Test-Path -LiteralPath $profileRoot)) { continue }
        Assert-OrdinaryDirectory $profileRoot

        # Refuse a build that still holds any of Cargo's existing lock files.
        # Keep handles open until cleanup finishes; create no new lock files.
        foreach ($name in @('.cargo-lock', '.cargo-build-lock', '.cargo-artifact-lock')) {
            $path = Join-Path $profileRoot $name
            if (Test-Path -LiteralPath $path) {
                $locks.Add([System.IO.File]::Open(
                    $path, [System.IO.FileMode]::Open,
                    [System.IO.FileAccess]::ReadWrite, [System.IO.FileShare]::None))
            }
        }

        foreach ($name in @('incremental', 'deps', 'build', '.fingerprint')) {
            $path = Join-Path $profileRoot $name
            if (Test-Path -LiteralPath $path) {
                Assert-OrdinaryDirectory $path
                $items += Get-Item -LiteralPath $path -Force
            }
        }
        $items += Get-ChildItem -LiteralPath $profileRoot -File -Force |
            Where-Object { $_.Extension -in @('.pdb', '.rlib', '.rmeta') }
    }

    # Validate every deletion target before removing anything. Never follow
    # directory junctions/symlinks into user data or outside this repository.
    $bytes = 0L
    foreach ($item in $items) {
        $path = [System.IO.Path]::GetFullPath($item.FullName)
        if (-not $path.StartsWith($target + '\', [System.StringComparison]::OrdinalIgnoreCase)) {
            throw "Refusing a path outside target: $path"
        }
        if ($item.PSIsContainer) {
            $redirects = @(Get-ChildItem -LiteralPath $path -Directory -Recurse -Force |
                Where-Object { $_.Attributes -band [System.IO.FileAttributes]::ReparsePoint })
            if ($redirects.Count -gt 0) {
                throw "Refusing a cache containing redirected directories: $path"
            }
            foreach ($file in Get-ChildItem -LiteralPath $path -File -Recurse -Force) {
                $bytes += $file.Length
            }
        } else {
            $bytes += $item.Length
        }
    }

    foreach ($item in $items) {
        if ($PSCmdlet.ShouldProcess($item.FullName, 'Remove regenerable Rust build cache')) {
            Remove-Item -LiteralPath $item.FullName -Recurse -Force
        }
    }
    Write-Output ('Selected cache files: {0:N2} GiB (logical size; hard links may share disk space).' -f ($bytes / 1GB))
    Write-Output 'Executables, user data, UI, bridge, installer payload and dist are preserved.'
} finally {
    foreach ($handle in $locks) { $handle.Dispose() }
}
