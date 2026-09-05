use crate::audio::{self, AudioDevice, SharedCapture};
use crate::build_info::{self, BuildInfo};
use crate::catalog::ModelRecord;
use crate::config::AppSettings;
use crate::dictionary::DictionaryEntry;
use crate::download::{self, ModelDownloadProgress, ModelInstallStatus};
use crate::engine::SharedEngine;
use crate::error::LfError;
use crate::history::HistoryItem;
use crate::pipeline::{PipelineOutput, PipelineSnapshot};
use serde::Serialize;
use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, Emitter};

#[derive(Debug, Serialize)]
pub struct CommandError {
    pub code: String,
    pub message: String,
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl From<LfError> for CommandError {
    fn from(value: LfError) -> Self {
        Self {
            code: value.code().to_string(),
            message: value.to_string(),
        }
    }
}

fn lock(
    engine: &SharedEngine,
) -> Result<std::sync::MutexGuard<'_, crate::engine::AppEngine>, CommandError> {
    engine.lock().map_err(|_| CommandError {
        code: "ERROR".into(),
        message: "engine lock poisoned".into(),
    })
}

#[tauri::command]
pub fn get_build_info() -> BuildInfo {
    build_info::current()
}

#[tauri::command]
pub fn get_snapshot(engine: tauri::State<SharedEngine>) -> Result<PipelineSnapshot, CommandError> {
    Ok(lock(&engine)?.snapshot.clone())
}

#[tauri::command]
pub fn get_settings(engine: tauri::State<SharedEngine>) -> Result<AppSettings, CommandError> {
    Ok(lock(&engine)?.settings.clone())
}

#[tauri::command]
pub fn save_settings(
    engine: tauri::State<SharedEngine>,
    settings: AppSettings,
) -> Result<(), CommandError> {
    let mut eng = lock(&engine)?;
    eng.settings = settings;
    eng.persist()?;
    Ok(())
}

#[tauri::command]
pub fn list_models(engine: tauri::State<SharedEngine>) -> Result<Vec<ModelRecord>, CommandError> {
    Ok(lock(&engine)?.catalog.models.clone())
}

#[tauri::command]
pub fn list_microphones() -> Result<Vec<AudioDevice>, CommandError> {
    Ok(audio::list_input_devices()?)
}

#[tauri::command]
pub fn list_dictionary(
    engine: tauri::State<SharedEngine>,
) -> Result<Vec<DictionaryEntry>, CommandError> {
    Ok(lock(&engine)?.dictionary.entries.clone())
}

#[tauri::command]
pub fn upsert_dictionary_entry(
    engine: tauri::State<SharedEngine>,
    entry: DictionaryEntry,
) -> Result<(), CommandError> {
    let mut eng = lock(&engine)?;
    eng.dictionary.upsert(entry);
    eng.persist()?;
    Ok(())
}

#[tauri::command]
pub fn remove_dictionary_entry(
    engine: tauri::State<SharedEngine>,
    id: String,
) -> Result<(), CommandError> {
    let mut eng = lock(&engine)?;
    eng.dictionary.remove(&id);
    eng.persist()?;
    Ok(())
}

#[tauri::command]
pub fn export_configuration(engine: tauri::State<SharedEngine>) -> Result<String, CommandError> {
    Ok(lock(&engine)?.export_json()?)
}

#[tauri::command]
pub fn import_configuration(
    engine: tauri::State<SharedEngine>,
    json: String,
) -> Result<(), CommandError> {
    lock(&engine)?.import_json(&json)?;
    Ok(())
}

#[tauri::command]
pub fn list_history(engine: tauri::State<SharedEngine>) -> Result<Vec<HistoryItem>, CommandError> {
    Ok(lock(&engine)?.store.list_history()?)
}

#[tauri::command]
pub fn delete_history(engine: tauri::State<SharedEngine>) -> Result<(), CommandError> {
    lock(&engine)?.delete_history()?;
    Ok(())
}

#[tauri::command]
pub fn reset_personalization(engine: tauri::State<SharedEngine>) -> Result<(), CommandError> {
    lock(&engine)?.reset_personalization()?;
    Ok(())
}

#[tauri::command]
pub fn process_transcript(
    engine: tauri::State<SharedEngine>,
    transcript: String,
) -> Result<PipelineOutput, CommandError> {
    Ok(lock(&engine)?.run_scripted(&transcript)?)
}

#[tauri::command]
pub fn dictation_stop(
    app: tauri::AppHandle,
    engine: tauri::State<SharedEngine>,
    capture: tauri::State<SharedCapture>,
) -> Result<(), CommandError> {
    crate::dictation::stop_and_process(&app, &engine, &capture);
    Ok(())
}

#[tauri::command]
pub fn dictation_cancel(
    app: tauri::AppHandle,
    engine: tauri::State<SharedEngine>,
    capture: tauri::State<SharedCapture>,
) -> Result<(), CommandError> {
    crate::dictation::cancel(&app, &engine, &capture);
    Ok(())
}

#[tauri::command]
pub fn get_last_transcript(
    engine: tauri::State<SharedEngine>,
) -> Result<Option<PipelineOutput>, CommandError> {
    Ok(lock(&engine)?.last_output.clone())
}

#[tauri::command]
pub fn copy_last_transcript(engine: tauri::State<SharedEngine>) -> Result<String, CommandError> {
    Ok(lock(&engine)?.copy_last_transcript()?)
}

#[tauri::command]
pub fn paste_last_transcript(engine: tauri::State<SharedEngine>) -> Result<String, CommandError> {
    Ok(lock(&engine)?.paste_last_transcript()?)
}

#[tauri::command]
pub fn clear_last_transcript(engine: tauri::State<SharedEngine>) -> Result<(), CommandError> {
    lock(&engine)?.clear_last_transcript()?;
    Ok(())
}

