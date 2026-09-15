<#
.SYNOPSIS
    Starts all Tether (screentime) services in dev mode (Agent -> Session -> Tauri UI).

.DESCRIPTION
    1. Sets SCREENTIME_DATA_DIR to local\data (isolated dev database & logs).
    2. Cleans up any dangling dev agent/session processes.
    3. Builds st-agent and st-session (unless -NoBuild).
    4. Starts screentime-agent and waits for \\.\pipe\screentime to become ready.
    5. Starts screentime-session sampling front.
    6. Launches Tauri UI development server (npm run tauri dev).
    7. On exit (window closed or Ctrl+C), cleanly terminates agent and session processes.

.PARAMETER Headless
    Run agent and session hidden without spawning separate console windows.

.PARAMETER NoBuild
    Skip cargo build check and immediately run existing target\debug binaries.

.PARAMETER BackendOnly
    Start only the backend services (Agent + Session) without launching the Tauri UI.

.EXAMPLE
    .\dev.ps1
    .\dev.ps1 -Headless
    .\dev.ps1 -BackendOnly
#>
[CmdletBinding()]
param(
    [switch]$Headless,
    [switch]$NoBuild,
    [switch]$BackendOnly
)

$ErrorActionPreference = "Stop"
$RepoRoot = $PSScriptRoot
$DataDir = Join-Path $RepoRoot "local\data"
$LogsDir = Join-Path $DataDir "logs"

Write-Host ""
Write-Host "==========================================================" -ForegroundColor Cyan
Write-Host "  Tether (Screentime) Unified Dev Environment" -ForegroundColor Cyan
Write-Host "==========================================================" -ForegroundColor Cyan
Write-Host "Data Directory: $DataDir" -ForegroundColor Gray

# Ensure local data directory and logs exist
if (!(Test-Path $DataDir)) {
    New-Item -ItemType Directory -Path $DataDir -Force | Out-Null
}
if (!(Test-Path $LogsDir)) {
    New-Item -ItemType Directory -Path $LogsDir -Force | Out-Null
}

$env:SCREENTIME_DATA_DIR = $DataDir

# 1. Clean up stale dev processes
Write-Host "`n[1/4] Checking for existing dev processes..." -ForegroundColor Yellow
$staleNames = @("screentime-agent", "screentime-session", "screentime-ui")
foreach ($name in $staleNames) {
    $existing = Get-Process -Name $name -ErrorAction SilentlyContinue
    if ($existing) {
        Write-Host "  Stopping stale $name (PID: $($existing.Id -join ', '))..." -ForegroundColor Gray
        $existing | Stop-Process -Force -ErrorAction SilentlyContinue
    }
}

# Clean up any stale process occupying port 1420 (Vite dev server)
$stalePortConns = Get-NetTCPConnection -LocalPort 1420 -ErrorAction SilentlyContinue
if ($stalePortConns) {
    $stalePids = $stalePortConns | Select-Object -ExpandProperty OwningProcess -Unique
    foreach ($pidToKill in $stalePids) {
        if ($pidToKill -gt 0) {
            Write-Host "  Freeing port 1420 (Stopping process PID: $pidToKill)..." -ForegroundColor Gray
            Stop-Process -Id $pidToKill -Force -ErrorAction SilentlyContinue
        }
    }
}

# 2. Build binaries if needed
$AgentExe = Join-Path $RepoRoot "target\debug\screentime-agent.exe"
$SessionExe = Join-Path $RepoRoot "target\debug\screentime-session.exe"

if (!$NoBuild -or !(Test-Path $AgentExe) -or !(Test-Path $SessionExe)) {
    Write-Host "`n[2/4] Building backend binaries (st-agent, st-session)..." -ForegroundColor Yellow
    cargo build -p st-agent -p st-session
    if ($LASTEXITCODE -ne 0) {
        Write-Error "Failed to build backend binaries."
        exit 1
    }
} else {
    Write-Host "`n[2/4] Skipping build check (-NoBuild specified)..." -ForegroundColor Gray
}

# 3. Start Agent & wait for pipe
Write-Host "`n[3/4] Starting backend services..." -ForegroundColor Yellow
$winStyle = if ($Headless) { "Hidden" } else { "Normal" }

$AgentProc = Start-Process -FilePath $AgentExe `
    -WorkingDirectory $RepoRoot `
    -WindowStyle $winStyle `
    -PassThru

Write-Host "  -> screentime-agent started (PID: $($AgentProc.Id))" -ForegroundColor Green
Write-Host "  Waiting for \\.\pipe\screentime to become ready..." -ForegroundColor Gray

$pipeReady = $false
for ($i = 0; $i -lt 30; $i++) {
    Start-Sleep -Milliseconds 300
    if ($AgentProc.HasExited) {
        Write-Error "screentime-agent terminated unexpectedly (Exit Code: $($AgentProc.ExitCode)). Check logs in local\data\logs."
        exit 1
    }
    if ([System.IO.File]::Exists("\\.\pipe\screentime") -or (Get-ChildItem \\.\pipe\ -ErrorAction SilentlyContinue | Where-Object Name -eq "screentime")) {
        $pipeReady = $true
        break
    }
}

if ($pipeReady) {
    Write-Host "  -> Agent IPC server ready on \\.\pipe\screentime" -ForegroundColor Green
} else {
    Write-Warning "Named pipe \\.\pipe\screentime not detected within 9s. Attempting to proceed..."
}

# Start Session
$SessionProc = Start-Process -FilePath $SessionExe `
    -WorkingDirectory $RepoRoot `
    -WindowStyle $winStyle `
    -PassThru

Write-Host "  -> screentime-session sampling front started (PID: $($SessionProc.Id))" -ForegroundColor Green

# 4. Launch UI or keep backend alive
try {
    if ($BackendOnly) {
        Write-Host "`n[4/4] Running in BackendOnly mode." -ForegroundColor Cyan
        Write-Host "Press Ctrl+C to terminate all services." -ForegroundColor Gray
        while ($true) {
            Start-Sleep -Seconds 1
            if ($AgentProc.HasExited) {
                Write-Warning "screentime-agent exited."
                break
            }
            if ($SessionProc.HasExited) {
                Write-Warning "screentime-session exited."
                break
            }
        }
    } else {
        Write-Host "`n[4/4] Launching Tauri UI (npm run tauri dev)..." -ForegroundColor Yellow
        Push-Location (Join-Path $RepoRoot "ui")
        npm run tauri dev
        Pop-Location
    }
}
finally {
    Write-Host "`n[Clean-up] Stopping dev services..." -ForegroundColor Cyan
    if ($SessionProc -and !$SessionProc.HasExited) {
        Write-Host "  Stopping screentime-session (PID: $($SessionProc.Id))..." -ForegroundColor Gray
        Stop-Process -Id $SessionProc.Id -Force -ErrorAction SilentlyContinue
    }
    if ($AgentProc -and !$AgentProc.HasExited) {
        Write-Host "  Stopping screentime-agent (PID: $($AgentProc.Id))..." -ForegroundColor Gray
        Stop-Process -Id $AgentProc.Id -Force -ErrorAction SilentlyContinue
    }
    Write-Host "[Clean-up] All dev services stopped successfully." -ForegroundColor Green
}
