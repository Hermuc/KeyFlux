# Shared helpers for the KeyFlux parity harnesses and the build/deploy tools.
#
# WHY THIS EXISTS:
#   run_parity.ps1, run_api_parity.ps1 and the other tools each re-implemented (or
#   duplicated) the same primitives: repo-root resolution, the unique %TEMP% sandbox
#   name, the SHA256 digest, and the "capture twice, refuse to record on drift"
#   determinism gate. A gate copied N times drifts apart silently (the copy that is
#   not updated keeps reporting green), so this file is the single source of truth
#   for all of them. (drop-in-rust.ps1 was a consumer until it was removed together
#   with the Go backend on 2026-10-06; consumers today include tools/cargo-gates.ps1,
#   tools/deploy_panel.ps1 and tools/sync-plugins.ps1.)
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
# Reactor staging (config-ui-reactor/target/release -> bin/ui)
# ---------------------------------------------------------------------------

# The robocopy exclude set for staging the reactor release output into bin/ui.
#
# WHY THIS EXISTS:
#   The same /XD + /XF list used to be copied verbatim in THREE places -- the
#   `buildClientReactor` recipe in the Makefile, tools/deploy_panel.ps1 and
#   .github/workflows/release.yml -- which is exactly the "a gate copied N times
#   drifts apart silently" failure this file was created to prevent, and which
#   the repo has already paid for once (see the module header above). Only one
#   of the three copies would have been updated, and the stale ones would have
#   kept staging cargo intermediates (or worse, a second settings.exe) into the
#   shipped bin/ui.
#
#   Order matters for robocopy: /XD consumes names until the next switch, so the
#   directory list must stay immediately after /XD and the file list immediately
#   after /XF, in this one array.
function Get-KfReactorExcludes {
  [CmdletBinding()]
  param()
  return @(
    '/XD', '.fingerprint', 'build', 'deps', 'examples', 'incremental',
    '/XF', '*.pdb', '*.d', '*.rlib', '*.rmeta',
    '*.cargo-lock', '*.cargo-build-lock', '*.cargo-artifact-lock',
    'keyflux-settings.exe', 'settings.exe', 'build-tools.exe'
  )
}

# Stage the reactor release output into a destination directory (normally bin/ui)
# using the shared exclude set above. Returns the robocopy exit code; callers keep
# their own policy (0-7 = success, >= 8 = failure) and their own follow-up steps
# (renaming keyflux-settings.exe, copying fonts, printing a file count).
function Invoke-KfReactorStaging {
  [CmdletBinding()]
  param(
    [Parameter(Mandatory)][string]$Source,
    [Parameter(Mandatory)][string]$Destination
  )
  $robocopyArgs = @($Source, $Destination, '/E') + (Get-KfReactorExcludes) +
                  @('/NFL', '/NDL', '/NJH')
  robocopy @robocopyArgs | Out-Null
  return $LASTEXITCODE
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

  # ---------------------------------------------------------------------------
  # Template-derived include list
  # ---------------------------------------------------------------------------

  # Derive the engine's #Include order from templates/keyflux.tmpl -- the single source.
  # Returns paths relative to the template's own directory (e.g. "lib/core/Functions.ahk").
  # Lines carrying the plugin / custom-script placeholders are skipped (the generator fills
  # those in, they are not hand-written includes).
  # 2026-10-08 (batch K): tools/oracle.ps1 used to hand-maintain a 14-line copy of this list,
  # which had ALREADY drifted from the template (it omitted the four command-box modules:
  # CommandDisplay / ImeInputHost / CommandImeGuard / CommandInputHooks). Deriving it here
  # keeps oracle in step automatically.
  # NOTE: the AHK harnesses under tools/ keep their OWN subsets on purpose (each includes only
  # the modules it exercises), so this helper is used by oracle.ps1 only.
  function Get-KfIncludeList {
    [CmdletBinding()]
    param([Parameter(Mandatory)][string]$TemplatePath)
    $out = @()
    foreach ($ln in [IO.File]::ReadAllLines($TemplatePath, [Text.Encoding]::UTF8)) {
      if ($ln -notmatch '^\s*#include\s') { continue }
      if ($ln -match 'PLUGIN_INCLUDES' -or $ln -match 'CUSTOM_SCRIPT') { continue }
      $m = [regex]::Match($ln, '^\s*#include\s+(.+?)\s*$',
            [Text.RegularExpressions.RegexOptions]::IgnoreCase)
      if ($m.Success) { $out += $m.Groups[1].Value }
    }
    return $out
  }

  # ---------------------------------------------------------------------------
  # robocopy result convention
  # ---------------------------------------------------------------------------

  # robocopy exit codes 0-7 mean success (bit flags: 1 = copied, 2 = extra, 4 = mismatched,
  # ...); >= 8 means at least one file or directory failed. This convention used to be
  # re-implemented at a dozen call sites -- this is the single source of truth for PowerShell.
  # NOTE: the Makefile still spells `[ $$? -le 7 ]` inline at five sites. Those are sh recipe
  # lines and cannot call a PowerShell function, so they are intentionally left as-is (they
  # carry a comment pointing here).
  function Test-KfRobocopyOk {
    [CmdletBinding()]
    param([Parameter(Mandatory)][int]$ExitCode)
    return ($ExitCode -lt 8)
  }