#[tauri::command]
pub fn complete_onboarding(engine: tauri::State<SharedEngine>) -> Result<(), CommandError> {
    let mut eng = lock(&engine)?;
    eng.settings.onboarding_complete = true;
    eng.persist()?;
    Ok(())
}

#[tauri::command]
pub fn privacy_summary() -> PrivacySummary {
    PrivacySummary {
        audio_local: true,
        stt_local: true,
        llm_local: true,
        dictionary_local: true,
        personalization_local: true,
        history_local: true,
        cloud_account_required: false,
        network_operations: vec![
            "model download (user initiated)".into(),
            "optional application update (user initiated)".into(),
        ],
        data_root: "~/Library/Application Support/LocalFlow/".into(),
    }
}

#[derive(Serialize)]
pub struct PrivacySummary {
    pub audio_local: bool,
    pub stt_local: bool,
    pub llm_local: bool,
    pub dictionary_local: bool,
    pub personalization_local: bool,
    pub history_local: bool,
    pub cloud_account_required: bool,
    pub network_operations: Vec<String>,
    pub data_root: String,
}

#[tauri::command]
pub async fn verify_model(
    engine: tauri::State<'_, SharedEngine>,
    model_id: String,
) -> Result<String, CommandError> {
    let (path, record) = {
        let eng = lock(&engine)?;
        let record = eng.catalog.get(&model_id)?.clone();
        (eng.model_path(&record), record)
    };
    let verify_path = path.clone();
    tokio::task::spawn_blocking(move || crate::integrity::activate_model(&verify_path, &record))
        .await
        .map_err(|err| CommandError {
            code: "ERROR".into(),
            message: err.to_string(),
        })?
        .map_err(CommandError::from)?;
    Ok(path.display().to_string())
}

fn inflight_downloads() -> &'static Mutex<HashSet<String>> {
    static LOCK: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(HashSet::new()))
}

#[tauri::command]
pub fn list_model_status(
    engine: tauri::State<SharedEngine>,
) -> Result<Vec<ModelInstallStatus>, CommandError> {
    let mut statuses = {
        let eng = lock(&engine)?;
        eng.catalog
            .models
            .iter()
            .map(|model| eng.model_status(&model.model_id))
            .collect::<Result<Vec<_>, _>>()
            .map_err(CommandError::from)?
    };
    if let Ok(inflight) = inflight_downloads().lock() {
        for status in &mut statuses {
            if inflight.contains(&status.model_id) && status.state != "verified" {
                status.state = "downloading".into();
            }
        }
    }
    Ok(statuses)
}

#[tauri::command]
pub async fn download_model(
    app: AppHandle,
    engine: tauri::State<'_, SharedEngine>,
    model_id: String,
) -> Result<String, CommandError> {
    {
        let mut inflight = inflight_downloads().lock().map_err(|_| CommandError {
            code: "ERROR".into(),
            message: "download lock poisoned".into(),
        })?;
        if !inflight.insert(model_id.clone()) {
            return Err(CommandError {
                code: "ERROR".into(),
                message: format!("{model_id} is already downloading"),
            });
        }
    }

    let result = download_model_inner(app, engine.inner().clone(), model_id.clone()).await;

    if let Ok(mut inflight) = inflight_downloads().lock() {
        inflight.remove(&model_id);
    }
    result
}

async fn download_model_inner(
    app: AppHandle,
    engine: SharedEngine,
    model_id: String,
) -> Result<String, CommandError> {
    let (record, dest) = {
        let eng = lock(&engine)?;
        let record = eng.catalog.get(&model_id)?.clone();
        let dest = eng.model_path(&record);
        (record, dest)
    };

    let app_for_progress = app.clone();
    let progress_id = record.model_id.clone();
    download::download_and_install(&record, &dest, move |progress: ModelDownloadProgress| {
        let _ = app_for_progress.emit("model-download-progress", &progress);
    })
    .await
    .map_err(|err| {
        let _ = app.emit(
            "model-download-progress",
            ModelDownloadProgress {
                model_id: progress_id.clone(),
                phase: "error".into(),
                bytes_downloaded: 0,
                total_bytes: record.size,
            },
        );
        CommandError::from(err)
    })?;

    let path = {
        let mut eng = lock(&engine)?;
        eng.mark_active(&model_id)?;
        let record = eng.catalog.get(&model_id)?.clone();
        eng.model_path(&record)
    };
    Ok(path.display().to_string())
}

#[tauri::command]
pub async fn set_active_model(
    engine: tauri::State<'_, SharedEngine>,
    model_id: String,
) -> Result<String, CommandError> {
    let (path, record) = {
        let eng = lock(&engine)?;
        let record = eng.catalog.get(&model_id)?.clone();
        (eng.model_path(&record), record)
    };
    let verify_path = path.clone();
    tokio::task::spawn_blocking(move || crate::integrity::activate_model(&verify_path, &record))
        .await
        .map_err(|err| CommandError {
            code: "ERROR".into(),
            message: err.to_string(),
        })?
        .map_err(CommandError::from)?;
    lock(&engine)?.mark_active(&model_id)?;
    Ok(path.display().to_string())
}

#[derive(Serialize)]
pub struct HotkeyStatus {
    pub requested: String,
    pub registered: Option<String>,
    pub error: Option<String>,
}

#[tauri::command]
pub fn get_hotkey_status(engine: tauri::State<SharedEngine>) -> Result<HotkeyStatus, CommandError> {
    let eng = lock(&engine)?;
    Ok(HotkeyStatus {
        requested: eng.settings.hotkey.clone(),
        registered: eng.hotkey_registered.clone(),
        error: eng.hotkey_error.clone(),
    })
}
