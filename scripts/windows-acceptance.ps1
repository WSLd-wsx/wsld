param([Parameter(Mandatory=$true)][string]$Package, [Parameter(Mandatory=$true)][string]$Fixtures, [Parameter(Mandatory=$true)][string]$Evidence)
$ErrorActionPreference = 'Stop'
if (Test-Path $Evidence) { throw 'Evidence must be a new directory' }
New-Item -ItemType Directory $Evidence | Out-Null
$local = Join-Path ([IO.Path]::GetTempPath()) ('wsx-acceptance-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory $local | Out-Null
Copy-Item "$Package\*.exe" $local
# Deliberately keep actual Pro beside CLI for isolation coverage when supplied.
if (Test-Path "$Package\wsxpro.dll") { Copy-Item "$Package\wsxpro.dll" $local }
foreach ($source in Get-ChildItem "$Package\*.exe") {
    if ((Get-FileHash $source.FullName).Hash -ne (Get-FileHash (Join-Path $local $source.Name)).Hash) { throw 'NTFS copy hash mismatch' }
}
$env:WSX_LOAD_TRACE = '1'
$results = [Collections.Generic.List[object]]::new()
function Check([string]$Name,[string]$Exe,[string[]]$Arguments,[int]$Expected,[string[]]$Markers,[string]$Cwd=$local) {
    $stdout=Join-Path $Evidence "$Name.stdout.log"; $stderr=Join-Path $Evidence "$Name.stderr.log"
    $parameters=@{FilePath=$Exe;WorkingDirectory=$Cwd;PassThru=$true;RedirectStandardOutput=$stdout;RedirectStandardError=$stderr}
    if ($Arguments.Count) { $parameters.ArgumentList=$Arguments }
    $process=Start-Process @parameters; $null=$process.Handle
    if (-not $process.WaitForExit(20000)) {
        @{name=$Name;passed=$false;timed_out=$true;pid=$process.Id;process_retained=$true} | ConvertTo-Json | Set-Content "$Evidence\timeout.json"
        throw "Timeout; owned process $($process.Id) retained"
    }
    $output=(Get-Content $stdout -Raw)+(Get-Content $stderr -Raw)
    $pass=$process.ExitCode -eq $Expected
    foreach ($marker in $Markers) { $pass=$pass -and $output.Contains($marker) }
    $results.Add(@{name=$Name;passed=$pass;exit_code=$process.ExitCode})
    $results | ConvertTo-Json -Depth 5 | Set-Content "$Evidence\results.json"
    if (-not $pass) { throw "Case failed: $Name`n$output" }
    $process.Dispose()
}
foreach ($name in @('wsx','wsld')) {
    Check "$name-help" "$local\$name.exe" @('--help') 0 @('Usage:','LOAD_TRACE pro=0 webview=0')
    Check "$name-version" "$local\$name.exe" @('--version') 0 @('WSX Core','LOAD_TRACE pro=0 webview=0')
    Check "$name-catalog" "$local\$name.exe" @('catalog') 0 @('wsx-cachyos-plasma','LOAD_TRACE pro=0 webview=0')
    Check "$name-diagnostic" "$local\$name.exe" @('diagnostic-task','3') 0 @('no installation performed','LOAD_TRACE pro=0 webview=0')
    Check "$name-install-invalid" "$local\$name.exe" @('--install','invalid-distro') 1 @('Unknown distribution ID','LOAD_TRACE pro=0 webview=0')
    Check "$name-invalid" "$local\$name.exe" @('--gui') 1 @('Invalid CLI arguments','LOAD_TRACE pro=0 webview=0')
    Check "$name-invalid-steps" "$local\$name.exe" @('diagnostic-task','0') 1 @('between 1 and 100','LOAD_TRACE pro=0 webview=0')
}
Copy-Item "$local\wsx.exe" "$local\unknown.exe"
Check 'unknown-name' "$local\unknown.exe" @('--help') 1 @('Unknown executable name','LOAD_TRACE pro=0 webview=0')
Check 'gui-mode-switch' "$local\wsxdesk.exe" @('--cli') 1 @('GUI entry accepts no mode switches','LOAD_TRACE pro=0 webview=0')
$decoy=New-Item -ItemType Directory "$local\decoy"
Copy-Item "$Fixtures\fail-init.dll" "$decoy\wsxpro.dll"
$priorPath=$env:PATH
try {
    $env:PATH="$decoy;"+$env:PATH
    foreach ($case in @('missing-dll','missing-export','bad-abi','fail-init','wrong-arch','valid')) {
        $dir=New-Item -ItemType Directory "$local\$case"; Copy-Item "$local\wsxdesk.exe" $dir
        if ($case -ne 'missing-dll') { Copy-Item "$Fixtures\$case.dll" "$dir\wsxpro.dll" }
        $expected=1; $marker=switch ($case) {
            'missing-dll' {'Cannot inspect DLL path'}
            'missing-export' {'Missing Pro export'}
            'bad-abi' {'Pro ABI mismatch'}
            'fail-init' {'Pro initialization failed: -4'}
            'wrong-arch' {'LoadLibraryExW failed: 193'}
            'valid' {$expected=0; 'module retained until process exit'}
        }
        Check $case "$dir\wsxdesk.exe" @() $expected @($marker) $decoy.FullName
    }
} finally { $env:PATH=$priorPath }
Check 'argv0-spoof-cli' "$Fixtures\argv0-probe.exe" @(('"'+"$local\wsx.exe"+'"'),'"wsxdesk.exe --help"') 0 @('Usage:','LOAD_TRACE pro=0 webview=0')
Check 'argv0-spoof-pro' "$Fixtures\argv0-probe.exe" @(('"'+"$local\valid\wsxdesk.exe"+'"'),'"wsx.exe"') 0 @('module retained until process exit')
@{kind='real Windows processes';local_package=$local;cases=$results.Count;passed=$true;existing_wsl_modified=$false} | ConvertTo-Json | Set-Content "$Evidence\summary.json"
