use crate::error::{LfError, LfResult};
use crate::pipeline::TranscriptCue;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, Once, OnceLock};
use std::time::{Duration, Instant};
use whisper_rs::{
    FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperState,
    WhisperVadParams,
};

/// Knobs that change what the recognizer is willing to emit.
#[derive(Debug, Clone, Default)]
pub struct DecodeOptions {
    /// Runtime selected in Settings. Whisper remains the default.
    pub stt_engine: String,
    /// Vocabulary shown to the model before decoding. Whisper conditions on it,
    /// which is what makes it spell "RestAssured" or "PostgreSQL" instead of a
    /// phonetic guess.
    pub prompt: String,
    /// Whisper's non-speech suppression list also covers `/`, `_`, `#`, and
    /// brackets, so it has to be lifted to dictate code and paths.
    pub allow_symbols: bool,
    /// ggml Silero VAD model. When present whisper.cpp keeps only speech
    /// segments before encoding, which trims noise and shortens the encoder pass.
    pub vad_model: Option<PathBuf>,
    /// Segment timestamps for file / interview transcripts.
    pub timestamps: bool,
    /// Long recording: keep all windows, do not bias with the dictation dictionary.
    pub long_form: bool,
    /// Energy threshold used before decoding each long-form window.
    pub vad_threshold: f32,
}

impl DecodeOptions {
    /// File / interview recognition: no dictionary prompt, skip Whisper
    /// timestamp tokens (times come from energy in the window), keep windows
    /// without overlap.
    pub fn long_form_interview() -> Self {
        Self {
            stt_engine: "whisper".into(),
            prompt: String::new(),
            allow_symbols: false,
            vad_model: None,
            timestamps: false,
            long_form: true,
            vad_threshold: crate::vad::default_threshold(),
        }
    }
}

struct TranscribeJob {
    model_path: PathBuf,
    pcm: Vec<f32>,
    language: String,
    options: DecodeOptions,
    cancellation: Cancellation,
    reply: Sender<LfResult<String>>,
}

#[derive(Clone, Debug)]
pub enum Cancellation {
    Dictation,
    File(Arc<AtomicBool>),
}

impl Cancellation {
    pub(crate) fn is_cancelled(&self) -> bool {
        match self {
            Self::Dictation => crate::dictation::is_cancelled(),
            Self::File(token) => token.load(Ordering::Relaxed),
        }
    }
}

enum WorkerCmd {
    Transcribe(TranscribeJob),
    Preload(PathBuf),
    Unload(Sender<()>),
}

/// Model, its GPU flag, and one reusable decode state. `whisper_init_state`
/// allocates the KV cache (hundreds of MB for Medium) and caches the VAD
/// context, so creating it per utterance was pure overhead.
struct Loaded {
    path: PathBuf,
    gpu: bool,
    // Owns the native allocation referenced by `state`.
    _ctx: WhisperContext,
    state: WhisperState,
}

static JOBS: OnceLock<Sender<WorkerCmd>> = OnceLock::new();
static LAST_CUES: OnceLock<Mutex<Vec<TranscriptCue>>> = OnceLock::new();
static USE_GPU: AtomicBool = AtomicBool::new(false);
/// Set when whisper.cpp rejects the VAD model, so the session falls back to
/// plain decoding instead of failing every utterance.
static VAD_BROKEN: AtomicBool = AtomicBool::new(false);
/// Some whisper.cpp CPU backends reject a reduced encoder context with -6.
/// Once observed, keep the upstream full context for the rest of the session.
static DYNAMIC_AUDIO_CTX_BROKEN: AtomicBool = AtomicBool::new(false);
static CHUNK_INDEX: AtomicU32 = AtomicU32::new(0);
static CHUNK_COUNT: AtomicU32 = AtomicU32::new(1);
static AUDIO_MS: AtomicU32 = AtomicU32::new(0);
static LAST_PROGRESS: OnceLock<Mutex<TranscribeProgress>> = OnceLock::new();
static LAST_EMIT_AT: OnceLock<Mutex<Option<Instant>>> = OnceLock::new();

