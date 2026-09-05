use localflow_lib::config::{export_config, import_config, AppSettings};
use localflow_lib::dictionary::{Dictionary, DictionaryEntry};
use localflow_lib::engine::AppEngine;
use localflow_lib::paths::DataPaths;
use localflow_lib::personalization::PersonalizationState;
use localflow_lib::pipeline::PipelineState;
use tempfile::tempdir;

#[test]
fn integration_pipeline_dictionary_and_history() {
    let dir = tempdir().unwrap();
    let mut engine = AppEngine::open(DataPaths::from_override(dir.path().to_path_buf())).unwrap();
    engine
        .dictionary
        .upsert(DictionaryEntry::rule("sql", "селект", "SELECT"));
    let output = engine.run_scripted("сделай селект из users").unwrap();
    assert!(output.dictionary_text.contains("SELECT"));
    assert_eq!(engine.snapshot.state, PipelineState::Idle);
    assert_eq!(engine.store.list_history().unwrap().len(), 1);
}

#[test]
fn integration_config_roundtrip_and_history_delete() {
    let dir = tempdir().unwrap();
    let mut engine = AppEngine::open(DataPaths::from_override(dir.path().to_path_buf())).unwrap();
    engine.settings.hotkey = "Alt+Shift+Space".into();
    let json = engine.export_json().unwrap();
    engine.settings.hotkey = "Alt+Space".into();
    engine.import_json(&json).unwrap();
    assert_eq!(engine.settings.hotkey, "Alt+Shift+Space");
    engine.run_scripted("ping").unwrap();
    engine.delete_history().unwrap();
    assert!(engine.store.list_history().unwrap().is_empty());
}

#[test]
fn integration_exported_config_validates_against_catalog() {
    let settings = AppSettings::default();
    let exported = export_config(
        &settings,
        &[],
        &Dictionary::default(),
        &PersonalizationState::default(),
        &localflow_lib::snippets::SnippetBook::default(),
    );
    let json = serde_json::to_string(&exported).unwrap();
    let catalog = localflow_lib::catalog::ModelCatalog::embedded().unwrap();
    import_config(&json, &catalog).unwrap();
}

#[test]
fn integration_scripted_stt_history_and_clear() {
    let dir = tempdir().unwrap();
    let mut engine = AppEngine::open(DataPaths::from_override(dir.path().to_path_buf())).unwrap();
    engine.run_scripted("первая фраза").unwrap();
    engine.run_scripted("вторая фраза").unwrap();
    assert_eq!(engine.store.list_history().unwrap().len(), 2);
    engine.clear_last_transcript().unwrap();
    assert!(engine.last_output.is_none());
    assert_eq!(engine.store.list_history().unwrap().len(), 2);
}

#[test]
fn integration_runtime_is_whisper_rs() {
    let id = localflow_lib::runtime::runtime_id();
    assert!(id.starts_with("whisper-rs/"));
    assert!(!id.contains("stub"));
}
