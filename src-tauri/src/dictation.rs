use crate::audio::{self, SharedCapture};
use crate::engine::SharedEngine;
use crate::error::LfError;
use crate::injection::ClipboardInjector;
use crate::llm::NativeLlm;
use crate::pipeline::PipelineState;
use crate::stt::NativeStt;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

static CANCEL: AtomicBool = AtomicBool::new(false);
static PRESS_AT: Mutex<Option<Instant>> = Mutex::new(None);
static WORKER: OnceLock<Sender<DictationCmd>> = OnceLock::new();
static BOUND_HOTKEYS: Mutex<(String, String, String, String)> =
    Mutex::new((String::new(), String::new(), String::new(), String::new()));
static MICROPHONE: Mutex<Option<String>> = Mutex::new(None);
static VAD_BITS: AtomicU32 = AtomicU32::new(0);
static LAST_RMS_BITS: AtomicU32 = AtomicU32::new(0);
static HANDS_FREE: AtomicBool = AtomicBool::new(false);
static TRAY_MARK: Mutex<String> = Mutex::new(String::new());
static TRAY_TIP: Mutex<String> = Mutex::new(String::new());

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
    if hands_free && is_recording {
        ReleaseAction::StayRecording
    } else if is_recording && held < MIN_PTT_HOLD {
        ReleaseAction::DiscardTooShort
    } else {
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
                "Command+Control+C".into(),
                "Command+Control+V".into(),
                "Command+Control+E".into(),
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

fn cached_rms() -> f32 {
    f32::from_bits(LAST_RMS_BITS.load(Ordering::Relaxed))
}

fn remember_rms(rms: f32) {
    LAST_RMS_BITS.store(rms.to_bits(), Ordering::Relaxed);
}

fn cached_microphone() -> Option<String> {
    MICROPHONE.lock().ok().and_then(|g| g.clone())
}

#[derive(Debug, Clone, Copy)]
pub enum DictationCmd {
    Pressed,
    Released,
    Cancel,
    Stop,
    CopyLast,
    PasteLast,
}

pub fn start_worker(app: AppHandle, engine: SharedEngine, capture: SharedCapture) {
    let (tx, rx) = mpsc::channel();
    let _ = WORKER.set(tx);
    std::thread::Builder::new()
        .name("localflow-dictation".into())
        .spawn(move || {
            while let Ok(cmd) = rx.recv() {
                match cmd {
                    DictationCmd::Pressed => on_hotkey_pressed(&app, &engine, &capture),
                    DictationCmd::Released => on_hotkey_released(&app, &engine, &capture),
                    DictationCmd::Cancel => cancel(&app, &engine, &capture),
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

pub fn clear_cancel() {
    CANCEL.store(false, Ordering::Relaxed);
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

pub fn emit_state(app: &AppHandle, state: DictationState) {
    sync_tray(app, &state.phase);
    for label in ["main", "bar"] {
        if let Some(window) = app.get_webview_window(label) {
            let _ = window.emit("dictation-state", &state);
        }
    }
}

pub fn notify_hotkey(app: &AppHandle, edge: &str) {
    emit_state(
        app,
        DictationState {
            phase: edge.into(),
            message: if edge == "pressed" {
                "Hotkey down…".into()
            } else {
                "Hotkey up…".into()
            },
            transcript: None,
            raw_transcript: None,
            duration_ms: 0,
            insert_ok: true,
            rms: 0.0,
            wpm: None,
        },
    );
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
    if let Some(window) = app.get_webview_window("bar") {
        crate::position_flow_bar(&window);
        let _ = window.show();
    }
}

pub fn hide_bar_later(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(2400));
        if CANCEL.load(Ordering::Relaxed) {
            return;
        }
        if let Some(window) = app.get_webview_window("bar") {
            let _ = window.hide();
        }
    });
}

pub fn on_hotkey_pressed(app: &AppHandle, engine: &SharedEngine, capture: &SharedCapture) {
    if crate::screenlock::screen_is_locked() {
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
    if capture.is_recording() {
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
    CANCEL.store(false, Ordering::Relaxed);
    if let Ok(mut slot) = PRESS_AT.lock() {
        *slot = Some(Instant::now());
    }
    let mic = match engine.try_lock() {
        Ok(mut eng) => {
            remember_microphone(eng.settings.microphone_name.clone());
            remember_vad(eng.settings.vad_threshold);
            remember_hands_free(eng.settings.hands_free);
            eng.snapshot.reset();
            let _ = eng.snapshot.transition(PipelineState::Recording);
            eng.settings.microphone_name.clone()
        }
        Err(_) => cached_microphone(),
    };
    let engine_for_target = engine.clone();
    std::thread::spawn(move || {
        let (pid, name) = crate::injection::frontmost_target();
        if let Ok(mut eng) = engine_for_target.lock() {
            eng.insert_target_pid = pid;
            eng.insert_target_app = name;
        }
    });
    match capture.start(mic) {
        Ok(()) => {
            crate::journal::log("record_start", "microphone on");
            if let Ok(eng) = engine.try_lock() {
                if eng.settings.sound_cues {
                    crate::cues::play_start(eng.settings.sound_cue_volume);
                }
            }
            show_bar(app, engine);
            spawn_streaming_preview(app, engine, capture);
            spawn_level_meter(app, capture);
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
    match classify_release_ex(held, capture.is_recording(), cached_hands_free()) {
        ReleaseAction::DiscardTooShort => discard_short_hold(app, engine, capture),
        ReleaseAction::Process => finish_recording(app, engine, capture),
        ReleaseAction::StayRecording => {}
    }
}

pub fn stop_and_process(app: &AppHandle, engine: &SharedEngine, capture: &SharedCapture) {
    finish_recording(app, engine, capture);
}

pub fn cancel(app: &AppHandle, engine: &SharedEngine, capture: &SharedCapture) {
    CANCEL.store(true, Ordering::Relaxed);
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
    if let Some(window) = app.get_webview_window("bar") {
        let _ = window.hide();
    }
}

fn spawn_level_meter(app: &AppHandle, capture: &SharedCapture) {
    let app = app.clone();
    let capture = capture.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(80));
        if !capture.is_recording() || CANCEL.load(Ordering::Relaxed) {
            remember_rms(0.0);
            break;
        }
        let Some(peeked) = capture.peek() else {
            continue;
        };
        let pcm = audio::to_whisper_pcm(&peeked);
        let window = if pcm.len() > 1_600 {
            &pcm[pcm.len() - 1_600..]
        } else {
            pcm.as_slice()
        };
        let rms = crate::vad::rms(window);
        remember_rms(rms);
        if let Some(err) = crate::audio::take_stream_error() {
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
            continue;
        }
        let held = PRESS_AT
            .lock()
            .ok()
            .and_then(|g| *g)
            .map(|t| t.elapsed())
            .unwrap_or_default();
        let quiet = rms < cached_vad() * 0.45;
        let warn = held > Duration::from_millis(700) && quiet;
        let partial = crate::whisper_stt::last_partial();
        let message = if warn {
            "No mic signal — check the input device.".into()
        } else {
            partial.clone().unwrap_or_else(|| "Listening…".into())
        };
        emit_state(
            &app,
            DictationState {
                phase: "recording".into(),
                message,
                transcript: partial,
                raw_transcript: None,
                duration_ms: audio::duration_ms(&peeked),
                insert_ok: true,
                rms,
                wpm: None,
            },
        );
    });
}

fn spawn_streaming_preview(app: &AppHandle, engine: &SharedEngine, capture: &SharedCapture) {
    crate::whisper_stt::allow_partial();
    let app = app.clone();
    let engine = engine.clone();
    let capture = capture.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(1100));
        if !capture.is_recording() || CANCEL.load(Ordering::Relaxed) {
            break;
        }
        let Some(peeked) = capture.peek() else {
            continue;
        };
        let pcm = audio::to_whisper_pcm(&peeked);
        if pcm.len() < 16_000 {
            continue;
        }
        let (path, lang) = match engine.try_lock() {
            Ok(eng) => (
                eng.ready_model_path("stt"),
                eng.settings.stt_language.clone(),
            ),
            Err(_) => continue,
        };
        if let Some(path) = path {
            crate::whisper_stt::try_partial(&path, &pcm, &lang);
        }
        if let Some(text) = crate::whisper_stt::last_partial() {
            emit_state(
                &app,
                DictationState {
                    phase: "recording".into(),
                    message: text.clone(),
                    transcript: Some(text),
                    raw_transcript: None,
                    duration_ms: audio::duration_ms(&peeked),
                    insert_ok: true,
                    rms: cached_rms(),
                    wpm: None,
                },
            );
        }
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
    if let Some(window) = app.get_webview_window("bar") {
        let _ = window.hide();
    }
}

fn finish_recording(app: &AppHandle, engine: &SharedEngine, capture: &SharedCapture) {
    if let Ok(mut slot) = PRESS_AT.lock() {
        *slot = None;
    }
    if CANCEL.load(Ordering::Relaxed) {
        let _ = capture.stop();
        return;
    }
    let Some(captured) = capture.stop() else {
        crate::journal::log("record_stop", "microphone off (empty)");
        return;
    };
    crate::journal::log("record_stop", "microphone off");
    emit_state(
        app,
        DictationState {
            phase: "processing".into(),
            message: "Processing…".into(),
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
        crate::injection::prepare_keyboard_for_insert();
        if CANCEL.load(Ordering::Relaxed) {
            emit_cancelled(&app, &engine);
            return;
        }
        let duration_ms = audio::duration_ms(&captured);
        let mut pcm =
            crate::vad::trim_silence_at(&audio::to_whisper_pcm(&captured), 16_000, cached_vad());
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
        if !crate::vad::had_speech_at(&pcm, 16_000, cached_vad()) {
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
        ) = match engine.lock() {
            Ok(eng) => (
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
            ),
            Err(_) => {
                fail(&app, &engine, "engine lock poisoned", duration_ms);
                return;
            }
        };
        if keep_audio {
            let _ = crate::macos_stt::write_wav_s16le_mono(&last_wav, 16_000, &pcm);
        } else {
            let _ = std::fs::remove_file(&last_wav);
        }
        let Some(stt_path) = stt_path else {
            fail(
                &app,
                &engine,
                &crate::error::user_guidance(&LfError::ModelMissing("whisper-medium".into())),
                duration_ms,
            );
            return;
        };
        let raw = match crate::stt::transcribe_with_paragraph_pauses(
            &NativeStt,
            &pcm,
            Some(&stt_path),
            &lang,
            cached_vad(),
        ) {
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
            let result = match engine_for_pipe.lock() {
                Ok(mut eng) => eng.run_text_pipeline(
                    &raw,
                    &NativeStt,
                    &NativeLlm,
                    &ClipboardInjector {
                        target_pid: pid,
                        target_app: app_name,
                        insert_delay_ms: delay_ms,
                    },
                    &[],
                ),
                Err(_) => Err(LfError::Other("engine lock poisoned".into())),
            };
            let _ = tx.send(result);
        });
        let result = match rx.recv_timeout(Duration::from_millis(timeout_ms.max(1_000))) {
            Ok(r) => r,
            Err(_) => {
                CANCEL.store(true, Ordering::Relaxed);
                fail(
                    &app,
                    &engine,
                    "Post-processing timed out. Raise the timeout in Settings.",
                    duration_ms,
                );
                CANCEL.store(false, Ordering::Relaxed);
                return;
            }
        };
        if CANCEL.load(Ordering::Relaxed)
            || matches!(&result, Err(LfError::Other(m)) if m == "cancelled")
        {
            emit_cancelled(&app, &engine);
            return;
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
                                "Text ready but insert failed. Copy last / Paste last: {}",
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

fn emit_cancelled(app: &AppHandle, engine: &SharedEngine) {
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
    if let Some(window) = app.get_webview_window("bar") {
        let _ = window.hide();
    }
}

fn fail(app: &AppHandle, engine: &SharedEngine, message: &str, duration_ms: u64) {
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
            ReleaseAction::StayRecording
        );
        assert_eq!(
            classify_release_ex(Duration::from_millis(80), true, false),
            ReleaseAction::DiscardTooShort
        );
    }

    #[test]
    fn default_talk_hotkey_cache_is_control_shift_space() {
        let (talk, copy, paste, edit) = bound_hotkeys();
        assert!(talk.to_lowercase().contains("space") || talk == "Control+Shift+Space");
        assert!(!copy.is_empty());
        assert!(!paste.is_empty());
        assert!(!edit.is_empty());
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
        assert!(TRAY_MARK_IDLE.is_empty());
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
}
