use crate::error::{LfError, LfResult};
use crate::pipeline::TranscriptCue;
use std::ffi::c_void;
use std::os::raw::c_char;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Mutex, Once, OnceLock};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

static SKIP_PARTIAL: AtomicBool = AtomicBool::new(false);
static BUSY: AtomicBool = AtomicBool::new(false);
static LAST_PARTIAL: OnceLock<Mutex<Option<String>>> = OnceLock::new();

struct TranscribeJob {
    model_path: PathBuf,
    pcm: Vec<f32>,
    language: String,
    reply: Sender<LfResult<String>>,
    partial: bool,
}

fn last_partial_slot() -> &'static Mutex<Option<String>> {
    LAST_PARTIAL.get_or_init(|| Mutex::new(None))
}

pub fn last_partial() -> Option<String> {
    last_partial_slot().lock().ok().and_then(|g| g.clone())
}

pub fn clear_partial() {
    if let Ok(mut slot) = last_partial_slot().lock() {
        *slot = None;
    }
}

pub fn allow_partial() {
    SKIP_PARTIAL.store(false, Ordering::Relaxed);
    clear_partial();
}

pub fn try_partial(model_path: &Path, pcm: &[f32], language: &str) {
    if SKIP_PARTIAL.load(Ordering::Relaxed) || BUSY.load(Ordering::Relaxed) {
        return;
    }
    if pcm.len() < 16_000 || !model_path.is_file() {
        return;
    }
    let tx = worker();
    let (reply_tx, reply_rx) = mpsc::channel();
    let job = TranscribeJob {
        model_path: model_path.to_path_buf(),
        pcm: pad_to_whisper_window(&pcm[pcm.len().saturating_sub(8 * 16_000)..]),
        language: language.to_string(),
        reply: reply_tx,
        partial: true,
    };
    if tx.send(job).is_err() {
        return;
    }
    std::thread::spawn(move || {
        if let Ok(Ok(text)) = reply_rx.recv() {
            if !text.is_empty() {
                if let Ok(mut slot) = last_partial_slot().lock() {
                    *slot = Some(text);
                }
            }
        }
    });
}

static JOBS: OnceLock<Sender<TranscribeJob>> = OnceLock::new();
static LAST_CUES: OnceLock<Mutex<Vec<TranscriptCue>>> = OnceLock::new();

fn cues_slot() -> &'static Mutex<Vec<TranscriptCue>> {
    LAST_CUES.get_or_init(|| Mutex::new(Vec::new()))
}

pub fn last_cues() -> Vec<TranscriptCue> {
    cues_slot().lock().map(|g| g.clone()).unwrap_or_default()
}

pub fn transcribe(
    model_path: &Path,
    pcm: &[f32],
    _cancel: &std::sync::atomic::AtomicBool,
    language: &str,
) -> LfResult<String> {
    if !model_path.is_file() {
        return Err(LfError::ModelMissing(model_path.display().to_string()));
    }
    if pcm.is_empty() {
        return Ok(String::new());
    }
    SKIP_PARTIAL.store(true, Ordering::Relaxed);
    let tx = worker();
    let (reply_tx, reply_rx) = mpsc::channel();
    tx.send(TranscribeJob {
        model_path: model_path.to_path_buf(),
        pcm: pad_to_whisper_window(pcm),
        language: language.to_string(),
        reply: reply_tx,
        partial: false,
    })
    .map_err(|_| LfError::RuntimeUnsupported("whisper worker stopped".into()))?;
    let result = reply_rx
        .recv()
        .map_err(|_| LfError::RuntimeUnsupported("whisper worker stopped".into()))?;
    SKIP_PARTIAL.store(false, Ordering::Relaxed);
    result
}

fn worker() -> Sender<TranscribeJob> {
    JOBS.get_or_init(|| {
        silence_whisper_logs();
        let (tx, rx) = mpsc::channel::<TranscribeJob>();
        std::thread::Builder::new()
            .name("localflow-whisper".into())
            .spawn(move || {
                let mut loaded: Option<(PathBuf, WhisperContext)> = None;
                while let Ok(job) = rx.recv() {
                    if job.partial && SKIP_PARTIAL.load(Ordering::Relaxed) {
                        let _ = job.reply.send(Ok(String::new()));
                        continue;
                    }
                    BUSY.store(true, Ordering::Relaxed);
                    let result = run_job(&mut loaded, job.model_path, &job.pcm, &job.language);
                    BUSY.store(false, Ordering::Relaxed);
                    let _ = job.reply.send(result);
                }
            })
            .expect("start whisper worker");
        tx
    })
    .clone()
}

