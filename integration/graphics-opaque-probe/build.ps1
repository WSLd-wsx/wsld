param([Parameter(Mandatory=$true)][string]$Toolchain,
      [Parameter(Mandatory=$true)][string]$Output)
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot '..\..\scripts\msvc-env.ps1') -Toolchain $Toolchain
if (Test-Path -LiteralPath $Output) { throw 'Output already exists' }
$receipt=Get-Content -LiteralPath (Join-Path $PSScriptRoot 'source-receipt.json') -Raw | ConvertFrom-Json
foreach ($item in $receipt.files.PSObject.Properties) {
    $file=Join-Path $PSScriptRoot $item.Name
    if ((Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash.ToLowerInvariant() -cne $item.Value) {
        throw "Source receipt mismatch: $($item.Name)"
    }
}
$build=Join-Path ([IO.Path]::GetTempPath()) ('wsx-opaque-probe-'+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $build | Out-Null
$exe=Join-Path $build 'opaque-buffer-probe.exe'
Push-Location $build
try {
    & $env:CC /nologo /std:c11 /W3 /O2 /TC "/I$PSScriptRoot" (Join-Path $PSScriptRoot 'check-buffer.c') "/Fe:$exe" /link advapi32.lib
    if ($LASTEXITCODE -ne 0) { throw 'Opaque probe compilation failed' }
    & $exe --check
    if ($LASTEXITCODE -ne 0) { throw 'Opaque probe invocation check failed' }
    Copy-Item -LiteralPath $exe -Destination $Output
    Get-FileHash -LiteralPath $Output -Algorithm SHA256
} finally { Pop-Location }
