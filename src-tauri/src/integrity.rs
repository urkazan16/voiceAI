use crate::catalog::ModelRecord;
use crate::error::{LfError, LfResult};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

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
            if &magic != b"ggml" && &magic != b"ggmf" && &magic != b"ggjt" && &magic != b"GGUF" {
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
    verify_checksum(path, &record.sha256)?;
    validate_format(path, &record.format)?;
    let meta = std::fs::metadata(path)?;
    if record.size > 0 && meta.len() == 0 {
        return Err(LfError::ModelFormatInvalid("empty model file".into()));
    }
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
}
