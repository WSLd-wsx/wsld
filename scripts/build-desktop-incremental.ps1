param([Parameter(Mandatory=$true)][string]$Toolchain,[Parameter(Mandatory=$true)][string]$Evidence,
      [Parameter(Mandatory=$true)][string[]]$GraphicsReference)
$ErrorActionPreference='Stop'
& "$PSScriptRoot\check-workspace.ps1" -RepositoryRoot (Split-Path $PSScriptRoot -Parent)
if(Test-Path -LiteralPath $Evidence){throw 'Evidence directory must be new'}
New-Item -ItemType Directory -Path $Evidence | Out-Null
Start-Transcript -Path (Join-Path $Evidence 'build.log') | Out-Null
try {
 . "$PSScriptRoot\msvc-env.ps1" -Toolchain $Toolchain
 Set-Location $RepoRoot
 Invoke-WsxNative $Cargo @('test','--locked','--offline','--target','x86_64-pc-windows-msvc')
 Invoke-WsxNative $Cargo @('build','--locked','--offline','--target','x86_64-pc-windows-msvc')
 $package=New-Item -ItemType Directory -Path (Join-Path $Evidence 'package')
 $opaqueNotices=Join-Path $package 'integration\graphics-opaque-probe'
 New-Item -ItemType Directory -Force $opaqueNotices | Out-Null
 Copy-Item (Join-Path $RepoRoot 'integration\graphics-opaque-probe\APACHE-2.0.txt') $opaqueNotices
 Copy-Item (Join-Path $RepoRoot 'integration\graphics-opaque-probe\README.md') $opaqueNotices
 foreach($name in @('wsld','wsx','wsxdesk')){Copy-Item (Join-Path $env:CARGO_TARGET_DIR 'x86_64-pc-windows-msvc\debug\wsx-host.exe') (Join-Path $package "$name.exe")}
 foreach($reference in $GraphicsReference){& "$PSScriptRoot\package-graphics.ps1" -Package $package -Reference $reference}
 $hashes=@(Get-FileHash (Join-Path $package '*.exe') -Algorithm SHA256)
 if(@($hashes.Hash | Select-Object -Unique).Count -ne 1){throw 'Single PE hash contract failed'}
 $hashes | ConvertTo-Json | Set-Content (Join-Path $Evidence 'hashes.json')
} finally {Stop-Transcript | Out-Null}
