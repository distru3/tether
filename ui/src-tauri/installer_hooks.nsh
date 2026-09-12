
!macro NSIS_HOOK_POSTINSTALL
  ExecWait '"$INSTDIR\bin\screentime-agent.exe" --install'
  ExecWait 'sc.exe start ScreentimeAgent'
  ExecWait '"$INSTDIR\bin\screentime-session.exe" --autostart on'
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ClearErrors
  ReadRegDWORD $0 HKLM "Software\Screentime" "FamilyDnsApplied"
  IfErrors skip_dns_remove
  IntCmp $0 1 do_restore skip_dns_remove skip_dns_remove

  do_restore:
    ExecWait '"$INSTDIR\bin\screentime-agent.exe" --disable-family-dns'
    DeleteRegValue HKLM "Software\Screentime" "FamilyDnsApplied"
  skip_dns_remove:

  ExecWait 'sc.exe stop ScreentimeAgent'
  ExecWait '"$INSTDIR\bin\screentime-agent.exe" --uninstall'
  ExecWait '"$INSTDIR\bin\screentime-session.exe" --autostart off'
!macroend
