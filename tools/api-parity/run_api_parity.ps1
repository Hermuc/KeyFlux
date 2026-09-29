# API-level parity harness -- captures byte-exact baseline responses of the Go settings
# backend and replays the same requests against a candidate implementation (Rust) to
# diff them byte-for-byte.
#
# WHY: docs/plan-rust-migration.md replaces the Go panel backend with a Rust one. The
# route surface is locked by config-server/internal/server/bridge_test.go (19 routes).
# This tool freezes the Go responses (via the in-process `Call` transport, so no port
# management) into tools/api-parity/reference/go/*.json and later diffs any exe.
#
# USAGE (PowerShell 7 recommended; parses under Windows PowerShell 5.1 too):
#   pwsh -File tools/api-parity/run_api_parity.ps1 -Capture               # record baseline from repo bin/settings.exe
#   pwsh -File tools/api-parity/run_api_parity.ps1 -Capture -Exe <path>   # record from a specific exe
#   pwsh -File tools/api-parity/run_api_parity.ps1 -Check                 # replay repo bin/settings.exe vs reference
#   pwsh -File tools/api-parity/run_api_parity.ps1 -Check -Exe <path>     # replay a candidate (e.g. Rust) exe
#
# CONVENTIONS:
#   - ASCII-only on purpose: `pwsh -File` / PS 5.1 misparse non-BOM UTF-8 (same rule as
#     tools/parity/run_parity.ps1). Human-readable Chinese docs live in README.md.
#   - Side-effect endpoints are NOT executed in this phase (see README "Side-effect endpoints").
#   - Sandbox always under %TEMP%, unique per run, deleted afterwards.
#   - Final line is ASCII: "API-PARITY: <pass>/<total> PASS [MODE]" (exit 0) or FAIL (exit 1).
#   - Determinism: -Capture records TWICE into two independent sandboxes and refuses to
#     write the reference unless both passes are byte-identical.

param(
  [switch]$Capture,
  # Target exe for capture (default repo bin/settings.exe) or candidate exe for check.
  [string]$Exe = ''
)

$ErrorActionPreference = 'Stop'

$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$repo = Split-Path -Parent (Split-Path -Parent $here)
$corpusDir = Join-Path $here 'corpus'
$refDir = Join-Path $here 'reference'
$refGoDir = Join-Path $refDir 'go'
$manifestPath = Join-Path $refDir 'manifest.json'

if ([string]::IsNullOrEmpty($Exe)) { $Exe = Join-Path $repo 'bin\settings.exe' }
if (!(Test-Path $Exe)) { Write-Host "API-PARITY: 0/0 FAIL [exe not found: $Exe]"; exit 1 }
$Exe = (Resolve-Path $Exe).Path

# ---------------------------------------------------------------------------
# Endpoint table. Read-only set only; surface locked by bridge_test.go.
# body = corpus file name (relative to corpus/) or $null.
# ---------------------------------------------------------------------------
$items = @(
  @{ method = 'GET';  path = '/health';                                body = $null },
  @{ method = 'GET';  path = '/config';                                body = $null },
  @{ method = 'GET';  path = '/shortcuts';                             body = $null },
  @{ method = 'GET';  path = '/api/behaviors';                         body = $null },
  @{ method = 'GET';  path = '/api/plugins';                           body = $null },
  @{ method = 'GET';  path = '/api/plugins/everything_search/settings'; body = $null },
  @{ method = 'POST'; path = '/api/selected-action/test';              body = 'test_selected-action_url_hit.json' },
  @{ method = 'POST'; path = '/api/selected-action/test';              body = 'test_selected-action_textfeature_hit.json' },
  @{ method = 'POST'; path = '/api/selected-action/test';              body = 'test_selected-action_nomatch.json' }
)

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

# Reference file name: "<METHOD>_<path with / . etc escaped>.json" (+ corpus stem suffix
# when a request body is involved, since one endpoint may carry several samples).
function Get-RefName([string]$method, [string]$path, [string]$body) {
  $p = $path.TrimStart('/') -replace '[^A-Za-z0-9._-]', '_'
  if ($body) {
    $stem = [IO.Path]::GetFileNameWithoutExtension($body)
    return "${method}_${p}.${stem}.json"
  }
  return "${method}_${p}.json"
}

# Quote an argument for a raw command line (sandbox paths may contain spaces).
function Quote-Arg([string]$s) {
  if ($s -match '\s') { return '"' + $s + '"' }
  return $s
}

