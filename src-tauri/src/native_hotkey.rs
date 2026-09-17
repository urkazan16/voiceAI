//! Native keyboard monitoring for keys which are not representable by the
//! Tauri global-shortcut parser.  macOS exposes Fn through `flagsChanged`
//! events rather than as a normal global shortcut.

#[cfg(target_os = "macos")]
mod macos {
    use crate::dictation::{self, DictationCmd};
    use std::ffi::c_void;
    use std::ptr;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{mpsc, Mutex, OnceLock};
    use std::time::Duration;
    use tauri::{AppHandle, Emitter};

    const CG_EVENT_FLAGS_CHANGED: u32 = 12;
    const CG_EVENT_TAP_DISABLED_BY_TIMEOUT: u32 = 0xffff_fffe;
    const CG_EVENT_TAP_DISABLED_BY_USER_INPUT: u32 = 0xffff_fffd;
    const CG_EVENT_TAP_OPTION_LISTEN_ONLY: u32 = 1;
    const HID_EVENT_TAP: u32 = 0;
    const HEAD_INSERTION: u32 = 0;
    const FN_KEYCODE: u16 = 0x3f;
    const KEYBOARD_KEYCODE_FIELD: u32 = 9;
    // kCGEventFlagMaskSecondaryFn from CGEventTypes.h.
    const SECONDARY_FN_FLAG: u64 = 0x0080_0000;

    struct State {
        hotkey: Mutex<String>,
        pressed: AtomicBool,
        capture: AtomicBool,
        available: AtomicBool,
    }

    static STATE: OnceLock<State> = OnceLock::new();
    static APP: OnceLock<Mutex<Option<AppHandle>>> = OnceLock::new();

    #[derive(Clone, serde::Serialize)]
    struct NativeHotkeyEvent {
        key: &'static str,
        pressed: bool,
    }

