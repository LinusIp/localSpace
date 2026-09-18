; What localSpace adds to the installer, through the hooks tauri-cli's template
; offers (docs/DECISIONS.md, 2026-09-18, the answers after day 1).

; "Delete the application data" means the person's conversations, boards and
; downloaded models. They live in %LOCALAPPDATA%\localSpace, not under the
; bundle identifier the template clears by itself (that one holds only the
; webview's own files). Asked for by the checkbox and never during an update.
!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $DeleteAppDataCheckboxState = 1
  ${AndIf} $UpdateMode <> 1
    SetShellVarContext current
    RMDir /r "$LOCALAPPDATA\localSpace"
  ${EndIf}
!macroend
