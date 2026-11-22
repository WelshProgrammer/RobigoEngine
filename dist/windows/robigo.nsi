; The Windows setup. It installs the engine alone: the game asks for the player's copy of
; Settlers 2 the first time it starts and keeps it, the saved games and the settings in
; %APPDATA%\robigo, which the uninstaller leaves.
;
; usage: makensis -DVERSION=<version> -DSOURCE=<the folder package.ps1 staged>
;        -DICON=<assets/icons/robigo.ico> -DOUTFILE=<setup.exe> robigo.nsi

Unicode true
SetCompressor /SOLID lzma
RequestExecutionLevel admin

!include "MUI2.nsh"

!define UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\robigo"

Name "Robigo ${VERSION}"
OutFile "${OUTFILE}"
InstallDir "$PROGRAMFILES64\Robigo"
InstallDirRegKey HKLM "${UNINSTALL_KEY}" "InstallLocation"

!define MUI_ICON "${ICON}"
!define MUI_UNICON "${ICON}"
!define MUI_ABORTWARNING
!define MUI_FINISHPAGE_TEXT "Robigo is installed.$\r$\n$\r$\nThe first time it starts it asks where your copy of Settlers 2 is: a CD or its image, the Gold Edition's ISO or BIN and CUE files, or a GOG installation."

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "${SOURCE}\LICENSE.txt"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "English"

; The staged folder's RETAIL stays out. The game looks beside its binary first, and a RETAIL in
; Program Files is one it cannot copy the player's game into.
Section "Robigo"
    SetShellVarContext all
    SetOutPath "$INSTDIR"
    File "${SOURCE}\robigo.exe"
    File "${SOURCE}\LICENSE.txt"
    WriteUninstaller "$INSTDIR\uninstall.exe"
    CreateShortcut "$SMPROGRAMS\Robigo.lnk" "$INSTDIR\robigo.exe"

    WriteRegStr HKLM "${UNINSTALL_KEY}" "DisplayName" "Robigo"
    WriteRegStr HKLM "${UNINSTALL_KEY}" "DisplayVersion" "${VERSION}"
    WriteRegStr HKLM "${UNINSTALL_KEY}" "Publisher" "WelshProgrammer"
    WriteRegStr HKLM "${UNINSTALL_KEY}" "DisplayIcon" "$INSTDIR\robigo.exe"
    WriteRegStr HKLM "${UNINSTALL_KEY}" "InstallLocation" "$INSTDIR"
    WriteRegStr HKLM "${UNINSTALL_KEY}" "UninstallString" '"$INSTDIR\uninstall.exe"'
    WriteRegDWORD HKLM "${UNINSTALL_KEY}" "NoModify" 1
    WriteRegDWORD HKLM "${UNINSTALL_KEY}" "NoRepair" 1
SectionEnd

Section "Uninstall"
    SetShellVarContext all
    Delete "$SMPROGRAMS\Robigo.lnk"
    Delete "$INSTDIR\robigo.exe"
    Delete "$INSTDIR\LICENSE.txt"
    Delete "$INSTDIR\uninstall.exe"
    RMDir "$INSTDIR"
    DeleteRegKey HKLM "${UNINSTALL_KEY}"
SectionEnd
