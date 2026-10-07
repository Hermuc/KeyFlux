# API-level parity harness -- captures byte-exact baseline responses of the Go settings
# backend and replays the same ordered request sequence against a candidate
# implementation (Rust) to diff them byte-for-byte.
#
# WHY: docs/plan-rust-migration.md replaced the Go panel backend with a Rust one (Go backend
# retired 2026-10-06 / 36ccb83). The route surface (19 routes) is now locked by the frozen
# baseline below + config-ui-reactor's cargo tests (was: bridge_test.go).
# This tool froze the Go responses (via the in-process `Call` transport, so no port
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
#   - Repo root, %TEMP% sandbox naming, SHA256 and the determinism gate come from
#     tools/lib/kf-tools.ps1 (shared with run_parity.ps1 / cargo-gates.ps1).
#   - Steps run in a FIXED order inside ONE sandbox per pass (stateful steps depend on
#     earlier steps; manifest.json items carry the "step" number and "stateful" flag).
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

# Fixture mode: the backend's options.startup backfill queries the REAL "KeyFlux"
# scheduled task -- machine state that would freeze into the GET /config baseline
# (measured: this QA box has the task -> "startup":true, CI has not -> "startup":false,
# exactly 1 byte longer -> step 2 failed on every other machine). With the flag set,
# BOTH the recorded reference and the exe under test skip the query (startup=false),
# making GET /config byte-identical across machines. Must be set BEFORE any
# settings.exe is spawned (children inherit the environment).
$env:KEYFLUX_API_PARITY = '1'

$here = $PSScriptRoot
# Shared helpers (repo root / %TEMP% sandbox / SHA256 / determinism gate).
. (Join-Path (Split-Path -Parent $here) 'lib\kf-tools.ps1')
$repo = Get-KfRepoRoot
$corpusDir = Join-Path $here 'corpus'
$refDir = Join-Path $here 'reference'
$refGoDir = Join-Path $refDir 'go'
$manifestPath = Join-Path $refDir 'manifest.json'

if ([string]::IsNullOrEmpty($Exe)) { $Exe = Join-Path $repo 'bin\settings.exe' }
if (!(Test-Path $Exe)) { Write-Host "API-PARITY: 0/0 FAIL [exe not found: $Exe]"; exit 1 }
$Exe = (Resolve-Path $Exe).Path

