use crate::audio::{self, SharedCapture};
use crate::engine::SharedEngine;
use crate::error::LfError;
use crate::pipeline::PipelineState;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::GlobalShortcutExt;

static CANCEL: AtomicBool = AtomicBool::new(false);
static HANDS_FREE: AtomicBool = AtomicBool::new(false);
static PRESS_AT: Mutex<Option<Instant>> = Mutex::new(None);

pub fn cancel_flag() -> &'static AtomicBool {
    &CANCEL
}

pub fn is_cancelled() -> bool {
    CANCEL.load(Ordering::Relaxed)
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
    for label in ["main", "bar"] {
        if let Some(window) = app.get_webview_window(label) {
            let _ = window.emit("dictation-state", &state);
        }
    }
}

pub fn show_bar(app: &AppHandle, engine: &SharedEngine) {
    let enabled = engine
        .lock()
        .map(|eng| eng.settings.show_flow_bar)
        .unwrap_or(true);
    if !enabled {
        return;
    }
    if let Some(window) = app.get_webview_window("bar") {
        let _ = window.show();
    }
}

pub fn hide_bar_later(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(2400));
        if CANCEL.load(Ordering::Relaxed) || HANDS_FREE.load(Ordering::Relaxed) {
            return;
        }
        if let Some(window) = app.get_webview_window("bar") {
            let _ = window.hide();
        }
        let _ = app.global_shortcut().unregister("Escape");
    });
}

pub fn on_hotkey_pressed(app: &AppHandle, engine: &SharedEngine, capture: &SharedCapture) {
    if capture.is_recording() || HANDS_FREE.load(Ordering::Relaxed) {
        return;
    }
    CANCEL.store(false, Ordering::Relaxed);
    HANDS_FREE.store(false, Ordering::Relaxed);
    if let Ok(mut slot) = PRESS_AT.lock() {
        *slot = Some(Instant::now());
    }
    let mic = engine
        .lock()
        .ok()
        .and_then(|eng| eng.settings.microphone_name.clone());
    let target_pid = crate::injection::frontmost_unix_id();
    if let Ok(mut eng) = engine.lock() {
        eng.insert_target_pid = target_pid;
        eng.snapshot.reset();
        let _ = eng.snapshot.transition(PipelineState::Recording);
    }
    let _ = app.global_shortcut().register("Escape");
    match capture.start(mic) {
        Ok(()) => {
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
    if held < Duration::from_millis(320) && capture.is_recording() {
        HANDS_FREE.store(true, Ordering::Relaxed);
        emit_state(
            app,
            DictationState {
                phase: "recording".into(),
                message: "Hands-free listening… Stop or pause to finish.".into(),
                transcript: None,
                raw_transcript: None,
                duration_ms: 0,
            },
        );
        let app = app.clone();
        let engine = engine.clone();
        let capture = capture.clone();
        std::thread::spawn(move || wait_for_vad_stop(&app, &engine, &capture));
        return;
    }
    finish_recording(app, engine, capture);
}

pub fn stop_and_process(app: &AppHandle, engine: &SharedEngine, capture: &SharedCapture) {
    HANDS_FREE.store(false, Ordering::Relaxed);
    finish_recording(app, engine, capture);
}

pub fn cancel(app: &AppHandle, engine: &SharedEngine, capture: &SharedCapture) {
    CANCEL.store(true, Ordering::Relaxed);
    HANDS_FREE.store(false, Ordering::Relaxed);
    let _ = capture.stop();
    if let Ok(mut eng) = engine.lock() {
        eng.snapshot.reset();
    }
    let _ = app.global_shortcut().unregister("Escape");
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

fn wait_for_vad_stop(app: &AppHandle, engine: &SharedEngine, capture: &SharedCapture) {
    let mut heard = false;
    loop {
        if CANCEL.load(Ordering::Relaxed) {
            return;
        }
        if !capture.is_recording() {
            return;
        }
        if let Some(peek) = capture.peek() {
            let pcm = audio::to_whisper_pcm(&peek);
            if crate::vad::had_speech(&pcm, 16_000) {
                heard = true;
            }
            if heard && crate::vad::trailing_silence_ms(&pcm, 16_000) >= 1_600 {
                HANDS_FREE.store(false, Ordering::Relaxed);
                finish_recording(app, engine, capture);
                return;
            }
        }
        std::thread::sleep(Duration::from_millis(60));
    }
}

fn finish_recording(app: &AppHandle, engine: &SharedEngine, capture: &SharedCapture) {
    if CANCEL.load(Ordering::Relaxed) {
        let _ = capture.stop();
        return;
    }
    let Some(captured) = capture.stop() else {
        return;
    };
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
        let pcm = crate::vad::trim_silence(&audio::to_whisper_pcm(&captured), 16_000);
        if pcm.len() < 1_600 {
            fail(
                &app,
                &engine,
                "Recording too short. Hold the hotkey, speak, then release.",
                duration_ms,
            );
            return;
        }
        let result = match engine.lock() {
            Ok(mut eng) => eng.process_captured_audio(&pcm),
            Err(_) => Err(LfError::Other("engine lock poisoned".into())),
        };
        if CANCEL.load(Ordering::Relaxed)
            || matches!(&result, Err(LfError::Other(m)) if m == "cancelled")
        {
            emit_cancelled(&app, &engine);
            return;
        }
        match result {
            Ok(output) => {
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
    let _ = app.global_shortcut().unregister("Escape");
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
