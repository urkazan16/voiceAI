use crate::audio::{downmix_mono, resample_linear};
use crate::error::{LfError, LfResult};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

const AUDIO_EXT: &[&str] = &["wav", "mp3", "m4a", "aac", "ogg", "flac", "aiff", "aif"];

pub fn is_audio_path(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| AUDIO_EXT.iter().any(|x| e.eq_ignore_ascii_case(x)))
        .unwrap_or(false)
}

pub fn load_pcm_16k_mono(path: &Path) -> LfResult<Vec<f32>> {
    if !path.exists() {
        return Err(LfError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("{} not found", path.display()),
        )));
    }
    let meta = fs::metadata(path)?;
    if meta.len() == 0 {
        return Err(LfError::Other(format!(
            "{} is 0 bytes — not an audio file",
            path.display()
        )));
    }
    let bytes = fs::read(path)?;
    load_bytes(&bytes, path)
}

pub fn load_bytes(bytes: &[u8], hint: &Path) -> LfResult<Vec<f32>> {
    if bytes.is_empty() {
        return Err(LfError::Other("empty audio input".into()));
    }
    if let Ok(pcm) = decode_wav(bytes) {
        return Ok(pcm);
    }
    decode_via_converter(hint, bytes)
}

fn decode_wav(bytes: &[u8]) -> Result<Vec<f32>, ()> {
    if bytes.len() < 44 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(());
    }
    let mut offset = 12usize;
    let mut channels = 1u16;
    let mut rate = 16_000u32;
    let mut bits = 16u16;
    let mut data = None;
    while offset + 8 <= bytes.len() {
        let id = &bytes[offset..offset + 4];
        let size = u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().unwrap()) as usize;
        let start = offset + 8;
        let end = start.saturating_add(size).min(bytes.len());
        if id == b"fmt " && size >= 16 {
            channels = u16::from_le_bytes(bytes[start + 2..start + 4].try_into().unwrap());
            rate = u32::from_le_bytes(bytes[start + 4..start + 8].try_into().unwrap());
            bits = u16::from_le_bytes(bytes[start + 14..start + 16].try_into().unwrap());
        } else if id == b"data" {
            data = Some(&bytes[start..end]);
        }
        offset = end + (size % 2);
    }
    let payload = data.ok_or(())?;
    if payload.is_empty() {
        return Err(());
    }
    let samples = match bits {
        16 => payload
            .chunks_exact(2)
            .map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0)
            .collect::<Vec<_>>(),
        32 => payload
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect::<Vec<_>>(),
        8 => payload
            .iter()
            .map(|b| (*b as f32 - 128.0) / 128.0)
            .collect(),
        _ => return Err(()),
    };
    let mono = downmix_mono(&samples, channels.max(1));
    Ok(resample_linear(&mono, rate.max(1), 16_000))
}

fn decode_via_converter(hint: &Path, bytes: &[u8]) -> LfResult<Vec<f32>> {
    let dir = std::env::temp_dir().join(format!("localflow-{}", std::process::id()));
    fs::create_dir_all(&dir)?;
    let ext = hint.extension().and_then(|e| e.to_str()).unwrap_or("bin");
    let src = dir.join(format!("in.{ext}"));
    let wav = dir.join("out.wav");
    fs::write(&src, bytes)?;
    let converted = run_afconvert(&src, &wav).or_else(|_| run_ffmpeg(&src, &wav));
    let pcm = match converted {
        Ok(()) => decode_wav(&fs::read(&wav)?).map_err(|_| {
            LfError::Other(format!(
                "{} decoded to a WAV without a usable audio track",
                hint.display()
            ))
        }),
        Err(err) => Err(LfError::Other(format!(
            "cannot decode {}: {err}. Install ffmpeg or use WAV/AIFF/M4A.",
            hint.display()
        ))),
    };
    let _ = fs::remove_file(&src);
    let _ = fs::remove_file(&wav);
    pcm
}

fn run_afconvert(src: &Path, wav: &Path) -> Result<(), String> {
    let status = std::process::Command::new("afconvert")
        .args([
            "-f",
            "WAVE",
            "-d",
            "LEI16@16000",
            src.to_str().unwrap_or(""),
            wav.to_str().unwrap_or(""),
        ])
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() && wav.is_file() {
        Ok(())
    } else {
        Err("afconvert failed".into())
    }
}

fn run_ffmpeg(src: &Path, wav: &Path) -> Result<(), String> {
    let status = std::process::Command::new("ffmpeg")
        .args([
            "-y",
            "-i",
            src.to_str().unwrap_or(""),
            "-ac",
            "1",
            "-ar",
            "16000",
            wav.to_str().unwrap_or(""),
        ])
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() && wav.is_file() {
        Ok(())
    } else {
        Err("ffmpeg failed".into())
    }
}

pub fn load_stdin() -> LfResult<Vec<f32>> {
    let mut bytes = Vec::new();
    std::io::stdin().read_to_end(&mut bytes)?;
    load_bytes(&bytes, Path::new("stdin.wav"))
}

pub fn list_audio_files(dir: &Path) -> LfResult<Vec<PathBuf>> {
    if !dir.is_dir() {
        return Err(LfError::Other(format!(
            "{} is not a directory",
            dir.display()
        )));
    }
    let mut files = Vec::new();
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_file() && is_audio_path(&path) {
            files.push(path);
        }
    }
    files.sort();
    if files.is_empty() {
        return Err(LfError::Other(format!(
            "no audio files in {}",
            dir.display()
        )));
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::macos_stt::write_wav_s16le_mono;
    use tempfile::tempdir;

    #[test]
    fn wav_roundtrip_16k_mono() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("t.wav");
        let pcm = vec![0.0, 0.5, -0.5, 0.25];
        write_wav_s16le_mono(&path, 16_000, &pcm).unwrap();
        let loaded = load_pcm_16k_mono(&path).unwrap();
        assert_eq!(loaded.len(), 4);
    }

    #[test]
    fn zero_byte_file_is_error() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("empty.wav");
        fs::write(&path, []).unwrap();
        let err = load_pcm_16k_mono(&path).unwrap_err();
        assert!(err.to_string().contains("0 bytes"));
    }

    #[test]
    fn resample_44100_to_16k() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("44.wav");
        let pcm: Vec<f32> = (0..4410).map(|i| (i as f32 / 44100.0).sin()).collect();
        write_wav_s16le_mono(&path, 44_100, &pcm).unwrap();
        let loaded = load_pcm_16k_mono(&path).unwrap();
        assert!((loaded.len() as i32 - 1600).abs() < 8);
    }
}
