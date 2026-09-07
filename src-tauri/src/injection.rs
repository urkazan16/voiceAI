use crate::error::{LfError, LfResult};
use crate::platform::{self, InsertRequest};
use std::path::PathBuf;
use std::sync::Mutex;

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

pub(crate) fn persist_clipboard_snapshot(items: &[platform::ClipboardItem]) -> std::io::Result<()> {
    persist_clipboard_snapshot_at(&clipboard_snapshot_path(), items)
}

fn persist_clipboard_snapshot_at(
    path: &std::path::Path,
    items: &[platform::ClipboardItem],
) -> std::io::Result<()> {
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

fn take_clipboard_snapshot() -> Option<Vec<platform::ClipboardItem>> {
    take_clipboard_snapshot_at(&clipboard_snapshot_path())
}

fn take_clipboard_snapshot_at(path: &std::path::Path) -> Option<Vec<platform::ClipboardItem>> {
    let bytes = std::fs::read(path).ok()?;
    let _ = std::fs::remove_file(path);
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

pub(crate) fn clear_clipboard_backups() {
    let _ = std::fs::remove_file(clipboard_backup_path());
    let _ = std::fs::remove_file(clipboard_snapshot_path());
}

pub(crate) fn persist_clipboard_backup(text: &str) -> std::io::Result<()> {
    persist_clipboard_backup_at(&clipboard_backup_path(), text)
}

fn persist_clipboard_backup_at(path: &std::path::Path, text: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, text)
}

pub fn take_clipboard_backup() -> Option<String> {
    take_clipboard_backup_at(&clipboard_backup_path())
}

fn take_clipboard_backup_at(path: &std::path::Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let _ = std::fs::remove_file(path);
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
    if let Some(items) = take_clipboard_snapshot() {
        if platform::current().restore_clipboard_items(&items) {
            let _ = std::fs::remove_file(clipboard_backup_path());
            return;
        }
    }
    let Some(text) = take_clipboard_backup() else {
        return;
    };
    let _ = platform::current().set_clipboard_text(&text);
}

pub fn set_clipboard_text(text: &str) -> LfResult<()> {
    platform::current().set_clipboard_text(text)
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
        platform::current().insert_text(&InsertRequest {
            text,
            restore_clipboard,
            target_pid: self.target_pid,
            target_app: self.target_app.as_deref(),
            insert_delay_ms: self.insert_delay_ms,
        })
    }
}

pub fn frontmost_unix_id() -> Option<i32> {
    frontmost_target().0
}

pub fn frontmost_app_name() -> Option<String> {
    frontmost_target().1
}

pub fn frontmost_target() -> (Option<i32>, Option<String>) {
    platform::current().frontmost_target()
}

pub fn space_key_down() -> bool {
    platform::current().space_key_down()
}

/// True while the configured talk chord is physically held (including Space).
pub fn talk_combo_held(hotkey: &str) -> bool {
    platform::current().talk_combo_held(hotkey)
}

/// True while Control/Shift/Command/Option from the talk hotkey are down (Space ignored).
pub fn talk_modifiers_held(hotkey: &str) -> bool {
    platform::current().talk_modifiers_held(hotkey)
}

pub fn prepare_keyboard_for_insert() {
    platform::current().prepare_keyboard();
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
    fn clipboard_backup_roundtrip_on_disk() {
        // Addresses an explicit path rather than the process-global one: every
        // `AppEngine::open` rebinds that global, so a parallel engine test used
        // to steal this backup and make the assertion below fail at random.
        let dir = tempfile::tempdir().unwrap();
        let backup = dir.path().join("clipboard-restore.txt");
        let snapshot = backup.with_extension("json");
        persist_clipboard_backup_at(&backup, "keep me").unwrap();
        assert_eq!(
            take_clipboard_backup_at(&backup).as_deref(),
            Some("keep me")
        );
        assert!(take_clipboard_backup_at(&backup).is_none());
        persist_clipboard_snapshot_at(
            &snapshot,
            &[("public.utf8-plain-text".into(), b"rtf-or-img".to_vec())],
        )
        .unwrap();
        let snap = take_clipboard_snapshot_at(&snapshot).unwrap();
        assert_eq!(snap[0].1, b"rtf-or-img");
        assert!(take_clipboard_snapshot_at(&snapshot).is_none());
    }

    #[test]
    fn snapshot_path_follows_the_configured_backup_path() {
        let dir = tempfile::tempdir().unwrap();
        let backup = dir.path().join("clipboard-restore.txt");
        set_clipboard_backup_path(backup.clone());
        assert_eq!(clipboard_snapshot_path(), backup.with_extension("json"));
    }
}
