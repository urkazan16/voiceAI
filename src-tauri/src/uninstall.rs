use crate::error::LfResult;
use crate::paths::DataPaths;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UninstallReport {
    pub kept_history: bool,
    pub removed: Vec<String>,
    pub skipped: Vec<String>,
}

pub fn uninstall(keep_history: bool) -> LfResult<UninstallReport> {
    uninstall_with_paths(keep_history, DataPaths::detect())
}

fn uninstall_with_paths(keep_history: bool, paths: DataPaths) -> LfResult<UninstallReport> {
    let _ = crate::autostart::apply(false);
    let mut removed = Vec::new();
    let mut skipped = Vec::new();

    remove_path(
        "dictate helper",
        &dictate_macro_path(),
        &mut removed,
        &mut skipped,
    );

    if keep_history {
        for (label, path) in [
            ("audio cache", paths.audio()),
            ("models", paths.models()),
            ("logs", paths.logs()),
            ("config", paths.config_dir()),
        ] {
            remove_path(label, &path, &mut removed, &mut skipped);
        }
        remove_path(
            "instance lock",
            &paths.root.join("localflow.lock"),
            &mut removed,
            &mut skipped,
        );
        remove_path(
            "clipboard backup",
            &paths.clipboard_backup(),
            &mut removed,
            &mut skipped,
        );
        skipped.push(format!(
            "history database kept: {}",
            paths.database_file().display()
        ));
    } else if paths.root.exists() {
        fs::remove_dir_all(&paths.root)?;
        removed.push(format!("data root: {}", paths.root.display()));
    } else {
        skipped.push("data root (missing)".into());
    }

    if let Some(app) = removable_install() {
        let shown = app.display().to_string();
        schedule_remove_after_exit(&app);
        removed.push(format!("app (after quit): {shown}"));
    }

    Ok(UninstallReport {
        kept_history: keep_history,
        removed,
        skipped,
    })
}

fn dictate_macro_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join("Applications/LocalFlow Dictate.command")
}

fn remove_path(label: &str, path: &Path, removed: &mut Vec<String>, skipped: &mut Vec<String>) {
    if !path.exists() {
        skipped.push(format!("{label} (missing)"));
        return;
    }
    let result = if path.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    };
    match result {
        Ok(()) => removed.push(format!("{label}: {}", path.display())),
        Err(err) => skipped.push(format!("{label} ({err})")),
    }
}

fn removable_install() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let raw = exe.to_string_lossy();
    if raw.contains("/target/") || raw.contains("\\target\\") || raw.contains("src-tauri") {
        return None;
    }
    #[cfg(target_os = "macos")]
    {
        exe.ancestors()
            .find(|p| p.extension().is_some_and(|ext| ext == "app"))
            .map(Path::to_path_buf)
            .filter(|p| {
                let s = p.to_string_lossy();
                s.contains("/Applications/") || s.contains("/Applications\\")
            })
    }
    #[cfg(windows)]
    {
        exe.parent().map(Path::to_path_buf)
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = exe;
        None
    }
}

fn schedule_remove_after_exit(path: &Path) {
    #[cfg(windows)]
    {
        let quoted = format!("\"{}\"", path.display().to_string().replace('"', ""));
        let cmdline = format!("timeout /t 2 /nobreak >nul & rmdir /s /q {quoted}");
        let _ = Command::new("cmd")
            .args(["/C", "start", "", "/MIN", "cmd", "/C", &cmdline])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
    }
    #[cfg(not(windows))]
    {
        let quoted = format!("'{}'", path.to_string_lossy().replace('\'', "'\\''"));
        let _ = Command::new("/bin/sh")
            .args(["-c", &format!("sleep 2; rm -rf {quoted}")])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn report_lists_removed_and_can_keep_history() {
        let dir = tempdir().unwrap();
        let paths = DataPaths::from_override(dir.path().to_path_buf());
        paths.ensure().unwrap();
        std::fs::write(paths.database_file(), "db").unwrap();
        std::fs::write(paths.models_whisper().join("ggml-small.bin"), b"model").unwrap();
        let report = uninstall_with_paths(true, paths.clone()).unwrap();
        assert!(report.kept_history);
        assert!(report.removed.iter().any(|r| r.contains("models")));
        assert!(!paths.models().exists());
        assert!(paths.database_file().exists());
    }

    #[test]
    fn full_wipe_deletes_models_and_data_root() {
        let dir = tempdir().unwrap();
        let paths = DataPaths::from_override(dir.path().to_path_buf());
        paths.ensure().unwrap();
        std::fs::write(paths.models_whisper().join("ggml-medium.bin"), b"model").unwrap();
        std::fs::write(paths.database_file(), "db").unwrap();
        let report = uninstall_with_paths(false, paths.clone()).unwrap();
        assert!(!report.kept_history);
        assert!(report.removed.iter().any(|r| r.contains("data root")));
        assert!(!paths.root.exists());
    }

    #[test]
    fn test_binaries_are_not_scheduled_for_deletion() {
        assert!(removable_install().is_none());
    }
}
