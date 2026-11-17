param([Parameter(Mandatory=$true)][string]$Toolchain,
      [Parameter(Mandatory=$true)][string]$Evidence,
      [Parameter(Mandatory=$true)][string]$Coverage,
      [string]$KernelBuilder)
$ErrorActionPreference = 'Stop'
if (Test-Path -LiteralPath $Evidence) { throw 'Evidence directory must be new' }
New-Item -ItemType Directory -Path $Evidence | Out-Null
$python = Get-Command python.exe -ErrorAction Stop
$report = Join-Path $Evidence 'kernel-coverage.json'
$args = @((Join-Path $PSScriptRoot 'kernel-release-coverage.py'), '--coverage', $Coverage,
          '--admission', (Join-Path $PSScriptRoot '..\core\compatibility\host-updates.json'), '--report', $report)
if ($KernelBuilder) { $args += @('--builder', $KernelBuilder) }
& $python.Source @args
if ($LASTEXITCODE -ne 0) { throw "Kernel coverage incomplete; see $report. Product package was not built." }
$scan = Get-Content -LiteralPath $report -Raw | ConvertFrom-Json
if (-not $scan.release_gate_passed) { throw 'Kernel release gate did not pass' }
$references = @($scan.targets | ForEach-Object {
    $record = Get-Content -LiteralPath (Join-Path $Coverage ("targets\" + $_.tag + '.json')) -Raw | ConvertFrom-Json
    Split-Path -Parent (Join-Path $Coverage $record.receipt)
} | Select-Object -Unique)
if ($references.Count -eq 0) { throw 'No verified graphics component bundles were produced' }
& (Join-Path $PSScriptRoot 'build-msvc.ps1') -Toolchain $Toolchain -Evidence (Join-Path $Evidence 'product') -GraphicsReference $references
