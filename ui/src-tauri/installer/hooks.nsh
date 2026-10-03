; Tether's install and uninstall steps, injected into Tauri's NSIS template
; (tauri.conf.json > bundle > windows > nsis > installerHooks). The template
; is ui/src-tauri/installer/installer.nsi; it includes style.nsh and
; strings.nsh from this folder through TETHER_INSTALLER_DIR.

!define TETHER_INSTALLER_DIR "${__FILEDIR__}"

; Runs a command hidden and discards its exit code (nsExec leaves it on the
; stack).
!macro TETHER_EXEC CMD
  nsExec::Exec '${CMD}'
  Pop $0
!macroend

!macro NSIS_HOOK_PREINSTALL
  ; Stop the service and close the helpers so their files are not locked
  ; during an upgrade or reinstall.
  DetailPrint "$(tetherDetailClosing)"
  !insertmacro TETHER_EXEC '"$SYSDIR\sc.exe" stop ScreentimeAgent'
  !insertmacro TETHER_EXEC '"$SYSDIR\taskkill.exe" /F /IM screentime-session.exe'
  !insertmacro TETHER_EXEC '"$SYSDIR\taskkill.exe" /F /IM screentime-ui.exe'
  Sleep 300
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ; The installing page's subtitle says the service runs only if it does
  ; (sc.exe exits with the Win32 error: 0 started, 1056 already running).
  DetailPrint "$(tetherDetailService)"
  StrCpy $TetherInstalledSub "$(tetherInstalledSubtitle)"
  !insertmacro TETHER_EXEC '"$INSTDIR\bin\screentime-agent.exe" --install'
  nsExec::Exec '"$SYSDIR\sc.exe" start ScreentimeAgent'
  Pop $0
  ${If} $0 <> 0
  ${AndIf} $0 <> 1056
    DetailPrint "$(tetherDetailServiceFailed)"
    StrCpy $TetherInstalledSub "$(tetherServiceFailedSubtitle)"
  ${EndIf}

  ; One HKLM Run entry starts the session helper at logon for every
  ; interactive user. (An HKCU entry written here would land in the hive of
  ; whoever approved the UAC prompt, which is not always the person who
  ; signs in, and would start a second copy for them.)
  DetailPrint "$(tetherDetailStartup)"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Run" "ScreentimeSession" '"$INSTDIR\bin\screentime-session.exe"'
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "ScreentimeSession"

  ; RivaTuner Statistics Server hooks every process it sees and can break the
  ; HUD (session) and block overlay (UI) windows over games. Its profile folder
  ; lives under Program Files, which only this elevated installer can write, so
  ; drop "do not hook" profiles here (the session helper itself cannot).
  ${If} ${FileExists} "$PROGRAMFILES32\RivaTuner Statistics Server\Profiles\*.*"
    DetailPrint "$(tetherDetailOverlay)"
    !insertmacro TETHER_RTSS_NO_HOOK "screentime-session.exe"
    !insertmacro TETHER_RTSS_NO_HOOK "screentime-ui.exe"
    !insertmacro TETHER_RTSS_NO_HOOK "Tether.exe"
  ${EndIf}

  ; Remember the language for the uninstaller. MUI saves it on the
  ; installing page, which a silent install never shows; without it the
  ; uninstaller would ask which language to use.
  WriteRegStr ${MUI_LANGDLL_REGISTRY_ROOT} "${MUI_LANGDLL_REGISTRY_KEY}" "${MUI_LANGDLL_REGISTRY_VALUENAME}" $LANGUAGE

  ; Start tracking now rather than at the next sign-in. Run it as the
  ; signed-in user, not with the installer's elevated token: the helper
  ; belongs to the desktop user and keeps a per-user single-instance lock.
  DetailPrint "$(tetherDetailTracking)"
  nsis_tauri_utils::RunAsUser "$INSTDIR\bin\screentime-session.exe" ""
  Pop $0
!macroend

!macro TETHER_RTSS_NO_HOOK EXE
  FileOpen $0 "$PROGRAMFILES32\RivaTuner Statistics Server\Profiles\${EXE}.cfg" w
  FileWrite $0 "[Hooking]$\r$\nEnableHooking = 0$\r$\n"
  FileClose $0
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; Stop the agent service first so it releases locks and does not re-apply DNS
  DetailPrint "$(tetherUnDetailService)"
  !insertmacro TETHER_EXEC '"$SYSDIR\sc.exe" stop ScreentimeAgent'

  ; Restore the previous DNS only when Family DNS is on (the agent records
  ; that in HKLM\Software\Screentime\FamilyDnsApplied) and the person kept
  ; the box ticked. Passive and silent uninstalls restore it whenever it is
  ; on (see un.onInit).
  ${If} $RestoreDnsCheckboxState = 1
    DetailPrint "$(tetherUnDetailDns)"
    !insertmacro TETHER_EXEC '"$INSTDIR\bin\screentime-agent.exe" --disable-family-dns'
    DeleteRegValue HKLM "Software\Screentime" "FamilyDnsApplied"
  ${EndIf}

  ; Terminate running session and UI processes so files are unlocked for deletion
  !insertmacro TETHER_EXEC '"$SYSDIR\taskkill.exe" /F /IM screentime-session.exe'
  !insertmacro TETHER_EXEC '"$SYSDIR\taskkill.exe" /F /IM screentime-ui.exe'
  !insertmacro TETHER_EXEC '"$INSTDIR\bin\screentime-session.exe" --autostart off'
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "ScreentimeSession"
  DeleteRegValue HKLM "Software\Microsoft\Windows\CurrentVersion\Run" "ScreentimeSession"
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "screentime-session"
  DeleteRegValue HKLM "Software\Microsoft\Windows\CurrentVersion\Run" "screentime-session"
  Sleep 500

  ; Uninstall the agent service silently
  DetailPrint "$(tetherUnDetailRemoveService)"
  !insertmacro TETHER_EXEC '"$INSTDIR\bin\screentime-agent.exe" --uninstall'
!macroend
