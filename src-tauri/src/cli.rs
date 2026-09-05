//! Headless CLI: transcribe files, batch a directory, local checker.

use crate::config::AppSettings;
use crate::engine::AppEngine;
use crate::error::LfResult;
use crate::eval;
use crate::injection::{ClipboardInjector, MemoryInjector, TextInjector};
use crate::llm::NativeLlm;
use crate::media;
use crate::paths::DataPaths;
use crate::pipeline::{PipelineMode, PipelineOutput};
use crate::stt::NativeStt;
use crate::vad;
use std::path::{Path, PathBuf};

pub fn invoked(args: &[String]) -> bool {
    args.iter().any(|a| {
        matches!(
            a.as_str(),
            "transcribe"
                | "check"
                | "devices"
                | "paste-smoke"
                | "--help"
                | "-h"
                | "--version"
                | "-V"
                | "--json"
        ) || a == "--"
            || Path::new(a)
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| media::is_audio_path(Path::new(&format!("x.{e}"))))
    }) && args.len() > 1
}

pub fn run(args: &[String]) -> i32 {
    match run_inner(args) {
        Ok(code) => code,
        Err(err) => {
            eprintln!("{err}");
            1
        }
    }
}

struct Opts {
    json: bool,
    no_post: bool,
    language: Option<String>,
    model: Option<String>,
    device: Option<String>,
    files: Vec<PathBuf>,
    dir: Option<PathBuf>,
    stdin: bool,
    command: Command,
}

enum Command {
    Transcribe,
    Check,
    Devices,
    PasteSmoke,
    Help,
    Version,
}

fn parse(args: &[String]) -> Result<Opts, String> {
    let mut opts = Opts {
        json: false,
        no_post: false,
        language: None,
        model: None,
        device: None,
        files: Vec::new(),
        dir: None,
        stdin: false,
        command: Command::Transcribe,
    };
    let mut rest = args.iter().skip(1);
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "transcribe" => opts.command = Command::Transcribe,
            "check" => opts.command = Command::Check,
            "devices" => opts.command = Command::Devices,
            "paste-smoke" => opts.command = Command::PasteSmoke,
            "--help" | "-h" => opts.command = Command::Help,
            "--version" | "-V" => opts.command = Command::Version,
            "--json" => opts.json = true,
            "--no-postprocess" | "--raw" => opts.no_post = true,
            "--stdin" => opts.stdin = true,
            "--language" | "-l" => {
                opts.language = Some(rest.next().cloned().ok_or("--language needs a value")?)
            }
            "--model" | "-m" => {
                opts.model = Some(rest.next().cloned().ok_or("--model needs a value")?)
            }
            "--device" | "-d" => {
                opts.device = Some(rest.next().cloned().ok_or("--device needs a value")?)
            }
            "--dir" => opts.dir = Some(PathBuf::from(rest.next().ok_or("--dir needs a path")?)),
            "--" => {}
            flag if flag.starts_with('-') => return Err(format!("unknown flag {flag}")),
            other => opts.files.push(PathBuf::from(other)),
        }
    }
    Ok(opts)
}

fn run_inner(args: &[String]) -> Result<i32, String> {
    let opts = parse(args)?;
    match opts.command {
        Command::Help => {
            print_help();
            Ok(0)
        }
        Command::Version => {
            println!("{}", crate::build_info::current().version);
            Ok(0)
        }
        Command::Devices => {
            let devices = crate::audio::list_input_devices().map_err(|e| e.to_string())?;
            for d in devices {
                println!("{}{}", d.name, if d.is_default { " (default)" } else { "" });
            }
            Ok(0)
        }
        Command::Check => {
            let report = local_check();
            if opts.json {
                println!("{}", serde_json::to_string_pretty(&report).unwrap());
            } else {
                println!("{}", report.join("\n"));
            }
            if report.iter().any(|l| l.contains("FAIL")) {
                Ok(1)
            } else {
                Ok(0)
            }
        }
        Command::PasteSmoke => paste_smoke(),
        Command::Transcribe => transcribe(opts),
    }
}

