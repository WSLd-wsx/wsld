# Windows-only process, registry and HCS adapter. Core owns policy and state.
param([Parameter(Mandatory=$true)][ValidateSet('Inspect','Observe','SignalStop')][string]$Action,
      [Parameter(Mandatory=$true)][string]$Directory,
      [string]$Registration,[string]$Vm,[string]$Instance,[string]$IdentityExecutable,[switch]$AllowConcurrent)
$ErrorActionPreference='Stop'
[Console]::OutputEncoding=New-Object System.Text.UTF8Encoding($false)
$exe=Join-Path $Directory 'wsx-native-host.exe'
$receipt=Join-Path $Directory 'native.json'
if($Action -eq 'Inspect'){
    $owners=@(Get-Process | Where-Object {$_.ProcessName -match '^(wsx-native-host|wsx-input-host|venus.*|rutabaga.*)$'})
    if($owners.Count -and !$AllowConcurrent){throw 'Existing graphics Host processes require owner diagnosis; none were stopped'}
    $entries=@(Get-ChildItem 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Lxss' | ForEach-Object {Get-ItemProperty $_.PSPath} | Where-Object {$_.DistributionName -eq $Registration})
    if($entries.Count -ne 1 -or $entries[0].Version -ne 2){throw 'Expected one WSL 2 registration'}
    $listing=(& "$env:SystemRoot\System32\hcsdiag.exe" list | Out-String)
    if($LASTEXITCODE -ne 0){throw 'HCS discovery failed'}
    $identityMatches=[regex]::Matches($listing,'(?im)^\s*([0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12})\s*\r?\n\s*VM,.*\bWSL\b')
    if($identityMatches.Count -ne 1){throw 'Expected exactly one running WSL VM'}
    if (!$IdentityExecutable) { throw 'Graphics identity executable is unavailable' }
    $paths=& $IdentityExecutable --internal-graphics-identity display
    if($LASTEXITCODE -ne 0){throw 'Display inventory failed'}
    $devices=& $IdentityExecutable --internal-graphics-identity vulkan
    if($LASTEXITCODE -ne 0){throw 'Vulkan inventory failed; inspect the Vulkan loader and graphics driver. Staging Copy is not yet admitted.'}
    @{vm=$identityMatches[0].Groups[1].Value;instance=[guid]::NewGuid().ToString('B');
      disk=$entries[0].BasePath;paths_json=($paths -join "`n");
      devices_json=($devices -join "`n");
      registration=@{name=$entries[0].DistributionName;id=([guid]$entries[0].PSChildName).ToString('D');disk=$entries[0].BasePath;version=$entries[0].Version};
      kernel=(Join-Path $env:ProgramFiles 'WSL\tools\kernel')} | ConvertTo-Json -Depth 20 -Compress
    exit
}
$record=Get-Content -LiteralPath $receipt -Raw | ConvertFrom-Json
$process=Get-Process -Id $record.pid -ErrorAction SilentlyContinue
if($record.creation_filetime){
    $identityMatches=$process -and $process.StartTime.ToUniversalTime().ToFileTimeUtc().ToString() -eq $record.creation_filetime
}else{
    $identityMatches=$process -and $process.StartTime.ToUniversalTime().ToString('o') -eq $record.started_utc
}
if($identityMatches -and $process.Path -ne $record.executable){throw 'Native executable identity differs from receipt'}
if($Action -eq 'SignalStop'){
    if(!$identityMatches){throw 'Recorded native process is no longer running; no event signalled'}
    $eventName="Local\WSXDirectStop-$(([guid]$record.instance).ToString('B').ToUpperInvariant())"
    $event=[Threading.EventWaitHandle]::OpenExisting($eventName)
    try{[void]$event.Set()}finally{$event.Dispose()}
}
$vmError=$null;$vmPresent=$true;$currentVm=$null
try {
 $listing=(& "$env:SystemRoot\System32\hcsdiag.exe" list | Out-String)
 if($LASTEXITCODE -ne 0){throw 'HCS observation failed'}
 $ids=@([regex]::Matches($listing,'(?im)^\s*([0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12})\s*\r?\n\s*VM,.*\bWSL\b') | ForEach-Object {([guid]$_.Groups[1].Value).ToString('D')})
 $vmPresent=$ids -contains ([guid]$record.vm).ToString('D')
 if($ids.Count -eq 1){$currentVm=$ids[0]}
} catch {$vmError=$_.Exception.Message}
$boot=(Get-CimInstance Win32_OperatingSystem).LastBootUpTime.ToUniversalTime().ToFileTimeUtc().ToString()
$graphics=@(Get-Process -ErrorAction Stop | Where-Object {$_.ProcessName -match '^(wsx-native-host|wsx-input-host|venus.*|rutabaga.*)$'})
@{running=[bool]$identityMatches;pid=$record.pid;windows_boot_filetime=$boot;old_vm_present=$vmPresent;vm_query_error=$vmError;
 old_vm_id=([guid]$record.vm).ToString('D');current_vm_id=$currentVm;graphics_owners_absent=($graphics.Count -eq 0)} | ConvertTo-Json -Compress
