use crate::audio::{self, SharedCapture};
use crate::engine::SharedEngine;
use crate::error::LfError;
use crate::injection::{ClipboardInjector, MemoryInjector, TextInjector};
use crate::llm::NativeLlm;
use crate::pipeline::PipelineState;
use crate::stt::NativeStt;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

static CANCEL: AtomicBool = AtomicBool::new(false);
/// Number of active pipelines. T-One may briefly have a finishing phrase and
/// the next captured phrase at once; a counter prevents the first listener
/// from making the second one look idle.
static BUSY: AtomicUsize = AtomicUsize::new(0);
static PRESS_AT: Mutex<Option<Instant>> = Mutex::new(None);
static WORKER: OnceLock<Sender<DictationCmd>> = OnceLock::new();
type AudioPersistJob = (std::path::PathBuf, bool, Vec<f32>);
static AUDIO_WRITER: OnceLock<Sender<AudioPersistJob>> = OnceLock::new();
static BOUND_HOTKEYS: Mutex<(String, String, String, String)> =
    Mutex::new((String::new(), String::new(), String::new(), String::new()));
static MICROPHONE: Mutex<Option<String>> = Mutex::new(None);
static VAD_BITS: AtomicU32 = AtomicU32::new(0);
static HANDS_FREE: AtomicBool = AtomicBool::new(false);
static TONE_MODE: AtomicBool = AtomicBool::new(false);
static TRAY_MARK: Mutex<String> = Mutex::new(String::new());
static TRAY_TIP: Mutex<String> = Mutex::new(String::new());
static APP: Mutex<Option<AppHandle>> = Mutex::new(None);
/// Each T-One decoding job owns a durable cancellation flag. The shared flag
/// is reset for the next utterance, so it cannot safely control an older job.
static TONE_CANCELLATIONS: Mutex<Vec<Weak<AtomicBool>>> = Mutex::new(Vec::new());

/// Holds shorter than this are discarded. A 320 ms tap used to enter hands-free
/// and leave the microphone open.
pub const MIN_PTT_HOLD: Duration = Duration::from_millis(500);
pub const REPEAT_PRESS_GUARD: Duration = Duration::from_millis(250);

/// Menu-bar title marks. Idle is empty so the template icon stays clean.
pub const TRAY_MARK_IDLE: &str = "";
pub const TRAY_MARK_RECORDING: &str = "●";
pub const TRAY_MARK_PROCESSING: &str = "◐";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayKind {
    Idle,
    Recording,
    Processing,
}

/// Map a dictation phase to one of three tray states.
pub fn tray_kind_for_phase(phase: &str) -> TrayKind {
    match phase {
        "recording" | "pressed" => TrayKind::Recording,
        "processing" | "released" => TrayKind::Processing,
        _ => TrayKind::Idle,
    }
}

pub fn tray_mark_for_phase(phase: &str) -> &'static str {
    match tray_kind_for_phase(phase) {
        TrayKind::Recording => TRAY_MARK_RECORDING,
        TrayKind::Processing => TRAY_MARK_PROCESSING,
        TrayKind::Idle => TRAY_MARK_IDLE,
    }
}

pub fn tray_tooltip_for_phase(phase: &str) -> &'static str {
    match tray_kind_for_phase(phase) {
        TrayKind::Recording => "LocalFlow — recording",
        TrayKind::Processing => "LocalFlow — processing",
        TrayKind::Idle => "LocalFlow",
    }
}

pub fn tray_appearance(phase: &str) -> (&'static str, &'static str) {
    (tray_mark_for_phase(phase), tray_tooltip_for_phase(phase))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseAction {
    DiscardTooShort,
    Process,
    StayRecording,
}

pub fn classify_release(held: Duration, is_recording: bool) -> ReleaseAction {
    classify_release_ex(held, is_recording, false)
}

pub fn classify_release_ex(held: Duration, is_recording: bool, hands_free: bool) -> ReleaseAction {
    if !is_recording {
        ReleaseAction::Process
    } else if held < MIN_PTT_HOLD {
        if hands_free {
            ReleaseAction::StayRecording
        } else {
            ReleaseAction::DiscardTooShort
        }
    } else {
        // Hold-to-talk always processes on release, even if hands-free is enabled.
        ReleaseAction::Process
    }
}

/// Cached shortcuts so the Carbon/hotkey callback never waits on the engine mutex
/// (Whisper can hold that lock for tens of seconds).
pub fn remember_hotkeys(talk: String, copy: String, paste: String, edit: String) {
    if let Ok(mut slot) = BOUND_HOTKEYS.lock() {
        *slot = (talk, copy, paste, edit);
    }
}

pub fn bound_hotkeys() -> (String, String, String, String) {
    BOUND_HOTKEYS
        .lock()
        .ok()
        .map(|g| g.clone())
        .filter(|(talk, _, _, _)| !talk.is_empty())
        .unwrap_or_else(|| {
            (
                "Control+Shift+Space".into(),
                crate::platform::default_copy_hotkey().into(),
                crate::platform::default_paste_hotkey().into(),
                crate::platform::default_edit_hotkey().into(),
            )
        })
}

pub fn remember_microphone(name: Option<String>) {
    if let Ok(mut slot) = MICROPHONE.lock() {
        *slot = name;
    }
}

pub fn remember_vad(threshold: f32) {
    VAD_BITS.store(
        crate::vad::clamp_threshold(threshold).to_bits(),
        Ordering::Relaxed,
    );
}

pub fn remember_hands_free(enabled: bool) {
    HANDS_FREE.store(enabled, Ordering::Relaxed);
}

pub fn remember_stt_engine(engine: &str) {
    TONE_MODE.store(engine.trim().eq_ignore_ascii_case("tone"), Ordering::Relaxed);
}

pub fn tone_engine_active() -> bool {
    TONE_MODE.load(Ordering::Relaxed)
}

fn cached_hands_free() -> bool {
    HANDS_FREE.load(Ordering::Relaxed)
}

fn cached_vad() -> f32 {
    let bits = VAD_BITS.load(Ordering::Relaxed);
    if bits == 0 {
        crate::vad::default_threshold()
    } else {
        f32::from_bits(bits)
    }
}

fn cached_microphone() -> Option<String> {
    MICROPHONE.lock().ok().and_then(|g| g.clone())
}

#[derive(Debug, Clone, Copy)]
pub enum DictationCmd {
    Pressed,
    Released,
    Cancel(CancelSource),
    Stop,
    CopyLast,
    PasteLast,
}

/// Explicitly preserve the origin of a cancellation. It is the quickest way
/// to distinguish an Escape event, a UI action and a programmatic stop when a
/// user reports an unexpected `Cancelled.` state.
#[derive(Debug, Clone, Copy)]
pub enum CancelSource {
    EscapeShortcut,
    TrayMenu,
    FlowBarCancel,
    FlowBarDismiss,
    UserInterface,
}

impl CancelSource {
    fn label(self) -> &'static str {
        match self {
            Self::EscapeShortcut => "escape_shortcut",
            Self::TrayMenu => "tray_menu",
            Self::FlowBarCancel => "flow_bar_cancel",
            Self::FlowBarDismiss => "flow_bar_dismiss",
            Self::UserInterface => "user_interface",
        }
    }
}

