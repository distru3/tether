@echo off
set "ROOT=%~dp0..\"
cd /d "%ROOT%"

set "SESSION_EXE=%ROOT%target\release\screentime-session.exe"
if not exist "%SESSION_EXE%" (
    set "SESSION_EXE=%ROOT%target\debug\screentime-session.exe"
)

echo Disabling logon autostart for Screentime...
"%SESSION_EXE%" --autostart off

echo.
"%SESSION_EXE%" --autostart status
echo.
pause
