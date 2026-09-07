use crate::error::LfResult;
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
    crate::platform::current().accessibility_trusted()
}

pub fn open_pane(kind: &str) -> LfResult<()> {
    crate::platform::current().open_privacy_pane(kind)
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
