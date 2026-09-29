# Parity harness (P0) -- differential gate for the Rust migration.
#
# WHY THIS EXISTS:
#   docs/plan-rust-migration.md replaces the Go generator with a Rust one. A byte-exact
#   comparison against a frozen reference is the safety net that keeps the two
#   implementations from drifting -- i.e. it turns "two unguarded copies" into a
#   *guarded mirror* instead of a second source of truth.
#
# USAGE:
#   pwsh -File tools/parity/run_parity.ps1                 # check bin/settings.exe vs reference
#   pwsh -File tools/parity/run_parity.ps1 -Exe <path>     # check a specific implementation
#   pwsh -File tools/parity/run_parity.ps1 -Capture        # (re)record reference from -Exe
#
# CONVENTION:
#   ASCII-only on purpose -- `pwsh -File` misreads non-BOM UTF-8 (same rule as tools/oracle.ps1).
#   Exit 0 = all pass, 1 = any mismatch/error. Final line is ASCII: "PARITY: n/n PASS [MODE]".
#
# NOTE: -Capture refuses to record when an item is not byte-deterministic across two runs
#   (the Go renderer has known map-iteration nondeterminism for configs with ties; the corpus
#   must stay free of those, cf. golden_test.go "determinism constraints").

param(
  [string]$Exe = '',
  [switch]$Capture
)

$ErrorActionPreference = 'Stop'

$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$repo = Split-Path -Parent (Split-Path -Parent $here)
if ([string]::IsNullOrEmpty($Exe)) { $Exe = Join-Path $repo 'bin\settings.exe' }
if (!(Test-Path $Exe)) { Write-Host "PARITY: 0/0 FAIL [exe not found: $Exe]"; exit 1 }

$manifest = Get-Content -Raw -Encoding UTF8 (Join-Path $here 'manifest.json') | ConvertFrom-Json
$template = Join-Path $repo $manifest.template
$refDir = Join-Path $here 'reference'
$work = Join-Path $env:TEMP 'kfparity'
if (Test-Path $work) { Remove-Item -Recurse -Force $work }
New-Item -ItemType Directory -Force -Path $work | Out-Null

function Get-Sha256([string]$p) { (Get-FileHash -Algorithm SHA256 -Path $p).Hash }

$total = 0
$pass = 0
$fail = @()

Push-Location $repo
try {
  foreach ($item in $manifest.items) {
    $name = $item.name
    $total++
    $itemWork = Join-Path $work $name
    New-Item -ItemType Directory -Force -Path $itemWork | Out-Null
    Copy-Item (Join-Path $here $item.config) (Join-Path $itemWork 'config.json') -Force
    if (($item.PSObject.Properties.Name -contains 'plugins') -and $item.plugins) {
      $dst = Join-Path $itemWork 'plugins'
      New-Item -ItemType Directory -Force -Path $dst | Out-Null
      Copy-Item (Join-Path (Join-Path $repo $item.plugins) '*') $dst -Recurse -Force
    }

    $cfg = Join-Path $itemWork 'config.json'
    $planOut = Join-Path $itemWork 'plan.json'
    $ahkOut = Join-Path $itemWork 'keyflux.ahk'

    $ok = $true
    & $Exe DumpPlan $cfg $planOut | Out-Null
    if ($LASTEXITCODE -ne 0) { $fail += ("$name : DumpPlan exit " + $LASTEXITCODE); $ok = $false }
    & $Exe GenerateAHK $cfg $template $ahkOut | Out-Null
    if ($LASTEXITCODE -ne 0) { $fail += ("$name : GenerateAHK exit " + $LASTEXITCODE); $ok = $false }
    if (!$ok) { continue }

    if ($Capture) {
      $plan2 = Join-Path $itemWork 'plan2.json'
      $ahk2 = Join-Path $itemWork 'keyflux2.ahk'
      & $Exe DumpPlan $cfg $plan2 | Out-Null
      & $Exe GenerateAHK $cfg $template $ahk2 | Out-Null
      if ((Get-Sha256 $planOut) -ne (Get-Sha256 $plan2) -or (Get-Sha256 $ahkOut) -ne (Get-Sha256 $ahk2)) {
        $fail += "$name : NONDETERMINISTIC (reference not recorded)"
        continue
      }
      New-Item -ItemType Directory -Force -Path $refDir | Out-Null
      Copy-Item $planOut (Join-Path $refDir "$name.plan.json") -Force
      Copy-Item $ahkOut (Join-Path $refDir "$name.keyflux.ahk") -Force
      $pass++
    }
    else {
      $refPlan = Join-Path $refDir "$name.plan.json"
      $refAhk = Join-Path $refDir "$name.keyflux.ahk"
      if (!(Test-Path $refPlan) -or !(Test-Path $refAhk)) { $fail += "$name : reference missing"; continue }
      $bad = @()
      if ((Get-Sha256 $planOut) -ne (Get-Sha256 $refPlan)) { $bad += 'plan' }
      if ((Get-Sha256 $ahkOut) -ne (Get-Sha256 $refAhk)) { $bad += 'ahk' }
      if ($bad.Count -eq 0) { $pass++ } else { $fail += ("$name : MISMATCH [" + ($bad -join ',') + "]") }
    }
  }
}
finally { Pop-Location }

$mode = if ($Capture) { 'CAPTURE' } else { 'CHECK' }
if ($fail.Count -gt 0) {
  $fail | ForEach-Object { Write-Host "  - $_" }
  Write-Host "PARITY: $pass/$total PASS [$mode] FAIL"
  exit 1
}
Write-Host "PARITY: $pass/$total PASS [$mode]"
exit 0
