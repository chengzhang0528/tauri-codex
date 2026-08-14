!macro NSIS_HOOK_PREINSTALL
  ${If} ${FileExists} "$INSTDIR\${MAINBINARYNAME}.exe"
    ReadRegStr $R1 SHCTX "${MANUPRODUCTKEY}" ""
    ${If} $R1 == ""
      Abort "An existing tauri-codex Launcher was found outside a registered installation. Remove it or choose the registered installation directory."
    ${EndIf}
    ${If} $R1 != $INSTDIR
      Abort "The selected tauri-codex directory does not match the registered installation. In-place repair and upgrade must use the registered directory."
    ${EndIf}
    DetailPrint "Waiting for running tauri-codex processes to close..."
    ${If} ${Silent}
    ${OrIf} $PassiveMode = 1
      ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --installer-takeover "$INSTDIR" --silent' $0
    ${Else}
      ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --installer-takeover "$INSTDIR"' $0
    ${EndIf}
    ${If} $0 != 0
      Abort "tauri-codex installation was cancelled because running product processes could not be closed safely."
    ${EndIf}
  ${Else}
    nsis_tauri_utils::FindProcess "${MAINBINARYNAME}.exe"
    Pop $0
    ${If} $0 = 0
      Abort "A same-named process is already running outside this installation. Close it before installing tauri-codex."
    ${EndIf}
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTINSTALL
  Call CreateOrUpdateDesktopShortcut
  DetailPrint "Checking thin installer bootstrap..."
  ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --thin-setup' $0
  ${If} $0 != 0
    MessageBox MB_ICONSTOP|MB_OK "tauri-codex was installed, but its thin installer bootstrap is invalid. Please report an Issue with the installer version."
    Abort
  ${EndIf}
  ${If} ${Silent}
  ${OrIf} $PassiveMode = 1
    ClearErrors
    ${GetOptions} $CMDLINE "/R" $0
    ${If} ${Errors}
      nsis_tauri_utils::RunAsUser "$INSTDIR\${MAINBINARYNAME}.exe" ""
    ${EndIf}
  ${EndIf}
!macroend
