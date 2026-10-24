[CmdletBinding()]
param(
    [ValidateSet('Initialize', 'Check', 'Sign', 'Verify')][string]$Action = 'Check',
    [string[]]$Path,
    [string]$CertificateThumbprint = $env:SLEEPY_DOLL_SIGN_CERT_SHA1,
    [switch]$Optional
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
# npm/cmd may inherit a PowerShell 7 PSModulePath while invoking Windows PowerShell.
Import-Module (Join-Path $PSHOME 'Modules/Microsoft.PowerShell.Security/Microsoft.PowerShell.Security.psd1') -ErrorAction Stop
$root = Split-Path -Parent $PSScriptRoot
$env:TEMP = Join-Path $root 'target/tmp'
$env:TMP = $env:TEMP
$env:TMPDIR = $env:TEMP
[IO.Directory]::CreateDirectory($env:TEMP) | Out-Null
$friendlyName = 'Sleepy Doll Code Signing'
$codeSigningOid = '1.3.6.1.5.5.7.3.3'

function Get-SigningCertificate {
    if ($CertificateThumbprint) {
        $thumbprint = $CertificateThumbprint.Replace(' ', '')
        if ($thumbprint -notmatch '^[0-9a-fA-F]{40}$') { throw 'Invalid signing certificate thumbprint' }
        $certificate = Get-Item -LiteralPath "Cert:\CurrentUser\My\$thumbprint"
    } else {
        $certificates = @(Get-ChildItem Cert:\CurrentUser\My | Where-Object { $_.FriendlyName -eq $friendlyName })
        if ($certificates.Count -gt 1) { throw 'Multiple signing certificates found; set SLEEPY_DOLL_SIGN_CERT_SHA1 explicitly' }
        if (-not $certificates.Count) { return $null }
        $certificate = $certificates[0]
    }
    if ($codeSigningOid -notin @($certificate.EnhancedKeyUsageList | ForEach-Object { $_.ObjectId })) { throw 'Certificate does not allow code signing' }
    if (-not $certificate.HasPrivateKey -and $Action -ne 'Verify') { throw 'Signing certificate has no private key' }
    if ($Action -ne 'Verify' -and ($certificate.NotBefore -gt (Get-Date) -or $certificate.NotAfter -le (Get-Date))) { throw 'Signing certificate is not currently valid' }
    return $certificate
}

function Get-SignTool {
    $sdk = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits/10/bin'
    foreach ($directory in (Get-ChildItem -LiteralPath $sdk -Directory | Where-Object { $_.Name -match '^10\.\d+\.\d+\.\d+$' } | Sort-Object { [version]$_.Name } -Descending)) {
        $tool = Join-Path $directory.FullName 'x64/signtool.exe'
        if (Test-Path -LiteralPath $tool -PathType Leaf) { return $tool }
    }
    throw 'Windows SDK x64 SignTool is required for Authenticode signing'
}

function Get-BuildPaths {
    if (-not $Path) { throw 'At least one build file is required' }
    $prefix = [IO.Path]::GetFullPath((Join-Path $root 'target')) + [IO.Path]::DirectorySeparatorChar
    foreach ($file in $Path) {
        $full = [IO.Path]::GetFullPath($file)
        if (-not $full.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase) -or [IO.Path]::GetExtension($full) -notin @('.exe', '.dll')) {
            throw "Only EXE/DLL build outputs under target may be signed or verified: $full"
        }
        $item = Get-Item -LiteralPath $full -Force
        if ($item.PSIsContainer) { throw "Expected a build file: $full" }
        while ($item.FullName -ne $root) {
            if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "Signing path contains a link: $($item.FullName)" }
            $item = Get-Item -LiteralPath (Split-Path -Parent $item.FullName) -Force
        }
        $full
    }
}

