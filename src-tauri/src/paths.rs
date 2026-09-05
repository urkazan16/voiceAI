use std::path::{Path, PathBuf};

const APP_DIR_NAME: &str = "LocalFlow";

#[derive(Debug, Clone)]
pub struct DataPaths {
    pub root: PathBuf,
}

impl DataPaths {
    pub fn from_override(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn detect() -> Self {
        if let Ok(value) = std::env::var("LOCALFLOW_DATA_DIR") {
            return Self {
                root: PathBuf::from(value),
            };
        }
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        Self {
            root: PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join(APP_DIR_NAME),
        }
    }

    pub fn ensure(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(self.models_whisper())?;
        std::fs::create_dir_all(self.models_llm())?;
        std::fs::create_dir_all(self.database_dir())?;
        std::fs::create_dir_all(self.logs())?;
        std::fs::create_dir_all(self.config_dir())?;
        std::fs::create_dir_all(self.audio())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            for dir in [self.audio(), self.logs(), self.root.clone()] {
                let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
            }
        }
        Ok(())
    }

    pub fn models(&self) -> PathBuf {
        self.root.join("models")
    }
    pub fn models_whisper(&self) -> PathBuf {
        self.models().join("whisper")
    }
    pub fn models_llm(&self) -> PathBuf {
        self.models().join("llm")
    }
    pub fn database_dir(&self) -> PathBuf {
        self.root.join("database")
    }
    pub fn database_file(&self) -> PathBuf {
        self.database_dir().join("localflow.sqlite")
    }
    pub fn logs(&self) -> PathBuf {
        self.root.join("logs")
    }
    pub fn audio(&self) -> PathBuf {
        self.root.join("audio")
    }
    pub fn config_dir(&self) -> PathBuf {
        self.root.join("config")
    }
    pub fn settings_file(&self) -> PathBuf {
        self.config_dir().join("settings.json")
    }

    pub fn clipboard_backup(&self) -> PathBuf {
        self.root.join("clipboard-restore.txt")
    }

    pub fn utterances(&self) -> PathBuf {
        self.logs().join("utterances.jsonl")
    }

    pub fn model_file(&self, kind: &str, filename: &str) -> PathBuf {
        match kind {
            "llm" => self.models_llm().join(filename),
            _ => self.models_whisper().join(filename),
        }
    }
}

pub fn is_inside_boundary(root: &Path, candidate: &Path) -> bool {
    candidate.starts_with(root)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn override_data_dir_is_used() {
        let dir = tempdir().unwrap();
        std::env::set_var("LOCALFLOW_DATA_DIR", dir.path());
        let paths = DataPaths::detect();
        assert_eq!(paths.root, dir.path());
        std::env::remove_var("LOCALFLOW_DATA_DIR");
    }

    #[test]
    fn boundary_rejects_outside_paths() {
        let root = PathBuf::from("/tmp/LocalFlow");
        assert!(is_inside_boundary(&root, &root.join("models")));
        assert!(!is_inside_boundary(&root, Path::new("/tmp/other")));
    }

    #[test]
    fn audio_dir_is_private() {
        let dir = tempdir().unwrap();
        let paths = DataPaths::from_override(dir.path().to_path_buf());
        paths.ensure().unwrap();
        assert!(paths.audio().is_dir());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(paths.audio())
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o700);
        }
    }
}
