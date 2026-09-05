use crate::catalog::ModelRecord;
use crate::error::{LfError, LfResult};
use crate::integrity::{
    looks_installed, magic_matches_format, peek_magic, sha256_file, sidecar_matches, write_sidecar,
};
use futures_util::StreamExt;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::Instant;
use tokio::io::AsyncWriteExt;

#[derive(Debug, Clone, Serialize)]
pub struct ModelDownloadProgress {
    pub model_id: String,
    pub phase: String,
    pub bytes_downloaded: u64,
    pub total_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelInstallStatus {
    pub model_id: String,
    pub state: String,
    pub installed: bool,
    pub verified: bool,
    pub local_path: Option<String>,
    pub bytes_on_disk: u64,
    pub expected_bytes: u64,
    pub active: bool,
}

pub fn partial_path(dest: &Path) -> PathBuf {
    let mut raw = dest.as_os_str().to_os_string();
    raw.push(".partial");
    PathBuf::from(raw)
}

pub fn inspect_install(record: &ModelRecord, dest: &Path) -> ModelInstallStatus {
    if dest.exists() {
        let bytes_on_disk = std::fs::metadata(dest).map(|m| m.len()).unwrap_or(0);
        let verified = sidecar_matches(dest, record);
        let installed = verified || looks_installed(dest, record);
        let state = if verified {
            "verified"
        } else if installed {
            "installed"
        } else {
            "unverified"
        };
        return ModelInstallStatus {
            model_id: record.model_id.clone(),
            state: state.into(),
            installed,
            verified,
            local_path: Some(dest.display().to_string()),
            bytes_on_disk,
            expected_bytes: record.size,
            active: false,
        };
    }

    let partial = partial_path(dest);
    if let Ok(meta) = std::fs::metadata(&partial) {
        if meta.len() > 0 {
            return ModelInstallStatus {
                model_id: record.model_id.clone(),
                state: "incomplete".into(),
                installed: false,
                verified: false,
                local_path: Some(partial.display().to_string()),
                bytes_on_disk: meta.len(),
                expected_bytes: record.size,
                active: false,
            };
        }
    }

    ModelInstallStatus {
        model_id: record.model_id.clone(),
        state: "missing".into(),
        installed: false,
        verified: false,
        local_path: None,
        bytes_on_disk: 0,
        expected_bytes: record.size,
        active: false,
    }
}

pub async fn download_and_install(
    record: &ModelRecord,
    dest: &Path,
    mut on_progress: impl FnMut(ModelDownloadProgress),
) -> LfResult<()> {
    if record.download_url.trim().is_empty() {
        return Err(LfError::NetworkRequired(format!(
            "no download URL for {}",
            record.model_id
        )));
    }
    if dest.exists() && (sidecar_matches(dest, record) || looks_installed(dest, record)) {
        on_progress(ModelDownloadProgress {
            model_id: record.model_id.clone(),
            phase: "complete".into(),
            bytes_downloaded: record.size,
            total_bytes: record.size,
        });
        return Ok(());
    }

    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let partial = partial_path(dest);

    on_progress(ModelDownloadProgress {
        model_id: record.model_id.clone(),
        phase: "downloading".into(),
        bytes_downloaded: tokio::fs::metadata(&partial)
            .await
            .map(|m| m.len())
            .unwrap_or(0),
        total_bytes: record.size,
    });

    let digest = fetch_to_file(&record.download_url, &partial, record, &mut on_progress).await?;

    on_progress(ModelDownloadProgress {
        model_id: record.model_id.clone(),
        phase: "verifying".into(),
        bytes_downloaded: record.size,
        total_bytes: record.size,
    });

    if !digest.eq_ignore_ascii_case(&record.sha256) {
        return Err(LfError::ModelChecksumMismatch {
            expected: record.sha256.to_ascii_lowercase(),
            actual: digest,
        });
    }
    let magic = peek_magic(&partial)?;
    if !magic_matches_format(&magic, &record.format) {
        return Err(LfError::ModelFormatInvalid(format!(
            "{} is not a {} artifact",
            partial.display(),
            record.format
        )));
    }

    on_progress(ModelDownloadProgress {
        model_id: record.model_id.clone(),
        phase: "installing".into(),
        bytes_downloaded: record.size,
        total_bytes: record.size,
    });

    tokio::fs::rename(&partial, dest).await?;
    write_sidecar(dest, &digest)?;

    on_progress(ModelDownloadProgress {
        model_id: record.model_id.clone(),
        phase: "complete".into(),
        bytes_downloaded: record.size,
        total_bytes: record.size,
    });
    Ok(())
}

async fn fetch_to_file(
    url: &str,
    dest: &Path,
    record: &ModelRecord,
    on_progress: &mut impl FnMut(ModelDownloadProgress),
) -> LfResult<String> {
    let client = reqwest::Client::builder()
        .user_agent("LocalFlow/0.1.0 (model-manager; https://github.com/urkazan16/voiceAI)")
        .redirect(reqwest::redirect::Policy::limited(16))
        .build()
        .map_err(|err| LfError::Other(err.to_string()))?;

    let existing = tokio::fs::metadata(dest)
        .await
        .ok()
        .map(|m| m.len())
        .unwrap_or(0);
    if existing > 0 && record.size > 0 && existing == record.size {
        let dest = dest.to_path_buf();
        return tokio::task::spawn_blocking(move || sha256_file(&dest))
            .await
            .map_err(|err| LfError::Other(err.to_string()))?;
    }
    let mut request = client
        .get(url)
        .header(reqwest::header::ACCEPT, "application/octet-stream");
    if existing > 0 && (record.size == 0 || existing < record.size) {
        request = request.header(reqwest::header::RANGE, format!("bytes={existing}-"));
    }

    let response = request
        .send()
        .await
        .map_err(|err| LfError::Other(format!("download failed: {err}")))?;

    let status = response.status();
    let resume = status == reqwest::StatusCode::PARTIAL_CONTENT;
    if !status.is_success() && !resume {
        return Err(LfError::Other(format!(
            "download HTTP {} for {}",
            status, record.model_id
        )));
    }

    let mut file = if resume {
        tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dest)
            .await?
    } else {
        tokio::fs::File::create(dest).await?
    };
    let mut downloaded: u64 = if resume { existing } else { 0 };
    let total = if record.size > 0 {
        record.size
    } else {
        downloaded + response.content_length().unwrap_or(0)
    };
    let dest_for_hash = dest.to_path_buf();
    let mut hasher = if resume && existing > 0 {
        tokio::task::spawn_blocking(move || hash_existing_file(&dest_for_hash))
            .await
            .map_err(|err| LfError::Other(err.to_string()))??
    } else {
        Sha256::new()
    };
    let mut last_emit = Instant::now();
    let mut stream = response.bytes_stream();

