param([Parameter(Mandatory=$true)][string]$Toolchain,
      [Parameter(Mandatory=$true)][string]$StagedNative,
      [Parameter(Mandatory=$true)][string]$Evidence)
$ErrorActionPreference='Stop'
if(Test-Path -LiteralPath $Evidence){throw 'Evidence directory must be new'}
New-Item -ItemType Directory -Path $Evidence | Out-Null
$here=$PSScriptRoot
. (Join-Path $here '..\..\scripts\msvc-env.ps1') -Toolchain $Toolchain
$work=Join-Path $env:TEMP ('wsx-lifetime-fixture-'+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $work | Out-Null
Start-Transcript -Path (Join-Path $Evidence 'tests.log') | Out-Null
try {
 foreach($name in @('direct-owner.c','direct-owner.h','guest-memory.c','guest-memory.h','host.h','aperture-exit.h','retire-guest.py')){Copy-Item -LiteralPath (Join-Path $StagedNative $name) -Destination $work}
 foreach($name in @('hcs-exit-check.c','lifetime_policy.rs','aperture-exit-check.c','guest_retirement.rs','guest_retirement_tests.rs','fake-wsl.c','retire-system.sh','native-call-trace.h','native-call-trace-check.c')){Copy-Item -LiteralPath (Join-Path $here $name) -Destination $work}
 Invoke-WsxNative $env:CC @('/nologo','/std:c11','/TC',(Join-Path $work 'native-call-trace-check.c'),"/Fe:$work\native-call-trace-check.exe","/Fo:$work\native-call-trace-check.obj")
 Invoke-WsxNative (Join-Path $work 'native-call-trace-check.exe') @()
 Invoke-WsxNative $env:CC @('/nologo','/std:c11','/TC',(Join-Path $work 'hcs-exit-check.c'),"/Fe:$work\hcs-exit-check.exe","/Fo:$work\hcs-exit-check.obj",'/link','ole32.lib')
 Invoke-WsxNative (Join-Path $work 'hcs-exit-check.exe') @()
 Invoke-WsxNative $env:CC @('/nologo','/std:c11','/TC',(Join-Path $work 'aperture-exit-check.c'),"/Fe:$work\aperture-exit-check.exe","/Fo:$work\aperture-exit-check.obj")
 Invoke-WsxNative (Join-Path $work 'aperture-exit-check.exe') @()
 Invoke-WsxNative $env:CC @('/nologo','/std:c11','/TC',(Join-Path $work 'fake-wsl.c'),"/Fe:$work\fake-wsl.exe","/Fo:$work\fake-wsl.obj")
 $env:WSX_TEST_WSL=Join-Path $work 'fake-wsl.exe'
 Invoke-WsxNative $env:RUSTC @('--edition=2021','--test',(Join-Path $work 'guest_retirement.rs'),'-o',(Join-Path $work 'guest-retirement.exe'))
 Invoke-WsxNative (Join-Path $work 'guest-retirement.exe') @('--nocapture')
 Copy-Item -Path (Join-Path $work '*') -Destination $Evidence
 @{passed=$true;native_call_trace='opt-in, elapsed time and LastError preservation';hcs_return_policy='simulated';aperture_close='production C, simulated HDV/HCS, real Windows memory';rust_tests=3;helper_scenarios=5;gpu_executed=$false;vm_stopped=$false} |
  ConvertTo-Json | Set-Content -Encoding UTF8 (Join-Path $Evidence 'result.json')
} finally {Stop-Transcript | Out-Null}
