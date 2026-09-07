//! Linux host integration.
//!
//! X11 (including XWayland) pastes with XTEST Ctrl+V. Native Wayland cannot
//! inject into another client: the text is copied and the user is told to
//! press Ctrl+V, unless `wtype` or `ydotool` is on PATH.

use super::shared::{
    self, linux_autostart_desktop, linux_autostart_path, linux_session, wayland_paste_hint, Chord,
    LinuxSession,
};
use super::{ClipboardItem, Cue, InsertRequest, Platform};
use crate::error::{LfError, LfResult};
use crate::injection;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub struct Linux;

const CLIP_UTF8: &str = "text/plain;charset=utf-8";
const XK_CONTROL_L: u32 = 0xffe3;
const XK_SHIFT_L: u32 = 0xffe1;
const XK_ALT_L: u32 = 0xffe9;
const XK_SUPER_L: u32 = 0xffeb;
const XK_SPACE: u32 = 0x0020;
const XK_V: u32 = 0x0076;
const KEY_PRESS: u8 = 2;
const KEY_RELEASE: u8 = 3;

fn data_root_from(xdg_data_home: Option<&str>, home: Option<&str>) -> PathBuf {
    if let Some(xdg) = xdg_data_home {
        let xdg = PathBuf::from(xdg);
        if xdg.is_absolute() {
            return xdg.join(crate::paths::APP_DIR_NAME);
        }
    }
    PathBuf::from(home.unwrap_or("."))
        .join(".local")
        .join("share")
        .join(crate::paths::APP_DIR_NAME)
}

fn current_session() -> LinuxSession {
    linux_session(
        std::env::var("XDG_SESSION_TYPE").ok().as_deref(),
        std::env::var_os("WAYLAND_DISPLAY").is_some(),
        std::env::var_os("DISPLAY").is_some(),
    )
}

fn spawn_stdin(program: &str, args: &[&str], text: &str) -> bool {
    let Ok(mut child) = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    else {
        return false;
    };
    if let Some(stdin) = child.stdin.as_mut() {
        let _ = stdin.write_all(text.as_bytes());
    }
    child.wait().map(|s| s.success()).unwrap_or(false)
}

fn spawn_stdout(program: &str, args: &[&str]) -> Option<Vec<u8>> {
    let output = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .ok()?;
    if output.status.success() {
        Some(output.stdout)
    } else {
        None
    }
}

fn set_clipboard_text(text: &str) -> LfResult<()> {
    let wayland_first = current_session() == LinuxSession::Wayland;
    let attempts: &[&[&str]] = if wayland_first {
        &[
            &["wl-copy", "--type", "text/plain;charset=utf-8"],
            &["wl-copy"],
            &["xclip", "-selection", "clipboard"],
            &["xsel", "--clipboard", "--input"],
        ]
    } else {
        &[
            &["xclip", "-selection", "clipboard"],
            &["xsel", "--clipboard", "--input"],
            &["wl-copy", "--type", "text/plain;charset=utf-8"],
            &["wl-copy"],
        ]
    };
    for spec in attempts {
        let (program, args) = spec.split_first().unwrap();
        if spawn_stdin(program, args, text) {
            return Ok(());
        }
    }
    Err(LfError::InjectionFailed(
        "clipboard tools missing (install xclip, xsel, or wl-clipboard)".into(),
    ))
}

fn snapshot_clipboard() -> Vec<ClipboardItem> {
    let wayland_first = current_session() == LinuxSession::Wayland;
    let attempts: &[&[&str]] = if wayland_first {
        &[
            &["wl-paste", "--no-newline"],
            &["xclip", "-selection", "clipboard", "-o"],
            &["xsel", "--clipboard", "--output"],
        ]
    } else {
        &[
            &["xclip", "-selection", "clipboard", "-o"],
            &["xsel", "--clipboard", "--output"],
            &["wl-paste", "--no-newline"],
        ]
    };
    for spec in attempts {
        let (program, args) = spec.split_first().unwrap();
        if let Some(bytes) = spawn_stdout(program, args) {
            if bytes.len() <= 16 * 1024 * 1024 {
                return vec![(CLIP_UTF8.to_string(), bytes)];
            }
        }
    }
    Vec::new()
}

