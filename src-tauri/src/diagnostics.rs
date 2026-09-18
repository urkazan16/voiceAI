//! Privacy-preserving diagnostics for the development terminal.
//!
//! Dictation text and audio never belong in a terminal log.  This module only
//! reports lifecycle, permission and error metadata so a broken recording can
//! be traced without exposing what the person dictated.

use std::time::{SystemTime, UNIX_EPOCH};

const MAX_DETAIL_CHARS: usize = 480;

/// In `tauri dev` this is enabled automatically. A packaged build can opt in
/// with `LOCALFLOW_TERMINAL_LOG=1` when support diagnostics are needed.
pub fn terminal_enabled() -> bool {
    cfg!(debug_assertions) || std::env::var_os("LOCALFLOW_TERMINAL_LOG").is_some()
}

pub fn event(name: &str, detail: &str) {
    if terminal_enabled() {
        print("INFO", name, detail);
    }
}

/// Errors must remain visible even for a packaged build; ordinary lifecycle
/// events stay opt-in there to avoid noisy terminals for normal use.
pub fn error(name: &str, detail: &str) {
    print("ERROR", name, detail);
}

pub fn startup() {
    if terminal_enabled() {
        print(
            "INFO",
            "diagnostics",
            "terminal diagnostics enabled; dictated text and audio are not logged",
        );
    }
}

fn print(level: &str, name: &str, detail: &str) {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    eprintln!(
        "[LocalFlow][{millis}][{level}][{name}] {}",
        safe_detail(detail)
    );
}

fn safe_detail(detail: &str) -> String {
    let redacted = crate::journal::redact(detail);
    let mut result = String::new();
    for (index, ch) in redacted.chars().enumerate() {
        if index == MAX_DETAIL_CHARS {
            result.push_str("… [truncated]");
            break;
        }
        result.push(ch);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_detail_redacts_and_bounds_secrets() {
        let secret = safe_detail("token=very-secret-value");
        assert!(secret.contains("[redacted]"));
        assert!(!secret.contains("very-secret-value"));

        let long = safe_detail(&"x".repeat(MAX_DETAIL_CHARS + 1));
        assert!(long.ends_with("… [truncated]"));
    }
}
