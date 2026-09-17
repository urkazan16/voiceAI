//! Offline sherpa-onnx adapters for the model families exposed in Settings.
//! The model manager keeps the primary file and its companion files together;
//! this module only needs the primary path to locate the rest.

use crate::error::{LfError, LfResult};
use sherpa_onnx::{
    OfflineNemoEncDecCtcModelConfig, OfflineRecognizer, OfflineRecognizerConfig,
    OfflineTransducerModelConfig,
};
use std::path::Path;

fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

pub fn transcribe(engine: &str, model_path: &Path, pcm: &[f32]) -> LfResult<String> {
    crate::error::catch_runtime_panic("Speech recognition", || {
        transcribe_inner(engine, model_path, pcm)
    })
    .and_then(|result| result)
}

fn transcribe_inner(engine: &str, model_path: &Path, pcm: &[f32]) -> LfResult<String> {
    let parent = model_path.parent().ok_or_else(|| {
        LfError::ModelFormatInvalid("speech model has no parent directory".into())
    })?;
    let tokens = parent.join("tokens.txt");
    if !tokens.exists() {
        return Err(LfError::ModelMissing(format!("{engine} tokens.txt")));
    }

    let mut config = OfflineRecognizerConfig::default();
    config.model_config.tokens = Some(path_string(&tokens));
    config.model_config.num_threads = num_cpus::get_physical().max(1) as i32;
    config.model_config.provider = Some("cpu".into());

    match engine.trim().to_ascii_lowercase().as_str() {
        "gigaam" => {
            config.model_config.nemo_ctc = OfflineNemoEncDecCtcModelConfig {
                model: Some(path_string(model_path)),
            };
            config.model_config.model_type = Some("nemo_ctc".into());
        }
        "parakeet" => {
            let decoder = parent.join("decoder.int8.onnx");
            let joiner = parent.join("joiner.int8.onnx");
            if !decoder.exists() || !joiner.exists() {
                return Err(LfError::ModelMissing(
                    "Parakeet decoder.int8.onnx/joiner.int8.onnx".into(),
                ));
            }
            config.model_config.transducer = OfflineTransducerModelConfig {
                encoder: Some(path_string(model_path)),
                decoder: Some(path_string(&decoder)),
                joiner: Some(path_string(&joiner)),
            };
            config.model_config.model_type = Some("nemo_transducer".into());
        }
        other => {
            return Err(LfError::ConfigInvalid(format!(
                "Unsupported speech engine: {other}"
            )))
        }
    }

    let recognizer = OfflineRecognizer::create(&config).ok_or_else(|| {
        LfError::ModelFormatInvalid(format!("Unable to initialize {engine} sherpa-onnx model"))
    })?;
    let stream = recognizer.create_stream();
    stream.accept_waveform(16_000, pcm);
    recognizer.decode(&stream);
    let text = stream
        .get_result()
        .map(|result| result.text)
        .unwrap_or_default();
    if text.trim().is_empty() {
        return Err(LfError::Other(
            "No speech detected. Nothing was inserted.".into(),
        ));
    }
    Ok(text)
}
