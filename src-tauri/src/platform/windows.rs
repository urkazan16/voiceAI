//! Windows host integration: clipboard paste via SendInput, autostart, lock.

use super::shared::{self, Chord};
use super::{ClipboardItem, Cue, InsertRequest, Platform};
use crate::error::{LfError, LfResult};
use crate::injection;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{HANDLE, HWND, LPARAM};
use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, OpenClipboard, SetClipboardData,
};
use windows_sys::Win32::System::Memory::{
    GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE,
};
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegSetValueExW, HKEY_CURRENT_USER, KEY_WRITE,
    REG_OPTION_NON_VOLATILE, REG_SZ,
};
use windows_sys::Win32::System::StationsAndDesktops::{
    CloseDesktop, OpenInputDesktop, SwitchDesktop, DESKTOP_SWITCHDESKTOP,
};
use windows_sys::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, SendInput, INPUT, INPUT_KEYBOARD, KEYEVENTF_KEYUP, VIRTUAL_KEY, VK_CONTROL,
    VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_MENU, VK_RCONTROL, VK_RMENU, VK_RSHIFT, VK_RWIN,
    VK_SHIFT, VK_SPACE, VK_V,
};
use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId, IsWindow, MessageBoxW,
    SetForegroundWindow, MB_ICONINFORMATION, MB_OK, SW_SHOWNORMAL,
};

const SND_ASYNC: u32 = 0x0001;
const SND_NODEFAULT: u32 = 0x0002;
const SND_ALIAS: u32 = 0x0001_0000;

#[link(name = "winmm")]
extern "system" {
    fn PlaySoundW(psz_sound: *const u16, hmod: HANDLE, fdw_sound: u32) -> i32;
}

pub struct Windows;

const CF_UNICODETEXT: u32 = 13;
const AUTOSTART_VALUE: &str = "LocalFlow";
const CLIP_UTF16: &str = "CF_UNICODETEXT";

