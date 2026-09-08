//! macOS host integration. Moved here verbatim from `injection.rs`,
//! `permissions.rs`, `autostart.rs`, `cues.rs`, `screenlock.rs`, and
//! `instance.rs`; the behaviour is unchanged.

use super::{ClipboardItem, Cue, InsertRequest, Platform};
use crate::error::{LfError, LfResult};
use std::ffi::c_void;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub struct MacOs;

const LAUNCH_AGENT_LABEL: &str = "app.localflow.desktop";

const VK_COMMAND: u16 = 0x37;
const VK_RIGHT_COMMAND: u16 = 0x36;
const VK_SHIFT: u16 = 0x38;
const VK_RIGHT_SHIFT: u16 = 0x3C;
const VK_CONTROL: u16 = 0x3B;
const VK_RIGHT_CONTROL: u16 = 0x3E;
const VK_OPTION: u16 = 0x3A;
const VK_RIGHT_OPTION: u16 = 0x3D;
const VK_SPACE: u16 = 0x31;
const VK_ANSI_V: u16 = 0x09;
const COMMAND_FLAG: u64 = 0x0010_0000;
const HID_EVENT_TAP: u32 = 0;
const SESSION_EVENT_TAP: u32 = 1;
const HID_SYSTEM_STATE: i32 = 1;
const COMBINED_SESSION_STATE: i32 = 0;

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventSourceCreate(state_id: i32) -> *mut c_void;
    fn CGEventSourceKeyState(state_id: i32, key: u16) -> bool;
    fn CGEventCreateKeyboardEvent(
        source: *mut c_void,
        virtual_key: u16,
        key_down: bool,
    ) -> *mut c_void;
    fn CGEventSetFlags(event: *mut c_void, flags: u64);
    fn CGEventPost(tap: u32, event: *mut c_void);
    fn CGEventPostToPid(pid: i32, event: *mut c_void);
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRelease(cf: *const c_void);
}

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> bool;
}

extern "C" {
    fn lf_screen_is_locked() -> i32;
    fn getuid() -> u32;
}