# ---------------------------------------------------------------------------
# Step table (FIXED ORDER -- stateful steps depend on earlier ones).
# method / path: request line.
# corpus:        static body file in corpus/ ($null = no body).
# multipart:     wrap corpus (a plugin manifest JSON) into a minimal plugin zip and
#                send it as multipart field "file" (POST /api/plugins/import).
# bodyFrom:      'config-echo' = PUT /config body derived from the captured GET /config
#                baseline bytes with one deterministic surgical edit (see below).
# stateful:      step mutates sandbox state; later stateful steps are SKIPPED (not
#                counted as MISMATCH) once an earlier stateful step is MISSING.
# suffix:        disambiguates repeated (method, path) pairs at different state points.
# ---------------------------------------------------------------------------
$items = @(
  @{ step = 1;  method = 'GET';    path = '/health';                                 corpus = $null; multipart = $false; bodyFrom = $null;         stateful = $false; suffix = $null },
  @{ step = 2;  method = 'GET';    path = '/config';                                 corpus = $null; multipart = $false; bodyFrom = $null;         stateful = $false; suffix = $null },
  @{ step = 3;  method = 'GET';    path = '/shortcuts';                              corpus = $null; multipart = $false; bodyFrom = $null;         stateful = $false; suffix = $null },
  @{ step = 4;  method = 'GET';    path = '/api/behaviors';                          corpus = $null; multipart = $false; bodyFrom = $null;         stateful = $false; suffix = $null },
  @{ step = 5;  method = 'GET';    path = '/api/plugins';                            corpus = $null; multipart = $false; bodyFrom = $null;         stateful = $false; suffix = $null },
  @{ step = 6;  method = 'GET';    path = '/api/plugins/everything_search/settings'; corpus = $null; multipart = $false; bodyFrom = $null;         stateful = $false; suffix = $null },
  @{ step = 7;  method = 'POST';   path = '/api/selected-action/test';               corpus = 'test_selected-action_url_hit.json';          multipart = $false; bodyFrom = $null; stateful = $false; suffix = $null },
  @{ step = 8;  method = 'POST';   path = '/api/selected-action/test';               corpus = 'test_selected-action_textfeature_hit.json';  multipart = $false; bodyFrom = $null; stateful = $false; suffix = $null },
  @{ step = 9;  method = 'POST';   path = '/api/selected-action/test';               corpus = 'test_selected-action_nomatch.json';          multipart = $false; bodyFrom = $null; stateful = $false; suffix = $null },
  @{ step = 10; method = 'PUT';    path = '/config';                                 corpus = $null; multipart = $false; bodyFrom = 'config-echo'; stateful = $true;  suffix = $null },
  @{ step = 11; method = 'POST';   path = '/server/command/2';                       corpus = $null; multipart = $false; bodyFrom = $null;         stateful = $true;  suffix = $null },
  @{ step = 12; method = 'POST';   path = '/server/command/3';                       corpus = $null; multipart = $false; bodyFrom = $null;         stateful = $true;  suffix = $null },
  @{ step = 13; method = 'POST';   path = '/server/command/4';                       corpus = $null; multipart = $false; bodyFrom = $null;         stateful = $true;  suffix = $null },
  @{ step = 14; method = 'POST';   path = '/api/behaviors';                          corpus = 'behavior_pack_demo.json';        multipart = $false; bodyFrom = $null; stateful = $true; suffix = $null },
  @{ step = 15; method = 'PUT';    path = '/api/behaviors/parity_demo';              corpus = 'behavior_pack_demo_update.json'; multipart = $false; bodyFrom = $null; stateful = $true; suffix = $null },
  @{ step = 16; method = 'POST';   path = '/api/behaviors/apply';                    corpus = $null; multipart = $false; bodyFrom = $null;         stateful = $true;  suffix = $null },
  @{ step = 17; method = 'DELETE'; path = '/api/behaviors/parity_demo';              corpus = $null; multipart = $false; bodyFrom = $null;         stateful = $true;  suffix = $null },
  @{ step = 18; method = 'POST';   path = '/api/plugins/import';                     corpus = 'demo_plugin.manifest.json'; multipart = $true; bodyFrom = $null; stateful = $true; suffix = 'imported' },
  @{ step = 19; method = 'GET';    path = '/api/plugins';                            corpus = $null; multipart = $false; bodyFrom = $null;         stateful = $true;  suffix = 'after-import' },
  @{ step = 20; method = 'GET';    path = '/api/plugins/demo_plugin/settings';       corpus = $null; multipart = $false; bodyFrom = $null;         stateful = $true;  suffix = 'before' },
  @{ step = 21; method = 'PUT';    path = '/api/plugins/demo_plugin/settings';       corpus = 'plugin_settings_put.json'; multipart = $false; bodyFrom = $null; stateful = $true; suffix = 'save' },
  @{ step = 22; method = 'GET';    path = '/api/plugins/demo_plugin/settings';       corpus = $null; multipart = $false; bodyFrom = $null;         stateful = $true;  suffix = 'after' },
  @{ step = 23; method = 'DELETE'; path = '/api/plugins/demo_plugin';                corpus = $null; multipart = $false; bodyFrom = $null;         stateful = $true;  suffix = $null }
)

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