fn restore_clipboard(items: &[ClipboardItem]) -> bool {
    let Some((_, bytes)) = items.iter().find(|(ty, _)| ty == CLIP_UTF8) else {
        return false;
    };
    let Ok(text) = std::str::from_utf8(bytes) else {
        return false;
    };
    set_clipboard_text(text).is_ok()
}

fn snapshot_plain_text(items: &[ClipboardItem]) -> Option<String> {
    items.iter().find_map(|(ty, bytes)| {
        if ty == CLIP_UTF8 {
            std::str::from_utf8(bytes).ok().map(str::to_string)
        } else {
            None
        }
    })
}

mod x11 {
    use super::*;
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{AtomEnum, ConnectionExt, EventMask, Window};
    use x11rb::protocol::xtest::ConnectionExt as _;
    use x11rb::rust_connection::RustConnection;

    pub struct Display {
        conn: RustConnection,
        root: Window,
    }

    impl Display {
        pub fn connect() -> Option<Self> {
            let (conn, screen) = x11rb::connect(None).ok()?;
            let root = conn.setup().roots.get(screen)?.root;
            Some(Self { conn, root })
        }

        fn keycode(&self, keysym: u32) -> Option<u8> {
            let setup = self.conn.setup();
            let min = setup.min_keycode;
            let count = setup.max_keycode.saturating_sub(min).saturating_add(1);
            let mapping = self
                .conn
                .get_keyboard_mapping(min, count)
                .ok()?
                .reply()
                .ok()?;
            let width = mapping.keysyms_per_keycode as usize;
            if width == 0 {
                return None;
            }
            for (i, chunk) in mapping.keysyms.chunks(width).enumerate() {
                if chunk.contains(&keysym) {
                    return Some(min.saturating_add(i as u8));
                }
            }
            None
        }

        fn keymap(&self) -> Option<[u8; 32]> {
            Some(self.conn.query_keymap().ok()?.reply().ok()?.keys)
        }

        pub fn key_down(&self, keysym: u32) -> bool {
            let Some(code) = self.keycode(keysym) else {
                return false;
            };
            let Some(keys) = self.keymap() else {
                return false;
            };
            let byte = (code / 8) as usize;
            let bit = code % 8;
            keys.get(byte).is_some_and(|b| b & (1 << bit) != 0)
        }

        fn fake_key(&self, keysym: u32, press: bool) -> LfResult<()> {
            let code = self.keycode(keysym).ok_or_else(|| {
                LfError::InjectionFailed("X11 keyboard mapping has no V/Control keys".into())
            })?;
            let ty = if press { KEY_PRESS } else { KEY_RELEASE };
            self.conn
                .xtest_fake_input(ty, code, 0, 0, 0, 0, 0)
                .map_err(|e| LfError::InjectionFailed(e.to_string()))?;
            self.conn
                .flush()
                .map_err(|e| LfError::InjectionFailed(e.to_string()))?;
            Ok(())
        }

        pub fn post_paste(&self) -> LfResult<()> {
            self.fake_key(XK_CONTROL_L, true)?;
            self.fake_key(XK_V, true)?;
            self.fake_key(XK_V, false)?;
            self.fake_key(XK_CONTROL_L, false)?;
            Ok(())
        }

        pub fn release_modifiers(&self) {
            for keysym in [XK_CONTROL_L, XK_SHIFT_L, XK_ALT_L, XK_SUPER_L] {
                if self.key_down(keysym) {
                    let _ = self.fake_key(keysym, false);
                }
            }
        }

        fn intern(&self, name: &str) -> Option<u32> {
            let cookie = self.conn.intern_atom(false, name.as_bytes()).ok()?;
            let reply = cookie.reply().ok()?;
            if reply.atom == 0 {
                None
            } else {
                Some(reply.atom)
            }
        }