#[derive(Debug, Clone, Serialize)]
pub struct TranscribeProgress {
    pub phase: String,
    pub percent: u8,
    pub chunk: u32,
    pub chunks: u32,
    pub audio_ms: u64,
    pub message: String,
}

impl Default for TranscribeProgress {
    fn default() -> Self {
        Self {
            phase: "idle".into(),
            percent: 0,
            chunk: 0,
            chunks: 1,
            audio_ms: 0,
            message: String::new(),
        }
    }
}

pub fn current_progress() -> TranscribeProgress {
    LAST_PROGRESS
        .get_or_init(|| Mutex::new(TranscribeProgress::default()))
        .lock()
        .map(|g| g.clone())
        .unwrap_or_default()
}

pub fn set_progress(phase: &str, percent: u8, message: impl Into<String>) {
    let chunk = CHUNK_INDEX.load(Ordering::Relaxed);
    let chunks = CHUNK_COUNT.load(Ordering::Relaxed).max(1);
    let audio_ms = u64::from(AUDIO_MS.load(Ordering::Relaxed));
    let percent = percent.min(100);
    let progress = TranscribeProgress {
        phase: phase.into(),
        percent,
        chunk,
        chunks,
        audio_ms,
        message: message.into(),
    };
    if let Ok(mut slot) = LAST_PROGRESS
        .get_or_init(|| Mutex::new(TranscribeProgress::default()))
        .lock()
    {
        *slot = progress.clone();
    }
    let force = phase != "recognize" || percent == 0 || percent >= 100;
    if !force {
        let stamp = LAST_EMIT_AT.get_or_init(|| Mutex::new(None));
        if let Ok(mut last) = stamp.lock() {
            if let Some(prev) = *last {
                if prev.elapsed() < Duration::from_millis(80) {
                    return;
                }
            }
            *last = Some(Instant::now());
        }
    }
    crate::dictation::emit_transcribe_progress(progress);
}

fn note_inner_progress(inner: u8) {
    let chunks = CHUNK_COUNT.load(Ordering::Relaxed).max(1);
    let chunk = CHUNK_INDEX
        .load(Ordering::Relaxed)
        .min(chunks.saturating_sub(1));
    let span = 100 / chunks;
    let percent = (chunk * span + u32::from(inner) * span / 100).min(99) as u8;
    set_progress(
        "recognize",
        percent,
        format!("Recognizing audio… {}/{} ({}%)", chunk + 1, chunks, percent),
    );
}

/// How long the UI thread waits for whisper.cpp. Medium on CPU is often much
/// slower than realtime, so a fixed 3-minute cap aborted long file jobs.
pub fn transcribe_wait(n_samples: usize) -> Duration {
    let audio_secs = (n_samples as u64 / 16_000).max(1);
    let scaled = audio_secs.saturating_mul(12).saturating_add(180);
    Duration::from_secs(scaled.clamp(180, 45 * 60))
}

pub const WINDOW_SAMPLES: usize = 16_000 * 30;
/// Long-form file transcription steps by a full window. Dictation uses the
/// smaller overlap below because it must preserve words at an utterance seam.
pub const HOP_SAMPLES: usize = WINDOW_SAMPLES;
/// A held hotkey recording can be longer than one Whisper encoder window.
/// Keep a small overlap there so a word spoken across a 30-second boundary is
/// decoded in a complete context. The text stitcher removes only a matching
/// prefix, never a merely similar sentence.
pub const DICTATION_OVERLAP_SAMPLES: usize = 16_000 * 2;

pub fn window_ranges(len: usize) -> Vec<(usize, usize)> {
    window_ranges_with_hop(len, HOP_SAMPLES)
}

pub fn dictation_window_ranges(len: usize) -> Vec<(usize, usize)> {
    window_ranges_with_hop(
        len,
        WINDOW_SAMPLES.saturating_sub(DICTATION_OVERLAP_SAMPLES),
    )
}

