use crate::audio::{self, AudioDevice};
use crate::build_info::{self, BuildInfo};
use crate::catalog::ModelRecord;
use crate::config::AppSettings;
use crate::dictionary::DictionaryEntry;
use crate::engine::SharedEngine;
use crate::error::LfError;
use crate::history::HistoryItem;
use crate::pipeline::{PipelineOutput, PipelineSnapshot};
use serde::Serialize;

#[derive(Serialize)]
pub struct CommandError {
    pub code: String,
    pub message: String,
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
pub fn verify_model(
    engine: tauri::State<SharedEngine>,
    model_id: String,
) -> Result<String, CommandError> {
    let path = lock(&engine)?.verified_model(&model_id)?;
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