pub fn start_worker(app: AppHandle, engine: SharedEngine, capture: SharedCapture) {
    if let Ok(mut slot) = APP.lock() {
        *slot = Some(app.clone());
    }
    let (tx, rx) = mpsc::channel();
    let _ = WORKER.set(tx);
    std::thread::Builder::new()
        .name("localflow-dictation".into())
        .spawn(move || {
            while let Ok(cmd) = rx.recv() {
                let result = crate::error::catch_runtime_panic("Dictation command", || match cmd {
                    DictationCmd::Pressed => on_hotkey_pressed(&app, &engine, &capture),
                    DictationCmd::Released => on_hotkey_released(&app, &engine, &capture),
                    DictationCmd::Cancel(source) => {
                        crate::journal::log(
                            "dictation_cancel",
                            &format!("source={}", source.label()),
                        );
                        cancel(&app, &engine, &capture)
                    }
                    DictationCmd::Stop => stop_and_process(&app, &engine, &capture),
                    DictationCmd::CopyLast => {
                        let _ = engine
                            .lock()
                            .ok()
                            .and_then(|eng| eng.copy_last_transcript().ok());
                    }
                    DictationCmd::PasteLast => {
                        let _ = engine
                            .lock()
                            .ok()
                            .and_then(|eng| eng.paste_last_transcript().ok());
                    }
                });
                if let Err(err) = result {
                    CANCEL.store(true, Ordering::Relaxed);
                    cancel_tone_sessions();
                    BUSY.store(0, Ordering::Relaxed);
                    let _ = capture.stop();
                    fail(&app, &engine, &crate::error::user_guidance(&err), 0);
                }
            }
        })
        .expect("start dictation worker");
}

pub fn enqueue(cmd: DictationCmd) {
    if let Some(tx) = WORKER.get() {
        let _ = tx.send(cmd);
    }
}

pub fn cancel_flag() -> &'static AtomicBool {
    &CANCEL
}

pub fn is_cancelled() -> bool {
    CANCEL.load(Ordering::Relaxed)
}

pub fn is_busy() -> bool {
    BUSY.load(Ordering::Relaxed) > 0
}

struct BusyGuard;

impl Drop for BusyGuard {
    fn drop(&mut self) {
        let _ = BUSY.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |count| {
            count.checked_sub(1)
        });
    }
}

pub fn clear_cancel() {
    CANCEL.store(false, Ordering::Relaxed);
}

fn new_tone_cancellation() -> Arc<AtomicBool> {
    let token = Arc::new(AtomicBool::new(false));
    if let Ok(mut tokens) = TONE_CANCELLATIONS.lock() {
        tokens.retain(|weak| weak.strong_count() > 0);
        tokens.push(Arc::downgrade(&token));
    }
    token
}

fn cancel_tone_sessions() {
    if let Ok(mut tokens) = TONE_CANCELLATIONS.lock() {
        tokens.retain(|weak| {
            if let Some(token) = weak.upgrade() {
                token.store(true, Ordering::Relaxed);
                true
            } else {
                false
            }
        });
    }
}

#[derive(Clone, Serialize)]
pub struct DictationState {
    pub phase: String,
    pub message: String,
    pub transcript: Option<String>,
    pub raw_transcript: Option<String>,
    pub duration_ms: u64,
    pub insert_ok: bool,
    #[serde(default)]
    pub rms: f32,
    #[serde(default)]
    pub wpm: Option<f64>,
}

fn dictation_state(
    phase: &str,
    message: impl Into<String>,
    transcript: Option<String>,
    raw_transcript: Option<String>,
    duration_ms: u64,
    insert_ok: bool,
) -> DictationState {
    DictationState {
        phase: phase.into(),
        message: message.into(),
        transcript,
        raw_transcript,
        duration_ms,
        insert_ok,
        rms: 0.0,
        wpm: None,
    }
}

/// Miniaturized or hidden WKWebViews can stall IPC; tray still updates.
pub fn window_accepts_dictation_events(visible: bool, minimized: bool) -> bool {
    visible && !minimized
}

fn on_ui(app: &AppHandle, f: impl FnOnce(&AppHandle) + Send + 'static) {
    let app = app.clone();
    let _ = app.clone().run_on_main_thread(move || f(&app));
}

pub fn emit_state(app: &AppHandle, state: DictationState) {
    on_ui(app, move |app| {
        sync_tray(app, &state.phase);
        for label in ["main", "bar"] {
            if let Some(window) = app.get_webview_window(label) {
                if label != "bar" {
                    let visible = window.is_visible().unwrap_or(false);
                    let minimized = window.is_minimized().unwrap_or(false);
                    if !window_accepts_dictation_events(visible, minimized) {
                        continue;
                    }
                }
                let _ = window.emit("dictation-state", &state);
            }
        }
    });
}

pub fn emit_transcribe_progress(progress: crate::whisper_stt::TranscribeProgress) {
    if let Ok(guard) = APP.lock() {
        if let Some(app) = guard.as_ref() {
            let _ = app.emit("transcribe-progress", &progress);
        }
    }
}

fn sync_tray(app: &AppHandle, phase: &str) {
    if let Some(tray) = app.tray_by_id("localflow") {
        let (mark, tooltip) = tray_appearance(phase);
        let mut same = false;
        if let (Ok(mut last_mark), Ok(mut last_tip)) = (TRAY_MARK.lock(), TRAY_TIP.lock()) {
            same = last_mark.as_str() == mark && last_tip.as_str() == tooltip;
            if !same {
                last_mark.clear();
                last_mark.push_str(mark);
                last_tip.clear();
                last_tip.push_str(tooltip);
            }
        }
        if same {
            return;
        }
        let _ = tray.set_tooltip(Some(tooltip));
        let _ = tray.set_title(Some(mark));
    }
}

pub fn show_bar(app: &AppHandle, engine: &SharedEngine) {
    let enabled = engine
        .try_lock()
        .map(|eng| eng.settings.show_flow_bar)
        .unwrap_or(true);
    if !enabled {
        return;
    }
    let target_pid = engine.try_lock().ok().and_then(|eng| eng.insert_target_pid);
    on_ui(app, move |app| {
        if let Some(window) = app.get_webview_window("bar") {
            crate::show_flow_bar(&window, target_pid);
        }
    });
}

/// Hide the overlay so Cmd+V is not delivered to LocalFlow.
pub fn conceal_overlay() {
    if let Ok(guard) = APP.lock() {
        if let Some(app) = guard.as_ref() {
            hide_bar(app);
        }
    }
}

pub fn reveal_overlay() {
    if let Ok(guard) = APP.lock() {
        if let Some(app) = guard.as_ref() {
            let app = app.clone();
            let _ = app.clone().run_on_main_thread(move || {
                if let Some(window) = app.get_webview_window("bar") {
                    crate::show_flow_bar(&window, None);
                }
            });
        }
    }
}

