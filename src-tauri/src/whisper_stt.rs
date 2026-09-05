use crate::error::{LfError, LfResult};
use std::path::Path;
use std::sync::atomic::AtomicBool;

pub fn transcribe(
    model_path: &Path,
    pcm: &[f32],
    _cancel: &AtomicBool,
    language: &str,
) -> LfResult<String> {
    if !model_path.is_file() {
        return Err(LfError::ModelMissing(model_path.display().to_string()));
    }
    if pcm.is_empty() {
        return Ok(String::new());
    }
    inner(model_path, pcm, language)
}

fn inner(model_path: &Path, pcm: &[f32], language: &str) -> LfResult<String> {
    use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

    let path = model_path
        .to_str()
        .ok_or_else(|| LfError::Other("model path is not UTF-8".into()))?;
    let ctx = WhisperContext::new_with_params(path, WhisperContextParameters::default())
        .map_err(|err| LfError::RuntimeUnsupported(format!("whisper.cpp: {err}")))?;
    let mut state = ctx
        .create_state()
        .map_err(|err| LfError::RuntimeUnsupported(err.to_string()))?;
    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_n_threads(num_threads());
    params.set_translate(false);
    let lang = language.trim();
    if lang.is_empty() || lang.eq_ignore_ascii_case("auto") {
        params.set_language(Some("auto"));
        params.set_detect_language(true);
    } else {
        params.set_language(Some(lang));
        params.set_detect_language(false);
    }
    params.set_print_special(false);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);
    params.set_suppress_blank(true);
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
    for i in 0..n {
        if let Ok(seg) = state.full_get_segment_text(i) {
            out.push_str(&seg);
        }
    }
    Ok(out.replace("[BLANK_AUDIO]", "").trim().to_string())
}

fn num_threads() -> std::ffi::c_int {
    std::thread::available_parallelism()
        .map(|n| n.get().clamp(1, 8) as std::ffi::c_int)
        .unwrap_or(2)
}
