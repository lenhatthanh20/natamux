!ifndef BIN
  !error "BIN is required"
!endif
!ifndef ROOT
  !error "ROOT is required"
!endif
!ifndef OUT
  !error "OUT is required"
!endif
!ifndef VERSION
  !error "VERSION is required"
!endif

!include FileFunc.nsh

Name "Natamux"
OutFile "${OUT}\Natamux-win-x64.exe"
InstallDir "$LOCALAPPDATA\Programs\Natamux"
RequestExecutionLevel user
Unicode True
Icon "${ROOT}\crates\natamux-app\resources\icon.ico"
UninstallIcon "${ROOT}\crates\natamux-app\resources\icon.ico"
!define UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\Natamux"

Section "Install"
  SetOutPath "$INSTDIR"
  ; The app updates itself with /S: a file in use must fail the install, not be skipped.
  SetOverwrite on
  ClearErrors
  ; Not Natamux.exe: Windows names ignore case, so it would be the CLI natamux.exe.
  File "${BIN}\natamux-app.exe"
  File "${BIN}\natamux-daemon.exe"
  File "${BIN}\natamux.exe"
  File "${BIN}\nmx.exe"
  File "${ROOT}\LICENSE"
  File "${ROOT}\THIRD-PARTY-NOTICES.md"
  File "${ROOT}\THIRD-PARTY-RUST.md"
  IfErrors 0 files_written
    SetErrorLevel 2
    Abort "Some Natamux files could not be written. Quit Natamux and run the installer again."
  files_written:
  CreateShortcut "$DESKTOP\Natamux.lnk" "$INSTDIR\natamux-app.exe" "" "$INSTDIR\natamux-app.exe" 0
  CreateShortcut "$SMPROGRAMS\Natamux.lnk" "$INSTDIR\natamux-app.exe" "" "$INSTDIR\natamux-app.exe" 0
  WriteUninstaller "$INSTDIR\Uninstall.exe"
  ; The entry in Settings > Apps.
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayName" "Natamux"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayIcon" '"$INSTDIR\natamux-app.exe",0'
  WriteRegStr HKCU "${UNINSTALL_KEY}" "UninstallString" '"$INSTDIR\Uninstall.exe"'
  WriteRegStr HKCU "${UNINSTALL_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "Publisher" "lenhatthanh20"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayVersion" "${VERSION}"
  ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "EstimatedSize" $0
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoRepair" 1
SectionEnd

Section "Uninstall"
  Delete "$DESKTOP\Natamux.lnk"
  Delete "$SMPROGRAMS\Natamux.lnk"
  Delete "$INSTDIR\natamux-app.exe"
  Delete "$INSTDIR\natamux-daemon.exe"
  Delete "$INSTDIR\natamux.exe"
  Delete "$INSTDIR\nmx.exe"
  Delete "$INSTDIR\LICENSE"
  Delete "$INSTDIR\THIRD-PARTY-NOTICES.md"
  Delete "$INSTDIR\THIRD-PARTY-RUST.md"
  Delete "$INSTDIR\Uninstall.exe"
  DeleteRegKey HKCU "${UNINSTALL_KEY}"
  RMDir "$INSTDIR"
SectionEnd