fn hide_bar(app: &AppHandle) {
    on_ui(app, |app| {
        if let Some(window) = app.get_webview_window("bar") {
            crate::hide_flow_bar_window(&window);
        }
    });
}

pub fn hide_bar_later(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(1200));
        if CANCEL.load(Ordering::Relaxed) {
            return;
        }
        if is_busy() {
            return;
        }
        if let Some(capture) = app.try_state::<SharedCapture>() {
            if capture.is_recording() {
                return;
            }
        }
        hide_bar(&app);
    });
}

pub fn on_hotkey_pressed(app: &AppHandle, engine: &SharedEngine, capture: &SharedCapture) {
    if crate::screenlock::screen_is_locked() {
        crate::diagnostics::error("record_start", "SCREEN_LOCKED");
        emit_state(
            app,
            DictationState {
                phase: "error".into(),
                message: "Screen is locked. Unlock to dictate.".into(),
                transcript: None,
                raw_transcript: None,
                duration_ms: 0,
                insert_ok: true,
                rms: 0.0,
                wpm: None,
            },
        );
        return;
    }
    // T-One sessions are queued by its worker. Let the next hold begin while
    // the previous phrase is finalizing so normal repeated dictation is not
    // ignored and its microphone audio is retained in the bounded queue.
    if is_busy() && !capture.is_recording() && !engine_is_tone(engine) {
        return;
    }
    if capture.is_recording() {
        if cached_hands_free() && !engine_is_tone(engine) {
            let too_soon = PRESS_AT
                .lock()
                .ok()
                .and_then(|g| *g)
                .map(|t| t.elapsed() < REPEAT_PRESS_GUARD)
                .unwrap_or(false);
            if too_soon {
                return;
            }
            stop_and_process(app, engine, capture);
            return;
        }
        let (talk, _, _, _) = bound_hotkeys();
        if crate::injection::talk_modifiers_held(&talk) {
            // Key-repeat while the chord is still down.
            return;
        }
        // Chord already up but the stream is still open (missed Released).
        stop_and_process(app, engine, capture);
        return;
    }
    CANCEL.store(false, Ordering::Relaxed);
    if let Ok(mut slot) = PRESS_AT.lock() {
        *slot = Some(Instant::now());
    }
    let (mic, tone_path) = match engine.try_lock() {
        Ok(mut eng) => {
            remember_microphone(eng.settings.microphone_name.clone());
            remember_vad(eng.settings.vad_threshold);
            remember_hands_free(eng.settings.hands_free);
            remember_stt_engine(&eng.settings.stt_engine);
            eng.snapshot.reset();
            let _ = eng.snapshot.transition(PipelineState::Recording);
            (
                eng.settings.microphone_name.clone(),
                eng.settings
                    .stt_engine
                    .eq_ignore_ascii_case("tone")
                    .then(|| eng.ready_model_path("stt"))
                    .flatten(),
            )
        }
        Err(_) => (cached_microphone(), None),
    };
    // Capture Chrome/etc. before the overlay is shown. Showing the bar with
    // Tauri's show() makes LocalFlow frontmost, so a delayed NSWorkspace
    // read would paste into the bar instead of the field.
    let (pid, name) = crate::injection::frontmost_target();
    if let Ok(mut eng) = engine.lock() {
        if !crate::injection::is_own_process(pid, name.as_deref()) {
            eng.insert_target_pid = pid;
            eng.insert_target_app = name;
        }
    }
    if engine_is_tone(engine) && tone_path.is_none() {
        emit_state(
            app,
            dictation_state(
                "error",
                "T-One is not installed. Download T-One Streaming Russian in Models first.",
                last_processed(engine),
                last_raw(engine),
                0,
                true,
            ),
        );
        return;
    }
    let tone_cancellation = tone_path.as_ref().map(|_| new_tone_cancellation());
    let start = if let Some(path) = tone_path {
        capture
            .start_streaming(mic)
            .and_then(|audio| {
                crate::tone_stt::start_session(
                    path,
                    audio,
                    tone_cancellation
                        .as_ref()
                        .expect("T-One session has a cancellation token")
                        .clone(),
                )
            })
            .map(Some)
    } else {
        capture.start(mic).map(|()| None)
    };
    match start {
        Ok(tone_events) => {
            if let Some(events) = tone_events {
                BUSY.fetch_add(1, Ordering::Relaxed);
                spawn_tone_listener(
                    app,
                    engine,
                    events,
                    tone_cancellation.expect("T-One listener has a cancellation token"),
                );
            }
            crate::journal::log("record_start", "microphone on");
            if let Ok(eng) = engine.try_lock() {
                if eng.settings.sound_cues {
                    crate::cues::play_start(eng.settings.sound_cue_volume);
                }
            }
            show_bar(app, engine);
            spawn_level_meter(app, capture);
            spawn_ptt_release_watch(capture);
            emit_state(
                app,
                DictationState {
                    phase: "recording".into(),
                    message: "Listening…".into(),
                    transcript: None,
                    raw_transcript: None,
                    duration_ms: 0,
                    insert_ok: true,
                    rms: 0.0,
                    wpm: None,
                },
            );
        }
        Err(err) => {
            crate::diagnostics::error("record_start", &format!("code={} detail={err}", err.code()));
            crate::journal::log("record_start_failed", &format!("code={} {err}", err.code()));
            let _ = capture.stop();
            if let Ok(mut eng) = engine.lock() {
                eng.snapshot.fail(err.to_string());
            }
            emit_state(
                app,
                DictationState {
                    phase: "error".into(),
                    message: crate::error::user_guidance(&err),
                    transcript: last_processed(engine),
                    raw_transcript: last_raw(engine),
                    duration_ms: 0,
                    insert_ok: true,
                    rms: 0.0,
                    wpm: None,
                },
            );
        }
    }
}

pub fn on_hotkey_released(app: &AppHandle, engine: &SharedEngine, capture: &SharedCapture) {
    let held = PRESS_AT
        .lock()
        .ok()
        .and_then(|g| *g)
        .map(|t| t.elapsed())
        .unwrap_or(Duration::from_secs(1));
    // A T-One stream must be closed on release so its last decoder frames are
    // flushed and inserted. The setting is normalized to false as well, but
    // this guard also handles a live settings change without a restart.
    let hands_free = cached_hands_free() && !engine_is_tone(engine);
    match classify_release_ex(held, capture.is_recording(), hands_free) {
        ReleaseAction::DiscardTooShort if engine_is_tone(engine) => {
            crate::journal::log("tone_discard", "hotkey hold was under 500 ms");
            CANCEL.store(true, Ordering::Relaxed);
            cancel_tone_sessions();
            discard_short_hold(app, engine, capture);
        }
        ReleaseAction::DiscardTooShort => discard_short_hold(app, engine, capture),
        ReleaseAction::Process if engine_is_tone(engine) => {
            finish_tone_recording(app, engine, capture)
        }
        ReleaseAction::Process => finish_recording(app, engine, capture),
        ReleaseAction::StayRecording => {}
    }
}

pub fn stop_and_process(app: &AppHandle, engine: &SharedEngine, capture: &SharedCapture) {
    if engine_is_tone(engine) {
        finish_tone_recording(app, engine, capture);
    } else {
        finish_recording(app, engine, capture);
    }
}

