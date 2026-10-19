[CmdletBinding()]
param([ValidateSet('build','publish','preview','check')][string]$Action = 'build')

$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$androidRoot = Join-Path $projectRoot 'android'
$runtimeRoot = Join-Path $projectRoot 'target/tmp/android'
$gradleRoot = Join-Path $projectRoot 'target/android/gradle'
New-Item -ItemType Directory -Path $runtimeRoot,$gradleRoot -Force | Out-Null
$env:TEMP = $env:TMP = $env:TMPDIR = $runtimeRoot
$env:GRADLE_USER_HOME = $gradleRoot
$env:ANDROID_USER_HOME = Join-Path $projectRoot 'target/android/preferences'
$localSettings = @{}
if (Test-Path -LiteralPath (Join-Path $androidRoot 'local.properties')) {
    foreach ($line in Get-Content -LiteralPath (Join-Path $androidRoot 'local.properties')) {
        if ($line -match '^([^#=]+)=(.*)$') { $localSettings[$matches[1].Trim()] = $matches[2].Trim() }
    }
}
if ($localSettings['java.home']) { $env:JAVA_HOME = $localSettings['java.home'] }
if ($localSettings['sdk.dir']) { $env:ANDROID_HOME = $env:ANDROID_SDK_ROOT = $localSettings['sdk.dir'] }
if (-not $env:JAVA_HOME -or -not (Test-Path -LiteralPath (Join-Path $env:JAVA_HOME 'bin/java.exe'))) { throw 'Set JAVA_HOME to JDK 17 or newer, or java.home in android/local.properties.' }
$env:JAVA_OPTS = "-Djava.io.tmpdir=$runtimeRoot -Duser.home=$env:USERPROFILE"
[string[]]$tasks = @(switch ($Action) {
    'build' { @(':app:assembleDebug',':ui:assembleRelease') }
    'publish' { @(':ui:publishReleasePublicationToProjectRepository',':app:assembleDebug') }
    'preview' { @(':preview-host:assembleDebug') }
    'check' { @(':ui:lintRelease',':app:lintDebug',':preview-host:lintDebug') }
})
& (Join-Path $androidRoot 'gradlew.bat') -p $androidRoot --no-daemon --console=plain "-Djava.io.tmpdir=$runtimeRoot" @tasks
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
if ($Action -in @('build','publish')) {
    $distribution = Join-Path $projectRoot 'dist/android'
    New-Item -ItemType Directory -Path $distribution -Force | Out-Null
    $version = (Get-Content (Join-Path $projectRoot 'package.json') -Raw -Encoding UTF8 | ConvertFrom-Json).version
    $apk = Join-Path $distribution "sleepy-doll-$version-android-debug.apk"
    Copy-Item -LiteralPath (Join-Path $projectRoot 'target/android/build/app/outputs/apk/debug/app-debug.apk') -Destination $apk -Force
    $stream = [IO.File]::OpenRead($apk)
    $algorithm = [Security.Cryptography.SHA256]::Create()
    try { $digest = [BitConverter]::ToString($algorithm.ComputeHash($stream)).Replace('-', '').ToLowerInvariant() }
    finally { $algorithm.Dispose(); $stream.Dispose() }
    [IO.File]::WriteAllText("$apk.sha256", "$digest  $([IO.Path]::GetFileName($apk))`n", [Text.UTF8Encoding]::new($false))
}
if ($Action -eq 'preview') {
    $assets = Join-Path $projectRoot 'target/frame-android/assets'
    New-Item -ItemType Directory -Path $assets -Force | Out-Null
    Copy-Item -LiteralPath (Join-Path $projectRoot 'target/android/build/preview-host/outputs/apk/debug/preview-host-debug.apk') -Destination (Join-Path $assets 'preview.apk') -Force
    & node (Join-Path $PSScriptRoot 'android-assets.mjs')
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}
