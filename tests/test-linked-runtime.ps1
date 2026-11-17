param([Parameter(Mandatory=$true)][string]$RuntimePackage,
      [Parameter(Mandatory=$true)][string]$RuntimeCatalog,
      [Parameter(Mandatory=$true)][string]$GuestAdapterCatalog,
      [Parameter(Mandatory=$true)][string]$Evidence)
$ErrorActionPreference = 'Stop'
if (Test-Path -LiteralPath $Evidence) { throw 'Evidence destination already exists' }
New-Item -ItemType Directory -Path $Evidence | Out-Null
$verify = Join-Path (Split-Path $PSScriptRoot -Parent) 'scripts\verify-linked-runtime.ps1'
& $verify -RuntimePackage $RuntimePackage -RuntimeCatalog $RuntimeCatalog -GuestAdapterCatalog $GuestAdapterCatalog
$runtime = Get-Content -LiteralPath $RuntimeCatalog -Raw | ConvertFrom-Json
$runtime.bundles[0].manifest_sha256 = '0' * 64
$badRuntime = Join-Path $Evidence 'bad-runtime-catalog.json'
$runtime | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $badRuntime -Encoding UTF8
$rejected = $false
try { & $verify -RuntimePackage $RuntimePackage -RuntimeCatalog $badRuntime -GuestAdapterCatalog $GuestAdapterCatalog }
catch { $rejected = $true }
if (-not $rejected) { throw 'A mismatched Native catalog was accepted' }
$guest = Get-Content -LiteralPath $GuestAdapterCatalog -Raw | ConvertFrom-Json
$guest.adapters[0].manifest_sha256 = '0' * 64
$badGuest = Join-Path $Evidence 'bad-guest-catalog.json'
$guest | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $badGuest -Encoding UTF8
$rejected = $false
try { & $verify -RuntimePackage $RuntimePackage -RuntimeCatalog $RuntimeCatalog -GuestAdapterCatalog $badGuest }
catch { $rejected = $true }
if (-not $rejected) { throw 'A mismatched Guest catalog was accepted' }
Write-Output 'Valid component closure admitted; altered Native and Guest catalogs rejected.'
