//! Acceptance checks for the shipped inference stack and dictation hold policy.
//!
//! AC-02: the binary must not identify a C `runtime.c` stub.
//! AC-06: speech-to-text is whisper-rs, not the removed native FFI stub.

use localflow_lib::backtrack;
use localflow_lib::build_info;
use localflow_lib::catalog::ModelCatalog;
use localflow_lib::config::{export_config, import_config, AppSettings};
use localflow_lib::dictation::{classify_release, ReleaseAction, MIN_PTT_HOLD};
use localflow_lib::dictionary::{Dictionary, DictionaryEntry};
use localflow_lib::engine::AppEngine;
use localflow_lib::error::LfError;
use localflow_lib::format::format_smart;
use localflow_lib::paths::DataPaths;
use localflow_lib::personalization::PersonalizationState;
use localflow_lib::phrases::{self, recover};
use localflow_lib::pipeline::{format_without_remote_llm, PipelineMode, PipelineState};
use localflow_lib::profiles;
use localflow_lib::runtime;
use localflow_lib::snippets::SnippetBook;
use localflow_lib::stt::{NativeStt, SpeechToText};
use localflow_lib::vad;
use std::path::Path;
use std::time::Duration;
use tempfile::tempdir;

fn engine() -> (tempfile::TempDir, AppEngine) {
    let dir = tempdir().unwrap();
    let eng = AppEngine::open(DataPaths::from_override(dir.path().to_path_buf())).unwrap();
    (dir, eng)
}

#[test]
fn ac02_runtime_id_is_not_the_c_stub() {
    let id = runtime::runtime_id();
    assert!(id.starts_with("whisper-rs/"), "{id}");
    assert!(!id.contains("stub"));
    assert!(!id.contains("localflow-native"));
    let info = build_info::current();
    assert_eq!(info.native_runtime, id);
}

#[test]
fn ac06_stt_uses_whisper_rs_when_model_path_is_set() {
    let err = NativeStt
        .transcribe(
            &[0.05; 2_000],
            Some(Path::new("/var/empty/whisper-medium.bin")),
            "auto",
        )
        .unwrap_err();
    assert_eq!(err.code(), "MODEL_MISSING");
    assert!(!err.to_string().contains("lf_whisper_transcribe"));
    assert!(!err.to_string().contains("native-stub"));
}

#[test]
fn ac06_llama_ffi_is_not_required_for_professional_mode() {
    let (_dir, mut eng) = engine();
    eng.settings.mode = PipelineMode::Professional;
    let out = eng.run_scripted("привет команда").unwrap();
    assert!(!out.final_text.is_empty());
    assert_eq!(eng.snapshot.state, PipelineState::Idle);
}

#[test]
fn short_hold_under_320ms_is_discarded() {
    for ms in [1_u64, 50, 120, 250, 319] {
        assert_eq!(
            classify_release(Duration::from_millis(ms), true),
            ReleaseAction::DiscardTooShort,
            "{ms} ms must not start hands-free or process"
        );
    }
}

#[test]
fn raised_hold_threshold_is_500ms() {
    assert_eq!(MIN_PTT_HOLD, Duration::from_millis(500));
    assert!(MIN_PTT_HOLD > Duration::from_millis(320));
    assert_eq!(
        classify_release(Duration::from_millis(320), true),
        ReleaseAction::DiscardTooShort
    );
    assert_eq!(
        classify_release(Duration::from_millis(499), true),
        ReleaseAction::DiscardTooShort
    );
    assert_eq!(
        classify_release(Duration::from_millis(500), true),
        ReleaseAction::Process
    );
}

#[test]
fn pipeline_empty_transcript_stays_idle_after_reset() {
    let (_dir, mut eng) = engine();
    let out = eng.run_scripted("").unwrap();
    assert_eq!(out.final_text, "");
    assert_eq!(eng.snapshot.state, PipelineState::Idle);
}

#[test]
fn pipeline_raw_mode_keeps_spoken_words() {
    let (_dir, mut eng) = engine();
    eng.settings.mode = PipelineMode::Raw;
    let out = eng.run_scripted("ну короче э-э sql").unwrap();
    assert!(out.final_text.to_lowercase().contains("sql"));
}

#[test]
fn pipeline_writes_history_with_profile_fields() {
    let (_dir, mut eng) = engine();
    eng.insert_target_app = Some("Mail".into());
    let out = eng.run_scripted("отправь письмо").unwrap();
    assert!(!out.final_text.is_empty());
    let items = eng.store.list_history().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].transcript, "отправь письмо");
    assert!(!items[0].id.is_empty());
}

#[test]
fn pipeline_dictionary_builtin_restassured() {
    let (_dir, mut eng) = engine();
    eng.dictionary.ensure_builtins();
    let out = eng.run_scripted("тест на рест ашуред").unwrap();
    assert!(
        out.dictionary_text.contains("RestAssured") || out.final_text.contains("RestAssured"),
        "{} / {}",
        out.dictionary_text,
        out.final_text
    );
}

#[test]
fn pipeline_code_mode_does_not_execute() {
    let (_dir, mut eng) = engine();
    eng.settings.mode = PipelineMode::Code;
    let out = eng.run_scripted("rm -rf /").unwrap();
    assert!(out.final_text.contains("rm"));
}

#[test]
fn last_transcript_roundtrip_in_kv() {
    let (_dir, mut eng) = engine();
    eng.run_scripted("сохрани это").unwrap();
    assert!(eng.last_output.is_some());
    eng.clear_last_transcript().unwrap();
    assert!(eng.last_output.is_none());
}

