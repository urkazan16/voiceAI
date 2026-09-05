use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HistoryItem {
    pub id: String,
    pub created_at: String,
    pub mode: String,
    pub transcript: String,
    pub output: String,
    #[serde(default)]
    pub application: String,
    #[serde(default)]
    pub profile: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub processing_time_ms: u64,
    #[serde(default)]
    pub timecodes: String,
}
