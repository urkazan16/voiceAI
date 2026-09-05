use crate::audio::{self, SharedCapture};
use crate::engine::SharedEngine;
use crate::error::LfError;
use crate::pipeline::PipelineState;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

#[derive(Clone, Serialize)]
pub struct DictationState {
    pub phase: String,
    pub message: String,
    pub transcript: Option<String>,
    pub duration_ms: u64,
}

pub fn emit_state(app: &AppHandle, state: DictationState) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.emit("dictation-state", &state);
    }
}

pub fn on_hotkey_pressed(app: &AppHandle, engine: &SharedEngine, capture: &SharedCapture) {
    if capture.is_recording() {
        return;
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
    match capture.start(mic) {
        Ok(()) => {
            emit_state(
                app,
                DictationState {
                    phase: "recording".into(),
                    message: "Recording… keep holding, then release to process.".into(),
                    transcript: None,
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
                    transcript: None,
                    duration_ms: 0,
                },
            );
        }
    }
}

pub fn on_hotkey_released(app: &AppHandle, engine: &SharedEngine, capture: &SharedCapture) {
    let Some(captured) = capture.stop() else {
        return;
    };
    emit_state(
        app,
        DictationState {
            phase: "processing".into(),
            message: "Processing recording…".into(),
            transcript: None,
            duration_ms: 0,
        },
    );
    let app = app.clone();
    let engine = engine.clone();
    std::thread::spawn(move || {
        crate::injection::prepare_keyboard_for_insert();
        let duration_ms = audio::duration_ms(&captured);
        let pcm = audio::to_whisper_pcm(&captured);
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
        match result {
            Ok(output) => {
                emit_state(
                    &app,
                    DictationState {
                        phase: "done".into(),
                        message: if output.final_text.is_empty() {
                            format!("Processed {duration_ms} ms of audio. No text to insert.")
                        } else {
                            format!("Inserted: {}", output.final_text)
                        },
                        transcript: Some(output.final_text),
                        duration_ms,
                    },
                );
            }
            Err(err) => fail(&app, &engine, &err.to_string(), duration_ms),
        }
    });
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
            transcript: None,
            duration_ms,
        },
    );
}
