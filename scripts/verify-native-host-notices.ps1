param([Parameter(Mandatory=$true)][string]$RuntimePackage)
$ErrorActionPreference = 'Stop'
$notices = Join-Path $RuntimePackage 'licenses/native-host'
foreach ($name in @('linaro-apache-2.0.txt','linaro-attribution.txt','virglrenderer-copying.txt',
                    'virglrenderer-attribution.txt','rutabaga-license.txt','openvmm-license.txt',
                    'libepoxy-copying.txt','host-rust-dependencies.tar.gz')) {
    $path = Join-Path $notices $name
    if (-not (Test-Path -LiteralPath $path -PathType Leaf) -or (Get-Item -LiteralPath $path).Length -le 0) {
        throw "Native Host license material missing or empty: $name"
    }
}
