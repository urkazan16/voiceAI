use crate::error::{LfError, LfResult};
use crate::paths::DataPaths;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::sync::Mutex;

static HELD: Mutex<Option<File>> = Mutex::new(None);

/// Exclusive GUI lock. CLI does not take this lock.
pub fn acquire_gui_lock(paths: &DataPaths) -> LfResult<()> {
    paths.ensure()?;
    let path = paths.root.join("localflow.lock");
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(&path)?;
    if !try_exclusive(&file) {
        let pid = std::fs::read_to_string(&path).unwrap_or_default();
        if activate_lock_holder(&pid) {
            return Err(LfError::Other(format!(
                "LocalFlow is already running{} (activated)",
                pid_suffix(&pid)
            )));
        }
        return Err(LfError::Other(format!(
            "LocalFlow is already running{}. Quit the first copy before opening another.",
            pid_suffix(&pid)
        )));
    }
    file.set_len(0)?;
    writeln!(file, "{}", std::process::id())?;
    let _ = file.sync_all();
    *HELD.lock().map_err(|e| LfError::Other(e.to_string()))? = Some(file);
    Ok(())
}

fn activate_lock_holder(contents: &str) -> bool {
    let Ok(pid) = contents.trim().parse::<u32>() else {
        return false;
    };
    if pid == std::process::id() {
        return false;
    }
    activate_pid(pid)
}

pub fn activate_pid(pid: u32) -> bool {
    #[cfg(target_os = "macos")]
    {
        let script = format!(
            "tell application \"System Events\" to set frontmost of first process whose unix id is {pid} to true"
        );
        std::process::Command::new("osascript")
            .args(["-e", &script])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = pid;
        false
    }
}

fn pid_suffix(contents: &str) -> String {
    let pid = contents.trim();
    if pid.is_empty() {
        String::new()
    } else {
        format!(" (pid {pid})")
    }
}

#[cfg(unix)]
fn try_exclusive(file: &File) -> bool {
    use std::os::unix::io::AsRawFd;
    const LOCK_EX: i32 = 2;
    const LOCK_NB: i32 = 4;
    extern "C" {
        fn flock(fd: i32, operation: i32) -> i32;
    }
    unsafe { flock(file.as_raw_fd(), LOCK_EX | LOCK_NB) == 0 }
}

#[cfg(not(unix))]
fn try_exclusive(_file: &File) -> bool {
    true
}

pub fn notify_already_running(message: &str) {
    let safe = message.replace('"', "'");
    eprintln!("{message}");
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("osascript")
            .args(["-e", &format!("display dialog \"{safe}\" buttons {{\"OK\"}} default button 1 with title \"LocalFlow\"")])
            .status();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn second_acquire_fails() {
        let dir = tempdir().unwrap();
        let paths = DataPaths::from_override(dir.path().to_path_buf());
        acquire_gui_lock(&paths).unwrap();
        let err = acquire_gui_lock(&paths).unwrap_err();
        assert!(err.to_string().contains("already running"));
        *HELD.lock().unwrap() = None;
    }
}
