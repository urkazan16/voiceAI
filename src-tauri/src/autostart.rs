use crate::error::{LfError, LfResult};
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};

const LABEL: &str = "app.localflow.desktop";

pub fn apply(enabled: bool) -> LfResult<()> {
    #[cfg(target_os = "macos")]
    {
        apply_macos(enabled)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = enabled;
        Ok(())
    }
}

#[cfg(target_os = "macos")]
fn launchctl(args: &[&str]) {
    let _ = Command::new("launchctl")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

#[cfg(target_os = "macos")]
fn apply_macos(enabled: bool) -> LfResult<()> {
    let plist = agent_path()?;
    let uid = users_id();
    let domain = format!("gui/{uid}/{LABEL}");
    if enabled {
        let exe = std::env::current_exe()?;
        let body = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>{LABEL}</string>
  <key>ProgramArguments</key>
  <array><string>{}</string></array>
  <key>RunAtLoad</key><true/>
</dict>
</plist>
"#,
            exe.display()
        );
        if let Some(parent) = plist.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&plist, body)?;
        launchctl(&["bootout", &domain]);
        let status = Command::new("launchctl")
            .args([
                "bootstrap",
                &format!("gui/{uid}"),
                plist.to_str().unwrap_or(""),
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|e| LfError::Other(e.to_string()))?;
        if !status.success() {
            launchctl(&["load", "-w", plist.to_str().unwrap_or("")]);
        }
    } else if plist.exists() {
        launchctl(&["bootout", &domain]);
        launchctl(&["unload", "-w", plist.to_str().unwrap_or("")]);
        let _ = fs::remove_file(&plist);
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn agent_path() -> LfResult<PathBuf> {
    let home = std::env::var("HOME").map_err(|e| LfError::Other(e.to_string()))?;
    Ok(PathBuf::from(home)
        .join("Library/LaunchAgents")
        .join(format!("{LABEL}.plist")))
}

#[cfg(target_os = "macos")]
fn users_id() -> u32 {
    libc_getuid()
}

#[cfg(target_os = "macos")]
extern "C" {
    fn getuid() -> u32;
}

#[cfg(target_os = "macos")]
fn libc_getuid() -> u32 {
    unsafe { getuid() }
}

#[cfg(test)]
mod tests {
    #[test]
    fn disable_is_idempotent() {
        super::apply(false).unwrap();
    }

    #[test]
    fn disable_when_agent_missing_is_silent_ok() {
        super::apply(false).unwrap();
        super::apply(false).unwrap();
    }
}
