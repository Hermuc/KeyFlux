# drop-in-rust: P4 production switch helper.
#
# Order of operations (everything must be green before the switch):
#   [1] cargo gates + release build   -> tools/cargo-gates.ps1 (single source for the
#       fmt / clippy / test sequence shared with make and both workflows)
#   [2] parity self-check             -> bin/settings.exe (baseline not drifted) AND the
#       freshly built Rust binary must both reproduce tools/parity/reference 4/4.
#       The Makefile comment always required this ("make parity 4/4 PASS") but never
#       declared it as a dependency; enforcing it HERE keeps a Go-only environment from
#       being blocked by a make prerequisite.
#   [3] overwrite bin/settings.exe with the Rust build.
#
# Consumers call bin/settings.exe by filename, so overwriting the file IS the switch.
# Rollback = make buildServer (Go sources are kept until retirement).
#
# NOTE: keep this file ASCII-only (pwsh -File / PS 5.1 misparse non-BOM UTF-8).
$ErrorActionPreference = 'Stop'

# Shared helpers (repo root / %TEMP% sandbox / SHA256 / determinism gate).
. (Join-Path $PSScriptRoot 'lib\kf-tools.ps1')
$repo = Get-KfRepoRoot
$parityScript = Join-Path $repo 'tools\parity\run_parity.ps1'

# --- [1/3] cargo gates + release build --------------------------------------------
# Mirror Makefile's `version = ...` (single source of truth): the Go build injects it via
# -ldflags; the Rust build needs it as KEYFLUX_VERSION (option_env!) so that GET /config's
# keyfluxVersion field matches the Go baseline byte-for-byte.
$mkLine = Select-String -Path (Join-Path $repo 'Makefile') -Pattern '^version\s*=\s*(\S+)' | Select-Object -First 1
if (-not $mkLine) { Write-Error '[FAIL] cannot read version from Makefile'; exit 1 }
$version = $mkLine.Matches[0].Groups[1].Value

Write-Host '== [1/3] cargo gates + release build'
& pwsh -NoProfile -ExecutionPolicy Bypass -File (Join-Path $repo 'tools\cargo-gates.ps1') `
  -Release -Version $version -EnvScript (Join-Path $repo 'config-ui-reactor\env.ps1')
if ($LASTEXITCODE -ne 0) { Write-Error "[FAIL] cargo gates (exit=$LASTEXITCODE)"; exit 1 }

# --- [2/3] parity self-check -------------------------------------------------------
# run_parity.ps1 is its own process on purpose: it ends with `exit`, so dot-sourcing it
# would terminate this session. Exit 0 <=> every manifest item matched its reference.
function Assert-ParityGreen([string]$label, [string]$exe) {
  if (!(Test-Path $exe)) { Write-Error "[FAIL] parity: exe not found: $exe"; exit 1 }
  Write-Host "== [2/3] parity: $label"
  & pwsh -NoProfile -ExecutionPolicy Bypass -File $parityScript -Exe $exe
  if ($LASTEXITCODE -ne 0) {
    Write-Error "[FAIL] parity not all-PASS for $label (exit=$LASTEXITCODE) -- refusing to overwrite bin/settings.exe"
    exit 1
  }
}

Assert-ParityGreen 'bin/settings.exe (current Go build)' (Join-Path $repo 'bin\settings.exe')
Assert-ParityGreen 'config-ui-reactor/target/release/settings.exe (Rust candidate)' (Join-Path $repo 'config-ui-reactor\target\release\settings.exe')

# --- [3/3] switch -----------------------------------------------------------------
Write-Host '== [3/3] overwrite bin/settings.exe with Rust build'
Copy-Item (Join-Path $repo 'config-ui-reactor\target\release\settings.exe') (Join-Path $repo 'bin\settings.exe') -Force
Write-Host '[OK] Rust settings.exe -> bin/settings.exe (drop-in done; next deploy/sync-out takes effect; rollback = make buildServer)'
