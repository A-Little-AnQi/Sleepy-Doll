[CmdletBinding()]
param(
    [ValidateSet('Build', 'Check')][string]$Action = 'Check',
    [ValidateSet('march7th', 'maaend', 'zzz')][string[]]$Plugin = @('march7th', 'maaend', 'zzz')
)
$ErrorActionPreference = 'Stop'
$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$output = Join-Path $root 'dist\plugins'
$utf8 = [Text.UTF8Encoding]::new($false)
$binaries = @{
    march7th = @('march7th_bridge.dll', 'march7th_injector.exe')
    maaend = @('maaend-bridge.exe')
    zzz = @('zzz_bridge.dll', 'zzz_injector.exe')
}
$ports = @{march7th=23111; maaend=23121; zzz=23101}
$tokenNames = @{march7th='MARCH7TH_BRIDGE_TOKEN'; maaend='MAAEND_BRIDGE_TOKEN'; zzz='ZZZ_BRIDGE_TOKEN'}

function Get-Sha256([string]$Path) {
    $stream = [IO.File]::OpenRead($Path)
    $algorithm = [Security.Cryptography.SHA256]::Create()
    try { return [BitConverter]::ToString($algorithm.ComputeHash($stream)).Replace('-', '').ToLowerInvariant() }
    finally { $algorithm.Dispose(); $stream.Dispose() }
}