fn run_job(
    loaded: &mut Option<(PathBuf, WhisperContext)>,
    model_path: PathBuf,
    pcm: &[f32],
    language: &str,
) -> LfResult<String> {
    let needs_reload = match loaded {
        Some((path, _)) => path != &model_path,
        None => true,
    };
    if needs_reload {
        let path = model_path
            .to_str()
            .ok_or_else(|| LfError::Other("model path is not UTF-8".into()))?;
        eprintln!("localflow: loading whisper model {}", model_path.display());
        let ctx = WhisperContext::new_with_params(path, WhisperContextParameters::default())
            .map_err(|err| LfError::RuntimeUnsupported(format!("whisper.cpp: {err}")))?;
        *loaded = Some((model_path, ctx));
    }
    let ctx = &loaded.as_ref().expect("whisper context").1;
    decode(ctx, pcm, language)
}

fn pad_to_whisper_window(pcm: &[f32]) -> Vec<f32> {
    // whisper.cpp skips (or fails language detect on) clips shorter than 1s.
    const MIN: usize = 16_000;
    if pcm.len() >= MIN {
        return pcm.to_vec();
    }
    let mut out = vec![0.0; MIN];
    out[..pcm.len()].copy_from_slice(pcm);
    out
}

fn decode(ctx: &WhisperContext, pcm: &[f32], language: &str) -> LfResult<String> {
    let mut state = ctx
        .create_state()
        .map_err(|err| LfError::RuntimeUnsupported(err.to_string()))?;
    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_n_threads(num_threads());
    params.set_translate(false);
    let lang = language.trim();
    // detect_language=true means "identify language and return" — no ASR.
    // Auto-detect for transcription is language "auto" / null with that flag off.
    if lang.is_empty() || lang.eq_ignore_ascii_case("auto") {
        params.set_language(Some("auto"));
        params.set_detect_language(false);
    } else {
        params.set_language(Some(lang));
        params.set_detect_language(false);
    }
    params.set_print_special(false);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);
    params.set_no_timestamps(true);
    params.set_suppress_blank(true);
    params.set_suppress_non_speech_tokens(true);
    params.set_no_speech_thold(0.6);
    params.set_no_context(true);
    params.set_abort_callback_safe(crate::dictation::is_cancelled);
    state
        .full(params, pcm)
        .map_err(|err| LfError::RuntimeUnsupported(err.to_string()))?;
    if crate::dictation::is_cancelled() {
        return Err(LfError::Other("cancelled".into()));
    }
    let n = state
        .full_n_segments()
        .map_err(|err| LfError::RuntimeUnsupported(err.to_string()))?;
    let mut out = String::new();
    let mut cues = Vec::new();
    for i in 0..n {
        if let Ok(seg) = state.full_get_segment_text(i) {
            let text = crate::sanitize::strip_model_tags(&seg);
            if text.is_empty() {
                continue;
            }
            let t0 = state.full_get_segment_t0(i).unwrap_or(0).max(0) as u64 * 10;
            let t1 = state.full_get_segment_t1(i).unwrap_or(0).max(0) as u64 * 10;
            cues.push(TranscriptCue {
                start_ms: t0,
                end_ms: t1.max(t0),
                text: text.clone(),
            });
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(&text);
        }
    }
    if let Ok(mut slot) = cues_slot().lock() {
        *slot = cues;
    }
    let cleaned = crate::sanitize::strip_model_tags(&out);
    if crate::sanitize::is_likely_hallucination(&cleaned) {
        return Ok(String::new());
    }
    Ok(cleaned)
}

fn silence_whisper_logs() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| unsafe {
        whisper_rs::set_log_callback(Some(quiet_whisper_log), std::ptr::null_mut());
    });
}

unsafe extern "C" fn quiet_whisper_log(
    _level: std::os::raw::c_uint,
    _text: *const c_char,
    _user_data: *mut c_void,
) {
}

fn num_threads() -> std::ffi::c_int {
    std::thread::available_parallelism()
        .map(|n| n.get().clamp(1, 8) as std::ffi::c_int)
        .unwrap_or(2)
}

#[cfg(test)]
mod tests {
    use super::pad_to_whisper_window;

    #[test]
    fn pads_short_clips_to_one_second() {
        let pcm = vec![0.1; 800];
        let padded = pad_to_whisper_window(&pcm);
        assert_eq!(padded.len(), 16_000);
        assert!((padded[0] - 0.1).abs() < f32::EPSILON);
        assert_eq!(padded[800], 0.0);
    }
}
