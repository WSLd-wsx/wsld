param(
    [Parameter(Mandatory=$true)][string]$Registration,
    [Parameter(Mandatory=$true)][string]$CoreExe,
    [string]$Timezone = ''
)
$ErrorActionPreference = 'Stop'
if ($Registration -notmatch '^WSX-Test-Omarchy-[0-9]+-[0-9]+$') { throw 'Expected an owned Omarchy test registration.' }
if (-not $Timezone) {
    $Timezone = & $CoreExe timezone
    if ($LASTEXITCODE -ne 0) { throw 'Cannot map the Windows timezone; select an IANA timezone explicitly.' }
}
if ($Timezone -notmatch '^[A-Za-z0-9_+.-]+(/[A-Za-z0-9_+.-]+)*$') { throw 'Invalid IANA timezone.' }
$wsl = Join-Path $env:SystemRoot 'System32\wsl.exe'
& $wsl -d $Registration -u root -- /usr/bin/test -f "/usr/share/zoneinfo/$Timezone"
if ($LASTEXITCODE -ne 0) { throw 'The mapped timezone is unavailable in the Guest.' }
Write-Host "Windows timezone maps to $Timezone"
$username = Read-Host 'Linux username'
if ($username -notmatch '^[a-z_][a-z0-9_-]{0,30}$' -or $username -eq 'root') { throw 'Invalid Linux username.' }
Write-Host 'Enter and confirm the password at the Guest prompts. Input is not echoed.'
& $wsl -d $Registration -u root -- /usr/bin/wsx-guest initialize --user $username --timezone $Timezone
if ($LASTEXITCODE -ne 0) { Write-Host "Initialization failed with exit code $LASTEXITCODE. Preserve the instance and inspect pending state."; Read-Host 'Press Enter to close'; exit $LASTEXITCODE }
Write-Host 'Guest initialization returned successfully. The caller must verify the recorded account and UID.'
Read-Host 'Press Enter to close'
