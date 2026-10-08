# check-deps.ps1 - dependency supply-chain audit (multi-dimension report #8).
#
# Runs, over every cargo project in the repo:
#   * cargo-audit  -- RUSTSEC advisories (known vulnerabilities);
#   * cargo-deny   -- advisories + licenses + bans + sources (config: <repo>/deny.toml).
#
# WHY A SCRIPT (not inlined into CI / the Makefile):
#   the "install two tools, then run them over each crate" sequence must live in ONE place;
#   a gate copied into two places drifts (same lesson as tools/lib/kf-tools.ps1).
#   CI job: .github/workflows/deps-watch.yml (weekly). Local: `make check-deps`.
#
# REQUIREMENTS: cargo on PATH; cargo-audit and cargo-deny installed
#   (`cargo install cargo-audit cargo-deny`). Network is needed (advisory DB + crates.io).
#
# Exit 0 = every crate clean; non-zero = a finding, or a required tool is missing.
# ASCII-only on purpose (pwsh -File misparses non-BOM UTF-8, repo convention).

param(
    # Skip a crate that has no Cargo.lock (cannot be audited).
    [switch]$SkipUnlocked
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'lib/kf-tools.ps1')
$repo = Get-KfRepoRoot
$config = Join-Path $repo 'deny.toml'
if (!(Test-Path -LiteralPath $config)) { Write-Host "[FAIL] missing config: $config"; exit 1 }

function Get-Tool([string]$name) { (Get-Command $name -ErrorAction SilentlyContinue).Source }

if (!(Get-Tool 'cargo')) { Write-Host '[FAIL] cargo not on PATH'; exit 1 }
$audit = Get-Tool 'cargo-audit'
$deny = Get-Tool 'cargo-deny'
if (!$audit -and !$deny) {
    Write-Host '[FAIL] neither cargo-audit nor cargo-deny installed.'
    Write-Host '       install: cargo install cargo-audit cargo-deny'
    exit 1
}

# Both cargo projects in this repo are standalone (each has its own Cargo.lock).
$crates = @('config-ui-reactor', 'command-input')
$failed = 0

foreach ($crate in $crates) {
    $dir = Join-Path $repo $crate
    if (!(Test-Path -LiteralPath (Join-Path $dir 'Cargo.lock'))) {
        if ($SkipUnlocked) { Write-Host "  [skip] $crate (no Cargo.lock)"; continue }
        Write-Host "[FAIL] $crate has no Cargo.lock -- cannot audit"; $failed++; continue
    }
    Push-Location $dir
    try {
        if ($audit) {
            Write-Host "== cargo audit @ $crate"
            & cargo audit
            if ($LASTEXITCODE -ne 0) { Write-Host "[FAIL] cargo audit @ $crate"; $failed++ }
        }
        if ($deny) {
            Write-Host "== cargo deny @ $crate (config: deny.toml)"
            & cargo deny --config $config check
            if ($LASTEXITCODE -ne 0) { Write-Host "[FAIL] cargo deny @ $crate"; $failed++ }
        }
    }
    finally { Pop-Location }
}

if ($failed -gt 0) { Write-Host "[FAIL] dependency audit: $failed finding(s)"; exit 1 }
Write-Host '[ok] dependency audit clean'