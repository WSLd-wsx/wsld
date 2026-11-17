param([Parameter(Mandatory=$true)][string]$RepositoryRoot)
$repository = (Resolve-Path -LiteralPath $RepositoryRoot).Path
if (-not (Test-Path -LiteralPath (Join-Path $repository '.git'))) {
    throw "WSX Core repository is missing: $repository"
}
