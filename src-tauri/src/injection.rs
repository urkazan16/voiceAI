use crate::error::{LfError, LfResult};

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

pub struct ClipboardInjector;

impl TextInjector for ClipboardInjector {
    fn insert_text(&self, text: &str, restore_clipboard: bool) -> LfResult<()> {
        insert_via_clipboard(text, restore_clipboard)
    }
}

fn insert_via_clipboard(text: &str, restore_clipboard: bool) -> LfResult<()> {
    #[cfg(target_os = "macos")]
    {
        let previous = if restore_clipboard {
            std::process::Command::new("pbpaste")
                .output()
                .ok()
                .and_then(|o| String::from_utf8(o.stdout).ok())
        } else {
            None
        };
        let mut child = std::process::Command::new("pbcopy")
            .stdin(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| LfError::InjectionFailed(e.to_string()))?;
        if let Some(stdin) = child.stdin.as_mut() {
            use std::io::Write;
            stdin
                .write_all(text.as_bytes())
                .map_err(|e| LfError::InjectionFailed(e.to_string()))?;
        }
        let status = child
            .wait()
            .map_err(|e| LfError::InjectionFailed(e.to_string()))?;
        if !status.success() {
            return Err(LfError::InjectionFailed("pbcopy failed".into()));
        }
        let paste = std::process::Command::new("osascript")
            .args([
                "-e",
                "tell application \"System Events\" to keystroke \"v\" using command down",
            ])
            .status()
            .map_err(|e| LfError::InjectionFailed(e.to_string()))?;
        if !paste.success() {
            return Err(LfError::PermissionDenied(
                "Accessibility permission required for insertion".into(),
            ));
        }
        if let Some(prev) = previous {
            let mut child = std::process::Command::new("pbcopy")
                .stdin(std::process::Stdio::piped())
                .spawn()
                .map_err(|e| LfError::InjectionFailed(e.to_string()))?;
            if let Some(stdin) = child.stdin.as_mut() {
                use std::io::Write;
                let _ = stdin.write_all(prev.as_bytes());
            }
            let _ = child.wait();
        }
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (text, restore_clipboard);
        Err(LfError::InjectionFailed(
            "clipboard injection is implemented for macOS in this MVP".into(),
        ))
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
}
