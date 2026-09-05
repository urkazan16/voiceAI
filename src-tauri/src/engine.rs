use crate::catalog::{ModelCatalog, ModelRecord};
use crate::config::{export_config, import_config, AppSettings, ExportedConfig, Profile};
use crate::db::Store;
use crate::dictionary::Dictionary;
use crate::error::{LfError, LfResult};
use crate::history::HistoryItem;
use crate::injection::{ClipboardInjector, MemoryInjector, TextInjector};
use crate::integrity::activate_model;
use crate::integrity::looks_installed;
use crate::llm::{LanguageModel, NativeLlm, ScriptedLlm};
use crate::paths::DataPaths;
use crate::personalization::PersonalizationState;
use crate::pipeline::{PipelineMode, PipelineOutput, PipelineSnapshot, PipelineState};
use crate::stt::{NativeStt, ScriptedStt, SpeechToText};
use chrono::Utc;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

pub struct AppEngine {
    pub paths: DataPaths,
    pub catalog: ModelCatalog,
    pub settings: AppSettings,
    pub profiles: Vec<Profile>,
    pub dictionary: Dictionary,
    pub personalization: PersonalizationState,
    pub snapshot: PipelineSnapshot,
    pub store: Store,
    pub last_output: Option<PipelineOutput>,
    pub hotkey_registered: Option<String>,
    pub hotkey_error: Option<String>,
    pub insert_target_pid: Option<i32>,
    pub session_text: String,
}

impl AppEngine {
    pub fn open(paths: DataPaths) -> LfResult<Self> {
        paths.ensure()?;
        let store = Store::open(&paths)?;
        let catalog = ModelCatalog::embedded()?;
        let mut engine = Self {
            paths,
            catalog,
            settings: AppSettings::default(),
            profiles: vec![Profile {
                id: "default".into(),
                name: "Default".into(),
                mode: PipelineMode::Normal,
                dictionary_ids: vec![],
            }],
            dictionary: Dictionary::default(),
            personalization: PersonalizationState::default(),
            snapshot: PipelineSnapshot::default(),
            store,
            last_output: None,
            hotkey_registered: None,
            hotkey_error: None,
            insert_target_pid: None,
            session_text: String::new(),
        };
        engine.load_persisted();
        if let Ok(Some(json)) = engine.store.get_kv("last_transcript") {
            if let Ok(output) = serde_json::from_str::<PipelineOutput>(&json) {
                engine.last_output = Some(output);
            }
        }
        Ok(engine)
    }

    fn load_persisted(&mut self) {
        if let Ok(Some(json)) = self.store.get_kv("config") {
            if let Ok(cfg) = import_config(&json, &self.catalog) {
                self.apply_imported(cfg);
            }
        }
    }

    pub fn persist(&self) -> LfResult<()> {
        let exported = export_config(
            &self.settings,
            &self.profiles,
            &self.dictionary,
            &self.personalization,
        );
        self.store
            .put_kv("config", &serde_json::to_string_pretty(&exported)?)?;
        Ok(())
    }

    pub fn apply_imported(&mut self, cfg: ExportedConfig) {
        self.settings = cfg.settings;
        self.profiles = cfg.profiles;
        self.dictionary = cfg.dictionary;
        self.personalization = cfg.personalization;
        self.settings.active_stt_model = cfg.models.active_stt_model;
        self.settings.active_llm_model = cfg.models.active_llm_model;
    }

    pub fn export_json(&self) -> LfResult<String> {
        let exported = export_config(
            &self.settings,
            &self.profiles,
            &self.dictionary,
            &self.personalization,
        );
        Ok(serde_json::to_string_pretty(&exported)?)
    }

    pub fn import_json(&mut self, json: &str) -> LfResult<()> {
        let cfg = import_config(json, &self.catalog)?;
        self.apply_imported(cfg);
        self.persist()?;
        Ok(())
    }

    pub fn model_path(&self, record: &ModelRecord) -> PathBuf {
        self.paths.model_file(&record.kind, &record.filename)
    }

    pub fn verified_model(&self, model_id: &str) -> LfResult<PathBuf> {
        let record = self.catalog.get(model_id)?;
        let path = self.model_path(record);
        activate_model(&path, record)?;
        Ok(path)
    }

    fn ready_model_path(&self, kind: &str) -> Option<PathBuf> {
        let id = match kind {
            "llm" => self.settings.active_llm_model.as_ref()?,
            _ => self.settings.active_stt_model.as_ref()?,
        };
        let record = self.catalog.get(id).ok()?;
        let path = self.model_path(record);
        if looks_installed(&path, record) {
            Some(path)
        } else {
            None
        }
    }