# Minimal JSON string escaper; inputs here are ASCII-only, hand-serialized so that the
# emitted bytes are stable across PowerShell 5.1 / 7 ConvertTo-Json formatting drift.
function ConvertTo-JsonStringLiteral([string]$s) {
  return '"' + $s.Replace('\', '\\').Replace('"', '\"') + '"'
}

# Run `settings.exe Call <METHOD> <PATH> <out-file> [--body f]` with cwd = the exe's
# directory (Go resolves ../data, ./behaviors, ./templates relative to it -- bridge.go).
# Returns @{ ExitCode; StdOut; StdErr }. No shell involved; %TEMP% spaces are safe.
function Invoke-SettingsCall([string]$exePath, [string]$cwd, [string]$method, [string]$path,
                             [string]$outFile, [string]$bodyFile) {
  $argStr = 'Call ' + (Quote-Arg $method) + ' ' + (Quote-Arg $path) + ' ' + (Quote-Arg $outFile)
  if ($bodyFile) { $argStr += ' --body ' + (Quote-Arg $bodyFile) }

  $psi = New-Object System.Diagnostics.ProcessStartInfo
  $psi.FileName = $exePath
  $psi.Arguments = $argStr
  $psi.WorkingDirectory = $cwd
  $psi.UseShellExecute = $false
  $psi.RedirectStandardOutput = $true
  $psi.RedirectStandardError = $true
  $proc = [System.Diagnostics.Process]::Start($psi)
  $so = $proc.StandardOutput.ReadToEnd()
  $se = $proc.StandardError.ReadToEnd()
  $proc.WaitForExit()
  return @{ ExitCode = $proc.ExitCode; StdOut = $so; StdErr = $se }
}

# Build the deploy-tree sandbox under %TEMP%:
#   <sandbox>/bin/settings.exe  <sandbox>/bin/behaviors/  <sandbox>/bin/templates/
#   <sandbox>/data/config.json  <sandbox>/data/plugins/
# cwd for every Call = <sandbox>/bin (deploy-tree convention; see bridge.go).
function New-Sandbox([string]$exeSrc) {
  $root = Join-Path $env:TEMP ('kfapiparity-' + [guid]::NewGuid().ToString('N'))
  $bin = Join-Path $root 'bin'
  $data = Join-Path $root 'data'
  New-Item -ItemType Directory -Force -Path $bin, $data | Out-Null
  Copy-Item $exeSrc (Join-Path $bin 'settings.exe')
  Copy-Item (Join-Path $repo 'data\config.json') $data
  Copy-Item (Join-Path $repo 'data\plugins') $data -Recurse
  Copy-Item (Join-Path $repo 'bin\behaviors') $bin -Recurse
  Copy-Item (Join-Path $repo 'bin\templates') $bin -Recurse
  return $root
}

function Remove-Sandbox([string]$root) {
  if ($root -and (Test-Path $root)) { Remove-Item -Recurse -Force $root }
}

# Capture one full pass over $items inside a fresh sandbox.
# Returns array of @{ method; path; body; exit; callLine; status; bytes }.
function Invoke-CapturePass([string]$exeSrc) {
  $sandbox = New-Sandbox $exeSrc
  $results = @()
  try {
    foreach ($it in $items) {
      $bodyFile = $null
      if ($it.body) { $bodyFile = Join-Path $corpusDir $it.body }
      $outFile = Join-Path $sandbox 'bin\__resp__.bin'
      # ALWAYS run the sandboxed copy, never $exeSrc directly: /shortcuts resolves
      # shortcuts/ relative to the exe's parent dir (os.Executable), so running the
      # repo exe would pollute the baseline with the repo's shortcuts/ content.
      $r = Invoke-SettingsCall (Join-Path $sandbox 'bin\settings.exe') (Join-Path $sandbox 'bin') $it.method $it.path $outFile $bodyFile

      $callLine = ''
      if ($r.StdOut -match '(?m)^KEYFLUX_CALL status=(\d+)\s*$') { $callLine = $Matches[0] }
      $status = 0
      if ($r.StdOut -match 'KEYFLUX_CALL status=(\d+)') { $status = [int]$Matches[1] }

      $bytes = $null
      if ((Test-Path $outFile) -and $r.ExitCode -eq 0) {
        $bytes = [IO.File]::ReadAllBytes($outFile)
        Remove-Item -Force $outFile
      }
      $results += @{
        method = $it.method; path = $it.path; body = $it.body
        exit = $r.ExitCode; callLine = $callLine; status = $status; bytes = $bytes
      }
    }
  } finally {
    Remove-Sandbox $sandbox
  }
  return ,$results
}

# Serialize one record to deterministic JSON bytes (hand-rolled, UTF-8 no BOM, LF).
function ConvertTo-BaselineJson([hashtable]$r) {
  $b64 = ''
  if ($r.bytes) { $b64 = [Convert]::ToBase64String($r.bytes) }
  $corpus = 'null'
  if ($r.body) { $corpus = ConvertTo-JsonStringLiteral $r.body }
  return '{' + "`n" +
    '  "method": ' + (ConvertTo-JsonStringLiteral $r.method) + ',' + "`n" +
    '  "path": ' + (ConvertTo-JsonStringLiteral $r.path) + ',' + "`n" +
    '  "corpus": ' + $corpus + ',' + "`n" +
    '  "callLine": ' + (ConvertTo-JsonStringLiteral $r.callLine) + ',' + "`n" +
    '  "status": ' + $r.status + ',' + "`n" +
    '  "bodyBase64": ' + (ConvertTo-JsonStringLiteral $b64) + "`n" +
    '}' + "`n"
}

function Write-Utf8NoBom([string]$path, [string]$text) {
  $dir = Split-Path -Parent $path
  if (!(Test-Path $dir)) { New-Item -ItemType Directory -Force -Path $dir | Out-Null }
  [IO.File]::WriteAllText($path, $text, (New-Object System.Text.UTF8Encoding($false)))
}

# ---------------------------------------------------------------------------
# Modes
# ---------------------------------------------------------------------------

if ($Capture) {
  Write-Host "[capture] exe: $Exe"
  Write-Host '[capture] pass 1 ...'
  $pass1 = Invoke-CapturePass $Exe
  Write-Host '[capture] pass 2 (determinism gate) ...'
  $pass2 = Invoke-CapturePass $Exe

  $drift = @()
  for ($i = 0; $i -lt $items.Count; $i++) {
    $a = $pass1[$i]; $b = $pass2[$i]
    $same = ($a.exit -eq $b.exit) -and ($a.callLine -eq $b.callLine)
    if ($same) {
      if ($null -eq $a.bytes -and $null -eq $b.bytes) { $same = $true }
      elseif ($null -eq $a.bytes -or $null -eq $b.bytes) { $same = $false }
      else { $same = ([Convert]::ToBase64String($a.bytes) -eq [Convert]::ToBase64String($b.bytes)) }
    }
    if (-not $same) { $drift += ('{0} {1} (corpus: {2})' -f $a.method, $a.path, $a.body) }
  }
  if ($drift.Count -gt 0) {
    Write-Host '[capture] NON-DETERMINISTIC OUTPUT, reference NOT written:'
    $drift | ForEach-Object { Write-Host "  DRIFT $_" }
    Write-Host 'API-PARITY: 0/'$items.Count' FAIL [CAPTURE-DRIFT]'
    exit 1
  }
  Write-Host '[capture] two passes byte-identical: OK'

  $failed = @()
  New-Item -ItemType Directory -Force -Path $refGoDir | Out-Null
  foreach ($r in $pass1) {
    $refName = Get-RefName $r.method $r.path $r.body
    if ($r.exit -ne 0 -or $null -eq $r.bytes) {
      $failed += ('{0} {1} (exit={2})' -f $r.method, $r.path, $r.exit)
      continue
    }
    Write-Utf8NoBom (Join-Path $refGoDir $refName) (ConvertTo-BaselineJson $r)
    Write-Host ('  captured {0} -> reference/go/{1} (status={2}, {3} bytes)' -f $r.path, $refName, $r.status, $r.bytes.Length)
  }

  # manifest.json -- index of the frozen baseline (deterministic content, no timestamp).
  $exeHash = (Get-FileHash -Algorithm SHA256 -Path $Exe).Hash.ToLower()
  $mf = '{' + "`n" +
    '  "schema": "api-parity-manifest/1",' + "`n" +
    '  "transport": "Call",' + "`n" +
    '  "sourceExeSha256": ' + (ConvertTo-JsonStringLiteral $exeHash) + ',' + "`n" +
    '  "items": [' + "`n"
  $first = $true
  foreach ($r in $pass1) {
    if (-not $first) { $mf += ',' + "`n" }
    $first = $false
    $corpus = 'null'
    if ($r.body) { $corpus = ConvertTo-JsonStringLiteral $r.body }
    $mf += '    { "method": ' + (ConvertTo-JsonStringLiteral $r.method) +
           ', "path": ' + (ConvertTo-JsonStringLiteral $r.path) +
           ', "corpus": ' + $corpus +
           ', "ref": ' + (ConvertTo-JsonStringLiteral (Get-RefName $r.method $r.path $r.body)) + ' }'
  }
  $mf += "`n" + '  ]' + "`n" + '}' + "`n"
  Write-Utf8NoBom $manifestPath $mf

  if ($failed.Count -gt 0) {
    Write-Host '[capture] FAILED CALLS (reference not written for them):'
    $failed | ForEach-Object { Write-Host "  CALL-FAILED $_" }
    Write-Host 'API-PARITY: 0/'$items.Count' FAIL [CAPTURE-CALL-FAILED]'
    exit 1
  }
  Write-Host ('API-PARITY: {0}/{1} PASS [CAPTURE]' -f $items.Count, $items.Count)
  exit 0
}

# ---- Check mode ----
if (!(Test-Path $manifestPath)) {
  Write-Host "API-PARITY: 0/0 FAIL [manifest not found: $manifestPath -- run -Capture first]"
  exit 1
}
$manifest = Get-Content -Raw -Encoding UTF8 $manifestPath | ConvertFrom-Json

Write-Host "[check] exe: $Exe"
$sandbox = New-Sandbox $Exe
$pass = 0
$missing = @()
$mismatch = @()
try {
  foreach ($item in $manifest.items) {
    $refFile = Join-Path $refGoDir $item.ref
    if (!(Test-Path $refFile)) { $mismatch += ('{0} {1} [reference file missing]' -f $item.method, $item.path); continue }
    $ref = Get-Content -Raw -Encoding UTF8 $refFile | ConvertFrom-Json

    $bodyFile = $null
    if ($item.corpus) { $bodyFile = Join-Path $corpusDir $item.corpus }
    $outFile = Join-Path $sandbox 'bin\__resp__.bin'
    # Run the sandboxed copy of the candidate exe (deploy-tree semantics; /shortcuts
    # resolves shortcuts/ relative to the exe's parent dir, see bridge.go).
    $r = Invoke-SettingsCall (Join-Path $sandbox 'bin\settings.exe') (Join-Path $sandbox 'bin') $item.method $item.path $outFile $bodyFile

    if ($r.ExitCode -ne 0) {
      # Candidate does not implement this endpoint/transport -- expected for Rust-in-progress.
      $missing += ('{0} {1} (exit={2})' -f $item.method, $item.path, $r.ExitCode)
      Write-Host ('  MISSING_ENDPOINT {0} {1}' -f $item.method, $item.path)
      continue
    }
    if ($r.StdOut -notmatch 'KEYFLUX_CALL status=(\d+)') {
      $mismatch += ('{0} {1} [KEYFLUX_CALL status line missing]' -f $item.method, $item.path)
      continue
    }
    $status = [int]$Matches[1]
    $bytes = [IO.File]::ReadAllBytes($outFile)
    Remove-Item -Force $outFile
    $b64 = [Convert]::ToBase64String($bytes)

    if ($status -ne $ref.status) {
      $mismatch += ('{0} {1} [status {2} != baseline {3}]' -f $item.method, $item.path, $status, $ref.status)
      continue
    }
    if ($b64 -ne $ref.bodyBase64) {
      $mismatch += ('{0} {1} [body {2} bytes != baseline {3} bytes]' -f $item.method, $item.path, $bytes.Length, $ref.bodyBase64.Length)
      continue
    }
    $pass++
    Write-Host ('  PASS {0} {1}' -f $item.method, $item.path)
  }
} finally {
  Remove-Sandbox $sandbox
}

$total = $manifest.items.Count
if ($mismatch.Count -gt 0) {
  Write-Host '[check] MISMATCHES:'
  $mismatch | ForEach-Object { Write-Host "  MISMATCH $_" }
}
Write-Host ('MISSING_ENDPOINT count: {0}' -f $missing.Count)
Write-Host ('API-PARITY: {0}/{1} PASS [CHECK]' -f $pass, $total)
if ($mismatch.Count -gt 0) { exit 1 }
# Only MISSING_ENDPOINT (Rust endpoints not implemented yet) is NOT a tool failure.
exit 0
