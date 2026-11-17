param([string]$Toolchain, [Parameter(Mandatory=$true)][string]$Evidence, [switch]$Acceptance,
      [Parameter(Mandatory=$true)][string[]]$GraphicsReference,
      [string]$GuestAdapterCatalog, [string]$HyprlandAdapter, [string]$PlasmaAdapter,
      [string]$GuestAdapterNotices, [string]$GraphicsProductDirectory, [string]$GraphicsRuntimeCatalog,
      [switch]$DiagnosticSimulateOpaqueUnavailable)
$ErrorActionPreference = 'Stop'
& "$PSScriptRoot\check-workspace.ps1" -RepositoryRoot (Split-Path $PSScriptRoot -Parent)
if (Test-Path $Evidence) { throw "Evidence exists: $Evidence" }
New-Item -ItemType Directory $Evidence | Out-Null
Start-Transcript -Path (Join-Path $Evidence 'build.log') | Out-Null
try {
    . "$PSScriptRoot\msvc-env.ps1" -Toolchain $Toolchain
    Set-Location $RepoRoot
    $guestBundles = @{}
    $graphicsProduct = @{}
    if ($GuestAdapterCatalog) {
        if (-not $GraphicsProductDirectory -or -not $GraphicsRuntimeCatalog) { throw 'Guest v2 requires two catalogued Native runtime bundles' }
        $runtimeCatalogPath = (Resolve-Path -LiteralPath $GraphicsRuntimeCatalog).ProviderPath
        $runtimeCatalog = Get-Content -LiteralPath $runtimeCatalogPath -Raw | ConvertFrom-Json
        if ($runtimeCatalog.schema_version -ne 1 -or @($runtimeCatalog.bundles).Count -ne 2) { throw 'Invalid Native runtime catalog' }
        $graphicsRoot = (Resolve-Path -LiteralPath $GraphicsProductDirectory).ProviderPath
        foreach ($entry in $runtimeCatalog.bundles) {
            $mode = $entry.mode
            if ($mode -notin @('zero-copy','staging-copy') -or $graphicsProduct.ContainsKey($mode) -or $entry.manifest_sha256 -cnotmatch '^[0-9a-f]{64}$') { throw 'Invalid Native runtime catalog entry' }
            $source = Join-Path $graphicsRoot $mode
            $sourceItem = Get-Item -LiteralPath $source -Force
            if (-not $sourceItem.PSIsContainer -or ($sourceItem.Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw "Unsafe Native runtime directory: $mode" }
            $manifest = Join-Path $source 'component-manifest.json'
            $manifestItem = Get-Item -LiteralPath $manifest -Force
            if ($manifestItem.PSIsContainer -or ($manifestItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -or $manifestItem.Length -gt 65536 -or (Get-FileHash -LiteralPath $manifest -Algorithm SHA256).Hash.ToLowerInvariant() -cne $entry.manifest_sha256) { throw "Native runtime manifest rejected: $mode" }
            $declaration = Get-Content -LiteralPath $manifest -Raw | ConvertFrom-Json
            $expectedFiles = @('wsx-native-host.exe','libvirglrenderer-1.dll','wsx-input-host.exe','libepoxy-0.dll')
            if ($declaration.schema_version -ne 1 -or $declaration.kind -cne 'wsx-native-runtime' -or $declaration.mode -cne $mode -or $declaration.peer -cne 'linaro-virtio-msg' -or 'guest-bootstrap-v2' -cnotin $declaration.native_capabilities -or @($declaration.files.PSObject.Properties).Count -ne $expectedFiles.Count) { throw "Native runtime does not declare the required Guest v2 capability: $mode" }
            foreach ($file in $expectedFiles) {
                $record = $declaration.files.$file
                $payload = Get-Item -LiteralPath (Join-Path $source $file) -Force
                if ($null -eq $record -or $payload.PSIsContainer -or ($payload.Attributes -band [IO.FileAttributes]::ReparsePoint) -or $payload.Length -lt 1 -or $payload.Length -gt 268435456 -or $payload.Length -ne $record.bytes -or (Get-FileHash -LiteralPath $payload.FullName -Algorithm SHA256).Hash.ToLowerInvariant() -cne $record.sha256) { throw "Native runtime payload rejected: $mode/$file" }
            }
            $graphicsProduct[$mode] = $source
        }
        $env:WSX_RUNTIME_CATALOG = $runtimeCatalogPath
        if (-not $GuestAdapterNotices) { throw 'Guest adapter license and provenance notices are required' }
        $GuestAdapterNotices = (Resolve-Path -LiteralPath $GuestAdapterNotices).ProviderPath
        $noticeDirectory = Get-Item -LiteralPath $GuestAdapterNotices -Force
        if (-not $noticeDirectory.PSIsContainer -or ($noticeDirectory.Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'Unsafe Guest adapter notices directory' }
        foreach ($file in @('LICENSE-MAP.md','OUTPUT-LICENSES.md','WSX-MIT.txt')) {
            $notice = Get-Item -LiteralPath (Join-Path $GuestAdapterNotices $file) -Force
            if ($notice.PSIsContainer -or ($notice.Attributes -band [IO.FileAttributes]::ReparsePoint) -or $notice.Length -lt 1 -or $notice.Length -gt 1048576) { throw "Invalid Guest adapter notice: $file" }
        }
        $catalogPath = (Resolve-Path -LiteralPath $GuestAdapterCatalog).ProviderPath
        $catalog = Get-Content -LiteralPath $catalogPath -Raw | ConvertFrom-Json
        if ($catalog.schema_version -ne 1 -or @($catalog.adapters).Count -eq 0 -or @($catalog.adapters).Count -gt 2) { throw 'Invalid Guest adapter catalog' }
        foreach ($entry in $catalog.adapters) {
            $name = $entry.implementation
            if ($name -notin @('hyprland','plasma') -or $guestBundles.ContainsKey($name) -or $entry.manifest_sha256 -cnotmatch '^[0-9a-f]{64}$') { throw 'Invalid Guest adapter catalog entry' }
            $source = if ($name -eq 'hyprland') { $HyprlandAdapter } else { $PlasmaAdapter }
            if (-not $source) { throw "Missing Guest adapter bundle: $name" }
            $source = (Resolve-Path -LiteralPath $source).ProviderPath
            $item = Get-Item -LiteralPath $source -Force
            if (-not $item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw "Unsafe Guest adapter directory: $name" }
            $manifest = Join-Path $source 'manifest.json'
            $item = Get-Item -LiteralPath $manifest -Force
            if ($item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -or $item.Length -gt 65536 -or (Get-FileHash -LiteralPath $manifest -Algorithm SHA256).Hash.ToLowerInvariant() -cne $entry.manifest_sha256) { throw "Guest adapter manifest rejected: $name" }
            $declaration = Get-Content -LiteralPath $manifest -Raw | ConvertFrom-Json
            $expectedFiles = if ($declaration.schema_version -eq 2) { @('wsx-guest') } else { throw "Product builds require a native Guest adapter: $name" }
            if ($declaration.kind -cne 'wsx-guest-adapter' -or $declaration.protocol_major -ne 2 -or $declaration.implementation -cne $name -or @($declaration.files.PSObject.Properties).Count -ne $expectedFiles.Count) { throw "Invalid Guest adapter declaration: $name" }
            foreach ($file in $expectedFiles) {
                $record = $declaration.files.$file
                $payload = Join-Path $source $file
                $payloadItem = Get-Item -LiteralPath $payload -Force
                if ($null -eq $record -or $payloadItem.PSIsContainer -or ($payloadItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -or $payloadItem.Length -lt 1 -or $payloadItem.Length -gt 16777216 -or $payloadItem.Length -ne $record.bytes -or (Get-FileHash -LiteralPath $payload -Algorithm SHA256).Hash.ToLowerInvariant() -cne $record.sha256) { throw "Guest adapter payload rejected: $name/$file" }
            }
            $guestBundles[$name] = $source
        }
        if (($HyprlandAdapter -and -not $guestBundles.ContainsKey('hyprland')) -or ($PlasmaAdapter -and -not $guestBundles.ContainsKey('plasma'))) { throw 'Adapter bundle is absent from the catalog' }
        $env:WSX_GUEST_ADAPTER_CATALOG = $catalogPath
    } elseif ($HyprlandAdapter -or $PlasmaAdapter -or $GuestAdapterNotices -or $env:WSX_GUEST_ADAPTER_CATALOG) {
        throw 'Guest adapter bundles require an explicit catalog build argument'
    }
    $licenseInventory = Get-Content 'licenses\dependency-inventory.json' -Raw | ConvertFrom-Json
    if ((Get-FileHash 'Cargo.lock' -Algorithm SHA256).Hash -ne $licenseInventory.cargo_lock_sha256) {
        throw 'Cargo.lock changed; review dependency licenses and refresh the notice inventory'
    }
    $licenseFiles = @($licenseInventory.project_license, $licenseInventory.core_license, $licenseInventory.core_change_license) + @($licenseInventory.packages | ForEach-Object { $_.notices })
    $licenseFiles += @($licenseInventory.bundled_notices) + @($licenseInventory.bundled_data)
    foreach ($entry in $licenseFiles) {
        if ((Get-FileHash (Join-Path $RepoRoot $entry.path) -Algorithm SHA256).Hash -ne $entry.sha256) {
            throw "License notice hash mismatch: $($entry.path)"
        }
    }
    Invoke-WsxNative $env:RUSTC @('-vV')
    $diagnosticFeatures = @()
    if ($DiagnosticSimulateOpaqueUnavailable) {
        Write-Warning 'Diagnostic build: opaque capability is simulated as unavailable; do not distribute.'
        $diagnosticFeatures = @('--features','wsx-host/simulate-opaque-unavailable')
    }
    Invoke-WsxNative $Cargo (@('test','--locked','--target','x86_64-pc-windows-msvc') + $diagnosticFeatures)
    Invoke-WsxNative $Cargo (@('build','--locked','--target','x86_64-pc-windows-msvc') + $diagnosticFeatures)
    $bin = Join-Path $env:CARGO_TARGET_DIR 'x86_64-pc-windows-msvc\debug'
    $package = New-Item -ItemType Directory (Join-Path $Evidence 'package')
    foreach ($name in @('LICENSE','NOTICE','LICENSING.md','THIRD-PARTY-NOTICES.md','licenses')) {
        Copy-Item (Join-Path $RepoRoot $name) $package -Recurse
    }
    foreach ($name in @('core\LICENSE','core\CHANGE-LICENSE-APACHE-2.0.txt','core\data\UNICODE-LICENSE.txt','integration\LICENSE')) {
        $destination = Join-Path $package $name
        New-Item -ItemType Directory -Force (Split-Path $destination -Parent) | Out-Null
        Copy-Item (Join-Path $RepoRoot $name) $destination
    }
    & "$PSScriptRoot\verify-change-license.ps1" -Directory $package.FullName
    $opaqueNotices = Join-Path $package 'integration\graphics-opaque-probe'
    New-Item -ItemType Directory -Force $opaqueNotices | Out-Null
    Copy-Item (Join-Path $RepoRoot 'integration\graphics-opaque-probe\APACHE-2.0.txt') $opaqueNotices
    Copy-Item (Join-Path $RepoRoot 'integration\graphics-opaque-probe\README.md') $opaqueNotices
    foreach ($name in @('wsx','wsld','wsxdesk')) { Copy-Item "$bin\wsx-host.exe" "$package\$name.exe" }
    foreach ($name in $guestBundles.Keys) {
        $destination = New-Item -ItemType Directory -Force (Join-Path $package "guest-adapters\$name")
        $sourceDeclaration = Get-Content -LiteralPath (Join-Path $guestBundles[$name] 'manifest.json') -Raw | ConvertFrom-Json
        $expectedFiles = if ($sourceDeclaration.schema_version -eq 2) { @('wsx-guest') } else { throw "Product builds require a native Guest adapter: $name" }
        foreach ($file in @('manifest.json') + $expectedFiles) {
            $source = Join-Path $guestBundles[$name] $file
            $item = Get-Item -LiteralPath $source -Force
            if ($item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -or $item.Length -gt 16777216) { throw "Unsafe Guest adapter file: $name/$file" }
            Copy-Item -LiteralPath $source -Destination (Join-Path $destination $file)
        }
        $pin = @($catalog.adapters | Where-Object { $_.implementation -ceq $name })[0].manifest_sha256
        $manifest = Join-Path $destination 'manifest.json'
        if ((Get-FileHash -LiteralPath $manifest -Algorithm SHA256).Hash.ToLowerInvariant() -cne $pin) { throw "Staged Guest manifest changed: $name" }
        $declaration = Get-Content -LiteralPath $manifest -Raw | ConvertFrom-Json
        foreach ($file in $expectedFiles) {
            $payload = Get-Item -LiteralPath (Join-Path $destination $file) -Force
            $record = $declaration.files.$file
            if ($payload.Length -ne $record.bytes -or (Get-FileHash -LiteralPath $payload.FullName -Algorithm SHA256).Hash.ToLowerInvariant() -cne $record.sha256) { throw "Staged Guest payload changed: $name/$file" }
        }
    }
    if ($GuestAdapterCatalog) {
        $notices = New-Item -ItemType Directory -Force (Join-Path $package 'licenses\guest-adapters')
        foreach ($file in @('LICENSE-MAP.md','OUTPUT-LICENSES.md','WSX-MIT.txt')) {
            Copy-Item -LiteralPath (Join-Path $GuestAdapterNotices $file) -Destination (Join-Path $notices $file)
        }
        $rustNotices = Join-Path $GuestAdapterNotices 'rust-dependencies'
        if (-not (Test-Path -LiteralPath (Join-Path $rustNotices 'DEPENDENCIES.txt') -PathType Leaf)) { throw 'Rust Guest dependency notices are missing' }
        $noticeItems = @(Get-Item -LiteralPath $rustNotices -Force) + @(Get-ChildItem -LiteralPath $rustNotices -Recurse -Force)
        foreach ($item in $noticeItems) {
            if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Guest dependency notices contain a reparse point' }
        }
        Copy-Item -LiteralPath $rustNotices -Destination (Join-Path $notices 'rust-dependencies') -Recurse
        foreach ($mode in $graphicsProduct.Keys) {
            $destination = New-Item -ItemType Directory -Force (Join-Path $package "graphics\product\$mode")
            foreach ($file in @('component-manifest.json','wsx-native-host.exe','libvirglrenderer-1.dll','wsx-input-host.exe','libepoxy-0.dll')) {
                Copy-Item -LiteralPath (Join-Path $graphicsProduct[$mode] $file) -Destination (Join-Path $destination $file)
            }
            $pin = @($runtimeCatalog.bundles | Where-Object { $_.mode -ceq $mode })[0].manifest_sha256
            $manifest = Join-Path $destination 'component-manifest.json'
            if ((Get-FileHash -LiteralPath $manifest -Algorithm SHA256).Hash.ToLowerInvariant() -cne $pin) { throw "Staged Native manifest changed: $mode" }
            $declaration = Get-Content -LiteralPath $manifest -Raw | ConvertFrom-Json
            foreach ($file in @('wsx-native-host.exe','libvirglrenderer-1.dll','wsx-input-host.exe','libepoxy-0.dll')) {
                $payload = Get-Item -LiteralPath (Join-Path $destination $file) -Force
                $record = $declaration.files.$file
                if ($payload.Length -ne $record.bytes -or (Get-FileHash -LiteralPath $payload.FullName -Algorithm SHA256).Hash.ToLowerInvariant() -cne $record.sha256) { throw "Staged Native payload changed: $mode/$file" }
            }
        }
    }
    foreach ($reference in $GraphicsReference) {
        & "$PSScriptRoot\package-graphics.ps1" -Package $package -Reference $reference
    }
    $hashes = @(Get-FileHash "$package\*.exe" -Algorithm SHA256)
    if (@($hashes.Hash | Select-Object -Unique).Count -ne 1) { throw 'Single PE hash contract failed' }
    $hashes | ConvertTo-Json | Set-Content (Join-Path $Evidence 'hashes.json')
    Invoke-WsxNative "$vc\bin\Hostx64\x64\dumpbin.exe" @('/headers','/imports',"$package\wsx.exe")
    $imports = (Invoke-WsxNative "$vc\bin\Hostx64\x64\dumpbin.exe" @('/dependents',"$package\wsx.exe")) -join "`n"
    if ($imports -match '(?im)^\s*(wsxpro|WebView2Loader|embeddedbrowserwebview)\.dll\s*$') { throw 'Public PE imports a private GUI dependency' }
    $fixtures = New-Item -ItemType Directory (Join-Path $Evidence 'fixtures')
    Invoke-WsxNative $env:CC @('/nologo','/std:c11',"$RepoRoot\tests\layout.c","/Fe:$fixtures\layout.exe","/Fo:$fixtures\layout.obj")
    Invoke-WsxNative "$fixtures\layout.exe" @()
    Invoke-WsxNative $env:CC @('/nologo',"$RepoRoot\tests\argv0-probe.c","/Fe:$fixtures\argv0-probe.exe","/Fo:$fixtures\argv0-probe.obj")
    foreach ($case in @('valid','bad-abi','missing-export','fail-init')) {
        $args=@('/nologo','/LD',"$RepoRoot\tests\pro-fixture.c","/Fe:$fixtures\$case.dll","/Fo:$fixtures\$case.obj",'/link',"/IMPLIB:$fixtures\$case.lib")
        if ($case -ne 'valid') { $args=@('/D'+$case.Replace('-','_').ToUpperInvariant())+$args }
        Invoke-WsxNative $env:CC $args
    }
    $env:LIB = "$vc\lib\x86;$sdk\Lib\$sdkVersion\um\x86;$sdk\Lib\$sdkVersion\ucrt\x86"
    Invoke-WsxNative "$vc\bin\Hostx64\x86\cl.exe" @('/nologo','/LD',"$RepoRoot\tests\wrong-arch.c","/Fe:$fixtures\wrong-arch.dll","/Fo:$fixtures\wrong-arch.obj",'/link',"/IMPLIB:$fixtures\wrong-arch.lib")
} finally { Stop-Transcript | Out-Null }
