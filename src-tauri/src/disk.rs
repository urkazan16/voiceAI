//! Free space on the LocalFlow data volume vs model file sizes.

use crate::catalog::ModelRecord;
use crate::download::ModelInstallStatus;
use serde::Serialize;
use std::path::Path;

/// Logs, SQLite, journal rotation, and download leftovers besides the model file.
pub const OVERHEAD_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DiskUsage {
    pub data_root: String,
    pub free_bytes: Option<u64>,
    pub used_models_bytes: u64,
    pub overhead_bytes: u64,
    pub stt_name: String,
    pub stt_required_bytes: u64,
    pub stt_on_disk_bytes: u64,
    pub stt_still_needed_bytes: u64,
    pub llm_name: String,
    pub llm_required_bytes: u64,
    pub llm_on_disk_bytes: u64,
    pub llm_still_needed_bytes: u64,
    pub enough_for_speech: bool,
    pub enough_for_speech_and_formatting: bool,
    pub messages: Vec<String>,
}

pub fn still_needed(required: u64, on_disk: u64) -> u64 {
    required.saturating_sub(on_disk)
}

pub fn format_bytes(n: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let x = n as f64;
    if x < KB {
        format!("{n} B")
    } else if x < MB {
        format!("{:.1} KB", x / KB)
    } else if x < GB {
        format!("{:.1} MB", x / MB)
    } else {
        format!("{:.1} GB", x / GB)
    }
}

pub fn volume_free_bytes(path: &Path) -> Option<u64> {
    let probe = if path.exists() {
        path.to_path_buf()
    } else {
        path.parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| Path::new("/").to_path_buf())
    };
    #[cfg(unix)]
    {
        unix_free(&probe)
    }
    #[cfg(not(unix))]
    {
        let _ = probe;
        None
    }
}

#[cfg(unix)]
fn unix_free(path: &Path) -> Option<u64> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let cstr = CString::new(path.as_os_str().as_bytes()).ok()?;
    unsafe {
        let mut buf: libc::statfs = std::mem::zeroed();
        if libc::statfs(cstr.as_ptr(), &mut buf) != 0 {
            return None;
        }
        Some((buf.f_bavail as u64).saturating_mul(buf.f_bsize as u64))
    }
}

pub fn report(
    data_root: &Path,
    stt: Option<(&ModelRecord, &ModelInstallStatus)>,
    llm: Option<(&ModelRecord, &ModelInstallStatus)>,
    all_status: &[ModelInstallStatus],
) -> DiskUsage {
    let free = volume_free_bytes(data_root);
    let used_models_bytes = all_status.iter().map(|s| s.bytes_on_disk).sum();
    let (stt_name, stt_required, stt_on_disk) = match stt {
        Some((rec, status)) => (
            rec.display_name.clone(),
            rec.size,
            status.bytes_on_disk.min(rec.size),
        ),
        None => ("Not selected".into(), 0, 0),
    };
    let (llm_name, llm_required, llm_on_disk) = match llm {
        Some((rec, status)) => (
            rec.display_name.clone(),
            rec.size,
            status.bytes_on_disk.min(rec.size),
        ),
        None => ("Not selected".into(), 0, 0),
    };
    let stt_still = still_needed(stt_required, stt_on_disk);
    let llm_still = still_needed(llm_required, llm_on_disk);
    let speech_need = stt_still.saturating_add(OVERHEAD_BYTES);
    let both_need = speech_need.saturating_add(llm_still);
    let enough_for_speech = match free {
        Some(free) => free >= speech_need,
        None => stt_still == 0,
    };
    let enough_for_speech_and_formatting = match free {
        Some(free) => free >= both_need,
        None => stt_still == 0 && llm_still == 0,
    };
    let mut usage = DiskUsage {
        data_root: data_root.display().to_string(),
        free_bytes: free,
        used_models_bytes,
        overhead_bytes: OVERHEAD_BYTES,
        stt_name,
        stt_required_bytes: stt_required,
        stt_on_disk_bytes: stt_on_disk,
        stt_still_needed_bytes: stt_still,
        llm_name,
        llm_required_bytes: llm_required,
        llm_on_disk_bytes: llm_on_disk,
        llm_still_needed_bytes: llm_still,
        enough_for_speech,
        enough_for_speech_and_formatting,
        messages: Vec::new(),
    };
    usage.messages = describe(&usage);
    usage
}

