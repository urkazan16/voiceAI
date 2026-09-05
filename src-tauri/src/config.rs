use crate::catalog::ModelCatalog;
use crate::dictionary::Dictionary;
use crate::error::{LfError, LfResult};
use crate::personalization::PersonalizationState;
use crate::pipeline::PipelineMode;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppSettings {
    pub hotkey: String,
    pub mode: PipelineMode,
    pub microphone_name: Option<String>,
    pub active_stt_model: Option<String>,
    pub active_llm_model: Option<String>,
    pub restore_clipboard: bool,
    pub onboarding_complete: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            hotkey: "Control+Shift+Space".into(),
            mode: PipelineMode::Normal,
            microphone_name: None,
            active_stt_model: Some("whisper-small".into()),
            active_llm_model: Some("Qwen3-4B-Instruct-2507".into()),
            restore_clipboard: true,
            onboarding_complete: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub mode: PipelineMode,
    pub dictionary_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExportedConfig {
    pub version: u32,
    pub settings: AppSettings,
    pub profiles: Vec<Profile>,
    pub dictionary: Dictionary,
    pub personalization: PersonalizationState,
    pub models: ModelSelection,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelSelection {
    pub active_stt_model: Option<String>,
    pub active_llm_model: Option<String>,
}

impl ExportedConfig {
    pub fn validate(&self, catalog: &ModelCatalog) -> LfResult<()> {
        if self.version != 1 {
            return Err(LfError::ConfigInvalid(format!(
                "unsupported config version {}",
                self.version
            )));
        }
        if let Some(id) = &self.models.active_stt_model {
            catalog.get(id)?;
        }
        if let Some(id) = &self.models.active_llm_model {
            catalog.get(id)?;
        }
        Ok(())
    }
}

pub fn export_config(
    settings: &AppSettings,
    profiles: &[Profile],
    dictionary: &Dictionary,
    personalization: &PersonalizationState,
) -> ExportedConfig {
    ExportedConfig {
        version: 1,
        models: ModelSelection {
            active_stt_model: settings.active_stt_model.clone(),
            active_llm_model: settings.active_llm_model.clone(),
        },
        settings: settings.clone(),
        profiles: profiles.to_vec(),
        dictionary: dictionary.clone(),
        personalization: personalization.clone(),
    }
}

pub fn import_config(json: &str, catalog: &ModelCatalog) -> LfResult<ExportedConfig> {
    let parsed: ExportedConfig = serde_json::from_str(json)?;
    parsed.validate(catalog)?;
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_config() {
        let catalog = ModelCatalog::embedded().unwrap();
        let settings = AppSettings::default();
        let exported = export_config(
            &settings,
            &[],
            &Dictionary::default(),
            &PersonalizationState::default(),
        );
        let json = serde_json::to_string_pretty(&exported).unwrap();
        let imported = import_config(&json, &catalog).unwrap();
        assert_eq!(imported.settings.hotkey, "Control+Shift+Space");
    }
}