    pub fn model_status(&self, model_id: &str) -> LfResult<crate::download::ModelInstallStatus> {
        let record = self.catalog.get(model_id)?;
        let path = self.model_path(record);
        let mut status = crate::download::inspect_install(record, &path);
        status.active = match record.kind.as_str() {
            "llm" => self.settings.active_llm_model.as_deref() == Some(model_id),
            _ => self.settings.active_stt_model.as_deref() == Some(model_id),
        };
        Ok(status)
    }

    pub fn mark_active(&mut self, model_id: &str) -> LfResult<()> {
        let kind = self.catalog.get(model_id)?.kind.clone();
        match kind.as_str() {
            "llm" => self.settings.active_llm_model = Some(model_id.to_string()),
            _ => self.settings.active_stt_model = Some(model_id.to_string()),
        }
        self.persist()?;
        Ok(())
    }

    pub fn activate_installed(&mut self, model_id: &str) -> LfResult<PathBuf> {
        let path = self.verified_model(model_id)?;
        self.mark_active(model_id)?;
        Ok(path)
    }

    pub fn process_captured_audio(&mut self, pcm_16k: &[f32]) -> LfResult<PipelineOutput> {
        let pcm = crate::vad::trim_silence(pcm_16k, 16_000);
        self.run_text_pipeline(
            "",
            &NativeStt,
            &NativeLlm,
            &ClipboardInjector {
                target_pid: self.insert_target_pid,
            },
            &pcm,
        )
    }

    pub fn run_text_pipeline(
        &mut self,
        transcript: &str,
        stt: &dyn SpeechToText,
        llm: &dyn LanguageModel,
        injector: &dyn TextInjector,
        pcm: &[f32],
    ) -> LfResult<PipelineOutput> {
        self.snapshot.reset();
        self.snapshot.mode = self.settings.mode;
        self.snapshot.transition(PipelineState::Recording)?;
        self.snapshot.transition(PipelineState::ProcessingStt)?;
        if crate::dictation::is_cancelled() {
            return Err(LfError::Other("cancelled".into()));
        }
        let raw = if transcript.is_empty() {
            stt.transcribe(pcm, self.ready_model_path("stt").as_deref())?
        } else {
            transcript.to_string()
        };
        if crate::dictation::is_cancelled() {
            return Err(LfError::Other("cancelled".into()));
        }
        self.snapshot.transition(PipelineState::Dictionary)?;
        let dictionary_text = self.dictionary.apply(&raw);
        self.snapshot.transition(PipelineState::Backtrack)?;
        let backtrack_text = if self.settings.mode == PipelineMode::Raw {
            dictionary_text.clone()
        } else {
            crate::backtrack::apply(&dictionary_text, &self.session_text)
        };
        self.snapshot.transition(PipelineState::Formatting)?;
        let formatted_text = crate::format::format_smart(self.settings.mode, &backtrack_text);
        self.snapshot.transition(PipelineState::Personalization)?;
        let personalized_text = self.personalization.apply(&formatted_text);
        self.snapshot.transition(PipelineState::Llm)?;
        let llm_text = match self.settings.mode {
            PipelineMode::Raw | PipelineMode::Normal => personalized_text.clone(),
            PipelineMode::Professional | PipelineMode::Code => {
                match llm.generate(
                    &personalized_text,
                    self.settings.mode,
                    self.ready_model_path("llm").as_deref(),
                ) {
                    Ok(text) => text,
                    Err(LfError::RuntimeUnsupported(_)) | Err(LfError::ModelMissing(_)) => {
                        personalized_text.clone()
                    }
                    Err(other) => return Err(other),
                }
            }
        };
        self.snapshot.transition(PipelineState::Validate)?;
        let final_text = llm_text.trim().to_string();
        if self.settings.mode == PipelineMode::Code {
            debug_assert!(crate::llm::assert_non_execution_policy());
        }
        self.snapshot.transition(PipelineState::Injecting)?;
        let mut insert_ok = true;
        let mut insert_err = None;
        if !final_text.is_empty() && !crate::dictation::is_cancelled() {
            if let Err(err) = injector.insert_text(&final_text, self.settings.restore_clipboard) {
                insert_ok = false;
                insert_err = Some(err);
            }
        }
        if crate::dictation::is_cancelled() {
            return Err(LfError::Other("cancelled".into()));
        }
        self.snapshot.transition(PipelineState::Completed)?;
        let output = PipelineOutput {
            raw_transcript: raw.clone(),
            dictionary_text,
            backtrack_text,
            formatted_text,
            personalized_text,
            final_text: final_text.clone(),
            mode: self.settings.mode,
            insert_ok,
        };
        self.last_output = Some(output.clone());
        if !final_text.is_empty() {
            self.session_text = final_text.clone();
        }
        let _ = self.store.put_kv(
            "last_transcript",
            &serde_json::to_string(&output).unwrap_or_default(),
        );
        let item = HistoryItem {
            id: Uuid::new_v4().to_string(),
            created_at: Utc::now().to_rfc3339(),
            mode: format!("{:?}", self.settings.mode).to_lowercase(),
            transcript: raw,
            output: final_text,
        };
        self.store.insert_history(&item)?;
        self.snapshot.transition(PipelineState::Idle)?;
        if let Some(err) = insert_err {
            return Err(err);
        }
        Ok(output)
    }