function Get-TrustResult([string]$File) {
    if (-not ('SleepyDoll.BuildAuthenticode' -as [type])) {
        Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
namespace SleepyDoll {
    public static class BuildAuthenticode {
        [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
        private struct FileInfo {
            public uint Size;
            [MarshalAs(UnmanagedType.LPWStr)] public string Path;
            public IntPtr File, Subject;
        }
        [StructLayout(LayoutKind.Sequential)]
        private struct TrustData {
            public uint Size;
            public IntPtr Policy, Sip;
            public uint Ui, Revocation, Choice;
            public IntPtr File;
            public uint StateAction;
            public IntPtr State, Url;
            public uint Flags, Context;
        }
        [DllImport("wintrust.dll", ExactSpelling = true)]
        private static extern uint WinVerifyTrust(IntPtr window, ref Guid action, ref TrustData data);
        public static uint Verify(string path) {
            var info = new FileInfo { Size = (uint)Marshal.SizeOf(typeof(FileInfo)), Path = path };
            var pointer = Marshal.AllocHGlobal(Marshal.SizeOf(typeof(FileInfo)));
            Marshal.StructureToPtr(info, pointer, false);
            var data = new TrustData {
                Size = (uint)Marshal.SizeOf(typeof(TrustData)), Ui = 2, Choice = 1,
                File = pointer, StateAction = 1,
                // Cache-only retrieval and no revocation network requests during verification.
                Flags = 0x1000 | 0x10
            };
            var action = new Guid("00AAC56B-CD44-11d0-8CC2-00C04FC295EE");
            try { return WinVerifyTrust(new IntPtr(-1), ref action, ref data); }
            finally {
                data.StateAction = 2;
                WinVerifyTrust(new IntPtr(-1), ref action, ref data);
                Marshal.DestroyStructure(pointer, typeof(FileInfo));
                Marshal.FreeHGlobal(pointer);
            }
        }
    }
}
'@
    }
    return [SleepyDoll.BuildAuthenticode]::Verify($File)
}

function Assert-Signature([string]$File, $Certificate) {
    $signature = Get-AuthenticodeSignature -LiteralPath $File
    if (-not $signature.SignerCertificate -or $signature.SignerCertificate.Thumbprint -ne $Certificate.Thumbprint) {
        throw "Missing signature or unexpected signer: $File"
    }
    # A self-signed publisher is intentionally not installed into a trust store.
    # PowerShell maps untrusted-root to different status labels across versions.
    # Accept only the exact Windows untrusted-root HRESULT, never UnknownError in general.
    $selfSigned = $Certificate.Subject -eq $Certificate.Issuer
    $trustResult = Get-TrustResult $File
    if ($trustResult -ne 0 -and -not ($selfSigned -and $trustResult -eq 2148204809)) {
        throw ('Signature verification failed: {0} (0x{1:X8}: {2})' -f $File, $trustResult, $signature.StatusMessage)
    }
    if (-not $signature.TimeStamperCertificate) { throw "Missing Authenticode timestamp: $File" }
    $trust = if ($trustResult -eq 0) { 'trusted' } else { 'integrity verified; self-signed root is untrusted' }
    Write-Host "Signature: $File [$trust] $($Certificate.Thumbprint)"
}

$certificate = Get-SigningCertificate
if ($Action -eq 'Initialize') {
    if (-not $certificate) {
        $certificate = New-SelfSignedCertificate -Type CodeSigningCert -Subject 'CN=Sleepy Doll' -FriendlyName $friendlyName `
            -CertStoreLocation 'Cert:\CurrentUser\My' -KeyAlgorithm RSA -KeyLength 3072 -HashAlgorithm SHA256 `
            -KeyExportPolicy NonExportable -NotAfter (Get-Date).AddYears(3)
    }
    $publicCertificate = Join-Path $root 'target/release/Sleepy-Doll.cer'
    [IO.Directory]::CreateDirectory((Split-Path -Parent $publicCertificate)) | Out-Null
    Export-Certificate -Cert $certificate -FilePath $publicCertificate -Force | Out-Null
    Write-Host "Certificate: $($certificate.Subject) [$($certificate.Thumbprint)], expires $($certificate.NotAfter.ToString('yyyy-MM-dd'))"
    Write-Host "Public certificate: $publicCertificate (no private key)"
    return
}
if (-not $certificate) {
    if ($Optional -and $Action -in @('Check', 'Sign')) {
        Write-Host 'Authenticode signing is not configured; building an unsigned package.'
        return
    }
    throw 'No signing certificate found. Run scripts/sign-windows.ps1 -Action Initialize first.'
}
if ($Action -eq 'Check') {
    $null = Get-SignTool
    Write-Host "Authenticode signer: $($certificate.Subject) [$($certificate.Thumbprint)]"
    return
}
$files = @(Get-BuildPaths)
if ($Action -eq 'Sign') {
    $signTool = Get-SignTool
    foreach ($file in $files) {
        & $signTool sign /sha1 $certificate.Thumbprint /s My /fd SHA256 /d 'Sleepy Doll' /tr 'http://timestamp.digicert.com' /td SHA256 $file
        if ($LASTEXITCODE -ne 0) { throw "SignTool failed: $file (exit $LASTEXITCODE)" }
        Assert-Signature $file $certificate
    }
} else {
    foreach ($file in $files) { Assert-Signature $file $certificate }
}