pub fn describe(usage: &DiskUsage) -> Vec<String> {
    let mut lines = Vec::new();
    match usage.free_bytes {
        Some(free) => lines.push(format!(
            "Free on this volume: {}. LocalFlow data: {}.",
            format_bytes(free),
            usage.data_root
        )),
        None => lines.push(format!(
            "Could not read free space. LocalFlow data: {}.",
            usage.data_root
        )),
    }
    lines.push(format!(
        "Models already on disk: {}. Keep about {} extra for logs, history, and incomplete downloads.",
        format_bytes(usage.used_models_bytes),
        format_bytes(usage.overhead_bytes)
    ));
    if usage.stt_still_needed_bytes == 0 && usage.stt_required_bytes > 0 {
        lines.push(format!(
            "Speech model {} is on disk ({}) — no extra download space needed.",
            usage.stt_name,
            format_bytes(usage.stt_on_disk_bytes)
        ));
    } else if usage.stt_required_bytes > 0 {
        lines.push(format!(
            "Speech model {} needs {} more (file size {}).",
            usage.stt_name,
            format_bytes(usage.stt_still_needed_bytes),
            format_bytes(usage.stt_required_bytes)
        ));
        if !usage.enough_for_speech {
            if let Some(free) = usage.free_bytes {
                lines.push(format!(
                    "Not enough space for speech: need about {} including overhead, {} free. Free some disk before LocalFlow can finish the Whisper download.",
                    format_bytes(usage.stt_still_needed_bytes.saturating_add(usage.overhead_bytes)),
                    format_bytes(free)
                ));
            }
        } else {
            lines.push("There is enough free space to finish the speech model download.".into());
        }
    }
    if usage.llm_required_bytes > 0 && usage.llm_still_needed_bytes == 0 {
        lines.push(format!(
            "Formatting model {} is on disk ({}).",
            usage.llm_name,
            format_bytes(usage.llm_on_disk_bytes)
        ));
    } else if usage.llm_required_bytes > 0 {
        lines.push(format!(
            "Optional formatting model {} would need {} more (file size {}). Dictation works without it.",
            usage.llm_name,
            format_bytes(usage.llm_still_needed_bytes),
            format_bytes(usage.llm_required_bytes)
        ));
        if !usage.enough_for_speech_and_formatting {
            if let Some(free) = usage.free_bytes {
                lines.push(format!(
                    "Not enough space to add formatting as well: {} free.",
                    format_bytes(free)
                ));
            }
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn still_needed_subtracts_partial_file() {
        assert_eq!(still_needed(100, 40), 60);
        assert_eq!(still_needed(100, 100), 0);
        assert_eq!(still_needed(100, 120), 0);
    }

    #[test]
    fn describe_warns_when_free_is_below_whisper() {
        let usage = DiskUsage {
            data_root: "/tmp/LocalFlow".into(),
            free_bytes: Some(50 * 1024 * 1024),
            used_models_bytes: 0,
            overhead_bytes: OVERHEAD_BYTES,
            stt_name: "Whisper Small".into(),
            stt_required_bytes: 487_601_967,
            stt_on_disk_bytes: 0,
            stt_still_needed_bytes: 487_601_967,
            llm_name: "Qwen".into(),
            llm_required_bytes: 0,
            llm_on_disk_bytes: 0,
            llm_still_needed_bytes: 0,
            enough_for_speech: false,
            enough_for_speech_and_formatting: false,
            messages: Vec::new(),
        };
        let lines = describe(&usage);
        assert!(lines.iter().any(|l| l.contains("Not enough space for speech")));
        assert!(lines.iter().any(|l| l.contains("Whisper Small")));
    }

    #[test]
    fn format_bytes_uses_mb_for_whisper_small() {
        assert!(format_bytes(487_601_967).contains("MB"));
    }
}
