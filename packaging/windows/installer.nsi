; Gospel Getter — Windows installer.
;
; Installs per-user (no admin required) to %LocalAppData%\GospelGetter:
;   bin\gospel_getter.exe   the server (built with windows_subsystem = "windows",
;                           so it never pops a console window)
;   .env                    fixed config (port, sqlite path) — the app reads
;                           this from its working directory, same as the
;                           systemd unit does on Linux via `Environment=`
;   data\                   where the SQLite database self-creates on first run
;   gospel-getter.ico       used for shortcuts
;   open-gospel-getter.vbs  opens the app in an Edge --app= window (falls back
;                           to the default browser), so a click on Start Menu
;                           feels like opening a native app, not just a URL
;
; A Startup-folder shortcut launches gospel_getter.exe silently at login —
; the Windows equivalent of the systemd --user service on Linux. A Start
; Menu shortcut runs the browser-opener instead, so the two "modes" (always
; running in the background vs. actually looking at it) match the Linux
; install exactly.

Unicode true

!include "MUI2.nsh"

!define APP_NAME "Gospel Getter"
!define APP_VERSION "1.0.0"
!define APP_PUBLISHER "tossbaws"
!define UNINST_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\GospelGetter"

Name "${APP_NAME}"
OutFile "GospelGetterSetup.exe"
InstallDir "$LOCALAPPDATA\GospelGetter"
InstallDirRegKey HKCU "${UNINST_KEY}" "InstallLocation"
RequestExecutionLevel user
SetCompressor /SOLID lzma

!define MUI_ICON "gospel-getter.ico"
!define MUI_UNICON "gospel-getter.ico"
!define MUI_ABORTWARNING

!define MUI_FINISHPAGE_RUN "$SYSDIR\wscript.exe"
!define MUI_FINISHPAGE_RUN_PARAMETERS "$\"$INSTDIR\open-gospel-getter.vbs$\""
!define MUI_FINISHPAGE_RUN_TEXT "Open Gospel Getter now"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "..\..\LICENSE"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "English"

VIProductVersion "1.0.0.0"
VIAddVersionKey "ProductName" "${APP_NAME}"
VIAddVersionKey "CompanyName" "${APP_PUBLISHER}"
VIAddVersionKey "FileVersion" "${APP_VERSION}"
VIAddVersionKey "ProductVersion" "${APP_VERSION}"
VIAddVersionKey "FileDescription" "${APP_NAME} installer"
VIAddVersionKey "LegalCopyright" "(c) ${APP_PUBLISHER}"

Section "Install"
  SetOutPath "$INSTDIR\bin"
  File "..\..\target\x86_64-pc-windows-gnu\release\gospel_getter.exe"

  ; SetOutPath also becomes the working directory ("Start in") for every
  ; shortcut/Exec below, so it must be $INSTDIR (not \bin) before those —
  ; that's what lets the app's relative "./data/..." path in .env resolve.
  SetOutPath "$INSTDIR"
  File "/oname=.env" "env.template"
  File "gospel-getter.ico"
  File "open-gospel-getter.vbs"
  CreateDirectory "$INSTDIR\data"

  CreateDirectory "$SMPROGRAMS\Gospel Getter"
  CreateShortCut "$SMPROGRAMS\Gospel Getter\Gospel Getter.lnk" "$SYSDIR\wscript.exe" '"$INSTDIR\open-gospel-getter.vbs"' "$INSTDIR\gospel-getter.ico"
  CreateShortCut "$SMPROGRAMS\Gospel Getter\Uninstall Gospel Getter.lnk" "$INSTDIR\Uninstall.exe"

  ; Autostart at login, minimized, no console (background service equivalent).
  CreateShortCut "$SMSTARTUP\Gospel Getter.lnk" "$INSTDIR\bin\gospel_getter.exe" "" "$INSTDIR\gospel-getter.ico" "" SW_SHOWMINIMIZED

  WriteRegStr HKCU "${UNINST_KEY}" "DisplayName" "${APP_NAME}"
  WriteRegStr HKCU "${UNINST_KEY}" "UninstallString" '"$INSTDIR\Uninstall.exe"'
  WriteRegStr HKCU "${UNINST_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayIcon" "$INSTDIR\gospel-getter.ico"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayVersion" "${APP_VERSION}"
  WriteRegStr HKCU "${UNINST_KEY}" "Publisher" "${APP_PUBLISHER}"
  WriteRegDWORD HKCU "${UNINST_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINST_KEY}" "NoRepair" 1

  WriteUninstaller "$INSTDIR\Uninstall.exe"

  ; Start the server right away so it's already running post-install,
  ; instead of waiting for the next login (mirrors install.sh's
  ; `systemctl restart` doing the same thing immediately on Linux).
  Exec '"$INSTDIR\bin\gospel_getter.exe"'
SectionEnd

Section "Uninstall"
  ExecWait 'taskkill /IM gospel_getter.exe /F'

  Delete "$SMSTARTUP\Gospel Getter.lnk"
  Delete "$SMPROGRAMS\Gospel Getter\Gospel Getter.lnk"
  Delete "$SMPROGRAMS\Gospel Getter\Uninstall Gospel Getter.lnk"
  RMDir "$SMPROGRAMS\Gospel Getter"

  Delete "$INSTDIR\bin\gospel_getter.exe"
  RMDir "$INSTDIR\bin"
  Delete "$INSTDIR\.env"
  Delete "$INSTDIR\gospel-getter.ico"
  Delete "$INSTDIR\open-gospel-getter.vbs"
  Delete "$INSTDIR\Uninstall.exe"

  ; Silent uninstalls (used for automated testing) default to keeping data
  ; rather than popping a dialog with nothing there to answer it.
  IfSilent skip_data_prompt
  MessageBox MB_YESNO "Also delete your local Bible reading data (translations, cross-references, reading position)?" IDNO skip_data_prompt
  RMDir /r "$INSTDIR\data"
  skip_data_prompt:

  RMDir "$INSTDIR" ; only succeeds if empty (i.e. data\ was also removed above)
  DeleteRegKey HKCU "${UNINST_KEY}"
SectionEnd
