# Moot Windows Installer
# Downloads Tor Expert Bundle, extracts tor.exe, builds moot, sets up data dir

$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location $ScriptDir

Write-Host "=== Moot Windows Installer ==="
Write-Host ""

# 1. Check for Rust
if (-not (Get-Command "cargo" -ErrorAction SilentlyContinue)) {
    Write-Host "Error: Rust is not installed."
    Write-Host "Install from: https://rustup.rs"
    exit 1
}

# 2. Build moot
Write-Host "[1/3] Building Moot..."
cargo build --release
if (-not (Test-Path "target\release\moot.exe")) {
    Write-Host "Error: Build failed"
    exit 1
}
Write-Host "  moot.exe built successfully"
Write-Host ""

# 3. Download and extract Tor Expert Bundle
$TorVersion = "14.0.9"
$TorUrl = "https://dist.torproject.org/torbrowser/$TorVersion/tor-win64-$TorVersion.tar.gz"
$TorArchive = "$env:TEMP\tor-win64.tar.gz"

Write-Host "[2/3] Downloading Tor Expert Bundle ($TorVersion)..."
try {
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
    Invoke-WebRequest -Uri $TorUrl -OutFile $TorArchive -UseBasicParsing
} catch {
    Write-Host "  Download failed. Trying alternative URL..."
    $TorUrl = "https://archive.torproject.org/tor-package-archive/torbrowser/$TorVersion/tor-win64-$TorVersion.tar.gz"
    Invoke-WebRequest -Uri $TorUrl -OutFile $TorArchive -UseBasicParsing
}

Write-Host "  Extracting tor.exe..."
# tar is built into Windows 10+ and PowerShell 7+
if (Get-Command "tar" -ErrorAction SilentlyContinue) {
    tar -xzf $TorArchive -C "$env:TEMP\tor-extract" 2>$null
    # Find tor.exe in extracted folder
    $torExe = Get-ChildItem -Path "$env:TEMP\tor-extract" -Recurse -Filter "tor.exe" | Select-Object -First 1
    if ($torExe) {
        Copy-Item $torExe.FullName -Destination "target\release\tor.exe" -Force
        Write-Host "  tor.exe extracted to target\release\tor.exe"
    }
} else {
    Write-Host "  tar not found. Manual step required:"
    Write-Host "  Download from: $TorUrl"
    Write-Host "  Extract and place tor.exe next to moot.exe"
}

# Cleanup temp
Remove-Item $TorArchive -ErrorAction SilentlyContinue
Remove-Item "$env:TEMP\tor-extract" -Recurse -ErrorAction SilentlyContinue
Write-Host ""

# 4. Create data directory
Write-Host "[3/3] Creating data directories..."
$DataDir = "$env:APPDATA\moot"
New-Item -ItemType Directory -Path "$DataDir\tor_data" -Force | Out-Null
New-Item -ItemType Directory -Path "$DataDir\hidden_service" -Force | Out-Null
New-Item -ItemType Directory -Path "$DataDir\db" -Force | Out-Null
Write-Host "  Data dir: $DataDir"
Write-Host ""

Write-Host "=== Installation Complete ==="
Write-Host ""
Write-Host "Run Moot with:"
Write-Host "  .\run.ps1"
Write-Host ""
Write-Host "Or manually:"
Write-Host "  target\release\moot.exe --data-dir ""$DataDir"""
Write-Host ""
Write-Host "To run without Tor:"
Write-Host "  target\release\moot.exe --gateway"