    pub fn set_app_handle(app: AppHandle) {
        if let Ok(mut current) = APP.get_or_init(|| Mutex::new(None)).lock() {
            *current = Some(app);
        }
    }

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGEventTapCreate(
            tap: u32,
            place: u32,
            options: u32,
            mask: u64,
            callback: extern "C" fn(*mut c_void, u32, *mut c_void, *mut c_void) -> *mut c_void,
            user_info: *mut c_void,
        ) -> *mut c_void;
        fn CGEventTapEnable(tap: *mut c_void, enable: bool);
        fn CGEventGetFlags(event: *mut c_void) -> u64;
        fn CGEventGetIntegerValueField(event: *mut c_void, field: u32) -> i64;
        fn CGEventSourceKeyState(state_id: i32, key: u16) -> bool;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFMachPortCreateRunLoopSource(
            allocator: *const c_void,
            port: *mut c_void,
            order: isize,
        ) -> *mut c_void;
        fn CFRunLoopGetCurrent() -> *mut c_void;
        fn CFRunLoopAddSource(run_loop: *mut c_void, source: *mut c_void, mode: *const c_void);
        fn CFRunLoopRun();
        static kCFRunLoopDefaultMode: *const c_void;
    }

    pub fn start() {
        if STATE.get().is_some() {
            return;
        }
        let state = STATE.get_or_init(|| State {
            hotkey: Mutex::new(String::new()),
            pressed: AtomicBool::new(false),
            capture: AtomicBool::new(false),
            available: AtomicBool::new(false),
        });
        // Raw pointers are intentionally reconstructed on the event-tap
        // thread; the state itself is stored in a process-lifetime OnceLock.
        let state_ptr = state as *const State as usize;
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("localflow-macos-event-tap".into())
            .spawn(move || unsafe {
                let mask = 1u64 << CG_EVENT_FLAGS_CHANGED;
                let tap = CGEventTapCreate(
                    HID_EVENT_TAP,
                    HEAD_INSERTION,
                    CG_EVENT_TAP_OPTION_LISTEN_ONLY,
                    mask,
                    event_tap_callback,
                    state_ptr as *mut c_void,
                );
                if tap.is_null() {
                    let _ = ready_tx.send(false);
                    return;
                }
                let source = CFMachPortCreateRunLoopSource(ptr::null(), tap, 0);
                if source.is_null() {
                    let _ = ready_tx.send(false);
                    return;
                }
                CFRunLoopAddSource(CFRunLoopGetCurrent(), source, kCFRunLoopDefaultMode);
                CGEventTapEnable(tap, true);
                state.available.store(true, Ordering::Release);
                let _ = ready_tx.send(true);
                CFRunLoopRun();
            })
            .ok();
        // Creation is synchronous from the caller's point of view, so saving
        // Fn can fail cleanly when Input Monitoring/Accessibility is missing.
        let tap_ready = ready_rx
            .recv_timeout(Duration::from_secs(1))
            .unwrap_or(false);
        if !tap_ready {
            state.available.store(true, Ordering::Release);
            std::thread::Builder::new()
                .name("localflow-macos-fn-poll".into())
                .spawn(move || {
                    let mut previous = false;
                    loop {
                        std::thread::sleep(Duration::from_millis(15));
                        let down = unsafe { CGEventSourceKeyState(1, FN_KEYCODE) };
                        if down != previous {
                            previous = down;
                            handle_fn_state(state, down);
                        }
                    }
                })
                .ok();
        }
    }

    extern "C" fn event_tap_callback(
        proxy: *mut c_void,
        event_type: u32,
        event: *mut c_void,
        user_info: *mut c_void,
    ) -> *mut c_void {
        if event_type == CG_EVENT_TAP_DISABLED_BY_TIMEOUT
            || event_type == CG_EVENT_TAP_DISABLED_BY_USER_INPUT
        {
            // The tap is listen-only and can be re-enabled after macOS
            // temporarily disables it for a slow callback.
            if !proxy.is_null() {
                unsafe { CGEventTapEnable(proxy, true) };
            }
            return event;
        }
        if event_type != CG_EVENT_FLAGS_CHANGED || event.is_null() || user_info.is_null() {
            return event;
        }
        let state = unsafe { &*(user_info as *const State) };
        let keycode = unsafe { CGEventGetIntegerValueField(event, KEYBOARD_KEYCODE_FIELD) } as u16;
        if keycode != FN_KEYCODE {
            return event;
        }
        let down = unsafe { CGEventGetFlags(event) } & SECONDARY_FN_FLAG != 0;
        handle_fn_state(state, down);
        event
    }

    fn handle_fn_state(state: &State, down: bool) {
        if state.capture.load(Ordering::Acquire) {
            if down {
                if let Some(app) = APP
                    .get()
                    .and_then(|slot| slot.lock().ok().and_then(|current| current.clone()))
                {
                    let _ = app.emit(
                        "native-hotkey-event",
                        NativeHotkeyEvent {
                            key: "Fn",
                            pressed: true,
                        },
                    );
                }
            }
            return;
        }
        let configured = state
            .hotkey
            .lock()
            .map(|value| {
                matches!(
                    value.trim().to_ascii_lowercase().as_str(),
                    "fn" | "function" | "globe"
                )
            })
            .unwrap_or(false);
        if !configured {
            return;
        }
        if state.pressed.swap(down, Ordering::AcqRel) == down {
            return;
        }
        if down {
            if !dictation::is_busy() {
                dictation::enqueue(DictationCmd::Pressed);
            }
        } else {
            dictation::enqueue(DictationCmd::Released);
        }
    }

    pub fn configure(hotkey: &str) {
        if let Some(state) = STATE.get() {
            if let Ok(mut current) = state.hotkey.lock() {
                *current = hotkey.to_string();
            }
            state.pressed.store(false, Ordering::Release);
        }
    }

    pub fn active_for(hotkey: &str) -> bool {
        STATE
            .get()
            .map(|state| {
                state.available.load(Ordering::Acquire)
                    && matches!(
                        hotkey.trim().to_ascii_lowercase().as_str(),
                        "fn" | "function" | "globe"
                    )
            })
            .unwrap_or(false)
    }

    pub fn set_capture(value: bool) {
        if let Some(state) = STATE.get() {
            state.capture.store(value, Ordering::Release);
            state.pressed.store(false, Ordering::Release);
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod macos {
    pub fn start() {}
    pub fn configure(_hotkey: &str) {}
    pub fn active_for(_hotkey: &str) -> bool {
        false
    }
    pub fn set_capture(_value: bool) {}
    pub fn set_app_handle(_app: tauri::AppHandle) {}
}

pub use macos::{active_for, configure, set_app_handle, set_capture, start};
