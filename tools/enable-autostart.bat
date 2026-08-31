@echo off
set "ROOT=%~dp0..\"
cd /d "%ROOT%"

set "SESSION_EXE=%ROOT%target\release\screentime-session.exe"
if not exist "%SESSION_EXE%" (
    set "SESSION_EXE=%ROOT%target\debug\screentime-session.exe"
)

echo Enabling user logon autostart for Screentime Session Tracker...
"%SESSION_EXE%" --autostart on

echo.
"%SESSION_EXE%" --autostart status
echo.
pause
