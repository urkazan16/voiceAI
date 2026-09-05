use crate::error::LfResult;
use crate::runtime;
use std::path::Path;

pub trait SpeechToText: Send + Sync {
    fn transcribe(&self, pcm: &[f32], model_path: Option<&Path>) -> LfResult<String>;
}

pub struct NativeStt;

impl SpeechToText for NativeStt {
    fn transcribe(&self, pcm: &[f32], model_path: Option<&Path>) -> LfResult<String> {
        if let Some(path) = model_path {
            match runtime::native_transcribe(&path.to_string_lossy(), pcm) {
                Ok(text) if !text.trim().is_empty() => return Ok(text),
                Ok(_) => {}
                Err(crate::error::LfError::RuntimeUnsupported(_))
                | Err(crate::error::LfError::ModelMissing(_)) => {}
                Err(other) => return Err(other),
            }
        }
        crate::macos_stt::transcribe_pcm_16k(pcm)
    }
}

pub struct ScriptedStt {
    pub transcript: String,
}

impl SpeechToText for ScriptedStt {
    fn transcribe(&self, _pcm: &[f32], _model_path: Option<&Path>) -> LfResult<String> {
        Ok(self.transcript.clone())
    }
}