impl Platform for MacOs {
    fn data_root(&self) -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join(crate::paths::APP_DIR_NAME)
    }

    fn set_clipboard_text(&self, text: &str) -> LfResult<()> {
        pbcopy(text)
    }

    fn restore_clipboard_items(&self, items: &[ClipboardItem]) -> bool {
        if items.is_empty() {
            return false;
        }
        restore_pasteboard(&PasteboardSnapshot {
            items: items.to_vec(),
        });
        true
    }

    fn insert_text(&self, request: &InsertRequest<'_>) -> LfResult<()> {
        if secure_event_input_enabled() {
            return Err(LfError::PermissionDenied(
                "secure input blocked paste".into(),
            ));
        }
        prepare_keyboard();
        focus_target(request.target_pid, request.target_app);
        let delay = request.insert_delay_ms.max(40);
        let extra = if is_editor_or_terminal(request.target_app) {
            delay.saturating_add(40)
        } else {
            delay
        };
        std::thread::sleep(Duration::from_millis(extra));

        let previous = if request.restore_clipboard {
            Some(snapshot_pasteboard())
        } else {
            None
        };
        if let Some(prev) = &previous {
            let _ = crate::injection::persist_clipboard_snapshot(&prev.items);
            if let Some(plain) = prev.plain_text() {
                let _ = crate::injection::persist_clipboard_backup(plain);
            }
        }

        write_pasteboard_string(request.text)?;
        // Give other apps time to observe the new changeCount before Cmd+V.
        std::thread::sleep(Duration::from_millis(80));
        let paste_result = post_paste(request.target_pid);
        release_stuck_modifiers();

        if let Some(prev) = previous {
            if paste_result.is_ok() {
                std::thread::spawn(move || {
                    // Installed .app: wait long enough that Cmd+V is consumed
                    // before we put the previous clipboard back.
                    std::thread::sleep(Duration::from_millis(800));
                    restore_pasteboard(&prev);
                    crate::injection::clear_clipboard_backups();
                });
            }
            // Failed paste: leave the transcript on the pasteboard so a
            // manual Cmd+V still works after enabling Accessibility.
        }
        paste_result
    }

    fn prepare_keyboard(&self) {
        prepare_keyboard();
    }

    fn frontmost_target(&self) -> (Option<i32>, Option<String>) {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(frontmost_target_blocking());
        });
        rx.recv_timeout(Duration::from_millis(250))
            .unwrap_or((None, None))
    }

    fn activate_pid(&self, pid: u32) -> bool {
        activate_pid(pid as i32)
    }

    fn talk_combo_held(&self, hotkey: &str) -> bool {
        let t = hotkey.to_ascii_lowercase();
        let mut required = false;
        if t.contains("control") || t.contains("ctrl") {
            required = true;
            if !control_down() {
                return false;
            }
        }
        if t.contains("shift") {
            required = true;
            if !shift_down() {
                return false;
            }
        }
        if t.contains("command") || t.contains("cmd") || t.contains("super") {
            required = true;
            if !command_down() {
                return false;
            }
        }
        if t.contains("option") || t.contains("alt") {
            required = true;
            if !option_down() {
                return false;
            }
        }
        if t.contains("space") {
            return required && key_down(VK_SPACE);
        }
        required
    }

    fn talk_modifiers_held(&self, hotkey: &str) -> bool {
        let t = hotkey.to_ascii_lowercase();
        let mut any = false;
        if t.contains("control") || t.contains("ctrl") {
            any = true;
            if !control_down() {
                return false;
            }
        }
        if t.contains("shift") {
            any = true;
            if !shift_down() {
                return false;
            }
        }
        if t.contains("command") || t.contains("cmd") || t.contains("super") {
            any = true;
            if !command_down() {
                return false;
            }
        }
        if t.contains("option") || t.contains("alt") {
            any = true;
            if !option_down() {
                return false;
            }
        }
        any
    }

    fn space_key_down(&self) -> bool {
        key_down(VK_SPACE)
    }

    fn accessibility_trusted(&self) -> bool {
        process_is_trusted()
    }

    fn open_privacy_pane(&self, kind: &str) -> LfResult<()> {
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
            if Command::new("open")
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

    fn set_autostart(&self, enabled: bool) -> LfResult<()> {
        let plist = launch_agent_path()?;
        let uid = unsafe { getuid() };
        let domain = format!("gui/{uid}/{LAUNCH_AGENT_LABEL}");
        if enabled {
            let exe = std::env::current_exe()?;
            let body = format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>{LAUNCH_AGENT_LABEL}</string>
  <key>ProgramArguments</key>
  <array><string>{}</string></array>
  <key>RunAtLoad</key><true/>
</dict>
</plist>
"#,
                exe.display()
            );
            if let Some(parent) = plist.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&plist, body)?;
            launchctl(&["bootout", &domain]);
            let status = Command::new("launchctl")
                .args([
                    "bootstrap",
                    &format!("gui/{uid}"),
                    plist.to_str().unwrap_or(""),
                ])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .map_err(|e| LfError::Other(e.to_string()))?;
            if !status.success() {
                launchctl(&["load", "-w", plist.to_str().unwrap_or("")]);
            }
        } else if plist.exists() {
            launchctl(&["bootout", &domain]);
            launchctl(&["unload", "-w", plist.to_str().unwrap_or("")]);
            let _ = fs::remove_file(&plist);
        }
        Ok(())
    }

    fn screen_is_locked(&self) -> bool {
        unsafe { lf_screen_is_locked() != 0 }
    }

    fn play_cue(&self, cue: Cue, volume: f32) {
        let name = match cue {
            Cue::Start => "Tink",
            Cue::End => "Pop",
        };
        let path = format!("/System/Library/Sounds/{name}.aiff");
        let vol = crate::config::clamp_cue_volume(volume);
        let _ = Command::new("afplay")
            .arg("-v")
            .arg(format!("{vol:.2}"))
            .arg(&path)
            .spawn();
    }

    fn report_already_running(&self, message: &str) {
        let safe = message.replace('"', "'");
        let _ = Command::new("osascript")
            .args(["-e", &format!("display dialog \"{safe}\" buttons {{\"OK\"}} default button 1 with title \"LocalFlow\"")])
            .status();
    }
}

fn launch_agent_path() -> LfResult<PathBuf> {
    let home = std::env::var("HOME").map_err(|e| LfError::Other(e.to_string()))?;
    Ok(PathBuf::from(home)
        .join("Library/LaunchAgents")
        .join(format!("{LAUNCH_AGENT_LABEL}.plist")))
}

