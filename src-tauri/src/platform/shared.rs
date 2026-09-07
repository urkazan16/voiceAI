//! Host-agnostic helpers used by the Windows and Linux implementations.
//! macOS keeps its original in-file logic so a helper rename cannot drift it.
//! The helpers still compile on macOS so CI can unit-test them.

#![cfg_attr(target_os = "macos", allow(dead_code))]

use std::time::Duration;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Chord {
    pub control: bool,
    pub shift: bool,
    pub super_key: bool,
    pub alt: bool,
    pub space: bool,
}

impl Chord {
    pub fn parse(hotkey: &str) -> Self {
        let t = hotkey.to_ascii_lowercase();
        Self {
            control: t.contains("control") || t.contains("ctrl"),
            shift: t.contains("shift"),
            super_key: t.contains("command")
                || t.contains("cmd")
                || t.contains("super")
                || t.contains("meta")
                || t.contains("win"),
            alt: t.contains("option") || t.contains("alt"),
            space: t.contains("space"),
        }
    }

    pub fn combo_held(
        self,
        control: bool,
        shift: bool,
        super_key: bool,
        alt: bool,
        space: bool,
    ) -> bool {
        let mut required = false;
        if self.control {
            required = true;
            if !control {
                return false;
            }
        }
        if self.shift {
            required = true;
            if !shift {
                return false;
            }
        }
        if self.super_key {
            required = true;
            if !super_key {
                return false;
            }
        }
        if self.alt {
            required = true;
            if !alt {
                return false;
            }
        }
        if self.space {
            return required && space;
        }
        required
    }

    pub fn modifiers_held(self, control: bool, shift: bool, super_key: bool, alt: bool) -> bool {
        let mut any = false;
        if self.control {
            any = true;
            if !control {
                return false;
            }
        }
        if self.shift {
            any = true;
            if !shift {
                return false;
            }
        }
        if self.super_key {
            any = true;
            if !super_key {
                return false;
            }
        }
        if self.alt {
            any = true;
            if !alt {
                return false;
            }
        }
        any
    }
}

pub(crate) fn is_editor_or_terminal(app: Option<&str>) -> bool {
    let Some(name) = app else {
        return false;
    };
    let n = name.to_ascii_lowercase();
    n.contains("term")
        || n.contains("iterm")
        || n.contains("warp")
        || n.contains("kitty")
        || n.contains("ghostty")
        || n.contains("alacritty")
        || n.contains("wezterm")
        || n.contains("code")
        || n.contains("cursor")
        || n.contains("zed")
        || n.contains("xcode")
        || n.contains("sublime")
        || n.contains("vim")
        || n.contains("nvim")
        || n.contains("helix")
        || n.contains("notepad")
        || n.contains("powershell")
        || n.contains("pwsh")
        || n.contains("cmd.exe")
        || n.contains("windows terminal")
}

pub(crate) fn insert_pause(insert_delay_ms: u64, target_app: Option<&str>) -> Duration {
    let delay = insert_delay_ms.max(40);
    let extra = if is_editor_or_terminal(target_app) {
        delay.saturating_add(40)
    } else {
        delay
    };
    Duration::from_millis(extra)
}

pub fn default_copy_hotkey() -> &'static str {
    if cfg!(target_os = "macos") {
        "Command+Control+C"
    } else {
        "Control+Alt+C"
    }
}

pub fn default_paste_hotkey() -> &'static str {
    if cfg!(target_os = "macos") {
        "Command+Control+V"
    } else {
        "Control+Alt+V"
    }
}

pub fn default_edit_hotkey() -> &'static str {
    if cfg!(target_os = "macos") {
        "Command+Control+E"
    } else {
        "Control+Alt+E"
    }
}

