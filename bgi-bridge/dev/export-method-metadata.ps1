# Export pure method metadata (InputSchema + AgentGuide) for the isolated
# skill-plan CLI. Loads the compiled BgiBridge.dll, registers only the tool
# classes the CLI needs, and never invokes any handler, loads BGI, or opens
# windows. Regenerate after editing AgentSchemas.cs or an AgentGuide:
#   dotnet build managed -o target/dotnet/managed
#   pwsh -File bgi-bridge/dev/export-method-metadata.ps1
# Output: bgi-bridge/dev/bgi-method-metadata.json (committed; CLI embeds it).

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$bin = Join-Path $root '..\target\dotnet\managed'
$context = [System.Runtime.Loader.AssemblyLoadContext]::Default
$context.add_Resolving({ param($context, $name)
    $candidate = Join-Path $bin "$($name.Name).dll"
    if (Test-Path $candidate) { return $context.LoadFromAssemblyPath($candidate) }
    return $null })
$assembly = $context.LoadFromAssemblyPath((Join-Path $bin 'BgiBridge.dll'))
$registry = $assembly.CreateInstance('BgiBridge.Catalog.MethodRegistry')
if ($null -eq $registry) { throw 'MethodRegistry instance creation failed' }
foreach ($tools in @(
        'BgiBridge.Tools.PathingPreparationTools',
        'BgiBridge.Tools.ScriptGroupTools',
        'BgiBridge.Tools.ScriptGroupPlanTools')) {
    $type = $assembly.GetType($tools)
    if ($null -eq $type) { throw "type not found: $tools" }
    $type.GetMethod('Register').Invoke($null, @($registry)) | Out-Null
}

$out = [ordered]@{}
foreach ($descriptor in $registry.All | Sort-Object Id) {
    $guide = $descriptor.Guide
    $out[$descriptor.Id] = [ordered]@{
        group       = $descriptor.Group
        summary     = $descriptor.Summary
        readOnly    = $descriptor.ReadOnly
        destructive = $descriptor.Destructive
        effect      = $descriptor.Effect
        inputSchema = $descriptor.InputSchema.GetRawText() | ConvertFrom-Json
        guide       = [ordered]@{
            title               = $guide.Title
            purpose             = $guide.Purpose
            whenToUse           = $guide.WhenToUse
            preconditions       = $guide.Preconditions
            sideEffects         = $guide.SideEffects
            resultMeaning       = $guide.ResultMeaning
            verification        = $guide.Verification
            rollback            = $guide.Rollback
            documentationSource = $guide.DocumentationSource
        }
    }
}
$destination = Join-Path $PSScriptRoot 'bgi-method-metadata.json'
$out | ConvertTo-Json -Depth 12 | Out-File -FilePath $destination -Encoding utf8
Write-Host "exported $($out.Count) methods -> $destination"