fn launchctl(args: &[&str]) {
    let _ = Command::new("launchctl")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

fn pbcopy(text: &str) -> LfResult<()> {
    let mut child = Command::new("pbcopy")
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| LfError::InjectionFailed(e.to_string()))?;
    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(text.as_bytes())
            .map_err(|e| LfError::InjectionFailed(e.to_string()))?;
    }
    let status = child
        .wait()
        .map_err(|e| LfError::InjectionFailed(e.to_string()))?;
    if status.success() {
        Ok(())
    } else {
        Err(LfError::InjectionFailed("pbcopy failed".into()))
    }
}

fn frontmost_target_blocking() -> (Option<i32>, Option<String>) {
    on_main(frontmost_target_on_main)
}

fn frontmost_target_on_main() -> (Option<i32>, Option<String>) {
    use objc2_app_kit::NSWorkspace;
    let Some(app) = NSWorkspace::sharedWorkspace().frontmostApplication() else {
        return (None, None);
    };
    let pid = app.processIdentifier();
    let name = app
        .localizedName()
        .map(|s| s.to_string())
        .filter(|n| !n.is_empty());
    if pid <= 0 {
        (None, name)
    } else {
        (Some(pid), name)
    }
}

fn process_is_trusted() -> bool {
    on_main(|| unsafe { AXIsProcessTrusted() })
}

fn prepare_keyboard() {
    wait_for_modifiers_up(Duration::from_millis(250));
    release_stuck_modifiers();
}

fn key_down(vk: u16) -> bool {
    unsafe {
        CGEventSourceKeyState(COMBINED_SESSION_STATE, vk)
            || CGEventSourceKeyState(HID_SYSTEM_STATE, vk)
    }
}

fn control_down() -> bool {
    key_down(VK_CONTROL) || key_down(VK_RIGHT_CONTROL)
}

fn shift_down() -> bool {
    key_down(VK_SHIFT) || key_down(VK_RIGHT_SHIFT)
}

fn command_down() -> bool {
    key_down(VK_COMMAND) || key_down(VK_RIGHT_COMMAND)
}

fn option_down() -> bool {
    key_down(VK_OPTION) || key_down(VK_RIGHT_OPTION)
}

fn on_main<T: Send>(f: impl FnOnce() -> T + Send) -> T {
    if cfg!(test) {
        return f();
    }
    dispatch2::run_on_main(|_| f())
}

pub(crate) fn secure_event_input_enabled() -> bool {
    #[link(name = "Carbon", kind = "framework")]
    extern "C" {
        fn IsSecureEventInputEnabled() -> u8;
    }
    unsafe { IsSecureEventInputEnabled() != 0 }
}

struct PasteboardSnapshot {
    items: Vec<ClipboardItem>,
}

impl PasteboardSnapshot {
    fn plain_text(&self) -> Option<&str> {
        self.items.iter().find_map(|(ty, bytes)| {
            if ty == "public.utf8-plain-text" || ty == "NSStringPboardType" {
                std::str::from_utf8(bytes).ok()
            } else {
                None
            }
        })
    }
}

fn snapshot_pasteboard() -> PasteboardSnapshot {
    on_main(snapshot_pasteboard_on_main)
}

fn snapshot_pasteboard_on_main() -> PasteboardSnapshot {
    use objc2_app_kit::NSPasteboard;
    let pb = NSPasteboard::generalPasteboard();
    let mut items = Vec::new();
    let mut total = 0usize;
    if let Some(types) = pb.types() {
        for ty in types.iter() {
            let name = ty.to_string();
            let keep = name == "public.utf8-plain-text"
                || name == "NSStringPboardType"
                || name == "public.utf16-plain-text"
                || name.contains("plain-text");
            if !keep {
                continue;
            }
            let Some(data) = pb.dataForType(&ty) else {
                continue;
            };
            let bytes = unsafe { data.as_bytes_unchecked() }.to_vec();
            total = total.saturating_add(bytes.len());
            if total > 16 * 1024 * 1024 {
                break;
            }
            items.push((ty.to_string(), bytes));
        }
    }
    PasteboardSnapshot { items }
}

fn restore_pasteboard(snapshot: &PasteboardSnapshot) {
    let snapshot = PasteboardSnapshot {
        items: snapshot.items.clone(),
    };
    on_main(move || restore_pasteboard_on_main(&snapshot));
}