function Assert-NoLink([string]$Path) {
    $cursor = [IO.Path]::GetFullPath($Path)
    while ($cursor.StartsWith($root + '\', [StringComparison]::OrdinalIgnoreCase)) {
        if ((Test-Path -LiteralPath $cursor) -and ((Get-Item -LiteralPath $cursor -Force).Attributes -band [IO.FileAttributes]::ReparsePoint)) {
            throw "Redirected package path: $cursor"
        }
        $cursor = [IO.Path]::GetDirectoryName($cursor)
    }
}

function Assert-Package([string]$Directory, [string]$Id) {
    Assert-NoLink $Directory
    $metadataPath = Join-Path $Directory 'bridge-package.json'
    $metadata = Get-Content -LiteralPath $metadataPath -Raw -Encoding UTF8 | ConvertFrom-Json
    if ($metadata.schemaVersion -ne 1 -or $metadata.id -ne $Id -or $metadata.platform -ne 'win-x64' -or $metadata.published -ne $false) {
        throw "Invalid adapter package metadata: $Directory"
    }
    $expected = @('bridge-package.json')
    foreach ($file in $metadata.files) {
        if ($file.path -notmatch '^[A-Za-z0-9_.-]+(?:/[A-Za-z0-9_.-]+)*$' -or $file.path.Split('/') -contains '..' -or $file.path -in $expected) {
            throw "Unsafe or duplicate package entry: $($file.path)"
        }
        $path = Join-Path $Directory $file.path
        Assert-NoLink $path
        if (-not (Test-Path -LiteralPath $path -PathType Leaf) -or (Get-Item -LiteralPath $path).Length -ne $file.size -or (Get-Sha256 $path) -ne $file.sha256) {
            throw "Adapter package hash mismatch: $path"
        }
        $expected += $file.path
    }
    foreach ($item in Get-ChildItem -LiteralPath $Directory -Recurse -Force) {
        Assert-NoLink $item.FullName
        $relative = $item.FullName.Substring($Directory.Length + 1).Replace('\','/')
        if ($item.PSIsContainer) {
            if (-not @($expected | Where-Object { $_.StartsWith($relative + '/') }).Count) { throw "Unknown package directory: $relative" }
        } elseif ($relative -notin $expected) { throw "Unknown package file: $relative" }
    }
    $manifest = Get-Content -LiteralPath (Join-Path $Directory '.sleepy-doll-plugin\plugin.json') -Raw -Encoding UTF8 | ConvertFrom-Json
    $mcpServers = $manifest.PSObject.Properties['mcpServers']
    $adapters = $manifest.PSObject.Properties['adapters']
    if ($manifest.id -ne $Id -or $manifest.version -ne $metadata.version -or
        ($mcpServers -and @($mcpServers.Value).Count) -or ($adapters -and @($adapters.Value).Count)) {
        throw "Adapter manifest mismatch: $Directory"
    }
    foreach ($name in $binaries[$Id]) {
        $path = Join-Path $Directory "bridge\$name"
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { throw "Missing adapter binary: $path" }
        $hash = (Get-Content -LiteralPath "$path.sha256" -Raw).Split(' ')[0]
        if ($hash -ne (Get-Sha256 $path)) { throw "Binary checksum mismatch: $path" }
    }
    Write-Host "Adapter package verified: $Id $($metadata.version)"
}

if ($Action -eq 'Check') {
    if (-not (Test-Path -LiteralPath $output)) { exit 0 }
    foreach ($directory in Get-ChildItem -LiteralPath $output -Force) {
        if (-not $directory.PSIsContainer -or $directory.Name -notin $binaries.Keys) { throw "Unexpected adapter distribution item: $($directory.FullName)" }
        Assert-Package $directory.FullName $directory.Name
    }
    exit 0
}

foreach ($id in $Plugin) {
    $source = Join-Path $root "plugins\$id"
    $manifestPath = Join-Path $source '.sleepy-doll-plugin\plugin.json'
    $manifest = Get-Content -LiteralPath $manifestPath -Raw -Encoding UTF8 | ConvertFrom-Json
    if ($manifest.id -ne $id) { throw "Source plugin ID mismatch: $source" }
    $files = @{}
    $files['.sleepy-doll-plugin/plugin.json'] = $manifestPath
    foreach ($file in Get-ChildItem -LiteralPath (Join-Path $source 'skills') -Recurse -File) {
        if ($file.Extension -ne '.md') { throw "Unsupported skill package file: $($file.FullName)" }
        $files[$file.FullName.Substring($source.Length + 1).Replace('\','/')] = $file.FullName
    }
    foreach ($name in $binaries[$id]) {
        $path = Join-Path $root "target\bridge\$id\$name"
        Assert-NoLink $path
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { throw "Build adapter first: $path" }
        $nativeSource = Get-ChildItem -LiteralPath (Join-Path $source 'bridge') -Recurse -File |
            Where-Object { $_.Extension -in @('.cpp', '.hpp', '.h', '.rc', '.cmd') } |
            Sort-Object LastWriteTimeUtc -Descending | Select-Object -First 1
        if ($nativeSource -and (Get-Item -LiteralPath $path).LastWriteTimeUtc -lt $nativeSource.LastWriteTimeUtc) {
            throw "Native payload predates source; rebuild adapter before packaging: $path"
        }
        $files["bridge/$name"] = $path
    }
    $directory = Join-Path $output $id
    if (Test-Path -LiteralPath $directory) { Assert-Package $directory $id }
    $generated = @{}
    foreach ($name in $binaries[$id]) {
        $hash = Get-Sha256 $files["bridge/$name"]
        $generated["bridge/$name.sha256"] = "$hash  $name`n"
    }
    $token = $tokenNames[$id]
    $port = $ports[$id]
    if ($id -eq 'maaend') {
        $parameters = '[ValidateRange(1,65535)][int]$UpstreamPort = 12701'
        $command = '& (Join-Path $PSScriptRoot ''bridge\maaend-bridge.exe'') --upstream-port $UpstreamPort --token-env ' + $token + ' --port ' + $port
        $manual = '先手动启动 MaaEnd 并启用本机 HTTP 服务，再运行 `start-bridge.ps1`。后端端口不是 12701 时使用 `-UpstreamPort` 指定；桥不会自动启动 MaaEnd、控制器或 Agent。'
    } else {
        $parameters = '[Parameter(Mandatory=$true)][ValidateRange(1,2147483647)][int]$TargetProcessId'
        $command = '& (Join-Path $PSScriptRoot ''bridge\' + $id + '_injector.exe'') --pid $TargetProcessId --token-env ' + $token + ' --port ' + $port
        $manual = '先手动启动对应工具，在任务管理器取得其 GUI 进程 ID，再运行 `start-bridge.ps1 -TargetProcessId <进程 ID>`。桥只附着明确指定的现有进程，不会启动工具或游戏。'
    }
    $generated['start-bridge.ps1'] = "param($parameters)`r`n`$ErrorActionPreference = 'Stop'`r`nif (-not [Environment]::GetEnvironmentVariable('$token')) { throw 'Missing process environment: $token' }`r`n$command`r`nexit `$LASTEXITCODE`r`n"
    $generated['README.md'] = "# $($manifest.name)`n`n版本：$($manifest.version)。这是独立插件目录；在 Sleepy-Doll 的插件导入入口选择本目录，按需启用。`n`n$manual`n`nSleepy-Doll 和启动桥的进程必须取得同一份 $token 环境变量，令牌不得写入插件包。默认桥地址为 http://127.0.0.1:$port。导入插件与启动桥分别操作；均不自动运行游戏任务。`n`n源码和开发说明位于正式工程 plugins/$id/。包内只含本插件的原生桥、清单、技能和说明，不含第三方工具、用户配置或测试程序。`n`n本轮仅完成编译、伪宿主和隔离夹具验证；未向真实第三方工具注入、连接或执行游戏。bridge-package.json 记录本目录文件校验值，原生文件各有对应 .sha256。`n"
    $expected = @($files.Keys) + @($generated.Keys) + @('bridge-package.json')
    if (Test-Path -LiteralPath $directory) {
        foreach ($item in Get-ChildItem -LiteralPath $directory -Recurse -File -Force) {
            $relative = $item.FullName.Substring($directory.Length + 1).Replace('\','/')
            if ($relative -notin $expected) { throw "Obsolete adapter entry requires reviewed cleanup: $($item.FullName)" }
        }
    }
    New-Item -ItemType Directory -Path $directory -Force | Out-Null
    foreach ($relative in $files.Keys) {
        $destination = Join-Path $directory $relative
        Assert-NoLink $files[$relative]
        Assert-NoLink $destination
        New-Item -ItemType Directory -Path ([IO.Path]::GetDirectoryName($destination)) -Force | Out-Null
        Copy-Item -LiteralPath $files[$relative] -Destination $destination -Force
    }
    foreach ($relative in $generated.Keys) {
        [IO.File]::WriteAllText((Join-Path $directory $relative), $generated[$relative], $utf8)
    }
    $entries = @(foreach ($file in Get-ChildItem -LiteralPath $directory -Recurse -File -Force | Sort-Object FullName) {
        if ($file.Name -eq 'bridge-package.json') { continue }
        [ordered]@{path=$file.FullName.Substring($directory.Length + 1).Replace('\','/'); size=$file.Length; sha256=(Get-Sha256 $file.FullName)}
    })
    $metadata = [ordered]@{schemaVersion=1; id=$id; version=$manifest.version; platform='win-x64'; published=$false; files=$entries}
    [IO.File]::WriteAllText((Join-Path $directory 'bridge-package.json'), (($metadata | ConvertTo-Json -Depth 8) + "`n"), $utf8)
    Assert-Package $directory $id
}
exit 0
