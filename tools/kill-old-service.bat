@echo off
echo Stopping old background ScreentimeAgent service...
net stop ScreentimeAgent >nul 2>&1
sc.exe stop ScreentimeAgent >nul 2>&1
sc.exe delete ScreentimeAgent >nul 2>&1
taskkill /F /IM screentime-agent.exe /T >nul 2>&1
taskkill /F /IM screentime-session.exe /T >nul 2>&1
taskkill /F /IM screentime-ui.exe /T >nul 2>&1
if exist "C:\Program Files\screentime" (
    rmdir /S /Q "C:\Program Files\screentime" >nul 2>&1
)
echo Done!

