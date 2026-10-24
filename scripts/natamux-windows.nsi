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
  File "/oname=Natamux.exe" "${BIN}\natamux-app.exe"
  File "${BIN}\natamux-daemon.exe"
  File "${BIN}\natamux.exe"
  File "${BIN}\nmx.exe"
  CreateShortcut "$DESKTOP\Natamux.lnk" "$INSTDIR\Natamux.exe"
  CreateShortcut "$SMPROGRAMS\Natamux.lnk" "$INSTDIR\Natamux.exe"
  WriteUninstaller "$INSTDIR\Uninstall.exe"
SectionEnd

Section "Uninstall"
  Delete "$DESKTOP\Natamux.lnk"
  Delete "$SMPROGRAMS\Natamux.lnk"
  Delete "$INSTDIR\Natamux.exe"
  Delete "$INSTDIR\natamux-daemon.exe"
  Delete "$INSTDIR\natamux.exe"
  Delete "$INSTDIR\nmx.exe"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir "$INSTDIR"
SectionEnd
