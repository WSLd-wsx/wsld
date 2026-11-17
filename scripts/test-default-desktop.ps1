param([Parameter(Mandatory=$true)][string]$Executable,
      [Parameter(Mandatory=$true)][string]$Evidence,
      [Parameter(Mandatory=$true)][string]$Distro)
$ErrorActionPreference='Stop'
if(Test-Path $Evidence){throw 'Evidence directory must be new'}
function Owners {
    @(Get-CimInstance Win32_Process | Where-Object Name -eq wsx-native-host.exe |
        Select-Object ProcessId,CreationDate,ExecutablePath,CommandLine)
}
$before=Owners
if($before.Count -eq 0){throw 'This refusal test requires an existing unrelated Host; no launch attempted'}
New-Item -ItemType Directory $Evidence | Out-Null
$before | ConvertTo-Json | Set-Content "$Evidence\owners-before.json"
$cases=@()
function Check([string]$Name,[string[]]$Arguments,[int]$Exit,[string]$Pattern) {
    $p=Start-Process $Executable -ArgumentList $Arguments -PassThru -Wait `
        -RedirectStandardOutput "$Evidence\$Name.stdout" -RedirectStandardError "$Evidence\$Name.stderr"
    $text=(Get-Content -Raw "$Evidence\$Name.stdout")+(Get-Content -Raw "$Evidence\$Name.stderr")
    $passed=$p.ExitCode -eq $Exit -and $text -match $Pattern
    $script:cases+=@{name=$Name;arguments=$Arguments;exit_code=$p.ExitCode;passed=$passed}
    if(!$passed){throw "Failed: $Name; inspect retained output"}
}
try {
    Check 'short-default' @('-d',$Distro) 1 'Native ownership is ambiguous|Another Native Host'
    Check 'long-default' @('--distribution',$Distro) 1 'Native ownership is ambiguous|Another Native Host'
    Check 'explicit-default' @('-d',$Distro,'desktop','start') 1 'Native ownership is ambiguous|Another Native Host'
    Check 'long-status' @('--distribution',$Distro,'desktop','status') 0 '"distro_name"'
    $status=Get-Content -Raw "$Evidence\long-status.stdout" | ConvertFrom-Json
    if($status.distro_name -cne $Distro){throw 'Unexpected distro name'}
    if($status.distribution.running){
        if($status.guest.initialized -ne $false){throw 'Unexpected account initialization'}
    } elseif($status.guest.state -ne 'not_queried') {throw 'Status must not wake a stopped distro'}
    Check 'missing-name' @('--distribution') 1 'Usage:'
    Check 'mode-injection' @('--distribution','--shutdown') 1 'Invalid WSL distro name'
    $after=Owners
    $after | ConvertTo-Json | Set-Content "$Evidence\owners-after.json"
    if(($before | ConvertTo-Json -Compress) -cne ($after | ConvertTo-Json -Compress)){throw 'Existing Native identities changed'}
} finally {
    @{scope='Real Windows alias/default routing and existing-owner refusal; no desktop launch or initialization';cases=$cases} |
        ConvertTo-Json -Depth 8 | Set-Content "$Evidence\summary.json"
}
