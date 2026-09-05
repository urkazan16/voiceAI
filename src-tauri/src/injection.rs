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

pub struct ClipboardInjector {
    pub target_pid: Option<i32>,
}

impl Default for ClipboardInjector {
    fn default() -> Self {
        Self { target_pid: None }
    }
}

impl TextInjector for ClipboardInjector {
    fn insert_text(&self, text: &str, restore_clipboard: bool) -> LfResult<()> {
        insert_via_clipboard(text, restore_clipboard, self.target_pid)
    }
}

pub fn frontmost_unix_id() -> Option<i32> {
    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("osascript")
            .args([
                "-e",
                "tell application \"System Events\" to get unix id of first application process whose frontmost is true",
            ])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        String::from_utf8(output.stdout)
            .ok()?
            .trim()
            .parse()
            .ok()
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

fn insert_via_clipboard(
    text: &str,
    restore_clipboard: bool,
    target_pid: Option<i32>,
) -> LfResult<()> {
    #[cfg(target_os = "macos")]
    {
        macos::insert_text(text, restore_clipboard, target_pid)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (text, restore_clipboard, target_pid);
        Err(LfError::InjectionFailed(
            "clipboard injection is implemented for macOS in this MVP".into(),
        ))
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use std::ffi::c_void;
    use std::io::Write;
    use std::time::{Duration, Instant};

    const VK_COMMAND: u16 = 0x37;
    const VK_RIGHT_COMMAND: u16 = 0x36;
    const VK_SHIFT: u16 = 0x38;
    const VK_RIGHT_SHIFT: u16 = 0x3C;
    const VK_CONTROL: u16 = 0x3B;
    const VK_RIGHT_CONTROL: u16 = 0x3E;
    const VK_OPTION: u16 = 0x3A;
    const VK_RIGHT_OPTION: u16 = 0x3D;
    const VK_SPACE: u16 = 0x31;
    const VK_ANSI_V: u16 = 0x09;
    const COMMAND_FLAG: u64 = 0x0010_0000;
    const SESSION_EVENT_TAP: u32 = 1;
    const HID_SYSTEM_STATE: i32 = 1;
    const COMBINED_SESSION_STATE: i32 = 0;

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGEventSourceCreate(state_id: i32) -> *mut c_void;
        fn CGEventSourceKeyState(state_id: i32, key: u16) -> bool;
        fn CGEventCreateKeyboardEvent(
            source: *mut c_void,
            virtual_key: u16,
            key_down: bool,
        ) -> *mut c_void;
        fn CGEventSetFlags(event: *mut c_void, flags: u64);
        fn CGEventPost(tap: u32, event: *mut c_void);
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFRelease(cf: *const c_void);
    }

    pub fn prepare_keyboard() {
        wait_for_modifiers_up(Duration::from_millis(800));
        release_stuck_modifiers();
    }

    pub fn insert_text(text: &str, restore_clipboard: bool, target_pid: Option<i32>) -> LfResult<()> {
        prepare_keyboard();
        focus_pid(target_pid);
        std::thread::sleep(Duration::from_millis(40));

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

        post_paste()?;
        release_stuck_modifiers();

        if let Some(prev) = previous {
            std::thread::sleep(Duration::from_millis(40));
            let mut child = std::process::Command::new("pbcopy")
                .stdin(std::process::Stdio::piped())
                .spawn()
                .map_err(|e| LfError::InjectionFailed(e.to_string()))?;
            if let Some(stdin) = child.stdin.as_mut() {
                let _ = stdin.write_all(prev.as_bytes());
            }
            let _ = child.wait();
        }
        Ok(())
    }

    fn focus_pid(pid: Option<i32>) {
        let Some(pid) = pid else {
            return;
        };
        let script = format!(
            "tell application \"System Events\" to set frontmost of first application process whose unix id is {pid} to true"
        );
        let _ = std::process::Command::new("osascript")
            .args(["-e", &script])
            .status();
    }

    fn modifier_down() -> bool {
        unsafe {
            CGEventSourceKeyState(COMBINED_SESSION_STATE, VK_CONTROL)
                || CGEventSourceKeyState(COMBINED_SESSION_STATE, VK_RIGHT_CONTROL)
                || CGEventSourceKeyState(COMBINED_SESSION_STATE, VK_SHIFT)
                || CGEventSourceKeyState(COMBINED_SESSION_STATE, VK_RIGHT_SHIFT)
                || CGEventSourceKeyState(COMBINED_SESSION_STATE, VK_COMMAND)
                || CGEventSourceKeyState(COMBINED_SESSION_STATE, VK_RIGHT_COMMAND)
                || CGEventSourceKeyState(COMBINED_SESSION_STATE, VK_OPTION)
                || CGEventSourceKeyState(COMBINED_SESSION_STATE, VK_RIGHT_OPTION)
                || CGEventSourceKeyState(COMBINED_SESSION_STATE, VK_SPACE)
        }
    }

    fn wait_for_modifiers_up(timeout: Duration) {
        let start = Instant::now();
        while modifier_down() && start.elapsed() < timeout {
            std::thread::sleep(Duration::from_millis(16));
        }
        if modifier_down() {
            release_stuck_modifiers();
            std::thread::sleep(Duration::from_millis(30));
        }
    }

    fn post_paste() -> LfResult<()> {
        unsafe {
            let source = CGEventSourceCreate(HID_SYSTEM_STATE);
            let down = CGEventCreateKeyboardEvent(source, VK_ANSI_V, true);
            let up = CGEventCreateKeyboardEvent(source, VK_ANSI_V, false);
            if down.is_null() || up.is_null() {
                if !down.is_null() {
                    CFRelease(down);
                }
                if !up.is_null() {
                    CFRelease(up);
                }
                if !source.is_null() {
                    CFRelease(source);
                }
                return fallback_osascript_paste();
            }
            CGEventSetFlags(down, COMMAND_FLAG);
            CGEventSetFlags(up, COMMAND_FLAG);
            CGEventPost(SESSION_EVENT_TAP, down);
            CGEventPost(SESSION_EVENT_TAP, up);
            CFRelease(down);
            CFRelease(up);
            if !source.is_null() {
                CFRelease(source);
            }
        }
        Ok(())
    }

    fn fallback_osascript_paste() -> LfResult<()> {
        let status = std::process::Command::new("osascript")
            .args([
                "-e",
                "tell application \"System Events\" to key code 9 using command down",
            ])
            .status()
            .map_err(|e| LfError::InjectionFailed(e.to_string()))?;
        if status.success() {
            Ok(())
        } else {
            Err(LfError::PermissionDenied(
                "Accessibility permission required for insertion".into(),
            ))
        }
    }

    fn release_stuck_modifiers() {
        unsafe {
            let source = CGEventSourceCreate(HID_SYSTEM_STATE);
            for key in [
                VK_SPACE,
                VK_CONTROL,
                VK_RIGHT_CONTROL,
                VK_SHIFT,
                VK_RIGHT_SHIFT,
                VK_OPTION,
                VK_RIGHT_OPTION,
                VK_COMMAND,
                VK_RIGHT_COMMAND,
            ] {
                let up = CGEventCreateKeyboardEvent(source, key, false);
                if !up.is_null() {
                    CGEventSetFlags(up, 0);
                    CGEventPost(SESSION_EVENT_TAP, up);
                    CFRelease(up);
                }
            }
            if !source.is_null() {
                CFRelease(source);
            }
        }
    }
}

pub fn prepare_keyboard_for_insert() {
    #[cfg(target_os = "macos")]
    {
        macos::prepare_keyboard();
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