pub fn cancel(app: &AppHandle, engine: &SharedEngine, capture: &SharedCapture) {
    CANCEL.store(true, Ordering::Relaxed);
    cancel_tone_sessions();
    if let Ok(mut slot) = PRESS_AT.lock() {
        *slot = None;
    }
    let _ = capture.stop();
    if let Ok(mut eng) = engine.lock() {
        eng.snapshot.reset();
    }
    emit_state(
        app,
        DictationState {
            phase: "cancelled".into(),
            message: "Cancelled.".into(),
            transcript: last_processed(engine),
            raw_transcript: last_raw(engine),
            duration_ms: 0,
            insert_ok: true,
            rms: 0.0,
            wpm: None,
        },
    );
    hide_bar(app);
}

fn spawn_ptt_release_watch(capture: &SharedCapture) {
    let capture = capture.clone();
    let (talk, _, _, _) = bound_hotkeys();
    if !poll_physical_ptt_release(&talk) {
        return;
    }
    std::thread::spawn(move || {
        let mut saw_space = false;
        let began = std::time::Instant::now();
        loop {
            std::thread::sleep(Duration::from_millis(20));
            if !capture.is_recording() || CANCEL.load(Ordering::Relaxed) {
                break;
            }
            if cached_hands_free() {
                continue;
            }
            if began.elapsed() < Duration::from_millis(80) {
                continue;
            }
            if crate::injection::space_key_down() {
                saw_space = true;
            }
            if crate::injection::talk_combo_held(&talk) {
                continue;
            }
            // Space often missing from the event tap; keep the stream only while
            // modifiers stay down, and only until we have seen Space go up.
            if !saw_space && crate::injection::talk_modifiers_held(&talk) {
                continue;
            }
            enqueue(DictationCmd::Released);
            break;
        }
    });
}

fn poll_physical_ptt_release(hotkey: &str) -> bool {
    let t = hotkey.trim().to_ascii_lowercase();
    t.contains("space")
}

fn spawn_level_meter(app: &AppHandle, capture: &SharedCapture) {
    let app = app.clone();
    let capture = capture.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(80));
        if !capture.is_recording() || CANCEL.load(Ordering::Relaxed) {
            break;
        }
        let Some(peeked) = capture.peek_tail(8_000) else {
            continue;
        };
        let pcm = audio::to_whisper_pcm(&peeked);
        let window = if pcm.len() > 1_600 {
            &pcm[pcm.len() - 1_600..]
        } else {
            pcm.as_slice()
        };
        let rms = crate::vad::rms(window);
        if let Some(err) = crate::audio::take_stream_error() {
            crate::journal::log("audio_stream_error", &err);
            crate::diagnostics::error("audio_stream", &err);
            CANCEL.store(true, Ordering::Relaxed);
            cancel_tone_sessions();
            if let Ok(mut slot) = PRESS_AT.lock() {
                *slot = None;
            }
            let _ = capture.stop();
            emit_state(
                &app,
                DictationState {
                    phase: "error".into(),
                    message: crate::error::user_guidance(&LfError::DeviceUnavailable(err)),
                    transcript: None,
                    raw_transcript: None,
                    duration_ms: audio::duration_ms(&peeked),
                    insert_ok: true,
                    rms,
                    wpm: None,
                },
            );
            hide_bar(&app);
            break;
        }
        let held = PRESS_AT
            .lock()
            .ok()
            .and_then(|g| *g)
            .map(|t| t.elapsed())
            .unwrap_or_default();
        let quiet = rms < crate::vad::soft_threshold(cached_vad()) * 0.45;
        let warn = held > Duration::from_millis(700) && quiet;
        let message = if warn {
            "No mic signal — check the input device.".into()
        } else {
            "Listening…".into()
        };
        emit_state(
            &app,
            DictationState {
                phase: "recording".into(),
                message,
                transcript: None,
                raw_transcript: None,
                duration_ms: audio::duration_ms(&peeked),
                insert_ok: true,
                rms,
                wpm: None,
            },
        );
    });
}

fn discard_short_hold(app: &AppHandle, engine: &SharedEngine, capture: &SharedCapture) {
    if let Ok(mut slot) = PRESS_AT.lock() {
        *slot = None;
    }
    let _ = capture.stop();
    if let Ok(mut eng) = engine.lock() {
        eng.snapshot.reset();
    }
    emit_state(
        app,
        DictationState {
            phase: "error".into(),
            message: "Hold longer than 500 ms, then release to dictate.".into(),
            transcript: last_processed(engine),
            raw_transcript: last_raw(engine),
            duration_ms: 0,
            insert_ok: true,
            rms: 0.0,
            wpm: None,
        },
    );
    hide_bar(app);
}

fn engine_is_tone(_engine: &SharedEngine) -> bool {
    TONE_MODE.load(Ordering::Relaxed)
}

fn finish_tone_recording(app: &AppHandle, engine: &SharedEngine, capture: &SharedCapture) {
    if let Ok(mut slot) = PRESS_AT.lock() {
        *slot = None;
    }
    if CANCEL.load(Ordering::Relaxed) {
        let _ = capture.stop();
        return;
    }
    let Some(captured) = capture.stop() else {
        return;
    };
    let (last_wav, keep_audio) = match engine.try_lock() {
        Ok(eng) => (eng.paths.last_utterance(), eng.settings.keep_last_audio),
        Err(_) => {
            crate::journal::log("record_stop", "T-One microphone off; finalizing stream");
            emit_state(
                app,
                dictation_state(
                    "processing",
                    "Finalizing T-One stream…",
                    None,
                    None,
                    0,
                    true,
                ),
            );
            return;
        }
    };
    persist_last_audio_async(last_wav, keep_audio, audio::to_whisper_pcm(&captured));
    crate::journal::log("record_stop", "T-One microphone off; finalizing stream");
    emit_state(
        app,
        dictation_state(
            "processing",
            "Finalizing T-One stream…",
            None,
            None,
            0,
            true,
        ),
    );
}

