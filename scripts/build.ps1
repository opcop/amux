#
# AMUX Build Script — Windows (PowerShell)
#
# Usage:
#   .\scripts\build.ps1              # Debug build
#   .\scripts\build.ps1 -Release     # Release build (optimized)
#   .\scripts\build.ps1 -Package     # Build + create distributable zip
#
param(
    [switch]$Release,
    [switch]$Package
)

$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$ProjectDir = Split-Path -Parent $ScriptDir

# Read version
$CargoToml = Get-Content "$ProjectDir\Cargo.toml" -Raw
if ($CargoToml -match 'version\s*=\s*"([^"]+)"') {
    $Version = $Matches[1]
} else {
    $Version = "0.1.0"
}

$Arch = if ([Environment]::Is64BitOperatingSystem) { "x86_64" } else { "x86" }

Write-Host "=== AMUX Build ===" -ForegroundColor Cyan
Write-Host "Platform: windows ($Arch)"
Write-Host "Version:  $Version"
Write-Host ""

# Check Rust toolchain
Write-Host "--- Checking dependencies ---"
try {
    $null = & rustc --version 2>&1
    Write-Host "Rust: $(rustc --version)"
} catch {
    Write-Host "Rust not found! Install from https://rustup.rs" -ForegroundColor Red
    exit 1
}

# Check for Visual Studio Build Tools (needed for GPUI/DirectX)
$VsWhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
if (Test-Path $VsWhere) {
    $VsPath = & $VsWhere -latest -property installationPath 2>$null
    if ($VsPath) {
        Write-Host "Visual Studio: $VsPath"
    }
} else {
    Write-Host "Warning: Visual Studio Build Tools may be required for GPUI" -ForegroundColor Yellow
}

# Build
Write-Host ""
Write-Host "--- Building ---"
Set-Location $ProjectDir

if ($Package) { $Release = $true }

$Features = "gpui"
if ($Release) {
    & cargo build -p amux-desktop --features $Features --release
    $Binary = "target\release\amux-desktop.exe"
} else {
    & cargo build -p amux-desktop --features $Features
    $Binary = "target\debug\amux-desktop.exe"
}

if (-not (Test-Path $Binary)) {
    Write-Host "Build failed!" -ForegroundColor Red
    exit 1
}

$BinarySize = (Get-Item $Binary).Length / 1MB
Write-Host ""
Write-Host "Binary: $Binary ($([math]::Round($BinarySize, 1)) MB)"

# Package
if ($Package) {
    Write-Host ""
    Write-Host "--- Packaging ---"

    $DistDir = "$ProjectDir\dist"
    $DistName = "amux-$Version-windows-$Arch"
    $StageDir = "$DistDir\$DistName"

    # Clean and create
    if (Test-Path $StageDir) { Remove-Item -Recurse -Force $StageDir }
    New-Item -ItemType Directory -Path $StageDir -Force | Out-Null

    # Copy files
    Copy-Item $Binary "$StageDir\amux.exe"
    Copy-Item -Recurse "$ProjectDir\assets\icons" "$StageDir\icons"

    # Create README
    @"
AMUX - Terminal Multiplexer for Vibe Coding
============================================

Run: amux.exe

Requirements:
- Windows 10 (1903+) or Windows 11
- For WSL tools: WSL2 installed
- Recommended font: Cascadia Code

First time:
1. Double-click amux.exe
2. Right-click in terminal for options
3. Ctrl+D to split panes
4. Ctrl+P for command palette
"@ | Set-Content "$StageDir\README.txt"

    # Create zip
    $ZipPath = "$DistDir\$DistName.zip"
    if (Test-Path $ZipPath) { Remove-Item $ZipPath }
    Compress-Archive -Path "$StageDir\*" -DestinationPath $ZipPath

    Write-Host ""
    Write-Host "=== Package ready ===" -ForegroundColor Green
    Write-Host $ZipPath
    Write-Host "Size: $([math]::Round((Get-Item $ZipPath).Length / 1MB, 1)) MB"
}

Write-Host ""
Write-Host "=== Done ===" -ForegroundColor Green
