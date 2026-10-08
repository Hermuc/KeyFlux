# check-freshness.ps1 - "is the staged binary newer than HEAD?" gate.
#
# WHY THIS EXISTS (report #5):
#   The repo has shipped STALE staged binaries more than once ("二进制被调包"): the file
#   looks present, so nothing complains, but it predates the commit you are validating --
#   and `make check` then validates the OLD generator, while the release packages the OLD
#   panel. mtime-vs-HEAD turns that silent staleness into a red gate.
#
# WHAT IT CHECKS:
#   For each staged build artifact, LastWriteTimeUtc must be LATER than the HEAD committer
#   time. A file older than HEAD = it was built before the current commit = STALE.
#
# SCOPE (deliberately narrow, to keep the gate honest):
#   * Default set = the gitignored build artifacts whose mtime really is their build time:
#       bin/settings.exe              (backend leg; .gitignore)
#       bin/ui/KeyFlux.Settings.exe   (panel leg;   .gitignore)
#   * NOT included, on purpose:
#       bin/KeyFlux-CommandInput.exe  -- committed to git, so checkout stamps its mtime and
#                                        mtime no longer tracks build time (a HEAD compare is
#                                        meaningless for it). Its staleness is a build-hash
#                                        concern, not an mtime one.
#       bin/AutoHotkey64.exe          -- vendored third-party runtime, intentionally old.
#   Missing files are SKIPPED by default (a local `make check` should not require the UI to
#   have been built); pass -Strict to make missing files a failure.
#
# Usage (from repo root, pwsh 7):
#   pwsh -NoProfile -ExecutionPolicy Bypass -File tools/check-freshness.ps1
#   pwsh ... -Strict            # missing staged artifacts are failures too
#   pwsh ... -Ref 2026-10-08T00:00:00+08:00   # override the reference time
#
# Exit 0 = every present artifact is newer than HEAD; non-zero = at least one stale.
# ASCII-only on purpose (`pwsh -File` misparses non-BOM UTF-8, same rule as the lib).

param(
    [string[]]$Path,
    [switch]$Strict,
    [string]$Ref
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'lib/kf-tools.ps1')
$repo = Get-KfRepoRoot
Set-Location $repo

if (-not $Path) {
    $Path = @('bin/settings.exe', 'bin/ui/KeyFlux.Settings.exe')
}

# Reference time = HEAD committer date (UTC). No commits -> nothing to compare against.
if ([string]::IsNullOrEmpty($Ref)) {
    $headIso = (& git log -1 --format=%cI 2>$null)
    if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrEmpty($headIso)) {
        Write-Host '[freshness] no git HEAD available -- skipped'
        exit 0
    }
    $Ref = $headIso.Trim()
}
$refTime = ([DateTimeOffset]::Parse($Ref)).UtcDateTime

Write-Host ("[freshness] reference (HEAD) = " + $refTime.ToString('yyyy-MM-ddTHH:mm:ssZ'))
$stale = [System.Collections.ArrayList]::new()
foreach ($rel in $Path) {
    $p = Join-Path $repo $rel
    if (!(Test-Path -LiteralPath $p)) {
        if ($Strict) {
            [void]$stale.Add("$rel : MISSING (staged artifact not built) -- run: make buildClientReactor")
        }
        else {
            Write-Host "  [skip] $rel (not present)"
        }
        continue
    }
    $m = (Get-Item -LiteralPath $p).LastWriteTimeUtc
    if ($m -lt $refTime) {
        $mins = [int][Math]::Round(($refTime - $m).TotalMinutes)
        [void]$stale.Add("$rel : STALE (mtime $($m.ToString('yyyy-MM-ddTHH:mm:ssZ')) is ${mins} min older than HEAD) -- rebuild before check/deploy")
    }
    else {
        Write-Host ("  [ok] $rel  mtime=" + $m.ToString('yyyy-MM-ddTHH:mm:ssZ'))
    }
}

if ($stale.Count -gt 0) {
    foreach ($s in $stale) { Write-Host "[FAIL] $s" }
    Write-Error ('[FAIL] freshness: ' + $stale.Count + ' stale staged artifact(s) -- rebuild (make buildClientReactor / make out) first')
    exit 1
}
Write-Host '[freshness] OK -- staged artifacts are newer than HEAD'
exit 0