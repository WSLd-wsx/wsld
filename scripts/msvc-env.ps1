param([string]$Toolchain)
$RepoRoot = Split-Path $PSScriptRoot -Parent
if (-not $Toolchain) {
    $rust = Get-Command rustc.exe -ErrorAction SilentlyContinue
    if (-not $rust) { throw 'Rust 1.95.0 is required; pass -Toolchain explicitly or configure PATH' }
    $rustPath = $rust.Source
    $rustup = Get-Command rustup.exe -ErrorAction SilentlyContinue
    if ($rustup) {
        $resolved = & $rustup.Source which --toolchain 1.95.0 rustc 2>$null
        if ($LASTEXITCODE -eq 0 -and $resolved) { $rustPath = $resolved }
    }
    $Toolchain = Split-Path (Split-Path $rustPath -Parent) -Parent
}
$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
if (-not (Test-Path $vswhere)) { throw 'Visual Studio locator is not installed; install MSVC and Windows SDK separately' }
$vs = & $vswhere -latest -products '*' -all -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if (-not $vs) { throw 'Visual Studio installation not found' }
$vcVersion = (Get-Content (Join-Path $vs 'VC\Auxiliary\Build\Microsoft.VCToolsVersion.default.txt')).Trim()
$vc = Join-Path $vs "VC\Tools\MSVC\$vcVersion"
$sdk = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10'
$sdkVersion = (Get-ChildItem (Join-Path $sdk 'Lib') -Directory |
    Where-Object {
        $_.Name -match '^10\.0\.\d+\.\d+$' -and
        (Test-Path (Join-Path $sdk "Include\$($_.Name)\um\Windows.h") -PathType Leaf) -and
        (Test-Path (Join-Path $sdk "Include\$($_.Name)\ucrt\stdio.h") -PathType Leaf) -and
        (Test-Path (Join-Path $_.FullName 'um\x64\kernel32.lib') -PathType Leaf) -and
        (Test-Path (Join-Path $_.FullName 'ucrt\x64\ucrt.lib') -PathType Leaf)
    } | Sort-Object Name -Descending | Select-Object -First 1).Name
if (-not $sdkVersion) { throw 'A complete Windows SDK with headers and x64 libraries is required' }
Write-Output "Selected Windows SDK: $sdkVersion"
$env:PATH = "$Toolchain\bin;$vc\bin\Hostx64\x64;$sdk\bin\$sdkVersion\x64;" + $env:PATH
$env:LIB = "$vc\lib\x64;$sdk\Lib\$sdkVersion\um\x64;$sdk\Lib\$sdkVersion\ucrt\x64"
$env:INCLUDE = "$vc\include;$sdk\Include\$sdkVersion\ucrt;$sdk\Include\$sdkVersion\shared;$sdk\Include\$sdkVersion\um;$sdk\Include\$sdkVersion\winrt"
$env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER = "$vc\bin\Hostx64\x64\link.exe"
$env:CC = "$vc\bin\Hostx64\x64\cl.exe"
$env:AR = "$vc\bin\Hostx64\x64\lib.exe"
# Native Cargo/SQLite and rustc incremental locks require a Windows filesystem.
# This disposable cache is not an installed WSX deployment or a source dependency.
$BuildCache = Join-Path ([IO.Path]::GetTempPath()) 'wsx-product-msvc'
if ($env:WSX_BUILD_CACHE) { $BuildCache = $env:WSX_BUILD_CACHE }
New-Item -ItemType Directory -Force $BuildCache | Out-Null
$env:CARGO_HOME = Join-Path $BuildCache 'cargo'
$env:CARGO_TARGET_DIR = Join-Path $BuildCache 'target'
$env:CARGO_BUILD_JOBS = '4'
$version = & (Join-Path $Toolchain 'bin\rustc.exe') --version
if ($version -notmatch '^rustc 1\.95\.0 ') { throw "Rust 1.95.0 required: $version" }
$env:CARGO_TERM_COLOR = 'never'
$env:VSLANG = '1033'
$env:RUSTC = Join-Path $Toolchain 'bin\rustc.exe'
$Cargo = Join-Path $Toolchain 'bin\cargo.exe'
foreach ($required in @($Cargo, $env:RUSTC, $env:CC, $env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER)) {
    if (-not (Test-Path $required)) { throw "Required tool missing: $required" }
}
function Invoke-WsxNative([string]$Program, [string[]]$Arguments) {
    $previous = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    & $Program @Arguments 2>&1 | ForEach-Object { "$_" }
    $code = $LASTEXITCODE
    $ErrorActionPreference = $previous
    if ($code -ne 0) { throw "Native command failed ($code): $Program $Arguments" }
}
