use crate::error::{LfError, LfResult};
use crate::whisper_stt::DecodeOptions;
use std::path::Path;

pub trait SpeechToText: Send + Sync {
    fn transcribe(
        &self,
        pcm: &[f32],
        model_path: Option<&Path>,
        language: &str,
        options: &DecodeOptions,
    ) -> LfResult<String>;
}

pub struct NativeStt;

impl SpeechToText for NativeStt {
    fn transcribe(
        &self,
        pcm: &[f32],
        model_path: Option<&Path>,
        language: &str,
        options: &DecodeOptions,
    ) -> LfResult<String> {
        if let Some(path) = model_path {
            if matches!(
                options.stt_engine.trim().to_ascii_lowercase().as_str(),
                "gigaam" | "parakeet"
            ) {
                return crate::sherpa_stt::transcribe(&options.stt_engine, path, pcm);
            }
            if options.stt_engine.trim().eq_ignore_ascii_case("tone") {
                return match crate::tone_stt::transcribe(path, pcm) {
                    Ok(text) if !text.trim().is_empty() => Ok(text),
                    Ok(_) => Err(LfError::Other(
                        "No speech detected. Nothing was inserted.".into(),
                    )),
                    Err(err) => Err(err),
                };
            }
            match crate::whisper_stt::transcribe(
                path,
                pcm,
                crate::dictation::cancel_flag(),
                language,
                options,
            ) {
                Ok(text) if !text.trim().is_empty() => Ok(text),
                Ok(_) => Err(LfError::Other(
                    "No speech detected. Nothing was inserted.".into(),
                )),
                Err(err) => Err(err),
            }
        } else {
            let engine = options.stt_engine.trim().to_ascii_lowercase();
            let message = match engine.as_str() {
                "tone" => {
                    "T-One is not installed. Open Models and download T-One Streaming Russian."
                }
                "gigaam" => "GigaAM is not installed. Open Models and download GigaAM v3 CTC.",
                "parakeet" => {
                    "Parakeet is not installed. Open Models and download the Parakeet package."
                }
                _ => "Whisper is not installed. Open Models and download the speech model.",
            };
            Err(LfError::ModelMissing(message.into()))
        }
    }
}

pub fn transcribe_with_paragraph_pauses(
    stt: &dyn SpeechToText,
    pcm: &[f32],
    model_path: Option<&Path>,
    language: &str,
    vad_threshold: f32,
    options: &DecodeOptions,
) -> LfResult<String> {
    let _ = vad_threshold;
    // One Whisper `full()` per utterance. Extra passes on VAD chunks used to
    // multiply CPU on pauses; paragraph breaks still come from segment cues.
    stt.transcribe(pcm, model_path, language, options)
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
        _options: &DecodeOptions,
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
        assert_eq!(
            stt.transcribe(&[], None, "ru", &DecodeOptions::default())
                .unwrap(),
            "привет"
        );
    }

    #[test]
    fn native_stt_missing_model_is_whisper_not_c_stub() {
        let err = NativeStt
            .transcribe(
                &[0.1; 800],
                Some(Path::new("/no/such/whisper.bin")),
                "ru",
                &DecodeOptions::default(),
            )
            .unwrap_err();
        assert_eq!(err.code(), "MODEL_MISSING");
        assert!(!err.to_string().contains("localflow-native-stub"));
        assert!(!err.to_string().contains("build-native-runtime"));
    }

    #[test]
    fn native_stt_without_model_path_does_not_use_macos_speech() {
        let err = NativeStt
            .transcribe(&[0.1; 800], None, "ru", &DecodeOptions::default())
            .unwrap_err();
        assert_eq!(err.code(), "MODEL_MISSING");
    }

    #[test]
    fn native_stt_without_path_names_the_selected_engine() {
        let mut options = DecodeOptions::default();
        options.stt_engine = "tone".into();
        let err = NativeStt
            .transcribe(&[0.1; 800], None, "ru", &options)
            .unwrap_err();
        assert!(err.to_string().to_lowercase().contains("t-one"), "{err}");
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
                _options: &DecodeOptions,
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
        let text = transcribe_with_paragraph_pauses(
            &stt,
            &pcm,
            None,
            "ru",
            0.012,
            &DecodeOptions::default(),
        )
        .unwrap();
        assert_eq!(text, "один два");
        assert_eq!(stt.hits.load(Ordering::Relaxed), 1);
    }
}
