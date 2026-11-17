param([Parameter(Mandatory=$true)][string]$Package,
      [Parameter(Mandatory=$true)][string]$Reference,
      [Parameter(Mandatory=$true)][string]$IdentityTools,
      [string[]]$AdditionalReferences = @())
$ErrorActionPreference = 'Stop'
if (-not (Test-Path -LiteralPath (Join-Path $Package 'wsld.exe') -PathType Leaf)) { throw 'Product package must contain wsld.exe' }
$receipt = (Get-FileHash -LiteralPath (Join-Path $Reference 'verified-files.json') -Algorithm SHA256).Hash.ToLowerInvariant()
foreach ($entry in @(@('adapter-inventory.exe','4cef7ea4d19a49dd8e6aff486a0b99e9d3e67adede1f0f32c6b4371f9482e6ea'), @('vulkan-identity.exe','7b026cfc76c31b72a0151b308285c3ba194cf0b028758c9100ae9702345ad066'))) {
    if ((Get-FileHash -LiteralPath (Join-Path $IdentityTools $entry[0]) -Algorithm SHA256).Hash -ne $entry[1]) { throw "Identity tool digest mismatch: $($entry[0])" }
}
$parent = Join-Path $Package 'graphics'
New-Item -ItemType Directory -Force -Path $parent | Out-Null
& (Join-Path $PSScriptRoot 'stage-reference-graphics.ps1') -Reference $Reference -IdentityTools $IdentityTools -Destination (Join-Path $parent ("reference-" + $receipt.Substring(0,12)))
foreach ($candidate in $AdditionalReferences) {
    $candidateReceipt = (Get-FileHash -LiteralPath (Join-Path $candidate 'verified-files.json') -Algorithm SHA256).Hash.ToLowerInvariant()
    & (Join-Path $PSScriptRoot 'stage-reference-graphics.ps1') -Reference $candidate -IdentityTools $IdentityTools -Destination (Join-Path $parent ("reference-" + $candidateReceipt.Substring(0,12)))
}
