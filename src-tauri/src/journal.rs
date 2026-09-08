//! Size-rotated local journal. Secrets are redacted before write.

use crate::paths::DataPaths;
use serde::Serialize;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::Mutex;
use std::time::SystemTime;

pub const CURRENT_NAME: &str = "localflow.log";
pub const ROTATED_NAME: &str = "localflow.log.1";
const DEFAULT_READ_CAP: usize = 256 * 1024;

const DEFAULT_MAX: u64 = 2 * 1024 * 1024;
static MAX_BYTES: Mutex<u64> = Mutex::new(DEFAULT_MAX);
static JOURNAL_ROOT: Mutex<Option<std::path::PathBuf>> = Mutex::new(None);

pub fn set_max_bytes(bytes: u64) {
    if let Ok(mut slot) = MAX_BYTES.lock() {
        *slot = bytes.max(64 * 1024);
    }
}

/// Directs [`log`] at this data root. The GUI binds the opened engine path so
/// tests using a tempdir cannot append to the installed app's journal.
pub fn bind_root(root: impl Into<std::path::PathBuf>) {
    if let Ok(mut slot) = JOURNAL_ROOT.lock() {
        *slot = Some(root.into());
    }
}

fn journal_paths() -> Option<DataPaths> {
    if let Ok(slot) = JOURNAL_ROOT.lock() {
        if let Some(root) = slot.clone() {
            return Some(DataPaths::from_override(root));
        }
    }
    if cfg!(test) {
        return None;
    }
    Some(DataPaths::detect())
}

pub fn log(event: &str, detail: &str) {
    let Some(paths) = journal_paths() else {
        return;
    };
    let line = format!("{} {} {}\n", now_rfc3339(), redact(event), redact(detail));
    let _ = paths.ensure();
    let file = paths.logs().join("localflow.log");
    let max = MAX_BYTES.lock().map(|g| *g).unwrap_or(DEFAULT_MAX);
    if let Ok(meta) = fs::metadata(&file) {
        if meta.len() > max {
            let _ = fs::rename(&file, paths.logs().join("localflow.log.1"));
        }
    }
    if let Ok(mut out) = OpenOptions::new().create(true).append(true).open(&file) {
        let _ = out.write_all(line.as_bytes());
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct JournalView {
    pub path: String,
    pub text: String,
    pub truncated: bool,
    pub bytes: u64,
}

pub fn current_file(paths: &DataPaths) -> std::path::PathBuf {
    paths.logs().join(CURRENT_NAME)
}

/// Newest lines from the current file, then the rotated `.1` if needed to fill
/// the cap. Used by the Logs tab so the UI does not load multi-megabyte files.
pub fn read_recent(paths: &DataPaths, max_bytes: usize) -> JournalView {
    let cap = max_bytes.max(1);
    let current = current_file(paths);
    let rotated = paths.logs().join(ROTATED_NAME);
    let mut total = 0u64;
    if let Ok(meta) = fs::metadata(&rotated) {
        total = total.saturating_add(meta.len());
    }
    if let Ok(meta) = fs::metadata(&current) {
        total = total.saturating_add(meta.len());
    }
    let newer = read_tail(&current, cap);
    let remain = cap.saturating_sub(newer.len());
    let older = if remain > 0 {
        read_tail(&rotated, remain)
    } else {
        String::new()
    };
    let text = if older.is_empty() {
        newer
    } else if newer.is_empty() {
        older
    } else {
        format!("{older}{newer}")
    };
    let truncated = total > text.len() as u64;
    JournalView {
        path: current.display().to_string(),
        text,
        truncated,
        bytes: total,
    }
}

pub fn read_recent_default(paths: &DataPaths) -> JournalView {
    read_recent(paths, DEFAULT_READ_CAP)
}

fn read_tail(path: &Path, max: usize) -> String {
    let Ok(bytes) = fs::read(path) else {
        return String::new();
    };
    if bytes.is_empty() {
        return String::new();
    }
    if bytes.len() <= max {
        return String::from_utf8_lossy(&bytes).into_owned();
    }
    let start = bytes.len() - max;
    let slice = &bytes[start..];
    let slice = slice
        .iter()
        .position(|&b| b == b'\n')
        .map(|i| &slice[i + 1..])
        .unwrap_or(slice);
    String::from_utf8_lossy(slice).into_owned()
}

pub fn redact(input: &str) -> String {
    let mut out = input.to_string();
    for key in [
        "sk-",
        "ghp_",
        "github_pat_",
        "xoxb-",
        "Bearer ",
        "api_key=",
        "apikey=",
        "token=",
        "authorization:",
        "password=",
        "secret=",
        "access_key=",
        "secret_key=",
        "aws_secret",
        "private_key=",
    ] {
        if let Some(idx) = out.to_ascii_lowercase().find(&key.to_ascii_lowercase()) {
            let rest = idx + key.len();
            let end = out[rest..]
                .find(|c: char| c.is_whitespace() || c == '"' || c == '\'')
                .map(|n| rest + n)
                .unwrap_or(out.len());
            out.replace_range(rest..end, "[redacted]");
        }
    }
    out
}

fn now_rfc3339() -> String {
    let secs = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{secs}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_tokens_and_keys() {
        assert!(redact("Authorization: Bearer sk-abc123xyz").contains("[redacted]"));
        assert!(redact("token=secretvalue rest").contains("[redacted]"));
        assert!(!redact("token=secretvalue").contains("secretvalue"));
        assert!(!redact("access_key=AKIAEXAMPLE").contains("AKIAEXAMPLE"));
    }

    #[test]
    fn read_recent_returns_the_newest_lines_and_marks_truncation() {
        let dir = tempfile::tempdir().unwrap();
        let paths = DataPaths::from_override(dir.path().to_path_buf());
        paths.ensure().unwrap();
        let empty = read_recent(&paths, 64);
        assert!(empty.text.is_empty());
        assert!(!empty.truncated);
        assert!(empty.path.ends_with(CURRENT_NAME));

        fs::write(paths.logs().join(ROTATED_NAME), "old 1\nold 2\n").unwrap();
        fs::write(
            current_file(&paths),
            "100 record_start microphone on\n101 insert_failed PERMISSION_DENIED\n",
        )
        .unwrap();
        let view = read_recent(&paths, 64 * 1024);
        assert!(view.text.contains("old 1"));
        assert!(view.text.contains("insert_failed"));
        assert!(!view.truncated);

        let tiny = read_recent(&paths, 40);
        assert!(
            tiny.text.contains("insert_failed"),
            "tail must keep the newest events: {}",
            tiny.text
        );
        assert!(tiny.truncated);
    }

    #[test]
    fn log_writes_to_the_bound_root_not_the_installed_app() {
        let dir = tempfile::tempdir().unwrap();
        bind_root(dir.path());
        log("insert", "clipboard");
        let text = fs::read_to_string(current_file(&DataPaths::from_override(
            dir.path().to_path_buf(),
        )))
        .unwrap_or_default();
        assert!(
            text.contains("insert clipboard"),
            "bound journal missing insert: {text}"
        );
    }
}
