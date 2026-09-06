use crate::catalog::ModelCatalog;
use crate::dictionary::Dictionary;
use crate::error::{LfError, LfResult};
use crate::personalization::PersonalizationState;
use crate::pipeline::PipelineMode;
use crate::profiles::Profile;
use crate::snippets::SnippetBook;
use serde::{Deserialize, Serialize};

pub const DEFAULT_STT_MODEL: &str = "whisper-medium";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AppSettings {
    pub hotkey: String,
    pub mode: PipelineMode,
    pub microphone_name: Option<String>,
    pub active_stt_model: Option<String>,
    pub active_llm_model: Option<String>,
    pub restore_clipboard: bool,
    pub onboarding_complete: bool,
    pub copy_last_hotkey: String,
    pub paste_last_hotkey: String,
    pub show_flow_bar: bool,
    pub profile_override: Option<String>,
    pub personalization_enabled: bool,
    pub learn_from_corrections: bool,
    pub stt_language: String,
    #[serde(default = "default_insert_delay")]
    pub insert_delay_ms: u64,
    #[serde(default = "default_postprocess_timeout")]
    pub postprocess_timeout_ms: u64,
    #[serde(default = "default_true")]
    pub sound_cues: bool,
    #[serde(default = "default_cue_volume")]
    pub sound_cue_volume: f32,
    #[serde(default = "default_log_max")]
    pub log_max_bytes: u64,
    #[serde(default)]
    pub autostart: bool,
    #[serde(default = "default_true")]
    pub history_enabled: bool,
    #[serde(default = "default_vad")]
    pub vad_threshold: f32,
    #[serde(default = "default_history_max")]
    pub history_max_items: u32,
    #[serde(default)]
    pub hands_free: bool,
    #[serde(default = "default_true")]
    pub digits_from_speech: bool,
    #[serde(default = "default_date_format")]
    pub date_format: String,
    #[serde(default = "default_compute")]
    pub compute_device: String,
}

fn default_true() -> bool {
    true
}

fn default_insert_delay() -> u64 {
    120
}

fn default_postprocess_timeout() -> u64 {
    45_000
}

fn default_log_max() -> u64 {
    2 * 1024 * 1024
}

fn default_history_max() -> u32 {
    500
}

fn default_vad() -> f32 {
    crate::vad::default_threshold()
}

fn default_compute() -> String {
    "cpu".into()
}

fn default_date_format() -> String {
    "DMY".into()
}

fn default_cue_volume() -> f32 {
    0.25
}

pub fn clamp_cue_volume(volume: f32) -> f32 {
    if !volume.is_finite() {
        return default_cue_volume();
    }
    volume.clamp(0.05, 1.0)
}

impl AppSettings {
    pub fn normalize(&mut self) {
        self.vad_threshold = crate::vad::clamp_threshold(self.vad_threshold);
        self.history_max_items = self.history_max_items.clamp(50, 10_000);
        if self.history_max_items == 0 {
            self.history_max_items = default_history_max();
        }
        if self.date_format != "ISO" {
            self.date_format = default_date_format();
        }
        self.compute_device = default_compute();
        if self.insert_delay_ms < 40 {
            self.insert_delay_ms = 40;
        }
        self.sound_cue_volume = clamp_cue_volume(self.sound_cue_volume);
    }

    /// First install / first launch: speech model is Medium unless the user already picked Turbo or another catalog id.
    pub fn apply_shipped_stt_default(&mut self) {
        match self.active_stt_model.as_deref() {
            None | Some("") | Some("whisper-small") | Some("whisper-base") => {
                self.active_stt_model = Some(DEFAULT_STT_MODEL.to_string());
            }
            _ => {}
        }
    }
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            hotkey: "Control+Shift+Space".into(),
            mode: PipelineMode::Normal,
            microphone_name: None,
            active_stt_model: Some(DEFAULT_STT_MODEL.into()),
            active_llm_model: Some("Qwen3-4B-Instruct-2507".into()),
            restore_clipboard: true,
            onboarding_complete: false,
            copy_last_hotkey: "Command+Control+C".into(),
            paste_last_hotkey: "Command+Control+V".into(),
            show_flow_bar: true,
            profile_override: None,
            personalization_enabled: true,
            learn_from_corrections: true,
            stt_language: "ru".into(),
            insert_delay_ms: default_insert_delay(),
            postprocess_timeout_ms: default_postprocess_timeout(),
            sound_cues: true,
            sound_cue_volume: default_cue_volume(),
            log_max_bytes: default_log_max(),
            autostart: false,
            history_enabled: true,
            vad_threshold: default_vad(),
            history_max_items: default_history_max(),
            hands_free: false,
            digits_from_speech: true,
            date_format: default_date_format(),
            compute_device: default_compute(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExportedConfig {
    pub version: u32,
    pub settings: AppSettings,
    pub profiles: Vec<Profile>,
    pub dictionary: Dictionary,
    pub personalization: PersonalizationState,
    #[serde(default)]
    pub snippets: SnippetBook,
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
    snippets: &SnippetBook,
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
        snippets: snippets.clone(),
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
            &SnippetBook::default(),
        );
        let json = serde_json::to_string_pretty(&exported).unwrap();
        let imported = import_config(&json, &catalog).unwrap();
        assert_eq!(imported.settings.hotkey, "Control+Shift+Space");
        assert_eq!(imported.settings.stt_language, "ru");
        assert_eq!(
            imported.settings.active_stt_model.as_deref(),
            Some(DEFAULT_STT_MODEL)
        );
    }

    #[test]
    fn shipped_default_upgrades_legacy_base_and_small() {
        let mut settings = AppSettings {
            active_stt_model: Some("whisper-base".into()),
            stt_language: "auto".into(),
            ..AppSettings::default()
        };
        settings.apply_shipped_stt_default();
        assert_eq!(settings.active_stt_model.as_deref(), Some("whisper-medium"));
        assert_eq!(settings.stt_language, "auto");
        settings.active_stt_model = Some("whisper-large-v3-turbo".into());
        settings.apply_shipped_stt_default();
        assert_eq!(
            settings.active_stt_model.as_deref(),
            Some("whisper-large-v3-turbo")
        );
    }

    #[test]
    fn rejects_unknown_stt_model() {
        let catalog = ModelCatalog::embedded().unwrap();
        let settings = AppSettings {
            active_stt_model: Some("missing-whisper".into()),
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
        assert!(import_config(&json, &catalog).is_err());
    }
}
