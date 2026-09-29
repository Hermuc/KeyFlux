# drop-in-rust: P4 production switch helper.
# Runs the full gate (fmt / clippy / test / release build) and, ONLY if all
# pass, overwrites bin/settings.exe with the Rust build.
# Consumers call bin/settings.exe by filename, so overwriting the file IS the
# switch. Rollback = make buildServer (Go sources are kept until retirement).
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
. (Join-Path $repo 'config-ui-reactor/env.ps1')
Set-Location (Join-Path $repo 'config-ui-reactor')

function Assert-LastExit([string]$label) {
  if ($LASTEXITCODE -ne 0) {
    Write-Error "[FAIL] $label (exit=$LASTEXITCODE)"
    exit 1
  }
}

Write-Host '== [1/5] cargo fmt --all --check'
cargo fmt --all --check
Assert-LastExit 'fmt'

Write-Host '== [2/5] cargo clippy --all-targets -- -D warnings'
cargo clippy --all-targets -- -D warnings
Assert-LastExit 'clippy'

Write-Host '== [3/5] cargo test'
cargo test --quiet
Assert-LastExit 'test'

Write-Host '== [4/5] cargo build --release'
# Mirror Makefile's `version = ...` (single source of truth): the Go build injects
# it via -ldflags; the Rust build needs it as KEYFLUX_VERSION (option_env!) so that
# GET /config's keyfluxVersion field matches the Go baseline byte-for-byte.
$mkLine = Select-String -Path (Join-Path $repo 'Makefile') -Pattern '^version\s*=\s*(\S+)' | Select-Object -First 1
if (-not $mkLine) { Write-Error '[FAIL] cannot read version from Makefile'; exit 1 }
$env:KEYFLUX_VERSION = $mkLine.Matches[0].Groups[1].Value
Write-Host "   KEYFLUX_VERSION=$env:KEYFLUX_VERSION"
cargo build --release
Assert-LastExit 'build'

Write-Host '== [5/5] overwrite bin/settings.exe with Rust build'
Copy-Item target/release/settings.exe ../bin/settings.exe -Force
Write-Host '[OK] Rust settings.exe -> bin/settings.exe (drop-in done; next deploy/sync-out takes effect; rollback = make buildServer)'
