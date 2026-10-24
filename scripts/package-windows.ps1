[CmdletBinding()]
param([ValidateSet('Check', 'Stage', 'Complete', 'Clean', 'Desktop')][string]$Action = 'Check')

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$staging = Join-Path $root 'target/setup/staging'
$distribution = Join-Path $root 'dist'
$version = (Get-Content (Join-Path $root 'package.json') -Raw -Encoding UTF8 | ConvertFrom-Json).version
$installerName = "Sleepy-Doll-$version-setup.exe"
$bridgeNames = @(
    'BgiBridge.Injector.exe', 'BgiBridge.Bootstrap.dll', 'BgiBridge.dll',
    'BgiBridge.runtimeconfig.json', 'BgiBridge.deps.json',
    'BgiBridge.Recovery.exe', 'BgiBridge.Recovery.dll',
    'BgiBridge.Recovery.runtimeconfig.json', 'BgiBridge.Recovery.deps.json'
)

function Get-Sha256([string]$Path) {
    $stream = [IO.File]::OpenRead($Path)
    $algorithm = [Security.Cryptography.SHA256]::Create()
    try { return [BitConverter]::ToString($algorithm.ComputeHash($stream)).Replace('-', '').ToLowerInvariant() }
    finally { $algorithm.Dispose(); $stream.Dispose() }
}

