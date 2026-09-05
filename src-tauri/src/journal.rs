//! Size-rotated local journal. Secrets are redacted before write.

use crate::paths::DataPaths;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::sync::Mutex;
use std::time::SystemTime;

const DEFAULT_MAX: u64 = 2 * 1024 * 1024;
static MAX_BYTES: Mutex<u64> = Mutex::new(DEFAULT_MAX);

pub fn set_max_bytes(bytes: u64) {
    if let Ok(mut slot) = MAX_BYTES.lock() {
        *slot = bytes.max(64 * 1024);
    }
}

pub fn log(event: &str, detail: &str) {
    let line = format!("{} {} {}\n", now_rfc3339(), redact(event), redact(detail));
    let paths = DataPaths::detect();
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
    }
}
