# Explicit local candidate staging; inputs are checked again by wsld.exe.
param([Parameter(Mandatory=$true)][string]$Reference,
      [Parameter(Mandatory=$true)][string]$IdentityTools,
      [Parameter(Mandatory=$true)][string]$Destination)
$ErrorActionPreference='Stop'
if(Test-Path -LiteralPath $Destination){throw 'Reference destination must be new'}
$receipt=Join-Path $Reference 'verified-files.json'
$receiptHash=(Get-FileHash -LiteralPath $receipt -Algorithm SHA256).Hash.ToLowerInvariant()
$files=Get-Content -LiteralPath $receipt -Raw | ConvertFrom-Json
New-Item -ItemType Directory -Path $Destination | Out-Null
New-Item -ItemType Directory -Path (Join-Path $Destination 'guest') | Out-Null
foreach($entry in $files.PSObject.Properties){
    if($entry.Name -match '(^/|\.\.|:|\\)'){throw 'Invalid reference relative path'}
    $source=Join-Path $Reference $entry.Name
    if($entry.Name -ceq 'guest/start-guest.py'){
        if($receiptHash -cne '442f6032d8645c413b7398e2654c669169e79e9b46e79220134bb881b51d0561'){throw 'Unreviewed reference Guest launcher cannot be excluded'}
        if((Test-Path -LiteralPath $source) -and (Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash.ToLowerInvariant() -cne $entry.Value.sha256){throw 'Reference digest mismatch'}
        continue
    }
    if((Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash -ne $entry.Value.sha256){throw 'Reference digest mismatch'}
    Copy-Item -LiteralPath $source -Destination (Join-Path $Destination $entry.Name)
}
Copy-Item -LiteralPath $receipt -Destination $Destination
foreach($name in @('adapter-inventory.exe','vulkan-identity.exe')){
    Copy-Item -LiteralPath (Join-Path $IdentityTools $name) -Destination $Destination
}
Write-Output $Destination