fn spawn_tone_listener(
    app: &AppHandle,
    engine: &SharedEngine,
    events: mpsc::Receiver<crate::tone_stt::StreamEvent>,
    cancellation: Arc<AtomicBool>,
) {
    let app = app.clone();
    let engine = engine.clone();
    std::thread::spawn(move || {
        let _busy = BusyGuard;
        let mut last_final = String::new();
        while let Ok(event) = events.recv() {
            if cancellation.load(Ordering::Relaxed) {
                break;
            }
            match event {
                crate::tone_stt::StreamEvent::Partial { text, duration_ms } => {
                    emit_state(
                        &app,
                        dictation_state(
                            "recording",
                            format!("Listening… {text}"),
                            Some(text.clone()),
                            Some(text),
                            duration_ms,
                            true,
                        ),
                    );
                }
                crate::tone_stt::StreamEvent::Final { text, duration_ms } => {
                    crate::journal::log("tone_final", &format!("{duration_ms} ms"));
                    match process_tone_final(&engine, &text) {
                        Ok(Some(final_text)) => {
                            last_final = final_text.clone();
                            emit_state(
                                &app,
                                dictation_state(
                                    "recording",
                                    format!("T-One: {final_text}"),
                                    Some(final_text),
                                    Some(text),
                                    duration_ms,
                                    true,
                                ),
                            );
                        }
                        Ok(None) => {}
                        Err(message) => {
                            cancellation.store(true, Ordering::Relaxed);
                            fail(&app, &engine, &message, duration_ms);
                            break;
                        }
                    }
                }
                crate::tone_stt::StreamEvent::Finished { duration_ms } => {
                    crate::journal::log("tone_finished", &format!("{duration_ms} ms"));
                    if cancellation.load(Ordering::Relaxed) {
                        break;
                    }
                    if last_final.is_empty() {
                        fail(
                            &app,
                            &engine,
                            "No speech detected. Nothing was inserted.",
                            duration_ms,
                        );
                    } else {
                        emit_state(
                            &app,
                            dictation_state(
                                "done",
                                format!("Inserted: {last_final}"),
                                Some(last_final.clone()),
                                None,
                                duration_ms,
                                true,
                            ),
                        );
                        hide_bar_later(&app);
                    }
                    break;
                }
                crate::tone_stt::StreamEvent::Error(message) => {
                    crate::journal::log("tone_error", &message);
                    fail(&app, &engine, &message, 0);
                    break;
                }
            }
        }
    });
}

fn process_tone_final(engine: &SharedEngine, raw: &str) -> Result<Option<String>, String> {
    let raw = crate::sanitize::strip_model_tags(raw);
    if raw.is_empty() {
        return Ok(None);
    }
    // Formatting changes engine state, but posting Cmd+V can synchronously
    // touch the UI on macOS. Keep that operation outside the engine mutex.
    let (mut output, paste, target_pid, target_app, insert_delay_ms, restore_clipboard) = {
        let mut eng = engine
            .lock()
            .map_err(|_| "engine lock poisoned".to_string())?;
        let mem = MemoryInjector::default();
        let result = eng.run_text_pipeline(&raw, &NativeStt, &NativeLlm, &mem, &[]);
        let paste = mem.last.lock().ok().and_then(|slot| slot.clone());
        match result {
            Ok(output) => (
                output,
                paste,
                eng.insert_target_pid,
                eng.insert_target_app.clone(),
                eng.settings.insert_delay_ms,
                eng.settings.restore_clipboard,
            ),
            Err(err) if err.to_string() == "cancelled" => return Ok(None),
            Err(err) => {
                crate::journal::log("tone_pipeline", &err.to_string());
                return Err(crate::error::user_guidance(&err));
            }
        }
    };
    if let Some(text) = paste.filter(|text| !text.is_empty()) {
        if let Err(err) = (ClipboardInjector {
            target_pid,
            target_app,
            insert_delay_ms,
        })
        .insert_text(&text, restore_clipboard)
        {
            output.insert_ok = false;
            output.insert_error = Some(crate::error::user_guidance(&err));
            crate::journal::log("tone_insert_failed", &err.to_string());
            if let Ok(mut eng) = engine.lock() {
                if eng.session_text.ends_with(&text) {
                    let keep = eng.session_text.len() - text.len();
                    eng.session_text.truncate(keep);
                }
                if let Some(last) = eng.last_output.as_mut() {
                    last.insert_ok = false;
                    last.insert_error = output.insert_error.clone();
                }
            }
        }
    }
    Ok(Some(output.final_text))
}

