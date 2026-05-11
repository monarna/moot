# Moot server startup script (Windows)

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location $ScriptDir

# Build if needed
if (-not (Test-Path "target\release\moot.exe")) {
    Write-Host "Building Moot in release mode..."
    cargo build --release
}

# Run in background
Write-Host "Starting Moot server..."
$process = Start-Process -FilePath "target\release\moot.exe" -NoNewWindow -PassThru -RedirectStandardOutput "moot.log" -RedirectStandardError "moot.log"

# Save PID
$process.Id | Out-File -FilePath "moot.pid"
Write-Host "Moot server started (PID: $($process.Id))"
Write-Host "Server running at http://127.0.0.1:8080"
Write-Host "Logs: moot.log"
