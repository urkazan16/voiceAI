//! macOS Speech.framework recognizer, kept as an alternative to whisper-rs.
//! The WAV writer that used to live here is portable and now sits in `media`.

use crate::error::{LfError, LfResult};

pub fn transcribe_pcm_16k(pcm: &[f32]) -> LfResult<String> {
    #[cfg(target_os = "macos")]
    {
        macos::transcribe_pcm_16k(pcm)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = pcm;
        Err(LfError::RuntimeUnsupported(
            "speech-to-text runtime is not linked on this platform".into(),
        ))
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use std::ffi::CStr;
    use std::os::raw::c_char;

    extern "C" {
        fn lf_macos_transcribe(wav_path: *const c_char, out: *mut c_char, out_len: i32) -> i32;
    }

    pub fn transcribe_pcm_16k(pcm: &[f32]) -> LfResult<String> {
        let dir = std::env::temp_dir().join("localflow");
        std::fs::create_dir_all(&dir)?;
        let wav = dir.join(format!("capture-{}.wav", std::process::id()));
        crate::media::write_wav_s16le_mono(&wav, 16_000, pcm)?;
        let c_path = std::ffi::CString::new(wav.to_string_lossy().as_bytes())
            .map_err(|_| LfError::Other("wav path contains NUL".into()))?;
        let mut out = vec![0_i8; 16 * 1024];
        let rc =
            unsafe { lf_macos_transcribe(c_path.as_ptr(), out.as_mut_ptr(), out.len() as i32) };
        let _ = std::fs::remove_file(&wav);
        match rc {
            0 => {
                let text = unsafe { CStr::from_ptr(out.as_ptr()) }
                    .to_string_lossy()
                    .trim()
                    .to_string();
                if text.is_empty() {
                    Err(LfError::RuntimeUnsupported(
                        "speech recognizer returned empty text".into(),
                    ))
                } else {
                    Ok(text)
                }
            }
            6 => Err(LfError::PermissionDenied(
                "Whisper is not ready. Download a speech model in Models.".into(),
            )),
            5 => Err(LfError::RuntimeUnsupported(
                "on-device speech recognizer is unavailable for the current language".into(),
            )),
            _ => Err(LfError::RuntimeUnsupported(format!(
                "macOS speech recognizer failed (rc={rc})"
            ))),
        }
    }
}
