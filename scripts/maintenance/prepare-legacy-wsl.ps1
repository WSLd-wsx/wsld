param([Parameter(Mandatory=$true)][string]$Destination,
      [Parameter(Mandatory=$true)][string]$HistoricalLog,
      [Parameter(Mandatory=$true)][string]$RecoveryComManifest,
      [Parameter(Mandatory=$true)][string]$ProductPackage)
$ErrorActionPreference='Stop'
if(Test-Path -LiteralPath $Destination){throw 'Maintenance destination must be new'}
if($Destination.StartsWith('\\')){throw 'Maintenance kit must be Windows-local'}
$service=Get-CimInstance Win32_Service | Where-Object Name -eq WSLService
$owners=@(Get-CimInstance Win32_Process | Where-Object Name -eq wsx-native-host.exe)
if($owners.Count -ne 1 -or $owners[0].ParentProcessId -ne $service.ProcessId){throw 'Expected one service-owned Native Host'}
$match=[regex]::Match($owners[0].CommandLine,'--owner\s+"\{[0-9a-fA-F-]{36}\}"\s+"(\{[0-9a-fA-F-]{36}\})"')
if(!$match.Success){throw 'Expected the service-owner launch contract'}
$currentLog=Join-Path $env:TEMP ('wsx-native-'+$match.Groups[1].Value.ToLowerInvariant()+'.log')
$custom=$service.PathName.Trim('"');$customDir=Split-Path $custom
$stockDir=Join-Path $env:ProgramFiles WSL
$stock=Join-Path $stockDir wslservice.exe
$pluginKey='HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Lxss\Plugins'
$key=Get-Item $pluginKey;$plugin=$key.GetValue('WSXVenusNative')
if(!$plugin -or $custom -ieq $stock){throw 'Expected the custom WSL service and its WSX plugin'}
$com=@(foreach($entry in (Get-Content -Raw $RecoveryComManifest | ConvertFrom-Json)){
 if(!$entry.exists -or $entry.key -notmatch '^HKLM:\\SOFTWARE\\Classes\\CLSID\\\{[0-9A-Fa-f-]{36}\}\\InprocServer32$'){throw 'Unrecognized original COM backup'}
 $name=Split-Path $entry.value -Leaf
 if($name -notin @('wslserviceproxystub.dll','wsldevicehostproxystub.dll','wsldevicehost.dll') -or $entry.value -ine (Join-Path $stockDir $name)){throw 'Original COM backup is outside the official runtime'}
 $item=Get-Item $entry.key;$value=$item.GetValue('')
 if($value -ine (Join-Path $customDir $name)){throw 'Current COM registration differs from the old deployment'}
 @{key=$entry.key;original=$value;stock=$entry.value;kind=$item.GetValueKind('').ToString()}
})
if($com.Count -ne 6){throw 'Expected the six original COM registrations'}
New-Item -ItemType Directory $Destination | Out-Null
$backup=New-Item -ItemType Directory (Join-Path $Destination originals)
$binaries=@();$copies=@()
foreach($official in @($true,$false)){
 $directory=if($official){$stockDir}else{$customDir};$label=if($official){'stock'}else{'custom'}
 foreach($name in @('wslservice.exe','wslserviceproxystub.dll','wsldevicehostproxystub.dll','wsldevicehost.dll')){
  $copies+=@{path=(Join-Path $directory $name);name=($label+'-'+$name);official=$official}
 }
}
foreach($name in @('wsx-service-plugin.dll','wsx-native-host.exe','wsx-input-host.exe','libvirglrenderer-1.dll','libepoxy-0.dll','wsx-runtime.json')){
 $copies+=@{path=(Join-Path (Split-Path $plugin) $name);name=('graphics-'+$name);official=$false}
}
foreach($entry in $copies){
 if($entry.official){$sig=Get-AuthenticodeSignature $entry.path;if($sig.Status -ne 'Valid' -or $sig.SignerCertificate.Subject -notmatch 'O=Microsoft Corporation(?:,|$)'){throw 'Official runtime signature check failed'}}
 $hash=(Get-FileHash $entry.path -Algorithm SHA256).Hash;$copy=Join-Path $backup $entry.name
 Copy-Item -LiteralPath $entry.path -Destination $copy
 if((Get-FileHash $copy -Algorithm SHA256).Hash -cne $hash){throw 'Backup hash mismatch'}
 $binaries+=@{path=$entry.path;sha256=$hash;backup=$copy;official=$entry.official}
}
$other=@(foreach($n in $key.GetValueNames() | Where-Object {$_ -ne 'WSXVenusNative'} | Sort-Object){@{name=$n;kind=$key.GetValueKind($n).ToString();value=$key.GetValue($n)}})
$startMode=switch($service.StartMode){Auto {'Automatic'} Manual {'Manual'} default {throw 'Unexpected original start policy'}}
$record=@{schema_version=2;created_utc=[DateTime]::UtcNow.ToString('o');boot_utc=(Get-CimInstance Win32_OperatingSystem).LastBootUpTime.ToUniversalTime().ToString('o');
 custom_image=$service.PathName;stock_image=('"'+$stock+'"');custom_directory=$customDir;stock_directory=$stockDir;plugin=$plugin;
 com=$com;binaries=$binaries;other_plugins_json=(ConvertTo-Json -InputObject $other -Compress -Depth 8);
 original_service=@{state=$service.State;pid=$service.ProcessId;account=$service.StartName;start_mode=$service.StartMode;change_start_mode=$startMode};
 scope='Preparation only. Recovery requires separately authorized maintenance; not normal GPU retirement'}
