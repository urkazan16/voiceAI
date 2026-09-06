use crate::error::{LfError, LfResult};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

static CLIPBOARD_BACKUP: Mutex<Option<PathBuf>> = Mutex::new(None);

pub fn set_clipboard_backup_path(path: PathBuf) {
    if let Ok(mut slot) = CLIPBOARD_BACKUP.lock() {
        *slot = Some(path);
    }
}

pub fn clipboard_backup_path() -> PathBuf {
    CLIPBOARD_BACKUP
        .lock()
        .ok()
        .and_then(|g| g.clone())
        .unwrap_or_else(|| crate::paths::DataPaths::detect().clipboard_backup())
}

pub fn clipboard_snapshot_path() -> PathBuf {
    clipboard_backup_path().with_extension("json")
}

#[derive(serde::Serialize, serde::Deserialize)]
struct ClipboardDiskSnapshot {
    schema: u32,
    items: Vec<(String, String)>,
}

fn persist_clipboard_snapshot(items: &[(String, Vec<u8>)]) -> std::io::Result<()> {
    let path = clipboard_snapshot_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let disk = ClipboardDiskSnapshot {
        schema: 1,
        items: items
            .iter()
            .map(|(ty, bytes)| (ty.clone(), hex::encode(bytes)))
            .collect(),
    };
    std::fs::write(path, serde_json::to_vec(&disk)?)
}

fn take_clipboard_snapshot() -> Option<Vec<(String, Vec<u8>)>> {
    let path = clipboard_snapshot_path();
    let bytes = std::fs::read(&path).ok()?;
    let _ = std::fs::remove_file(&path);
    let disk: ClipboardDiskSnapshot = serde_json::from_slice(&bytes).ok()?;
    if disk.schema != 1 {
        return None;
    }
    let mut items = Vec::new();
    for (ty, hex_data) in disk.items {
        let Ok(data) = hex::decode(hex_data) else {
            continue;
        };
        items.push((ty, data));
    }
    Some(items)
}

fn clear_clipboard_backups() {
    let _ = std::fs::remove_file(clipboard_backup_path());
    let _ = std::fs::remove_file(clipboard_snapshot_path());
}

pub fn persist_clipboard_backup(text: &str) -> std::io::Result<()> {
    let path = clipboard_backup_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, text)
}

pub fn take_clipboard_backup() -> Option<String> {
    let path = clipboard_backup_path();
    let text = std::fs::read_to_string(&path).ok()?;
    let _ = std::fs::remove_file(&path);
    Some(text)
}

/// Cmd+V contract used by macOS insertion: insert at the caret, or replace an
/// existing selection. LocalFlow never Select-All before paste.
pub fn apply_native_paste(haystack: &str, sel_start: usize, sel_end: usize, clip: &str) -> String {
    let chars: Vec<char> = haystack.chars().collect();
    let start = sel_start.min(chars.len());
    let end = sel_end.min(chars.len()).max(start);
    let mut out: String = chars[..start].iter().collect();
    out.push_str(clip);
    out.extend(chars[end..].iter().copied());
    out
}

pub fn restore_orphaned_clipboard() {
    #[cfg(target_os = "macos")]
    {
        if macos::restore_orphaned_snapshot() {
            let _ = std::fs::remove_file(clipboard_backup_path());
            return;
        }
    }
    let Some(text) = take_clipboard_backup() else {
        return;
    };
    let _ = write_pasteboard(&text);
}

fn write_pasteboard(text: &str) -> LfResult<()> {
    #[cfg(target_os = "macos")]
    {
        let mut child = std::process::Command::new("pbcopy")
            .stdin(std::process::Stdio::piped())
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
    #[cfg(not(target_os = "macos"))]
    {
        let _ = text;
        Ok(())
    }
}

pub trait TextInjector: Send + Sync {
    fn insert_text(&self, text: &str, restore_clipboard: bool) -> LfResult<()>;
}

pub struct MemoryInjector {
    pub last: std::sync::Mutex<Option<String>>,
}

impl Default for MemoryInjector {
    fn default() -> Self {
        Self {
            last: std::sync::Mutex::new(None),
        }
    }
}

impl TextInjector for MemoryInjector {
    fn insert_text(&self, text: &str, _restore_clipboard: bool) -> LfResult<()> {
        *self
            .last
            .lock()
            .map_err(|e| LfError::Other(e.to_string()))? = Some(text.to_string());
        Ok(())
    }
}

#[derive(Default)]
pub struct ClipboardInjector {
    pub target_pid: Option<i32>,
    pub target_app: Option<String>,
    pub insert_delay_ms: u64,
}

impl TextInjector for ClipboardInjector {
    fn insert_text(&self, text: &str, restore_clipboard: bool) -> LfResult<()> {
        insert_via_clipboard(
            text,
            restore_clipboard,
            self.target_pid,
            self.target_app.as_deref(),
            self.insert_delay_ms,
        )
    }
}

pub fn frontmost_unix_id() -> Option<i32> {
    frontmost_target().0
}

pub fn frontmost_app_name() -> Option<String> {
    frontmost_target().1
}

pub fn frontmost_target() -> (Option<i32>, Option<String>) {
    #[cfg(target_os = "macos")]
    {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(frontmost_target_blocking());
        });
        rx.recv_timeout(Duration::from_millis(250))
            .unwrap_or((None, None))
    }
    #[cfg(not(target_os = "macos"))]
    {
        (None, None)
    }
}

