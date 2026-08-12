!macro NSIS_HOOK_POSTINSTALL
  Call CreateOrUpdateDesktopShortcut
  DetailPrint "Checking thin installer bootstrap..."
  ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --thin-setup' $0
  ${If} $0 != 0
    MessageBox MB_ICONSTOP|MB_OK "tauri-codex was installed, but its thin installer bootstrap is invalid. Please report an Issue with the installer version."
    Abort
  ${EndIf}
  ${If} ${Silent}
    Exec '"$INSTDIR\${MAINBINARYNAME}.exe"'
  ${EndIf}
!macroend
