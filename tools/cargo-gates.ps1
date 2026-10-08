# Cargo gate runner for config-ui-reactor -- the ONE place the "fmt / clippy / test"
# (optionally + release build) sequence is defined.
#
# WHY THIS EXISTS:
#   The same three-gate sequence was copy-pasted into five places:
#     Makefile (`buildClientReactor` / `analyzers` targets),
#     .github/workflows/analyzers.yml (reactor-gates), .github/workflows/release.yml,
#     .github/workflows/release.yml (reactor UI build).
#   Gates that exist five times drift apart: the copy nobody updates keeps reporting
#   green. This script is the single source of truth; every caller invokes it.
#
# USAGE (invoke as a script, NOT dot-sourced -- it calls exit):
#   pwsh -File tools/cargo-gates.ps1                     # fmt + clippy + test
#   pwsh -File tools/cargo-gates.ps1 -NoTest             # style gates only (make analyzers)
#   pwsh -File tools/cargo-gates.ps1 -Release            # + cargo build --release
#   pwsh -File tools/cargo-gates.ps1 -Release -Version 1.0-beta1 `
#        -EnvScript config-ui-reactor/env.ps1
#
# -EnvScript (local builds): dot-sourced first so cargo/rustc/link.exe resolve. CI
#   runners have the toolchain on PATH already and MUST NOT pass it (env.ps1 pins this
#   machine's MSVC/SDK paths and throws when they are absent).
# -Version: sets KEYFLUX_VERSION for the release build; the Rust binaries read it via
#   option_env! so GET /config matches the frozen api-parity baseline byte-for-byte
#   (captured from the Go backend, retired 2026-10-06).
#
# EXIT CODES (unchanged from the inlined recipes): 0 = every requested gate green,
#   1 = the first failing gate (fail fast, like the `if ($LASTEXITCODE -ne 0) { exit 1 }`
#   chains it replaces). CI and local make targets therefore still fail on failure.
#
# NOTE: keep this file ASCII-only and PS 5.1 parseable (repo convention; `pwsh -File`
#   and Windows PowerShell 5.1 misparse non-BOM UTF-8).
# NOTE: do NOT set $ErrorActionPreference='Stop' -- cargo writes progress to stderr and
#   PowerShell would turn that into a terminating error (same rule as env.ps1).

param(
  # Also run `cargo build --release` after the three gates.
  [switch]$Release,
  # Skip `cargo test` (make analyzers is style-only).
  [switch]$NoTest,
  # Optional KEYFLUX_VERSION to inject into the release build.
  [string]$Version = '',
  # Optional env script (e.g. config-ui-reactor/env.ps1) dot-sourced before running cargo.
  [string]$EnvScript = ''
)

$ErrorActionPreference = 'Continue'

# Repo root / shared helpers (tools/lib/kf-tools.ps1).
. (Join-Path $PSScriptRoot 'lib\kf-tools.ps1')
$repo = Get-KfRepoRoot

$projectDir = Join-Path $repo 'config-ui-reactor'
if (!(Test-Path $projectDir)) { Write-Host "[FAIL] not found: $projectDir"; exit 1 }

if (![string]::IsNullOrEmpty($EnvScript)) {
  $envPath = $EnvScript
  if (!(Test-Path $envPath)) { $envPath = Join-Path $repo $EnvScript }
  if (!(Test-Path $envPath)) { Write-Host "[FAIL] env script not found: $EnvScript"; exit 1 }
  . $envPath
}

if (![string]::IsNullOrEmpty($Version)) {
  # NOTE: only the release build below gets this; the test gate must keep running with
  # the variable UNSET (handlers_config's keyfluxVersion test asserts the empty default
  # and would fail otherwise).
  Write-Host "[cargo-gates] KEYFLUX_VERSION=$Version (release build only)"
}

$gates = @('fmt --all --check', 'clippy --all-targets -- -D warnings')
if (-not $NoTest) { $gates += 'test --quiet' }
if ($Release) { $gates += 'build --release' }
Write-Host ('[cargo-gates] project: ' + $projectDir)
Write-Host ('[cargo-gates] gates: ' + ($gates -join ' / '))

# Run one gate; fail fast on the first non-zero exit code.
function Invoke-CargoGate([string]$label, [string[]]$argv) {
  Write-Host "== cargo $label"
  & cargo @argv
  if ($LASTEXITCODE -ne 0) {
    Write-Host "[FAIL] cargo $label (exit=$LASTEXITCODE)"
    exit 1
  }
}

Push-Location $projectDir
Invoke-CargoGate 'fmt --all --check' @('fmt', '--all', '--check')
Invoke-CargoGate 'clippy --all-targets -- -D warnings' @('clippy', '--all-targets', '--', '-D', 'warnings')
if (-not $NoTest) { Invoke-CargoGate 'test --quiet' @('test', '--quiet') }
if ($Release) {
  # Set the version only now (see the note in the param block above).
  if (![string]::IsNullOrEmpty($Version)) { $env:KEYFLUX_VERSION = $Version }
  Invoke-CargoGate 'build --release' @('build', '--release')
}
Pop-Location

Write-Host ('[OK] cargo gates (' + ($gates -join ' / ') + ')')
exit 0
