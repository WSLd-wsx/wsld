param([Parameter(Mandatory=$true)][string]$DependencyTree,
      [Parameter(Mandatory=$true)][string]$CargoRegistry,
      [Parameter(Mandatory=$true)][string]$Output)
$ErrorActionPreference = 'Stop'
if (Test-Path -LiteralPath $Output) { throw 'Notice destination already exists' }
New-Item -ItemType Directory -Path $Output | Out-Null
$seen = @{}
$missing = [System.Collections.Generic.List[string]]::new()
foreach ($line in Get-Content -LiteralPath $DependencyTree) {
    if ($line -notmatch '^([\w-]+) v([\w.+-]+).*?\|(.+?)( \(\*\))?$') { continue }
    $name = $Matches[1]; $version = $Matches[2]; $license = $Matches[3]
    if ($name -like 'wsx-*') { continue }
    $key = "$name-$version"
    if ($seen.ContainsKey($key)) { continue }
    $seen[$key] = $true
    $source = Join-Path $CargoRegistry $key
    if (-not (Test-Path -LiteralPath $source -PathType Container)) { throw "Dependency source is missing: $key" }
    $destination = New-Item -ItemType Directory (Join-Path $Output $key)
    $notices = @(Get-ChildItem -LiteralPath $source -File | Where-Object { $_.Name -match '^(?i:LICEN[CS]E|COPYING|NOTICE)' })
    foreach ($notice in $notices) { Copy-Item -LiteralPath $notice.FullName -Destination $destination }
    $vcs = Join-Path $source '.cargo_vcs_info.json'
    if (Test-Path -LiteralPath $vcs) { Copy-Item -LiteralPath $vcs -Destination (Join-Path $destination 'cargo-vcs-info.json') }
    if (-not $notices.Count -and (Test-Path -LiteralPath $vcs)) {
        $commit = (Get-Content -LiteralPath $vcs -Raw | ConvertFrom-Json).git.sha1
        $repo = Split-Path $PSScriptRoot -Parent
        $fallback = $null; $expected = $null
        if (($key -eq 'webview2-com-0.39.1' -or $key -eq 'webview2-com-sys-0.39.1') -and $commit -eq 'edc2caf886175ccaebe86078c9cfe1ae2a187328') {
            $fallback = Join-Path $repo 'gui\upstream-notices\webview2-rs-edc2caf\LICENSE'
            $expected = '0dcf41516e608bbcb6cdc5229feb7b86fe4a643b85e7df251133c93408fdac73'
        } elseif ($key -eq 'webview2-com-macros-0.8.1' -and $commit -eq 'dffa41a8a46d3f5565eefbff2de57d38d399f158') {
            $fallback = Join-Path $repo 'gui\upstream-notices\webview2-rs-dffa41a\LICENSE'
            $expected = '0dcf41516e608bbcb6cdc5229feb7b86fe4a643b85e7df251133c93408fdac73'
        } elseif ($key -eq 'selectors-0.38.0' -and $commit -eq '572ecba2d1600e7c3d490586692a209faf703baa') {
            $fallback = Join-Path $CargoRegistry 'cssparser-0.37.0\LICENSE'
            $expected = 'fab3dd6bdab226f1c08630b1dd917e11fcb4ec5e1e020e2c16f83a0a13863e85'
        } elseif ($key -eq 'ruzstd-0.8.2' -and $commit -eq 'fcc40ba1bc766ba38eca852cb64550c827d7068d') {
            $fallback = Join-Path $repo 'licenses\dependencies\ruzstd-0.8.2\LICENSE'
            $expected = 'd715eb0d9cfe1cc0cc22577e387436aa568b31ca272a7c837004dc0cdde925b5'
        }
        if ($fallback) {
            if ((Get-FileHash -LiteralPath $fallback -Algorithm SHA256).Hash.ToLowerInvariant() -cne $expected) { throw "Pinned license mismatch: $key" }
            Copy-Item -LiteralPath $fallback -Destination (Join-Path $destination 'LICENSE-upstream')
            $notices = @($fallback)
        }
    }
    @("Package: $key", "Cargo license expression: $license") | Set-Content -Encoding UTF8 (Join-Path $destination 'METADATA.txt')
    if (-not $notices.Count) { $missing.Add("$key | $license") }
}
if (-not $seen.Count) { throw 'No dependency notices were enumerated' }
@("Collected packages: $($seen.Count)", "Packages without local notice text: $($missing.Count)",
  'This inventory does not establish license compliance or corresponding-source obligations.') + $missing |
    Set-Content -Encoding UTF8 (Join-Path $Output 'REVIEW-REQUIRED.txt')
