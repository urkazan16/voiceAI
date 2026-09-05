use crate::error::{LfError, LfResult};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelRecord {
    pub model_id: String,
    pub display_name: String,
    pub version: String,
    pub filename: String,
    pub format: String,
    pub quantization: String,
    pub kind: String,
    pub source: String,
    pub source_url: String,
    #[serde(default)]
    pub download_url: String,
    pub sha256: String,
    pub size: u64,
    pub license: String,
    pub license_url: String,
    #[serde(default)]
    pub network_required_to_obtain: bool,
    #[serde(default)]
    pub checksum_pinned: bool,
    #[serde(default)]
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCatalog {
    pub catalog_version: String,
    pub models: Vec<ModelRecord>,
}

impl ModelCatalog {
    pub fn from_json(json: &str) -> LfResult<Self> {
        Ok(serde_json::from_str(json)?)
    }

    pub fn load_path(path: &Path) -> LfResult<Self> {
        let json = std::fs::read_to_string(path)?;
        Self::from_json(&json)
    }

    pub fn embedded() -> LfResult<Self> {
        Self::from_json(include_str!("../resources/model-catalog.json"))
    }

    pub fn get(&self, model_id: &str) -> LfResult<&ModelRecord> {
        self.models
            .iter()
            .find(|m| m.model_id == model_id)
            .ok_or_else(|| LfError::ModelMissing(model_id.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_catalog_has_required_identity_fields() {
        let catalog = ModelCatalog::embedded().unwrap();
        assert!(!catalog.models.is_empty());
        for model in &catalog.models {
            assert!(!model.model_id.is_empty());
            assert!(!model.version.is_empty());
            assert!(!model.filename.is_empty());
            assert!(!model.format.is_empty());
            assert!(!model.quantization.is_empty());
            assert!(!model.source.is_empty());
            assert_eq!(model.sha256.len(), 64);
            assert!(!model.license.is_empty());
            assert!(
                model.checksum_pinned,
                "{} must pin a vendor SHA-256 before download",
                model.model_id
            );
            assert!(
                !model.download_url.is_empty(),
                "{} needs a download URL",
                model.model_id
            );
        }
    }
}