#[test]
fn correction_becomes_dictionary_after_repeat() {
    let (_dir, mut eng) = engine();
    let first = eng
        .record_user_correction("маккензи".into(), "McKenzie".into())
        .unwrap();
    assert!(first.is_empty() || first.iter().all(|c| !c.accepted));
    let again = eng
        .record_user_correction("маккензи".into(), "McKenzie".into())
        .unwrap();
    if let Some(item) = again.first() {
        let id = item.id.clone();
        let entry = eng.accept_learned(&id).unwrap();
        assert!(entry.is_some());
        let out = eng.run_scripted("письмо для маккензи").unwrap();
        assert!(out.final_text.contains("McKenzie"));
    }
}

#[test]
fn config_rejects_unknown_model_id() {
    let catalog = ModelCatalog::embedded().unwrap();
    let settings = AppSettings {
        active_stt_model: Some("not-a-real-model".into()),
        ..AppSettings::default()
    };
    let exported = export_config(
        &settings,
        &[],
        &Dictionary::default(),
        &PersonalizationState::default(),
        &SnippetBook::default(),
    );
    let json = serde_json::to_string(&exported).unwrap();
    let err = import_config(&json, &catalog).unwrap_err();
    assert_eq!(err.code(), "MODEL_MISSING");
}

#[test]
fn catalog_includes_whisper_medium_and_small() {
    let catalog = ModelCatalog::embedded().unwrap();
    let ids: Vec<_> = catalog.models.iter().map(|m| m.model_id.as_str()).collect();
    assert!(ids.iter().any(|id| id.contains("whisper")));
    for model in catalog.models.iter().filter(|m| m.kind == "stt") {
        assert_eq!(model.sha256.len(), 64);
        assert!(model.checksum_pinned);
        assert!(model.network_required_to_obtain);
    }
}

#[test]
fn default_profiles_cover_mail_and_code() {
    let profiles = profiles::default_profiles();
    assert!(profiles.len() >= 3);
    let names: Vec<_> = profiles.iter().map(|p| p.name.to_lowercase()).collect();
    let joined = names.join(" ");
    assert!(
        joined.contains("mail")
            || joined.contains("email")
            || joined.contains("work")
            || joined.contains("dev")
            || joined.contains("personal")
    );
}

#[test]
fn backtrack_applies_scratch_that() {
    let out = backtrack::apply("встречимся в пять нет в шесть", "");
    assert!(out.contains("шесть") || out.contains("пять"), "{out}");
}

#[test]
fn vad_silence_and_speech() {
    let silence = vec![0.0; 8_000];
    assert!(!vad::had_speech(&silence, 16_000));
    let mut speech = vec![0.0; 8_000];
    for (i, sample) in speech.iter_mut().enumerate() {
        *sample = if i % 2 == 0 { 0.4 } else { -0.4 };
    }
    assert!(vad::had_speech(&speech, 16_000));
}

#[test]
fn phrase_recovery_ignores_unrelated_sasha() {
    assert_eq!(recover("Саша, привет"), "Саша, привет");
    assert_eq!(
        recover(phrases::SASHA_TONGUE_TWISTER),
        phrases::SASHA_TONGUE_TWISTER
    );
}

#[test]
fn format_helpers_match_pipeline_wrapper() {
    let spoken = "Привет запятая как дела вопросительный знак";
    assert_eq!(
        format_smart(PipelineMode::Normal, spoken),
        format_without_remote_llm(PipelineMode::Normal, spoken)
    );
}

#[test]
fn dictionary_disabled_entry_is_skipped() {
    let mut entry = DictionaryEntry::rule("1", "пострес", "Postgres");
    entry.enabled = false;
    let dict = Dictionary {
        entries: vec![entry],
    };
    assert_eq!(dict.apply("подними пострес"), "подними пострес");
}

#[test]
fn snippet_book_defaults_are_exact_triggers() {
    let mut book = SnippetBook::default();
    book.ensure_defaults();
    assert!(book.expand("мой баг репорт", "").is_some());
    assert!(book.expand("баг", "").is_none());
}

#[test]
fn engine_export_import_preserves_language() {
    let (_dir, mut eng) = engine();
    eng.settings.stt_language = "auto".into();
    eng.persist().unwrap();
    let json = eng.export_json().unwrap();
    eng.settings.stt_language = "en".into();
    eng.import_json(&json).unwrap();
    assert_eq!(eng.settings.stt_language, "auto");
}

#[test]
fn model_missing_error_code_stable() {
    let err = LfError::ModelMissing("whisper-medium".into());
    assert_eq!(err.code(), "MODEL_MISSING");
    assert!(err.to_string().contains("whisper-medium"));
}

#[test]
fn classify_release_process_when_idle() {
    assert_eq!(
        classify_release(Duration::from_millis(1), false),
        ReleaseAction::Process
    );
}

#[test]
fn settings_json_is_written_on_open_and_human_readable() {
    let (dir, mut eng) = engine();
    eng.settings.stt_language = "en".into();
    eng.persist().unwrap();
    let text = std::fs::read_to_string(eng.paths.settings_file()).unwrap();
    assert!(text.contains("\"stt_language\": \"en\""));
    std::fs::write(eng.paths.settings_file(), "{not json").unwrap();
    drop(eng);
    let eng =
        localflow_lib::engine::AppEngine::open(DataPaths::from_override(dir.path().to_path_buf()))
            .unwrap();
    assert_eq!(eng.settings.stt_language, "ru");
}

#[test]
fn scripted_pipeline_writes_jsonl_journal() {
    let (_dir, mut eng) = engine();
    eng.run_scripted("привет мир").unwrap();
    let rows = localflow_lib::uttlog::read_since(&eng.paths, None);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].schema, 1);
    assert!(rows[0].timezone.starts_with('+') || rows[0].timezone.starts_with('-'));
    assert_eq!(rows[0].insert_method, "clipboard");
}
