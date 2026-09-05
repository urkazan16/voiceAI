use crate::error::{LfError, LfResult};
use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PermissionStatus {
    pub microphone_device_count: usize,
    pub accessibility_trusted: bool,
}

pub fn status() -> PermissionStatus {
    let microphone_device_count = crate::audio::list_input_devices()
        .map(|d| d.len())
        .unwrap_or(0);
    PermissionStatus {
        microphone_device_count,
        accessibility_trusted: accessibility_trusted(),
    }
}

pub fn accessibility_trusted() -> bool {
    #[cfg(target_os = "macos")]
    {
        macos::ax_trusted()
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

pub fn open_pane(kind: &str) -> LfResult<()> {
    #[cfg(target_os = "macos")]
    {
        macos::open_pane(kind)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = kind;
        Err(LfError::RuntimeUnsupported(
            "privacy panes are opened on macOS only".into(),
        ))
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXIsProcessTrusted() -> bool;
    }

    pub fn ax_trusted() -> bool {
        unsafe { AXIsProcessTrusted() }
    }

    pub fn open_pane(kind: &str) -> LfResult<()> {
        let urls: &[&str] = match kind {
            "microphone" => &[
                "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_Microphone",
                "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone",
            ],
            "accessibility" => &[
                "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_Accessibility",
                "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
            ],
            "speech" => &[
                "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_SpeechRecognition",
                "x-apple.systempreferences:com.apple.preference.security?Privacy_SpeechRecognition",
            ],
            _ => {
                return Err(LfError::ConfigInvalid(format!(
                    "unknown privacy pane {kind}"
                )))
            }
        };
        for url in urls {
            if std::process::Command::new("open")
                .arg(url)
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
            {
                return Ok(());
            }
        }
        Err(LfError::Other(format!(
            "could not open System Settings for {kind}"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_struct_is_serializable() {
        let json = serde_json::to_string(&status()).unwrap();
        assert!(json.contains("microphone_device_count"));
        assert!(json.contains("accessibility_trusted"));
    }
}
