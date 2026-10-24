# Export actual bridge registrations without invoking handlers or loading BetterGI.
# Build bgi-bridge/managed/BgiBridge.csproj first; output is a maintained test/index input.
param([string]$AssemblyPath)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
if (-not $AssemblyPath) {
    $AssemblyPath = Join-Path $projectRoot 'target/dotnet/managed/net8.0-windows/win-x64/BgiBridge.dll'
}
$AssemblyPath = (Resolve-Path -LiteralPath $AssemblyPath).Path
$bin = Split-Path -Parent $AssemblyPath
$context = [System.Runtime.Loader.AssemblyLoadContext]::Default
$context.add_Resolving({ param($context, $name)
    $candidate = Join-Path $bin "$($name.Name).dll"
    if (Test-Path -LiteralPath $candidate) { return $context.LoadFromAssemblyPath($candidate) }
    return $null
})
$assembly = $context.LoadFromAssemblyPath($AssemblyPath)
$registryType = $assembly.GetType('BgiBridge.Catalog.MethodRegistry', $true)
$registrations = @{}
foreach ($type in ($assembly.GetTypes() | Where-Object Namespace -eq 'BgiBridge.Tools' | Sort-Object FullName)) {
    $register = $type.GetMethods() | Where-Object {
        $_.Name -eq 'Register' -and $_.IsStatic -and $_.GetParameters().Length -eq 1 -and
        $_.GetParameters()[0].ParameterType -eq $registryType
    }
    if (-not $register) { continue }
    # Each registration root gets its own registry. Delegated registrations may recur;
    # identical descriptors are deduplicated, conflicting contracts fail the export.
    $registry = [Activator]::CreateInstance($registryType)
    $register.Invoke($null, [object[]]@($registry)) | Out-Null
    foreach ($descriptor in $registry.All) {
        $guide = $descriptor.Guide
        $entry = [ordered]@{
            group = $descriptor.Group
            summary = $descriptor.Summary
            readOnly = $descriptor.ReadOnly
            destructive = $descriptor.Destructive
            effect = $descriptor.Effect
            inputSchema = $descriptor.InputSchema.GetRawText() | ConvertFrom-Json
            guide = [ordered]@{
                title = $guide.Title; purpose = $guide.Purpose
                whenToUse = $guide.WhenToUse; preconditions = $guide.Preconditions
                sideEffects = $guide.SideEffects; resultMeaning = $guide.ResultMeaning
                verification = $guide.Verification; rollback = $guide.Rollback
                documentationSource = $guide.DocumentationSource
            }
        }
        $serialized = $entry | ConvertTo-Json -Depth 30 -Compress
        if ($registrations.ContainsKey($descriptor.Id)) {
            if ($registrations[$descriptor.Id].Serialized -ne $serialized) {
                throw "Conflicting registration: $($descriptor.Id)"
            }
            continue
        }
        $registrations[$descriptor.Id] = @{ Entry = $entry; Serialized = $serialized; Type = $type }
    }
}
$out = [ordered]@{}
foreach ($id in ($registrations.Keys | Sort-Object)) {
    $record = $registrations[$id]
    $entry = $record.Entry
    $entry.source = [ordered]@{path = "bgi-bridge/managed/Tools/$($record.Type.Name).cs"; registrationType = $record.Type.FullName}
    $out[$id] = $entry
}
$destination = Join-Path $PSScriptRoot 'bgi-method-metadata.json'
$out | ConvertTo-Json -Depth 30 | Set-Content -LiteralPath $destination -Encoding utf8
Write-Host "Exported $($out.Count) registered contracts -> $destination"
