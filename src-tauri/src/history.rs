use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HistoryItem {
    pub id: String,
    pub created_at: String,
    pub mode: String,
    pub transcript: String,
    pub output: String,
}
