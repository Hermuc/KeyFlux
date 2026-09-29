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
# NOTE 1: -Capture refuses to record when an artifact is not byte-deterministic across two runs
#   (the Go renderer has known map-iteration nondeterminism for configs with ties; the corpus
#   must stay free of those, cf. golden_test.go "determinism constraints").
#
# NOTE 2: artifacts per item are declared in manifest.json ("artifacts"). Both outputs of the
#   runtime generation pipeline are covered: bin/KeyFlux.ahk (keyflux.tmpl) and
#   bin/CommandInputSkin.txt (CommandInputSkin.tmpl) -- the latter is produced by
#   script.GenerateScripts via a second template, so leaving it out would leave half of the
#   generated artifacts unguarded.

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
$skinTemplate = Join-Path $repo $manifest.skinTemplate
$refDir = Join-Path $here 'reference'
$work = Join-Path $env:TEMP 'kfparity'
if (Test-Path $work) { Remove-Item -Recurse -Force $work }
New-Item -ItemType Directory -Force -Path $work | Out-Null

# Artifact kinds: id -> @{ Work = file name inside the item work dir; Ref = reference file suffix }
$ArtifactMap = @{
  plan = @{ Work = 'plan.json';   Ref = 'plan.json' }
  ahk  = @{ Work = 'keyflux.ahk'; Ref = 'keyflux.ahk' }
  skin = @{ Work = 'skin.txt';    Ref = 'skin.txt' }
}

function Get-Sha256([string]$p) { (Get-FileHash -Algorithm SHA256 -Path $p).Hash }

# Produce one artifact; returns the child exit code (0 = ok).
# Mirrors the runtime pipeline: DumpPlan / GenerateAHK both Preprocess + set BehaviorCatalog.
function Invoke-Produce([string]$Kind, [string]$Cfg, [string]$Out) {
  switch ($Kind) {
    'plan' { & $Exe DumpPlan $Cfg $Out | Out-Null; return $LASTEXITCODE }
    'ahk'  { & $Exe GenerateAHK $Cfg $template $Out | Out-Null; return $LASTEXITCODE }
    'skin' { & $Exe GenerateAHK $Cfg $skinTemplate $Out | Out-Null; return $LASTEXITCODE }
    default { return 127 }
  }
}

$total = 0
$pass = 0
$fail = @()

Push-Location $repo
try {
  foreach ($item in $manifest.items) {
    $name = $item.name
    $total++
    $kinds = @($item.artifacts)
    if ($kinds.Count -eq 0) { $kinds = @('plan', 'ahk') }

    $itemWork = Join-Path $work $name
    New-Item -ItemType Directory -Force -Path $itemWork | Out-Null
    Copy-Item (Join-Path $here $item.config) (Join-Path $itemWork 'config.json') -Force
    if (($item.PSObject.Properties.Name -contains 'plugins') -and $item.plugins) {
      $dst = Join-Path $itemWork 'plugins'
      New-Item -ItemType Directory -Force -Path $dst | Out-Null
      Copy-Item (Join-Path (Join-Path $repo $item.plugins) '*') $dst -Recurse -Force
    }

    $cfg = Join-Path $itemWork 'config.json'
    $ok = $true
    $outs = @{}
    foreach ($kind in $kinds) {
      if (-not $ArtifactMap.ContainsKey($kind)) { $fail += ("$name : unknown artifact '" + $kind + "'"); $ok = $false; break }
      $outs[$kind] = Join-Path $itemWork $ArtifactMap[$kind].Work
      $code = Invoke-Produce $kind $cfg $outs[$kind]
      if ($code -ne 0) { $fail += ("$name : " + $kind + " exit " + $code); $ok = $false }
    }
    if (!$ok) { continue }

    if ($Capture) {
      $deterministic = $true
      foreach ($kind in $kinds) {
        $second = Join-Path $itemWork ('second-' + $ArtifactMap[$kind].Work)
        $code = Invoke-Produce $kind $cfg $second
        if ($code -ne 0) {
          $fail += ("$name : " + $kind + " exit " + $code + " (2nd run)")
          $deterministic = $false
        }
        elseif ((Get-Sha256 $outs[$kind]) -ne (Get-Sha256 $second)) {
          $fail += ("$name : NONDETERMINISTIC [" + $kind + "] (reference not recorded)")
          $deterministic = $false
        }
      }
      if (!$deterministic) { continue }
      New-Item -ItemType Directory -Force -Path $refDir | Out-Null
      foreach ($kind in $kinds) {
        Copy-Item $outs[$kind] (Join-Path $refDir ($name + '.' + $ArtifactMap[$kind].Ref)) -Force
      }
      $pass++
    }
    else {
      $bad = @()
      foreach ($kind in $kinds) {
        $ref = Join-Path $refDir ($name + '.' + $ArtifactMap[$kind].Ref)
        if (!(Test-Path $ref)) { $bad += ($kind + '(no-ref)'); continue }
        if ((Get-Sha256 $outs[$kind]) -ne (Get-Sha256 $ref)) { $bad += $kind }
      }
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
