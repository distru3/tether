
!macro NSIS_HOOK_POSTINSTALL
  ExecWait '"$INSTDIR\bin\screentime-agent.exe" --install'
  ExecWait 'sc.exe start ScreentimeAgent'
  ExecWait '"$INSTDIR\bin\screentime-session.exe" --autostart on'

  MessageBox MB_YESNO|MB_ICONQUESTION "Do you want to enable the Family DNS filter now? This will block adult content across the entire device by switching your internet to Cloudflare Family." IDNO skip_dns
    ExecWait '"$INSTDIR\bin\screentime-agent.exe" --enable-family-dns'
    WriteRegDWORD HKLM "Software\Screentime" "FamilyDnsApplied" 1
  skip_dns:
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ClearErrors
  ReadRegDWORD $0 HKLM "Software\Screentime" "FamilyDnsApplied"
  IfErrors skip_dns_remove
  IntCmp $0 1 prompt_uninstall skip_dns_remove skip_dns_remove

  prompt_uninstall:
  MessageBox MB_YESNO|MB_ICONQUESTION "Do you want to remove the Family DNS filter and restore automatic DNS settings (recommended)?" IDNO skip_dns_remove
    ExecWait '"$INSTDIR\bin\screentime-agent.exe" --disable-family-dns'
    DeleteRegValue HKLM "Software\Screentime" "FamilyDnsApplied"
  skip_dns_remove:

  ExecWait 'sc.exe stop ScreentimeAgent'
  ExecWait '"$INSTDIR\bin\screentime-agent.exe" --uninstall'
  ExecWait '"$INSTDIR\bin\screentime-session.exe" --autostart off'
!macroend
