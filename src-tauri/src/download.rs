use crate::catalog::ModelRecord;
use crate::error::{LfError, LfResult};
use crate::integrity::activate_model;
use futures_util::StreamExt;
use serde::Serialize;
use std::path::Path;
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
    pub installed: bool,
    pub verified: bool,
    pub local_path: Option<String>,
    pub bytes_on_disk: u64,
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
    if dest.exists() && activate_model(dest, record).is_ok() {
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

    let partial = dest.with_extension(format!(
        "{}.partial",
        dest.extension().and_then(|e| e.to_str()).unwrap_or("bin")
    ));
    let _ = tokio::fs::remove_file(&partial).await;

    on_progress(ModelDownloadProgress {
        model_id: record.model_id.clone(),
        phase: "downloading".into(),
        bytes_downloaded: 0,
        total_bytes: record.size,
    });

    fetch_to_file(&record.download_url, &partial, record, &mut on_progress).await?;

    on_progress(ModelDownloadProgress {
        model_id: record.model_id.clone(),
        phase: "verifying".into(),
        bytes_downloaded: record.size,
        total_bytes: record.size,
    });

    if let Err(err) = activate_model(&partial, record) {
        let _ = tokio::fs::remove_file(&partial).await;
        return Err(err);
    }

    on_progress(ModelDownloadProgress {
        model_id: record.model_id.clone(),
        phase: "installing".into(),
        bytes_downloaded: record.size,
        total_bytes: record.size,
    });

    let _ = tokio::fs::remove_file(dest).await;
    tokio::fs::rename(&partial, dest).await?;
    activate_model(dest, record)?;

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
) -> LfResult<()> {
    let client = reqwest::Client::builder()
        .user_agent("LocalFlow/0.1.0 (model-manager; https://github.com/urkazan16/voiceAI)")
        .redirect(reqwest::redirect::Policy::limited(16))
        .build()
        .map_err(|err| LfError::Other(err.to_string()))?;

    let response = client
        .get(url)
        .header(reqwest::header::ACCEPT, "application/octet-stream")
        .send()
        .await
        .map_err(|err| LfError::Other(format!("download failed: {err}")))?;

    if !response.status().is_success() {
        return Err(LfError::Other(format!(
            "download HTTP {} for {}",
            response.status(),
            record.model_id
        )));
    }

    let total = response
        .content_length()
        .unwrap_or(record.size)
        .max(record.size);
    let mut file = tokio::fs::File::create(dest).await?;
    let mut downloaded: u64 = 0;
    let mut last_emit = Instant::now();
    let mut stream = response.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|err| LfError::Other(format!("download stream: {err}")))?;
        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;
        if record.size > 0 && downloaded > record.size.saturating_mul(2) {
            let _ = tokio::fs::remove_file(dest).await;
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
        let _ = tokio::fs::remove_file(dest).await;
        return Err(LfError::ModelFormatInvalid(format!(
            "size mismatch for {}: expected {} got {}",
            record.model_id, record.size, downloaded
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::integrity::sha256_file;
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
