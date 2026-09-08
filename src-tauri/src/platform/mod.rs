//! The single seam between LocalFlow and the host operating system.
//!
//! Everything only one OS can answer lives behind [`Platform`]: pasting into
//! the frontmost application, reading physically held modifier keys, autostart,
//! the session lock, and where user data belongs. Code that is POSIX-wide
//! rather than per-OS — `flock`, `statfs`, `chmod 0700` — stays with its own
//! module behind `cfg(unix)`, because splitting it per OS would only duplicate
//! it into macOS and Linux.

use crate::error::LfResult;
use std::path::PathBuf;

#[cfg(all(unix, not(target_os = "macos")))]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
mod shared;
#[cfg(windows)]
mod windows;

#[cfg(windows)]
pub(crate) fn restack_windows_overlay(hwnd: isize) {
    windows::restack_overlay_without_activating(hwnd);
}

pub use shared::{
    default_copy_hotkey, default_edit_hotkey, default_paste_hotkey, talk_hotkey_fallbacks,
};

/// A clipboard flavour and its raw bytes, e.g. `("public.utf8-plain-text", …)`.
pub type ClipboardItem = (String, Vec<u8>);

/// Short confirmation sound played around an utterance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cue {
    Start,
    End,
}

/// One paste into whatever application currently owns the caret.
pub struct InsertRequest<'a> {
    pub text: &'a str,
    pub restore_clipboard: bool,
    pub target_pid: Option<i32>,
    pub target_app: Option<&'a str>,
    pub insert_delay_ms: u64,
}

pub trait Platform: Send + Sync {
    /// Where `~/Library/Application Support`, `%APPDATA%`, or `$XDG_DATA_HOME`
    /// puts the LocalFlow directory. `LOCALFLOW_DATA_DIR` overrides it earlier,
    /// in [`crate::paths::DataPaths::detect`].
    fn data_root(&self) -> PathBuf;

    /// Plain-text clipboard write. Used by Copy last and by clipboard recovery;
    /// insertion has its own richer path inside [`Platform::insert_text`].
    fn set_clipboard_text(&self, text: &str) -> LfResult<()>;

    /// Puts every captured flavour back after a paste. `false` means the host
    /// cannot hold anything richer than text, so the caller falls back to the
    /// plain-text backup.
    fn restore_clipboard_items(&self, items: &[ClipboardItem]) -> bool;

    fn insert_text(&self, request: &InsertRequest<'_>) -> LfResult<()>;

    /// Clears modifiers left down by the talk chord so the paste is not
    /// reinterpreted as another shortcut.
    fn prepare_keyboard(&self);

    /// Process id and name of the application that will receive the text.
    fn frontmost_target(&self) -> (Option<i32>, Option<String>);

    fn activate_pid(&self, pid: u32) -> bool;

    /// True while the whole talk chord is physically held, Space included.
    fn talk_combo_held(&self, hotkey: &str) -> bool;

    /// True while the chord's modifiers are held, ignoring Space.
    fn talk_modifiers_held(&self, hotkey: &str) -> bool;

    fn space_key_down(&self) -> bool;

    fn accessibility_trusted(&self) -> bool;

    /// Ask macOS to show the Accessibility prompt if the process is not yet
    /// trusted. No-op on other hosts. Must not be called from insert/paste.
    fn prompt_accessibility(&self) {}

    fn open_privacy_pane(&self, kind: &str) -> LfResult<()>;

    fn set_autostart(&self, enabled: bool) -> LfResult<()>;

    fn screen_is_locked(&self) -> bool;

    fn play_cue(&self, cue: Cue, volume: f32);

    /// Native "already running" notice for the second copy of the app.
    fn report_already_running(&self, message: &str);
}

#[cfg(target_os = "macos")]
pub fn current() -> &'static dyn Platform {
    &macos::MacOs
}

#[cfg(windows)]
pub fn current() -> &'static dyn Platform {
    &windows::Windows
}

#[cfg(all(unix, not(target_os = "macos")))]
pub fn current() -> &'static dyn Platform {
    &linux::Linux
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_root_is_absolute_and_named_after_the_app() {
        let root = current().data_root();
        assert!(root.is_absolute(), "{}", root.display());
        assert_eq!(
            root.file_name().and_then(|n| n.to_str()),
            Some(crate::paths::APP_DIR_NAME)
        );
    }

    #[test]
    fn talk_chord_helpers_agree_on_an_empty_hotkey() {
        let host = current();
        assert!(!host.talk_combo_held(""));
        assert!(!host.talk_modifiers_held(""));
    }

    #[test]
    fn unknown_privacy_pane_is_rejected() {
        let err = current().open_privacy_pane("nonsense").unwrap_err();
        assert!(
            matches!(
                err.code(),
                "CONFIG_INVALID" | "RUNTIME_UNSUPPORTED" | "ERROR"
            ),
            "{err}"
        );
    }

    #[test]
    fn windows_and_linux_paste_via_ctrl_v_never_select_all() {
        let win = include_str!("windows.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        assert!(win.contains("VK_V"), "Windows paste must send Ctrl+V");
        assert!(
            !win.contains("VK_A"),
            "Windows paste must not send Select-All"
        );
        let linux = include_str!("linux.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        assert!(linux.contains("XK_V"), "Linux paste must send Ctrl+V");
        assert!(
            !linux.to_ascii_lowercase().contains("ctrl+a"),
            "Linux paste must not send Select-All"
        );
        assert!(
            linux.contains("compositor_frontmost"),
            "Wayland must capture the focused client without X11"
        );
        assert!(
            linux.contains("privacy_command_opened") && linux.contains("try_wait"),
            "opening settings must not succeed just because spawn() did not immediately fail"
        );
    }
}