$record | ConvertTo-Json -Depth 12 | Set-Content -Encoding UTF8 "$Destination\state.json"
Copy-Item "$PSScriptRoot\legacy-wsl.ps1" $Destination
Copy-Item $RecoveryComManifest "$backup\original-stock-com-paths.json"
$exports=@('HKLM\SYSTEM\CurrentControlSet\Services\WSLService','HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Lxss\Plugins')
$exports+=@($com | ForEach-Object {(Split-Path $_.key -Parent).Replace('HKLM:','HKLM')})
# Preserve only interface registrations attached to these two WSL proxy classes.
$proxyClasses=@($com | Where-Object {($_.original -like '*proxystub.dll')} | ForEach-Object {(Split-Path (Split-Path $_.key -Parent) -Leaf)})
$interfaces=Get-Item 'HKLM:\SOFTWARE\Classes\Interface'
foreach($id in $interfaces.GetSubKeyNames()){
 $child=$interfaces.OpenSubKey($id+'\ProxyStubClsid32')
 if($child){try{if($child.GetValue('') -in $proxyClasses){$exports+='HKLM\SOFTWARE\Classes\Interface\'+$id}}finally{$child.Dispose()}}
}
for($i=0;$i -lt $exports.Count;$i++){
 & "$env:SystemRoot\System32\reg.exe" export $exports[$i] "$backup\registry-$i.reg" /y | Out-Null
 if($LASTEXITCODE -ne 0){throw 'Registry export failed'}
}
$exports | ConvertTo-Json | Set-Content -Encoding UTF8 "$backup\registry-index.json"
Get-CimInstance Win32_Process | Where-Object {$_.Name -eq 'wsx-native-host.exe' -or $_.Name -eq 'wsx-input-host.exe'} |
 Select-Object Name,ProcessId,ParentProcessId,CreationDate,ExecutablePath,CommandLine | ConvertTo-Json -Depth 4 | Set-Content -Encoding UTF8 "$backup\hosts.json"
Copy-Item $currentLog "$backup\current-native.log"
Copy-Item $HistoricalLog "$backup\previous-retirement-failure.log"
$product=New-Item -ItemType Directory "$Destination\product"
foreach($name in @('wsld.exe','wsx.exe','wsxdesk.exe')){Copy-Item (Join-Path $ProductPackage $name) $product}
$hashes=@(Get-FileHash "$product\*.exe" -Algorithm SHA256)
if(@($hashes.Hash | Select-Object -Unique).Count -ne 1){throw 'Public PE copies differ'}
$hashes | ConvertTo-Json | Set-Content -Encoding UTF8 "$Destination\product-hashes.json"
Get-ChildItem $Destination -File -Recurse | Get-FileHash -Algorithm SHA256 | ConvertTo-Json -Depth 4 | Set-Content -Encoding UTF8 "$Destination\backup-hashes.json"
Write-Output $Destination
