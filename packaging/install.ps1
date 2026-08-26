<#
.SYNOPSIS
    Manual/elevated install of Screentime's two daemons (Story A in README.md):
    builds release binaries, copies them to the install directory, registers
    screentime-agent as a Windows service, and turns on logon autostart for
    screentime-session.

.DESCRIPTION
    PowerShell 5.1 compatible. Must run elevated (sc.exe needs it). The
    dashboard (Tauri UI) is NOT installed by this script; build it separately
    with "npm run tauri build" from ui/.

.EXAMPLE
    .\install.ps1
    .\install.ps1 -InstallDir "D:\Tools\Screentime"
#>
[CmdletBinding()]
param(
    # Target directory for the daemon executables. sc.exe and the autostart
    # entry both reference this exact path, so quote-safety matters.
    [string]$InstallDir = "C:\Program Files\Screentime"
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version 2

$ServiceName = "ScreentimeAgent"

function Fail([string]$Message) {
    Write-Error $Message -ErrorAction Continue
    exit 1
}

# --- 0. Preconditions -------------------------------------------------------

$principal = New-Object Security.Principal.WindowsPrincipal(
    [Security.Principal.WindowsIdentity]::GetCurrent())
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    Fail "This script must run from an elevated PowerShell (the service registration via sc.exe requires it)."
}

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Fail "cargo was not found on PATH; install the Rust toolchain first."
}

$RepoRoot = Split-Path -Parent $PSScriptRoot
$AgentSource = Join-Path $RepoRoot "target\release\screentime-agent.exe"
$SessionSource = Join-Path $RepoRoot "target\release\screentime-session.exe"
$AgentDest = Join-Path $InstallDir "screentime-agent.exe"
$SessionDest = Join-Path $InstallDir "screentime-session.exe"

Write-Host "== Screentime install =="
Write-Host "Repo root : $RepoRoot"
Write-Host "Install to: $InstallDir"

# --- 1. Stop running instances ---------------------------------------------
# The release binaries cannot be overwritten while running, and a service
# reinstall against a live process leaves SCM confused. Graceful close first;
# these daemons have no interactive window to decline with, so after a short
# wait we stop them outright (deliberate, not reckless: they will be replaced
# moments later).
foreach ($procName in @("screentime-agent", "screentime-session")) {
    $running = Get-Process -Name $procName -ErrorAction SilentlyContinue
    if (-not $running) { continue }
    Write-Host "Stopping $procName ($($running.Count) process(es))..."
    foreach ($p in $running) {
        if ($p.HasExited) { continue }
        $null = $p.CloseMainWindow()   # harmless if windowless
    }
    $running | Wait-Process -Timeout 3 -ErrorAction SilentlyContinue
    Get-Process -Name $procName -ErrorAction SilentlyContinue | Stop-Process
    Start-Sleep -Milliseconds 500
}

# --- 2. Build release binaries ----------------------------------------------
Push-Location $RepoRoot
try {
    Write-Host "Building release binaries..."
    cargo build --release -p st-agent -p st-session
    if ($LASTEXITCODE -ne 0) {
        Fail "cargo build --release failed with exit code $LASTEXITCODE."
    }
}
finally {
    Pop-Location
}

# --- 3. Copy executables -----------------------------------------------------
if (-not (Test-Path -LiteralPath $AgentSource)) { Fail "Build output missing: $AgentSource" }
if (-not (Test-Path -LiteralPath $SessionSource)) { Fail "Build output missing: $SessionSource" }

if (-not (Test-Path -LiteralPath $InstallDir)) {
    New-Item -ItemType Directory -Path $InstallDir | Out-Null
}
Copy-Item -LiteralPath $AgentSource -Destination $AgentDest
Copy-Item -LiteralPath $SessionSource -Destination $SessionDest
Write-Host "Copied daemons to $InstallDir"

# --- 4. Register / update the agent service ---------------------------------
# sc.exe argument rules: a literal space after each '=', and the binPath must
# keep embedded quotes so Windows keeps it one path despite spaces.
$binPathQuoted = "`"$AgentDest`""

$existing = Get-Service -Name $ServiceName -ErrorAction SilentlyContinue
if ($existing) {
    Write-Host "Service $ServiceName exists; updating..."
    if ($existing.Status -ne "Stopped") {
        Stop-Service -Name $ServiceName
        $existing.WaitForStatus("Stopped", "00:00:15")
    }
    & sc.exe config $ServiceName type= own start= auto binPath= $binPathQuoted
} else {
    Write-Host "Creating service $ServiceName..."
    & sc.exe create $ServiceName type= own start= auto binPath= $binPathQuoted DisplayName= "Screentime Agent"
}
if ($LASTEXITCODE -ne 0) { Fail "sc.exe failed to configure $ServiceName (exit $LASTEXITCODE)." }

& sc.exe failure $ServiceName reset= 86400 actions= restart/60000/restart/60000/restart/60000
if ($LASTEXITCODE -ne 0) { Fail "sc.exe failure-actions setup failed (exit $LASTEXITCODE)." }

# --- 5. Session helper logon autostart (current user, no elevation needed) --
# Run against the INSTALLED copy so HKCU Run records the real path.
Write-Host "Enabling logon autostart for the session helper..."
& $SessionDest --autostart on
if ($LASTEXITCODE -ne 0) { Fail "screentime-session --autostart on failed (exit $LASTEXITCODE)." }

# --- 6. Start the agent ------------------------------------------------------
Start-Service -Name $ServiceName

# --- 7. Verification hints ---------------------------------------------------
Write-Host ""
Write-Host "== Install complete. Verify: =="
Write-Host "  Get-Service $ServiceName                       # agent service Running"
Write-Host "  Test-Path \\.\pipe\screentime                  # pipe exists"
Write-Host "  reg query HKCU\Software\Microsoft\Windows\CurrentVersion\Run /v ScreentimeSession"
Write-Host "  Log off/on (or run '$SessionDest' once) then check:"
Write-Host "    $env:LOCALAPPDATA\screentime\logs            # session helper logs"
Write-Host "    $env:ProgramData\screentime\logs             # agent logs"
