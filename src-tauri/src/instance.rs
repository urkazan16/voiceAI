use crate::error::{LfError, LfResult};
use crate::paths::DataPaths;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::sync::Mutex;

static HELD: Mutex<Option<File>> = Mutex::new(None);

/// Exclusive GUI lock. CLI does not take this lock. Unix uses `flock`; Windows
/// uses `LockFileEx` on the same file so the second copy still fails.
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

pub fn release_gui_lock() {
    *HELD.lock().unwrap_or_else(|e| e.into_inner()) = None;
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
    crate::platform::current().activate_pid(pid)
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

#[cfg(windows)]
fn try_exclusive(file: &File) -> bool {
    use std::os::windows::io::AsRawHandle;
    #[repr(C)]
    struct Overlapped {
        internal: usize,
        internal_high: usize,
        offset: u32,
        offset_high: u32,
        event: *mut core::ffi::c_void,
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn LockFileEx(
            file: *mut core::ffi::c_void,
            flags: u32,
            reserved: u32,
            low: u32,
            high: u32,
            overlapped: *mut Overlapped,
        ) -> i32;
    }
    const LOCKFILE_FAIL_IMMEDIATELY: u32 = 1;
    const LOCKFILE_EXCLUSIVE_LOCK: u32 = 2;
    let mut ov = Overlapped {
        internal: 0,
        internal_high: 0,
        offset: 0,
        offset_high: 0,
        event: std::ptr::null_mut(),
    };
    unsafe {
        LockFileEx(
            file.as_raw_handle(),
            LOCKFILE_FAIL_IMMEDIATELY | LOCKFILE_EXCLUSIVE_LOCK,
            0,
            1,
            0,
            &mut ov,
        ) != 0
    }
}

pub fn notify_already_running(message: &str) {
    eprintln!("{message}");
    crate::platform::current().report_already_running(message);
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
