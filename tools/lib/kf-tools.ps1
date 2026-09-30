# Shared helpers for the KeyFlux parity harnesses and the drop-in switch helper.
#
# WHY THIS EXISTS:
#   run_parity.ps1, run_api_parity.ps1 and drop-in-rust.ps1 each re-implemented the
#   same primitives: repo-root resolution, the unique %TEMP% sandbox name, the SHA256
#   digest, and the "capture twice, refuse to record on drift" determinism gate.
#   A gate copied N times drifts apart silently (the copy that is not updated keeps
#   reporting green), so this file is the single source of truth for all of them.
#
# CONVENTION:
#   ASCII-only on purpose -- `pwsh -File` and Windows PowerShell 5.1 misparse non-BOM
#   UTF-8 (same rule as tools/oracle.ps1 and config-ui-reactor/env.ps1).
#   Human-readable docs stay in English here; the Chinese docs live in the READMEs.
#
# USAGE:
#   . (Join-Path $PSScriptRoot 'lib/kf-tools.ps1')
#   $repo = Get-KfRepoRoot
#
# This file sets no global state on purpose (no $ErrorActionPreference, no cwd change);
# each caller keeps its own policy.

# ---------------------------------------------------------------------------
# Repo root
# ---------------------------------------------------------------------------

# Repository root, derived from THIS file's own location (tools/lib -> two levels up),
# so it is correct no matter which script dot-sources the library and what the cwd is.
# Order: -Path (explicit), $env:KEYFLUX_REPO_ROOT (user override, used by tools/oracle.ps1
# to drop its hardcoded path), then this checkout.
function Get-KfRepoRoot {
  [CmdletBinding()]
  param([string]$Path)
  if (![string]::IsNullOrEmpty($Path)) { return (Resolve-Path -LiteralPath $Path).Path }
  if (![string]::IsNullOrEmpty($env:KEYFLUX_REPO_ROOT)) {
    return (Resolve-Path -LiteralPath $env:KEYFLUX_REPO_ROOT).Path
  }
  return (Split-Path -Parent (Split-Path -Parent $PSScriptRoot))
}

# ---------------------------------------------------------------------------
# %TEMP% sandbox
# ---------------------------------------------------------------------------

# Create a fresh, empty scratch directory under %TEMP% and return its path.
# Unique per call: two concurrent invocations (or two passes of one -Capture) must not
# clobber each other's scratch tree, and a fixed name would be a real foot-gun because
# the harnesses delete their sandbox on exit.
# -Prefix keeps the historic names (kfparity / kfapiparity) so logs stay greppable.
function New-KfSandbox {
  [CmdletBinding()]
  param([string]$Prefix = 'kf')
  $p = Join-Path $env:TEMP ($Prefix + '-' + [guid]::NewGuid().ToString('N'))
  if (Test-Path $p) { Remove-Item -Recurse -Force $p }
  New-Item -ItemType Directory -Force -Path $p | Out-Null
  return $p
}

# ---------------------------------------------------------------------------
# Hashing
# ---------------------------------------------------------------------------

# SHA256 hex digest of a file. Uppercase, i.e. exactly Get-FileHash's own casing, which
# is what the parity harness compares; callers that need lowercase (the api-parity
# manifest) call .ToLower() on the result.
function Get-KfSha256 {
  [CmdletBinding()]
  param([Parameter(Mandatory)][string]$Path)
  return (Get-FileHash -Algorithm SHA256 -Path $Path).Hash
}

# ---------------------------------------------------------------------------
# Determinism gate
# ---------------------------------------------------------------------------

# Normalise one captured artifact to a byte[]:
#   $null    -> $null            (the capture produced no artifact at all)
#   byte[]   -> as-is            (api-parity captures responses in memory)
#   string   -> file contents    (parity captures artifacts on disk)
function Get-KfArtifactBytes {
  [CmdletBinding()]
  param([AllowNull()]$Artifact)
  if ($null -eq $Artifact) { return $null }
  if ($Artifact -is [byte[]]) { return $Artifact }
  return [IO.File]::ReadAllBytes([string]$Artifact)
}

# The determinism gate shared by both harnesses: a capture run twice must be
# byte-identical, otherwise the reference must NOT be recorded.
#   Returns $true when both captures match.
#   On drift: appends -Message to -Collect and returns $false (the harness keeps
#   scanning so the whole drift list is reported, then fails), or throws a terminating
#   error when no collector is supplied (fail fast).
# -Collect is an ArrayList on purpose: the callers must be able to append.
function Assert-KfDeterministic {
  [CmdletBinding()]
  param(
    [AllowNull()]$First,
    [AllowNull()]$Second,
    [string]$Message = 'NONDETERMINISTIC (artifact not recorded)',
    [System.Collections.ArrayList]$Collect
  )
  $a = Get-KfArtifactBytes $First
  $b = Get-KfArtifactBytes $Second
  $same = $false
  if ($null -eq $a -and $null -eq $b) { $same = $true }
  elseif ($null -eq $a -or $null -eq $b) { $same = $false }
  else { $same = ([Convert]::ToBase64String($a) -eq [Convert]::ToBase64String($b)) }
  if ($same) { return $true }
  # NOTE: test for $null, not truthiness -- an empty ArrayList is falsy in PowerShell.
  if ($null -ne $Collect) { [void]$Collect.Add($Message); return $false }
  throw $Message
}

# ---------------------------------------------------------------------------
# Engine guard
# ---------------------------------------------------------------------------

# Refuse to keep going while the engine or the command-box exe is running.
# Rationale (make sync-out): robocopy retries a locked destination file forever, so
# overwriting bin/*.exe while KeyFlux holds them open turns into a silent hang instead
# of an error. Fail fast with a message that names the offending processes.
# The settings panel (KeyFlux.Settings.exe) is not listed: it lives in bin/ui, which the
# /MIR step re-copies, and it is launched/closed by the engine rather than held open.
function Assert-KfEngineStopped {
  [CmdletBinding()]
  param([string[]]$Name = @('KeyFlux', 'KeyFlux-CommandInput'))
  $running = @(Get-Process -Name $Name -ErrorAction SilentlyContinue)
  if ($running.Count -eq 0) { return }
  $list = ($running | ForEach-Object { $_.ProcessName + '(' + $_.Id + ')' }) -join ', '
  throw ('engine is running: ' + $list + ' -- close it/them first (robocopy retries a locked ' +
         'exe forever, so the sync would hang instead of failing)')
}
