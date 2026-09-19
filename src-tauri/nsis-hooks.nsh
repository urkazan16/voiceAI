; Tauri inserts macro bodies into a generated .nsi file. Capture this path at
; the top level: ${__FILEDIR__} inside a macro would point at that generated
; file, not this hook's directory.
!define LOCALFLOW_HOOK_DIR "${__FILEDIR__}"

; sherpa-onnx is linked as a Windows shared library. The Windows loader
; resolves it before LocalFlow's Rust code can run, so include every staged
; runtime DLL directly in the installer and extract it beside localflow.exe.
; `File` deliberately fails the NSIS build if staging did not produce a DLL.
!macro NSIS_HOOK_PREINSTALL
  SetOutPath "$INSTDIR"
  File "${LOCALFLOW_HOOK_DIR}\resources\runtime\*.dll"
!macroend

; These files are installed by the hook rather than Tauri's app payload, so
; remove them explicitly without touching WebView2.
!macro NSIS_HOOK_PREUNINSTALL
  Delete "$INSTDIR\sherpa-onnx*.dll"
  Delete "$INSTDIR\onnxruntime*.dll"
!macroend