fn print_help() {
    eprintln!(
        "\
LocalFlow CLI (no GUI)

Usage:
  localflow transcribe [--json] [--no-postprocess] [--language ru|en|auto]
                       [--model MODEL_ID] [--device NAME] [--dir DIR] [--stdin] [FILE...]
  localflow devices
  localflow check [--json]
  localflow paste-smoke
  localflow --version
  localflow --help

Exit status is 0 on success and non-zero on error.
Progress goes to stderr; transcripts go to stdout.
"
    );
}

fn transcribe(opts: Opts) -> Result<i32, String> {
    let mut paths_to_run: Vec<(String, Result<Vec<f32>, String>)> = Vec::new();
    if opts.stdin {
        eprintln!("reading audio from stdin");
        paths_to_run.push((
            "stdin".into(),
            media::load_stdin().map_err(|e| e.to_string()),
        ));
    }
    if let Some(dir) = &opts.dir {
        eprintln!("batch {}", dir.display());
        for file in media::list_audio_files(dir).map_err(|e| e.to_string())? {
            eprintln!("loading {}", file.display());
            paths_to_run.push((
                file.display().to_string(),
                media::load_pcm_16k_mono(&file).map_err(|e| e.to_string()),
            ));
        }
    }
    for file in &opts.files {
        eprintln!("loading {}", file.display());
        paths_to_run.push((
            file.display().to_string(),
            media::load_pcm_16k_mono(file).map_err(|e| e.to_string()),
        ));
    }
    if paths_to_run.is_empty() {
        return Err("no input: pass files, --dir, or --stdin".into());
    }
    let mut engine = AppEngine::open(DataPaths::detect()).map_err(|e| e.to_string())?;
    engine.inject_enabled = false;
    if let Some(lang) = opts.language {
        engine.settings.stt_language = lang;
    }
    if let Some(model) = opts.model {
        engine.settings.active_stt_model = Some(model);
    }
    if let Some(device) = opts.device {
        engine.settings.microphone_name = Some(device);
    }
    if opts.no_post {
        engine.settings.mode = PipelineMode::Raw;
        engine.settings.personalization_enabled = false;
    }
    let mut outputs = Vec::new();
    for (name, pcm) in paths_to_run {
        let pcm = pcm?;
        eprintln!("transcribing {name}");
        let out = run_pcm(&mut engine, &pcm).map_err(|e| e.to_string())?;
        if !opts.json {
            println!("{}", out.final_text);
        }
        outputs.push(CliRow {
            file: name,
            text: out.final_text.clone(),
            raw: out.raw_transcript.clone(),
        });
        let sidecar = PathBuf::from(&outputs.last().unwrap().file);
        if sidecar.exists() {
            let txt = sidecar.with_extension("txt");
            if txt != sidecar {
                let _ = std::fs::write(txt, format!("{}\n", out.final_text));
            }
        }
    }
    if opts.json {
        println!("{}", serde_json::to_string_pretty(&outputs).unwrap());
    }
    Ok(0)
}

#[derive(serde::Serialize)]
struct CliRow {
    file: String,
    text: String,
    raw: String,
}

fn run_pcm(engine: &mut AppEngine, pcm: &[f32]) -> LfResult<PipelineOutput> {
    engine.run_text_pipeline("", &NativeStt, &NativeLlm, &MemoryInjector::default(), pcm)
}

