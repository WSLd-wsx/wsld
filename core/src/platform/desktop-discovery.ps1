# Read OS identity and live process state. No instance index or active pointer.
$ErrorActionPreference='Stop'
[Console]::OutputEncoding=New-Object System.Text.UTF8Encoding($false)
$request=[Console]::In.ReadToEnd() | ConvertFrom-Json
$entries=@(Get-ChildItem 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Lxss' | ForEach-Object {Get-ItemProperty $_.PSPath} | Where-Object {$_.DistributionName -eq $request.distribution})
if($entries.Count -ne 1){throw 'Expected exactly one registered WSL distribution'}
$entry=$entries[0]
$registration=@{name=$entry.DistributionName;registration_guid=([guid]$entry.PSChildName).ToString('D');disk=$entry.BasePath;version=$entry.Version}
function Observe-Native($item) {
 $record=@{pid=[int]$item.ProcessId;identity_valid=$false}
 try {
  $process=Get-Process -Id $item.ProcessId -ErrorAction Stop
  if(!$process.Path -or !$item.CommandLine){throw 'Native identity is inaccessible'}
  # Only accept the exact four-argument Native form. Windows file paths cannot
  # contain quotes, and any unusual quoting or additional arguments fail closed.
  $pattern='^(?:"[^"\r\n]+"|[^\s"]+)\s+--direct\s+(\{?[0-9a-fA-F-]{36}\}?)\s+(\{?[0-9a-fA-F-]{36}\}?)\s*$'
  if($item.CommandLine -cnotmatch $pattern){throw 'Unsupported Native Host entry'}
  $vm=([guid]$Matches[1]).ToString('D');$instance=([guid]$Matches[2]).ToString('D')
  if([guid]$vm -eq [guid]::Empty -or [guid]$instance -eq [guid]::Empty){throw 'Empty native identity'}
  $record.executable=$process.Path
  $record.creation_filetime=$process.StartTime.ToUniversalTime().ToFileTimeUtc().ToString()
  $record.vm=$vm;$record.instance=$instance
  $record.window_present=($process.MainWindowHandle -ne 0)
  $record.stop_event='unavailable'
  try {
   $event=[Threading.EventWaitHandle]::OpenExisting("Local\WSXDirectStop-$(([guid]$instance).ToString('B').ToUpperInvariant())",[Security.AccessControl.EventWaitHandleRights]::Synchronize)
   try {$record.stop_event=if($event.WaitOne(0)){'requested'}else{'ready'}}finally{$event.Dispose()}
  } catch {$record.stop_event='unavailable'}
  $record.identity_valid=$true
 } catch {$record.error=$_.Exception.Message}
 return $record
}
$processes=@(Get-CimInstance Win32_Process -Filter "Name = 'wsx-native-host.exe'")
if($processes.Count -gt 128){throw 'Native process inventory exceeds limit'}
$native=@($processes | ForEach-Object {Observe-Native $_})
if($request.action -eq 'stop'){
 if($registration.registration_guid -cne $request.registration_guid){throw 'WSL registration changed; no stop requested'}
 $matches=@($native | Where-Object {$_.pid -eq $request.native.pid})
 if($matches.Count -ne 1){throw 'Native process changed; no stop requested'}
 $current=$matches[0]
 foreach($key in @('creation_filetime','executable','instance','vm')){
  if(!$current.identity_valid -or $current[$key] -cne $request.native.$key){throw 'Native process identity changed; no stop requested'}
 }
 $target=$null
 try {
  if($request.wait_for_exit -eq $true){
   $target=Get-Process -Id $current.pid -ErrorAction Stop
   $null=$target.Handle
   if($target.StartTime.ToUniversalTime().ToFileTimeUtc().ToString() -cne $current.creation_filetime -or $target.HasExited){
    throw 'Native process identity changed; no stop requested'
   }
  }
  $event=[Threading.EventWaitHandle]::OpenExisting("Local\WSXDirectStop-$(([guid]$current.instance).ToString('B').ToUpperInvariant())",[Security.AccessControl.EventWaitHandleRights]::Modify)
  try{if(!$event.Set()){throw 'Stop event signalling failed'}}finally{$event.Dispose()}
  if($null -ne $target -and !$target.WaitForExit(90000)){throw 'Native retirement timed out'}
  @{stop_requested=$true;native_exited=($null -ne $target);native=$current;registration=$registration} | ConvertTo-Json -Depth 8 -Compress
 } finally {if($null -ne $target){$target.Dispose()}}
 exit
}
if($request.action -ne 'discover'){throw 'Unsupported desktop observation action'}
$running=@(& "$env:SystemRoot\System32\wsl.exe" --list --running --quiet)
if($LASTEXITCODE -ne 0){throw 'WSL running-distribution query failed'}
$names=@($running | ForEach-Object {($_ -replace "`0",'').Trim().Trim([char]0xfeff)} | Where-Object {$_})
$registration.running=($names -contains $entry.DistributionName)
$vms=@();$vmError=$null
if($request.skip_vm_query -eq $true){
 $vmError='VM observation omitted for launch; startup performs its own admission'
}else{try{
 $listing=(& "$env:SystemRoot\System32\hcsdiag.exe" list | Out-String)
 if($LASTEXITCODE -ne 0){throw 'HCS VM query failed'}
 $vms=@([regex]::Matches($listing,'(?im)^\s*([0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12})\s*\r?\n\s*VM,.*\bWSL\b') | ForEach-Object {([guid]$_.Groups[1].Value).ToString('D')})
}catch{$vmError=$_.Exception.Message}}
@{registration=$registration;native_processes=$native;wsl_vms=$vms;vm_query_error=$vmError} | ConvertTo-Json -Depth 8 -Compress
