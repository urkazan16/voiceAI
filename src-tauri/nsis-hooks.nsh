; sherpa-onnx is linked as a Windows shared library. The loader resolves it
; before LocalFlow's Rust code can run, so it must live beside localflow.exe;
; `$INSTDIR\resources` is too late in the DLL search path.
!macro NSIS_HOOK_POSTINSTALL
  CopyFiles /SILENT "$INSTDIR\resources\runtime\*.dll" "$INSTDIR"
!macroend

; These files are copied by the hook rather than by Tauri's resource list, so
; remove them explicitly when the app is uninstalled without touching WebView2.
!macro NSIS_HOOK_PREUNINSTALL
  Delete "$INSTDIR\sherpa-onnx*.dll"
  Delete "$INSTDIR\onnxruntime*.dll"
!macroend