    on_progress(ModelDownloadProgress {
        model_id: record.model_id.clone(),
        phase: "downloading".into(),
        bytes_downloaded: downloaded,
        total_bytes: total,
    });

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|err| LfError::Other(format!("download stream: {err}")))?;
        hasher.update(&chunk);
        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;
        if record.size > 0 && downloaded > record.size.saturating_mul(2) {
            drop(file);
            return Err(LfError::ModelFormatInvalid(format!(
                "{} grew larger than twice the catalog size",
                record.model_id
            )));
        }
        if last_emit.elapsed().as_millis() >= 250 {
            on_progress(ModelDownloadProgress {
                model_id: record.model_id.clone(),
                phase: "downloading".into(),
                bytes_downloaded: downloaded,
                total_bytes: total,
            });
            last_emit = Instant::now();
        }
    }
    file.flush().await?;
    drop(file);

    if record.size > 0 && downloaded != record.size {
        return Err(LfError::Other(format!(
            "download incomplete for {}: {} of {} bytes — resume from Model Manager",
            record.model_id, downloaded, record.size
        )));
    }
    Ok(hex::encode(hasher.finalize()))
}

fn hash_existing_file(path: &Path) -> LfResult<Sha256> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0_u8; 65536];
    loop {
        let read = std::io::Read::read(&mut file, &mut buf)?;
        if read == 0 {
            break;
        }
        hasher.update(&buf[..read]);
    }
    Ok(hasher)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::integrity::{activate_model, sha256_file};
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use tempfile::tempdir;

    fn record_for(bytes: &[u8], url: String, dest_name: &str) -> ModelRecord {
        let dir = tempdir().unwrap();
        let path = dir.path().join("hash.bin");
        std::fs::write(&path, bytes).unwrap();
        let sha = sha256_file(&path).unwrap();
        ModelRecord {
            model_id: "fixture".into(),
            display_name: "fixture".into(),
            version: "1".into(),
            filename: dest_name.into(),
            format: "GGUF".into(),
            quantization: "Q4_K_M".into(),
            kind: "llm".into(),
            source: "test".into(),
            source_url: "".into(),
            download_url: url,
            sha256: sha,
            size: bytes.len() as u64,
            license: "MIT".into(),
            license_url: "".into(),
            network_required_to_obtain: true,
            checksum_pinned: true,
            notes: "".into(),
        }
    }

    fn serve_bytes(body: &'static [u8]) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0_u8; 2048];
            let _ = stream.read(&mut buf);
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/octet-stream\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(header.as_bytes());
            let _ = stream.write_all(body);
        });
        format!("http://{addr}/model.gguf")
    }

    #[test]
    fn partial_sidecar_keeps_original_name() {
        assert_eq!(
            partial_path(Path::new("/models/whisper/ggml-small.bin")),
            Path::new("/models/whisper/ggml-small.bin.partial")
        );
    }

    #[test]
    fn inspect_reports_incomplete_partial() {
        let dir = tempdir().unwrap();
        let dest = dir.path().join("ggml-small.bin");
        std::fs::write(partial_path(&dest), b"partial").unwrap();
        let record = record_for(b"GGUFdata", "http://example".into(), "ggml-small.bin");
        let status = inspect_install(&record, &dest);
        assert_eq!(status.state, "incomplete");
        assert!(!status.installed);
        assert_eq!(status.bytes_on_disk, 7);
    }

    #[tokio::test]
    async fn downloads_verifies_and_installs() {
        let body: &'static [u8] = b"GGUFtestdata";
        let url = serve_bytes(body);
        let record = record_for(body, url, "model.gguf");
        let dir = tempdir().unwrap();
        let dest = dir.path().join("model.gguf");
        download_and_install(&record, &dest, |_| {}).await.unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), body);
        activate_model(&dest, &record).unwrap();
        let status = inspect_install(&record, &dest);
        assert_eq!(status.state, "verified");
        assert!(status.installed && status.verified);
    }

    #[test]
    fn inspect_treats_matching_file_as_installed_without_hashing() {
        let dir = tempdir().unwrap();
        let dest = dir.path().join("model.gguf");
        let body = b"GGUFtestdata";
        std::fs::write(&dest, body).unwrap();
        let record = record_for(body, "http://example".into(), "model.gguf");
        let status = inspect_install(&record, &dest);
        assert_eq!(status.state, "installed");
        assert!(status.installed);
        assert!(!status.verified);
    }

    #[tokio::test]
    async fn checksum_failure_keeps_previously_installed_file() {
        let previous: &'static [u8] = b"GGUFold";
        let expected: &'static [u8] = b"GGUFnewvalue";
        let poisoned: &'static [u8] = b"GGUFbadvalue";
        assert_eq!(expected.len(), poisoned.len());
        assert_ne!(previous.len(), expected.len());
        let url = serve_bytes(poisoned);
        let record = record_for(expected, url, "model.gguf");
        let dir = tempdir().unwrap();
        let dest = dir.path().join("model.gguf");
        std::fs::write(&dest, previous).unwrap();
        let err = download_and_install(&record, &dest, |_| {})
            .await
            .unwrap_err();
        assert_eq!(err.code(), "MODEL_CHECKSUM_MISMATCH");
        assert_eq!(std::fs::read(&dest).unwrap(), previous);
    }

    #[tokio::test]
    async fn rejects_empty_download_url() {
        let mut record = record_for(b"GGUF", String::new(), "model.gguf");
        record.download_url.clear();
        let dir = tempdir().unwrap();
        let dest = dir.path().join("model.gguf");
        let err = download_and_install(&record, &dest, |_| {})
            .await
            .unwrap_err();
        assert_eq!(err.code(), "NETWORK_OPERATION_REQUIRED");
    }
}
