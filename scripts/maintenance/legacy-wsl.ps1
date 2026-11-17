# Windows-local recovery of the historical service deployment. No process kill,
# service stop, WSL shutdown, driver unload, automatic reboot or disk deletion.
param([ValidateSet('Inspect','StageReboot','FinishStock','Restore')][string]$Action='Inspect',
      [string]$State=(Join-Path $PSScriptRoot 'state.json'),[switch]$ConfirmMaintenance)
$ErrorActionPreference='Stop'
if(!([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)){throw 'Use elevated Windows PowerShell'}
$record=Get-Content -LiteralPath $State -Raw | ConvertFrom-Json
if($record.schema_version -ne 2){throw 'Unsupported maintenance schema; use the complete COM-aware kit'}
$pluginKey='HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Lxss\Plugins'
$service=Get-CimInstance Win32_Service | Where-Object Name -eq WSLService
if(!$service){throw 'WSLService is not installed'}
foreach($entry in $record.binaries){
 if((Get-FileHash -LiteralPath $entry.path -Algorithm SHA256).Hash -cne $entry.sha256){throw "Prepared file changed: $($entry.path)"}
 if($entry.official){
  $sig=Get-AuthenticodeSignature -LiteralPath $entry.path
  if($sig.Status -ne 'Valid' -or $sig.SignerCertificate.Subject -notmatch 'O=Microsoft Corporation(?:,|$)'){throw 'Official WSL signature check failed'}
 }
}
$key=Get-Item $pluginKey
$plugin=$key.GetValue('WSXVenusNative')
$others=@(foreach($n in $key.GetValueNames() | Where-Object {$_ -ne 'WSXVenusNative'} | Sort-Object){@{name=$n;kind=$key.GetValueKind($n).ToString();value=$key.GetValue($n)}})
if((ConvertTo-Json -InputObject $others -Compress -Depth 8) -cne $record.other_plugins_json){throw 'Other plugin registrations changed'}
if($null -ne $plugin -and $plugin -cne $record.plugin){throw 'WSX plugin registration changed'}
if($service.PathName -cne $record.custom_image -and $service.PathName -cne $record.stock_image){throw 'Unexpected WSL service path'}
$boot=(Get-CimInstance Win32_OperatingSystem).LastBootUpTime.ToUniversalTime().ToString('o')
$hosts=@(Get-CimInstance Win32_Process | Where-Object {$_.Name -eq 'wsx-native-host.exe' -or $_.Name -eq 'wsx-input-host.exe'} | Select-Object Name,ProcessId,CreationDate,ExecutablePath,CommandLine)
$process=if($service.ProcessId){Get-CimInstance Win32_Process | Where-Object ProcessId -eq $service.ProcessId}else{$null}
$view=[ordered]@{utc=[DateTime]::UtcNow.ToString('o');action=$Action;configured_service=$service.PathName;state=$service.State;start_mode=$service.StartMode;
 service_pid=$service.ProcessId;running_service_path=$process.ExecutablePath;plugin=$plugin;boot_utc=$boot;hosts=$hosts;effect='Read-only observation'}
if($Action -eq 'Inspect'){$view | ConvertTo-Json -Depth 8;exit}
if(!$ConfirmMaintenance){throw 'Explicit maintenance authorization is required; no changes made'}
function Checked-Change($Result,[string]$Operation){if($Result.ReturnValue -ne 0){throw "$Operation failed ($($Result.ReturnValue)); state retained, no forced cleanup"}}
$receipt=Join-Path $PSScriptRoot ($Action+'-'+[DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffffffZ'))
New-Item -ItemType Directory $receipt | Out-Null
$view | ConvertTo-Json -Depth 8 | Set-Content -Encoding UTF8 "$receipt\before.json"
if($Action -eq 'StageReboot'){
 if($boot -cne $record.boot_utc -or $service.State -ne 'Running' -or $service.ProcessId -ne $record.original_service.pid -or $service.PathName -cne $record.custom_image){throw 'Prepared service lifetime changed; inspect again'}
 if($service.StartMode -ne $record.original_service.start_mode -or $plugin -cne $record.plugin){throw 'Prepared configuration changed'}
 # Only the next-start policy changes. Current COM registration, service, VM,
 # plugin module and all device/Host lifetimes remain untouched.
 Checked-Change (Invoke-CimMethod -InputObject $service -MethodName ChangeStartMode -Arguments @{StartMode='Disabled'}) 'Disable next automatic service start'
 $after=Get-CimInstance Win32_Service | Where-Object Name -eq WSLService
 if($after.StartMode -ne 'Disabled' -or $after.ProcessId -ne $service.ProcessId -or $after.State -ne $service.State){throw 'Staging state differs; inspect retained receipt'}
 $after | Select-Object Name,PathName,ProcessId,State,StartMode | ConvertTo-Json | Set-Content -Encoding UTF8 "$receipt\after.json"
 Write-Output 'Next automatic WSL start disabled only. Save all work, then perform the authorized normal Windows restart manually. Run FinishStock from this Windows-local directory afterward.'
 exit
}
if($Action -eq 'Restore' -and $boot -ceq $record.boot_utc -and $service.ProcessId -eq $record.original_service.pid -and $service.PathName -ceq $record.custom_image){
 if($plugin -cne $record.plugin){throw 'Original plugin changed; do not infer rollback'}
 foreach($com in $record.com){if((Get-Item $com.key).GetValue('') -cne $com.original){throw 'COM registration changed'}}
 Checked-Change (Invoke-CimMethod -InputObject $service -MethodName ChangeStartMode -Arguments @{StartMode=$record.original_service.change_start_mode}) 'Cancel staged restart'
 Write-Output 'Original start policy restored; running service and VM were not changed.'
 exit
}
# COM mutation and service selection are permitted only after a different OS boot,
# with WSLService still disabled/stopped and no old or new graphics Owner present.
if($boot -ceq $record.boot_utc -or $service.State -ne 'Stopped' -or $service.StartMode -ne 'Disabled' -or $hosts.Count){throw 'Requires a new Windows boot, disabled/stopped WSLService and no graphics Host; nothing was stopped automatically'}
foreach($com in $record.com){
 $value=(Get-Item $com.key).GetValue('')
 if($value -cne $com.original -and $value -cne $com.stock){throw 'COM registration changed outside this maintenance transaction'}
}
$stock=$Action -eq 'FinishStock'
$runtime=if($stock){$record.stock_directory}else{$record.custom_directory}
# These are the exact proxy DLL registrations changed by the original deployment.
# Their prepared bytes are rehashed above; official binaries also require signature.
foreach($name in @('wslserviceproxystub.dll','wsldevicehostproxystub.dll')){
 $dll=Join-Path $runtime $name
 $registration=Start-Process "$env:SystemRoot\System32\regsvr32.exe" -ArgumentList ('/s "'+$dll+'"') -Wait -PassThru
 if($registration.ExitCode -ne 0){throw "Proxy registration failed: $name; service remains disabled"}
}
foreach($com in $record.com){
 $value=if($stock){$com.stock}else{$com.original}
 (Get-Item $com.key).SetValue('',$value,([Microsoft.Win32.RegistryValueKind]$com.kind))
}
if($stock){
 if($null -ne $plugin){Remove-ItemProperty -LiteralPath $pluginKey -Name WSXVenusNative}
}else{New-ItemProperty -LiteralPath $pluginKey -Name WSXVenusNative -Value $record.plugin -PropertyType String -Force | Out-Null}
$target=if($stock){$record.stock_image}else{$record.custom_image}
Checked-Change (Invoke-CimMethod -InputObject $service -MethodName Change -Arguments @{PathName=$target}) 'Select service executable'
foreach($com in $record.com){$expected=if($stock){$com.stock}else{$com.original};if((Get-Item $com.key).GetValue('') -cne $expected){throw 'COM path verification failed; service remains disabled'}}
Checked-Change (Invoke-CimMethod -InputObject $service -MethodName ChangeStartMode -Arguments @{StartMode=$record.original_service.change_start_mode}) 'Restore service start policy'
if($stock){
 Start-Service WSLService
 (Get-Service WSLService).WaitForStatus('Running',[TimeSpan]::FromSeconds(30))
 $service=Get-CimInstance Win32_Service | Where-Object Name -eq WSLService
 $process=Get-CimInstance Win32_Process | Where-Object ProcessId -eq $service.ProcessId
 if($process.ExecutablePath -ine (Join-Path $record.stock_directory 'wslservice.exe')){throw 'Running service is not the official executable'}
}
Get-CimInstance Win32_Service | Where-Object Name -eq WSLService | Select-Object Name,PathName,ProcessId,State,StartMode | ConvertTo-Json | Set-Content -Encoding UTF8 "$receipt\after.json"
Write-Output 'Prepared service and COM configuration restored. This was maintenance recovery, not proof of normal GPU retirement. No distro or desktop was launched.'