pub fn talk_hotkey_fallbacks() -> [&'static str; 2] {
    if cfg!(target_os = "macos") {
        ["Control+Shift+Space", "Command+Shift+D"]
    } else {
        ["Control+Shift+Space", "Control+Shift+D"]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LinuxSession {
    X11,
    Wayland,
    Unknown,
}

pub(crate) fn linux_session(
    xdg_session_type: Option<&str>,
    wayland_display: bool,
    x11_display: bool,
) -> LinuxSession {
    let session = xdg_session_type.unwrap_or("").to_ascii_lowercase();
    if session == "x11" || (x11_display && session != "wayland") {
        return LinuxSession::X11;
    }
    if x11_display {
        // XWayland: prefer XTEST for X11 clients; native Wayland windows still
        // need the clipboard fallback below if the fake key is ignored.
        return LinuxSession::X11;
    }
    if session == "wayland" || wayland_display {
        return LinuxSession::Wayland;
    }
    LinuxSession::Unknown
}

pub(crate) fn wayland_paste_hint() -> &'static str {
    "Wayland does not let apps type into other windows. The text is on the clipboard — press Ctrl+V. Install wtype or ydotool for automatic paste."
}

pub(crate) fn linux_autostart_desktop(exec: &str) -> String {
    format!(
        "[Desktop Entry]\nType=Application\nVersion=1.0\nName=LocalFlow\nComment=Local voice-to-text pipeline\nExec={exec}\nTerminal=false\nX-GNOME-Autostart-enabled=true\n"
    )
}

pub(crate) fn linux_autostart_path(
    xdg_config_home: Option<&str>,
    home: Option<&str>,
) -> std::path::PathBuf {
    let config = if let Some(xdg) = xdg_config_home {
        let p = std::path::PathBuf::from(xdg);
        if p.is_absolute() {
            p
        } else {
            std::path::PathBuf::from(home.unwrap_or(".")).join(".config")
        }
    } else {
        std::path::PathBuf::from(home.unwrap_or(".")).join(".config")
    };
    config.join("autostart").join("app.localflow.desktop")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_hotkeys_match_the_host() {
        if cfg!(target_os = "macos") {
            assert_eq!(default_copy_hotkey(), "Command+Control+C");
            assert_eq!(default_paste_hotkey(), "Command+Control+V");
            assert_eq!(default_edit_hotkey(), "Command+Control+E");
            assert_eq!(talk_hotkey_fallbacks()[1], "Command+Shift+D");
        } else {
            assert_eq!(default_copy_hotkey(), "Control+Alt+C");
            assert_eq!(default_paste_hotkey(), "Control+Alt+V");
            assert_eq!(default_edit_hotkey(), "Control+Alt+E");
            assert_eq!(talk_hotkey_fallbacks()[1], "Control+Shift+D");
        }
    }

    #[test]
    fn talk_combo_requires_space_when_the_chord_names_it() {
        let chord = Chord::parse("Control+Shift+Space");
        assert!(chord.control && chord.shift && chord.space);
        assert!(chord.combo_held(true, true, false, false, true));
        assert!(!chord.combo_held(true, true, false, false, false));
        assert!(!chord.combo_held(true, false, false, false, true));
        assert!(chord.modifiers_held(true, true, false, false));
        assert!(!chord.modifiers_held(true, false, false, false));
    }

    #[test]
    fn empty_hotkey_is_never_held() {
        let chord = Chord::parse("");
        assert!(!chord.combo_held(true, true, true, true, true));
        assert!(!chord.modifiers_held(true, true, true, true));
    }

    #[test]
    fn xwayland_is_treated_as_x11_for_injection() {
        assert_eq!(
            linux_session(Some("wayland"), true, true),
            LinuxSession::X11
        );
        assert_eq!(
            linux_session(Some("wayland"), true, false),
            LinuxSession::Wayland
        );
        assert_eq!(linux_session(Some("x11"), false, true), LinuxSession::X11);
    }

    #[test]
    fn wayland_hint_tells_the_user_to_paste() {
        assert!(wayland_paste_hint().contains("Ctrl+V"));
        assert!(wayland_paste_hint()
            .to_ascii_lowercase()
            .contains("wayland"));
    }

    #[test]
    fn autostart_desktop_file_runs_the_binary() {
        let body = linux_autostart_desktop("/opt/LocalFlow/localflow");
        assert!(body.contains("Exec=/opt/LocalFlow/localflow"));
        assert!(body.contains("X-GNOME-Autostart-enabled=true"));
        let path = linux_autostart_path(Some("/home/u/.config"), Some("/home/u"));
        assert!(path.ends_with("autostart/app.localflow.desktop"));
    }

    #[test]
    fn editors_get_the_extra_insert_pause() {
        assert_eq!(insert_pause(40, Some("Code")).as_millis(), 80);
        assert_eq!(insert_pause(40, Some("Notepad")).as_millis(), 80);
        assert_eq!(insert_pause(40, Some("Slack")).as_millis(), 40);
        assert_eq!(insert_pause(10, None).as_millis(), 40);
    }
}
