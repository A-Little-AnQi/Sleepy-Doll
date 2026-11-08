param(
    [ValidateSet('march7th', 'maaend', 'zzz')]
    [string[]]$Plugin = @('march7th', 'maaend', 'zzz'),
    [switch]$SkipBuild
)
$ErrorActionPreference = 'Stop'
$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
$work = Join-Path $root 'target\.tmp\local-adapters'
$env:TEMP = $work
$env:TMP = $work
$env:TMPDIR = $work
New-Item -ItemType Directory -Path $work -Force | Out-Null
$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
$installation = if (Test-Path -LiteralPath $vswhere) {
    & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
}
$vcvars = if ($installation) { Join-Path $installation 'VC\Auxiliary\Build\vcvars64.bat' }
if (-not $vcvars -or -not (Test-Path -LiteralPath $vcvars)) { throw 'MSVC x64 toolchain unavailable' }
$runner = Join-Path $work 'native-runner.exe'
$object = Join-Path $work 'native-runner.obj'
$compile = Join-Path $work 'compile-runner.cmd'
$remove = Join-Path $root 'scripts\remove-build-output.ps1'
$helperFiles = @($runner, $object, $compile)
try {
    if (-not $SkipBuild) {
        foreach ($id in $Plugin) {
            $build = Join-Path $root "plugins\$id\bridge\build.cmd"
            if (-not (Test-Path -LiteralPath $build)) { throw "Missing native bridge build: $build" }
            & $build
            if ($LASTEXITCODE -ne 0) { throw "$id native bridge build failed: $LASTEXITCODE" }
        }
    }
    $source = Join-Path $PSScriptRoot 'native-runner.cpp'
    @"
@echo off
call "$vcvars" >nul
if errorlevel 1 exit /b 1
cl /nologo /std:c++20 /utf-8 /EHsc /W4 /O2 /MT /Fe"$runner" /Fo"$object" "$source" /link user32.lib
exit /b %errorlevel%
"@ | Set-Content -LiteralPath $compile -Encoding ascii
    & $compile
    if ($LASTEXITCODE -ne 0) { throw 'Native isolation runner build failed' }
    & $runner --verify-isolation $root
    if ($LASTEXITCODE -ne 0) { throw 'Native isolation self-test failed' }
    foreach ($id in $Plugin) {
        $output = Join-Path $root "target\bridge\$id"
        $tests = @(Get-ChildItem -LiteralPath $output -File -Filter '*.exe' |
            Where-Object { $_.BaseName -match '(?i)test' -and $_.BaseName -notmatch '(?i)inject' })
        if ($tests.Count -ne 1) { throw "Expected exactly one native test executable in $output" }
        $dumpCommand = '""' + $vcvars + '" >nul && dumpbin /nologo /imports "' + $tests[0].FullName + '""'
        $imports = & $env:ComSpec /d /s /c $dumpCommand 2>&1
        if ($LASTEXITCODE -ne 0) { throw "Cannot inspect $id test imports" }
        $forbidden = '(?i)\b(?:CreateRemoteThread(?:Ex)?|VirtualAllocEx|WriteProcessMemory|OpenProcess|CreateProcess[AW]|ShellExecute(?:Ex)?[AW]|SendInput|keybd_event|mouse_event|WinHttpConnect|connect)\b'
        if (($imports -join "`n") -match $forbidden) {
            throw "$id test binary imports a real process, input or network operation: $($Matches[0])"
        }
        Write-Host "[$id] imports checked; starting fixture in isolated desktop and single-process Job"
        & $runner $root $tests[0].FullName
        if ($LASTEXITCODE -ne 0) { throw "$id isolated fixture failed: $LASTEXITCODE" }
    }
} finally {
    $present = @($helperFiles | Where-Object { Test-Path -LiteralPath $_ })
    if ($present.Count) {
        & $remove -Path $present
        if ($LASTEXITCODE -ne 0 -or @($present | Where-Object { Test-Path -LiteralPath $_ }).Count) {
            throw "Native validation helper cleanup failed: $($present -join ', ')"
        }
    }
}
