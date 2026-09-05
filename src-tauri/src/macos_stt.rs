use crate::error::{LfError, LfResult};
use std::io::Write;
use std::path::Path;

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

pub fn write_wav_s16le_mono(path: &Path, sample_rate: u32, pcm: &[f32]) -> std::io::Result<()> {
    let mut data = Vec::with_capacity(pcm.len() * 2);
    for sample in pcm {
        let clipped = sample.clamp(-1.0, 1.0);
        let int = (clipped * 32767.0).round() as i16;
        data.extend_from_slice(&int.to_le_bytes());
    }
    let data_len = data.len() as u32;
    let byte_rate = sample_rate * 2;
    let mut header = Vec::with_capacity(44);
    header.extend_from_slice(b"RIFF");
    header.extend_from_slice(&(36 + data_len).to_le_bytes());
    header.extend_from_slice(b"WAVE");
    header.extend_from_slice(b"fmt ");
    header.extend_from_slice(&16u32.to_le_bytes());
    header.extend_from_slice(&1u16.to_le_bytes());
    header.extend_from_slice(&1u16.to_le_bytes());
    header.extend_from_slice(&sample_rate.to_le_bytes());
    header.extend_from_slice(&byte_rate.to_le_bytes());
    header.extend_from_slice(&2u16.to_le_bytes());
    header.extend_from_slice(&16u16.to_le_bytes());
    header.extend_from_slice(b"data");
    header.extend_from_slice(&data_len.to_le_bytes());

    let mut file = std::fs::File::create(path)?;
    file.write_all(&header)?;
    file.write_all(&data)?;
    Ok(())
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
        write_wav_s16le_mono(&wav, 16_000, pcm)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn wav_header_is_44_bytes_plus_samples() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("t.wav");
        write_wav_s16le_mono(&path, 16_000, &[0.0, 0.5, -0.5]).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");
        assert_eq!(bytes.len(), 44 + 6);
    }
}
