//! Keep the process interactive while windows are hidden or miniaturized.
//! macOS App Nap otherwise delays the main queue, which looks like a freeze
//! on the next hotkey, tray click, or clipboard paste.

#[cfg(target_os = "macos")]
mod imp {
    use objc2_foundation::{NSActivityOptions, NSProcessInfo, NSString};
    use std::sync::atomic::{AtomicBool, Ordering};

    static STARTED: AtomicBool = AtomicBool::new(false);

    pub fn prevent_app_nap() {
        if STARTED.swap(true, Ordering::Relaxed) {
            return;
        }
        let activity = NSProcessInfo::processInfo().beginActivityWithOptions_reason(
            NSActivityOptions::UserInitiatedAllowingIdleSystemSleep,
            &NSString::from_str("LocalFlow stays awake for dictation"),
        );
        std::mem::forget(activity);
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    pub fn prevent_app_nap() {}
}

pub use imp::prevent_app_nap;

#[cfg(test)]
mod tests {
    use super::prevent_app_nap;

    #[test]
    fn prevent_app_nap_can_be_called_twice() {
        prevent_app_nap();
        prevent_app_nap();
    }
}