fn last_error() -> u32 {
    unsafe { windows_sys::Win32::Foundation::GetLastError() }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

fn from_wide(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

fn win_err(op: &str) -> LfError {
    LfError::InjectionFailed(format!("{op} failed (win32 {})", last_error()))
}

fn key_down(vk: VIRTUAL_KEY) -> bool {
    unsafe { GetAsyncKeyState(vk as i32) as u16 & 0x8000 != 0 }
}

fn control_down() -> bool {
    key_down(VK_CONTROL) || key_down(VK_LCONTROL) || key_down(VK_RCONTROL)
}

fn shift_down() -> bool {
    key_down(VK_SHIFT) || key_down(VK_LSHIFT) || key_down(VK_RSHIFT)
}

fn alt_down() -> bool {
    key_down(VK_MENU) || key_down(VK_LMENU) || key_down(VK_RMENU)
}

fn win_down() -> bool {
    key_down(VK_LWIN) || key_down(VK_RWIN)
}

fn send_vk(vk: VIRTUAL_KEY, up: bool) {
    let mut input = unsafe { std::mem::zeroed::<INPUT>() };
    input.r#type = INPUT_KEYBOARD;
    unsafe {
        input.Anonymous.ki.wVk = vk;
        input.Anonymous.ki.dwFlags = if up { KEYEVENTF_KEYUP } else { 0 };
        SendInput(1, &input, std::mem::size_of::<INPUT>() as i32);
    }
}

fn release_stuck_modifiers() {
    for vk in [
        VK_CONTROL,
        VK_LCONTROL,
        VK_RCONTROL,
        VK_SHIFT,
        VK_LSHIFT,
        VK_RSHIFT,
        VK_MENU,
        VK_LMENU,
        VK_RMENU,
        VK_LWIN,
        VK_RWIN,
    ] {
        if key_down(vk) {
            send_vk(vk, true);
        }
    }
}

fn modifier_down() -> bool {
    control_down() || shift_down() || alt_down() || win_down()
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

fn prepare_keyboard() {
    wait_for_modifiers_up(Duration::from_millis(250));
    release_stuck_modifiers();
}

fn post_paste() -> LfResult<()> {
    send_vk(VK_CONTROL, false);
    send_vk(VK_V, false);
    send_vk(VK_V, true);
    send_vk(VK_CONTROL, true);
    Ok(())
}

fn open_clipboard() -> LfResult<()> {
    let started = Instant::now();
    loop {
        if unsafe { OpenClipboard(0 as HWND) } != 0 {
            return Ok(());
        }
        if started.elapsed() > Duration::from_millis(200) {
            return Err(win_err("OpenClipboard"));
        }
        std::thread::sleep(Duration::from_millis(8));
    }
}

fn set_clipboard_unicode(text: &str) -> LfResult<()> {
    let mut encoded: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    let bytes = encoded.len() * 2;
    open_clipboard()?;
    let result = (|| -> LfResult<()> {
        if unsafe { EmptyClipboard() } == 0 {
            return Err(win_err("EmptyClipboard"));
        }
        let handle = unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes) };
        if handle.is_null() {
            return Err(win_err("GlobalAlloc"));
        }
        let locked = unsafe { GlobalLock(handle) } as *mut u16;
        if locked.is_null() {
            return Err(win_err("GlobalLock"));
        }
        unsafe {
            std::ptr::copy_nonoverlapping(encoded.as_mut_ptr(), locked, encoded.len());
            GlobalUnlock(handle);
        }
        if unsafe { SetClipboardData(CF_UNICODETEXT, handle) }.is_null() {
            return Err(win_err("SetClipboardData"));
        }
        Ok(())
    })();
    unsafe {
        CloseClipboard();
    }
    result
}

fn snapshot_unicode() -> Vec<ClipboardItem> {
    if open_clipboard().is_err() {
        return Vec::new();
    }
    let handle = unsafe { GetClipboardData(CF_UNICODETEXT) };
    let mut items = Vec::new();
    if !handle.is_null() {
        let locked = unsafe { GlobalLock(handle) } as *const u8;
        if !locked.is_null() {
            let size = unsafe { GlobalSize(handle) };
            if size > 0 && size < 16 * 1024 * 1024 {
                let bytes = unsafe { std::slice::from_raw_parts(locked, size) }.to_vec();
                items.push((CLIP_UTF16.to_string(), bytes));
            }
            unsafe {
                GlobalUnlock(handle);
            }
        }
    }
    unsafe {
        CloseClipboard();
    }
    items
}

fn restore_unicode(items: &[ClipboardItem]) -> bool {
    let Some((_, bytes)) = items.iter().find(|(ty, _)| ty == CLIP_UTF16) else {
        return false;
    };
    if bytes.len() < 2 {
        return false;
    }
    let u16_len = bytes.len() / 2;
    let mut encoded = vec![0u16; u16_len];
    for (i, chunk) in bytes.chunks_exact(2).enumerate() {
        encoded[i] = u16::from_le_bytes([chunk[0], chunk[1]]);
    }
    let text = from_wide(&encoded);
    set_clipboard_unicode(&text).is_ok()
}

fn snapshot_plain_text(items: &[ClipboardItem]) -> Option<String> {
    items.iter().find_map(|(ty, bytes)| {
        if ty != CLIP_UTF16 {
            return None;
        }
        let u16s: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        Some(from_wide(&u16s))
    })
}

fn hwnd_for_pid(pid: u32) -> Option<HWND> {
    struct State {
        pid: u32,
        hwnd: HWND,
    }
    unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> i32 {
        let state = &mut *(lparam as *mut State);
        let mut window_pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut window_pid);
        if window_pid == state.pid && IsWindow(hwnd) != 0 {
            state.hwnd = hwnd;
            return 0;
        }
        1
    }
    let mut state = State {
        pid,
        hwnd: 0 as HWND,
    };
    unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::EnumWindows(
            Some(enum_proc),
            &mut state as *mut State as LPARAM,
        );
    }
    if state.hwnd.is_null() {
        None
    } else {
        Some(state.hwnd)
    }
}

