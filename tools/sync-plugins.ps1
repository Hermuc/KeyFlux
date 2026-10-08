# sync-plugins.ps1 - Sync bundled (official) plugins into <OutDir>/data/plugins,
# then honor the tombstone list (config.options.plugins.removed, 2026-10-02 P4):
# any removed bundled plugin directory is deleted after the copy so that
# "user deleted" wins over "bundled ships it".
#
# Replaces the inline robocopy in Makefile sync-plugins (same copy semantics:
# robocopy WITHOUT /MIR - only adds/updates, never touches user-imported plugins).

param(
    [Parameter(Mandatory = $true)]
    [string]$OutDir
)

$ErrorActionPreference = 'Stop'

# Repo root via the shared helper (single source; also honors $env:KEYFLUX_REPO_ROOT
# like the parity/deploy tools -- this script is invoked from make, so its cwd is not
# guaranteed to be the repo root).
. (Join-Path $PSScriptRoot 'lib/kf-tools.ps1')
$repo = Get-KfRepoRoot
$src = Join-Path $repo 'plugins\examples'
$dst = Join-Path $OutDir 'data\plugins'

if (-not (Test-Path $src)) {
    Write-Error "[FAIL] bundled plugin source not found: $src"
    exit 1
}
New-Item -ItemType Directory -Force -Path $dst | Out-Null

# robocopy exit codes 0-7 are success (bit flags); >= 8 is a failure.
$robocopy = Start-Process -FilePath robocopy.exe -ArgumentList @(
    $src, $dst, '/E', '/NFL', '/NDL', '/NJH', '/NJS'
) -Wait -PassThru -NoNewWindow
if (!(Test-KfRobocopyOk $robocopy.ExitCode)) {
    Write-Error "[FAIL] robocopy exit $($robocopy.ExitCode)"
    exit 1
}

# --- Tombstone pass: remove bundled plugins the user has deleted. ---
$configPath = Join-Path $OutDir 'data\config.json'
if (-not (Test-Path $configPath)) {
    Write-Host '[ok] sync-plugins done (no config.json, no tombstones)'
    exit 0
}

$removed = $null
try {
    $config = Get-Content $configPath -Raw -Encoding UTF8 | ConvertFrom-Json
    if ($config.options -and $config.options.plugins -and $config.options.plugins.removed) {
        $removed = @($config.options.plugins.removed)
    }
} catch {
    Write-Host "[warn] config.json unreadable, tombstones skipped: $_"
    exit 0
}

foreach ($id in $removed) {
    if ([string]::IsNullOrWhiteSpace($id)) { continue }
    $dir = Join-Path $dst $id
    if (Test-Path $dir) {
        Remove-Item $dir -Recurse -Force -Confirm:$false
        Write-Host "[tombstone] removed bundled plugin: $id"
    }
}

Write-Host '[ok] sync-plugins done'
