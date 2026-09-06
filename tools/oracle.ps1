# Oracle diff (phase 4): AHK runtime registry dump vs Go DumpPlan abbr section
# NOTE: pure ASCII on purpose - pwsh -File misreads non-BOM UTF-8 Chinese as ANSI.
# TODO(selected-action): extend to the selectedAction data array once the AHK side
# SelectedActionInit consumer lands (task #30) - compare DumpPlan selectedAction
# projection vs the rendered SelectedActionData rows (same ResolveRuleAction source).
$ErrorActionPreference = 'Stop'
$repo = 'D:\PortableApps\KeyFlux-main'
$tmp = "$env:TEMP\mk_baseline"
New-Item -ItemType Directory -Force -Path $tmp | Out-Null
Copy-Item "$repo\bin\settings.exe" "$tmp\settings.exe" -Force

# 1. Extract register lines from regenerated script (-Take N for bisection)
$take = 0
for ($i = 0; $i -lt $args.Count; $i++) { if ($args[$i] -eq '-Take') { $take = [int]$args[$i + 1] } }
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
# Full include set identical to generated script so all closure references resolve
$harness += "#Include $repo\bin\lib\core\translation.ahk"
$harness += "#Include $repo\bin\lib\core\IKeyEventBus.ahk"
$harness += "#Include $repo\bin\lib\core\EventBus.ahk"
$harness += "#Include $repo\bin\lib\core\Functions.ahk"
$harness += "#Include $repo\bin\lib\core\Programs.ahk"
$harness += "#Include $repo\bin\lib\core\WindowUtils.ahk"
$harness += "#Include $repo\bin\lib\core\AbbrInput.ahk"
$harness += "#Include $repo\bin\lib\actions\Actions.ahk"
$harness += "#Include $repo\bin\lib\core\KeymapManager.ahk"
$harness += "#Include $repo\bin\lib\core\InputTipWindow.ahk"
$harness += "#Include $repo\bin\lib\core\Utils.ahk"
$harness += "#Include $repo\bin\lib\context\SelectionContext.ahk"
$harness += "#Include $repo\bin\lib\rules\SelectedAction.ahk"
$harness += "#Include $repo\bin\lib\commands\CommandResolver.ahk"
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
Get-Process | Where-Object { $_.ProcessName -match 'AutoHotkey' -and $_.Path -notlike '*KeyFlux-1.0-beta1*' } | Stop-Process -Force
Start-Sleep -Milliseconds 300
Remove-Item "$tmp\resolver_dump.json", "$tmp\oracle_progress.txt" -ErrorAction SilentlyContinue
$p = Start-Process -FilePath "$repo\bin\AutoHotkey64.exe" -ArgumentList '/ErrorStdOut', "$repo\tmp_oracle_harness.ahk" -WorkingDirectory "$repo\bin" -PassThru -NoNewWindow -RedirectStandardError "$tmp\oracle_err.txt"
if (!$p.WaitForExit(20000)) { $p | Stop-Process -Force; Get-Content "$tmp\oracle_progress.txt" -ErrorAction SilentlyContinue; Get-Content "$tmp\oracle_err.txt" -ErrorAction SilentlyContinue; throw 'harness hung' }
if (!(Test-Path "$tmp\resolver_dump.json")) { Get-Content "$tmp\oracle_err.txt"; throw 'no resolver dump' }

# 3. Go side plan
& "$tmp\settings.exe" DumpPlan 'D:\PortableApps\KeyFlux-1.0-beta1\data\config.json' "$tmp\plan.json"

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
if ($bad.Count -eq 0) { echo 'ORACLE DIFF: PASS' } else { $bad | ForEach-Object { echo $_ }; echo 'ORACLE DIFF: FAIL' }
