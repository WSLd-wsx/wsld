# SPDX-License-Identifier: BUSL-1.1
# Record explicit manual observations for one existing image; never changes WSL.
param(
    [Parameter(Mandatory=$true)][string]$Manifest,
    [Parameter(Mandatory=$true)][string]$Archive,
    [Parameter(Mandatory=$true)][string]$Output,
    [Parameter(Mandatory=$true)][string]$GraphicsRuntime
)
$ErrorActionPreference = 'Stop'
if (Test-Path -LiteralPath $Output) { throw 'Output already exists; refusing to overwrite evidence.' }
$manifestData = Get-Content -LiteralPath $Manifest -Raw | ConvertFrom-Json
$archiveItem = Get-Item -LiteralPath $Archive
$archiveHash = (Get-FileHash -LiteralPath $Archive -Algorithm SHA256).Hash.ToLowerInvariant()
$manifestHash = (Get-FileHash -LiteralPath $Manifest -Algorithm SHA256).Hash.ToLowerInvariant()
if ($archiveHash -ne $manifestData.sha256 -or $archiveItem.Length -ne $manifestData.compressed_size -or $archiveItem.Name -ne $manifestData.asset_name) {
    throw 'Archive name, size or SHA-256 differs from the manifest.'
}
$tag = '{0}-{1}' -f $manifestData.distribution_id, $manifestData.image_version
Write-Host "Recording manual evidence for $tag; image SHA-256 $archiveHash"
Write-Host 'Only report observations for a distro imported from this exact image. Do not enter passwords or personal data.'
$stages = [ordered]@{}
foreach ($stage in @('wsl_boot','account_initialization','gpu_first_frame','desktop_input','normal_stop')) {
    do {
        $answer = (Read-Host "$stage result: passed / failed / not_run").Trim().ToLowerInvariant()
    } while ($answer -notin @('passed','failed','not_run'))
    $stages[$stage] = [ordered]@{result=$answer; source='manual'}
}
$gpus = @(Get-CimInstance Win32_VideoController | ForEach-Object { $_.Name }) -join ', '
$os = Get-CimInstance Win32_OperatingSystem
$wslVersion = (& wsl.exe --version 2>$null | Out-String).Trim() -replace '[\r\n]+','; '
if (-not $wslVersion) { $wslVersion = 'Unavailable' }
if (-not $gpus) { $gpus = 'Unavailable' }
$record = [ordered]@{
    schema_version=1; tag=$tag; archive_sha256=$archiveHash; manifest_sha256=$manifestHash
    observed_at_utc=(Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')
    environment=[ordered]@{windows_version=('{0} ({1})' -f $os.Caption,$os.BuildNumber); wsl_version=$wslVersion; gpu_description=$gpus; graphics_runtime=$GraphicsRuntime}
    stages=$stages
}
$json = $record | ConvertTo-Json -Depth 5
$path = [System.IO.Path]::GetFullPath($Output)
$stream = [System.IO.File]::Open($path,[System.IO.FileMode]::CreateNew,[System.IO.FileAccess]::Write,[System.IO.FileShare]::None)
try {
    $writer = [System.IO.StreamWriter]::new($stream,[System.Text.UTF8Encoding]::new($false))
    try { $writer.WriteLine($json) } finally { $writer.Dispose() }
} finally { $stream.Dispose() }
Write-Host "Saved manual evidence: $path"
Write-Host 'This record is not a publisher signature or a stable release authorization.'
