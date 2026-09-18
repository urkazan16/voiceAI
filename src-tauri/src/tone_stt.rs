//! Streaming Russian recognition through the T-One CTC model.
//!
//! The online recognizer owns model state while each held-hotkey session owns
//! an `OnlineStream`. Audio is delivered in small microphone chunks, so a
//! long dictation never needs to be assembled into one in-memory PCM buffer.

use crate::audio::CapturedAudio;
use crate::error::{LfError, LfResult};
use sherpa_onnx::{OnlineRecognizer, OnlineRecognizerConfig};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, OnceLock};

/// Official T-One CTC models are trained at 8 kHz. Microphone PCM is 16 kHz
/// and is resampled by sherpa when `accept_waveform` is given that rate.
const TONE_SAMPLE_RATE: i32 = 8_000;
const PCM_SAMPLE_RATE: i32 = 16_000;
/// Official `online-t-one-ctc-decode-files.py`: 0.3 s lead-in, 0.66 s tail.
const LEFT_PAD_MS: u32 = 300;
const TAIL_PAD_MS: u32 = 660;
const DECODE_STEPS_PER_CHUNK: u32 = 8;
const DECODE_STEPS_FINAL: u32 = 64;

struct Loaded {
    path: PathBuf,
    recognizer: OnlineRecognizer,
}

struct SessionJob {
    model_path: PathBuf,
    audio: Receiver<CapturedAudio>,
    events: Sender<StreamEvent>,
    cancellation: Arc<AtomicBool>,
}

struct TranscribeJob {
    model_path: PathBuf,
    pcm: Vec<f32>,
    reply: Sender<LfResult<String>>,
}

enum WorkerCmd {
    Preload(PathBuf),
    Run(SessionJob),
    Transcribe(TranscribeJob),
    Unload(Sender<()>),
}

#[derive(Debug, Clone)]
pub enum StreamEvent {
    Partial { text: String, duration_ms: u64 },
    Final { text: String, duration_ms: u64 },
    Finished { duration_ms: u64 },
    Error(String),
}

static JOBS: OnceLock<Sender<WorkerCmd>> = OnceLock::new();

pub fn preload(model_path: PathBuf) {
    crate::journal::log("tone_preload", "queued");
    let _ = worker().send(WorkerCmd::Preload(model_path));
}

pub fn unload() {
    let Some(tx) = JOBS.get() else {
        return;
    };
    let (done, rx) = mpsc::channel();
    if tx.send(WorkerCmd::Unload(done)).is_ok() {
        let _ = rx.recv_timeout(std::time::Duration::from_secs(8));
    }
}

pub fn start_session(
    model_path: PathBuf,
    audio: Receiver<CapturedAudio>,
    cancellation: Arc<AtomicBool>,
) -> LfResult<Receiver<StreamEvent>> {
    if !model_path.is_file() {
        return Err(LfError::ModelMissing(model_path.display().to_string()));
    }
    let (events, event_rx) = mpsc::channel();
    crate::journal::log("tone_session", "queued");
    worker()
        .send(WorkerCmd::Run(SessionJob {
            model_path,
            audio,
            events,
            cancellation,
        }))
        .map_err(|_| LfError::RuntimeUnsupported("T-One worker stopped".into()))?;
    Ok(event_rx)
}

/// Used by file import and CLI paths. Live dictation uses `start_session`.
pub fn transcribe(model_path: &Path, pcm: &[f32]) -> LfResult<String> {
    if !model_path.is_file() {
        return Err(LfError::ModelMissing(model_path.display().to_string()));
    }
    let (reply, rx) = mpsc::channel();
    worker()
        .send(WorkerCmd::Transcribe(TranscribeJob {
            model_path: model_path.to_path_buf(),
            pcm: pcm.to_vec(),
            reply,
        }))
        .map_err(|_| LfError::RuntimeUnsupported("T-One worker stopped".into()))?;
    rx.recv_timeout(crate::whisper_stt::transcribe_wait(pcm.len().max(16_000)))
        .map_err(|_| LfError::RuntimeUnsupported("T-One recognition timed out.".into()))?
}

