!macro NSIS_HOOK_POSTINSTALL
  DetailPrint "Checking system Node.js and npm..."
  ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --ensure-system-runtime' $0
  ${If} $0 != 0
    MessageBox MB_ICONSTOP|MB_OK "Node.js/npm installation or validation failed. tauri-codex was installed, but Codex cannot start until Setup completes the system runtime installation."
    Abort
  ${EndIf}
!macroend
