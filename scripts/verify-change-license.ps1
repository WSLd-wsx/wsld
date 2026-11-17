param([Parameter(Mandatory=$true)][string]$Directory)
$ErrorActionPreference = 'Stop'
$license = Join-Path $Directory 'core\LICENSE'
$apache = Join-Path $Directory 'core\CHANGE-LICENSE-APACHE-2.0.txt'
foreach ($path in @($license, $apache)) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { throw "Required BSL license material missing: $path" }
}
$expected = 'cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30'
if ((Get-FileHash -LiteralPath $apache -Algorithm SHA256).Hash.ToLowerInvariant() -cne $expected) {
    throw 'Apache-2.0 Change License does not match the official license text'
}