fn finish_recording(app: &AppHandle, engine: &SharedEngine, capture: &SharedCapture) {
    let processing_started = Instant::now();
    if let Ok(mut slot) = PRESS_AT.lock() {
        *slot = None;
    }
    if CANCEL.load(Ordering::Relaxed) {
        let _ = capture.stop();
        return;
    }
    let Some(captured) = capture.stop() else {
        crate::journal::log("record_stop", "microphone off (empty)");
        if is_busy() {
            return;
        }
        emit_state(
            app,
            DictationState {
                phase: "idle".into(),
                message: "Ready.".into(),
                transcript: last_processed(engine),
                raw_transcript: last_raw(engine),
                duration_ms: 0,
                insert_ok: true,
                rms: 0.0,
                wpm: None,
            },
        );
        hide_bar(app);
        return;
    };
    crate::journal::log("record_stop", "microphone off");
    BUSY.store(1, Ordering::Relaxed);
    emit_state(
        app,
        DictationState {
            phase: "processing".into(),
            message: "Microphone off. Processing…".into(),
            transcript: None,
            raw_transcript: None,
            duration_ms: 0,
            insert_ok: true,
            rms: 0.0,
            wpm: None,
        },
    );
    let app = app.clone();
    let engine = engine.clone();
    std::thread::spawn(move || {
        let _busy = BusyGuard;
        let duration_ms = audio::duration_ms(&captured);
        if CANCEL.load(Ordering::Relaxed) {
            emit_cancelled(&app, &engine);
            return;
        }
        let source_pcm = audio::to_whisper_pcm(&captured);
        let configured_vad = cached_vad();
        let soft_vad = crate::vad::soft_threshold(configured_vad);
        let mut pcm = crate::vad::trim_silence_at(&source_pcm, 16_000, configured_vad);
        // Built-in and Bluetooth microphones often produce speech below the
        // configured UI threshold. Keep the user's threshold as the first
        // choice, but do one softer pass before declaring the recording empty.
        if pcm.is_empty() && !source_pcm.is_empty() {
            pcm = crate::vad::trim_silence_at(&source_pcm, 16_000, soft_vad);
        }
        drop(captured);
        // Keep the onset so the first phoneme/letter is not trimmed away.
        let mut onset = vec![0.0; 2_400];
        onset.append(&mut pcm);
        let pcm = onset;
        if pcm.len() < 4_800 + 2_400 {
            fail(
                &app,
                &engine,
                "Recording too short. Hold the hotkey, speak, then release.",
                duration_ms,
            );
            return;
        }
        if !crate::vad::had_speech_at(&pcm, 16_000, soft_vad) {
            crate::journal::log(
                "record_no_signal",
                &format!(
                    "samples={} duration_ms={} rms={:.6} threshold={:.6}",
                    source_pcm.len(),
                    duration_ms,
                    crate::vad::rms(&source_pcm),
                    configured_vad
                ),
            );
            fail(
                &app,
                &engine,
                "No mic signal — check the input device.",
                duration_ms,
            );
            return;
        }
        let (
            stt_path,
            lang,
            pid,
            app_name,
            delay_ms,
            timeout_ms,
            sounds,
            cue_vol,
            last_wav,
            keep_audio,
            decode_options,
            restore_clipboard,
            stt_engine,
        ) = match engine.lock() {
            Ok(eng) => {
                crate::whisper_stt::set_use_gpu(crate::whisper_stt::use_gpu_from_setting(
                    &eng.settings.compute_device,
                ));
                (
                    eng.ready_model_path("stt"),
                    eng.settings.stt_language.clone(),
                    eng.insert_target_pid,
                    eng.insert_target_app.clone(),
                    eng.settings.insert_delay_ms,
                    eng.settings.postprocess_timeout_ms,
                    eng.settings.sound_cues,
                    eng.settings.sound_cue_volume,
                    eng.paths.last_utterance(),
                    eng.settings.keep_last_audio,
                    eng.decode_options(),
                    eng.settings.restore_clipboard,
                    eng.settings.stt_engine.clone(),
                )
            }
            Err(_) => {
                fail(&app, &engine, "engine lock poisoned", duration_ms);
                return;
            }
        };
        if !crate::config::stt_engine_runtime_available(&stt_engine) {
            fail(
                &app,
                &engine,
                &format!("Speech engine {stt_engine} is not available in this build."),
                duration_ms,
            );
            return;
        }
        let Some(stt_path) = stt_path else {
            let model_id = crate::config::effective_stt_model_id(
                &stt_engine,
                engine
                    .lock()
                    .ok()
                    .and_then(|eng| eng.settings.active_stt_model.clone())
                    .as_deref(),
            );
            fail(
                &app,
                &engine,
                &crate::error::user_guidance(&LfError::ModelMissing(model_id)),
                duration_ms,
            );
            return;
        };
        let transcription = crate::stt::transcribe_with_paragraph_pauses(
            &NativeStt,
            &pcm,
            Some(&stt_path),
            &lang,
            cached_vad(),
            &decode_options,
        );
        persist_last_audio_async(last_wav, keep_audio, pcm);
        let raw = match transcription {
            Ok(text) => crate::sanitize::strip_model_tags(&text),
            Err(err) => {
                fail(
                    &app,
                    &engine,
                    &crate::error::user_guidance(&err),
                    duration_ms,
                );
                return;
            }
        };
        if CANCEL.load(Ordering::Relaxed) {
            emit_cancelled(&app, &engine);
            return;
        }
        let engine_for_pipe = engine.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mem = MemoryInjector::default();
            let result = match engine_for_pipe.lock() {
                Ok(mut eng) => eng.run_text_pipeline_with_prior_elapsed(
                    &raw,
                    &NativeStt,
                    &NativeLlm,
                    &mem,
                    &[],
                    processing_started.elapsed(),
                ),
                Err(_) => Err(LfError::Other("engine lock poisoned".into())),
            };
            let paste = mem.last.lock().ok().and_then(|slot| slot.clone());
            let _ = tx.send((result, paste));
        });
        let (mut result, paste) =
            match rx.recv_timeout(Duration::from_millis(timeout_ms.max(1_000))) {
                Ok(r) => r,
                Err(_) => {
                    CANCEL.store(true, Ordering::Relaxed);
                    fail(
                        &app,
                        &engine,
                        "Post-processing timed out. Raise the timeout in Settings.",
                        duration_ms,
                    );
                    return;
                }
            };
        if CANCEL.load(Ordering::Relaxed)
            || matches!(&result, Err(LfError::Other(m)) if m == "cancelled")
        {
            emit_cancelled(&app, &engine);
            return;
        }
        if let (Ok(output), Some(text)) = (result.as_mut(), paste.filter(|t| !t.is_empty())) {
            if let Err(err) = (ClipboardInjector {
                target_pid: pid,
                target_app: app_name,
                insert_delay_ms: delay_ms,
            })
            .insert_text(&text, restore_clipboard)
            {
                output.insert_ok = false;
                output.insert_error = Some(crate::error::user_guidance(&err));
                crate::journal::log("insert_failed", &err.to_string());
                if let Ok(mut eng) = engine.lock() {
                    if eng.session_text.ends_with(&text) {
                        let keep = eng.session_text.len() - text.len();
                        eng.session_text.truncate(keep);
                    }
                    if let Some(last) = eng.last_output.as_mut() {
                        last.insert_ok = false;
                        last.insert_error = output.insert_error.clone();
                    }
                }
            }
        }
        match result {
            Ok(output) => {
                if sounds {
                    crate::cues::play_end(cue_vol);
                }
                crate::journal::log(
                    "processed",
                    if output.insert_ok {
                        "inserted"
                    } else {
                        "ready"
                    },
                );
                if let Ok(eng) = engine.lock() {
                    let n: u64 = eng
                        .store
                        .get_kv("stats_recordings")
                        .ok()
                        .flatten()
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0);
                    let _ = eng.store.put_kv("stats_recordings", &(n + 1).to_string());
                }
                let inserted = output.insert_ok;
                let empty = output.final_text.is_empty();
                let words = crate::uttlog::word_count(&output.final_text);
                let wpm = crate::uttlog::wpm(words, duration_ms);
                let extra = if inserted || empty {
                    String::new()
                } else if let Some(err) = output.insert_error.clone().filter(|s| !s.is_empty()) {
                    format!(" {err}")
                } else if !crate::permissions::accessibility_trusted() {
                    " Enable Accessibility for LocalFlow in Privacy & Security, then Quit from the menu bar and reopen, then Paste last."
                        .into()
                } else {
                    String::new()
                };
                emit_state(
                    &app,
                    DictationState {
                        phase: if inserted {
                            "done".into()
                        } else {
                            "error".into()
                        },
                        message: if empty {
                            format!("Processed {duration_ms} ms of audio. No text to insert.")
                        } else if inserted {
                            format!("Inserted: {}", output.final_text)
                        } else {
                            format!(
                                "Text ready but insert failed.{extra} Copy last / Paste last: {}",
                                output.final_text
                            )
                        },
                        transcript: Some(output.final_text),
                        raw_transcript: Some(output.raw_transcript),
                        duration_ms,
                        insert_ok: inserted,
                        rms: 0.0,
                        wpm: if wpm > 0.0 { Some(wpm) } else { None },
                    },
                );
                if inserted || empty {
                    hide_bar_later(&app);
                } else {
                    show_bar(&app, &engine);
                }
            }
            Err(err) => fail(
                &app,
                &engine,
                &crate::error::user_guidance(&err),
                duration_ms,
            ),
        }
    });
}

fn persist_last_audio_async(path: std::path::PathBuf, keep: bool, pcm: Vec<f32>) {
    let writer = AUDIO_WRITER.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::channel::<AudioPersistJob>();
        std::thread::Builder::new()
            .name("localflow-audio-writer".into())
            .spawn(move || {
                while let Ok((path, keep, pcm)) = rx.recv() {
                    if keep {
                        let _ = crate::media::write_wav_s16le_mono(&path, 16_000, &pcm);
                    } else {
                        let _ = std::fs::remove_file(path);
                    }
                }
            })
            .expect("start audio persistence worker");
        tx
    });
    let _ = writer.send((path, keep, pcm));
}

fn emit_cancelled(app: &AppHandle, engine: &SharedEngine) {
    crate::journal::log("dictation_cancelled", "pipeline observed cancellation flag");
    emit_state(
        app,
        DictationState {
            phase: "cancelled".into(),
            message: "Cancelled.".into(),
            transcript: last_processed(engine),
            raw_transcript: last_raw(engine),
            duration_ms: 0,
            insert_ok: true,
            rms: 0.0,
            wpm: None,
        },
    );
    hide_bar(app);
}