fn worker() -> Sender<WorkerCmd> {
    JOBS.get_or_init(|| {
        let (tx, rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("localflow-tone".into())
            .spawn(move || {
                let mut loaded: Option<Loaded> = None;
                while let Ok(cmd) = rx.recv() {
                    match cmd {
                        WorkerCmd::Preload(path) => {
                            if let Err(err) = ensure_loaded(&mut loaded, path) {
                                crate::diagnostics::error(
                                    "tone_preload",
                                    &format!("code={} detail={err}", err.code()),
                                );
                            } else {
                                crate::journal::log("tone_preload", "ready");
                            }
                        }
                        WorkerCmd::Unload(done) => {
                            loaded = None;
                            let _ = done.send(());
                        }
                        WorkerCmd::Transcribe(job) => {
                            let result =
                                crate::error::catch_runtime_panic("T-One recognition", || {
                                    run_once(&mut loaded, job.model_path, &job.pcm)
                                })
                                .and_then(|result| result);
                            let _ = job.reply.send(result);
                        }
                        WorkerCmd::Run(job) => run_session(&mut loaded, job),
                    }
                }
            })
            .expect("start T-One worker");
        tx
    })
    .clone()
}

fn silence_pcm(ms: u32) -> Vec<f32> {
    let n = (PCM_SAMPLE_RATE as u32).saturating_mul(ms) / 1000;
    vec![0.0; n as usize]
}

fn run_once(loaded: &mut Option<Loaded>, model_path: PathBuf, pcm: &[f32]) -> LfResult<String> {
    ensure_loaded(loaded, model_path)?;
    let recognizer = &loaded.as_ref().expect("T-One recognizer").recognizer;
    let stream = recognizer.create_stream();
    let idle = AtomicBool::new(false);
    stream.accept_waveform(PCM_SAMPLE_RATE, &silence_pcm(LEFT_PAD_MS));
    stream.accept_waveform(PCM_SAMPLE_RATE, pcm);
    stream.accept_waveform(PCM_SAMPLE_RATE, &silence_pcm(TAIL_PAD_MS));
    stream.input_finished();
    decode_ready(recognizer, &stream, &idle, DECODE_STEPS_FINAL);
    Ok(recognizer
        .get_result(&stream)
        .map(|result| result.text)
        .unwrap_or_default())
}

fn decode_ready(
    recognizer: &OnlineRecognizer,
    stream: &sherpa_onnx::OnlineStream,
    cancel: &AtomicBool,
    max_steps: u32,
) {
    let mut steps = 0u32;
    while recognizer.is_ready(stream) {
        if cancel.load(Ordering::Relaxed) {
            return;
        }
        if steps >= max_steps {
            crate::journal::log(
                "tone_decode_cap",
                &format!("stopped after {max_steps} decode steps"),
            );
            return;
        }
        recognizer.decode(stream);
        steps += 1;
        if recognizer.is_endpoint(stream) {
            return;
        }
    }
}

fn handle_endpoint(
    recognizer: &OnlineRecognizer,
    stream: &sherpa_onnx::OnlineStream,
    events: &Sender<StreamEvent>,
    last_partial: &mut String,
    duration_samples: u64,
) {
    if recognizer.is_endpoint(stream) {
        emit_result(
            recognizer,
            stream,
            events,
            last_partial,
            duration_samples,
            true,
        );
        recognizer.reset(stream);
        last_partial.clear();
    } else {
        emit_result(
            recognizer,
            stream,
            events,
            last_partial,
            duration_samples,
            false,
        );
    }
}

fn run_session(loaded: &mut Option<Loaded>, job: SessionJob) {
    let result = crate::error::catch_runtime_panic("T-One streaming recognition", || {
        ensure_loaded(loaded, job.model_path)?;
        crate::journal::log("tone_session", "decoder ready");
        let recognizer = &loaded.as_ref().expect("T-One recognizer").recognizer;
        let stream = recognizer.create_stream();
        let mut duration_samples = 0u64;
        let mut last_partial = String::new();
        stream.accept_waveform(PCM_SAMPLE_RATE, &silence_pcm(LEFT_PAD_MS));

        loop {
            if job.cancellation.load(Ordering::Relaxed) {
                return Err(LfError::Other("cancelled".into()));
            }
            match job.audio.recv_timeout(std::time::Duration::from_millis(40)) {
                Ok(chunk) => {
                    let pcm = crate::audio::to_whisper_pcm(&chunk);
                    duration_samples = duration_samples.saturating_add(pcm.len() as u64);
                    stream.accept_waveform(PCM_SAMPLE_RATE, &pcm);
                    decode_ready(
                        recognizer,
                        &stream,
                        &job.cancellation,
                        DECODE_STEPS_PER_CHUNK,
                    );
                    handle_endpoint(
                        recognizer,
                        &stream,
                        &job.events,
                        &mut last_partial,
                        duration_samples,
                    );
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
        if job.cancellation.load(Ordering::Relaxed) {
            return Err(LfError::Other("cancelled".into()));
        }
        crate::journal::log("tone_audio_closed", "input closed; final decode started");
        stream.accept_waveform(PCM_SAMPLE_RATE, &silence_pcm(TAIL_PAD_MS));
        stream.input_finished();
        decode_ready(recognizer, &stream, &job.cancellation, DECODE_STEPS_FINAL);
        emit_result(
            recognizer,
            &stream,
            &job.events,
            &mut last_partial,
            duration_samples,
            true,
        );
        Ok(duration_samples.saturating_mul(1000) / 16_000)
    })
    .and_then(|result| result);

    match result {
        Ok(duration_ms) => {
            let _ = job.events.send(StreamEvent::Finished { duration_ms });
        }
        Err(err) if err.to_string() != "cancelled" => {
            crate::diagnostics::error("tone_stream", &format!("code={} detail={err}", err.code()));
            let _ = job.events.send(StreamEvent::Error(err.to_string()));
        }
        Err(_) => {
            crate::journal::log(
                "tone_stream_cancelled",
                "decoder observed cancellation flag",
            );
            // A cancellation must still close the event channel promptly so
            // the dictation listener can release its busy state.
            let _ = job
                .events
                .send(StreamEvent::Error("T-One stream cancelled.".into()));
        }
    }
}

fn emit_result(
    recognizer: &OnlineRecognizer,
    stream: &sherpa_onnx::OnlineStream,
    events: &Sender<StreamEvent>,
    last_partial: &mut String,
    duration_samples: u64,
    finalize: bool,
) {
    let text = recognizer
        .get_result(stream)
        .map(|result| result.text.trim().to_string())
        .unwrap_or_default();
    if text.is_empty() || (!finalize && text == *last_partial) {
        return;
    }
    let duration_ms = duration_samples.saturating_mul(1000) / 16_000;
    let event = if finalize {
        StreamEvent::Final {
            text: text.clone(),
            duration_ms,
        }
    } else {
        StreamEvent::Partial {
            text: text.clone(),
            duration_ms,
        }
    };
    let _ = events.send(event);
    *last_partial = text;
}

fn ensure_loaded(loaded: &mut Option<Loaded>, model_path: PathBuf) -> LfResult<()> {
    if loaded.as_ref().is_some_and(|slot| slot.path == model_path) {
        return Ok(());
    }
    let parent = model_path
        .parent()
        .ok_or_else(|| LfError::ModelFormatInvalid("T-One model has no parent directory".into()))?;
    let tokens = parent.join("tokens.txt");
    if !tokens.is_file() {
        return Err(LfError::ModelMissing("T-One tokens.txt".into()));
    }
    let mut config = OnlineRecognizerConfig::default();
    config.feat_config.sample_rate = TONE_SAMPLE_RATE;
    config.model_config.t_one_ctc.model = Some(model_path.to_string_lossy().into_owned());
    config.model_config.tokens = Some(tokens.to_string_lossy().into_owned());
    // T-One's streaming state is large; extra ONNX threads have hung decode.
    config.model_config.num_threads = 1;
    config.model_config.provider = Some("cpu".into());
    config.decoding_method = Some("greedy_search".into());
    config.enable_endpoint = true;
    config.rule1_min_trailing_silence = 2.4;
    config.rule2_min_trailing_silence = 1.2;
    config.rule3_min_utterance_length = 20.0;
    let recognizer = OnlineRecognizer::create(&config).ok_or_else(|| {
        LfError::ModelFormatInvalid("Unable to initialize T-One streaming model".into())
    })?;
    *loaded = Some(Loaded {
        path: model_path,
        recognizer,
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_tone_tokens_are_reported_before_runtime_load() {
        let dir = tempfile::tempdir().unwrap();
        let model = dir.path().join("model.onnx");
        std::fs::write(&model, b"model").unwrap();
        let err = ensure_loaded(&mut None, model).unwrap_err();
        assert_eq!(err.code(), "MODEL_MISSING");
    }

    #[test]
    fn tone_decoder_is_configured_for_8khz_features() {
        let src = include_str!("tone_stt.rs");
        assert!(
            src.contains("feat_config.sample_rate = TONE_SAMPLE_RATE"),
            "T-One CTC expects 8 kHz features"
        );
        assert!(
            src.contains("accept_waveform(PCM_SAMPLE_RATE"),
            "16 kHz capture is passed through; sherpa resamples to 8 kHz"
        );
        assert!(src.contains("LEFT_PAD_MS"), "T-One needs 300 ms lead-in");
        assert!(
            src.contains("TAIL_PAD_MS"),
            "T-One needs 660 ms tail padding"
        );
        assert!(
            src.contains("DECODE_STEPS_FINAL"),
            "is_ready must be capped"
        );
    }

    #[test]
    #[ignore = "requires the official T-One download path in LOCALFLOW_TONE_MODEL"]
    fn official_tone_model_initializes() {
        let model = std::env::var_os("LOCALFLOW_TONE_MODEL")
            .map(PathBuf::from)
            .expect("set LOCALFLOW_TONE_MODEL to model.onnx");
        ensure_loaded(&mut None, model).expect("official T-One model initializes");
    }

    #[test]
    #[ignore = "requires the official T-One download path in LOCALFLOW_TONE_MODEL"]
    fn official_tone_model_transcribes_its_sample() {
        let model = std::env::var_os("LOCALFLOW_TONE_MODEL")
            .map(PathBuf::from)
            .expect("set LOCALFLOW_TONE_MODEL to model.onnx");
        let sample = model.parent().expect("model parent").join("0.wav");
        let pcm = crate::media::load_pcm_16k_mono(&sample).expect("decode supplied sample");
        let text = run_once(&mut None, model, &pcm).expect("recognize supplied sample");
        assert!(
            !text.trim().is_empty(),
            "the supplied sample has a transcript"
        );
    }

    #[test]
    #[ignore = "requires LOCALFLOW_TONE_MODEL and LOCALFLOW_TONE_AUDIO"]
    fn official_tone_stream_finishes_after_microphone_chunks_close() {
        let model = std::env::var_os("LOCALFLOW_TONE_MODEL")
            .map(PathBuf::from)
            .expect("set LOCALFLOW_TONE_MODEL to model.onnx");
        let audio = std::env::var_os("LOCALFLOW_TONE_AUDIO")
            .map(PathBuf::from)
            .expect("set LOCALFLOW_TONE_AUDIO to a speech wav");
        let pcm = crate::media::load_pcm_16k_mono(&audio).expect("decode speech wav");
        crate::dictation::clear_cancel();
        let (tx, rx) = mpsc::sync_channel(32);
        let events = start_session(model, rx, Arc::new(AtomicBool::new(false)))
            .expect("start streaming session");
        for samples in pcm.chunks(1_600) {
            tx.send(CapturedAudio {
                samples: samples.to_vec(),
                sample_rate: 16_000,
                channels: 1,
            })
            .expect("send microphone chunk");
        }
        drop(tx);

        let mut final_text = String::new();
        let mut finished = false;
        while let Ok(event) = events.recv_timeout(std::time::Duration::from_secs(20)) {
            match event {
                StreamEvent::Final { text, .. } => final_text.push_str(&text),
                StreamEvent::Finished { .. } => {
                    finished = true;
                    break;
                }
                StreamEvent::Error(err) => panic!("streaming error: {err}"),
                StreamEvent::Partial { .. } => {}
            }
        }
        assert!(finished, "the closed microphone stream must finish");
        assert!(
            !final_text.trim().is_empty(),
            "speech must produce a final result"
        );
    }
}
