$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
# Windows PowerShell 5 emits a JSON array as one pipeline object. Keep its
# elements as the target list instead of wrapping that array in a second array.
$removalsJson = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($removalsBase64))
$removals = ConvertFrom-Json -InputObject $removalsJson
if ($ownerProcessId -gt 0) {
    $owner = Get-Process -Id $ownerProcessId -ErrorAction SilentlyContinue
    if ($owner) { $owner.WaitForExit() }
}

function Read-Node([string]$path) {
    try { Get-Item -LiteralPath $path -Force -ErrorAction Stop }
    catch {
        if ($_.Exception -is [Management.Automation.ItemNotFoundException]) { return $null }
        throw
    }
}

function Remove-Node([string]$path, [bool]$recursive) {
    $item = Read-Node $path
    if (-not $item) { return }
    # Never traverse junctions or symbolic links, including a linked root.
    if ($item.PSIsContainer) {
        if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -eq 0 -and $recursive) {
            foreach ($child in @(Get-ChildItem -LiteralPath $path -Force)) {
                Remove-Node $child.FullName $true
            }
        }
        [IO.Directory]::Delete($path, $false)
    } else {
        if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) {
            [IO.File]::Delete($path)
            return
        }
        if ($item.Attributes -band [IO.FileAttributes]::ReadOnly) {
            [IO.File]::SetAttributes($path, ($item.Attributes -band (-bnot [IO.FileAttributes]::ReadOnly)))
        }
        [IO.File]::Delete($path)
    }
}

$deadline = [DateTime]::UtcNow.AddSeconds(60)
do {
    foreach ($removal in $removals) {
        try { Remove-Node $removal.path ($removal.kind -eq 'tree') } catch { }
    }
    # Empty directory removal is part of completion, too.
    $remaining = @($removals | Where-Object {
        try { [bool](Read-Node $_.path) } catch { $true }
    })
    if ($remaining.Count -eq 0) { exit 0 }
    Start-Sleep -Milliseconds 250
} while ([DateTime]::UtcNow -lt $deadline)

Add-Type -AssemblyName System.Windows.Forms
[void][System.Windows.Forms.MessageBox]::Show(
    "卸载清理未完成。请关闭占用文件的程序，再删除以下残留：`n" + (($remaining | ForEach-Object { $_.path }) -join "`n"),
    'Sleepy Doll', [System.Windows.Forms.MessageBoxButtons]::OK, [System.Windows.Forms.MessageBoxIcon]::Warning)
exit 1
