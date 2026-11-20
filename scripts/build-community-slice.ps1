param([string]$Toolchain, [Parameter(Mandatory=$true)][string]$Evidence,
      [Parameter(Mandatory=$true)][string]$RuntimePackage,
      [Parameter(Mandatory=$true)][string]$RuntimeCatalog,
      [Parameter(Mandatory=$true)][string]$GuestAdapterCatalog)
$ErrorActionPreference = 'Stop'
if (Test-Path -LiteralPath $Evidence) { throw 'Evidence destination already exists' }
New-Item -ItemType Directory -Path $Evidence | Out-Null
. "$PSScriptRoot\msvc-env.ps1" -Toolchain $Toolchain
& "$PSScriptRoot\verify-linked-runtime.ps1" -RuntimePackage $RuntimePackage -RuntimeCatalog $RuntimeCatalog -GuestAdapterCatalog $GuestAdapterCatalog
$env:WSX_RUNTIME_CATALOG = (Resolve-Path -LiteralPath $RuntimeCatalog).ProviderPath
$env:WSX_GUEST_ADAPTER_CATALOG = (Resolve-Path -LiteralPath $GuestAdapterCatalog).ProviderPath
Invoke-WsxNative $Cargo @('test','--locked','--target','x86_64-pc-windows-msvc','--manifest-path',"$RepoRoot\community\Cargo.toml",'--lib')
Invoke-WsxNative $Cargo @('build','--release','--locked','--target','x86_64-pc-windows-msvc','--manifest-path',"$RepoRoot\community\Cargo.toml")
$source = Join-Path $env:CARGO_TARGET_DIR 'x86_64-pc-windows-msvc\release\wsx-community-slice.exe'
$package = New-Item -ItemType Directory (Join-Path $Evidence 'package')
foreach ($name in @('wsld.exe','wsx.exe','wsldesk.exe')) { Copy-Item -LiteralPath $source -Destination (Join-Path $package $name) }
Copy-Item -LiteralPath $RuntimeCatalog -Destination (Join-Path $package 'runtime-catalog.json')
Copy-Item -LiteralPath $GuestAdapterCatalog -Destination (Join-Path $package 'guest-adapter-catalog.json')
$hashes = @(Get-ChildItem $package -Filter '*.exe' | Get-FileHash -Algorithm SHA256)
if (@($hashes.Hash | Select-Object -Unique).Count -ne 1) { throw 'Executable copies have different hashes' }
$hashes | ConvertTo-Json | Set-Content (Join-Path $Evidence 'hashes.json')
$imports = Invoke-WsxNative "$vc\bin\Hostx64\x64\dumpbin.exe" @('/imports',(Join-Path $package 'wsld.exe'))
$imports | Set-Content (Join-Path $Evidence 'imports.txt')
if (($imports -join "`n") -match '(?i)wsxpro\.dll') { throw 'Community executable imports the old Pro DLL' }
$tree = Join-Path $Evidence 'dependency-tree.txt'
$dependencyTree = Invoke-WsxNative $Cargo @('tree','--locked','--target','x86_64-pc-windows-msvc','--manifest-path',"$RepoRoot\community\Cargo.toml",'-e','normal','--prefix','none','--format','{p}|{l}')
$dependencyTree | Set-Content -Encoding UTF8 $tree
$registries = @(Get-ChildItem (Join-Path $env:CARGO_HOME 'registry\src') -Directory | Where-Object { Test-Path (Join-Path $_.FullName 'tauri-2.12.5') })
if ($registries.Count -ne 1) { throw 'Cannot uniquely locate dependency sources for notices' }
& "$PSScriptRoot\collect-gui-notices.ps1" -DependencyTree $tree -CargoRegistry $registries[0].FullName -Output (Join-Path $Evidence 'gui-notices')
# Record import names from the exact PE; this is evidence, not a complete
# inventory of statically linked or dynamically loaded components.
& python "$PSScriptRoot\inspect-pe.py" (Join-Path $package 'wsld.exe') | Set-Content -Encoding utf8 (Join-Path $Evidence 'gui-notices\community-pe-imports.json')
if ($LASTEXITCODE -ne 0) { throw 'Cannot record community PE imports' }
$mplTree = Join-Path $Evidence 'mpl-dependency-tree.txt'
$mplDependencyTree = Invoke-WsxNative $Cargo @('tree','--locked','--target','x86_64-pc-windows-msvc','--manifest-path',"$RepoRoot\community\Cargo.toml",'-e','normal,build','--prefix','none','--format','{p}|{l}')
$mplDependencyTree | Set-Content -Encoding UTF8 $mplTree
$registryCache = Join-Path $env:CARGO_HOME ("registry\cache\" + $registries[0].Name)
& python "$PSScriptRoot\community-mpl-sources.py" collect --lock "$RepoRoot\community\Cargo.lock" --tree $mplTree `
    --registry-src $registries[0].FullName --registry-cache $registryCache --output (Join-Path $Evidence 'gui-notices\mpl-sources')
if ($LASTEXITCODE -ne 0) { throw 'Cannot collect MPL dependency sources' }
Write-Output 'Community integration slice built; product packaging and runtime acceptance remain separate.'
