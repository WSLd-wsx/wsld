param([Parameter(Mandatory=$true)][string]$ExecutablePackage,
      [Parameter(Mandatory=$true)][string]$RuntimePackage,
      [Parameter(Mandatory=$true)][string]$GuiNotices,
      [Parameter(Mandatory=$true)][string]$Output)
$ErrorActionPreference = 'Stop'
if (Test-Path -LiteralPath $Output) { throw 'Output already exists' }
& "$PSScriptRoot\verify-change-license.ps1" -Directory (Split-Path $PSScriptRoot -Parent)
if (-not (Test-Path -LiteralPath (Join-Path $GuiNotices 'REVIEW-REQUIRED.txt') -PathType Leaf)) { throw 'GUI dependency inventory is missing' }
foreach ($name in @('MPL-SOURCES.md','mpl-sources\manifest.json','mpl-sources\Cargo.lock','mpl-sources\dependency-tree.txt')) {
    if (-not (Test-Path -LiteralPath (Join-Path $GuiNotices $name) -PathType Leaf)) { throw "MPL dependency source evidence is missing: $name" }
}
& "$PSScriptRoot\verify-native-host-notices.ps1" -RuntimePackage $RuntimePackage
& "$PSScriptRoot\verify-linked-runtime.ps1" -RuntimePackage $RuntimePackage `
    -RuntimeCatalog (Join-Path $ExecutablePackage 'runtime-catalog.json') `
    -GuestAdapterCatalog (Join-Path $ExecutablePackage 'guest-adapter-catalog.json')
New-Item -ItemType Directory -Path $Output | Out-Null
$content = New-Item -ItemType Directory (Join-Path $Output 'content')
foreach ($name in @('wsld.exe','wsx.exe','wsxdesk.exe')) {
    Copy-Item -LiteralPath (Join-Path $ExecutablePackage $name) -Destination (Join-Path $content $name)
}
$hashes = @('wsld.exe','wsx.exe','wsxdesk.exe') | ForEach-Object { (Get-FileHash -LiteralPath (Join-Path $content $_) -Algorithm SHA256).Hash }
if (@($hashes | Select-Object -Unique).Count -ne 1) { throw 'Executable copies have different hashes' }
$repo = Split-Path $PSScriptRoot -Parent
foreach ($name in @('LICENSE','NOTICE','LICENSING.md','THIRD-PARTY-NOTICES.md','licenses')) {
    $source = Join-Path $repo $name
    if (-not (Test-Path -LiteralPath $source)) { throw "Required Core notice material missing: $name" }
    Copy-Item -LiteralPath $source -Destination (Join-Path $content $name) -Recurse
}
foreach ($name in @('integration\LICENSE','integration\graphics-opaque-probe\APACHE-2.0.txt','integration\graphics-opaque-probe\README.md','core\data\UNICODE-LICENSE.txt','core\LICENSE','core\CHANGE-LICENSE-APACHE-2.0.txt')) {
    $source = Join-Path $repo $name
    if (-not (Test-Path -LiteralPath $source -PathType Leaf)) {
        throw "Required Core notice material missing: $name"
    }
    $dest = Join-Path $content $name
    New-Item -ItemType Directory -Force (Split-Path $dest -Parent) | Out-Null
    Copy-Item -LiteralPath $source -Destination $dest
}
& "$PSScriptRoot\verify-change-license.ps1" -Directory $content.FullName
foreach ($name in @('native-host','guest-adapters')) {
    $source = Join-Path $RuntimePackage "licenses\$name"
    if (-not (Test-Path -LiteralPath $source -PathType Container)) { throw "Required runtime notices missing: $name" }
    Copy-Item -LiteralPath $source -Destination (Join-Path $content "licenses\$name") -Recurse
}
foreach ($name in @('graphics','guest-adapters')) {
    $source = Join-Path $RuntimePackage $name
    if (-not (Test-Path -LiteralPath $source)) { throw "Required runtime or notice material missing: $name" }
    Copy-Item -LiteralPath $source -Destination (Join-Path $content $name) -Recurse
}
& (Join-Path $PSScriptRoot 'strip-reference-guest-launcher.ps1') -GraphicsDirectory (Join-Path $content 'graphics')
$kernelModules = Join-Path $RuntimePackage 'kernel-modules'
if (-not (Test-Path -LiteralPath $kernelModules -PathType Container)) { throw 'Versioned kernel modules are required for the community package' }
& (Join-Path $PSScriptRoot 'verify-kernel-assets.ps1') -Directory $kernelModules
Copy-Item -LiteralPath $kernelModules -Destination (Join-Path $content 'kernel-modules') -Recurse
& (Join-Path $PSScriptRoot 'verify-kernel-assets.ps1') -Directory (Join-Path $content 'kernel-modules')
Copy-Item -LiteralPath $GuiNotices -Destination (Join-Path $content 'gui-third-party') -Recurse
Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem
$zipPath = Join-Path $Output 'wsx-community-slice.zip'
$stream = [IO.File]::Open($zipPath, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write)
try {
    $archive = [IO.Compression.ZipArchive]::new($stream, [IO.Compression.ZipArchiveMode]::Create, $true)
    try {
        $prefix = $content.FullName.TrimEnd('\') + '\'
        foreach ($file in Get-ChildItem -LiteralPath $content.FullName -File -Recurse -Force) {
            if (-not $file.FullName.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)) { throw 'ZIP source escapes content directory' }
            if ($file.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'ZIP source contains a reparse point' }
            $name = $file.FullName.Substring($prefix.Length).Replace('\','/')
            $entry = $archive.CreateEntry($name, [IO.Compression.CompressionLevel]::Optimal)
            # Reproducible inputs can predate the ZIP format's 1980 epoch.
            $entry.LastWriteTime = [DateTimeOffset]::new(1980, 1, 1, 0, 0, 0, [TimeSpan]::Zero)
            $inputStream = [IO.File]::OpenRead($file.FullName)
            try { $outputStream = $entry.Open(); try { $inputStream.CopyTo($outputStream) } finally { $outputStream.Dispose() } }
            finally { $inputStream.Dispose() }
        }
    } finally { $archive.Dispose() }
} finally { $stream.Dispose() }
Get-FileHash -LiteralPath (Join-Path $Output 'wsx-community-slice.zip') -Algorithm SHA256 | ConvertTo-Json | Set-Content (Join-Path $Output 'hash.json')
Write-Output 'Diagnostic ZIP created; dependency and license review and product acceptance remain required.'