fn focus_pid(pid: Option<i32>) {
    let Some(pid) = pid else {
        return;
    };
    let Some(hwnd) = hwnd_for_pid(pid as u32) else {
        return;
    };
    unsafe {
        let fg = GetForegroundWindow();
        let mut fg_pid = 0u32;
        let fg_thread = GetWindowThreadProcessId(fg, &mut fg_pid);
        let our_thread = GetCurrentThreadId();
        if fg_thread != 0 && fg_thread != our_thread {
            let _ = AttachThreadInput(our_thread, fg_thread, 1);
            let _ = SetForegroundWindow(hwnd);
            let _ = AttachThreadInput(our_thread, fg_thread, 0);
        } else {
            let _ = SetForegroundWindow(hwnd);
        }
    }
}

fn window_title(hwnd: HWND) -> Option<String> {
    let mut buf = [0u16; 512];
    let n = unsafe { GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
    if n <= 0 {
        return None;
    }
    Some(from_wide(&buf))
}

fn data_root_from(appdata: Option<&str>, user_profile: Option<&str>) -> PathBuf {
    if let Some(appdata) = appdata.filter(|v| !v.is_empty()) {
        return PathBuf::from(appdata).join(crate::paths::APP_DIR_NAME);
    }
    if let Some(profile) = user_profile.filter(|v| !v.is_empty()) {
        return PathBuf::from(profile)
            .join("AppData")
            .join("Roaming")
            .join(crate::paths::APP_DIR_NAME);
    }
    PathBuf::from(".").join(crate::paths::APP_DIR_NAME)
}

fn autostart_key(
    write: impl FnOnce(windows_sys::Win32::System::Registry::HKEY) -> LfResult<()>,
) -> LfResult<()> {
    let sub = wide("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
    let mut key = 0 as windows_sys::Win32::System::Registry::HKEY;
    let status = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            sub.as_ptr(),
            0,
            std::ptr::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            std::ptr::null(),
            &mut key,
            std::ptr::null_mut(),
        )
    };
    if status != 0 {
        return Err(LfError::Other(format!("registry Run key error {status}")));
    }
    let result = write(key);
    unsafe {
        RegCloseKey(key);
    }
    result
}

impl Platform for Windows {
    fn data_root(&self) -> PathBuf {
        data_root_from(
            std::env::var("APPDATA").ok().as_deref(),
            std::env::var("USERPROFILE").ok().as_deref(),
        )
    }

    fn set_clipboard_text(&self, text: &str) -> LfResult<()> {
        set_clipboard_unicode(text)
    }

    fn restore_clipboard_items(&self, items: &[ClipboardItem]) -> bool {
        restore_unicode(items)
    }

