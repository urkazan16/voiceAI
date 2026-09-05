use crate::error::{LfError, LfResult};
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
                Ok(text) if !text.trim().is_empty() => Ok(text),
                Ok(_) => Err(LfError::RuntimeUnsupported(
                    "whisper.cpp produced empty text".into(),
                )),
                Err(err) => Err(err),
            }
        } else {
            crate::macos_stt::transcribe_pcm_16k(pcm)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripted_stt_returns_provided_transcript() {
        let stt = ScriptedStt {
            transcript: "привет".into(),
        };
        assert_eq!(stt.transcribe(&[], None, "ru").unwrap(), "привет");
    }

    #[test]
    fn native_stt_missing_model_is_whisper_not_c_stub() {
        let err = NativeStt
            .transcribe(&[0.1; 800], Some(Path::new("/no/such/whisper.bin")), "ru")
            .unwrap_err();
        assert_eq!(err.code(), "MODEL_MISSING");
        assert!(!err.to_string().contains("localflow-native-stub"));
        assert!(!err.to_string().contains("build-native-runtime"));
    }
}
