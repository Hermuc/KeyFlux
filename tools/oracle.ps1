# Oracle diff (phase 4): AHK runtime registry dump vs Go DumpPlan abbr section
# NOTE: pure ASCII on purpose - pwsh -File misreads non-BOM UTF-8 Chinese as ANSI.
# TODO(selected-action): extend to the selectedAction data array once the AHK side
# SelectedActionInit consumer lands (task #30) - compare DumpPlan selectedAction
# projection vs the rendered SelectedActionData rows (same ResolveRuleAction source).
#
# PATHS (2026-09-30): the two machine-specific paths that used to be hardcoded here are
# now parameters with equivalent defaults, so the script is no longer tied to one host:
#   -Repo   repo root.       default: $env:KEYFLUX_REPO_ROOT, else the checkout this
#                            script lives in (tools/lib/kf-tools.ps1 Get-KfRepoRoot).
#   -Config deploy config.   default: $env:KEYFLUX_DEPLOY_CONFIG, else
#                            $env:KEYFLUX_DEPLOY_DIR/data/config.json, else
#                            D:\PortableApps\KeyFlux-compiled\data\config.json
#                            (the live deploy tree whose config.json the instance uses).
#   -Take   register lines   (previously parsed by hand from $args) limit for bisection.
param(
  [string]$Repo = '',
  [string]$Config = '',
  [int]$Take = 0
)
$ErrorActionPreference = 'Stop'

# Shared helpers (repo root / %TEMP% sandbox / SHA256 / determinism gate).
. (Join-Path $PSScriptRoot 'lib\kf-tools.ps1')
if ([string]::IsNullOrEmpty($Repo)) { $Repo = Get-KfRepoRoot }
$repo = (Resolve-Path -LiteralPath $Repo).Path
if ([string]::IsNullOrEmpty($Config)) {
  if (![string]::IsNullOrEmpty($env:KEYFLUX_DEPLOY_CONFIG)) {
    $Config = $env:KEYFLUX_DEPLOY_CONFIG
  }
  else {
    $deployDir = $env:KEYFLUX_DEPLOY_DIR
    if ([string]::IsNullOrEmpty($deployDir)) { $deployDir = 'D:\PortableApps\KeyFlux-compiled' }
    $Config = Join-Path $deployDir 'data\config.json'
  }
}
$config = $Config
if (!(Test-Path -LiteralPath $config)) {
  Write-Host "ORACLE DIFF: FAIL [deploy config not found: $config]"
  Write-Host '  hint: -Config <path> / $env:KEYFLUX_DEPLOY_CONFIG / $env:KEYFLUX_DEPLOY_DIR'
  exit 1
}
$tmp = "$env:TEMP\mk_baseline"
New-Item -ItemType Directory -Force -Path $tmp | Out-Null
Copy-Item "$repo\bin\settings.exe" "$tmp\settings.exe" -Force

# 0. Regenerate $repo\bin\KeyFlux.ahk from -Config so that BOTH sides come from the SAME
#    config. Without this the script compares a script generated from the REPO config against
#    DumpPlan of the -Config (deploy) config => always FAIL. 2026-10-08 (batch K): a direct
#    run reported 7 bogus "semicolon extra in AHK" purely from repo-vs-deploy config drift;
#    the repo copy of data/config.json has an EMPTY selectedAction and different keymaps.
#    `make check` already does this step before invoking us; repeating it here keeps the
#    script correct when run standalone (the previous behaviour was a silent usage trap).
& "$repo\bin\settings.exe" GenerateAHK $config "$repo\templates\keyflux.tmpl" "$repo\bin\KeyFlux.ahk"
if ($LASTEXITCODE -ne 0) { Write-Host "ORACLE DIFF: FAIL [GenerateAHK exit $LASTEXITCODE]"; exit 1 }

