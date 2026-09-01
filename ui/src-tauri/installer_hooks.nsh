
!macro NSIS_HOOK_POSTINSTALL
  ExecWait '"$INSTDIR\bin\screentime-agent.exe" --install'
  ExecWait 'sc.exe start ScreentimeAgent'
  ExecWait '"$INSTDIR\bin\screentime-session.exe" --autostart on'
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ExecWait 'sc.exe stop ScreentimeAgent'
  ExecWait '"$INSTDIR\bin\screentime-agent.exe" --uninstall'
  ExecWait '"$INSTDIR\bin\screentime-session.exe" --autostart off'
!macroend