pub fn window_ranges_with_hop(len: usize, hop: usize) -> Vec<(usize, usize)> {
    if len == 0 {
        return Vec::new();
    }
    if len <= WINDOW_SAMPLES {
        return vec![(0, len)];
    }
    let hop = hop.max(16_000);
    let mut out = Vec::new();
    let mut offset = 0usize;
    while offset < len {
        let end = (offset + WINDOW_SAMPLES).min(len);
        if end > offset {
            out.push((offset, end));
        }
        if end == len {
            break;
        }
        offset += hop;
    }
    out
}

/// Metal is compiled only for Apple Silicon. Intel/Windows/Linux stay on CPU.
pub fn gpu_compiled() -> bool {
    cfg!(all(target_os = "macos", target_arch = "aarch64"))
}

pub fn use_gpu_from_setting(compute: &str) -> bool {
    if !gpu_compiled() {
        return false;
    }
    matches!(
        compute.trim().to_ascii_lowercase().as_str(),
        "auto" | "gpu" | "metal"
    )
}

pub fn set_use_gpu(on: bool) {
    let previous = USE_GPU.swap(on, Ordering::Relaxed);
    if previous != on {
        unload();
    }
}

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
    let tx = worker();
    let _ = tx.send(WorkerCmd::Preload(model_path));
}

pub fn transcribe(
    model_path: &Path,
    pcm: &[f32],
    cancellation: Cancellation,
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
    let owned = pad_to_whisper_window(pcm);
    AUDIO_MS.store(
        ((owned.len() as u64 * 1000) / 16_000).min(u64::from(u32::MAX)) as u32,
        Ordering::Relaxed,
    );
    let tx = worker();
    let (reply_tx, reply_rx) = mpsc::channel();
    tx.send(WorkerCmd::Transcribe(TranscribeJob {
        model_path: model_path.to_path_buf(),
        pcm: owned,
        language: language.to_string(),
        options: options.clone(),
        cancellation: cancellation.clone(),
        reply: reply_tx,
    }))
    .map_err(|_| LfError::RuntimeUnsupported("whisper worker stopped".into()))?;
    wait_for_transcription(
        reply_rx,
        &cancellation,
        transcribe_wait(pcm.len().max(16_000)),
    )
}

