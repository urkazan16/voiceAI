use crate::catalog::{ModelCatalog, ModelRecord};
use crate::config::{export_config, import_config, AppSettings, ExportedConfig, Profile};
use crate::db::Store;
use crate::dictionary::Dictionary;
use crate::error::{LfError, LfResult};
use crate::history::HistoryItem;
use crate::injection::{MemoryInjector, TextInjector};
use crate::integrity::activate_model;
use crate::llm::{LanguageModel, ScriptedLlm};
use crate::paths::DataPaths;
use crate::personalization::PersonalizationState;
use crate::pipeline::{
    format_without_remote_llm, PipelineMode, PipelineOutput, PipelineSnapshot, PipelineState,
};
use crate::stt::{ScriptedStt, SpeechToText};
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
        };
        engine.load_persisted();
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

    pub fn activate_installed(&mut self, model_id: &str) -> LfResult<PathBuf> {
        let path = self.verified_model(model_id)?;
        let kind = self.catalog.get(model_id)?.kind.clone();
        match kind.as_str() {
            "llm" => self.settings.active_llm_model = Some(model_id.to_string()),
            _ => self.settings.active_stt_model = Some(model_id.to_string()),
        }
        self.persist()?;
        Ok(path)
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
        let raw = if transcript.is_empty() {
            let model_path = match &self.settings.active_stt_model {
                Some(id) => self.verified_model(id).ok(),
                None => None,
            };
            stt.transcribe(pcm, model_path.as_deref())?
        } else {
            transcript.to_string()
        };
        self.snapshot.transition(PipelineState::Dictionary)?;
        let dictionary_text = self.dictionary.apply(&raw);
        self.snapshot.transition(PipelineState::Personalization)?;
        let personalized_text = self.personalization.apply(&dictionary_text);
        self.snapshot.transition(PipelineState::Llm)?;
        let llm_path = match &self.settings.active_llm_model {
            Some(id) => self.verified_model(id).ok(),
            None => None,
        };
        let llm_text =
            match llm.generate(&personalized_text, self.settings.mode, llm_path.as_deref()) {
                Ok(text) => text,
                Err(LfError::RuntimeUnsupported(_)) | Err(LfError::ModelMissing(_)) => {
                    format_without_remote_llm(self.settings.mode, &personalized_text)
                }
                Err(other) => return Err(other),
            };
        self.snapshot.transition(PipelineState::Validate)?;
        let final_text = llm_text.trim().to_string();
        if self.settings.mode == PipelineMode::Code {
            debug_assert!(crate::llm::assert_non_execution_policy());
        }
        self.snapshot.transition(PipelineState::Injecting)?;
        injector.insert_text(&final_text, self.settings.restore_clipboard)?;
        self.snapshot.transition(PipelineState::Completed)?;
        let output = PipelineOutput {
            raw_transcript: raw.clone(),
            dictionary_text,
            personalized_text,
            final_text: final_text.clone(),
            mode: self.settings.mode,
        };
        self.last_output = Some(output.clone());
        let item = HistoryItem {
            id: Uuid::new_v4().to_string(),
            created_at: Utc::now().to_rfc3339(),
            mode: format!("{:?}", self.settings.mode).to_lowercase(),
            transcript: raw,
            output: final_text,
        };
        self.store.insert_history(&item)?;
        self.snapshot.transition(PipelineState::Idle)?;
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
}
