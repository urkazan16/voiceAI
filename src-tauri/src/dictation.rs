use crate::audio::{self, SharedCapture};
use crate::engine::SharedEngine;
use crate::error::LfError;
use crate::injection::ClipboardInjector;
use crate::llm::NativeLlm;
use crate::pipeline::PipelineState;
use crate::stt::{NativeStt, SpeechToText};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

static CANCEL: AtomicBool = AtomicBool::new(false);
static PRESS_AT: Mutex<Option<Instant>> = Mutex::new(None);
static WORKER: OnceLock<Sender<DictationCmd>> = OnceLock::new();
static BOUND_HOTKEYS: Mutex<(String, String, String)> =
    Mutex::new((String::new(), String::new(), String::new()));
static MICROPHONE: Mutex<Option<String>> = Mutex::new(None);

/// Holds shorter than this are discarded. A 320 ms tap used to enter hands-free
/// and leave the microphone open.
pub const MIN_PTT_HOLD: Duration = Duration::from_millis(500);
pub const REPEAT_PRESS_GUARD: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseAction {
    DiscardTooShort,
    Process,
}

pub fn classify_release(held: Duration, is_recording: bool) -> ReleaseAction {
    if is_recording && held < MIN_PTT_HOLD {
        ReleaseAction::DiscardTooShort
    } else {
        ReleaseAction::Process
    }
}

/// Cached shortcuts so the Carbon/hotkey callback never waits on the engine mutex
/// (Whisper can hold that lock for tens of seconds).
pub fn remember_hotkeys(talk: String, copy: String, paste: String) {
    if let Ok(mut slot) = BOUND_HOTKEYS.lock() {
        *slot = (talk, copy, paste);
    }
}

pub fn bound_hotkeys() -> (String, String, String) {
    BOUND_HOTKEYS
        .lock()
        .ok()
        .map(|g| g.clone())
        .filter(|(talk, _, _)| !talk.is_empty())
        .unwrap_or_else(|| {
            (
                "Control+Shift+Space".into(),
                "Command+Control+C".into(),
                "Command+Control+V".into(),
            )
        })
}

pub fn remember_microphone(name: Option<String>) {
    if let Ok(mut slot) = MICROPHONE.lock() {
        *slot = name;
    }
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
        },
    );
}

fn sync_tray(app: &AppHandle, phase: &str) {
    if let Some(tray) = app.tray_by_id("localflow") {
        let tooltip = match phase {
            "recording" => "LocalFlow — recording",
            "processing" => "LocalFlow — processing",
            "pressed" => "LocalFlow — recording",
            _ => "LocalFlow",
        };
        let _ = tray.set_tooltip(Some(tooltip));
        let mark = if phase == "recording" || phase == "pressed" {
            "●"
        } else {
            ""
        };
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
            if engine
                .try_lock()
                .map(|eng| eng.settings.sound_cues)
                .unwrap_or(true)
            {
                crate::cues::play_start();
            }
            show_bar(app, engine);
            emit_state(
                app,
                DictationState {
                    phase: "recording".into(),
                    message: "Listening…".into(),
                    transcript: None,
                    raw_transcript: None,
                    duration_ms: 0,
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
                    message: err.to_string(),
                    transcript: last_processed(engine),
                    raw_transcript: last_raw(engine),
                    duration_ms: 0,
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
    match classify_release(held, capture.is_recording()) {
        ReleaseAction::DiscardTooShort => discard_short_hold(app, engine, capture),
        ReleaseAction::Process => finish_recording(app, engine, capture),
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
        },
    );
    if let Some(window) = app.get_webview_window("bar") {
        let _ = window.hide();
    }
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
        let mut pcm = crate::vad::trim_silence(&audio::to_whisper_pcm(&captured), 16_000);
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
        let (stt_path, lang, pid, app_name, delay_ms, timeout_ms, sounds) = match engine.lock() {
            Ok(eng) => (
                eng.ready_model_path("stt"),
                eng.settings.stt_language.clone(),
                eng.insert_target_pid,
                eng.insert_target_app.clone(),
                eng.settings.insert_delay_ms,
                eng.settings.postprocess_timeout_ms,
                eng.settings.sound_cues,
            ),
            Err(_) => {
                fail(&app, &engine, "engine lock poisoned", duration_ms);
                return;
            }
        };
        let raw = match NativeStt.transcribe(&pcm, stt_path.as_deref(), &lang) {
            Ok(text) => crate::sanitize::strip_model_tags(&text),
            Err(err) => {
                fail(&app, &engine, &err.to_string(), duration_ms);
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
                    crate::cues::play_end();
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
                emit_state(
                    &app,
                    DictationState {
                        phase: if inserted {
                            "done".into()
                        } else {
                            "error".into()
                        },
                        message: if output.final_text.is_empty() {
                            format!("Processed {duration_ms} ms of audio. No text to insert.")
                        } else if inserted {
                            format!("Inserted: {}", output.final_text)
                        } else {
                            format!(
                                "Text ready but insert failed. Copy from Last Transcript: {}",
                                output.final_text
                            )
                        },
                        transcript: Some(output.final_text),
                        raw_transcript: Some(output.raw_transcript),
                        duration_ms,
                    },
                );
                hide_bar_later(&app);
            }
            Err(err) => fail(&app, &engine, &err.to_string(), duration_ms),
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
        DictationState {
            phase: "error".into(),
            message: message.to_string(),
            transcript: last_processed(engine),
            raw_transcript: last_raw(engine),
            duration_ms,
        },
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
    fn default_talk_hotkey_cache_is_control_shift_space() {
        let (talk, copy, paste) = bound_hotkeys();
        assert!(talk.to_lowercase().contains("space") || talk == "Control+Shift+Space");
        assert!(!copy.is_empty());
        assert!(!paste.is_empty());
    }
}
