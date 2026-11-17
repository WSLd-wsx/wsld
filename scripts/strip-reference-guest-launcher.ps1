param([Parameter(Mandatory=$true)][string]$GraphicsDirectory)
$ErrorActionPreference = 'Stop'
# The receipt is retained unchanged for identity; only the reviewed unused
# prototype launcher is excluded from a newly staged product package.
$launchers = @(Get-ChildItem -LiteralPath $GraphicsDirectory -Recurse -File -Filter 'start-guest.py' -Force)
foreach ($launcher in $launchers) {
    $reference = Join-Path $GraphicsDirectory 'reference-442f6032d864'
    $expected = Join-Path $reference 'guest\start-guest.py'
    if (-not [string]::Equals($launcher.FullName, $expected, [StringComparison]::OrdinalIgnoreCase)) { throw 'Unreviewed reference Guest launcher cannot be distributed' }
    $receipt = Join-Path $reference 'verified-files.json'
    if ((Get-FileHash -LiteralPath $receipt -Algorithm SHA256).Hash.ToLowerInvariant() -cne '442f6032d8645c413b7398e2654c669169e79e9b46e79220134bb881b51d0561' -or
        (Get-FileHash -LiteralPath $launcher.FullName -Algorithm SHA256).Hash.ToLowerInvariant() -cne 'ad37b095f13d4da78d610bf635adc54c8a2e062cd9ec0d309bfbe74f509bef85') { throw 'Unreviewed reference Guest launcher identity' }
    Remove-Item -LiteralPath $launcher.FullName
}
