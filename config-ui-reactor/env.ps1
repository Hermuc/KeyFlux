# Build environment for config-ui-reactor (KeyFlux settings panel).
#
# WHY THIS EXISTS:
#   rustc's Visual Studio auto-detection fails on this machine: VS Community 2026
#   ("Microsoft Visual Studio\18") does not register the component id that
#   `vswhere -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64` looks for,
#   so rustc cannot locate link.exe or the Windows SDK libs. Symptoms, in order:
#     error: linker `link.exe` not found
#     LINK : fatal error LNK1158: cannot run 'mt.exe'      (mt.exe lives in the SDK bin dir)
#   Everything below is required; omitting any line reproduces one of those errors.
#
# NOTE: keep this file ASCII-only (PowerShell 5.1 would mis-decode non-BOM UTF-8).

# NOTE: do NOT set $ErrorActionPreference='Stop' here - cargo writes progress to
# stderr, which PowerShell would then treat as a terminating error.

# Rust home/caches on D: to honour the "C: read-only" rule.
$env:RUSTUP_HOME  = 'D:\PortableApps\rust\rustup'
$env:CARGO_HOME   = 'D:\PortableApps\rust\cargo'
# windows-reactor-setup caches NuGet packages under %LOCALAPPDATA% by default.
$env:LOCALAPPDATA = 'D:\PortableApps\cache\LocalAppData'

$msvc = 'C:\Program Files\Microsoft Visual Studio\18\Community\VC\Tools\MSVC\14.44.35207'
$sdk  = 'C:\Program Files (x86)\Windows Kits\10'
$sdkv = '10.0.28000.0'

foreach ($p in @(
    "$msvc\bin\HostX64\x64\link.exe",
    "$sdk\bin\$sdkv\x64\mt.exe",
    "$sdk\Lib\$sdkv\um\x64\kernel32.lib"
)) {
    if (-not (Test-Path $p)) { throw "missing toolchain path: $p" }
}

$env:PATH    = "$msvc\bin\HostX64\x64;$sdk\bin\$sdkv\x64;$env:CARGO_HOME\bin;$env:PATH"
$env:LIB     = "$msvc\lib\x64;$sdk\Lib\$sdkv\ucrt\x64;$sdk\Lib\$sdkv\um\x64"
$env:INCLUDE = "$msvc\include;$sdk\Include\$sdkv\ucrt;$sdk\Include\$sdkv\shared;$sdk\Include\$sdkv\um;$sdk\Include\$sdkv\winrt"

Write-Host "[env] MSVC + Windows SDK toolchain ready (rustc 1.98.1, MSVC 14.44.35207, SDK $sdkv)"