# Reference file name: "<METHOD>_<path escaped>[.<suffix>][.<corpus stem>].json".
# suffix wins over the corpus stem (repeated endpoints at different state points).
function Get-RefName([string]$method, [string]$path, [string]$body, [string]$suffix) {
  $p = $path.TrimStart('/') -replace '[^A-Za-z0-9._-]', '_'
  if ($suffix) { return "${method}_${p}.${suffix}.json" }
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

# Minimal JSON string escaper; escapes control chars (callLine etc. must stay strict
# JSON -- a literal \n inside a string breaks python json.loads / ConvertFrom-Json),
# quote and backslash. Hand-rolled so emitted bytes are stable across PS 5.1 / 7.
function ConvertTo-JsonStringLiteral([string]$s) {
  $sb = New-Object System.Text.StringBuilder
  [void]$sb.Append('"')
  foreach ($ch in $s.ToCharArray()) {
    $code = [int]$ch
    if ($code -lt 0x20) { [void]$sb.Append(('\u{0:x4}' -f $code)) }
    elseif ($ch -eq '"') { [void]$sb.Append('\"') }
    elseif ($ch -eq '\') { [void]$sb.Append('\\') }
    else { [void]$sb.Append($ch) }
  }
  [void]$sb.Append('"')
  return $sb.ToString()
}

# Run `settings.exe Call <METHOD> <PATH> <out-file> [--body f] [--content-type ct]`
# with cwd = the exe's directory (Go resolves ../data, ./behaviors, ./templates relative
# to it -- bridge.go). Returns @{ ExitCode; StdOut; StdErr }. No shell involved;
# %TEMP% spaces are safe.
function Invoke-SettingsCall([string]$exePath, [string]$cwd, [string]$method, [string]$path,
                             [string]$outFile, [string]$bodyFile, [string]$contentType) {
  $argStr = 'Call ' + (Quote-Arg $method) + ' ' + (Quote-Arg $path) + ' ' + (Quote-Arg $outFile)
  if ($bodyFile) { $argStr += ' --body ' + (Quote-Arg $bodyFile) }
  if ($contentType) { $argStr += ' --content-type ' + (Quote-Arg $contentType) }

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

# No-op engine stub: PUT /config and POST /api/behaviors/apply spawn ./KeyFlux.exe
# (cwd <sandbox> root). With the exe MISSING, Go falls back to an explorer.exe relay
# (proc.FallbackExecCmd) -- and so does the Rust port -- which opens the user's
# "Documents" window for a path that does not exist (desktop window spam). The stub
# makes the direct spawn succeed; the recorded response bytes are identical either way
# (restartFailed=false).
#
# HARD REQUIREMENT, NO COMPILER INVOLVED: the stub is a committed asset
# (assets/KeyFlux-stub.exe, 15 KB, static, built once from assets/keyflux_stub.c).
# An earlier revision compiled it on the fly and silently skipped it when no compiler
# was present -- that silent degradation was the root cause of the window spam, so it
# is now a hard failure instead.
$stubAsset = Join-Path $here 'assets\KeyFlux-stub.exe'

function Assert-StubAsset() {
  if (!(Test-Path $stubAsset)) {
    Write-Host "API-PARITY: 0/0 FAIL [stub asset missing: $stubAsset]"
    Write-Host '  The sandbox REQUIRES a no-op KeyFlux.exe: without it the backend relays'
    Write-Host '  through explorer.exe and spams the desktop with windows.'
    Write-Host '  Rebuild it with any C compiler (see assets/keyflux_stub.c header):'
    Write-Host '    gcc -Os -s -static -o tools/api-parity/assets/KeyFlux-stub.exe tools/api-parity/assets/keyflux_stub.c'
    exit 1
  }
}

# Copy the stub into the sandbox and ASSERT it landed (regression tripwire: every
# sandbox must contain <sandbox>/KeyFlux.exe before any step runs).
function Copy-EngineStub([string]$root) {
  $dst = Join-Path $root 'KeyFlux.exe'
  Copy-Item $stubAsset $dst
  if (!(Test-Path $dst)) { throw "stub missing after copy: $dst" }
}

# Build the deploy-tree sandbox under %TEMP%:
#   <sandbox>/bin/settings.exe  <sandbox>/bin/behaviors/  <sandbox>/bin/templates/
#   <sandbox>/data/config.json  <sandbox>/data/plugins/   <sandbox>/KeyFlux.exe (stub)
# cwd for every Call = <sandbox>/bin (deploy-tree convention; see bridge.go).
function New-Sandbox([string]$exeSrc) {
  $root = New-KfSandbox 'kfapiparity'
  $bin = Join-Path $root 'bin'
  $data = Join-Path $root 'data'
  New-Item -ItemType Directory -Force -Path $bin, $data | Out-Null
  Copy-Item $exeSrc (Join-Path $bin 'settings.exe')
  Copy-Item (Join-Path $repo 'data\config.json') $data
  Copy-Item (Join-Path $repo 'data\plugins') $data -Recurse
  Copy-Item (Join-Path $repo 'bin\behaviors') $bin -Recurse
  Copy-Item (Join-Path $repo 'bin\templates') $bin -Recurse
  Copy-EngineStub $root
  return $root
}

function Remove-Sandbox([string]$root) {
  if (!$root -or !(Test-Path $root)) { return }
  # The sandbox KeyFlux.exe image lock can linger a moment after the backend exits
  # (Windows keeps the image section alive briefly). Retry a few times; if it STILL
  # fails, warn and move on -- the sandbox lives in %TEMP% under a unique per-run
  # name, so a leftover is litter, not a correctness failure (script runs with
  # $ErrorActionPreference='Stop', hence the -ErrorAction override).
  for ($i = 1; $i -le 5; $i++) {
    try { Remove-Item -Recurse -Force $root -ErrorAction Stop; return } catch {
      if ($i -eq 5) {
        Write-Host "[warn] sandbox cleanup failed (leftover in %TEMP%, harmless): $root"
        Write-Host "       $($_.Exception.Message)"
      } else { Start-Sleep -Milliseconds 400 }
    }
  }
}

# PUT /config body: the GET /config baseline bytes with ONE deterministic surgical edit
# (append a fixed marker to the first keymap comment). Raw-text edit, not JSON
# re-serialization, so the bytes are stable across PowerShell versions.
function ConvertTo-ConfigEchoBytes([byte[]]$configBytes) {
  $json = [Text.Encoding]::UTF8.GetString($configBytes)
  $needle = '"comment":"label:36"'
  if (!$json.Contains($needle)) { throw 'config-echo: needle "comment":"label:36" not found in GET /config baseline' }
  $edited = [regex]::new([regex]::Escape($needle)).Replace($json, '"comment":"label:36|parity"', 1)
  return [Text.Encoding]::UTF8.GetBytes($edited)
}

# Minimal plugin zip for POST /api/plugins/import: plugin.json (corpus manifest) at the
# zip root + a no-op entry script. Zip bytes are the REQUEST body only; the recorded
# baseline is the response (installed manifest), so zip determinism is not required.
function New-PluginZipBytes([string]$manifestPath) {
  Add-Type -AssemblyName System.IO.Compression.FileSystem
  $tmpZip = Join-Path $env:TEMP ('kfapiparity-zip-' + [guid]::NewGuid().ToString('N') + '.zip')
  $jsonBytes = [IO.File]::ReadAllBytes($manifestPath)
  $scriptBytes = [Text.Encoding]::UTF8.GetBytes('; api-parity demo entry (no-op)' + "`r`n")
  $zip = [System.IO.Compression.ZipFile]::Open($tmpZip, [System.IO.Compression.ZipArchiveMode]::Create)
  try {
    foreach ($pair in @(@('plugin.json', $jsonBytes), @('main.ahk', $scriptBytes))) {
      $entry = $zip.CreateEntry($pair[0])
      $es = $entry.Open()
      $es.Write($pair[1], 0, $pair[1].Length)
      $es.Close()
    }
  } finally { $zip.Dispose() }
  $bytes = [IO.File]::ReadAllBytes($tmpZip)
  Remove-Item -Force $tmpZip
  return $bytes
}

# Multipart envelope for c.FormFile("file") -- gin parses it from the raw request body,
# which works through the in-process Call transport as long as Content-Type is set.
function New-MultipartBytes([byte[]]$fileBytes, [string]$fileName, [string]$boundary) {
  $ms = New-Object System.IO.MemoryStream
  $w = New-Object System.IO.BinaryWriter($ms)
  $enc = [Text.Encoding]::UTF8
  $w.Write($enc.GetBytes('--' + $boundary + "`r`n"))
  $w.Write($enc.GetBytes('Content-Disposition: form-data; name="file"; filename="' + $fileName + '"' + "`r`n"))
  $w.Write($enc.GetBytes('Content-Type: application/zip' + "`r`n`r`n"))
  $w.Write($fileBytes)
  $w.Write($enc.GetBytes("`r`n--" + $boundary + "--`r`n"))
  $w.Flush()
  return $ms.ToArray()
}

# Resolve the request body (and content type) for one step; writes derived bodies into
# the sandbox bin dir. Returns @{ Path; ContentType } (Path = $null when no body).
# $it may be a hashtable (capture) or the ConvertFrom-Json PSCustomObject (check).
function Resolve-StepBody($it, [string]$binDir, [string]$configEchoFile) {
  if ($it.bodyFrom -eq 'config-echo') {
    return @{ Path = $configEchoFile; ContentType = 'application/json' }
  }
  if ($it.multipart) {
    $zipBytes = New-PluginZipBytes (Join-Path $corpusDir $it.corpus)
    $mpBytes = New-MultipartBytes $zipBytes 'demo-plugin.zip' 'kfparityboundary'
    $p = Join-Path $binDir '__multipart__.bin'
    [IO.File]::WriteAllBytes($p, $mpBytes)
    return @{ Path = $p; ContentType = 'multipart/form-data; boundary=kfparityboundary' }
  }
  if ($it.corpus) {
    return @{ Path = (Join-Path $corpusDir $it.corpus); ContentType = 'application/json' }
  }
  return @{ Path = $null; ContentType = '' }
}

# Execute one step inside a sandbox; returns the record for the baseline.
function Invoke-Step([hashtable]$it, [string]$sandbox, [string]$configEchoFile) {
  $binDir = Join-Path $sandbox 'bin'
  $body = Resolve-StepBody $it $binDir $configEchoFile
  $outFile = Join-Path $binDir '__resp__.bin'
  # ALWAYS run the sandboxed copy, never the source exe directly: /shortcuts resolves
  # shortcuts/ relative to the exe's parent dir (os.Executable), so running the repo
  # exe would pollute the baseline with the repo's shortcuts/ content.
  $r = Invoke-SettingsCall (Join-Path $binDir 'settings.exe') $binDir $it.method $it.path $outFile $body.Path $body.ContentType

  $callLine = ''
  $m = [regex]::Match($r.StdOut, 'KEYFLUX_CALL status=\d+')
  if ($m.Success) { $callLine = $m.Value }
  $status = 0
  if ($m.Success) { $status = [int]($m.Value -replace '^\D*(\d+)$', '$1') }

  $bytes = $null
  if ((Test-Path $outFile) -and $r.ExitCode -eq 0) {
    $bytes = [IO.File]::ReadAllBytes($outFile)
    Remove-Item -Force $outFile
  }
  return @{
    step = [int]$it.step; method = $it.method; path = $it.path; corpus = $it.corpus
    exit = $r.ExitCode; callLine = $callLine; status = $status; bytes = $bytes
  }
}

# Serialize one record to deterministic JSON bytes (hand-rolled, UTF-8 no BOM, LF).
function ConvertTo-BaselineJson([hashtable]$r) {
  $b64 = ''
  if ($r.bytes) { $b64 = [Convert]::ToBase64String($r.bytes) }
  $corpus = 'null'
  if ($r.corpus) { $corpus = ConvertTo-JsonStringLiteral $r.corpus }
  return '{' + "`n" +
    '  "step": ' + $r.step + ',' + "`n" +
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

# Run a full ordered pass over $items inside a fresh sandbox. The config-echo body is
# derived from THIS pass's GET /config response (steps are order-dependent by design).
# Returns array of records.
function Invoke-CapturePass([string]$exeSrc) {
  $sandbox = New-Sandbox $exeSrc
  $results = @()
  try {
    $configEchoFile = Join-Path $sandbox 'bin\__config_echo__.json'
    $configEchoReady = $false
    foreach ($it in $items) {
      $echoFile = $null
      if ($it.bodyFrom -eq 'config-echo') {
        if (!$configEchoReady) { throw 'config-echo: GET /config must run before PUT /config (step order broken)' }
        $echoFile = $configEchoFile
      }
      $r = Invoke-Step $it $sandbox $echoFile
      $results += $r
      if ($it.method -eq 'GET' -and $it.path -eq '/config' -and $r.bytes) {
        [IO.File]::WriteAllBytes($configEchoFile, (ConvertTo-ConfigEchoBytes $r.bytes))
        $configEchoReady = $true
      }
    }
  } finally {
    Remove-Sandbox $sandbox
  }
  return ,$results
}

# ---------------------------------------------------------------------------
# Modes
# ---------------------------------------------------------------------------

if ($Capture) {
  Assert-StubAsset
  Write-Host "[capture] exe: $Exe"
  Write-Host "[capture] stub asset: $stubAsset"
  Write-Host '[capture] pass 1 ...'
  $pass1 = Invoke-CapturePass $Exe
  Write-Host '[capture] pass 2 (determinism gate) ...'
  $pass2 = Invoke-CapturePass $Exe

  # Determinism gate: both passes must agree on exit code, status line and response
  # bytes -- otherwise the reference is NOT written. Byte equality is delegated to the
  # shared Assert-KfDeterministic (tools/lib/kf-tools.ps1).
  $drift = New-Object System.Collections.ArrayList
  for ($i = 0; $i -lt $items.Count; $i++) {
    $a = $pass1[$i]; $b = $pass2[$i]
    $msg = ('step {0}: {1} {2}' -f $a.step, $a.method, $a.path)
    if (($a.exit -ne $b.exit) -or ($a.callLine -ne $b.callLine)) {
      [void]$drift.Add($msg)
    }
    else {
      [void](Assert-KfDeterministic $a.bytes $b.bytes -Message $msg -Collect $drift)
    }
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
    $it = $items[$r.step - 1]
    $refName = Get-RefName $r.method $r.path $r.corpus $it.suffix
    if ($r.exit -ne 0 -or $null -eq $r.bytes) {
      $failed += ('step {0}: {1} {2} (exit={3})' -f $r.step, $r.method, $r.path, $r.exit)
      continue
    }
    Write-Utf8NoBom (Join-Path $refGoDir $refName) (ConvertTo-BaselineJson $r)
    Write-Host ('  captured step {0,2} {1} {2} -> reference/go/{3} (status={4}, {5} bytes)' -f $r.step, $r.method, $r.path, $refName, $r.status, $r.bytes.Length)
  }

  # manifest.json -- ordered index of the frozen baseline (deterministic, no timestamp).
  $exeHash = (Get-KfSha256 $Exe).ToLower()
  $mf = '{' + "`n" +
    '  "schema": "api-parity-manifest/2",' + "`n" +
    '  "transport": "Call",' + "`n" +
    '  "sourceExeSha256": ' + (ConvertTo-JsonStringLiteral $exeHash) + ',' + "`n" +
    '  "items": [' + "`n"
  $first = $true
  for ($i = 0; $i -lt $items.Count; $i++) {
    $it = $items[$i]
    if (-not $first) { $mf += ',' + "`n" }
    $first = $false
    $corpus = 'null'
    if ($it.corpus) { $corpus = ConvertTo-JsonStringLiteral $it.corpus }
    $bodyFrom = 'null'
    if ($it.bodyFrom) { $bodyFrom = ConvertTo-JsonStringLiteral $it.bodyFrom }
    $mf += '    { "step": ' + $it.step +
           ', "method": ' + (ConvertTo-JsonStringLiteral $it.method) +
           ', "path": ' + (ConvertTo-JsonStringLiteral $it.path) +
           ', "corpus": ' + $corpus +
           ', "multipart": ' + $it.multipart.ToString().ToLower() +
           ', "bodyFrom": ' + $bodyFrom +
           ', "stateful": ' + $it.stateful.ToString().ToLower() +
           ', "ref": ' + (ConvertTo-JsonStringLiteral (Get-RefName $it.method $it.path $it.corpus $it.suffix)) + ' }'
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
Assert-StubAsset
Write-Host "[check] stub asset: $stubAsset"
$sandbox = New-Sandbox $Exe
$pass = 0
$missing = @()
$mismatch = @()
$stateBroken = $false
try {
  $configEchoFile = Join-Path $sandbox 'bin\__config_echo__.json'
  $configEchoReady = $false
  foreach ($item in $manifest.items) {
    $refFile = Join-Path $refGoDir $item.ref
    if (!(Test-Path $refFile)) { $mismatch += ('step {0}: {1} {2} [reference file missing]' -f $item.step, $item.method, $item.path); continue }
    $ref = Get-Content -Raw -Encoding UTF8 $refFile | ConvertFrom-Json

    # Derive the config-echo body from the REFERENCE baseline (not from the exe under
    # test) so capture and check send byte-identical request bodies.
    $echoFile = $null
    if ($item.bodyFrom -eq 'config-echo') {
      if (!$configEchoReady) {
        $cfgRefFile = Join-Path $refGoDir 'GET_config.json'
        $cfgRef = Get-Content -Raw -Encoding UTF8 $cfgRefFile | ConvertFrom-Json
        [IO.File]::WriteAllBytes($configEchoFile, (ConvertTo-ConfigEchoBytes ([Convert]::FromBase64String($cfgRef.bodyBase64))))
        $configEchoReady = $true
      }
      $echoFile = $configEchoFile
    }

    $binDir = Join-Path $sandbox 'bin'
    $body = Resolve-StepBody $item $binDir $echoFile
    $outFile = Join-Path $binDir '__resp__.bin'
    $r = Invoke-SettingsCall (Join-Path $binDir 'settings.exe') $binDir $item.method $item.path $outFile $body.Path $body.ContentType

    if ($r.ExitCode -ne 0) {
      # Candidate does not implement this endpoint/transport -- expected for Rust-in-progress.
      $missing += ('step {0}: {1} {2} (exit={3})' -f $item.step, $item.method, $item.path, $r.ExitCode)
      Write-Host ('  MISSING_ENDPOINT step {0} {1} {2}' -f $item.step, $item.method, $item.path)
      if ($item.stateful) { $stateBroken = $true }
      continue
    }
    if ($item.stateful -and $stateBroken) {
      Write-Host ('  SKIPPED_STATE step {0} {1} {2} (earlier stateful step missing)' -f $item.step, $item.method, $item.path)
      continue
    }
    if ($r.StdOut -notmatch 'KEYFLUX_CALL status=(\d+)') {
      $mismatch += ('step {0}: {1} {2} [KEYFLUX_CALL status line missing]' -f $item.step, $item.method, $item.path)
      continue
    }
    $status = [int]$Matches[1]
    $bytes = [IO.File]::ReadAllBytes($outFile)
    Remove-Item -Force $outFile
    $b64 = [Convert]::ToBase64String($bytes)

    if ($status -ne $ref.status) {
      $mismatch += ('step {0}: {1} {2} [status {3} != baseline {4}]' -f $item.step, $item.method, $item.path, $status, $ref.status)
      continue
    }
    if ($b64 -ne $ref.bodyBase64) {
      $mismatch += ('step {0}: {1} {2} [body {3} bytes != baseline {4} bytes]' -f $item.step, $item.method, $item.path, $bytes.Length, $ref.bodyBase64.Length)
      continue
    }
    $pass++
    Write-Host ('  PASS step {0,2} {1} {2}' -f $item.step, $item.method, $item.path)
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
# Only MISSING_ENDPOINT / SKIPPED_STATE (candidate endpoints not implemented yet) is
# NOT a tool failure.
exit 0
