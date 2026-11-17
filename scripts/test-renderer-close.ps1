# Send a real WM_CLOSE only to an explicitly identified test lifetime.
param([Parameter(Mandatory=$true)][string]$Receipt,
      [Parameter(Mandatory=$true)][string]$Evidence)
$ErrorActionPreference='Stop'
if(Test-Path -LiteralPath $Evidence){throw 'Evidence directory must be new'}
New-Item -ItemType Directory -Path $Evidence | Out-Null
$record=Get-Content -LiteralPath $Receipt -Raw | ConvertFrom-Json
$owner=Get-Process -Id $record.pid
$created=$owner.StartTime.ToUniversalTime().ToFileTimeUtc().ToString()
if($created -ne $record.creation_filetime -or $owner.Path -ne $record.executable){throw 'Native receipt identity mismatch'}
if((Get-FileHash -LiteralPath $owner.Path -Algorithm SHA256).Hash -ne '4748e4769e6ce6eafb5d9774ecadfa2bcaa4ea343d634aee2827ff00ea5d8169'){
 throw 'Expected the paired renderer-close Native Host build'
}
$handle=$owner.Handle # Keep the exact process handle for exit observation.
$children=@(Get-CimInstance Win32_Process | Where-Object {$_.ParentProcessId -eq $owner.Id -and $_.Name -eq 'wsx-input-host.exe'} | ForEach-Object {Get-Process -Id $_.ProcessId})
if($children.Count -ne 1){throw 'Expected one input companion belonging to this Native Host'}
$inputOwner=$children[0];$inputHandle=$inputOwner.Handle
$before=@{native_pid=$owner.Id;native_creation_filetime=$created;input_pid=$inputOwner.Id;
 input_creation_filetime=$inputOwner.StartTime.ToUniversalTime().ToFileTimeUtc().ToString();instance=$record.instance;receipt=$Receipt}
$before | ConvertTo-Json | Set-Content -Encoding UTF8 (Join-Path $Evidence 'before.json')
& (Join-Path $PSScriptRoot 'capture-desktop-candidate.ps1') -Receipt $Receipt -Output (Join-Path $Evidence 'desktop.png') |
 Set-Content -Encoding UTF8 (Join-Path $Evidence 'capture.json')
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class WsxCloseTest {
 [DllImport("user32.dll", SetLastError=true)] public static extern bool PostMessage(IntPtr window, uint message, UIntPtr w, IntPtr l);
}
'@
# Recheck identity after capture; never target a reused process ID.
$current=Get-Process -Id $record.pid
if($current.StartTime.ToUniversalTime().ToFileTimeUtc().ToString() -ne $created -or $current.Path -ne $record.executable){throw 'Native process changed before close'}
$window=[WsxCandidateCapture]::Find($record.pid)
if($window -eq [IntPtr]::Zero){throw 'No visible Native Host window'}
if(![WsxCloseTest]::PostMessage($window,0x0010,[UIntPtr]::Zero,[IntPtr]::Zero)){throw 'WM_CLOSE delivery failed'}
@{message='WM_CLOSE';window=$window.ToInt64();utc=[DateTime]::UtcNow.ToString('o')} |
 ConvertTo-Json | Set-Content -Encoding UTF8 (Join-Path $Evidence 'close.json')
$deadline=[DateTime]::UtcNow.AddSeconds(90)
while(!$owner.HasExited -and [DateTime]::UtcNow -lt $deadline){Start-Sleep -Milliseconds 500}
$text=Get-Content -LiteralPath $record.log -Raw
Copy-Item -LiteralPath $record.log -Destination (Join-Path $Evidence 'native.log')
$instance=([guid]$record.instance).ToString('D')
$result=@{native_exited=$owner.HasExited;input_exited=$inputOwner.HasExited;forced_termination=$false;
 guest_retired=[bool]($text -match ('(?m)^WSX_GUEST_RETIRED instance='+[regex]::Escape($instance)+'(?:\s|$)'));
 zero_retained_memory=[bool]($text -match 'event=close_complete[^\r\n]*retained_slots=0 usable_pages=0 usable_bytes=0');
 native_closed=$text.Contains('WSX_NATIVE: memory, native device and bootstrap closed');
 input_normal=$text.Contains('WSX_NATIVE: input companion exited normally');
 quarantined=$text.Contains('quarantined') -or $text.Contains('retaining native state and the original COM thread after failure')}
if($owner.HasExited){$result.native_exit_code=$owner.ExitCode}
if($inputOwner.HasExited){$result.input_exit_code=$inputOwner.ExitCode}
$result.passed=$result.native_exited -and $result.input_exited -and $result.guest_retired -and $result.zero_retained_memory -and $result.native_closed -and $result.input_normal -and !$result.quarantined -and $result.native_exit_code -eq 0 -and $result.input_exit_code -eq 0
$result | ConvertTo-Json | Set-Content -Encoding UTF8 (Join-Path $Evidence 'result.json')
$result | ConvertTo-Json
if(!$result.passed){throw 'Normal renderer close did not pass; owners and logs retained without forced cleanup'}
