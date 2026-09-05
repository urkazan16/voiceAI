use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BuildInfo {
    pub application: String,
    pub version: String,
    pub git_sha: String,
    pub platform: String,
    pub architecture: String,
    pub build_date: String,
    pub tauri_version: String,
    pub rustc_version: String,
    pub native_runtime: String,
}

pub fn current() -> BuildInfo {
    let platform = if cfg!(target_os = "macos") {
        if cfg!(target_arch = "x86_64") {
            "macOS Intel"
        } else if cfg!(target_arch = "aarch64") {
            "macOS Apple Silicon"
        } else {
            "macOS"
        }
    } else {
        std::env::consts::OS
    };

    BuildInfo {
        application: "LocalFlow".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        git_sha: env!("LOCALFLOW_GIT_SHA").into(),
        platform: platform.into(),
        architecture: std::env::consts::ARCH.into(),
        build_date: env!("LOCALFLOW_BUILD_DATE").into(),
        tauri_version: "2.2.5".into(),
        rustc_version: "1.88.0".into(),
        native_runtime: crate::runtime::runtime_id(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_info_identifies_application() {
        let info = current();
        assert_eq!(info.application, "LocalFlow");
        assert_eq!(info.version, "0.1.0");
        assert!(!info.git_sha.is_empty());
        assert!(
            info.native_runtime.starts_with("whisper-rs/"),
            "{}",
            info.native_runtime
        );
        assert!(!info.native_runtime.contains("stub"));
    }
}
