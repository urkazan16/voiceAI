//! Offline sherpa-onnx adapters for the model families exposed in Settings.
//! The recognizer owns the expensive ONNX Runtime sessions, so it lives on a
//! dedicated worker and is reused between utterances. Only the lightweight
//! `OfflineStream` is created for each decode.

use crate::error::{LfError, LfResult};
use sherpa_onnx::{
    OfflineNemoEncDecCtcModelConfig, OfflineRecognizer, OfflineRecognizerConfig,
    OfflineTransducerModelConfig,
};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::sync::OnceLock;
use std::time::Duration;

struct Loaded {
    engine: String,
    path: PathBuf,
    recognizer: OfflineRecognizer,
}

struct TranscribeJob {
    engine: String,
    model_path: PathBuf,
    pcm: Vec<f32>,
    reply: Sender<LfResult<String>>,
}

enum WorkerCmd {
    Preload { engine: String, path: PathBuf },
    Transcribe(TranscribeJob),
    Unload(Sender<()>),
}

static JOBS: OnceLock<Sender<WorkerCmd>> = OnceLock::new();

fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn normalized_engine(engine: &str) -> String {
    engine.trim().to_ascii_lowercase()
}

pub fn preload(engine: impl Into<String>, path: PathBuf) {
    let _ = worker().send(WorkerCmd::Preload {
        engine: normalized_engine(&engine.into()),
        path,
    });
}

pub fn unload() {
    let Some(tx) = JOBS.get() else {
        return;
    };
    let (done, rx) = mpsc::channel();
    if tx.send(WorkerCmd::Unload(done)).is_ok() {
        let _ = rx.recv_timeout(Duration::from_secs(8));
    }
}

pub fn transcribe(engine: &str, model_path: &Path, pcm: &[f32]) -> LfResult<String> {
    if !model_path.is_file() {
        return Err(LfError::ModelMissing(model_path.display().to_string()));
    }
    if pcm.is_empty() {
        return Err(LfError::Other(
            "No speech detected. Nothing was inserted.".into(),
        ));
    }
    let (reply, rx) = mpsc::channel();
    worker()
        .send(WorkerCmd::Transcribe(TranscribeJob {
            engine: normalized_engine(engine),
            model_path: model_path.to_path_buf(),
            pcm: pcm.to_vec(),
            reply,
        }))
        .map_err(|_| LfError::RuntimeUnsupported("sherpa worker stopped".into()))?;
    let audio_secs = (pcm.len() as u64 / 16_000).max(1);
    let timeout = Duration::from_secs((audio_secs * 8 + 60).clamp(90, 20 * 60));
    rx.recv_timeout(timeout).map_err(|err| match err {
        mpsc::RecvTimeoutError::Timeout => {
            LfError::RuntimeUnsupported("Speech recognition timed out.".into())
        }
        mpsc::RecvTimeoutError::Disconnected => {
            LfError::RuntimeUnsupported("sherpa worker stopped".into())
        }
    })?
}

fn worker() -> Sender<WorkerCmd> {
    JOBS.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<WorkerCmd>();
        std::thread::Builder::new()
            .name("localflow-sherpa".into())
            .spawn(move || {
                let mut loaded: Option<Loaded> = None;
                while let Ok(cmd) = rx.recv() {
                    match cmd {
                        WorkerCmd::Unload(done) => {
                            loaded = None;
                            let _ = done.send(());
                        }
                        WorkerCmd::Preload { engine, path } => {
                            if let Err(err) = ensure_loaded(&mut loaded, &engine, path) {
                                eprintln!("localflow: sherpa preload failed: {err}");
                            }
                        }
                        WorkerCmd::Transcribe(job) => {
                            let result =
                                crate::error::catch_runtime_panic("Speech recognition", || {
                                    run_job(&mut loaded, &job.engine, job.model_path, &job.pcm)
                                })
                                .and_then(|result| result);
                            let _ = job.reply.send(result);
                        }
                    }
                }
            })
            .expect("start sherpa worker");
        tx
    })
    .clone()
}

fn run_job(
    loaded: &mut Option<Loaded>,
    engine: &str,
    model_path: PathBuf,
    pcm: &[f32],
) -> LfResult<String> {
    ensure_loaded(loaded, engine, model_path)?;
    if crate::dictation::is_cancelled() {
        return Err(LfError::Other("cancelled".into()));
    }
    let recognizer = &loaded.as_ref().expect("sherpa recognizer").recognizer;
    let stream = recognizer.create_stream();
    stream.accept_waveform(16_000, pcm);
    recognizer.decode(&stream);
    if crate::dictation::is_cancelled() {
        return Err(LfError::Other("cancelled".into()));
    }
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

fn ensure_loaded(loaded: &mut Option<Loaded>, engine: &str, model_path: PathBuf) -> LfResult<()> {
    let engine = normalized_engine(engine);
    if loaded
        .as_ref()
        .is_some_and(|slot| slot.engine == engine && slot.path == model_path)
    {
        return Ok(());
    }
    let config = recognizer_config(&engine, &model_path)?;
    eprintln!(
        "localflow: loading sherpa model {} ({engine})",
        model_path.display()
    );
    let recognizer = OfflineRecognizer::create(&config).ok_or_else(|| {
        LfError::ModelFormatInvalid(format!("Unable to initialize {engine} sherpa-onnx model"))
    })?;
    *loaded = Some(Loaded {
        engine,
        path: model_path,
        recognizer,
    });
    Ok(())
}

fn recognizer_config(engine: &str, model_path: &Path) -> LfResult<OfflineRecognizerConfig> {
    let parent = model_path.parent().ok_or_else(|| {
        LfError::ModelFormatInvalid("speech model has no parent directory".into())
    })?;
    let tokens = parent.join("tokens.txt");
    if !tokens.exists() {
        return Err(LfError::ModelMissing(format!("{engine} tokens.txt")));
    }

    let mut config = OfflineRecognizerConfig::default();
    config.model_config.tokens = Some(path_string(&tokens));
    config.model_config.num_threads = num_cpus::get_physical().clamp(1, 8) as i32;
    config.model_config.provider = Some("cpu".into());

    match engine {
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
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_key_is_stable_for_the_worker_cache() {
        assert_eq!(normalized_engine(" GigaAM "), "gigaam");
        assert_eq!(normalized_engine("PARAKEET"), "parakeet");
    }

    #[test]
    fn unknown_engine_is_rejected_before_model_load() {
        let dir = tempfile::tempdir().unwrap();
        let model = dir.path().join("model.onnx");
        std::fs::write(&model, b"model").unwrap();
        std::fs::write(dir.path().join("tokens.txt"), b"tokens").unwrap();
        let err = recognizer_config("unknown", &model).unwrap_err();
        assert_eq!(err.code(), "CONFIG_INVALID");
    }
}
