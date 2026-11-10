Unicode true
ManifestDPIAware true
ManifestSupportedOS all
RequestExecutionLevel user
SetCompressor /SOLID lzma

!include "MUI2.nsh"
!include "LogicLib.nsh"
!include "WinVer.nsh"
!include "x64.nsh"

!ifndef STAGE_DIR
  !error "Pass /DSTAGE_DIR=<installed application directory>"
!endif
!ifndef SRC_DIR
  !error "Pass /DSRC_DIR=<repository directory>"
!endif
!ifndef APP_VERSION
  !error "Pass /DAPP_VERSION=<version>"
!endif
!ifndef FILE_VERSION
  !error "Pass /DFILE_VERSION=<numeric version>"
!endif
!ifndef OUT_FILE
  !error "Pass /DOUT_FILE=<setup.exe>"
!endif

!define UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\SannyGit"
Name "SannyGit ${APP_VERSION}"
OutFile "${OUT_FILE}"
InstallDir "$LOCALAPPDATA\Programs\SannyGit"
InstallDirRegKey HKCU "${UNINSTALL_KEY}" "InstallLocation"
VIProductVersion "${FILE_VERSION}"
VIAddVersionKey "ProductName" "SannyGit"
VIAddVersionKey "FileDescription" "SannyGit Setup"
VIAddVersionKey "FileVersion" "${FILE_VERSION}"
VIAddVersionKey "ProductVersion" "${APP_VERSION}"
VIAddVersionKey "LegalCopyright" "Copyright (c) 2026 SannyGit contributors"

!define MUI_ICON "${SRC_DIR}\assets\app\sannygit.ico"
!define MUI_UNICON "${SRC_DIR}\assets\app\sannygit.ico"
!define MUI_ABORTWARNING
!define MUI_FINISHPAGE_RUN "$INSTDIR\SannyGit.exe"
!define MUI_FINISHPAGE_RUN_NOTCHECKED
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "${SRC_DIR}\LICENSE"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

Function .onInit
  ${IfNot} ${AtLeastWin10}
    MessageBox MB_OK|MB_ICONSTOP "SannyGit requires Windows 10 or later."
    Abort
  ${EndIf}
  ${IfNot} ${RunningX64}
    MessageBox MB_OK|MB_ICONSTOP "This installer requires 64-bit Windows."
    Abort
  ${EndIf}
  SetRegView 64
FunctionEnd

Section "SannyGit"
  SetShellVarContext current
  SetOutPath "$INSTDIR"
  File "${STAGE_DIR}\SannyGit.exe"
  File "${STAGE_DIR}\LICENSE"
  File "${STAGE_DIR}\SourceSans3-OFL.txt"
  WriteUninstaller "$INSTDIR\Uninstall.exe"
  CreateShortcut "$SMPROGRAMS\SannyGit.lnk" "$INSTDIR\SannyGit.exe"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayName" "SannyGit"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayVersion" "${APP_VERSION}"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayIcon" "$INSTDIR\SannyGit.exe"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "UninstallString" '$\"$INSTDIR\Uninstall.exe$\"'
  WriteRegStr HKCU "${UNINSTALL_KEY}" "QuietUninstallString" '$\"$INSTDIR\Uninstall.exe$\" /S'
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoRepair" 1
SectionEnd

Section "Uninstall"
  SetRegView 64
  SetShellVarContext current
  Delete "$INSTDIR\SannyGit.exe"
  Delete "$INSTDIR\LICENSE"
  Delete "$INSTDIR\SourceSans3-OFL.txt"
  Delete "$INSTDIR\Uninstall.exe"
  Delete "$SMPROGRAMS\SannyGit.lnk"
  DeleteRegKey HKCU "${UNINSTALL_KEY}"
  # Keep user-created files and application preferences.
  RMDir "$INSTDIR"
SectionEnd
