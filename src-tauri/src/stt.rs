use crate::error::{LfError, LfResult};
use crate::whisper_stt::DecodeOptions;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

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
        transcribe_native(
            pcm,
            model_path,
            language,
            options,
            crate::whisper_stt::Cancellation::Dictation,
        )
    }
}

pub fn transcribe_file(
    pcm: &[f32],
    model_path: Option<&Path>,
    language: &str,
    options: &DecodeOptions,
    cancellation: Arc<AtomicBool>,
) -> LfResult<String> {
    let result = transcribe_native(
        pcm,
        model_path,
        language,
        options,
        crate::whisper_stt::Cancellation::File(Arc::clone(&cancellation)),
    );
    if cancellation.load(Ordering::Relaxed) {
        return Err(LfError::AudioCancelled(
            "Audio file task was cancelled.".into(),
        ));
    }
    result
}

fn transcribe_native(
    pcm: &[f32],
    model_path: Option<&Path>,
    language: &str,
    options: &DecodeOptions,
    cancellation: crate::whisper_stt::Cancellation,
) -> LfResult<String> {
    if cancellation.is_cancelled() {
        return Err(LfError::Other("cancelled".into()));
    }
    if let Some(path) = model_path {
        if matches!(
            options.stt_engine.trim().to_ascii_lowercase().as_str(),
            "gigaam" | "parakeet"
        ) {
            let result = crate::sherpa_stt::transcribe(&options.stt_engine, path, pcm);
            return if cancellation.is_cancelled() {
                Err(LfError::Other("cancelled".into()))
            } else {
                result
            };
        }
        if options.stt_engine.trim().eq_ignore_ascii_case("tone") {
            let result = match crate::tone_stt::transcribe(path, pcm) {
                Ok(text) if !text.trim().is_empty() => Ok(text),
                Ok(_) => Err(LfError::Other(
                    "No speech detected. Nothing was inserted.".into(),
                )),
                Err(err) => Err(err),
            };
            return if cancellation.is_cancelled() {
                Err(LfError::Other("cancelled".into()))
            } else {
                result
            };
        }
        match crate::whisper_stt::transcribe(path, pcm, cancellation, language, options) {
            Ok(text) if !text.trim().is_empty() => Ok(text),
            Ok(_) => Err(LfError::Other(
                "No speech detected. Nothing was inserted.".into(),
            )),
            Err(err) => Err(err),
        }
    } else {
        let engine = options.stt_engine.trim().to_ascii_lowercase();
        let message = match engine.as_str() {
            "tone" => "T-One is not installed. Open Models and download T-One Streaming Russian.",
            "gigaam" => "GigaAM is not installed. Open Models and download GigaAM v3 CTC.",
            "parakeet" => {
                "Parakeet is not installed. Open Models and download the Parakeet package."
            }
            _ => "Whisper is not installed. Open Models and download the speech model.",
        };
        Err(LfError::ModelMissing(message.into()))
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
        let options = DecodeOptions {
            stt_engine: "tone".into(),
            ..DecodeOptions::default()
        };
        let err = NativeStt
            .transcribe(&[0.1; 800], None, "ru", &options)
            .unwrap_err();
        assert!(err.to_string().to_lowercase().contains("t-one"), "{err}");
    }

    #[test]
    fn file_cancellation_has_its_own_error_code_before_model_load() {
        let cancellation = Arc::new(AtomicBool::new(true));
        let err = transcribe_file(
            &[0.1; 800],
            Some(Path::new("/no/such/whisper.bin")),
            "ru",
            &DecodeOptions::default(),
            cancellation,
        )
        .unwrap_err();
        assert_eq!(err.code(), "AUDIO_CANCELLED");
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