fn restore_pasteboard_on_main(snapshot: &PasteboardSnapshot) {
    use objc2_app_kit::NSPasteboard;
    use objc2_foundation::{NSData, NSString};
    let pb = NSPasteboard::generalPasteboard();
    pb.clearContents();
    for (ty, bytes) in &snapshot.items {
        let ns_ty = NSString::from_str(ty);
        let data = NSData::with_bytes(bytes);
        let _ = pb.setData_forType(Some(&data), &ns_ty);
    }
}

fn write_pasteboard_string(text: &str) -> LfResult<()> {
    let text = text.to_string();
    on_main(move || write_pasteboard_string_on_main(&text))
}

fn write_pasteboard_string_on_main(text: &str) -> LfResult<()> {
    use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString};
    use objc2_foundation::NSString;
    let pb = NSPasteboard::generalPasteboard();
    pb.clearContents();
    let ns = NSString::from_str(text);
    let ok = pb.setString_forType(&ns, unsafe { NSPasteboardTypeString });
    if !ok {
        return pbcopy(text);
    }
    let got = pb
        .stringForType(unsafe { NSPasteboardTypeString })
        .map(|s| s.to_string())
        .unwrap_or_default();
    if got == text {
        Ok(())
    } else {
        pbcopy(text)
    }
}

fn is_editor_or_terminal(app: Option<&str>) -> bool {
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
        || n.contains("code")
        || n.contains("cursor")
        || n.contains("zed")
        || n.contains("xcode")
        || n.contains("sublime")
        || n.contains("vim")
        || n.contains("nvim")
        || n.contains("helix")
}

fn focus_target(pid: Option<i32>, app: Option<&str>) {
    // Packaged LocalFlow is a regular app with a window. On macOS 14+,
    // activateWithOptions(empty()) reports success without taking focus
    // away from us, so Cmd+V lands in LocalFlow instead of the field.
    if let Some(pid) = pid {
        let _ = activate_pid(pid);
    }
    if let Some(name) = app.map(str::trim).filter(|n| !n.is_empty()) {
        let _ = activate_named_app(name);
    }
    std::thread::sleep(Duration::from_millis(80));
}

fn activate_pid(pid: i32) -> bool {
    on_main(move || {
        use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication};
        let Some(target) =
            NSRunningApplication::runningApplicationWithProcessIdentifier(pid as libc::pid_t)
        else {
            return false;
        };
        let current = NSRunningApplication::currentApplication();
        // Bit 1 is NSApplicationActivateIgnoringOtherApps. The named constant
        // is deprecated on macOS 14 but still required to take focus from a
        // packaged LocalFlow window; empty() reports success and does nothing.
        let steal = NSApplicationActivationOptions::ActivateAllWindows
            | NSApplicationActivationOptions(1 << 1);
        let _ = target.activateFromApplication_options(&current, steal);
        target.activateWithOptions(steal)
    })
}

