!ifndef BIN
  !error "BIN is required"
!endif
!ifndef OUT
  !error "OUT is required"
!endif

Name "Natamux"
OutFile "${OUT}\Natamux-win-x64.exe"
InstallDir "$LOCALAPPDATA\Programs\Natamux"
RequestExecutionLevel user
Unicode True

Section "Install"
  SetOutPath "$INSTDIR"
  ; Not Natamux.exe: Windows names ignore case, so it would be the CLI natamux.exe.
  File "${BIN}\natamux-app.exe"
  File "${BIN}\natamux-daemon.exe"
  File "${BIN}\natamux.exe"
  File "${BIN}\nmx.exe"
  CreateShortcut "$DESKTOP\Natamux.lnk" "$INSTDIR\natamux-app.exe"
  CreateShortcut "$SMPROGRAMS\Natamux.lnk" "$INSTDIR\natamux-app.exe"
  WriteUninstaller "$INSTDIR\Uninstall.exe"
SectionEnd

Section "Uninstall"
  Delete "$DESKTOP\Natamux.lnk"
  Delete "$SMPROGRAMS\Natamux.lnk"
  Delete "$INSTDIR\natamux-app.exe"
  Delete "$INSTDIR\natamux-daemon.exe"
  Delete "$INSTDIR\natamux.exe"
  Delete "$INSTDIR\nmx.exe"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir "$INSTDIR"
SectionEnd
