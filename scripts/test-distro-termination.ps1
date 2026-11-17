# Terminate only an explicitly identified, Core-owned r11 test distribution.
# Never run --shutdown: the caller may be hosted in another WSL distribution.
param([Parameter(Mandatory=$true)][string]$Receipt,
      [Parameter(Mandatory=$true)][string]$Evidence,
      [Parameter(Mandatory=$true)][string]$NativeSha256)
$ErrorActionPreference='Stop'
if(Test-Path -LiteralPath $Evidence){throw 'Evidence directory must be new'}
New-Item -ItemType Directory -Path $Evidence | Out-Null
$record=Get-Content -LiteralPath $Receipt -Raw | ConvertFrom-Json
$dir=Split-Path $Receipt -Parent
$profile=Get-Content -LiteralPath (Join-Path $dir 'wsx-runtime.json') -Raw | ConvertFrom-Json
$binding=Get-Content -LiteralPath (Join-Path $dir 'registration.json') -Raw | ConvertFrom-Json
$distro=$profile.distribution
if($distro -cne 'wsx-omarchy-4.0.6-r11'){throw 'This acceptance is restricted to the owned r11 test distro'}
$registrations=@(Get-ChildItem 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Lxss' | ForEach-Object {Get-ItemProperty $_.PSPath})
$registration=@($registrations | Where-Object {$_.DistributionName -ceq $distro})
if($registration.Count -ne 1){throw 'Expected one exact test registration'}
if(([guid]$registration[0].PSChildName).ToString('D') -ne $binding.id -or $registration[0].BasePath -ne $binding.disk -or $binding.name -cne $distro){throw 'Test registration identity differs from Native binding'}
$owner=Get-Process -Id $record.pid
$created=$owner.StartTime.ToUniversalTime().ToFileTimeUtc().ToString()
if($created -ne $record.creation_filetime -or $owner.Path -ne $record.executable){throw 'Native receipt identity mismatch'}
if((Get-FileHash -LiteralPath $owner.Path -Algorithm SHA256).Hash -ne $NativeSha256){throw 'Native binary differs from expected test build'}
$handle=$owner.Handle
$children=@(Get-CimInstance Win32_Process | Where-Object {$_.ParentProcessId -eq $owner.Id -and $_.Name -eq 'wsx-input-host.exe'} | ForEach-Object {Get-Process -Id $_.ProcessId})
if($children.Count -ne 1){throw 'Expected one input companion'}
$inputOwner=$children[0];$inputHandle=$inputOwner.Handle
$helpers=@(Get-CimInstance Win32_Process | Where-Object {$_.ParentProcessId -eq $owner.Id -and $_.Name -eq 'wsl.exe' -and $_.CommandLine -match '--system'} | ForEach-Object {Get-Process -Id $_.ProcessId})
if($helpers.Count -ne 1){throw 'Expected the already running system retirement helper'}
$systemHelper=$helpers[0];$systemHandle=$systemHelper.Handle
$guestHelpers=@(Get-CimInstance Win32_Process | Where-Object {$_.ParentProcessId -eq $owner.Id -and $_.Name -eq 'wsl.exe' -and $_.CommandLine -match 'WSX_GUEST_READY'} | ForEach-Object {Get-Process -Id $_.ProcessId})
if($guestHelpers.Count -ne 1){throw 'Expected the pre-bound Guest retirement helper'}
$guestHelper=$guestHelpers[0];$guestHandle=$guestHelper.Handle
$wsl=Join-Path $env:SystemRoot 'System32\wsl.exe'
function Running {
 $names=@(& $wsl --list --running --quiet | ForEach-Object {($_ -replace "`0",'').Trim()} | Where-Object {$_})
 if($LASTEXITCODE -ne 0){throw 'WSL running-list observation failed'}
 return $names
}
$before=@(Running)
if($before -cnotcontains $distro){throw 'Test distro must already be running'}
$before | ConvertTo-Json | Set-Content -Encoding UTF8 (Join-Path $Evidence 'running-before.json')
$registrations | Select-Object DistributionName,PSChildName,BasePath,Version | ConvertTo-Json |
 Set-Content -Encoding UTF8 (Join-Path $Evidence 'registrations-before.json')
& (Join-Path $PSScriptRoot 'capture-desktop-candidate.ps1') -Receipt $Receipt -Output (Join-Path $Evidence 'desktop.png') |
 Set-Content -Encoding UTF8 (Join-Path $Evidence 'capture.json')
Set-Location $env:SystemRoot
$process=Start-Process -FilePath $wsl -ArgumentList @('--terminate',$distro) -PassThru -WindowStyle Hidden -RedirectStandardOutput (Join-Path $Evidence 'terminate.stdout') -RedirectStandardError (Join-Path $Evidence 'terminate.stderr')
$commandHandle=$process.Handle
$commandExited=$process.WaitForExit(60000)
$exitCode=if($commandExited){$process.ExitCode}else{$null}
$exitCode | Set-Content (Join-Path $Evidence 'terminate.exitcode')
$deadline=[DateTime]::UtcNow.AddSeconds(90)
while(!$owner.HasExited -and [DateTime]::UtcNow -lt $deadline){Start-Sleep -Milliseconds 500}
$after=@(Running)
$after | ConvertTo-Json | Set-Content -Encoding UTF8 (Join-Path $Evidence 'running-after.json')
$afterRegistrations=@(Get-ChildItem 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Lxss' | ForEach-Object {Get-ItemProperty $_.PSPath})
$afterRegistrations | Select-Object DistributionName,PSChildName,BasePath,Version | ConvertTo-Json |
 Set-Content -Encoding UTF8 (Join-Path $Evidence 'registrations-after.json')
$text=Get-Content -LiteralPath $record.log -Raw
Copy-Item -LiteralPath $record.log -Destination (Join-Path $Evidence 'native.log')
Copy-Item -LiteralPath $Receipt -Destination (Join-Path $Evidence 'native.json')
$instance=([guid]$record.instance).ToString('D')
$result=@{terminate_exit_code=$exitCode;terminate_timed_out=(!$commandExited);native_exited=$owner.HasExited;input_exited=$inputOwner.HasExited;
 distro_remains_stopped=($after -cnotcontains $distro);other_running_distros_preserved=@($before | Where-Object {$_ -cne $distro -and $after -cnotcontains $_}).Count -eq 0;
 registration_identity_preserved=((Get-Content (Join-Path $Evidence 'registrations-before.json') -Raw) -eq (Get-Content (Join-Path $Evidence 'registrations-after.json') -Raw));
 external_stop_observed=$text.Contains("WSX_DISTRO_STOPPED instance=$instance ");
 guest_retired=[bool]($text -match ('(?m)^WSX_GUEST_RETIRED instance='+[regex]::Escape($instance)+'[^\r\n]*source=system-distro'));
 zero_retained_memory=[bool]($text -match 'event=close_complete[^\r\n]*retained_slots=0 usable_pages=0 usable_bytes=0');
 native_closed=$text.Contains('WSX_NATIVE: memory, native device and bootstrap closed');
 lifetime_helpers_exited=$text.Contains('WSX_NATIVE: lifetime helpers exited without forced termination') -and $systemHelper.HasExited -and $guestHelper.HasExited;
 quarantined=$text.Contains('quarantined') -or $text.Contains('retaining native state and the original COM thread after failure');
 shutdown_executed=$false;forced_host_termination=$false}
if($owner.HasExited){$result.native_exit_code=$owner.ExitCode}
if($inputOwner.HasExited){$result.input_exit_code=$inputOwner.ExitCode}
$result.passed=$exitCode -eq 0 -and $result.native_exited -and $result.input_exited -and $result.distro_remains_stopped -and $result.other_running_distros_preserved -and $result.registration_identity_preserved -and $result.external_stop_observed -and $result.guest_retired -and $result.zero_retained_memory -and $result.native_closed -and $result.lifetime_helpers_exited -and !$result.quarantined -and $result.native_exit_code -eq 0 -and $result.input_exit_code -eq 0
$result | ConvertTo-Json | Set-Content -Encoding UTF8 (Join-Path $Evidence 'result.json')
$result | ConvertTo-Json
if(!$result.passed){throw 'External termination retirement did not pass; remaining owners and evidence retained'}
