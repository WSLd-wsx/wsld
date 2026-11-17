param([Parameter(Mandatory=$true)][string]$Package,
      [Parameter(Mandatory=$true)][string]$Evidence,
      [Parameter(Mandatory=$true)][string]$Toolchain,
      [Parameter(Mandatory=$true)][string]$RunningDistro,
      [Parameter(Mandatory=$true)][string]$StoppedDistro)
$ErrorActionPreference='Stop'
if(Test-Path $Evidence){throw 'Evidence directory must be new'}
New-Item -ItemType Directory $Evidence | Out-Null
. "$PSScriptRoot\msvc-env.ps1" -Toolchain $Toolchain
$exe=Join-Path $Package 'wsld.exe'
$cache=Join-Path $BuildCache ('distro-acceptance-'+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory $cache | Out-Null
$originalLocal=$env:LOCALAPPDATA
$cases=New-Object System.Collections.Generic.List[object]
function Running-Names {
 $names=@(& "$env:SystemRoot\System32\wsl.exe" --list --running --quiet)
 if($LASTEXITCODE -ne 0){throw 'Cannot inspect running distros'}
 return @($names | ForEach-Object {($_ -replace "`0",'').Trim().Trim([char]0xfeff)} | Where-Object {$_} | Sort-Object)
}
function Case([string]$Label,[string[]]$Arguments,[int]$Expected) {
 $out=Join-Path $Evidence "$Label.stdout";$err=Join-Path $Evidence "$Label.stderr"
 $previous=$ErrorActionPreference;$ErrorActionPreference='Continue'
 & $exe @Arguments 1> $out 2> $err
 $code=$LASTEXITCODE;$ErrorActionPreference=$previous
 $cases.Add(@{name=$Label;exit_code=$code;expected=$Expected})
 if($code -ne $Expected){throw "Unexpected exit code in $Label; inspect $err"}
 if($Expected -eq 0){return Get-Content $out -Raw | ConvertFrom-Json}
}
function Adapter([string]$Label,$Request,[int]$Expected) {
 $out=Join-Path $Evidence "$Label.stdout";$err=Join-Path $Evidence "$Label.stderr"
 $previous=$ErrorActionPreference;$ErrorActionPreference='Continue'
 $Request | ConvertTo-Json -Depth 8 -Compress | & "$env:SystemRoot\System32\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$RepoRoot\core\src\platform\desktop-discovery.ps1" 1> $out 2> $err
 $code=$LASTEXITCODE;$ErrorActionPreference=$previous
 $cases.Add(@{name=$Label;exit_code=$code;expected=$Expected})
 if($code -ne $Expected){throw "Unexpected adapter exit in $Label"}
 if($Expected -eq 0){return Get-Content $out -Raw | ConvertFrom-Json}
}
Start-Transcript -Path (Join-Path $Evidence 'acceptance.log') | Out-Null
try {
 & "$env:SystemRoot\System32\wsl.exe" -l -v 1> (Join-Path $Evidence 'wsl-list-verbose.stdout') 2> (Join-Path $Evidence 'wsl-list-verbose.stderr')
 if($LASTEXITCODE -ne 0){throw 'Cannot read WSL distro names'}
 $nativeBefore=@(Get-CimInstance Win32_Process -Filter "Name = 'wsx-native-host.exe'" | Select-Object ProcessId,CreationDate,ExecutablePath | Sort-Object ProcessId)
 $nativeBefore | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $Evidence 'existing-native-before.json')
 $before=@(Running-Names)
 if($before -notcontains $RunningDistro -or $before -contains $StoppedDistro){throw 'Expected one already-running and one already-stopped selected distro'}
 # A private, corrupt index proves observation does not depend on old Core state.
 $env:LOCALAPPDATA=Join-Path $cache 'local'
 $privateRoot=Join-Path $env:LOCALAPPDATA 'WSX'
 New-Item -ItemType Directory (Join-Path $privateRoot 'desktop') -Force | Out-Null
 Set-Content (Join-Path $privateRoot 'instances.json') 'intentionally invalid fixture index'
 Set-Content (Join-Path $privateRoot 'desktop\stale.json') 'intentionally invalid fixture pointer'
 $indexHash=(Get-FileHash (Join-Path $privateRoot 'instances.json')).Hash
 $running=Case 'running-status' @('-d',$RunningDistro,'desktop','status') 0
 if(!$running.distribution.running -or $running.uses_instance_index -or $running.uses_active_pointer){throw 'Live status authority mismatch'}
 if($running.distro_name -cne $RunningDistro -or $running.distribution.name -cne $RunningDistro -or !$running.distribution.registration_guid -or $running.distribution.PSObject.Properties.Name -contains 'id'){throw 'Public distro name must match the WSL NAME, with GUID explicitly distinguished'}
 $stopped=Case 'stopped-status' @('--distribution',$StoppedDistro,'desktop','status') 0
 if($stopped.distribution.running -or $stopped.guest.state -ne 'not_queried'){throw 'Stopped distro Guest must not execute'}
 if($stopped.distro_name -cne $StoppedDistro -or $stopped.distribution.name -cne $StoppedDistro){throw 'Stopped distro name differs from WSL'}
 $stopExit=if(@($stopped.unresolved_native_hosts).Count){1}else{0}
 $stop=Case 'stopped-stop' @('-d',$StoppedDistro,'desktop','stop') $stopExit
 if($stopExit -eq 0 -and $stop.stop_requested){throw 'Stop must not signal an unobserved Host'}
 Case 'unknown-distro' @('-d',('WSX-Unknown-'+[guid]::NewGuid().ToString('N')),'desktop','status') 1
 Case 'unknown-option' @('-d',$RunningDistro,'desktop','status','--force') 1
 Case 'unowned-start' @('-d',$StoppedDistro,'desktop','start','--reference-runtime',(Join-Path $cache 'absent-runtime')) 1
 if((Get-FileHash (Join-Path $privateRoot 'instances.json')).Hash -ne $indexHash){throw 'Observation changed the private ownership index'}
 $env:LOCALAPPDATA=$originalLocal
 Set-Location $cache
 Invoke-WsxNative $env:CC @('/nologo','/W4','/D_CRT_SECURE_NO_WARNINGS',"$RepoRoot\core\tests\fixtures\native-event.c",'/Fe:wsx-native-host.exe','/link','ole32.lib')
 $vm=[guid]::NewGuid().ToString('B');$instance=[guid]::NewGuid().ToString('B')
 $child=Start-Process -FilePath (Join-Path $cache 'wsx-native-host.exe') -ArgumentList @('--direct',$vm,$instance) -PassThru -RedirectStandardOutput (Join-Path $Evidence 'fixture.stdout') -RedirectStandardError (Join-Path $Evidence 'fixture.stderr')
 $deadline=[DateTime]::UtcNow.AddSeconds(5)
 do {Start-Sleep -Milliseconds 50;$ready=Get-Content (Join-Path $Evidence 'fixture.stdout') -Raw -ErrorAction SilentlyContinue} while(!$ready -and [DateTime]::UtcNow -lt $deadline)
 if(!$ready){throw 'Inert fixture did not become ready; it has a bounded natural timeout'}
 $discovery=Adapter 'fixture-discovery' @{action='discover';distribution=$RunningDistro} 0
 $native=@($discovery.native_processes | Where-Object {$_.pid -eq $child.Id})
 if($native.Count -ne 1 -or !$native[0].identity_valid -or $native[0].stop_event -ne 'ready'){throw 'Cannot discover the exact inert fixture'}
 $request=@{action='stop';distribution=$RunningDistro;registration_guid=[guid]::NewGuid().ToString('D');native=$native[0]}
 Adapter 'wrong-registration-refused' $request 1
 $request.registration_guid=$discovery.registration.registration_guid
 $created=$request.native.creation_filetime;$request.native.creation_filetime='0'
 Adapter 'reused-pid-refused' $request 1
 $request.native.creation_filetime=$created
 $signal=Adapter 'exact-fixture-stop' $request 0
 if(!$signal.stop_requested -or !$child.WaitForExit(5000)){throw 'Inert fixture stop failed; no process was killed'}
 if((Get-Content (Join-Path $Evidence 'fixture.stdout') -Raw) -notmatch 'STOP_REQUEST_OBSERVED'){throw 'Fixture did not receive its event'}
 $after=@(Running-Names)
 if(($before -join "`n") -cne ($after -join "`n")){throw 'Running distro set changed; inspect external activity'}
 $nativeAfter=@(Get-CimInstance Win32_Process -Filter "Name = 'wsx-native-host.exe'" | Select-Object ProcessId,CreationDate,ExecutablePath | Sort-Object ProcessId)
 $nativeAfter | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $Evidence 'existing-native-after.json')
 if(($nativeBefore | ConvertTo-Json -Depth 4 -Compress) -cne ($nativeAfter | ConvertTo-Json -Depth 4 -Compress)){throw 'Existing native process identities changed; inspect external activity'}
 @{passed=$true;cases=$cases;running_distros_unchanged=$true;existing_native_processes_unchanged=$true;existing_desktop_stopped=$false;gpu_executed=$false;full_desktop_start_validated=$false;exe_sha256=(Get-FileHash $exe).Hash} | ConvertTo-Json -Depth 8 | Set-Content (Join-Path $Evidence 'summary.json')
} finally {$env:LOCALAPPDATA=$originalLocal;Stop-Transcript | Out-Null}
