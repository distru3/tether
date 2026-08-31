@echo off
:: ============================================================================
:: Screentime App Launcher
:: Starts the Agent backend, Session Tracker, and the Dashboard UI with Tray
:: ============================================================================

set "ROOT=%~dp0"
cd /d "%ROOT%"

:: Ensure user data directory exists and is writable
if not defined SCREENTIME_DATA_DIR (
    set "SCREENTIME_DATA_DIR=%LOCALAPPDATA%\screentime"
)
if not exist "%SCREENTIME_DATA_DIR%\logs" (
    mkdir "%SCREENTIME_DATA_DIR%\logs" >nul 2>&1
)

:: Check if release binaries exist, fallback to debug if needed
set "BIN_DIR=%ROOT%target\release"
if not exist "%BIN_DIR%\screentime-ui.exe" (
    set "BIN_DIR=%ROOT%target\debug"
)

echo.
echo [1/3] Starting Screentime Agent backend (Elevated for DNS & Limits)...
net session >nul 2>&1
if %errorlevel% neq 0 (
    powershell -NoProfile -Command "Start-Process cmd -ArgumentList '/c set \"SCREENTIME_DATA_DIR=%SCREENTIME_DATA_DIR%\" & \"%BIN_DIR%\screentime-agent.exe\"' -Verb RunAs -WindowStyle Hidden"
) else (
    start "Screentime Agent" /B "%BIN_DIR%\screentime-agent.exe"
)

timeout /t 1 /nobreak >nul

echo [2/3] Starting Screentime Session Tracker (Tray & Hook)...
start "Screentime Session" /B "%BIN_DIR%\screentime-session.exe"

timeout /t 1 /nobreak >nul

echo [3/3] Launching Screentime Dashboard UI...
start "" "%BIN_DIR%\screentime-ui.exe"

echo.
echo Screentime is now running!
echo - Look for the icon in your Taskbar Tray ("Show hidden icons").
echo - Right-click the tray icon anytime to Open, Emergency Reset, or Stop All Services.
echo.