fn fail(app: &AppHandle, engine: &SharedEngine, message: &str, duration_ms: u64) {
    crate::diagnostics::error(
        "dictation_failed",
        &format!("duration_ms={duration_ms} detail={message}"),
    );
    if let Ok(mut eng) = engine.lock() {
        eng.snapshot.fail(message.to_string());
    }
    emit_state(
        app,
        dictation_state(
            "error",
            message.to_string(),
            last_processed(engine),
            last_raw(engine),
            duration_ms,
            false,
        ),
    );
    hide_bar_later(app);
}

fn last_processed(engine: &SharedEngine) -> Option<String> {
    engine
        .lock()
        .ok()
        .and_then(|eng| eng.last_output.as_ref().map(|o| o.final_text.clone()))
        .filter(|t| !t.is_empty())
}

fn last_raw(engine: &SharedEngine) -> Option<String> {
    engine
        .lock()
        .ok()
        .and_then(|eng| eng.last_output.as_ref().map(|o| o.raw_transcript.clone()))
        .filter(|t| !t.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tap_under_320ms_does_not_process() {
        assert_eq!(
            classify_release(Duration::from_millis(120), true),
            ReleaseAction::DiscardTooShort
        );
        assert_eq!(
            classify_release(Duration::from_millis(319), true),
            ReleaseAction::DiscardTooShort
        );
    }

    #[test]
    fn hold_at_old_hands_free_cutoff_is_still_too_short() {
        assert_eq!(
            classify_release(Duration::from_millis(320), true),
            ReleaseAction::DiscardTooShort
        );
        assert_eq!(
            classify_release(Duration::from_millis(499), true),
            ReleaseAction::DiscardTooShort
        );
    }

    #[test]
    fn half_second_hold_processes() {
        assert_eq!(classify_release(MIN_PTT_HOLD, true), ReleaseAction::Process);
        assert_eq!(
            classify_release(Duration::from_millis(800), true),
            ReleaseAction::Process
        );
    }

    #[test]
    fn release_when_not_recording_is_a_noop_process() {
        assert_eq!(
            classify_release(Duration::from_millis(10), false),
            ReleaseAction::Process
        );
    }

    #[test]
    fn hands_free_release_keeps_the_mic_open() {
        assert_eq!(
            classify_release_ex(Duration::from_millis(80), true, true),
            ReleaseAction::StayRecording
        );
        assert_eq!(
            classify_release_ex(Duration::from_millis(800), true, true),
            ReleaseAction::Process
        );
        assert_eq!(
            classify_release_ex(Duration::from_millis(80), true, false),
            ReleaseAction::DiscardTooShort
        );
    }

    #[test]
    fn hold_to_talk_processes_even_if_hands_free_is_on() {
        assert_eq!(
            classify_release_ex(MIN_PTT_HOLD, true, true),
            ReleaseAction::Process
        );
    }

    #[test]
    fn ptt_combo_release_is_space_up_for_default_hotkey() {
        remember_hotkeys(
            "Control+Shift+Space".into(),
            crate::platform::default_copy_hotkey().into(),
            crate::platform::default_paste_hotkey().into(),
            crate::platform::default_edit_hotkey().into(),
        );
        let (talk, copy, paste, edit) = bound_hotkeys();
        assert!(talk.to_ascii_lowercase().contains("space"));
        assert!(!copy.is_empty());
        assert!(!paste.is_empty());
        assert!(!edit.is_empty());
        assert!(poll_physical_ptt_release(&talk));
        assert!(!poll_physical_ptt_release("F13"));
        assert!(!poll_physical_ptt_release("Control+Shift+D"));
        assert!(!poll_physical_ptt_release("Fn"));
    }

    #[test]
    fn tray_has_three_distinct_marks() {
        let marks = [TRAY_MARK_IDLE, TRAY_MARK_RECORDING, TRAY_MARK_PROCESSING];
        assert_eq!(marks.len(), 3);
        assert_ne!(TRAY_MARK_RECORDING, TRAY_MARK_PROCESSING);
        assert_ne!(TRAY_MARK_RECORDING, TRAY_MARK_IDLE);
        assert_ne!(TRAY_MARK_PROCESSING, TRAY_MARK_IDLE);
        assert_eq!(TRAY_MARK_RECORDING, "●");
        assert_eq!(TRAY_MARK_PROCESSING, "◐");
        assert_eq!(TRAY_MARK_IDLE, "");
    }

    #[test]
    fn tray_recording_phases_use_filled_dot() {
        for phase in ["recording", "pressed"] {
            assert_eq!(tray_kind_for_phase(phase), TrayKind::Recording, "{phase}");
            let (mark, tip) = tray_appearance(phase);
            assert_eq!(mark, TRAY_MARK_RECORDING, "{phase}");
            assert_eq!(tip, "LocalFlow — recording", "{phase}");
        }
    }

    #[test]
    fn tray_processing_phases_use_half_dot() {
        for phase in ["processing", "released"] {
            assert_eq!(tray_kind_for_phase(phase), TrayKind::Processing, "{phase}");
            let (mark, tip) = tray_appearance(phase);
            assert_eq!(mark, TRAY_MARK_PROCESSING, "{phase}");
            assert_eq!(tip, "LocalFlow — processing", "{phase}");
        }
    }

    #[test]
    fn tray_idle_phases_clear_the_title_mark() {
        for phase in ["idle", "done", "cancelled", "error", ""] {
            assert_eq!(tray_kind_for_phase(phase), TrayKind::Idle, "{phase}");
            let (mark, tip) = tray_appearance(phase);
            assert_eq!(mark, TRAY_MARK_IDLE, "{phase}");
            assert_eq!(tip, "LocalFlow", "{phase}");
        }
    }

    #[test]
    fn tray_kind_covers_the_hold_speak_release_cycle() {
        assert_eq!(tray_kind_for_phase("pressed"), TrayKind::Recording);
        assert_eq!(tray_kind_for_phase("recording"), TrayKind::Recording);
        assert_eq!(tray_kind_for_phase("released"), TrayKind::Processing);
        assert_eq!(tray_kind_for_phase("processing"), TrayKind::Processing);
        assert_eq!(tray_kind_for_phase("done"), TrayKind::Idle);
    }

    #[test]
    fn busy_flag_is_clear_when_idle() {
        assert!(!is_busy());
    }

    #[test]
    fn hidden_or_miniaturized_windows_skip_dictation_ipc() {
        assert!(window_accepts_dictation_events(true, false));
        assert!(!window_accepts_dictation_events(false, false));
        assert!(!window_accepts_dictation_events(true, true));
        assert!(!window_accepts_dictation_events(false, true));
    }

    #[test]
    fn talk_press_records_frontmost_before_showing_the_bar() {
        let src = include_str!("dictation.rs");
        let press = src
            .split("pub fn on_hotkey_pressed")
            .nth(1)
            .unwrap()
            .split("pub fn on_hotkey_released")
            .next()
            .unwrap();
        let frontmost = press.find("frontmost_target()").expect("capture target");
        let show = press.find("show_bar(").expect("show overlay");
        assert!(
            frontmost < show,
            "showing the bar first makes LocalFlow frontmost"
        );
        assert!(
            !press[..frontmost].contains("std::thread::spawn"),
            "frontmost must be read on this thread before the overlay appears"
        );
    }

    #[test]
    fn dictation_drops_the_engine_lock_before_command_v() {
        let src = include_str!("dictation.rs");
        let spawn = src
            .split("let engine_for_pipe = engine.clone();")
            .nth(1)
            .unwrap()
            .split("rx.recv_timeout")
            .next()
            .unwrap();
        assert!(
            spawn.contains("MemoryInjector"),
            "format under the engine lock without posting Command+V"
        );
        assert!(
            !spawn.contains("ClipboardInjector"),
            "Cmd+V while the engine mutex is held deadlocks the UI on macOS"
        );
        let mem = src
            .find("MemoryInjector::default()")
            .expect("pipeline uses MemoryInjector");
        let clip = src[mem..]
            .find("ClipboardInjector {")
            .map(|offset| mem + offset)
            .expect("paste once the pipeline lock is dropped");
        assert!(mem < clip, "paste once the pipeline lock is dropped");
    }

    #[test]
    fn tone_pipeline_releases_the_engine_lock_before_pasting() {
        let src = include_str!("dictation.rs");
        let tone = src
            .split("fn process_tone_final")
            .nth(1)
            .expect("T-One pipeline");
        let mem = tone
            .find("MemoryInjector::default()")
            .expect("memory injector");
        let clipboard = tone
            .find("ClipboardInjector {")
            .expect("clipboard injector");
        assert!(mem < clipboard, "T-One must paste after engine formatting");
    }

    #[test]
    fn postprocess_timeout_keeps_cancel_so_orphan_insert_is_skipped() {
        let src = include_str!("dictation.rs");
        let timeout_at = src
            .find("Post-processing timed out")
            .expect("timeout fail path");
        let window = &src[timeout_at.saturating_sub(500)..timeout_at];
        assert!(
            window.contains("CANCEL.store(true"),
            "timeout must cancel the pipeline thread"
        );
        assert!(
            !window.contains("CANCEL.store(false"),
            "clearing CANCEL on timeout lets the orphan thread paste into the wrong field"
        );
        let receive = src
            .split("let (mut result, paste)")
            .nth(1)
            .unwrap()
            .split("if CANCEL.load")
            .next()
            .unwrap();
        assert_eq!(
            receive.matches("recv_timeout").count(),
            1,
            "the configured timeout must be one deadline, not timeout plus a hidden grace period"
        );
    }

    #[test]
    fn tone_mode_uses_streaming_capture_instead_of_finish_recording() {
        let src = include_str!("dictation.rs");
        let body = src
            .split("pub fn on_hotkey_pressed")
            .nth(1)
            .unwrap()
            .split("pub fn on_hotkey_released")
            .next()
            .unwrap();
        assert!(body.contains("start_streaming"));
        assert!(body.contains("tone_stt::start_session"));
    }

    #[test]
    fn tone_allows_a_followup_phrase_while_the_previous_one_finalizes() {
        let src = include_str!("dictation.rs");
        let press = src
            .split("pub fn on_hotkey_pressed")
            .nth(1)
            .unwrap()
            .split("CANCEL.store(false")
            .next()
            .unwrap();
        assert!(
            press.contains("!engine_is_tone(engine)"),
            "T-One must queue a follow-up hold instead of silently ignoring it"
        );
        assert!(
            src.contains("BUSY.fetch_add(1"),
            "each queued T-One session must keep the busy counter active"
        );
    }

    #[test]
    fn tone_release_ignores_hands_free_and_finishes_the_stream() {
        let src = include_str!("dictation.rs");
        let release = src
            .split("pub fn on_hotkey_released")
            .nth(1)
            .unwrap()
            .split("pub fn stop_and_process")
            .next()
            .unwrap();
        assert!(release.contains("cached_hands_free() && !engine_is_tone(engine)"));
        assert!(release.contains("finish_tone_recording"));
    }

    #[test]
    fn tone_mode_is_cached_outside_the_engine_lock() {
        let src = include_str!("dictation.rs");
        assert!(src.contains("static TONE_MODE"));
        assert!(src.contains("remember_stt_engine"));
        let is_tone = src
            .split("fn engine_is_tone")
            .nth(1)
            .unwrap()
            .split("fn finish_tone_recording")
            .next()
            .unwrap();
        assert!(
            is_tone.contains("TONE_MODE.load"),
            "T-One follow-up holds must not wait on the engine mutex"
        );
        assert!(!is_tone.contains("try_lock"));
    }

    #[test]
    fn tone_empty_stream_is_an_error_not_success() {
        let src = include_str!("dictation.rs");
        let finished = src
            .split("StreamEvent::Finished")
            .nth(1)
            .unwrap()
            .split("StreamEvent::Error")
            .next()
            .unwrap();
        assert!(finished.contains("No speech detected"));
        assert!(!finished.contains("Processed {duration_ms} ms of streaming audio"));
    }

    #[test]
    fn tone_stop_persists_the_utterance_for_repeat() {
        let src = include_str!("dictation.rs");
        let finish = src
            .split("fn finish_tone_recording")
            .nth(1)
            .unwrap()
            .split("fn spawn_tone_listener")
            .next()
            .unwrap();
        assert!(finish.contains("persist_last_audio_async"));
    }

    #[test]
    fn cancel_leaves_busy_until_the_pipeline_guard_drops() {
        let src = include_str!("dictation.rs")
            .split("pub fn cancel(")
            .nth(1)
            .unwrap()
            .split("fn spawn_ptt_release_watch")
            .next()
            .unwrap();
        assert!(src.contains("CANCEL.store(true"));
        assert!(
            !src.contains("BUSY.store(false"),
            "clearing BUSY in cancel lets a new hold start while insert still runs"
        );
    }

    #[test]
    fn insert_failure_message_includes_platform_guidance() {
        let src = include_str!("dictation.rs");
        assert!(src.contains("insert_error"));
        assert!(src.contains("wayland") || src.contains("user_guidance") || src.contains("extra"));
    }

    #[test]
    fn hide_bar_later_keeps_the_bar_if_the_next_phrase_started() {
        let src = include_str!("dictation.rs")
            .split("pub fn hide_bar_later")
            .nth(1)
            .unwrap()
            .split("pub fn on_hotkey_pressed")
            .next()
            .unwrap();
        assert!(src.contains("is_recording()"), "do not hide mid-hold");
        assert!(
            src.contains("is_busy()"),
            "do not hide while insert still runs"
        );
    }

    #[test]
    fn a_dead_mic_stream_stops_the_recording() {
        let src = include_str!("dictation.rs")
            .split("take_stream_error")
            .nth(1)
            .unwrap()
            .split("let held = PRESS_AT")
            .next()
            .unwrap();
        assert!(src.contains("capture.stop()"));
        assert!(src.contains("break"), "keep polling a dead CPAL stream");
        assert!(!src.contains("continue;"));
    }
}
