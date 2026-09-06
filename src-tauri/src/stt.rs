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
                Ok(_) => Err(LfError::Other(
                    "No speech detected. Nothing was inserted.".into(),
                )),
                Err(err) => Err(err),
            }
        } else {
            Err(LfError::ModelMissing(
                "Whisper is not installed. Open Models and download the speech model.".into(),
            ))
        }
    }
}

pub fn transcribe_with_paragraph_pauses(
    stt: &dyn SpeechToText,
    pcm: &[f32],
    model_path: Option<&Path>,
    language: &str,
    vad_threshold: f32,
) -> LfResult<String> {
    let _ = vad_threshold;
    // One Whisper `full()` per utterance. Extra passes on VAD chunks used to
    // multiply CPU on pauses; paragraph breaks still come from segment cues.
    stt.transcribe(pcm, model_path, language)
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

    #[test]
    fn native_stt_without_model_path_does_not_use_macos_speech() {
        let err = NativeStt.transcribe(&[0.1; 800], None, "ru").unwrap_err();
        assert_eq!(err.code(), "MODEL_MISSING");
    }

    #[test]
    fn paragraph_helper_runs_stt_once_even_with_a_long_pause() {
        use std::sync::atomic::{AtomicU32, Ordering};
        struct CountStt {
            hits: AtomicU32,
        }
        impl SpeechToText for CountStt {
            fn transcribe(
                &self,
                _pcm: &[f32],
                _model_path: Option<&Path>,
                _language: &str,
            ) -> LfResult<String> {
                self.hits.fetch_add(1, Ordering::Relaxed);
                Ok("один два".into())
            }
        }
        let sr = 16_000usize;
        let mut pcm = vec![0.0; sr * 6];
        for sample in pcm.iter_mut().take(sr / 2) {
            *sample = 0.2;
        }
        for sample in pcm.iter_mut().skip(sr * 4) {
            *sample = 0.2;
        }
        let stt = CountStt {
            hits: AtomicU32::new(0),
        };
        let text = transcribe_with_paragraph_pauses(&stt, &pcm, None, "ru", 0.012).unwrap();
        assert_eq!(text, "один два");
        assert_eq!(stt.hits.load(Ordering::Relaxed), 1);
    }
}