    fn insert_text(&self, request: &InsertRequest<'_>) -> LfResult<()> {
        prepare_keyboard();
        focus_pid(request.target_pid);
        std::thread::sleep(shared::insert_pause(
            request.insert_delay_ms,
            request.target_app,
        ));

        let previous = if request.restore_clipboard {
            Some(snapshot_unicode())
        } else {
            None
        };
        if let Some(prev) = &previous {
            let _ = injection::persist_clipboard_snapshot(prev);
            if let Some(plain) = snapshot_plain_text(prev) {
                let _ = injection::persist_clipboard_backup(&plain);
            }
        }

        set_clipboard_unicode(request.text)?;
        std::thread::sleep(Duration::from_millis(16));
        let paste_result = post_paste();
        release_stuck_modifiers();

        if let Some(prev) = previous {
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(80));
                let _ = restore_unicode(&prev);
                injection::clear_clipboard_backups();
            });
        }
        paste_result
    }

    fn prepare_keyboard(&self) {
        prepare_keyboard();
    }

    fn frontmost_target(&self) -> (Option<i32>, Option<String>) {
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.is_null() {
                return (None, None);
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, &mut pid);
            let name = window_title(hwnd);
            (if pid == 0 { None } else { Some(pid as i32) }, name)
        }
    }

    fn activate_pid(&self, pid: u32) -> bool {
        let Some(hwnd) = hwnd_for_pid(pid) else {
            return false;
        };
        unsafe { SetForegroundWindow(hwnd) != 0 }
    }

    fn talk_combo_held(&self, hotkey: &str) -> bool {
        Chord::parse(hotkey).combo_held(
            control_down(),
            shift_down(),
            win_down(),
            alt_down(),
            key_down(VK_SPACE),
        )
    }

    fn talk_modifiers_held(&self, hotkey: &str) -> bool {
        Chord::parse(hotkey).modifiers_held(control_down(), shift_down(), win_down(), alt_down())
    }

    fn space_key_down(&self) -> bool {
        key_down(VK_SPACE)
    }

    fn accessibility_trusted(&self) -> bool {
        true
    }

    fn open_privacy_pane(&self, kind: &str) -> LfResult<()> {
        let url = match kind {
            "microphone" => "ms-settings:privacy-microphone",
            "speech" => "ms-settings:privacy-speechtyping",
            "accessibility" => "ms-settings:easeofaccess",
            _ => {
                return Err(LfError::ConfigInvalid(format!(
                    "unknown privacy pane {kind}"
                )))
            }
        };
        let wide_url = wide(url);
        let open = wide("open");
        let rc = unsafe {
            ShellExecuteW(
                0 as HWND,
                open.as_ptr(),
                wide_url.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                SW_SHOWNORMAL,
            )
        };
        // ShellExecute returns > 32 on success (HINSTANCE as isize).
        if rc as isize > 32 {
            Ok(())
        } else {
            Err(LfError::Other(format!(
                "could not open Windows Settings for {kind}"
            )))
        }
    }

    fn set_autostart(&self, enabled: bool) -> LfResult<()> {
        autostart_key(|key| {
            let name = wide(AUTOSTART_VALUE);
            if enabled {
                let exe = std::env::current_exe()?;
                let quoted = format!("\"{}\"", exe.display());
                let value = wide(&quoted);
                let bytes = (value.len() * 2) as u32;
                let status = unsafe {
                    RegSetValueExW(
                        key,
                        name.as_ptr(),
                        0,
                        REG_SZ,
                        value.as_ptr() as *const u8,
                        bytes,
                    )
                };
                if status != 0 {
                    Err(LfError::Other(format!("registry set error {status}")))
                } else {
                    Ok(())
                }
            } else {
                let _ = unsafe { RegDeleteValueW(key, name.as_ptr()) };
                Ok(())
            }
        })
    }

    fn screen_is_locked(&self) -> bool {
        unsafe {
            let desk = OpenInputDesktop(0, 0, DESKTOP_SWITCHDESKTOP);
            if desk.is_null() {
                return true;
            }
            let switched = SwitchDesktop(desk);
            CloseDesktop(desk);
            switched == 0
        }
    }

    fn play_cue(&self, cue: Cue, _volume: f32) {
        let alias = match cue {
            Cue::Start => wide("SystemNotification"),
            Cue::End => wide("SystemAsterisk"),
        };
        unsafe {
            PlaySoundW(
                alias.as_ptr(),
                0 as HANDLE,
                SND_ALIAS | SND_ASYNC | SND_NODEFAULT,
            );
        }
    }

    fn report_already_running(&self, message: &str) {
        let text = wide(message);
        let title = wide("LocalFlow");
        unsafe {
            MessageBoxW(
                0 as HWND,
                text.as_ptr(),
                title.as_ptr(),
                MB_OK | MB_ICONINFORMATION,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_root_prefers_appdata() {
        assert_eq!(
            data_root_from(
                Some("C:\\Users\\tester\\AppData\\Roaming"),
                Some("C:\\Users\\tester")
            ),
            PathBuf::from("C:\\Users\\tester\\AppData\\Roaming\\LocalFlow")
        );
    }

    #[test]
    fn user_profile_covers_a_missing_appdata() {
        assert_eq!(
            data_root_from(None, Some("C:\\Users\\tester")),
            PathBuf::from("C:\\Users\\tester\\AppData\\Roaming\\LocalFlow")
        );
        assert_eq!(
            data_root_from(Some(""), Some("C:\\Users\\tester")),
            PathBuf::from("C:\\Users\\tester\\AppData\\Roaming\\LocalFlow")
        );
    }

    #[test]
    fn paste_posts_ctrl_v_and_never_select_all() {
        let prod = include_str!("windows.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        assert!(prod.contains("VK_V"), "paste must send Ctrl+V");
        assert!(
            !prod.contains("VK_A") && !prod.contains("0x41"),
            "Select-All would wipe the field and break mid-text insert"
        );
        assert!(prod.contains("wait_for_modifiers_up(Duration::from_millis(250))"));
    }
}