        fn window_pid(&self, window: Window) -> Option<u32> {
            let atom = self.intern("_NET_WM_PID")?;
            let reply = self
                .conn
                .get_property(false, window, atom, AtomEnum::CARDINAL, 0, 1)
                .ok()?
                .reply()
                .ok()?;
            let pid = reply.value32()?.next()?;
            Some(pid)
        }

        fn window_name(&self, window: Window) -> Option<String> {
            if let Some(atom) = self.intern("_NET_WM_NAME") {
                if let Some(utf8) = self.intern("UTF8_STRING") {
                    if let Ok(cookie) = self.conn.get_property(false, window, atom, utf8, 0, 256) {
                        if let Ok(reply) = cookie.reply() {
                            if let Ok(s) = std::str::from_utf8(&reply.value) {
                                let t = s.trim();
                                if !t.is_empty() {
                                    return Some(t.to_string());
                                }
                            }
                        }
                    }
                }
            }
            let reply = self
                .conn
                .get_property(false, window, AtomEnum::WM_CLASS, AtomEnum::STRING, 0, 256)
                .ok()?
                .reply()
                .ok()?;
            let raw = std::str::from_utf8(&reply.value).ok()?;
            raw.split('\0').find(|s| !s.is_empty()).map(str::to_string)
        }

        pub fn frontmost(&self) -> (Option<i32>, Option<String>) {
            let Some(atom) = self.intern("_NET_ACTIVE_WINDOW") else {
                return (None, None);
            };
            let Ok(cookie) = self
                .conn
                .get_property(false, self.root, atom, AtomEnum::WINDOW, 0, 1)
            else {
                return (None, None);
            };
            let Ok(reply) = cookie.reply() else {
                return (None, None);
            };
            let Some(window) = reply.value32().and_then(|mut v| v.next()) else {
                return (None, None);
            };
            if window == 0 {
                return (None, None);
            }
            (
                self.window_pid(window).map(|p| p as i32),
                self.window_name(window),
            )
        }

        pub fn activate_pid(&self, pid: u32) -> bool {
            let Some(list_atom) = self.intern("_NET_CLIENT_LIST") else {
                return false;
            };
            let Ok(cookie) =
                self.conn
                    .get_property(false, self.root, list_atom, AtomEnum::WINDOW, 0, 1024)
            else {
                return false;
            };
            let Ok(reply) = cookie.reply() else {
                return false;
            };
            let Some(windows) = reply.value32() else {
                return false;
            };
            for window in windows {
                if self.window_pid(window) != Some(pid) {
                    continue;
                }
                let Some(active) = self.intern("_NET_ACTIVE_WINDOW") else {
                    return false;
                };
                let event = x11rb::protocol::xproto::ClientMessageEvent::new(
                    32,
                    window,
                    active,
                    [2, 0, 0, 0, 0],
                );
                let mask = EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY;
                if self.conn.send_event(false, self.root, mask, event).is_ok() {
                    let _ = self.conn.flush();
                    return true;
                }
            }
            false
        }
    }
}

fn with_x11<T>(f: impl FnOnce(&x11::Display) -> T, fallback: T) -> T {
    match x11::Display::connect() {
        Some(display) => f(&display),
        None => fallback,
    }
}

fn wait_for_modifiers_up(timeout: Duration) {
    let start = Instant::now();
    while with_x11(
        |d| {
            d.key_down(XK_CONTROL_L)
                || d.key_down(XK_SHIFT_L)
                || d.key_down(XK_ALT_L)
                || d.key_down(XK_SUPER_L)
        },
        false,
    ) && start.elapsed() < timeout
    {
        std::thread::sleep(Duration::from_millis(16));
    }
    with_x11(
        |d| {
            d.release_modifiers();
        },
        (),
    );
}

fn prepare_keyboard() {
    wait_for_modifiers_up(Duration::from_millis(250));
}

