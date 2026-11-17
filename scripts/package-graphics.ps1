param([Parameter(Mandatory=$true)][string]$Package,
      [Parameter(Mandatory=$true)][string]$Reference,
      [string[]]$AdditionalReferences = @())
$ErrorActionPreference = 'Stop'
if (-not (Test-Path -LiteralPath (Join-Path $Package 'wsld.exe') -PathType Leaf)) { throw 'Product package must contain wsld.exe' }
$receipt = (Get-FileHash -LiteralPath (Join-Path $Reference 'verified-files.json') -Algorithm SHA256).Hash.ToLowerInvariant()
$parent = Join-Path $Package 'graphics'
New-Item -ItemType Directory -Force -Path $parent | Out-Null
& (Join-Path $PSScriptRoot 'stage-reference-graphics.ps1') -Reference $Reference -Destination (Join-Path $parent ("reference-" + $receipt.Substring(0,12)))
foreach ($candidate in $AdditionalReferences) {
    $candidateReceipt = (Get-FileHash -LiteralPath (Join-Path $candidate 'verified-files.json') -Algorithm SHA256).Hash.ToLowerInvariant()
    & (Join-Path $PSScriptRoot 'stage-reference-graphics.ps1') -Reference $candidate -Destination (Join-Path $parent ("reference-" + $candidateReceipt.Substring(0,12)))
}
