param(
    [Parameter(Mandatory=$true)][string]$Executable,
    [Parameter(Mandatory=$true)][string]$Evidence,
    [string]$Candidate='wsx-omarchy-4.0.6-r11'
)
$ErrorActionPreference='Stop'
if(Test-Path -LiteralPath $Evidence){throw 'Evidence directory must be new'}
New-Item -ItemType Directory -Path $Evidence | Out-Null
$rows=@()
function Invoke-Case([string]$Name,[string[]]$Arguments,[bool]$Success,[string]$Pattern) {
    $out=Join-Path $Evidence "$Name.stdout"
    $err=Join-Path $Evidence "$Name.stderr"
    $process=Start-Process -FilePath $Executable -ArgumentList $Arguments -RedirectStandardOutput $out -RedirectStandardError $err -Wait -PassThru
    $output=(Get-Content -Raw $out)+(Get-Content -Raw $err)
    $pass=(($process.ExitCode -eq 0) -eq $Success) -and ($output -match $Pattern)
    $script:rows+=@{name=$Name;arguments=$Arguments;exit_code=$process.ExitCode;passed=$pass}
    if(-not $pass){throw "Case failed: $Name; inspect $out and $err"}
}
try {
    Invoke-Case 'help' @('--help') $true '--install --pre-release'
    Invoke-Case 'old-command-rejected' @('--install-pre-release',$Candidate) $false 'Invalid CLI arguments'
    Invoke-Case 'standalone-option-rejected' @('--pre-release',$Candidate) $false 'Invalid CLI arguments'
    Invoke-Case 'missing-request' @('--install','--pre-release') $false 'Usage: wsld.exe --install'
    Invoke-Case 'duplicate-option' @('--install','--pre-release',$Candidate,'--pre-release') $false 'Usage: wsld.exe --install'
    Invoke-Case 'candidate-exact-required' @('--install','--pre-release','wsx-omarchy') $false 'exact version'
    Invoke-Case 'unknown-distro' @('--install','wsx-omarchy-other-4.0.6-r11') $false 'Invalid numeric|Expected stable|Unknown distribution'
    Invoke-Case 'rolling-latest' @('--install','wsx-cachyos-plasma') $false 'ordering is unresolved'
    Invoke-Case 'candidate-metadata' @('--install','--pre-release',$Candidate,'--check') $true '"installed":false'
    Invoke-Case 'candidate-option-after-version' @('--install',$Candidate,'--pre-release','--check') $true '"installed":false'
    Invoke-Case 'stable-excludes-candidate' @('--install',$Candidate,'--check') $false 'No published stable release'
    Invoke-Case 'missing-exact' @('--install','--pre-release','wsx-omarchy-999.0.0-r1','--check') $false 'HTTP 404'
} finally {
    @{scope='Real Windows CLI and public network metadata; no payload download or WSL import';cases=$rows} |
        ConvertTo-Json -Depth 8 | Set-Content (Join-Path $Evidence 'summary.json')
}