# 1. Extract register lines from regenerated script (-Take N for bisection)
$take = $Take
$gen = [IO.File]::ReadAllLines("$repo\bin\KeyFlux.ahk", [Text.Encoding]::UTF8)
$regs = @($gen | Where-Object { $_ -match '^\s*CommandResolver\.Register\(' })
if ($take -gt 0) { $regs = $regs[0..($take - 1)] }
echo "extracted register lines: $($regs.Count)"
$harness = @()
$harness += '#SingleInstance Off'
# type9_keyflux.ahk calls ExecCapslockAbbr defined only in generated script;
# AHK v2 default #Warn shows a blocking load-time dialog (ErrorStdOut cannot suppress it).
# Disable warnings at top (same semantics as the commented-out line in generated script).
$harness += ('#Warn All, ' + 'Off')
# Full include set derived from templates/keyflux.tmpl (single source) so it cannot drift.
# 2026-10-08 (batch K): this used to be a hand-maintained 14-line list that had ALREADY
# drifted from the template (it omitted the four command-box modules).
foreach ($inc in (Get-KfIncludeList -TemplatePath "$repo\templates\keyflux.tmpl")) {
  $harness += "#Include $repo\bin\" + ($inc -replace '/', '\')
}
$harness += 'Main()'
$harness += 'ExitApp()'
$harness += 'Main() {'
$harness += "  FileAppend(`"H start``n`", `"$tmp\oracle_progress.txt`")"
$ri = 0
foreach ($r in $regs) {
  $ri++
  $harness += "  FileAppend(`"R$ri``n`", `"$tmp\oracle_progress.txt`")"
  $harness += $r
}
$harness += "  FileAppend(`"H registered `" CommandResolver.Table.Count `"``n`", `"$tmp\oracle_progress.txt`")"
$harness += "  CommandResolver.DumpAbbr(`"$tmp\resolver_dump.json`")"
$harness += "  FileAppend(`"H dumped``n`", `"$tmp\oracle_progress.txt`")"
$harness += '}'
[IO.File]::WriteAllLines("$repo\tmp_oracle_harness.ahk", $harness, (New-Object Text.UTF8Encoding $false))

# 2. Run harness to export runtime registry (kill stray AHK processes first)
Get-Process | Where-Object { $_.ProcessName -match 'AutoHotkey' -and $_.Path -notlike '*KeyFlux-compiled*' } | Stop-Process -Force
Start-Sleep -Milliseconds 300
Remove-Item "$tmp\resolver_dump.json", "$tmp\oracle_progress.txt" -ErrorAction SilentlyContinue
$p = Start-Process -FilePath "$repo\bin\AutoHotkey64.exe" -ArgumentList '/ErrorStdOut', "$repo\tmp_oracle_harness.ahk" -WorkingDirectory "$repo\bin" -PassThru -NoNewWindow -RedirectStandardError "$tmp\oracle_err.txt"
if (!$p.WaitForExit(20000)) { $p | Stop-Process -Force; Get-Content "$tmp\oracle_progress.txt" -ErrorAction SilentlyContinue; Get-Content "$tmp\oracle_err.txt" -ErrorAction SilentlyContinue; throw 'harness hung' }
if (!(Test-Path "$tmp\resolver_dump.json")) { Get-Content "$tmp\oracle_err.txt"; throw 'no resolver dump' }

# 3. Go side plan
& "$tmp\settings.exe" DumpPlan $config "$tmp\plan.json"

# 4. Compare: command set + step count
$plan = [IO.File]::ReadAllText("$tmp\plan.json", [Text.Encoding]::UTF8) | ConvertFrom-Json
$dump = [IO.File]::ReadAllText("$tmp\resolver_dump.json", [Text.Encoding]::UTF8) | ConvertFrom-Json
$bad = @()
foreach ($scope in @('capslock', 'semicolon')) {
  $goSide = @{}
  foreach ($e in $plan.abbr.$scope) { $goSide[$e.abbr] = @($e.actions).Count }
  $ahkSide = @{}
  foreach ($e in $dump.$scope) { $ahkSide[$e.command] = $e.steps }
  foreach ($k in $goSide.Keys) {
    if (!$ahkSide.ContainsKey($k)) { $bad += "$scope missing in AHK: $k" }
    elseif ($ahkSide[$k] -ne $goSide[$k]) { $bad += "$scope steps mismatch: $k go=$($goSide[$k]) ahk=$($ahkSide[$k])" }
  }
  foreach ($k in $ahkSide.Keys) {
    if (!$goSide.ContainsKey($k)) { $bad += "$scope extra in AHK: $k" }
  }
  echo "$scope : go=$($goSide.Count) ahk=$($ahkSide.Count)"
}
# 5. Cleanup transient harness artifact (keep working tree clean)
Remove-Item "$repo\tmp_oracle_harness.ahk" -ErrorAction SilentlyContinue
if ($bad.Count -eq 0) { echo 'ORACLE DIFF: PASS' } else { $bad | ForEach-Object { echo $_ }; echo 'ORACLE DIFF: FAIL'; exit 1 }
