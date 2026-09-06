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
    let chunks = crate::vad::split_on_internal_silence(
        pcm,
        16_000,
        vad_threshold,
        crate::pipeline::PARAGRAPH_PAUSE_MS,
    );
    if chunks.len() <= 1 {
        return stt.transcribe(pcm, model_path, language);
    }
    let mut parts = Vec::new();
    let mut all_cues = Vec::new();
    let mut offset_ms = 0u64;
    for chunk in &chunks {
        let text = stt.transcribe(chunk, model_path, language)?;
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            parts.push(trimmed.to_string());
        }
        let mut cues = crate::whisper_stt::last_cues();
        for cue in &mut cues {
            cue.start_ms = cue.start_ms.saturating_add(offset_ms);
            cue.end_ms = cue.end_ms.saturating_add(offset_ms);
        }
        all_cues.extend(cues);
        offset_ms = offset_ms.saturating_add((chunk.len() as u64 * 1000) / 16_000);
    }
    crate::whisper_stt::store_cues(all_cues);
    Ok(parts.join("\n\n"))
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
}
