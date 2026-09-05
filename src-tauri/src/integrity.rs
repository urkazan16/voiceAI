use crate::catalog::ModelRecord;
use crate::error::{LfError, LfResult};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct VerifySidecar {
    sha256: String,
    size: u64,
    mtime: u64,
}

pub fn sidecar_path(model_path: &Path) -> PathBuf {
    let mut raw = model_path.as_os_str().to_os_string();
    raw.push(".verified.json");
    PathBuf::from(raw)
}

fn file_mtime(path: &Path) -> u64 {
    std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn write_sidecar(path: &Path, sha256: &str) -> LfResult<()> {
    let payload = VerifySidecar {
        sha256: sha256.to_ascii_lowercase(),
        size: std::fs::metadata(path)?.len(),
        mtime: file_mtime(path),
    };
    std::fs::write(sidecar_path(path), serde_json::to_vec_pretty(&payload)?)?;
    Ok(())
}

pub fn sidecar_matches(path: &Path, record: &ModelRecord) -> bool {
    let Ok(raw) = std::fs::read_to_string(sidecar_path(path)) else {
        return false;
    };
    let Ok(side) = serde_json::from_str::<VerifySidecar>(&raw) else {
        return false;
    };
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    side.size == meta.len()
        && (record.size == 0 || side.size == record.size)
        && side.mtime == file_mtime(path)
        && side.sha256.eq_ignore_ascii_case(&record.sha256)
}

pub fn peek_magic(path: &Path) -> LfResult<[u8; 4]> {
    let mut file = File::open(path)?;
    let mut magic = [0_u8; 4];
    file.read_exact(&mut magic)?;
    Ok(magic)
}

pub fn magic_matches_format(magic: &[u8; 4], format: &str) -> bool {
    match format.to_ascii_uppercase().as_str() {
        "GGUF" => magic == b"GGUF",
        "GGML" => matches!(
            magic,
            b"ggml" | b"ggmf" | b"ggjt" | b"lmgg" | b"fmgg" | b"tjgg" | b"GGUF"
        ),
        _ => false,
    }
}

pub fn looks_installed(path: &Path, record: &ModelRecord) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    if record.size > 0 && meta.len() != record.size {
        return false;
    }
    let Ok(magic) = peek_magic(path) else {
        return false;
    };
    magic_matches_format(&magic, &record.format)
}

pub fn sha256_file(path: &Path) -> LfResult<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0_u8; 8192];
    loop {
        let read = file.read(&mut buf)?;
        if read == 0 {
            break;
        }
        hasher.update(&buf[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

pub fn verify_checksum(path: &Path, expected_sha256: &str) -> LfResult<()> {
    let actual = sha256_file(path)?;
    if actual.eq_ignore_ascii_case(expected_sha256) {
        Ok(())
    } else {
        Err(LfError::ModelChecksumMismatch {
            expected: expected_sha256.to_ascii_lowercase(),
            actual,
        })
    }
}

pub fn validate_format(path: &Path, format: &str) -> LfResult<()> {
    let mut file = File::open(path)?;
    let mut magic = [0_u8; 4];
    file.read_exact(&mut magic)?;
    let kind = format.to_ascii_uppercase();
    match kind.as_str() {
        "GGUF" => {
            if &magic != b"GGUF" {
                return Err(LfError::ModelFormatInvalid(format!(
                    "{} is not a GGUF file",
                    path.display()
                )));
            }
        }
        "GGML" => {
            if !magic_matches_format(&magic, "GGML") {
                return Err(LfError::ModelFormatInvalid(format!(
                    "{} is not a ggml/whisper artifact",
                    path.display()
                )));
            }
        }
        other => {
            return Err(LfError::ModelFormatInvalid(format!(
                "unsupported format {other}"
            )));
        }
    }
    file.seek(SeekFrom::Start(0))?;
    Ok(())
}

pub fn activate_model(path: &Path, record: &ModelRecord) -> LfResult<()> {
    if !path.exists() {
        return Err(LfError::ModelMissing(record.model_id.clone()));
    }
    if sidecar_matches(path, record) {
        return Ok(());
    }
    verify_checksum(path, &record.sha256)?;
    validate_format(path, &record.format)?;
    let meta = std::fs::metadata(path)?;
    if record.size > 0 && meta.len() != record.size {
        return Err(LfError::ModelFormatInvalid(format!(
            "size mismatch: expected {} got {}",
            record.size,
            meta.len()
        )));
    }
    write_sidecar(path, &record.sha256)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::ModelRecord;
    use std::io::Write;
    use tempfile::tempdir;

    fn record(sha: &str, format: &str, filename: &str) -> ModelRecord {
        ModelRecord {
            model_id: "fixture".into(),
            display_name: "fixture".into(),
            version: "1".into(),
            filename: filename.into(),
            format: format.into(),
            quantization: "Q4_K_M".into(),
            kind: "llm".into(),
            source: "test".into(),
            source_url: "".into(),
            download_url: "".into(),
            sha256: sha.into(),
            size: 8,
            license: "MIT".into(),
            license_url: "".into(),
            network_required_to_obtain: false,
            checksum_pinned: true,
            notes: "".into(),
        }
    }

    #[test]
    fn checksum_mismatch_is_explicit() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("model.gguf");
        let mut f = File::create(&path).unwrap();
        f.write_all(b"GGUFxxxx").unwrap();
        let err = verify_checksum(&path, "00".repeat(32).as_str()).unwrap_err();
        assert_eq!(err.code(), "MODEL_CHECKSUM_MISMATCH");
    }

    #[test]
    fn activation_requires_matching_hash_and_format() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("model.gguf");
        let bytes = b"GGUFtest";
        std::fs::write(&path, bytes).unwrap();
        let sha = sha256_file(&path).unwrap();
        let rec = record(&sha, "GGUF", "model.gguf");
        activate_model(&path, &rec).unwrap();
    }

    #[test]
    fn whisper_ggml_accepts_little_endian_fourcc() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("ggml-base.bin");
        let bytes = b"lmggtest";
        std::fs::write(&path, bytes).unwrap();
        let sha = sha256_file(&path).unwrap();
        let rec = record(&sha, "ggml", "ggml-base.bin");
        activate_model(&path, &rec).unwrap();
        assert!(sidecar_matches(&path, &rec));
        activate_model(&path, &rec).unwrap();
    }
}