fn wait_for_transcription(
    reply_rx: mpsc::Receiver<LfResult<String>>,
    cancellation: &Cancellation,
    timeout: Duration,
) -> LfResult<String> {
    let deadline = Instant::now() + timeout;
    loop {
        if cancellation.is_cancelled() {
            return Err(LfError::Other("cancelled".into()));
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(LfError::RuntimeUnsupported(
                "Speech recognition timed out. On Intel Macs use Whisper Small or Base.".into(),
            ));
        }
        match reply_rx.recv_timeout(remaining.min(Duration::from_millis(50))) {
            Ok(_) if cancellation.is_cancelled() => return Err(LfError::Other("cancelled".into())),
            Ok(result) => return result,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(LfError::RuntimeUnsupported("whisper worker stopped".into()))
            }
        }
    }
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
                let mut loaded: Option<Loaded> = None;
                while let Ok(cmd) = rx.recv() {
                    match cmd {
                        WorkerCmd::Unload(done) => {
                            loaded = None;
                            let _ = done.send(());
                        }
                        WorkerCmd::Preload(path) => {
                            if crate::dictation::tone_engine_active() {
                                loaded = None;
                            } else {
                                let _ = ensure_loaded(&mut loaded, path);
                            }
                        }
                        WorkerCmd::Transcribe(job) => {
                            let result =
                                crate::error::catch_runtime_panic("Speech recognition", || {
                                    run_job(
                                        &mut loaded,
                                        job.model_path,
                                        &job.pcm,
                                        &job.language,
                                        &job.options,
                                        &job.cancellation,
                                    )
                                })
                                .and_then(|result| result);
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
    loaded: &mut Option<Loaded>,
    model_path: PathBuf,
    pcm: &[f32],
    language: &str,
    options: &DecodeOptions,
    cancellation: &Cancellation,
) -> LfResult<String> {
    ensure_loaded(loaded, model_path)?;
    let windows = if options.long_form {
        window_ranges(pcm.len())
    } else {
        dictation_window_ranges(pcm.len())
    };
    CHUNK_COUNT.store(windows.len().max(1) as u32, Ordering::Relaxed);
    CHUNK_INDEX.store(0, Ordering::Relaxed);
    let mut texts: Vec<String> = Vec::new();
    let mut cues: Vec<crate::pipeline::TranscriptCue> = Vec::new();
    for (i, (start, end)) in windows.iter().copied().enumerate() {
        if cancellation.is_cancelled() {
            return Err(LfError::Other("cancelled".into()));
        }
        CHUNK_INDEX.store(i as u32, Ordering::Relaxed);
        note_inner_progress(0);
        if !crate::vad::had_speech_at(&pcm[start..end], 16_000, options.vad_threshold) {
            note_inner_progress(100);
            continue;
        }
        let text = decode_loaded(loaded, &pcm[start..end], language, options, cancellation)?;
        let offset_ms = (start as u64 * 1000) / 16_000;
        let mut window_cues = last_cues();
        if options.long_form && !options.timestamps {
            crate::vad::spread_cues_over_speech(&mut window_cues, &pcm[start..end], 16_000);
        }
        let trimmed = if options.long_form {
            crate::sanitize::collapse_long_form_text(text.trim())
        } else {
            crate::sanitize::collapse_echoed_transcript(text.trim())
        };
        let skip_noise = trimmed.is_empty()
            || (options.long_form
                && (crate::sanitize::is_likely_hallucination(&trimmed)
                    || crate::sanitize::is_degenerate_repetition(&trimmed)));
        if skip_noise {
            note_inner_progress(100);
            continue;
        }
        if options.long_form {
            if texts.last().is_some_and(|prev| prev == &trimmed) {
                note_inner_progress(100);
                continue;
            }
        } else if let Some(prev) = texts.last_mut() {
            if crate::sanitize::is_same_or_truncated_echo(prev, &trimmed) {
                if trimmed.chars().count() > prev.chars().count() {
                    *prev = trimmed;
                }
                continue;
            }
            let stitched = crate::sanitize::strip_overlapping_prefix(prev, &trimmed);
            if stitched.is_empty() {
                note_inner_progress(100);
                continue;
            }
            texts.push(stitched);
            note_inner_progress(100);
            continue;
        }
        for mut cue in window_cues {
            cue.start_ms = cue.start_ms.saturating_add(offset_ms);
            cue.end_ms = cue.end_ms.saturating_add(offset_ms);
            cues.push(cue);
        }
        texts.push(trimmed);
    }
    let cues = if options.long_form {
        crate::pipeline::merge_long_form_cues(&cues)
    } else {
        cues
    };
    store_cues(cues.clone());
    set_progress("recognize", 100, "Recognition finished");
    if options.long_form {
        Ok(crate::pipeline::join_cues_with_pauses(
            &cues,
            crate::pipeline::PARAGRAPH_PAUSE_MS,
        ))
    } else {
        Ok(crate::sanitize::collapse_echoed_transcript(
            &texts.join(" "),
        ))
    }
}

fn decode_loaded(
    loaded: &mut Option<Loaded>,
    pcm: &[f32],
    language: &str,
    options: &DecodeOptions,
    cancellation: &Cancellation,
) -> LfResult<String> {
    // `no_context` is set for every decode below, so the state can be reused
    // safely between utterances without reallocating the large KV cache.
    let vad = options
        .vad_model
        .as_deref()
        .filter(|p| p.is_file() && native_vad_supported() && !VAD_BROKEN.load(Ordering::Relaxed))
        .map(Path::to_path_buf);
    let force_full_audio_ctx = DYNAMIC_AUDIO_CTX_BROKEN.load(Ordering::Relaxed);
    let mut result = {
        let slot = loaded.as_mut().expect("whisper context");
        decode(
            &mut slot.state,
            pcm,
            language,
            options,
            vad.as_deref(),
            cancellation,
            force_full_audio_ctx,
        )
    };
    if let Err(err) = &result {
        if vad.is_some() && !cancellation.is_cancelled() {
            // whisper.cpp keeps the VAD context on the Whisper context, not
            // only on the decode state. Reload the model before the plain
            // retry so a rejected VAD cannot poison subsequent utterances.
            eprintln!("localflow: whisper VAD failed ({err}); decoding without VAD");
            VAD_BROKEN.store(true, Ordering::Relaxed);
            reload_loaded(loaded)?;
            let slot = loaded.as_mut().expect("whisper context");
            result = decode(
                &mut slot.state,
                pcm,
                language,
                options,
                None,
                cancellation,
                force_full_audio_ctx,
            );
        }
    }
    if !force_full_audio_ctx
        && !cancellation.is_cancelled()
        && result.as_ref().is_err_and(|err| is_encoder_failure(err))
    {
        // A reduced `audio_ctx` is an optimization, not a correctness
        // requirement. whisper.cpp reports encoder backend failures as -6;
        // retry from a clean state with its normal 30-second context.
        eprintln!("localflow: reduced whisper audio context failed; retrying full context");
        reload_loaded(loaded)?;
        let slot = loaded.as_mut().expect("whisper context");
        result = decode(
            &mut slot.state,
            pcm,
            language,
            options,
            None,
            cancellation,
            true,
        );
        if result.is_ok() {
            DYNAMIC_AUDIO_CTX_BROKEN.store(true, Ordering::Relaxed);
        }
    }
    result
}

fn is_encoder_failure(err: &LfError) -> bool {
    matches!(err, LfError::RuntimeUnsupported(detail) if detail.contains("Error code: -6"))
}

/// whisper.cpp's integrated Silero path can leave the CPU backend unusable
/// after an encoder -6 on Intel macOS. LocalFlow still applies its energy VAD
/// before decode there; only the native Silero preprocessing is skipped.
fn native_vad_supported() -> bool {
    !cfg!(all(target_os = "macos", target_arch = "x86_64"))
}

fn ensure_loaded(loaded: &mut Option<Loaded>, model_path: PathBuf) -> LfResult<()> {
    if crate::dictation::tone_engine_active() {
        *loaded = None;
        return Err(LfError::RuntimeUnsupported(
            "Whisper is paused while T-One streaming is active.".into(),
        ));
    }
    let use_gpu = USE_GPU.load(Ordering::Relaxed) && gpu_compiled();
    let needs_reload = match loaded {
        Some(l) => l.path != model_path || l.gpu != use_gpu,
        None => true,
    };
    if needs_reload {
        // Drop the old model before allocating the replacement. Medium takes
        // enough memory that briefly holding two contexts can make the encoder
        // allocation fail on Intel Macs.
        *loaded = None;
        *loaded = Some(load_model(model_path, use_gpu)?);
    }
    Ok(())
}

fn reload_loaded(loaded: &mut Option<Loaded>) -> LfResult<()> {
    let slot = loaded.as_ref().expect("whisper context");
    let path = slot.path.clone();
    let gpu = slot.gpu;
    *loaded = None;
    *loaded = Some(load_model(path, gpu)?);
    Ok(())
}

fn load_model(model_path: PathBuf, use_gpu: bool) -> LfResult<Loaded> {
    let path = model_path
        .to_str()
        .ok_or_else(|| LfError::Other("model path is not UTF-8".into()))?;
    eprintln!(
        "localflow: loading whisper model {} (gpu={use_gpu})",
        model_path.display()
    );
    let mut params = WhisperContextParameters::default();
    params.use_gpu(use_gpu);
    // Flash attention helps Metal/CUDA and slows the CPU path.
    params.flash_attn(use_gpu);
    let ctx = WhisperContext::new_with_params(path, params)
        .map_err(|err| LfError::RuntimeUnsupported(format!("whisper.cpp: {err}")))?;
    let state = ctx
        .create_state()
        .map_err(|err| LfError::RuntimeUnsupported(err.to_string()))?;
    Ok(Loaded {
        path: model_path,
        gpu: use_gpu,
        _ctx: ctx,
        state,
    })
}

/// whisper.cpp only reads `initial_prompt` when `prompt_tokens` is null, and it
/// only keeps a prompt at all while the decoding temperature stays below 0.5.
/// Keep the text short enough to survive both.
pub const MAX_PROMPT_CHARS: usize = 400;

fn prompt_for(options: &DecodeOptions) -> String {
    if options.long_form {
        return String::new();
    }
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
    // A reduced encoder context still needs a sufficiently long mel input on
    // the Intel CPU backend. Padding with silence preserves the recording and
    // avoids native encoder -6 failures on short dictation.
    const MIN: usize = if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        16_000 * 6
    } else {
        16_000
    };
    if pcm.len() >= MIN {
        return pcm.to_vec();
    }
    let mut out = vec![0.0; MIN];
    out[..pcm.len()].copy_from_slice(pcm);
    out
}

/// Whisper's encoder always processes a 30 s window (1500 frames), so a 5 s
/// phrase pays for 25 s of padding. Shrinking `audio_ctx` to the clip length
/// gives ~3x faster encoding on CPU with unchanged WER for short clips
/// (ggml-org/whisper.cpp#1855). Too small a context hurts accuracy, hence the
/// floor.
pub const FULL_AUDIO_CTX: i32 = 1500;
pub const MIN_AUDIO_CTX: i32 = 416;
const AUDIO_CTX_ALIGNMENT: i32 = 32;

pub fn audio_ctx_for_samples(n_samples: usize) -> i32 {
    let secs = n_samples as f64 / 16_000.0;
    let requested = (secs / 30.0 * f64::from(FULL_AUDIO_CTX)).ceil() as i32 + 128;
    let aligned =
        ((requested + AUDIO_CTX_ALIGNMENT - 1) / AUDIO_CTX_ALIGNMENT) * AUDIO_CTX_ALIGNMENT;
    aligned.clamp(MIN_AUDIO_CTX, FULL_AUDIO_CTX)
}

fn vad_params(long_form: bool) -> WhisperVadParams {
    let mut p = WhisperVadParams::new();
    p.set_threshold(if long_form { 0.4 } else { 0.5 });
    p.set_min_speech_duration(150);
    p.set_min_silence_duration(if long_form { 200 } else { 120 });
    p.set_speech_pad(if long_form { 400 } else { 200 });
    p.set_samples_overlap(0.1);
    p
}

fn decode(
    state: &mut WhisperState,
    pcm: &[f32],
    language: &str,
    options: &DecodeOptions,
    vad_model: Option<&Path>,
    cancellation: &Cancellation,
    force_full_audio_ctx: bool,
) -> LfResult<String> {
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
    params.set_progress_callback_safe(on_whisper_progress);
    params.set_no_timestamps(!options.timestamps);
    params.set_single_segment(pcm.len() < 16_000 * 15 && !options.timestamps);
    params.set_suppress_blank(true);
    // Whisper's non-speech suppression also covers `/`, `_`, `#`, and brackets,
    // which are exactly the characters technical dictation needs.
    params.set_suppress_nst(!options.allow_symbols);
    params.set_no_speech_thold(if options.long_form { 0.4 } else { 0.6 });
    // Clears context carried over from the previous utterance. It is applied
    // before the initial prompt is seeded, so the two are compatible.
    params.set_no_context(true);
    // Leave the default value (0) alone for the fallback: whisper.cpp uses the
    // model's full context through that path. Explicitly setting 1500 is not
    // equivalent on every backend.
    params.set_temperature_inc(0.0);
    if !force_full_audio_ctx {
        // Short dictation should not pay for a full 30-second encoder window.
        // The lower bound avoids degrading recognition on very short speech.
        params.set_audio_ctx(audio_ctx_for_samples(pcm.len()));
    }
    // Deliberately no `set_tokens` call: whisper.cpp ignores `initial_prompt`
    // whenever `prompt_tokens` is non-null, and an empty slice still yields a
    // non-null pointer, which silently disabled prompting.
    let prompt = prompt_for(options);
    if !prompt.is_empty() {
        params.set_initial_prompt(&prompt);
    }
    let vad_path_str;
    if let Some(vad) = vad_model {
        vad_path_str = vad.to_string_lossy().into_owned();
        params.set_vad_model_path(Some(vad_path_str.as_str()));
        params.set_vad_params(vad_params(options.long_form));
        params.enable_vad(true);
    }
    let abort = cancellation.clone();
    params.set_abort_callback_safe(move || abort.is_cancelled());
    state
        .full(params, pcm)
        .map_err(|err| LfError::RuntimeUnsupported(err.to_string()))?;
    if cancellation.is_cancelled() {
        return Err(LfError::Other("cancelled".into()));
    }
    let n = state.full_n_segments();
    let mut out = String::new();
    let mut cues: Vec<TranscriptCue> = Vec::new();
    for i in 0..n {
        let Some(seg) = state.get_segment(i) else {
            continue;
        };
        let text = crate::sanitize::strip_model_tags(&seg.to_str_lossy().unwrap_or_default());
        if text.is_empty() || (options.long_form && crate::sanitize::is_likely_hallucination(&text))
        {
            continue;
        }
        if cues.last().is_some_and(|prev| {
            if options.long_form {
                prev.text.trim().eq_ignore_ascii_case(text.trim())
            } else {
                crate::sanitize::is_same_or_truncated_echo(&prev.text, &text)
            }
        }) {
            continue;
        }
        let t0 = seg.start_timestamp().max(0) as u64 * 10;
        let t1 = seg.end_timestamp().max(0) as u64 * 10;
        cues.push(TranscriptCue {
            start_ms: t0,
            end_ms: t1.max(t0),
            text,
        });
    }
    out.push_str(&crate::pipeline::join_cues_with_pauses(
        &cues,
        crate::pipeline::PARAGRAPH_PAUSE_MS,
    ));
    if let Ok(mut slot) = cues_slot().lock() {
        *slot = cues;
    }
    let cleaned = crate::sanitize::strip_model_tags(&out);
    let cleaned = if options.long_form {
        crate::sanitize::collapse_long_form_text(&cleaned)
    } else {
        crate::sanitize::collapse_echoed_transcript(&cleaned)
    };
    if crate::sanitize::is_likely_hallucination(&cleaned) && cleaned.split_whitespace().count() < 24
    {
        return Ok(String::new());
    }
    Ok(cleaned)
}

fn on_whisper_progress(progress: i32) {
    note_inner_progress(progress.clamp(0, 100) as u8);
}

fn silence_whisper_logs() {
    static ONCE: Once = Once::new();
    // Routes whisper.cpp/ggml logs into `log`; with no logger installed they
    // are dropped instead of spamming stderr.
    ONCE.call_once(whisper_rs::install_logging_hooks);
}

/// ggml matmuls scale with physical cores; hyperthreads compete for the same
/// SIMD units and usually slow the encoder down.
fn num_threads() -> std::ffi::c_int {
    let physical = num_cpus::get_physical().max(1);
    physical.clamp(1, 8) as std::ffi::c_int
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queued_file_transcription_observes_cancellation_within_one_second() {
        let token = Arc::new(AtomicBool::new(false));
        let cancellation = Cancellation::File(Arc::clone(&token));
        let (_reply_tx, reply_rx) = mpsc::channel();
        let started = Instant::now();
        let setter = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(20));
            token.store(true, Ordering::Relaxed);
        });
        let error =
            wait_for_transcription(reply_rx, &cancellation, Duration::from_secs(5)).unwrap_err();
        setter.join().unwrap();
        assert_eq!(error.to_string(), "cancelled");
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn pads_short_clips_to_safe_encoder_window() {
        let pcm = vec![0.1; 800];
        let padded = pad_to_whisper_window(&pcm);
        let expected = if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
            16_000 * 6
        } else {
            16_000
        };
        assert_eq!(padded.len(), expected);
        assert!((padded[0] - 0.1).abs() < f32::EPSILON);
        assert_eq!(padded[800], 0.0);
    }

    #[test]
    fn wait_grows_with_audio_length() {
        assert_eq!(transcribe_wait(16_000), Duration::from_secs(192));
        assert!(transcribe_wait(16_000 * 30) > Duration::from_secs(180));
        assert_eq!(
            transcribe_wait(16_000 * 60 * 20),
            Duration::from_secs(45 * 60)
        );
    }

    #[test]
    fn long_audio_is_split_into_windows() {
        assert_eq!(window_ranges(16_000 * 10), vec![(0, 16_000 * 10)]);
        let windows = window_ranges(16_000 * 90);
        assert_eq!(
            windows,
            vec![
                (0, WINDOW_SAMPLES),
                (WINDOW_SAMPLES, WINDOW_SAMPLES * 2),
                (WINDOW_SAMPLES * 2, 16_000 * 90),
            ]
        );
    }

    #[test]
    fn dictation_windows_overlap_at_the_thirty_second_seam() {
        let windows = dictation_window_ranges(16_000 * 40);
        assert_eq!(windows.len(), 2);
        assert_eq!(windows[0], (0, WINDOW_SAMPLES));
        assert_eq!(windows[1].0, 16_000 * 28);
        assert_eq!(windows[1].1, 16_000 * 40);
    }

    #[test]
    fn audio_ctx_scales_with_clip_length() {
        // 1 s clip sits on the floor.
        assert_eq!(audio_ctx_for_samples(16_000), MIN_AUDIO_CTX);
        // Reduced contexts are rounded up to a backend-safe block boundary.
        assert_eq!(audio_ctx_for_samples(16_000 * 6), 448);
        assert_eq!(audio_ctx_for_samples(16_000 * 20), 1152);
        assert_eq!(audio_ctx_for_samples(16_000 * 6) % AUDIO_CTX_ALIGNMENT, 0);
        // 30 s and longer use the full window.
        assert_eq!(audio_ctx_for_samples(16_000 * 30), FULL_AUDIO_CTX);
        assert_eq!(audio_ctx_for_samples(16_000 * 90), FULL_AUDIO_CTX);
    }

    #[test]
    fn recognizes_native_encoder_failure_for_full_context_fallback() {
        assert!(is_encoder_failure(&LfError::RuntimeUnsupported(
            "Generic whisper error. Varies depending on the function. Error code: -6".into()
        )));
        assert!(!is_encoder_failure(&LfError::RuntimeUnsupported(
            "Generic whisper error. Varies depending on the function. Error code: -7".into()
        )));
        assert!(!is_encoder_failure(&LfError::Other("cancelled".into())));
    }

    #[test]
    fn integrated_vad_is_disabled_on_intel_macos() {
        assert_eq!(
            native_vad_supported(),
            !cfg!(all(target_os = "macos", target_arch = "x86_64"))
        );
    }

    #[test]
    fn threads_never_exceed_eight_or_drop_below_one() {
        let n = num_threads();
        assert!((1..=8).contains(&n));
    }

    #[test]
    fn dictation_uses_dynamic_audio_ctx_without_temperature_retries() {
        let src = include_str!("whisper_stt.rs");
        let body = src
            .split("fn decode(")
            .nth(1)
            .unwrap()
            .split("fn silence_whisper_logs")
            .next()
            .unwrap();
        assert!(body.contains("set_temperature_inc(0.0)"));
        assert!(body.contains("set_audio_ctx(audio_ctx_for_samples("));
        assert!(body.contains("if !force_full_audio_ctx"));
        assert!(body.contains("set_no_timestamps(!options.timestamps)"));
    }

    #[test]
    fn dictation_uses_silero_vad_when_available() {
        let src = include_str!("engine.rs");
        let body = src
            .split("pub fn decode_options")
            .nth(1)
            .unwrap()
            .split("pub(crate) fn vad_model_path")
            .next()
            .unwrap();
        assert!(body.contains("vad_model: self.vad_model_path()"));
    }

    #[test]
    fn long_form_skips_timestamp_tokens_and_full_step() {
        let opts = DecodeOptions::long_form_interview();
        assert!(opts.long_form);
        assert!(!opts.timestamps);
        assert_eq!(HOP_SAMPLES, WINDOW_SAMPLES);
    }

    #[test]
    fn flash_attention_stays_off_on_cpu() {
        let src = include_str!("whisper_stt.rs");
        assert!(src.contains("params.flash_attn(use_gpu)"));
    }
}
