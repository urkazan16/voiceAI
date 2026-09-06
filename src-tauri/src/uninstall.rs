use crate::error::LfResult;
use crate::paths::DataPaths;
use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UninstallReport {
    pub kept_history: bool,
    pub removed: Vec<String>,
    pub skipped: Vec<String>,
}

pub fn uninstall(keep_history: bool) -> LfResult<UninstallReport> {
    let _ = crate::autostart::apply(false);
    let paths = DataPaths::detect();
    let mut removed = Vec::new();
    let mut skipped = Vec::new();
    let targets = [
        ("audio cache", paths.audio()),
        ("whisper models", paths.models_whisper()),
        ("llm models", paths.models_llm()),
        ("logs", paths.logs()),
        ("config", paths.config_dir()),
    ];
    for (label, path) in targets {
        if path.exists() {
            fs::remove_dir_all(&path)?;
            removed.push(format!("{label}: {}", path.display()));
        } else {
            skipped.push(format!("{label} (missing)"));
        }
    }
    if keep_history {
        skipped.push(format!(
            "history database kept: {}",
            paths.database_file().display()
        ));
    } else if paths.database_dir().exists() {
        fs::remove_dir_all(paths.database_dir())?;
        removed.push(format!(
            "history database: {}",
            paths.database_dir().display()
        ));
    }
    if !keep_history && paths.root.exists() {
        let _ = fs::remove_dir(paths.root); // only if empty
    }
    Ok(UninstallReport {
        kept_history: keep_history,
        removed,
        skipped,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn report_lists_removed_and_can_keep_history() {
        let dir = tempdir().unwrap();
        std::env::set_var("LOCALFLOW_DATA_DIR", dir.path());
        let paths = DataPaths::detect();
        paths.ensure().unwrap();
        std::fs::write(paths.database_file(), "db").unwrap();
        let report = uninstall(true).unwrap();
        assert!(report.kept_history);
        assert!(report.removed.iter().any(|r| r.contains("logs")));
        assert!(paths.database_file().exists());
        std::env::remove_var("LOCALFLOW_DATA_DIR");
    }
}
