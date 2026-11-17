param([Parameter(Mandatory=$true)][string]$Package,[Parameter(Mandatory=$true)][string]$Evidence)
$ErrorActionPreference='Stop'
if(Test-Path $Evidence) {throw 'Evidence must be new'}
New-Item -ItemType Directory $Evidence | Out-Null
$results=@()
foreach($name in @('wsld','wsx')) {
    $exe=Join-Path $Package "$name.exe"
    $output=@(& $exe --help | ForEach-Object {$_})
    if($LASTEXITCODE -ne 0 -or ($output -join "`n") -notmatch 'Usage:') {throw "Pipeline failed: $name"}
    $output | Set-Content "$Evidence\$name-pipe.log"
    & $exe --version > "$Evidence\$name-version.log"
    if($LASTEXITCODE -ne 0) {throw "Redirect exit failed: $name"}
    $prior=$ErrorActionPreference; $ErrorActionPreference='Continue'
    & $exe --gui 2> "$Evidence\$name-error.log"
    $code=$LASTEXITCODE; $ErrorActionPreference=$prior
    if($code -ne 1) {throw "Invalid-command exit failed: $name"}
    $results+=@{name=$name;pipe=$true;redirect=$true;error_exit=$code}
}
@{scope='real PowerShell pipeline/redirection/exit cases only';results=$results;ctrl_c='not_run';explorer_flash='not_run';shell_execute='not_run'} | ConvertTo-Json -Depth 4 | Set-Content "$Evidence\result.json"
