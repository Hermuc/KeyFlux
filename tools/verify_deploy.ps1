# verify_deploy.ps1 - mechanical check that both deploy "legs" were actually staged.
#
# WHY THIS EXISTS (report #6):
#   KeyFlux ships TWO independent binaries from the SAME cargo release build:
#     leg A (backend) : target/release/settings.exe        -> bin/settings.exe
#     leg B (panel)   : target/release/keyflux-settings.exe -> bin/ui/KeyFlux.Settings.exe
#   Every "the deploy was fine" incident this repo has had was one leg silently stale,
#   because the two legs are produced/copied by DIFFERENT steps (Makefile buildClientReactor
#   vs release.yml) and nothing compared them. Worse, an all-three-equal md5 (built A ==
#   staged A == built B == staged B) is itself a RED FLAG: it means one binary was copied
#   into both slots -- the exact "wrong exe in the wrong place" failure.
#
# WHAT IT CHECKS (4 assertions):
#   1. built backend == staged backend     (bin/settings.exe is not stale)
#   2. built panel   == staged panel       (bin/ui/KeyFlux.Settings.exe is not stale)
#   3. staged backend != staged panel      (the two legs were NOT swapped/alike)
#   4. both staged files exist             (leg was not forgotten entirely)
#
# NOTE on hashing: this compares a COPY against its SOURCE (same bytes, byte-identical),
#   so it is NOT the "reactor binaries are not byte-reproducible" case -- that caveat is
#   about recompiling the same source twice, and does not apply to a straight copy.
#   SHA256 (repo-wide convention, see tools/lib/kf-tools.ps1 Get-KfSha256); the report
#   said md5, SHA256 is the strictly stronger form of the same check.
#
# Usage (from repo root, pwsh 7):
#   pwsh -NoProfile -ExecutionPolicy Bypass -File tools/verify_deploy.ps1
#   pwsh ... -ReleaseDir 'config-ui-reactor/target/release' -BinRoot 'bin'
#
# Exit code 0 = both legs consistent; non-zero = at least one assertion failed.
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
function Assert-Leg([string]$Label, [string]$Built, [string]$Staged) {
    if (!(Test-Path -LiteralPath $Built)) {
        [void]$failures.Add("$Label : built artifact missing -> $Built (run: make buildClientReactor)")
        return $null
    }
    if (!(Test-Path -LiteralPath $Staged)) {
        [void]$failures.Add("$Label : staged artifact missing -> $Staged (run: make buildClientReactor)")
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

Write-Host '[verify-deploy] two-leg staging consistency'

$builtBackend = Join-Path $repo "$ReleaseDir/settings.exe"
$stagedBackend = Join-Path $repo "$BinRoot/settings.exe"
$builtPanel = Join-Path $repo "$ReleaseDir/keyflux-settings.exe"
$stagedPanel = Join-Path $repo "$BinRoot/ui/KeyFlux.Settings.exe"

$hBackend = Assert-Leg 'backend  bin/settings.exe' $builtBackend $stagedBackend
$hPanel = Assert-Leg 'panel    bin/ui/KeyFlux.Settings.exe' $builtPanel $stagedPanel

# Reverse check: the two staged legs must NOT be identical -- equal hashes mean one
# binary was copied into both slots (the "wrong exe" failure an all-equal md5 hides).
if ($hBackend -and $hPanel -and $hBackend -eq $hPanel) {
    [void]$failures.Add("backend and panel staged binaries are IDENTICAL (sha256=$($hBackend.Substring(0,16))) -- one exe was copied into both legs")
}

if ($failures.Count -gt 0) {
    foreach ($f in $failures) { Write-Host "[FAIL] $f" }
    Write-Error ('[FAIL] verify-deploy: ' + $failures.Count + ' problem(s) -- deploy legs are inconsistent')
    exit 1
}

Write-Host '[verify-deploy] OK -- both legs consistent and distinct'
exit 0