fn activate_named_app(name: &str) -> bool {
    let safe: String = name.chars().filter(|c| *c != '"' && *c != '\\').collect();
    if safe.is_empty() {
        return false;
    }
    let script = format!("tell application \"{safe}\" to activate");
    Command::new("osascript")
        .args(["-e", &script])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn modifier_down() -> bool {
    unsafe {
        CGEventSourceKeyState(COMBINED_SESSION_STATE, VK_CONTROL)
            || CGEventSourceKeyState(COMBINED_SESSION_STATE, VK_RIGHT_CONTROL)
            || CGEventSourceKeyState(COMBINED_SESSION_STATE, VK_SHIFT)
            || CGEventSourceKeyState(COMBINED_SESSION_STATE, VK_RIGHT_SHIFT)
            || CGEventSourceKeyState(COMBINED_SESSION_STATE, VK_COMMAND)
            || CGEventSourceKeyState(COMBINED_SESSION_STATE, VK_RIGHT_COMMAND)
            || CGEventSourceKeyState(COMBINED_SESSION_STATE, VK_OPTION)
            || CGEventSourceKeyState(COMBINED_SESSION_STATE, VK_RIGHT_OPTION)
    }
}

fn wait_for_modifiers_up(timeout: Duration) {
    let start = Instant::now();
    while modifier_down() && start.elapsed() < timeout {
        std::thread::sleep(Duration::from_millis(16));
    }
    if modifier_down() {
        release_stuck_modifiers();
        std::thread::sleep(Duration::from_millis(30));
    }
}

fn post_paste(target_pid: Option<i32>) -> LfResult<()> {
    // CGEventPost returns void and is dropped when the process is not in
    // Accessibility. Do not open System Settings or poke System Events
    // here: both open Universal Access on every insert even when the
    // LocalFlow toggle is already on (TCC attributes osascript, not us).
    let trusted = process_is_trusted();
    let posted = post_command_v(target_pid);
    std::thread::sleep(Duration::from_millis(40));
    if posted && trusted {
        return Ok(());
    }
    if !trusted {
        return Err(LfError::PermissionDenied(
            "Accessibility permission required for insertion".into(),
        ));
    }
    Err(LfError::InjectionFailed("could not post Command+V".into()))
}

fn post_command_v(target_pid: Option<i32>) -> bool {
    unsafe {
        let source = CGEventSourceCreate(HID_SYSTEM_STATE);
        if source.is_null() {
            return false;
        }
        let ok = post_key(source, VK_COMMAND, true, COMMAND_FLAG, target_pid)
            && post_key(source, VK_ANSI_V, true, COMMAND_FLAG, target_pid)
            && post_key(source, VK_ANSI_V, false, COMMAND_FLAG, target_pid)
            && post_key(source, VK_COMMAND, false, 0, target_pid);
        CFRelease(source);
        ok
    }
}

fn post_key(source: *mut c_void, vk: u16, down: bool, flags: u64, target_pid: Option<i32>) -> bool {
    unsafe {
        let event = CGEventCreateKeyboardEvent(source, vk, down);
        if event.is_null() {
            return false;
        }
        CGEventSetFlags(event, flags);
        CGEventPost(HID_EVENT_TAP, event);
        CGEventPost(SESSION_EVENT_TAP, event);
        if let Some(pid) = target_pid {
            CGEventPostToPid(pid, event);
        }
        CFRelease(event);
        true
    }
}

fn release_stuck_modifiers() {
    unsafe {
        let source = CGEventSourceCreate(HID_SYSTEM_STATE);
        for key in [
            VK_CONTROL,
            VK_RIGHT_CONTROL,
            VK_SHIFT,
            VK_RIGHT_SHIFT,
            VK_OPTION,
            VK_RIGHT_OPTION,
            VK_COMMAND,
            VK_RIGHT_COMMAND,
        ] {
            let up = CGEventCreateKeyboardEvent(source, key, false);
            if !up.is_null() {
                CGEventSetFlags(up, 0);
                CGEventPost(SESSION_EVENT_TAP, up);
                CFRelease(up);
            }
        }
        if !source.is_null() {
            CFRelease(source);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paste_posts_command_v_and_never_select_all() {
        let prod = include_str!("macos.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        assert!(prod.contains("VK_ANSI_V"), "paste must send Command+V");
        assert!(
            prod.contains("VK_COMMAND"),
            "Command must be posted, not only set as a flag on V"
        );
        assert!(
            prod.contains("NSApplicationActivationOptions(1 << 1)"),
            "packaged LocalFlow is frontmost; empty() activation does not steal focus on macOS 14+"
        );
        assert!(
            prod.contains("NSWorkspace"),
            "read the frontmost app via AppKit; osascript System Events opens Universal Access"
        );
        assert!(
            !prod.contains("tell application \"System Events\""),
            "osascript System Events opens Universal Access on every use even when LocalFlow is already trusted"
        );
        assert!(
            !prod.contains("prompt_accessibility"),
            "do not open System Settings from the paste path"
        );
        assert!(
            prod.contains("CGEventPostToPid"),
            "post Cmd+V to the remembered pid, not only the session tap"
        );
        assert!(
            prod.contains("AXIsProcessTrusted"),
            "do not report a successful paste when Accessibility dropped the events"
        );
        assert!(
            !prod.contains("VK_ANSI_A"),
            "Select-All would wipe the field and break mid-text insert"
        );
        assert!(
            prod.contains("wait_for_modifiers_up(Duration::from_millis(250))"),
            "do not wait a full second for leftover modifiers"
        );
    }

    #[test]
    fn pasteboard_main_hop_runs_when_already_on_main() {
        assert_eq!(on_main(|| 7), 7);
    }

    #[test]
    fn secure_event_input_query_does_not_panic() {
        let _ = secure_event_input_enabled();
    }

    #[test]
    fn data_root_lands_in_application_support() {
        let root = MacOs.data_root();
        assert!(
            root.ends_with("Library/Application Support/LocalFlow"),
            "{}",
            root.display()
        );
    }
}