function Get-ProductFiles {
    [pscustomobject]@{ Relative = 'sleepy-doll.exe'; Source = (Join-Path $root 'target/release/sleepy-doll.exe') }
    foreach ($name in $bridgeNames) {
        [pscustomobject]@{ Relative = "bridge/$name"; Source = (Join-Path $root "target/bridge/$name") }
    }
    [pscustomobject]@{ Relative = 'bridge/bridge.config.example.json'; Source = (Join-Path $root 'bgi-bridge/bridge.config.example.json') }
    $plugins = (Resolve-Path (Join-Path $root 'plugins/bgi')).Path
    foreach ($file in Get-ChildItem -LiteralPath $plugins -Recurse -File) {
        [pscustomobject]@{ Relative = ('plugins/bgi/' + $file.FullName.Substring($plugins.Length + 1).Replace('\', '/')); Source = $file.FullName }
    }
}

function Remove-BuildFiles([string[]]$Paths) {
    foreach ($path in $Paths) {
        $full = [IO.Path]::GetFullPath($path)
        $prefix = [IO.Path]::GetFullPath((Join-Path $root 'target/setup')) + [IO.Path]::DirectorySeparatorChar
        if (-not $full.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)) { throw "Cleanup escaped setup: $full" }
    }
    $existing = @($Paths | Where-Object { Test-Path -LiteralPath $_ })
    if ($existing.Count -eq 0) { return }
    if ($env:GITHUB_ACTIONS -eq 'true') {
        if (-not $env:GITHUB_WORKSPACE -or [IO.Path]::GetFullPath($env:GITHUB_WORKSPACE).TrimEnd('\', '/') -ne [IO.Path]::GetFullPath($root).TrimEnd('\', '/')) { throw 'CI cleanup workspace does not match this checkout' }
        foreach ($path in $existing) {
            $item = Get-Item -LiteralPath $path -Force
            if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "Unexpected CI cleanup link: $path" }
            if ($item.PSIsContainer) { [IO.Directory]::Delete($path, $true) }
            else { [IO.File]::Delete($path) }
        }
    } else {
        & E:\tools\Remove-Directory.ps1 -Path $existing -Json
        if ($LASTEXITCODE -ne 0) { throw 'Setup cleanup failed' }
    }
    foreach ($path in $existing) {
        if (Test-Path -LiteralPath $path) { throw "Setup cleanup incomplete: $path" }
    }
}

function Clear-Staging {
    if (Test-Path -LiteralPath $staging) {
        $allowed = @((Get-ProductFiles).Relative)
        foreach ($item in Get-ChildItem -LiteralPath $staging -Recurse -Force) {
            if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "Unexpected staging link: $($item.FullName)" }
            $relative = $item.FullName.Substring($staging.Length + 1).Replace('\', '/')
            if ($item.PSIsContainer) {
                if (-not @($allowed | Where-Object { $_.StartsWith($relative + '/') }).Count) { throw "Unknown staging directory: $relative" }
            } elseif ($relative -notin $allowed) { throw "Unknown staging file: $relative" }
        }
        if ((Get-Item -LiteralPath $staging -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Staging must not be a link' }
    }
    Remove-BuildFiles @($staging, (Join-Path $root 'target/setup/payload.bin'), (Join-Path $root 'target/setup/payload.json'))
}

function Copy-Product([string]$Destination) {
    $files = @(Get-ProductFiles)
    foreach ($file in $files) {
        if (-not (Test-Path -LiteralPath $file.Source -PathType Leaf)) { throw "Missing build input: $($file.Source)" }
    }
    foreach ($file in $files) {
        $output = Join-Path $Destination $file.Relative
        [IO.Directory]::CreateDirectory((Split-Path -Parent $output)) | Out-Null
        Copy-Item -LiteralPath $file.Source -Destination $output -Force
    }
}

function Assert-Distribution {
    if (-not (Test-Path -LiteralPath $distribution)) { return }
    $allowed = @($installerName, "$installerName.sha256", "Sleepy-Doll-$version-release.json", 'android')
    foreach ($item in Get-ChildItem -LiteralPath $distribution -Force) {
        if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -or $item.Name -notin $allowed) { throw "Unexpected distribution item: $($item.FullName)" }
        if ($item.PSIsContainer -ne ($item.Name -eq 'android')) { throw "Wrong distribution item type: $($item.FullName)" }
        if ($item.Name -eq 'android') {
            foreach ($android in Get-ChildItem -LiteralPath $item.FullName -Force) {
                if ($android.PSIsContainer -or ($android.Attributes -band [IO.FileAttributes]::ReparsePoint) -or $android.Name -notmatch '^sleepy-doll-[0-9A-Za-z.+-]+-android-[0-9A-Za-z-]+\.apk(\.sha256)?$') { throw "Unexpected Android distribution item: $($android.FullName)" }
                $apk = if ($android.Name.EndsWith('.sha256')) { $android.FullName.Substring(0, $android.FullName.Length - 7) } else { $android.FullName }
                if (-not (Test-Path -LiteralPath $apk -PathType Leaf) -or -not (Test-Path -LiteralPath "$apk.sha256" -PathType Leaf)) { throw "Missing Android package/checksum pair: $apk" }
                if ((Get-Sha256 $apk) -ne (Get-Content -LiteralPath "$apk.sha256" -Raw).Split(' ')[0]) { throw "Android checksum mismatch: $apk" }
            }
        }
    }
    $windows = @($installerName, "$installerName.sha256", "Sleepy-Doll-$version-release.json")
    if (@($windows | Where-Object { Test-Path -LiteralPath (Join-Path $distribution $_) }).Count) {
        foreach ($name in $windows) {
            if (-not (Test-Path -LiteralPath (Join-Path $distribution $name) -PathType Leaf)) { throw "Missing Windows distribution file: $name" }
        }
        $installer = Join-Path $distribution $installerName
        $digest = Get-Sha256 $installer
        $metadata = Get-Content -LiteralPath (Join-Path $distribution "Sleepy-Doll-$version-release.json") -Raw -Encoding UTF8 | ConvertFrom-Json
        if ($digest -ne (Get-Content -LiteralPath "$installer.sha256" -Raw).Split(' ')[0] -or $digest -ne $metadata.sha256 -or $metadata.version -ne $version -or $metadata.size -ne (Get-Item -LiteralPath $installer).Length) { throw 'Windows package, checksum and metadata do not match' }
    }
}

switch ($Action) {
    'Check' {
        Assert-Distribution
        & (Join-Path $PSScriptRoot 'sign-windows.ps1') -Action Check -Optional
    }
    'Stage' {
        Clear-Staging
        Copy-Product $staging
        $binaries = @(Get-ProductFiles | Where-Object { [IO.Path]::GetExtension($_.Relative) -in @('.exe', '.dll') } | ForEach-Object { Join-Path $staging $_.Relative })
        & (Join-Path $PSScriptRoot 'sign-windows.ps1') -Action Sign -Path $binaries -Optional
    }
    'Clean' { Clear-Staging }
    'Complete' {
        Assert-Distribution
        $built = Join-Path $root 'target/release/sleepy-doll-setup.exe'
        if (-not (Test-Path -LiteralPath $built -PathType Leaf)) { throw 'Built installer is missing' }
        & (Join-Path $PSScriptRoot 'sign-windows.ps1') -Action Sign -Path $built -Optional
        [IO.Directory]::CreateDirectory($distribution) | Out-Null
        $output = Join-Path $distribution $installerName
        Copy-Item -LiteralPath $built -Destination $output -Force
        $digest = Get-Sha256 $output
        [IO.File]::WriteAllText("$output.sha256", "$digest  $installerName`n", [Text.UTF8Encoding]::new($false))
        $channel = (Get-Content (Join-Path $root 'release-channel.json') -Raw -Encoding UTF8 | ConvertFrom-Json).channel
        $metadata = [ordered]@{ version = $version; channel = $channel; size = (Get-Item -LiteralPath $output).Length; sha256 = $digest; builtAt = [DateTimeOffset]::UtcNow.ToString('o'); published = $false }
        [IO.File]::WriteAllText((Join-Path $distribution "Sleepy-Doll-$version-release.json"), (($metadata | ConvertTo-Json) + "`n"), [Text.UTF8Encoding]::new($false))
        Assert-Distribution
        Write-Host "Installer: $output"
    }
    'Desktop' {
        $desktop = Join-Path $root 'target/dev/desktop'
        Copy-Product $desktop
        $config = Join-Path $desktop 'bridge/bridge.config.json'
        if (-not (Test-Path -LiteralPath $config)) { Copy-Item -LiteralPath (Join-Path $root 'bgi-bridge/bridge.config.example.json') -Destination $config }
        Start-Process -FilePath (Join-Path $desktop 'sleepy-doll.exe') -WorkingDirectory $desktop -WindowStyle Hidden
    }
}
