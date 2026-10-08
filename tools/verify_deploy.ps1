# verify_deploy.ps1 - mechanical check that the deploy "legs" were actually staged.
#
# WHY THIS EXISTS (report #6, extended by N2):
#   KeyFlux ships TWO binaries produced by the SAME cargo release build plus ONE self-built exe:
#     leg A (backend)      : target/release/settings.exe         -> bin/settings.exe
#     leg B (panel)        : target/release/keyflux-settings.exe -> bin/ui/KeyFlux.Settings.exe
#     leg C (command input): command-input/target/release/...    -> bin/KeyFlux-CommandInput.exe
#   Every "the deploy was fine" incident this repo has had was one leg silently stale, because
#   the legs are produced/copied by DIFFERENT steps and nothing compared them. Worse, an all-equal
#   md5 across legs is itself a RED FLAG: it means one binary was copied into two slots.
#
# WHAT IT CHECKS:
#   1. built == staged for each leg (leg C is OPTIONAL: skipped, not failed, when the built
#      artifact is absent -- release CI builds it explicitly, a bare dev checkout may not have);
#   2. the staged legs are pairwise DISTINCT (the "wrong exe in the wrong place" failure);
#   3. every staged file exists.
#
# NOTE on hashing: this compares a COPY against its SOURCE (byte-identical), so the "reactor
#   binaries are not byte-reproducible" caveat does NOT apply (that is about recompiling twice).
#   SHA256 (repo-wide convention, tools/lib/kf-tools.ps1 Get-KfSha256); the report said md5,
#   SHA256 is the strictly stronger form of the same check.
#
# Usage (from repo root, pwsh 7):
#   pwsh -NoProfile -ExecutionPolicy Bypass -File tools/verify_deploy.ps1
#
# Exit code 0 = legs consistent; non-zero = at least one assertion failed.
# ASCII-only on purpose (`pwsh -File` misparses non-BOM UTF-8, same rule as the lib).

param(
    [string]$ReleaseDir = 'config-ui-reactor/target/release',
    [string]$BinRoot = 'bin'
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'lib/kf-tools.ps1')
$repo = Get-KfRepoRoot
Set-Location $repo

$failures = [System.Collections.ArrayList]::new()
function Assert-Leg([string]$Label, [string]$Built, [string]$Staged, [switch]$Optional) {
    if (!(Test-Path -LiteralPath $Built)) {
        if ($Optional) {
            Write-Host "  [skip] $Label -- built artifact not present ($Built); build it to enable this leg"
            return $null
        }
        [void]$failures.Add("$Label : built artifact missing -> $Built (run: make buildClientReactor)")
        return $null
    }
    if (!(Test-Path -LiteralPath $Staged)) {
        [void]$failures.Add("$Label : staged artifact missing -> $Staged (run the leg's build/copy step)")
        return $null
    }
    $hBuilt = Get-KfSha256 $Built
    $hStaged = Get-KfSha256 $Staged
    if ($hBuilt -ne $hStaged) {
        [void]$failures.Add("$Label : STALE staging -- built=$($hBuilt.Substring(0,16)) staged=$($hStaged.Substring(0,16)); $Staged is not a copy of $Built")
        return $hStaged
    }
    Write-Host ("  [ok] $Label  sha256=" + $hStaged.Substring(0, 16))
    return $hStaged
}

Write-Host '[verify-deploy] deploy-leg staging consistency'

$hBackend = Assert-Leg 'backend       bin/settings.exe' `
    (Join-Path $repo "$ReleaseDir/settings.exe") `
    (Join-Path $repo "$BinRoot/settings.exe")
$hPanel = Assert-Leg 'panel         bin/ui/KeyFlux.Settings.exe' `
    (Join-Path $repo "$ReleaseDir/keyflux-settings.exe") `
    (Join-Path $repo "$BinRoot/ui/KeyFlux.Settings.exe")
# leg C: self-built by `make command-input`; optional because release CI builds it explicitly
# but a bare checkout may not have. When present it MUST match the committed copy (N2).
$hCommand = Assert-Leg 'command-input bin/KeyFlux-CommandInput.exe' `
    (Join-Path $repo 'command-input/target/release/keyflux-command-input.exe') `
    (Join-Path $repo "$BinRoot/KeyFlux-CommandInput.exe") -Optional

# Reverse check: staged legs must be pairwise distinct -- equal hashes mean one binary landed in
# two slots (the failure an all-equal md5 hides).
$present = [ordered]@{}
if ($hBackend) { $present['backend'] = $hBackend }
if ($hPanel) { $present['panel'] = $hPanel }
if ($hCommand) { $present['command-input'] = $hCommand }
$seen = @{}
foreach ($label in $present.Keys) {
    $h = $present[$label]
    if ($seen.ContainsKey($h)) {
        [void]$failures.Add("staged '$label' and '$($seen[$h])' are IDENTICAL (sha256=$($h.Substring(0,16))) -- one exe was copied into two legs")
    }
    else { $seen[$h] = $label }
}

if ($failures.Count -gt 0) {
    foreach ($f in $failures) { Write-Host "[FAIL] $f" }
    Write-Error ('[FAIL] verify-deploy: ' + $failures.Count + ' problem(s) -- deploy legs are inconsistent')
    exit 1
}

Write-Host '[verify-deploy] OK -- legs consistent and distinct'
exit 0