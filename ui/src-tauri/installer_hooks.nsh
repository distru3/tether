
!macro NSIS_HOOK_POSTINSTALL
  nsExec::Exec '"$INSTDIR\bin\screentime-agent.exe" --install'
  nsExec::Exec '"$SYSDIR\sc.exe" start ScreentimeAgent'
  nsExec::Exec '"$INSTDIR\bin\screentime-session.exe" --autostart on'
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
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "screentime-session"
  DeleteRegValue HKLM "Software\Microsoft\Windows\CurrentVersion\Run" "screentime-session"
  Sleep 500

  ; Uninstall the agent service silently
  nsExec::Exec '"$INSTDIR\bin\screentime-agent.exe" --uninstall'
!macroend
