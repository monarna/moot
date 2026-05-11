# Stop Moot server (Windows)

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location $ScriptDir

$PidFile = "moot.pid"
if (Test-Path $PidFile) {
    $PID = Get-Content $PidFile
    $process = Get-Process -Id $PID -ErrorAction SilentlyContinue
    if ($process) {
        Write-Host "Stopping Moot server (PID: $PID)..."
        Stop-Process -Id $PID
        Remove-Item $PidFile
        Write-Host "Server stopped"
    } else {
        Write-Host "Server not running (stale PID file)"
        Remove-Item $PidFile
    }
} else {
    Write-Host "No PID file found. Use Task Manager to find and kill moot.exe"
}
