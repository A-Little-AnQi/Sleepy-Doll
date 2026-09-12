param(
    [Parameter(Mandatory=$true)][string]$TestExecutable,
    [Parameter(Mandatory=$true)][string]$BetterGiPath,
    [Parameter(Mandatory=$true)][string]$ResultDirectory,
    [switch]$StopAfterTest
)
$ErrorActionPreference = 'Stop'
if (-not (Test-Path -LiteralPath $ResultDirectory -PathType Container)) {
    throw 'ResultDirectory must be an existing directory.'
}
try {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    if (-not ([Security.Principal.WindowsPrincipal]::new($identity)).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        throw 'Run this test with administrator rights.'
    }
    $target = (Resolve-Path -LiteralPath $BetterGiPath).Path
    $testBinary = (Resolve-Path -LiteralPath $TestExecutable).Path
    $existing = @(Get-CimInstance Win32_Process -Filter "Name = 'BetterGI.exe'")
    if ($existing.Count -gt 1 -or ($existing.Count -eq 1 -and $existing[0].ExecutablePath -ne $target)) {
        throw 'A different BetterGI instance is running. Close it before this dedicated test.'
    }
    if ($existing.Count -eq 0) {
        Start-Process -FilePath $target -WorkingDirectory (Split-Path -Parent $target) -WindowStyle Hidden
        Start-Sleep -Seconds 12
    }
    $test = Start-Process -FilePath $testBinary -ArgumentList '--ignored --exact real_bridge_switch_round_trip --nocapture' -WindowStyle Hidden -PassThru -Wait -RedirectStandardOutput (Join-Path $ResultDirectory 'test.log') -RedirectStandardError (Join-Path $ResultDirectory 'test-error.log')
    $code = $test.ExitCode
    if ($StopAfterTest -and $code -eq 0) {
        $targets = @(Get-CimInstance Win32_Process -Filter "Name = 'BetterGI.exe'" | Where-Object { $_.ExecutablePath -eq $target })
        foreach ($process in $targets) { Stop-Process -Id $process.ProcessId }
    }
    @{ completed=$true; exitCode=$code } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $ResultDirectory 'result.json')
    exit $code
} catch {
    @{ completed=$false; error=$_.Exception.Message } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $ResultDirectory 'result.json')
    exit 1
}
