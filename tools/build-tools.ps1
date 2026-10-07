# Single entry point for the Rust build/ops CLI (tools/build_tools.go -> Rust port).
#
# WHY THIS EXISTS: `Makefile` target `uploadLanZou` used to call
#   `go run scripts/build_tools.go <subcommand> ...`
# The Go toolchain was retired on 2026-10-07 (KeyFlux backend is fully Rust now), so the
# two call sites go through this wrapper instead. Same shape as tools/cargo-gates.ps1:
# repo-root resolution + env script + cargo invocation live in ONE place; callers never
# inline cargo flags (this repo has already paid for "same command copied in 3 places").
#
# USAGE (cwd does not matter; the wrapper pushes the repo root):
#   pwsh -File tools/build-tools.ps1 checkForAHKUpdate 2.0.19
#   pwsh -File tools/build-tools.ps1 updateShareLink 1.0-beta1 [siteDocPath]
#
# Exit codes are forwarded verbatim from the CLI (0 ok / 1 outdated AHK / 2 usage or IO).
# NOTE: keep this file ASCII-only and PS 5.1 parseable (repo convention; `pwsh -File` and
#   Windows PowerShell 5.1 misparse non-BOM UTF-8).
# NOTE: do NOT set $ErrorActionPreference='Stop' -- cargo writes progress to stderr and
#   PowerShell would turn that into a terminating error (same rule as cargo-gates.ps1).

param(
  # Subcommand + arguments, forwarded verbatim (e.g. `checkForAHKUpdate 2.0.19`).
  [Parameter(ValueFromRemainingArguments = $true)][string[]]$CommandArgs = @()
)

$ErrorActionPreference = 'Continue'

# Repo root / shared helpers (tools/lib/kf-tools.ps1).
. (Join-Path $PSScriptRoot 'lib\kf-tools.ps1')
$repo = Get-KfRepoRoot

$manifest = Join-Path $repo 'config-ui-reactor\Cargo.toml'
if (!(Test-Path $manifest)) { Write-Host "[FAIL] not found: $manifest"; exit 1 }

# Local dev needs the MSVC/Windows SDK env (CI runners already have it on PATH).
$envScript = Join-Path $repo 'config-ui-reactor\env.ps1'
if (Test-Path $envScript) { . $envScript }

if ($CommandArgs.Count -eq 0) {
  Write-Host 'usage: tools/build-tools.ps1 <checkForAHKUpdate|updateShareLink> [args...]'
  exit 2
}

# --quiet: keep cargo's own progress off stdout so the CLI's contract lines stay verbatim.
Push-Location $repo
try {
  & cargo run --quiet --release --manifest-path $manifest --bin build-tools -- @CommandArgs
  exit $LASTEXITCODE
}
finally {
  Pop-Location
}