    pub fn run_scripted(&mut self, transcript: &str) -> LfResult<PipelineOutput> {
        let stt = ScriptedStt {
            transcript: transcript.to_string(),
        };
        let llm = ScriptedLlm;
        let injector = MemoryInjector::default();
        self.run_text_pipeline(transcript, &stt, &llm, &injector, &[])
    }

    pub fn delete_history(&self) -> LfResult<()> {
        self.store.delete_history()
    }

    pub fn reset_personalization(&mut self) -> LfResult<()> {
        self.personalization.reset();
        self.persist()
    }

    pub fn copy_last_transcript(&self) -> LfResult<String> {
        let text = self
            .last_output
            .as_ref()
            .map(|o| o.final_text.clone())
            .filter(|t| !t.is_empty())
            .ok_or_else(|| LfError::Other("no last transcript".into()))?;
        let mut child = std::process::Command::new("pbcopy")
            .stdin(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| LfError::InjectionFailed(e.to_string()))?;
        if let Some(stdin) = child.stdin.as_mut() {
            use std::io::Write;
            stdin
                .write_all(text.as_bytes())
                .map_err(|e| LfError::InjectionFailed(e.to_string()))?;
        }
        let _ = child.wait();
        Ok(text)
    }

    pub fn paste_last_transcript(&self) -> LfResult<String> {
        let text = self
            .last_output
            .as_ref()
            .map(|o| o.final_text.clone())
            .filter(|t| !t.is_empty())
            .ok_or_else(|| LfError::Other("no last transcript".into()))?;
        crate::injection::ClipboardInjector {
            target_pid: crate::injection::frontmost_unix_id(),
        }
        .insert_text(&text, self.settings.restore_clipboard)?;
        Ok(text)
    }

    pub fn clear_last_transcript(&mut self) -> LfResult<()> {
        self.last_output = None;
        self.store.put_kv("last_transcript", "")?;
        Ok(())
    }
}

pub type SharedEngine = Arc<Mutex<AppEngine>>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dictionary::DictionaryEntry;
    use crate::personalization::CorrectionEvent;
    use tempfile::tempdir;

    fn engine() -> (tempfile::TempDir, AppEngine) {
        let dir = tempdir().unwrap();
        let eng = AppEngine::open(DataPaths::from_override(dir.path().to_path_buf())).unwrap();
        (dir, eng)
    }

    #[test]
    fn pipeline_applies_dictionary_and_personalization() {
        let (_dir, mut eng) = engine();
        eng.dictionary.upsert(DictionaryEntry {
            id: "1".into(),
            source: "жюнит".into(),
            replacement: "JUnit 5".into(),
            case_sensitive: false,
        });
        eng.personalization.record_correction(CorrectionEvent {
            id: "c".into(),
            original: "локалфлоу".into(),
            corrected: "LocalFlow".into(),
            accepted: true,
        });
        let out = eng.run_scripted("жюнит тест для локалфлоу").unwrap();
        assert!(out.final_text.contains("JUnit 5"));
        assert!(out.final_text.contains("LocalFlow"));
        assert_eq!(eng.snapshot.state, PipelineState::Idle);
    }

    #[test]
    fn smart_formatting_benchmarks_120_to_123() {
        let (_dir, mut eng) = engine();
        assert_eq!(
            eng.run_scripted("Давай встретимся в пять, нет, в шесть.")
                .unwrap()
                .final_text,
            "Давай встретимся в шесть."
        );
        assert_eq!(
            eng.run_scripted("Ну короче э-э давай завтра созвонимся.")
                .unwrap()
                .final_text,
            "Давай завтра созвонимся."
        );
        assert_eq!(
            eng.run_scripted("один API тесты два UI тесты три SQL")
                .unwrap()
                .final_text,
            "1. API тесты\n2. UI тесты\n3. SQL"
        );
        assert_eq!(
            eng.run_scripted("Привет запятая как дела вопросительный знак")
                .unwrap()
                .final_text,
            "Привет, как дела?"
        );
    }
}
