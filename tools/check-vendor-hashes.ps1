# check-vendor-hashes.ps1 - guard vendored third-party content against silent edits (report #13).
#
# WHY THIS EXISTS:
#   Several tracked files are THIRD-PARTY (an AHK class, the AHK runtime, a utility, and a forked
#   crate). They look like our own source, so a good-faith "fix" of an upstream typo -- or deleting
#   a branch that looks like dead code -- is easy and leaves no trace. This script pins each one to
#   a SHA256 recorded in tools/vendor-manifest.json; any drift fails the gate, forcing an explicit,
#   documented update. Inventory + rationale: vendor/README.md.
#
# WHAT IS GUARDED: the $targets table below (single source for "what counts as vendored").
#   kind = file  -> SHA256 of the file's bytes.
#   kind = tree  -> SHA256 over "<relpath>\n<file-sha256>\n" for every file (sorted by relpath),
#                   minus `exclude` (paths that are OURS, not upstream -- e.g. PATCHES.md).
#
# USAGE:
#   pwsh -File tools/check-vendor-hashes.ps1          # verify (CI job / `make check-vendor`)
#   pwsh -File tools/check-vendor-hashes.ps1 -Write   # regenerate the manifest after an intended change
#
# Exit 0 = every target matches; non-zero = drift (or a target is missing).
# ASCII-only on purpose (pwsh -File misparses non-BOM UTF-8, repo convention).

param([switch]$Write)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'lib/kf-tools.ps1')
$repo = Get-KfRepoRoot
$manifestPath = Join-Path $PSScriptRoot 'vendor-manifest.json'

# ---- what is vendored (add entries here; then run -Write) -------------------
$targets = @(
    @{ key = 'file:bin/lib/Monitor.ahk';                    kind = 'file'; path = 'bin/lib/Monitor.ahk' },
    @{ key = 'file:bin/AutoHotkey64.exe';                   kind = 'file'; path = 'bin/AutoHotkey64.exe' },
    @{ key = 'file:bin/SoundControl.exe';                   kind = 'file'; path = 'bin/SoundControl.exe' },
    @{ key = 'file:tools/Rexplorer_x64.exe';                kind = 'file'; path = 'tools/Rexplorer_x64.exe' },
    @{ key = 'tree:config-ui-reactor/vendor/windows-reactor'; kind = 'tree'
       path = 'config-ui-reactor/vendor/windows-reactor'
       # PATCHES.md is OURS (the patch record) and .cargo-ok is cargo's vendoring marker;
       # neither is upstream content, so they must not perturb the tree hash.
       exclude = @('PATCHES.md', '.cargo-ok') }
)

function Get-FileSha256([string]$full) {
    return (Get-FileHash -Algorithm SHA256 -LiteralPath $full).Hash
}

function Get-TreeSha256([string]$dir, [string[]]$exclude) {
    $prefix = $dir.TrimEnd('\', '/').Length + 1
    $entries = Get-ChildItem -LiteralPath $dir -Recurse -File |
        ForEach-Object { [pscustomobject]@{ rel = $_.FullName.Substring($prefix).Replace('\', '/'); full = $_.FullName } } |
        Where-Object { $exclude -notcontains $_.rel } |
        Sort-Object rel
    $sb = New-Object System.Text.StringBuilder
    foreach ($e in $entries) {
        [void]$sb.Append($e.rel).Append("`n").Append((Get-FileSha256 $e.full)).Append("`n")
    }
    $sha = [System.Security.Cryptography.SHA256]::Create()
    $bytes = [System.Text.Encoding]::UTF8.GetBytes($sb.ToString())
    return (([System.BitConverter]::ToString($sha.ComputeHash($bytes))) -replace '-', '')
}

function Get-TargetHash($t) {
    $full = Join-Path $repo $t.path
    if ($t.kind -eq 'tree') {
        if (!(Test-Path -LiteralPath $full)) { throw "missing vendored tree: $($t.path)" }
        return (Get-TreeSha256 $full @($t.exclude))
    }
    if (!(Test-Path -LiteralPath $full)) { throw "missing vendored file: $($t.path)" }
    return (Get-FileSha256 $full)
}

# ---- -Write: regenerate the manifest ---------------------------------------
if ($Write) {
    $map = [ordered]@{}
    foreach ($t in ($targets | Sort-Object { $_.key })) { $map[$t.key] = (Get-TargetHash $t) }
    $json = ($map | ConvertTo-Json -Depth 4)
    [IO.File]::WriteAllText($manifestPath, $json + "`n", (New-Object System.Text.UTF8Encoding($false)))
    Write-Host "[ok] wrote $($map.Count) vendor hash(es) -> tools/vendor-manifest.json"
    exit 0
}

# ---- verify ----------------------------------------------------------------
if (!(Test-Path -LiteralPath $manifestPath)) {
    Write-Host "[FAIL] missing tools/vendor-manifest.json -- run: pwsh -File tools/check-vendor-hashes.ps1 -Write"
    exit 1
}
$recorded = Get-Content -Raw -LiteralPath $manifestPath | ConvertFrom-Json
$failed = 0
foreach ($t in $targets) {
    $want = $recorded.($t.key)
    if ($null -eq $want) { Write-Host "[FAIL] $($t.key) not in manifest -- re-run -Write"; $failed++; continue }
    $got = Get-TargetHash $t
    if ($got -ne $want) {
        Write-Host "[FAIL] $($t.key) DRIFTED (recorded=$($want.Substring(0,12)) now=$($got.Substring(0,12)))"
        $failed++
    } else {
        Write-Host "  [ok] $($t.key)"
    }
}
# manifest rot: a recorded key no longer guarded
foreach ($p in $recorded.PSObject.Properties) {
    if (-not ($targets.key -contains $p.Name)) { Write-Host "[FAIL] stale manifest key '$($p.Name)' -- remove it"; $failed++ }
}
if ($failed -gt 0) {
    Write-Host "[FAIL] vendor hash guard: $failed problem(s)."
    Write-Host '       If the change was intended, update vendor/README.md and run -Write.'
    exit 1
}
Write-Host '[ok] vendored content unchanged'
exit 0