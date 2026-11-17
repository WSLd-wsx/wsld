param(
    [Parameter(Mandatory=$true)][string]$Executable,
    [Parameter(Mandatory=$true)][string]$Evidence,
    [Parameter(Mandatory=$true)][ValidatePattern('^wsx-(omarchy|cachyos-plasma)-[0-9]+\.[0-9]+\.[0-9]+-r[0-9]+$')][string]$Tag
)
# This opt-in acceptance creates one owned test distro. It never unregisters it.
$ErrorActionPreference='Stop'
if(Test-Path -LiteralPath $Evidence){throw 'Evidence directory must be new'}
function Registrations {
    @(Get-ChildItem HKCU:\Software\Microsoft\Windows\CurrentVersion\Lxss |
        Get-ItemProperty | Select-Object DistributionName,BasePath,Version,PSChildName)
}
$before=Registrations
if(@($before | Where-Object {$_.DistributionName -ieq $Tag}).Count){throw 'Test distro name already exists; no adoption'}
New-Item -ItemType Directory -Path $Evidence | Out-Null
$before | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $Evidence 'registrations-before.json')
Get-PSDrive -PSProvider FileSystem | Select-Object Name,Used,Free | ConvertTo-Json | Set-Content (Join-Path $Evidence 'space-before.json')
Get-FileHash $Executable -Algorithm SHA256 | ConvertTo-Json | Set-Content (Join-Path $Evidence 'executable.json')
$owners=@(Get-CimInstance Win32_Process -Filter "Name='wsx-native-host.exe'" | Select-Object ProcessId,CreationDate,ExecutablePath)
$owners | ConvertTo-Json | Set-Content (Join-Path $Evidence 'native-before.json')
$started=[DateTime]::UtcNow.ToString('o')
$process=Start-Process -FilePath $Executable -ArgumentList @('--install','--pre-release',$Tag,'--no-launch') -PassThru `
    -RedirectStandardOutput (Join-Path $Evidence 'install.stdout') -RedirectStandardError (Join-Path $Evidence 'install.stderr')
$processHandle=$process.Handle
$process.WaitForExit()
if($null -eq $process.ExitCode){throw 'Installer exit code unavailable'}
$after=Registrations
$after | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $Evidence 'registrations-after.json')
$unchanged=$true
foreach($old in $before){
    $current=@($after | Where-Object {$_.PSChildName -eq $old.PSChildName})
    if($current.Count -ne 1 -or $current[0].DistributionName -cne $old.DistributionName -or $current[0].BasePath -cne $old.BasePath -or $current[0].Version -ne $old.Version){$unchanged=$false}
}
$new=@($after | Where-Object {$_.DistributionName -ieq $Tag})
@{tag=$Tag;started_utc=$started;finished_utc=[DateTime]::UtcNow.ToString('o');exit_code=$process.ExitCode;
    previous_registrations_unchanged=$unchanged;new_registration=$new;cleanup='Retained; no unregister or VHD deletion';
    scope='Rootfs download, verification, zstd-to-tar and WSL import only; no initialization or desktop launch'} |
    ConvertTo-Json -Depth 6 | Set-Content (Join-Path $Evidence 'summary.json')
if($process.ExitCode -ne 0 -or -not $unchanged -or $new.Count -ne 1){throw 'Real import acceptance failed; state and logs retained'}
