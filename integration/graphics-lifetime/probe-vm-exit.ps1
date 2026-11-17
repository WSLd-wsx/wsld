param([Parameter(Mandatory=$true)][string]$Toolchain,
      [Parameter(Mandatory=$true)][string]$Evidence)
$ErrorActionPreference='Stop'
if(Test-Path -LiteralPath $Evidence){throw 'Evidence directory must be new'}
New-Item -ItemType Directory -Path $Evidence | Out-Null
. (Join-Path $PSScriptRoot '..\..\scripts\msvc-env.ps1') -Toolchain $Toolchain
$work=Join-Path $env:TEMP ('wsx-isolated-vm-'+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $work | Out-Null
Copy-Item (Join-Path $PSScriptRoot 'probe-vm-exit.cpp') $work
Copy-Item (Join-Path $PSScriptRoot 'aperture-exit.h') $work
Copy-Item (Join-Path $PSScriptRoot 'probe-aperture-bridge.c') $work
Start-Transcript -Path (Join-Path $Evidence 'probe.log') | Out-Null
try {
    Invoke-WsxNative $env:CC @('/nologo','/std:c11','/TC','/c',"$work\probe-aperture-bridge.c","/Fo:$work\bridge.obj")
    Invoke-WsxNative $env:CC @('/nologo','/EHsc','/std:c++17',"$work\probe-vm-exit.cpp","$work\bridge.obj","/Fe:$work\probe.exe","/Fo:$work\probe.obj",'/link','computecore.lib','ole32.lib')
    $info=New-Object Diagnostics.ProcessStartInfo
    $info.FileName="$work\probe.exe"
    $info.UseShellExecute=$false
    $info.RedirectStandardOutput=$true
    $info.RedirectStandardError=$true
    $probe=New-Object Diagnostics.Process
    $probe.StartInfo=$info
    if(!$probe.Start()){throw 'Isolated VM probe did not start'}
    $out=$probe.StandardOutput.ReadToEndAsync()
    $err=$probe.StandardError.ReadToEndAsync()
    if(!$probe.WaitForExit(120000)){throw 'Isolated VM probe timed out; process retained'}
    $probe.WaitForExit()
    $code=$probe.ExitCode
    [IO.File]::WriteAllText((Join-Path $Evidence 'stdout.txt'),$out.Result)
    [IO.File]::WriteAllText((Join-Path $Evidence 'stderr.txt'),$err.Result)
    @{exit_code=$code; scope='Diskless isolated HCS VM; no WSL shutdown or GPU work'} | ConvertTo-Json | Set-Content (Join-Path $Evidence 'result.json')
    Get-Content (Join-Path $Evidence 'stdout.txt')
    Copy-Item "$work\*" $Evidence
    if($code -ne 0){throw "Isolated probe failed: $code"}
} finally {Stop-Transcript | Out-Null}
