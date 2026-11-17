param([Parameter(Mandatory=$true)][string]$RuntimePackage,
      [Parameter(Mandatory=$true)][string]$RuntimeCatalog,
      [Parameter(Mandatory=$true)][string]$GuestAdapterCatalog)
$ErrorActionPreference = 'Stop'
function Verify-Files([string]$Directory, $Files, [string[]]$Names) {
    $actualNames = @($Files.PSObject.Properties.Name | Sort-Object)
    if (($actualNames -join '|') -cne ((@($Names) | Sort-Object) -join '|')) { throw 'Unexpected runtime component set' }
    foreach ($name in $Names) {
        $file = Get-Item -LiteralPath (Join-Path $Directory $name) -Force
        $entry = $Files.$name
        if ($file.PSIsContainer -or ($file.Attributes -band [IO.FileAttributes]::ReparsePoint) -or
            $file.Length -lt 1 -or $file.Length -gt 268435456 -or $file.Length -ne $entry.bytes -or
            (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant() -cne $entry.sha256) {
            throw "Runtime component rejected: $name"
        }
    }
}
function Verify-Manifest([string]$Directory, [string]$Name, [string]$Expected) {
    $dir = Get-Item -LiteralPath $Directory -Force
    if (-not $dir.PSIsContainer -or ($dir.Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'Unsafe runtime directory' }
    $file = Get-Item -LiteralPath (Join-Path $Directory $Name) -Force
    if ($file.PSIsContainer -or ($file.Attributes -band [IO.FileAttributes]::ReparsePoint) -or $file.Length -gt 65536 -or
        (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant() -cne $Expected) { throw 'Runtime manifest mismatch' }
    return Get-Content -LiteralPath $file.FullName -Raw | ConvertFrom-Json
}
$runtime = Get-Content -LiteralPath $RuntimeCatalog -Raw | ConvertFrom-Json
if ($runtime.schema_version -ne 1 -or @($runtime.bundles).Count -ne 2) { throw 'Invalid Native catalog' }
$seen = @{}
foreach ($entry in $runtime.bundles) {
    $mode = $entry.mode
    if ($mode -notin @('zero-copy','staging-copy') -or $seen.ContainsKey($mode) -or $entry.manifest_sha256 -cnotmatch '^[0-9a-f]{64}$') { throw 'Invalid Native catalog entry' }
    $seen[$mode] = $true
    $dir = Join-Path $RuntimePackage "graphics\product\$mode"
    $manifest = Verify-Manifest $dir 'component-manifest.json' $entry.manifest_sha256
    if ($manifest.schema_version -ne 1 -or $manifest.kind -cne 'wsx-native-runtime' -or $manifest.mode -cne $mode -or
        $manifest.peer -cne 'linaro-virtio-msg' -or 'guest-bootstrap-v2' -cnotin $manifest.native_capabilities) { throw 'Invalid Native runtime declaration' }
    Verify-Files $dir $manifest.files @('wsx-native-host.exe','wsx-input-host.exe','libvirglrenderer-1.dll','libepoxy-0.dll')
}
$adapters = Get-Content -LiteralPath $GuestAdapterCatalog -Raw | ConvertFrom-Json
if ($adapters.schema_version -ne 1 -or @($adapters.adapters).Count -ne 2) { throw 'Invalid Guest adapter catalog' }
$seen = @{}
foreach ($entry in $adapters.adapters) {
    $implementation = $entry.implementation
    if ($implementation -notin @('hyprland','plasma') -or $seen.ContainsKey($implementation) -or $entry.manifest_sha256 -cnotmatch '^[0-9a-f]{64}$') { throw 'Invalid Guest adapter catalog entry' }
    $seen[$implementation] = $true
    $dir = Join-Path $RuntimePackage "guest-adapters\$implementation"
    $manifest = Verify-Manifest $dir 'manifest.json' $entry.manifest_sha256
    $files = if ($manifest.schema_version -eq 2) { @('wsx-guest') } else { throw 'Product builds require a native Guest adapter' }
    if (-not (Test-Path -LiteralPath (Join-Path $RuntimePackage 'licenses\guest-adapters\rust-dependencies\DEPENDENCIES.txt') -PathType Leaf)) { throw 'Rust Guest dependency notices are missing' }
    if ($manifest.kind -cne 'wsx-guest-adapter' -or $manifest.protocol_major -ne 2 -or $manifest.implementation -cne $implementation) { throw 'Invalid Guest adapter declaration' }
    Verify-Files $dir $manifest.files $files
}
Write-Output 'Native and Guest runtime catalogs and component bytes verified'
