
!macro NSIS_HOOK_PREINSTALL
  ; Stop service and terminate existing helper processes if upgrading/reinstalling so files are not locked
  nsExec::Exec '"$SYSDIR\sc.exe" stop ScreentimeAgent'
  nsExec::Exec '"$SYSDIR\taskkill.exe" /F /IM screentime-session.exe'
  nsExec::Exec '"$SYSDIR\taskkill.exe" /F /IM screentime-ui.exe'
  Sleep 300
!macroend

!macro NSIS_HOOK_POSTINSTALL
  nsExec::Exec '"$INSTDIR\bin\screentime-agent.exe" --install'
  nsExec::Exec '"$SYSDIR\sc.exe" start ScreentimeAgent'
  ; Write to HKLM Run so screentime-session launches on logon for all interactive users
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Run" "ScreentimeSession" '"$INSTDIR\bin\screentime-session.exe"'
  ; Also register HKCU Run if available
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "ScreentimeSession" '"$INSTDIR\bin\screentime-session.exe"'
  ; Immediately spawn screentime-session.exe so tracking begins right away without waiting for reboot/logoff
  Exec '"$INSTDIR\bin\screentime-session.exe"'
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; Stop the agent service first so it releases locks and does not re-apply DNS
  nsExec::Exec '"$SYSDIR\sc.exe" stop ScreentimeAgent'

  ; If user opted to restore DNS settings (or default on passive mode)
  ${If} $RestoreDnsCheckboxState = 1
    nsExec::Exec '"$INSTDIR\bin\screentime-agent.exe" --disable-family-dns'
    DeleteRegValue HKLM "Software\Screentime" "FamilyDnsApplied"
  ${EndIf}

  ; Terminate running session and UI processes so files are unlocked for deletion
  nsExec::Exec '"$SYSDIR\taskkill.exe" /F /IM screentime-session.exe'
  nsExec::Exec '"$SYSDIR\taskkill.exe" /F /IM screentime-ui.exe'
  nsExec::Exec '"$INSTDIR\bin\screentime-session.exe" --autostart off'
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "ScreentimeSession"
  DeleteRegValue HKLM "Software\Microsoft\Windows\CurrentVersion\Run" "ScreentimeSession"
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "screentime-session"
  DeleteRegValue HKLM "Software\Microsoft\Windows\CurrentVersion\Run" "screentime-session"
  Sleep 500

  ; Uninstall the agent service silently
  nsExec::Exec '"$INSTDIR\bin\screentime-agent.exe" --uninstall'
!macroend
