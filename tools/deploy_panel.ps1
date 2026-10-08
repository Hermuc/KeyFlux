# deploy_panel.ps1 - guarded manual deploy of the settings panel.
#
# Why this exists: the hand-rolled deploy chain (build -> robocopy -> restart)
# pushed STALE artifacts twice on 2026-10-01 because a failed gate did not stop
# the chain. This script makes every step a hard gate:
#   0. probe-marker guard      : no "TEMP-PROBE" left in config-ui-reactor/src
#   1. gates + release build   : tools/cargo-gates.ps1 -Release (single source)
#   2. staging                 : robocopy target/release -> bin/ui + rename + fonts
#   3. stop running panels     : terminate KeyFlux.Settings.exe (file lock)
#   4. production sync + hash  : robocopy bin/ui -> <DeployRoot>\bin\ui, sha256 equal
#   5. relaunch + window check : detached start, top-level window must appear
#
# Usage (from repo root, pwsh 7):
#   pwsh -NoProfile -ExecutionPolicy Bypass -File tools/deploy_panel.ps1
#   pwsh ... -DeployRoot 'D:\PortableApps\KeyFlux-compiled' -Version '1.0-beta1' -SkipGates
#
# Exit code 0 = deployed and panel running; non-zero = aborted before any
# production write (steps 0-1) or at the failed step.

param(
    [string]$DeployRoot = 'D:\PortableApps\KeyFlux-compiled',
    [string]$Version = '1.0-beta1',
    [switch]$SkipGates
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'lib/kf-tools.ps1')
$repo = Get-KfRepoRoot
Set-Location $repo

function Step($n, $msg) { Write-Host "[deploy-panel $n/6] $msg" }

# ---- 0. probe-marker guard -------------------------------------------------
Step 0 'probe-marker guard'
$markers = Get-ChildItem 'config-ui-reactor/src' -Recurse -Filter '*.rs' |
    Select-String -Pattern 'TEMP-PROBE' -SimpleMatch
if ($markers) {
    $markers | ForEach-Object { Write-Host ("  MARKED: " + $_.Path + ":" + $_.LineNumber) }
    Write-Error '[FAIL] probe markers left in src/. Remove them first.'
    exit 1
}
Write-Host '  [OK] no probe markers'

# ---- 1. gates + release build (single source: cargo-gates.ps1) -------------
if ($SkipGates) {
    Step 1 'gates SKIPPED (-SkipGates); building release only'
    & pwsh -NoProfile -ExecutionPolicy Bypass -File tools/cargo-gates.ps1 `
        -Release -NoTest -Version $Version -EnvScript config-ui-reactor/env.ps1
    if ($LASTEXITCODE -ne 0) { Write-Error '[FAIL] release build failed'; exit 1 }
} else {
    Step 1 'gates + release build (cargo-gates.ps1 -Release)'
    & pwsh -NoProfile -ExecutionPolicy Bypass -File tools/cargo-gates.ps1 `
        -Release -Version $Version -EnvScript config-ui-reactor/env.ps1
    if ($LASTEXITCODE -ne 0) { Write-Error '[FAIL] gates/build failed'; exit 1 }
}

# ---- 2. staging (same recipe as make buildClientReactor) -------------------
Step 2 'staging target/release -> bin/ui'
$src = (Resolve-Path 'config-ui-reactor/target/release').Path
$dst = Join-Path $repo 'bin/ui'
# Exclude set + robocopy step come from tools/lib/kf-tools.ps1 (single source --
# it used to be copied verbatim here, in the Makefile and in release.yml).
$code = Invoke-KfReactorStaging -Source $src -Destination $dst
if ($code -ge 8) { Write-Error "[FAIL] robocopy exit $code"; exit 1 }
Copy-Item "$src/keyflux-settings.exe" "$dst/KeyFlux.Settings.exe" -Force
New-Item -ItemType Directory -Force -Path "$dst/fonts" | Out-Null
Copy-Item 'config-ui-reactor/resources/fonts/*.ttf' "$dst/fonts/" -Force
Write-Host '  [OK] staged (exe renamed to KeyFlux.Settings.exe, fonts copied)'

# ---- 3. stop running panels ------------------------------------------------
Step 3 'stop running KeyFlux.Settings.exe'
$killed = 0
Get-Process -Name 'KeyFlux.Settings' -ErrorAction SilentlyContinue | ForEach-Object {
    try { $_.Kill(); $_.WaitForExit(5000) | Out-Null; $script:killed++ } catch {}
}
Write-Host "  [OK] panels stopped: $killed"
Start-Sleep -Seconds 2

# ---- 4. production sync + hash gate ----------------------------------------
Step 4 "sync bin/ui -> $DeployRoot\bin\ui"
robocopy "$repo\bin\ui" "$DeployRoot\bin\ui" /E /R:1 /W:1 /NFL /NDL /NJH | Out-Null
if ($LASTEXITCODE -ge 8) { Write-Error "[FAIL] prod robocopy exit $LASTEXITCODE"; exit 1 }
$localHash = Get-KfSha256 "$repo\bin\ui\KeyFlux.Settings.exe"
$prodHash  = Get-KfSha256 "$DeployRoot\bin\ui\KeyFlux.Settings.exe"
if ($localHash -ne $prodHash) {
    Write-Error "[FAIL] hash mismatch local=$localHash prod=$prodHash"
    exit 1
}
Write-Host ("  [OK] deployed, exe sha256=" + $prodHash.Substring(0, 16))

# ---- 5. relaunch + window check --------------------------------------------
Step 5 'relaunch panel (detached) and verify window'
$panelDir = "$DeployRoot\bin\ui"
$proc = Start-Process -FilePath "$panelDir\KeyFlux.Settings.exe" `
    -WorkingDirectory $panelDir -PassThru `
    -WindowStyle Hidden
Start-Sleep -Seconds 8
$visible = Get-Process -Id $proc.Id -ErrorAction SilentlyContinue |
    Where-Object { $_.MainWindowHandle -ne 0 }
if (-not $visible) {
    Write-Error "[FAIL] panel pid=$($proc.Id) has no visible window"
    exit 1
}
Write-Host ("  [OK] panel running pid=" + $proc.Id)
Write-Host '[deploy-panel] DONE'
exit 0