fn tool_paste(program: &str, args: &[&str]) -> bool {
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn post_paste() -> LfResult<()> {
    if with_x11(|d| d.post_paste().is_ok(), false) {
        return Ok(());
    }
    if tool_paste("xdotool", &["key", "--clearmodifiers", "ctrl+v"]) {
        return Ok(());
    }
    if tool_paste("wtype", &["-M", "ctrl", "v", "-m", "ctrl"]) {
        return Ok(());
    }
    if tool_paste("ydotool", &["key", "29:1", "47:1", "47:0", "29:0"]) {
        return Ok(());
    }
    Err(LfError::InjectionFailed(wayland_paste_hint().into()))
}

fn screen_locked_via_loginctl() -> bool {
    let session = std::env::var("XDG_SESSION_ID").unwrap_or_else(|_| "self".into());
    let output = Command::new("loginctl")
        .args(["show-session", &session, "--property=LockedHint"])
        .output();
    let Ok(output) = output else {
        return false;
    };
    String::from_utf8_lossy(&output.stdout).contains("LockedHint=yes")
}

fn screen_locked_via_screensaver() -> bool {
    let output = Command::new("gdbus")
        .args([
            "call",
            "--session",
            "--dest",
            "org.freedesktop.ScreenSaver",
            "--object-path",
            "/org/freedesktop/ScreenSaver",
            "--method",
            "org.freedesktop.ScreenSaver.GetActive",
        ])
        .output();
    let Ok(output) = output else {
        return false;
    };
    let text = String::from_utf8_lossy(&output.stdout);
    text.contains("true") || text.contains("true,")
}

impl Platform for Linux {
    fn data_root(&self) -> PathBuf {
        data_root_from(
            std::env::var("XDG_DATA_HOME").ok().as_deref(),
            std::env::var("HOME").ok().as_deref(),
        )
    }

    fn set_clipboard_text(&self, text: &str) -> LfResult<()> {
        set_clipboard_text(text)
    }

    fn restore_clipboard_items(&self, items: &[ClipboardItem]) -> bool {
        restore_clipboard(items)
    }

    fn insert_text(&self, request: &InsertRequest<'_>) -> LfResult<()> {
        prepare_keyboard();
        if let Some(pid) = request.target_pid {
            let _ = with_x11(|d| d.activate_pid(pid as u32), false);
        }
        std::thread::sleep(shared::insert_pause(
            request.insert_delay_ms,
            request.target_app,
        ));

        let previous = if request.restore_clipboard {
            Some(snapshot_clipboard())
        } else {
            None
        };
        if let Some(prev) = &previous {
            let _ = injection::persist_clipboard_snapshot(prev);
            if let Some(plain) = snapshot_plain_text(prev) {
                let _ = injection::persist_clipboard_backup(&plain);
            }
        }

        set_clipboard_text(request.text)?;
        std::thread::sleep(Duration::from_millis(16));
        let paste_result = post_paste();
        with_x11(
            |d| {
                d.release_modifiers();
            },
            (),
        );

        if let Some(prev) = previous {
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(80));
                let _ = restore_clipboard(&prev);
                injection::clear_clipboard_backups();
            });
        }
        paste_result
    }

    fn prepare_keyboard(&self) {
        prepare_keyboard();
    }

    fn frontmost_target(&self) -> (Option<i32>, Option<String>) {
        with_x11(|d| d.frontmost(), (None, None))
    }

    fn activate_pid(&self, pid: u32) -> bool {
        with_x11(|d| d.activate_pid(pid), false)
    }

    fn talk_combo_held(&self, hotkey: &str) -> bool {
        with_x11(
            |d| {
                Chord::parse(hotkey).combo_held(
                    d.key_down(XK_CONTROL_L),
                    d.key_down(XK_SHIFT_L),
                    d.key_down(XK_SUPER_L),
                    d.key_down(XK_ALT_L),
                    d.key_down(XK_SPACE),
                )
            },
            false,
        )
    }

    fn talk_modifiers_held(&self, hotkey: &str) -> bool {
        with_x11(
            |d| {
                Chord::parse(hotkey).modifiers_held(
                    d.key_down(XK_CONTROL_L),
                    d.key_down(XK_SHIFT_L),
                    d.key_down(XK_SUPER_L),
                    d.key_down(XK_ALT_L),
                )
            },
            false,
        )
    }

    fn space_key_down(&self) -> bool {
        with_x11(|d| d.key_down(XK_SPACE), false)
    }

    fn accessibility_trusted(&self) -> bool {
        true
    }

    fn open_privacy_pane(&self, kind: &str) -> LfResult<()> {
        let commands: &[&[&str]] = match kind {
            "microphone" => &[
                &["gnome-control-center", "sound"],
                &["systemsettings", "kcm_pulseaudio"],
                &["pavucontrol"],
                &["xdg-open", "settings://sound"],
            ],
            "accessibility" => &[
                &["gnome-control-center", "universal-access"],
                &["systemsettings", "kcm_access"],
            ],
            "speech" => &[&["gnome-control-center", "sound"]],
            _ => {
                return Err(LfError::ConfigInvalid(format!(
                    "unknown privacy pane {kind}"
                )))
            }
        };
        for spec in commands {
            let (program, args) = spec.split_first().unwrap();
            if Command::new(program)
                .args(args)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .is_ok()
            {
                return Ok(());
            }
        }
        Err(LfError::RuntimeUnsupported(format!(
            "could not open a system settings page for {kind}"
        )))
    }

    fn set_autostart(&self, enabled: bool) -> LfResult<()> {
        let path = linux_autostart_path(
            std::env::var("XDG_CONFIG_HOME").ok().as_deref(),
            std::env::var("HOME").ok().as_deref(),
        );
        if enabled {
            let exe = std::env::current_exe()?;
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&path, linux_autostart_desktop(&exe.display().to_string()))?;
        } else if path.exists() {
            std::fs::remove_file(&path)?;
        }
        Ok(())
    }

    fn screen_is_locked(&self) -> bool {
        screen_locked_via_loginctl() || screen_locked_via_screensaver()
    }

    fn play_cue(&self, cue: Cue, volume: f32) {
        let vol = crate::config::clamp_cue_volume(volume);
        let pulse = (vol * 65536.0).round() as i32;
        let names = match cue {
            Cue::Start => &[
                "/usr/share/sounds/freedesktop/stereo/message.oga",
                "/usr/share/sounds/freedesktop/stereo/bell.oga",
            ],
            Cue::End => &[
                "/usr/share/sounds/freedesktop/stereo/complete.oga",
                "/usr/share/sounds/freedesktop/stereo/dialog-information.oga",
            ],
        };
        for path in *names {
            if Command::new("paplay")
                .args(["--volume", &pulse.to_string(), path])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .is_ok()
            {
                return;
            }
            if Command::new("pw-play")
                .arg(path)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .is_ok()
            {
                return;
            }
        }
        let id = match cue {
            Cue::Start => "message-new-instant",
            Cue::End => "complete",
        };
        let _ = Command::new("canberra-gtk-play")
            .args(["-i", id])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
    }

    fn report_already_running(&self, message: &str) {
        let _ = Command::new("notify-send")
            .args(["LocalFlow", message])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xdg_data_home_wins_over_home() {
        assert_eq!(
            data_root_from(Some("/tmp/xdg-data"), Some("/home/tester")),
            PathBuf::from("/tmp/xdg-data/LocalFlow")
        );
    }

    #[test]
    fn without_xdg_the_default_share_dir_is_used() {
        assert_eq!(
            data_root_from(None, Some("/home/tester")),
            PathBuf::from("/home/tester/.local/share/LocalFlow")
        );
    }

    #[test]
    fn a_relative_xdg_data_home_is_ignored_per_spec() {
        assert_eq!(
            data_root_from(Some("relative/share"), Some("/home/tester")),
            PathBuf::from("/home/tester/.local/share/LocalFlow")
        );
    }

    #[test]
    fn paste_posts_ctrl_v_and_never_select_all() {
        let prod = include_str!("linux.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        assert!(prod.contains("XK_V"), "paste must send Ctrl+V");
        assert!(
            !prod.to_ascii_lowercase().contains("ctrl+a"),
            "Select-All would wipe the field and break mid-text insert"
        );
        assert!(prod.contains("wait_for_modifiers_up(Duration::from_millis(250))"));
        assert!(prod.contains("wayland_paste_hint"));
    }
}
