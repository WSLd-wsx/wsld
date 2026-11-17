param([Parameter(Mandatory=$true)][string]$Kit)
$ErrorActionPreference='Stop'
$state=Get-Content "$Kit\state.json" -Raw | ConvertFrom-Json
function Snapshot {
 $svc=Get-CimInstance Win32_Service | Where-Object Name -eq WSLService
 [ordered]@{
  service=@($svc.PathName,$svc.ProcessId,$svc.State,$svc.StartMode)
  plugin=(Get-Item 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Lxss\Plugins').GetValue('WSXVenusNative')
  com=@($state.com | ForEach-Object {(Get-Item $_.key).GetValue('')})
  hosts=@(Get-CimInstance Win32_Process | Where-Object {$_.Name -in @('wsx-native-host.exe','wsx-input-host.exe')} | Sort-Object ProcessId | Select-Object Name,ProcessId,CreationDate)
 } | ConvertTo-Json -Depth 8 -Compress
}
$before=Snapshot
$results=@()
foreach($file in @("$Kit\legacy-wsl.ps1", "$PSScriptRoot\prepare-legacy-wsl.ps1")){
 $tokens=$null;$errors=$null
 [void][System.Management.Automation.Language.Parser]::ParseFile($file,[ref]$tokens,[ref]$errors)
 if($errors.Count){throw ($errors | Out-String)}
 $results+=@{case='PowerShell parser';file=$file;passed=$true}
}
& "$Kit\legacy-wsl.ps1" -Action Inspect | Set-Content -Encoding UTF8 "$Kit\inspection.json"
foreach($action in @('StageReboot','FinishStock','Restore')){
 $rejected=$false
 try {& "$Kit\legacy-wsl.ps1" -Action $action}
 catch {if($_.Exception.Message -eq 'Explicit maintenance authorization is required; no changes made'){$rejected=$true}else{throw}}
 if(!$rejected){throw "Missing authorization guard: $action"}
 $results+=@{case=$action+' without authorization';passed=$true}
}
$after=Snapshot
if($before -cne $after){throw 'Live configuration or process identities changed during observation'}
$results+=@{case='Service, plugin, six COM paths and Host identities unchanged';passed=$true}
@{schema_version=2;utc=[DateTime]::UtcNow.ToString('o');results=$results;before=($before | ConvertFrom-Json);after=($after | ConvertFrom-Json);mutating_actions_run=$false} |
 ConvertTo-Json -Depth 12 | Set-Content -Encoding UTF8 "$Kit\guard-tests.json"
Write-Output 'Preparation parser and refusal checks passed. No maintenance action was executed.'
