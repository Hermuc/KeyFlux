# check-upstream-reactor.ps1 - is there a newer windows-reactor upstream? (report #7)
#
# WHY THIS EXISTS:
#   config-ui-reactor/Cargo.toml carries a vendored fork:
#     [patch.crates-io] windows-reactor = { path = "vendor/windows-reactor" }
#   The patch surface (P1..P8) is INVISIBLE to the compiler, tests and CI (see the vendor
#   crate's own header). So when upstream ships a new release, nothing in the repo notices —
#   the fork silently ages. This script turns that into a visible, actionable signal:
#   a newer upstream means "go re-base the fork", per
#   vendor/windows-reactor/PATCHES.md section 4 (the rebase checklist).
#
# WHAT IT DOES: parses the pinned version requirement from Cargo.toml, asks crates.io for the
#   crate's max stable version, and fails (exit 1) when upstream has moved past the pin.
#
# SCOPE: this only DETECTS. It does not bump the pin or replay patches -- that is a human
#   step (PATCHES.md section 4) because a fork re-base can collide anywhere.
#
# Usage (any cwd; needs network):
#   pwsh -NoProfile -ExecutionPolicy Bypass -File tools/check-upstream-reactor.ps1
# ASCII-only on purpose (pwsh -File misparses non-BOM UTF-8, repo convention).

param(
    [string]$Crate = 'windows-reactor',
    [string]$CargoToml = 'config-ui-reactor/Cargo.toml'
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'lib/kf-tools.ps1')
$repo = Get-KfRepoRoot

$tomlPath = Join-Path $repo $CargoToml
if (!(Test-Path -LiteralPath $tomlPath)) { Write-Host "[FAIL] not found: $tomlPath"; exit 1 }
$toml = Get-Content -Raw -LiteralPath $tomlPath

# The dependency requirement, e.g. `windows-reactor = "0.100"`. Anchored at line start so it
# cannot match `windows-reactor-setup = ...`; the `[patch.crates-io]` entry uses `{ path = ... }`
# (not a quoted string) and is therefore not matched either.
$m = [regex]::Match($toml, '(?m)^\s*windows-reactor\s*=\s*"([^"]+)"')
if (!$m.Success) { Write-Host '[FAIL] could not find the windows-reactor version requirement in Cargo.toml'; exit 1 }
$pinned = $m.Groups[1].Value
Write-Host "[deps-watch] pinned requirement: windows-reactor = `"$pinned`""

# crates.io API. A descriptive User-Agent is requested by crates.io policy.
try {
    $api = Invoke-RestMethod -Uri "https://crates.io/api/v1/crates/$Crate" -Headers @{ 'User-Agent' = 'KeyFlux-deps-watch' } -TimeoutSec 30
} catch {
    Write-Host "[FAIL] crates.io query failed: $($_.Exception.Message)"; exit 1
}
$latest = $api.crate.max_stable_version
if ([string]::IsNullOrEmpty($latest)) { Write-Host '[FAIL] crates.io returned no max_stable_version'; exit 1 }
Write-Host "[deps-watch] upstream max stable:  $latest"

# Cargo caret semantics for "0.100" = ">=0.100.0, <0.101.0". So the pin is satisfied by
# anything that is `0.100` or `0.100.<patch>`; a different major/minor means upstream moved.
$inRange = ($latest -eq $pinned) -or ($latest.StartsWith($pinned + '.'))
if ($inRange) {
    Write-Host '[deps-watch] OK -- upstream is still within the pinned requirement'
    exit 0
}

Write-Host "[deps-watch] NEWER upstream windows-reactor available: $latest (pin = $pinned)"
Write-Host '[deps-watch] action: re-base the vendored fork per vendor/windows-reactor/PATCHES.md section 4,'
Write-Host '[deps-watch]         then update the pin + PATCHES.md (upstream version/sha, diff surface, commit).'
exit 1