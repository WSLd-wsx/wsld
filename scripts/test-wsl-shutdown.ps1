# Windows-local collector. Execute only after explicit whole-WSL shutdown consent.
# The arming file is created by the caller after readiness and identity checks.
param([Parameter(Mandatory=$true)][string]$Receipt,
      [Parameter(Mandatory=$true)][string]$Evidence,
      [Parameter(Mandatory=$true)][string]$CoreExe,
      [Parameter(Mandatory=$true)][string]$RunId)
$ErrorActionPreference='Stop'
foreach($path in @($Receipt,$Evidence,$CoreExe,$PSScriptRoot)){
 if($path -notmatch '^[A-Za-z]:\\'){throw 'Shutdown acceptance requires Windows-local paths'}
}
if(Test-Path -LiteralPath $Evidence){throw 'Evidence directory must be new'}
New-Item -ItemType Directory -Path $Evidence | Out-Null
Set-Location $Evidence
Start-Transcript -Path (Join-Path $Evidence 'collector.log') | Out-Null
function Save([string]$Name,$Value){
 ConvertTo-Json -InputObject $Value -Depth 20 | Set-Content -Encoding UTF8 (Join-Path $Evidence $Name)
}
function Registrations {
 @(Get-ChildItem 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Lxss' | ForEach-Object {
  $entry=Get-ItemProperty $_.PSPath
  [ordered]@{name=$entry.DistributionName;id=([guid]$_.PSChildName).ToString('D');disk=$entry.BasePath;version=$entry.Version}
 } | Sort-Object {$_.id})
}
$wsl=Join-Path $env:SystemRoot 'System32\wsl.exe'
function Running {
 $names=@(& $wsl --list --running --quiet | ForEach-Object {($_ -replace "`0",'').Trim()} | Where-Object {$_})
 if($LASTEXITCODE -ne 0){throw 'WSL running-list query failed'}
 return $names
}
function Vms {
 $listing=(& "$env:SystemRoot\System32\hcsdiag.exe" list | Out-String)
 if($LASTEXITCODE -ne 0){throw 'HCS VM query failed'}
 @{raw=$listing;ids=@([regex]::Matches($listing,'(?im)^\s*([0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12})\s*\r?\n\s*VM,.*\bWSL\b') | ForEach-Object {$_.Groups[1].Value.ToLowerInvariant()})}
}
try {
 $record=Get-Content -LiteralPath $Receipt -Raw | ConvertFrom-Json
 $lifetime=Split-Path $Receipt -Parent
 $binding=Get-Content -LiteralPath (Join-Path $lifetime 'registration.json') -Raw | ConvertFrom-Json
 $profile=Get-Content -LiteralPath (Join-Path $lifetime 'wsx-runtime.json') -Raw | ConvertFrom-Json
 if($profile.distribution -cne 'wsx-omarchy-4.0.6-r11' -or $binding.name -cne $profile.distribution){throw 'Expected the exact owned r11 test lifetime'}
 $before=Registrations
 $registration=@($before | Where-Object {$_.name -ceq $binding.name})
 if($registration.Count -ne 1 -or $registration[0].id -ne $binding.id -or $registration[0].disk -ne $binding.disk){throw 'WSL registration binding differs'}
 $owner=Get-Process -Id $record.pid
 $created=$owner.StartTime.ToUniversalTime().ToFileTimeUtc().ToString()
 if($created -ne $record.creation_filetime -or $owner.Path -ne $record.executable){throw 'Native receipt identity differs'}
 if((Get-FileHash -LiteralPath $owner.Path -Algorithm SHA256).Hash -ne '4748e4769e6ce6eafb5d9774ecadfa2bcaa4ea343d634aee2827ff00ea5d8169'){throw 'Unadmitted Native test build'}
 if((Get-FileHash -LiteralPath $CoreExe -Algorithm SHA256).Hash -ne '4a0dc7a0abd55f2d10afd1bbc7fe3eb87db130227e3cf2b835f923ff097a9ba9'){throw 'Unadmitted public Core build'}
 $ownerHandle=$owner.Handle
 $all=@(Get-CimInstance Win32_Process)
 $graphics=@($all | Where-Object {$_.Name -in @('wsx-native-host.exe','wsx-input-host.exe')})
 $inputs=@($graphics | Where-Object {$_.ParentProcessId -eq $owner.Id -and $_.Name -eq 'wsx-input-host.exe'})
 if($graphics.Count -ne 2 -or $inputs.Count -ne 1){throw 'Unexpected graphics owner; shutdown not executed'}
 $inputOwner=Get-Process -Id $inputs[0].ProcessId;$inputHandle=$inputOwner.Handle
 $helpers=@($all | Where-Object {$_.ParentProcessId -eq $owner.Id -and $_.Name -eq 'wsl.exe' -and $_.CommandLine -match '--system'})
 if($helpers.Count -ne 1){throw 'Expected the pre-existing system retirement helper'}
 $helper=Get-Process -Id $helpers[0].ProcessId;$helperHandle=$helper.Handle
 $guestHelpers=@($all | Where-Object {$_.ParentProcessId -eq $owner.Id -and $_.Name -eq 'wsl.exe' -and $_.CommandLine -match 'WSX_GUEST_READY'})
 if($guestHelpers.Count -ne 1){throw 'Expected the pre-existing Guest retirement helper'}
 $guestHelper=Get-Process -Id $guestHelpers[0].ProcessId;$guestHelperHandle=$guestHelper.Handle
 $running=@(Running)
 if($running -cnotcontains $binding.name -or @($running | Where-Object {$_ -cnotin @('Ubuntu-24.04',$binding.name)}).Count){throw 'Unexpected running distro set; shutdown not executed'}
 $vm=Vms
 $originalVm=([guid]$record.vm).ToString('D')
 if($vm.ids.Count -ne 1 -or $vm.ids -notcontains $originalVm){throw 'Original WSL VM identity differs'}
 Save 'registrations-before.json' $before
 Save 'running-before.json' $running
 Save 'vms-before.json' $vm
 Save 'processes-before.json' @{native=$owner.Id;native_created=$created;input=$inputOwner.Id;helper=$helper.Id;guest_helper=$guestHelper.Id;collector=$PID}
 $archive=New-Item -ItemType Directory (Join-Path $Evidence 'lifetime')
 foreach($name in @('native.json','prepared.json','preflight.json','registration.json','wsx-runtime.json','state.json','previous-lifetime.json','restart-plan.json')){
  Copy-Item -LiteralPath (Join-Path $lifetime $name) -Destination $archive
 }
 & (Join-Path $PSScriptRoot 'capture-desktop-candidate.ps1') -Receipt $Receipt -Output (Join-Path $Evidence 'desktop-before.png') |
  Set-Content -Encoding UTF8 (Join-Path $Evidence 'capture.json')
 Save 'ready.json' @{run_id=$RunId;collector_pid=$PID;native_pid=$owner.Id;instance=$record.instance;original_vm=$originalVm;utc=[DateTime]::UtcNow.ToString('o');shutdown_executed=$false}
 $arm=Join-Path $Evidence 'execute-shutdown.json'
 $deadline=[DateTime]::UtcNow.AddMinutes(10)
 while(!(Test-Path -LiteralPath $arm)){
  if([DateTime]::UtcNow -ge $deadline){throw 'Arming timed out; shutdown not executed'}
  Start-Sleep -Milliseconds 200
 }
 $consent=Get-Content -LiteralPath $arm -Raw | ConvertFrom-Json
 if($consent.run_id -cne $RunId -or $consent.operation -cne 'wsl.exe --shutdown'){throw 'Invalid arming identity; shutdown not executed'}
 if($owner.HasExited -or $inputOwner.HasExited -or $helper.HasExited -or $guestHelper.HasExited){throw 'Test lifetime changed before shutdown'}
 $nowRunning=@(Running)
 if(@(Compare-Object $running $nowRunning).Count){throw 'Running distro set changed before shutdown'}
 $nowGraphics=@(Get-Process -ErrorAction Stop | Where-Object {$_.ProcessName -in @('wsx-native-host','wsx-input-host')})
 if($nowGraphics.Count -ne 2 -or $nowGraphics.Id -notcontains $owner.Id -or $nowGraphics.Id -notcontains $inputOwner.Id){throw 'Graphics owners changed before shutdown'}
 Save 'shutdown-started.json' @{run_id=$RunId;utc=[DateTime]::UtcNow.ToString('o');command='wsl.exe --shutdown';original_vm=$originalVm}
 $command=Start-Process -FilePath $wsl -ArgumentList '--shutdown' -PassThru -WindowStyle Hidden -RedirectStandardOutput (Join-Path $Evidence 'shutdown.stdout') -RedirectStandardError (Join-Path $Evidence 'shutdown.stderr')
 $commandHandle=$command.Handle
 $commandExited=$command.WaitForExit(60000)
 $commandCode=if($commandExited){$command.ExitCode}else{$null}
 Save 'shutdown-command.json' @{pid=$command.Id;exited=$commandExited;exit_code=$commandCode;utc=[DateTime]::UtcNow.ToString('o')}
 # A user may restart Ubuntu to reconnect Codex. Track the original VM identity,
 # not whether some later WSL VM exists or all distros remain stopped forever.
 Save 'vms-immediate.json' (Vms)
 $deadline=[DateTime]::UtcNow.AddSeconds(120)
 do {
  Copy-Item -LiteralPath $record.log -Destination (Join-Path $Evidence 'native.log') -Force
  Save 'progress.json' @{utc=[DateTime]::UtcNow.ToString('o');native_exited=$owner.HasExited;input_exited=$inputOwner.HasExited;helper_exited=$helper.HasExited;guest_helper_exited=$guestHelper.HasExited}
  if($owner.HasExited -and $inputOwner.HasExited -and $helper.HasExited -and $guestHelper.HasExited){break}
  Start-Sleep -Milliseconds 500
 } while([DateTime]::UtcNow -lt $deadline)
 $after=Registrations;Save 'registrations-after.json' $after
 $afterVms=Vms;Save 'vms-after.json' $afterVms
 Save 'running-after.json' @(Running)
 $text=Get-Content -LiteralPath $record.log -Raw
 Copy-Item -LiteralPath $record.log -Destination (Join-Path $Evidence 'native.log') -Force
 $instance=([guid]$record.instance).ToString('D')
 $result=@{run_id=$RunId;shutdown_executed=$true;shutdown_command_exited=$commandExited;shutdown_exit_code=$commandCode;
  original_vm_absent=($afterVms.ids -notcontains $originalVm);native_exited=$owner.HasExited;input_exited=$inputOwner.HasExited;system_helper_exited=$helper.HasExited;guest_helper_exited=$guestHelper.HasExited;
  exact_vm_exit_confirmed=$text.Contains("WSX_VM_EXIT_CONFIRMED instance=$instance source=owned-hcs-handle");
  zero_retained_memory=[bool]($text -match 'event=close_complete[^\r\n]*retained_slots=0 usable_pages=0 usable_bytes=0');
  native_closed=$text.Contains('WSX_NATIVE: memory, native device and bootstrap closed');
  helpers_joined=$text.Contains('WSX_NATIVE: lifetime helpers exited without forced termination');
  quarantined=$text.Contains('quarantined') -or $text.Contains('retaining native state and the original COM thread after failure');
  registrations_preserved=(($before | ConvertTo-Json -Depth 10 -Compress) -ceq ($after | ConvertTo-Json -Depth 10 -Compress));
  forced_host_termination=$false;collector_requested_restart=$false;utc=[DateTime]::UtcNow.ToString('o')}
 if($owner.HasExited){$result.native_exit_code=$owner.ExitCode}
 if($inputOwner.HasExited){$result.input_exit_code=$inputOwner.ExitCode}
 if($helper.HasExited){$result.system_helper_exit_code=$helper.ExitCode}
 if($guestHelper.HasExited){$result.guest_helper_exit_code=$guestHelper.ExitCode}
 $result.passed=$commandExited -and $commandCode -eq 0 -and $result.original_vm_absent -and $result.native_exited -and $result.input_exited -and $result.system_helper_exited -and $result.guest_helper_exited -and $result.exact_vm_exit_confirmed -and $result.zero_retained_memory -and $result.native_closed -and $result.helpers_joined -and !$result.quarantined -and $result.registrations_preserved -and $result.native_exit_code -eq 0 -and $result.input_exit_code -eq 0
 Save 'result.json' $result
 # This legacy readback uses Windows process/HCS/log evidence only; it never
 # executes a Guest command or starts any distro after shutdown.
 $pointer=Get-Content -LiteralPath (Join-Path $lifetime 'previous-lifetime.json') -Raw | ConvertFrom-Json
 $installationId=$pointer.pointer.id
 $core=Start-Process -FilePath $CoreExe -ArgumentList @('candidate','desktop-status',$installationId) -PassThru -WindowStyle Hidden -RedirectStandardOutput (Join-Path $Evidence 'core-status.json') -RedirectStandardError (Join-Path $Evidence 'core-status.stderr')
 $coreHandle=$core.Handle # Retain the process handle before waiting for its exit code.
 if($core.WaitForExit(45000)){Save 'core-status-exit.json' @{exit_code=$core.ExitCode}}else{Save 'core-status-exit.json' @{timed_out=$true}}
 Save 'completed.json' @{run_id=$RunId;passed=$result.passed;utc=[DateTime]::UtcNow.ToString('o')}
} catch {
 $_ | Out-String | Set-Content -Encoding UTF8 (Join-Path $Evidence 'collector-error.txt')
 Save 'collector-failed.json' @{run_id=$RunId;error=$_.Exception.Message;utc=[DateTime]::UtcNow.ToString('o');forced_cleanup=$false}
} finally {Stop-Transcript | Out-Null}