#[cfg(target_os = "macos")]
fn frontmost_target_blocking() -> (Option<i32>, Option<String>) {
    let output = std::process::Command::new("osascript")
        .args([
            "-e",
            "tell application \"System Events\" to tell first application process whose frontmost is true to get name & tab & unix id",
        ])
        .output();
    let Ok(output) = output else {
        return (None, None);
    };
    if !output.status.success() {
        return (None, None);
    }
    let text = String::from_utf8(output.stdout).unwrap_or_default();
    let text = text.trim();
    if text.is_empty() {
        return (None, None);
    }
    match text.rsplit_once('\t') {
        Some((name, id)) => (
            id.trim().parse().ok(),
            Some(name.trim())
                .filter(|n| !n.is_empty())
                .map(str::to_string),
        ),
        None => (text.parse().ok(), None),
    }
}

fn insert_via_clipboard(
    text: &str,
    restore_clipboard: bool,
    target_pid: Option<i32>,
    target_app: Option<&str>,
    insert_delay_ms: u64,
) -> LfResult<()> {
    #[cfg(target_os = "macos")]
    {
        macos::insert_text(
            text,
            restore_clipboard,
            target_pid,
            target_app,
            insert_delay_ms,
        )
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (
            text,
            restore_clipboard,
            target_pid,
            target_app,
            insert_delay_ms,
        );
        Err(LfError::InjectionFailed(
            "clipboard injection is implemented for macOS in this MVP".into(),
        ))
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use std::ffi::c_void;
    use std::time::{Duration, Instant};

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
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFRelease(cf: *const c_void);
    }

    pub fn prepare_keyboard() {
        wait_for_modifiers_up(Duration::from_millis(1000));
        release_stuck_modifiers();
    }

    pub fn insert_text(
        text: &str,
        restore_clipboard: bool,
        target_pid: Option<i32>,
        target_app: Option<&str>,
        insert_delay_ms: u64,
    ) -> LfResult<()> {
        if secure_event_input_enabled() {
            return Err(LfError::PermissionDenied(
                "secure input blocked paste".into(),
            ));
        }
        prepare_keyboard();
        focus_pid(target_pid);
        let delay = insert_delay_ms.max(40);
        let extra = if is_editor_or_terminal(target_app) {
            delay.saturating_add(120)
        } else {
            delay
        };
        std::thread::sleep(Duration::from_millis(extra));

        let previous = if restore_clipboard {
            Some(snapshot_pasteboard())
        } else {
            None
        };
        if let Some(prev) = &previous {
            let _ = super::persist_clipboard_snapshot(&prev.items);
            if let Some(plain) = prev.plain_text() {
                let _ = super::persist_clipboard_backup(plain);
            }
        }

        write_pasteboard_string(text)?;
        let paste_result = post_paste();
        release_stuck_modifiers();

        if let Some(prev) = previous {
            std::thread::sleep(Duration::from_millis(100));
            restore_pasteboard(&prev);
            super::clear_clipboard_backups();
        }
        paste_result
    }

    pub(crate) fn secure_event_input_enabled() -> bool {
        #[link(name = "Carbon", kind = "framework")]
        extern "C" {
            fn IsSecureEventInputEnabled() -> u8;
        }
        unsafe { IsSecureEventInputEnabled() != 0 }
    }

    struct PasteboardSnapshot {
        items: Vec<(String, Vec<u8>)>,
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
        use objc2_app_kit::NSPasteboard;
        let pb = NSPasteboard::generalPasteboard();
        let mut items = Vec::new();
        let mut total = 0usize;
        if let Some(types) = pb.types() {
            for ty in types.iter() {
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

    pub(super) fn restore_orphaned_snapshot() -> bool {
        let Some(items) = super::take_clipboard_snapshot() else {
            return false;
        };
        if items.is_empty() {
            return false;
        }
        restore_pasteboard(&PasteboardSnapshot { items });
        true
    }

    fn write_pasteboard_string(text: &str) -> LfResult<()> {
        use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString};
        use objc2_foundation::NSString;
        let pb = NSPasteboard::generalPasteboard();
        pb.clearContents();
        if pb.setString_forType(&NSString::from_str(text), unsafe { NSPasteboardTypeString }) {
            Ok(())
        } else {
            super::write_pasteboard(text)
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

    fn focus_pid(pid: Option<i32>) {
        let Some(pid) = pid else {
            return;
        };
        let script = format!(
            "tell application \"System Events\" to set frontmost of first application process whose unix id is {pid} to true"
        );
        let _ = std::process::Command::new("osascript")
            .args(["-e", &script])
            .status();
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
                || CGEventSourceKeyState(COMBINED_SESSION_STATE, VK_SPACE)
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

    fn post_paste() -> LfResult<()> {
        unsafe {
            let source = CGEventSourceCreate(HID_SYSTEM_STATE);
            let down = CGEventCreateKeyboardEvent(source, VK_ANSI_V, true);
            let up = CGEventCreateKeyboardEvent(source, VK_ANSI_V, false);
            if down.is_null() || up.is_null() {
                if !down.is_null() {
                    CFRelease(down);
                }
                if !up.is_null() {
                    CFRelease(up);
                }
                if !source.is_null() {
                    CFRelease(source);
                }
                return fallback_osascript_paste();
            }
            CGEventSetFlags(down, COMMAND_FLAG);
            CGEventSetFlags(up, COMMAND_FLAG);
            CGEventPost(SESSION_EVENT_TAP, down);
            CGEventPost(SESSION_EVENT_TAP, up);
            CFRelease(down);
            CFRelease(up);
            if !source.is_null() {
                CFRelease(source);
            }
        }
        Ok(())
    }

    fn fallback_osascript_paste() -> LfResult<()> {
        let status = std::process::Command::new("osascript")
            .args([
                "-e",
                "tell application \"System Events\" to key code 9 using command down",
            ])
            .status()
            .map_err(|e| LfError::InjectionFailed(e.to_string()))?;
        if status.success() {
            Ok(())
        } else {
            Err(LfError::PermissionDenied(
                "Accessibility permission required for insertion".into(),
            ))
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
}

pub fn prepare_keyboard_for_insert() {
    #[cfg(target_os = "macos")]
    {
        macos::prepare_keyboard();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_injector_stores_text() {
        let inj = MemoryInjector::default();
        inj.insert_text("hello", true).unwrap();
        assert_eq!(inj.last.lock().unwrap().clone(), Some("hello".into()));
    }

    #[test]
    fn k17_paste_inserts_in_the_middle_of_existing_text() {
        assert_eq!(
            apply_native_paste("LEFT RIGHT", 5, 5, "MID "),
            "LEFT MID RIGHT"
        );
        assert_eq!(apply_native_paste("абв", 1, 1, "—"), "а—бв");
    }

    #[test]
    fn k18_paste_replaces_the_selection() {
        assert_eq!(
            apply_native_paste("hello world", 6, 11, "there"),
            "hello there"
        );
        assert_eq!(
            apply_native_paste("раз два три", 4, 7, "ДВА"),
            "раз ДВА три"
        );
    }

    #[test]
    fn paste_posts_command_v_and_never_select_all() {
        let prod = include_str!("injection.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        assert!(prod.contains("VK_ANSI_V"), "paste must send Command+V");
        assert!(
            !prod.contains("VK_ANSI_A"),
            "Select-All would wipe the field and break mid-text insert"
        );
    }

    #[test]
    fn clipboard_backup_roundtrip_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        set_clipboard_backup_path(dir.path().join("clipboard-restore.txt"));
        persist_clipboard_backup("keep me").unwrap();
        assert_eq!(take_clipboard_backup().as_deref(), Some("keep me"));
        assert!(take_clipboard_backup().is_none());
        persist_clipboard_snapshot(&[("public.utf8-plain-text".into(), b"rtf-or-img".to_vec())])
            .unwrap();
        let snap = take_clipboard_snapshot().unwrap();
        assert_eq!(snap[0].1, b"rtf-or-img");
        assert!(take_clipboard_snapshot().is_none());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn secure_event_input_query_does_not_panic() {
        let _ = macos::secure_event_input_enabled();
    }
}
