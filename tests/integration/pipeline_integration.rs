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
fn integration_dictation_pipeline_values_tags_and_spacing() {
    let dir = tempdir().unwrap();
    let mut engine = AppEngine::open(DataPaths::from_override(dir.path().to_path_buf())).unwrap();
    engine.settings.digits_from_speech = true;
    engine.settings.date_format = "DMY".into();
    engine.settings.mode = localflow_lib::pipeline::PipelineMode::Normal;

    let tagged = engine
        .run_scripted("[BLANK_AUDIO] встреча двадцать пять в 15 часов 30 минут 5.3.26")
        .unwrap();
    assert!(
        !tagged.final_text.contains("BLANK"),
        "{}",
        tagged.final_text
    );
    assert!(tagged.final_text.contains("25"), "{}", tagged.final_text);
    assert!(tagged.final_text.contains("15:30"), "{}", tagged.final_text);
    assert!(
        tagged.final_text.contains("05.03.2026"),
        "{}",
        tagged.final_text
    );

    engine.run_scripted("Привет").unwrap();
    engine.run_scripted("мир").unwrap();
    assert!(
        engine.session_text.contains("Привет. Мир"),
        "consecutive utterances need a space; got {}",
        engine.session_text
    );
}
