; Tether installer look (docs/DESIGN_SYSTEM.md): white pages with plum text,
; a plum sidebar on the welcome and finish pages (art/sidebar.bmp, drawn by
; art/make_art.py), the app icon in the header, and the app's own type:
; Rubik for text and Unbounded for page titles.
;
; The fonts are embedded and registered privately for the installer process
; only (AddFontResourceEx with FR_PRIVATE), so nothing is installed on the
; system. If loading fails, Windows substitutes its default UI font.
;
; Included by installer.nsi before the MUI page macros.

!define MUI_BGCOLOR "FFFFFF"
!define MUI_TEXTCOLOR "23163A"
!define TETHER_MUTED_TEXT "6B5F80"

!define MUI_HEADERIMAGE_RIGHT
!define MUI_HEADERIMAGE_BITMAP_STRETCH AspectFitHeight
!define MUI_HEADERIMAGE_UNBITMAP_STRETCH AspectFitHeight
!define MUI_INSTFILESPAGE_PROGRESSBAR smooth

!define MUI_ABORTWARNING
!define MUI_ABORTWARNING_TEXT "$(tetherAbortWarning)"
!define MUI_UNABORTWARNING
!define MUI_UNABORTWARNING_TEXT "$(tetherUnAbortWarning)"

!define MUI_CUSTOMFUNCTION_GUIINIT Tether.GuiInit
!define MUI_CUSTOMFUNCTION_UNGUIINIT un.Tether.GuiInit

; Dialog font for every page. Registered in .onInit (before any dialog
; exists) by Tether.LoadFonts.
SetFont "Rubik" 9
BrandingText "Tether ${VERSION}"

ReserveFile "${TETHER_INSTALLER_DIR}\fonts\Rubik-Regular.ttf"
ReserveFile "${TETHER_INSTALLER_DIR}\fonts\Rubik-SemiBold.ttf"
ReserveFile "${TETHER_INSTALLER_DIR}\fonts\Unbounded-SemiBold.ttf"

!define FR_PRIVATE 0x10
!define LANG_ID_ARABIC 1025

Var TetherTitleFont
; Subtitle of the installing page once done; hooks.nsh sets it.
Var TetherInstalledSub
Var TetherHeaderFont

!macro TETHER_LOAD_FONTS
  InitPluginsDir
  File "/oname=$PLUGINSDIR\Rubik-Regular.ttf" "${TETHER_INSTALLER_DIR}\fonts\Rubik-Regular.ttf"
  File "/oname=$PLUGINSDIR\Rubik-SemiBold.ttf" "${TETHER_INSTALLER_DIR}\fonts\Rubik-SemiBold.ttf"
  File "/oname=$PLUGINSDIR\Unbounded-SemiBold.ttf" "${TETHER_INSTALLER_DIR}\fonts\Unbounded-SemiBold.ttf"
  System::Call 'gdi32::AddFontResourceExW(w "$PLUGINSDIR\Rubik-Regular.ttf", i ${FR_PRIVATE}, p 0) i'
  System::Call 'gdi32::AddFontResourceExW(w "$PLUGINSDIR\Rubik-SemiBold.ttf", i ${FR_PRIVATE}, p 0) i'
  System::Call 'gdi32::AddFontResourceExW(w "$PLUGINSDIR\Unbounded-SemiBold.ttf", i ${FR_PRIVATE}, p 0) i'
!macroend

; Unbounded has no Arabic letters; Arabic titles use Rubik SemiBold.
!macro TETHER_CREATE_FONTS
  ${If} $LANGUAGE = ${LANG_ID_ARABIC}
    CreateFont $TetherTitleFont "Rubik SemiBold" 15 400
    CreateFont $TetherHeaderFont "Rubik SemiBold" 10 400
  ${Else}
    CreateFont $TetherTitleFont "Unbounded SemiBold" 13 400
    CreateFont $TetherHeaderFont "Unbounded SemiBold" 9 400
  ${EndIf}
!macroend

; Header: display font for the page title, muted colour for the subtitle.
!macro TETHER_GUIINIT
  !insertmacro TETHER_CREATE_FONTS
  SendMessage $mui.Header.Text ${WM_SETFONT} $TetherHeaderFont 0
  SetCtlColors $mui.Header.SubText "${TETHER_MUTED_TEXT}" "${MUI_BGCOLOR}"
!macroend

Function Tether.LoadFonts
  !insertmacro TETHER_LOAD_FONTS
FunctionEnd
Function un.Tether.LoadFonts
  !insertmacro TETHER_LOAD_FONTS
FunctionEnd

Function Tether.GuiInit
  !insertmacro TETHER_GUIINIT
FunctionEnd
Function un.Tether.GuiInit
  !insertmacro TETHER_GUIINIT
FunctionEnd

; Welcome and finish pages: big display title. A macro because the MUI
; page variables exist only after the page macros; installer.nsi inserts it
; after the finish page.
!macro TETHER_PAGE_FUNCTIONS
  Function Tether.WelcomeShow
    SendMessage $mui.WelcomePage.Title ${WM_SETFONT} $TetherTitleFont 0
    Push $mui.WelcomePage.Text
    Push $mui.WelcomePage.Title
    Call Tether.TuckText
  FunctionEnd
  Function Tether.FinishShow
    SendMessage $mui.FinishPage.Title ${WM_SETFONT} $TetherTitleFont 0
    Push $mui.FinishPage.Text
    Push $mui.FinishPage.Title
    Call Tether.TuckText
  FunctionEnd
!macroend

; MUI reserves two lines for the title; ours fit on one. Move the body text
; up to sit under the title's first line, keeping its bottom edge.
; Stack: title HWND, text HWND.
Function Tether.TuckText
  Exch $0 ; title
  Exch
  Exch $1 ; text
  Push $2
  Push $3
  Push $4
  Push $5
  Push $6
  Push $7
  Push $8
  System::Call "user32::GetDpiForWindow(p r0) i .r8"
  ${If} $8 <= 0
    StrCpy $8 96
  ${EndIf}
  System::Call '*(i, i, i, i) p .r2'
  System::Call 'user32::GetParent(p r0) p .r7'
  System::Call 'user32::GetWindowRect(p r0, p r2)'
  System::Call 'user32::MapWindowPoints(p 0, p r7, p r2, i 2)'
  System::Call '*$2(i, i .r3, i, i)' ; title top
  System::Call 'user32::GetWindowRect(p r1, p r2)'
  System::Call 'user32::MapWindowPoints(p 0, p r7, p r2, i 2)'
  System::Call '*$2(i .r4, i .r5, i .r6, i .r7)' ; text rect
  System::Free $2
  ${If} $4 > $6 ; mirrored (RTL): left > right
    StrCpy $2 $4
    StrCpy $4 $6
    StrCpy $6 $2
  ${EndIf}
  IntOp $2 46 * $8
  IntOp $2 $2 / 96
  IntOp $2 $3 + $2 ; new text top
  ${If} $2 < $5
    IntOp $7 $7 - $2 ; height down to the old bottom
    IntOp $6 $6 - $4 ; width
    ; SWP_NOZORDER | SWP_NOACTIVATE
    System::Call 'user32::SetWindowPos(p r1, p 0, i r4, i r2, i r6, i r7, i 0x14)'
  ${EndIf}
  Pop $8
  Pop $7
  Pop $6
  Pop $5
  Pop $4
  Pop $3
  Pop $2
  Pop $1
  Pop $0
FunctionEnd
