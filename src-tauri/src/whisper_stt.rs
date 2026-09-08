use crate::error::{LfError, LfResult};
use crate::pipeline::TranscriptCue;
use std::ffi::c_void;
use std::os::raw::c_char;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::sync::{Mutex, Once, OnceLock};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

/// Knobs that change what the recognizer is willing to emit.
#[derive(Debug, Clone, Default)]
pub struct DecodeOptions {
    /// Vocabulary shown to the model before decoding. Whisper conditions on it,
    /// which is what makes it spell "RestAssured" or "PostgreSQL" instead of a
    /// phonetic guess.
    pub prompt: String,
    /// Whisper's non-speech suppression list also covers `/`, `_`, `#`, and
    /// brackets, so it has to be lifted to dictate code and paths.
    pub allow_symbols: bool,
}

struct TranscribeJob {
    model_path: PathBuf,
    pcm: Vec<f32>,
    language: String,
    options: DecodeOptions,
    reply: Sender<LfResult<String>>,
}

enum WorkerCmd {
    Transcribe(TranscribeJob),
    Unload(Sender<()>),
}

static JOBS: OnceLock<Sender<WorkerCmd>> = OnceLock::new();
static LAST_CUES: OnceLock<Mutex<Vec<TranscriptCue>>> = OnceLock::new();

fn cues_slot() -> &'static Mutex<Vec<TranscriptCue>> {
    LAST_CUES.get_or_init(|| Mutex::new(Vec::new()))
}

pub fn last_cues() -> Vec<TranscriptCue> {
    cues_slot().lock().map(|g| g.clone()).unwrap_or_default()
}

pub fn store_cues(cues: Vec<TranscriptCue>) {
    if let Ok(mut slot) = cues_slot().lock() {
        *slot = cues;
    }
}

/// Load the ggml file into the worker so the first dictation is not a cold mmap.
pub fn preload(model_path: PathBuf) {
    std::thread::Builder::new()
        .name("localflow-whisper-preload".into())
        .spawn(move || {
            let _ = transcribe(
                &model_path,
                &[0.0; 16_000],
                crate::dictation::cancel_flag(),
                "ru",
                &DecodeOptions::default(),
            );
        })
        .ok();
}

pub fn transcribe(
    model_path: &Path,
    pcm: &[f32],
    _cancel: &std::sync::atomic::AtomicBool,
    language: &str,
    options: &DecodeOptions,
) -> LfResult<String> {
    if !model_path.is_file() {
        return Err(LfError::ModelMissing(model_path.display().to_string()));
    }
    if pcm.is_empty() {
        return Ok(String::new());
    }
    store_cues(Vec::new());
    let tx = worker();
    let (reply_tx, reply_rx) = mpsc::channel();
    tx.send(WorkerCmd::Transcribe(TranscribeJob {
        model_path: model_path.to_path_buf(),
        pcm: pad_to_whisper_window(pcm),
        language: language.to_string(),
        options: options.clone(),
        reply: reply_tx,
    }))
    .map_err(|_| LfError::RuntimeUnsupported("whisper worker stopped".into()))?;
    reply_rx
        .recv()
        .map_err(|_| LfError::RuntimeUnsupported("whisper worker stopped".into()))?
}

/// Drop the mmap'd ggml so uninstall can delete model files.
pub fn unload() {
    let Some(tx) = JOBS.get() else {
        return;
    };
    let (done, rx) = mpsc::channel();
    if tx.send(WorkerCmd::Unload(done)).is_err() {
        return;
    }
    let _ = rx.recv_timeout(std::time::Duration::from_secs(8));
}

fn worker() -> Sender<WorkerCmd> {
    JOBS.get_or_init(|| {
        silence_whisper_logs();
        let (tx, rx) = mpsc::channel::<WorkerCmd>();
        std::thread::Builder::new()
            .name("localflow-whisper".into())
            .spawn(move || {
                let mut loaded: Option<(PathBuf, WhisperContext)> = None;
                while let Ok(cmd) = rx.recv() {
                    match cmd {
                        WorkerCmd::Unload(done) => {
                            loaded = None;
                            let _ = done.send(());
                        }
                        WorkerCmd::Transcribe(job) => {
                            let result = run_job(
                                &mut loaded,
                                job.model_path,
                                &job.pcm,
                                &job.language,
                                &job.options,
                            );
                            let _ = job.reply.send(result);
                        }
                    }
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
    options: &DecodeOptions,
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
    decode(ctx, pcm, language, options)
}

/// whisper.cpp only reads `initial_prompt` when `prompt_tokens` is null, and it
/// only keeps a prompt at all while the decoding temperature stays below 0.5.
/// Keep the text short enough to survive both.
pub const MAX_PROMPT_CHARS: usize = 400;

fn prompt_for(options: &DecodeOptions) -> String {
    let mut prompt = options.prompt.trim().replace('\0', " ");
    if prompt.chars().count() > MAX_PROMPT_CHARS {
        let cut = prompt
            .char_indices()
            .nth(MAX_PROMPT_CHARS)
            .map(|(i, _)| i)
            .unwrap_or(prompt.len());
        prompt.truncate(cut);
    }
    prompt
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

fn decode(
    ctx: &WhisperContext,
    pcm: &[f32],
    language: &str,
    options: &DecodeOptions,
) -> LfResult<String> {
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
    // Token timestamps roughly double CPU time. Paragraphs already come from VAD.
    params.set_no_timestamps(true);
    params.set_single_segment(pcm.len() < 16_000 * 15);
    params.set_suppress_blank(true);
    // Whisper's non-speech suppression also covers `/`, `_`, `#`, and brackets,
    // which are exactly the characters technical dictation needs.
    params.set_suppress_non_speech_tokens(!options.allow_symbols);
    params.set_no_speech_thold(0.6);
    // Clears context carried over from the previous utterance. It is applied
    // before the initial prompt is seeded, so the two are compatible.
    params.set_no_context(true);
    // Deliberately no `set_tokens` call: whisper.cpp ignores `initial_prompt`
    // whenever `prompt_tokens` is non-null, and an empty slice still yields a
    // non-null pointer, which silently disabled prompting.
    let prompt = prompt_for(options);
    if !prompt.is_empty() {
        params.set_initial_prompt(&prompt);
    }
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
        }
    }
    out.push_str(&crate::pipeline::join_cues_with_pauses(
        &cues,
        crate::pipeline::PARAGRAPH_PAUSE_MS,
    ));
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
    _level: whisper_rs_sys::ggml_log_level,
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