fn local_check() -> Vec<String> {
    let mut lines = Vec::new();
    lines.push(format!(
        "os: {} {}",
        std::env::consts::OS,
        std::env::consts::ARCH
    ));
    #[cfg(target_os = "macos")]
    {
        if let Ok(out) = std::process::Command::new("sw_vers")
            .arg("-productVersion")
            .output()
        {
            lines.push(format!(
                "macos: {}",
                String::from_utf8_lossy(&out.stdout).trim()
            ));
        }
    }
    lines.push(format!(
        "screen_locked: {}",
        crate::screenlock::screen_is_locked()
    ));
    match crate::audio::list_input_devices() {
        Ok(devs) => {
            lines.push(format!("mic_count: {}", devs.len()));
            if let Some(default) = devs.iter().find(|d| d.is_default) {
                lines.push(format!("mic_default: {}", default.name));
            }
        }
        Err(err) => lines.push(format!("mic_count: FAIL ({err})")),
    }
    let paths = crate::paths::DataPaths::detect();
    let _ = paths.ensure();
    lines.push(format!(
        "settings_file: {} ({})",
        paths.settings_file().display(),
        if paths.settings_file().exists() {
            "present"
        } else {
            "absent"
        }
    ));
    if let Ok(eng) = AppEngine::open(paths.clone()) {
        let ready = eng
            .settings
            .active_stt_model
            .as_deref()
            .and_then(|id| eng.ready_model_path("stt").map(|_| id.to_string()));
        lines.push(format!(
            "whisper_ready: {}",
            ready.as_deref().unwrap_or("no")
        ));
    } else {
        lines.push("whisper_ready: no".into());
    }
    lines.push(format!(
        "wer_identity: {}",
        pass(eval::wer("один два", "один два") == 0.0)
    ));
    let mut speech = vec![0.0; 16_000];
    for (i, sample) in speech.iter_mut().skip(4_000).take(8_000).enumerate() {
        *sample = 0.25 * (i as f32 * 0.12).sin();
    }
    let mixed = eval::mix_snr(&speech, 15.0);
    lines.push(format!(
        "vad_snr_15db: {}",
        pass(vad::had_speech(&mixed, 16_000))
    ));
    lines.push(format!(
        "settings_json: {}",
        pass(serde_json::to_string(&AppSettings::default()).is_ok())
    ));
    let dir = tempfile_lock_dir();
    if let Some(root) = dir {
        let p = crate::paths::DataPaths::from_override(root);
        let first = crate::instance::acquire_gui_lock(&p);
        let second = crate::instance::acquire_gui_lock(&p);
        lines.push(format!(
            "single_instance: {}",
            pass(first.is_ok() && second.is_err())
        ));
    } else {
        lines.push("single_instance: SKIP".into());
    }
    lines.push("offline: PASS (checker uses no network)".into());
    lines
}

fn tempfile_lock_dir() -> Option<std::path::PathBuf> {
    let root = std::env::temp_dir().join(format!("lf-block0-{}", std::process::id()));
    std::fs::create_dir_all(&root).ok()?;
    Some(root)
}

fn paste_smoke() -> Result<i32, String> {
    let app = crate::injection::frontmost_app_name().unwrap_or_default();
    if !app.to_ascii_lowercase().contains("textedit") {
        eprintln!("paste-smoke skipped: frontmost is {app:?}, expected TextEdit");
        return Ok(2);
    }
    let token = format!("LFBLK0-{}", std::process::id());
    crate::injection::prepare_keyboard_for_insert();
    ClipboardInjector {
        target_pid: crate::injection::frontmost_unix_id(),
        target_app: Some(app),
        insert_delay_ms: 120,
    }
    .insert_text(&token, true)
    .map_err(|e| e.to_string())?;
    println!("{token}");
    Ok(0)
}

fn pass(ok: bool) -> &'static str {
    if ok {
        "PASS"
    } else {
        "FAIL"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_and_version_parse() {
        let help = parse(&["lf".into(), "--help".into()]).unwrap();
        assert!(matches!(help.command, Command::Help));
        let ver = parse(&["lf".into(), "--version".into()]).unwrap();
        assert!(matches!(ver.command, Command::Version));
    }

    #[test]
    fn transcribe_flags() {
        let opts = parse(&[
            "lf".into(),
            "transcribe".into(),
            "--json".into(),
            "--no-postprocess".into(),
            "--language".into(),
            "en".into(),
            "a.wav".into(),
        ])
        .unwrap();
        assert!(opts.json);
        assert!(opts.no_post);
        assert_eq!(opts.language.as_deref(), Some("en"));
        assert_eq!(opts.files.len(), 1);
    }

    #[test]
    fn paste_smoke_command_parses() {
        let opts = parse(&["lf".into(), "paste-smoke".into()]).unwrap();
        assert!(matches!(opts.command, Command::PasteSmoke));
    }
}
