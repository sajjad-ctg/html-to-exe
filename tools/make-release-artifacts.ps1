# Assembles the GitHub release artifacts (see docs/release-checklist.md).
# Expects `npm run tauri build` to have completed successfully first.
# Usage: tools\make-release-artifacts.ps1 [-OutDir <dir>]
param(
    [string]$OutDir = "$env:TEMP\htx-release"
)
$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.IO.Compression.FileSystem

$repoRoot = Split-Path -Path $PSScriptRoot -Parent
$version  = (Get-Content (Join-Path $repoRoot "package.json") -Raw | ConvertFrom-Json).version
$target   = Join-Path $repoRoot "src-tauri\target\release"

$gui = Join-Path $target "html-to-exe-builder.exe"
$cli = Join-Path $target "htx.exe"
if (-not (Test-Path $gui)) { throw "GUI exe missing at $gui — run 'npm run tauri build' first" }
if (-not (Test-Path $cli)) { throw "CLI exe missing at $cli — run 'cargo build --release' in src-tauri first" }

$setup = Get-ChildItem (Join-Path $target "bundle\nsis") -Filter "html-to-exe_${version}_x64-setup.exe" -ErrorAction SilentlyContinue
if (-not $setup) { throw "installer for $version missing under bundle\nsis — wrong version number?" }
$msi = Get-ChildItem (Join-Path $target "bundle\msi") -Filter "html-to-exe_${version}_x64_en-US.msi" -ErrorAction SilentlyContinue
if (-not $msi) { throw "MSI for $version missing under bundle\msi" }

New-Item -ItemType Directory -Force $OutDir | Out-Null

Copy-Item $setup.FullName (Join-Path $OutDir "html-to-exe_${version}_x64-setup.exe") -Force
Copy-Item $msi.FullName   (Join-Path $OutDir "html-to-exe_${version}_x64_en-US.msi") -Force
Copy-Item $cli            (Join-Path $OutDir "htx-x86_64-pc-windows-msvc.exe") -Force

# Portable = just the GUI exe zipped at the root (v0.1.0 layout).
$zip = Join-Path $OutDir "html-to-exe_${version}_x64_portable.zip"
if (Test-Path $zip) { Remove-Item $zip -Force }
Compress-Archive -Path $gui -DestinationPath $zip -Force

Get-ChildItem $OutDir | Select-Object Name, Length
