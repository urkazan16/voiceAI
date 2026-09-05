use crate::error::LfResult;
use crate::runtime;
use std::path::Path;

pub trait SpeechToText: Send + Sync {
    fn transcribe(&self, pcm: &[f32], model_path: Option<&Path>) -> LfResult<String>;
}

pub struct NativeStt;

impl SpeechToText for NativeStt {
    fn transcribe(&self, pcm: &[f32], model_path: Option<&Path>) -> LfResult<String> {
        let path = model_path
            .ok_or_else(|| crate::error::LfError::ModelMissing("stt".into()))?
            .to_string_lossy();
        runtime::native_transcribe(&path, pcm)
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
