param([Parameter(Mandatory=$true)][string]$Directory)
$ErrorActionPreference = 'Stop'
$item = Get-Item -LiteralPath $Directory -Force
if (-not $item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'Unsafe kernel modules directory' }
$index = Get-Item -LiteralPath (Join-Path $Directory 'index.json') -Force
if ($index.PSIsContainer -or ($index.Attributes -band [IO.FileAttributes]::ReparsePoint) -or $index.Length -gt 1048576) { throw 'Invalid kernel modules index' }
$data = Get-Content -LiteralPath $index.FullName -Raw | ConvertFrom-Json
if ($data.schema_version -ne 1 -or @($data.modules).Count -lt 1 -or @($data.modules).Count -gt 1000) { throw 'Invalid kernel modules index' }
$seen = @{}
foreach ($entry in $data.modules) {
    if ($entry.kernel_version.Length -gt 64 -or $entry.kernel_version -cnotmatch '^[0-9]{1,9}(?:\.[0-9]{1,9}){2,3}(?:-(?:(?:rc|RC)[1-9][0-9]{0,8}(?:-[0-9]+)?|[0-9]+))?$' -or $entry.archive -cne "modules_$($entry.kernel_version).tar.gz" -or $entry.sha256 -cnotmatch '^[0-9a-f]{64}$' -or $seen.ContainsKey($entry.kernel_version)) { throw 'Invalid kernel module entry' }
    $seen[$entry.kernel_version] = $true
    $file = Get-Item -LiteralPath (Join-Path $Directory $entry.archive) -Force
    if ($file.PSIsContainer -or ($file.Attributes -band [IO.FileAttributes]::ReparsePoint) -or $file.Length -lt 1 -or $file.Length -gt 1073741824 -or $file.Length -ne $entry.bytes -or (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant() -cne $entry.sha256) { throw "Kernel archive verification failed: $($entry.kernel_version)" }
}
if (@(Get-ChildItem -LiteralPath $Directory -Force).Count -ne ($seen.Count + 1)) { throw 'Unexpected file in kernel modules directory' }
