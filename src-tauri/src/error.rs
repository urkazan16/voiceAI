use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LfError {
    #[error("MODEL_CHECKSUM_MISMATCH expected={expected} actual={actual}")]
    ModelChecksumMismatch { expected: String, actual: String },
    #[error("MODEL_MISSING: {0}")]
    ModelMissing(String),
    #[error("MODEL_FORMAT_INVALID: {0}")]
    ModelFormatInvalid(String),
    #[error("MODEL_NOT_PINNED: {0}")]
    ModelNotPinned(String),
    #[error("NETWORK_OPERATION_REQUIRED: {0}")]
    NetworkRequired(String),
    #[error("PERMISSION_DENIED: {0}")]
    PermissionDenied(String),
    #[error("DEVICE_UNAVAILABLE: {0}")]
    DeviceUnavailable(String),
    #[error("INJECTION_FAILED: {0}")]
    InjectionFailed(String),
    #[error("PIPELINE_INVALID_STATE: {from} -> {to}")]
    PipelineInvalidState { from: String, to: String },
    #[error("RUNTIME_UNSUPPORTED: {0}")]
    RuntimeUnsupported(String),
    #[error("CONFIG_INVALID: {0}")]
    ConfigInvalid(String),
    #[error("IO: {0}")]
    Io(#[from] std::io::Error),
    #[error("DB: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Other(String),
}

impl LfError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::ModelChecksumMismatch { .. } => "MODEL_CHECKSUM_MISMATCH",
            Self::ModelMissing(_) => "MODEL_MISSING",
            Self::ModelFormatInvalid(_) => "MODEL_FORMAT_INVALID",
            Self::ModelNotPinned(_) => "MODEL_NOT_PINNED",
            Self::NetworkRequired(_) => "NETWORK_OPERATION_REQUIRED",
            Self::PermissionDenied(_) => "PERMISSION_DENIED",
            Self::DeviceUnavailable(_) => "DEVICE_UNAVAILABLE",
            Self::InjectionFailed(_) => "INJECTION_FAILED",
            Self::PipelineInvalidState { .. } => "PIPELINE_INVALID_STATE",
            Self::RuntimeUnsupported(_) => "RUNTIME_UNSUPPORTED",
            Self::ConfigInvalid(_) => "CONFIG_INVALID",
            Self::Io(_) => "IO",
            Self::Db(_) => "DB",
            Self::Json(_) => "JSON",
            Self::Other(_) => "ERROR",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorDto {
    pub code: String,
    pub message: String,
}

impl From<&LfError> for ErrorDto {
    fn from(value: &LfError) -> Self {
        Self {
            code: value.code().to_string(),
            message: value.to_string(),
        }
    }
}

pub type LfResult<T> = Result<T, LfError>;

pub fn path_buf_error(path: PathBuf) -> String {
    path.display().to_string()
}
