//! One JSON object per utterance. Secrets are redacted before write.

use crate::journal::redact;
use crate::paths::DataPaths;
use chrono::{Local, Utc};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UtteranceLine {
    pub schema: u32,
    pub id: String,
    pub ts: String,
    pub timezone: String,
    pub text: String,
    pub raw: String,
    pub application: String,
    pub profile: String,
    pub mode: String,
    pub model: String,
    pub processing_time_ms: u64,
    pub duration_ms: u64,
    pub word_count: u32,
    pub wpm: f64,
    pub insert_method: String,
    pub insert_ok: bool,
}

pub fn word_count(text: &str) -> u32 {
    text.split_whitespace().filter(|w| !w.is_empty()).count() as u32
}

pub fn wpm(words: u32, duration_ms: u64) -> f64 {
    if duration_ms == 0 || words == 0 {
        return 0.0;
    }
    (f64::from(words) * 60_000.0) / duration_ms as f64
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AppWpm {
    pub application: String,
    pub wpm_avg: f64,
    pub utterances: u64,
}

pub fn wpm_by_application(rows: &[UtteranceLine]) -> Vec<AppWpm> {
    use std::collections::BTreeMap;
    let mut acc: BTreeMap<String, (f64, u64)> = BTreeMap::new();
    for row in rows {
        if row.wpm <= 0.0 {
            continue;
        }
        let entry = acc.entry(row.application.clone()).or_insert((0.0, 0));
        entry.0 += row.wpm;
        entry.1 += 1;
    }
    acc.into_iter()
        .map(|(application, (sum, n))| AppWpm {
            application,
            wpm_avg: sum / n as f64,
            utterances: n,
        })
        .collect()
}

pub fn timezone_name() -> String {
    Local::now().format("%z").to_string()
}

pub fn now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}

pub fn append(paths: &DataPaths, mut line: UtteranceLine) -> std::io::Result<()> {
    paths.ensure()?;
    line.text = redact(&line.text);
    line.raw = redact(&line.raw);
    let file = paths.utterances();
    let mut out = OpenOptions::new().create(true).append(true).open(file)?;
    serde_json::to_writer(&mut out, &line)?;
    out.write_all(b"\n")?;
    Ok(())
}

pub fn read_since(paths: &DataPaths, epoch_rfc3339: Option<&str>) -> Vec<UtteranceLine> {
    let Ok(file) = fs::File::open(paths.utterances()) else {
        return Vec::new();
    };
    BufReader::new(file)
        .lines()
        .map_while(Result::ok)
        .filter_map(|l| serde_json::from_str::<UtteranceLine>(&l).ok())
        .filter(|row| match epoch_rfc3339 {
            Some(epoch) => row.ts.as_str() >= epoch,
            None => true,
        })
        .collect()
}

pub fn to_csv(rows: &[UtteranceLine]) -> String {
    let mut out = String::from("ts,timezone,wpm,words,application,mode,text\n");
    for row in rows {
        out.push_str(&format!(
            "{},{},{:.1},{},{},{},{}\n",
            csv_escape(&row.ts),
            csv_escape(&row.timezone),
            row.wpm,
            row.word_count,
            csv_escape(&row.application),
            csv_escape(&row.mode),
            csv_escape(&row.text)
        ));
    }
    out
}

fn csv_escape(value: &str) -> String {
    if value.contains([',', '"', '\n']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn wpm_is_words_per_minute() {
        assert!((wpm(120, 60_000) - 120.0).abs() < f64::EPSILON);
    }

    #[test]
    fn jsonl_roundtrip() {
        let dir = tempdir().unwrap();
        let paths = DataPaths::from_override(dir.path().to_path_buf());
        let line = UtteranceLine {
            schema: 1,
            id: "1".into(),
            ts: "2026-09-05T10:00:00+00:00".into(),
            timezone: "+0000".into(),
            text: "hello world".into(),
            raw: "hello world".into(),
            application: "Mail".into(),
            profile: "email".into(),
            mode: "normal".into(),
            model: "whisper-small".into(),
            processing_time_ms: 10,
            duration_ms: 2000,
            word_count: 2,
            wpm: 60.0,
            insert_method: "clipboard".into(),
            insert_ok: true,
        };
        append(&paths, line.clone()).unwrap();
        let rows = read_since(&paths, None);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].text, "hello world");
        assert!(to_csv(&rows).contains("Mail"));
        let by_app = wpm_by_application(&rows);
        assert_eq!(by_app.len(), 1);
        assert_eq!(by_app[0].application, "Mail");
        assert!((by_app[0].wpm_avg - 60.0).abs() < f64::EPSILON);
    }
}
