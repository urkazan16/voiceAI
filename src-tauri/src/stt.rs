use crate::error::{LfError, LfResult};
use crate::runtime;
use std::path::Path;

pub trait SpeechToText: Send + Sync {
    fn transcribe(
        &self,
        pcm: &[f32],
        model_path: Option<&Path>,
        language: &str,
    ) -> LfResult<String>;
}

pub struct NativeStt;

impl SpeechToText for NativeStt {
    fn transcribe(
        &self,
        pcm: &[f32],
        model_path: Option<&Path>,
        language: &str,
    ) -> LfResult<String> {
        if let Some(path) = model_path {
            match crate::whisper_stt::transcribe(
                path,
                pcm,
                crate::dictation::cancel_flag(),
                language,
            ) {
                Ok(text) if !text.trim().is_empty() => return Ok(text),
                Ok(_) => {
                    return Err(LfError::RuntimeUnsupported(
                        "whisper.cpp produced empty text".into(),
                    ))
                }
                Err(err) => return Err(err),
            }
        }
        match runtime::native_transcribe("", pcm) {
            Ok(text) if !text.trim().is_empty() => Ok(text),
            _ => crate::macos_stt::transcribe_pcm_16k(pcm),
        }
    }
}

pub struct ScriptedStt {
    pub transcript: String,
}

impl SpeechToText for ScriptedStt {
    fn transcribe(
        &self,
        _pcm: &[f32],
        _model_path: Option<&Path>,
        _language: &str,
    ) -> LfResult<String> {
        Ok(self.transcript.clone())
    }
}
