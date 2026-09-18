use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn git_sha() -> String {
    if let Ok(from_env) = std::env::var("LOCALFLOW_GIT_SHA") {
        if !from_env.is_empty() {
            return from_env;
        }
    }
    Command::new("git")
        .args(["rev-parse", "--short=7", "HEAD"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

fn main() {
    println!("cargo:rerun-if-changed=native/include/localflow_runtime.h");
    println!("cargo:rerun-if-env-changed=LOCALFLOW_GIT_SHA");
    println!("cargo:rerun-if-env-changed=LOCALFLOW_BUILD_DATE");
    println!("cargo:rustc-env=LOCALFLOW_GIT_SHA={}", git_sha());
    let date = std::env::var("LOCALFLOW_BUILD_DATE").unwrap_or_else(|_| build_date());
    println!("cargo:rustc-env=LOCALFLOW_BUILD_DATE={date}");

    #[cfg(target_os = "macos")]
    {
        println!("cargo:rerun-if-changed=native/src/speech.m");
        println!("cargo:rerun-if-changed=native/src/lock.m");
        println!("cargo:rustc-link-lib=framework=Speech");
        println!("cargo:rustc-link-lib=framework=Foundation");
        println!("cargo:rustc-link-lib=framework=ApplicationServices");
        println!("cargo:rustc-link-lib=framework=CoreGraphics");
        println!("cargo:rustc-link-lib=framework=CoreFoundation");
        cc::Build::new()
            .file("native/src/speech.m")
            .file("native/src/lock.m")
            .include("native/include")
            .flag("-fobjc-arc")
            .compile("localflow_speech");
    }

    bundle_sherpa_windows_dlls();
    tauri_build::try_build(tauri_build::Attributes::new()).expect("tauri build");
    embed_comctl32_v6_for_windows_tests();
}

/// Tauri embeds Common-Controls v6 only in the app binary. `cargo test --lib`
/// builds a separate harness that still imports `TaskDialogIndirect` via
/// tao/muda/tray. Without the v6 manifest Windows binds comctl32 v5 and the
/// process dies at load with STATUS_ENTRYPOINT_NOT_FOUND (0xc0000139).
///
/// `rustc-link-arg` (not `-tests`) is required: the lib unit-test harness is
/// not an `[[test]]` target. The extra dependency is merged into the shipped
/// exe, which already declares v6.
/// Windows links sherpa-onnx as `sherpa-onnx-c-api.dll` + `onnxruntime.dll`.
/// sherpa-onnx-sys extracts the shared archive under `target/sherpa-onnx-prebuilt`
/// and may copy DLLs next to the profile exe. `tauri.windows.conf.json` lists
/// those names as `bundle.resources`, so they must exist in `src-tauri/` before
/// `tauri_build` runs — including on a cold CI cache where the profile folder
/// is still empty.
fn bundle_sherpa_windows_dlls() {
    let windows = std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows");
    if !windows {
        return;
    }
    println!("cargo:rerun-if-env-changed=OUT_DIR");
    println!("cargo:rerun-if-env-changed=CARGO_MANIFEST_DIR");
    let Ok(out_dir) = std::env::var("OUT_DIR") else {
        return;
    };
    let out_dir = std::path::PathBuf::from(out_dir);
    let Some(profile_dir) = out_dir.ancestors().find(|path| {
        matches!(
            path.file_name().and_then(|name| name.to_str()),
            Some("debug" | "release")
        )
    }) else {
        return;
    };
    let Ok(manifest) = std::env::var("CARGO_MANIFEST_DIR") else {
        return;
    };
    let dest_dir = std::path::PathBuf::from(manifest);
    copy_sherpa_runtime_dlls(profile_dir, &dest_dir, false);
    copy_sherpa_runtime_dlls(&profile_dir.join("examples"), &dest_dir, false);
    let mut searched = vec![profile_dir.display().to_string()];
    if let Some(target_dir) = out_dir
        .ancestors()
        .find(|path| path.file_name().and_then(|name| name.to_str()) == Some("target"))
    {
        let prebuilt = target_dir.join("sherpa-onnx-prebuilt");
        println!("cargo:rerun-if-changed={}", prebuilt.display());
        copy_sherpa_runtime_dlls(&prebuilt, &dest_dir, true);
        searched.push(prebuilt.display().to_string());
    }
    let missing: Vec<_> = ["sherpa-onnx-c-api.dll", "onnxruntime.dll"]
        .iter()
        .copied()
        .filter(|name| !dest_dir.join(name).is_file())
        .collect();
    if !missing.is_empty() {
        panic!(
            "Windows sherpa-onnx runtime DLLs missing ({}) after searching {}. \
             sherpa-onnx-sys should extract them under target/sherpa-onnx-prebuilt.",
            missing.join(", "),
            searched.join(", ")
        );
    }
}

fn is_sherpa_runtime_dll(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".dll") && (lower.contains("sherpa") || lower.contains("onnxruntime"))
}

fn copy_sherpa_runtime_dlls(root: &std::path::Path, dest_dir: &std::path::Path, recursive: bool) {
    let mut stack = vec![(root.to_path_buf(), 0usize)];
    while let Some((dir, depth)) = stack.pop() {
        if depth > 8 {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if recursive {
                    stack.push((path, depth + 1));
                }
                continue;
            }
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if !is_sherpa_runtime_dll(name) {
                continue;
            }
            let dest = dest_dir.join(name);
            if std::fs::copy(&path, &dest).is_ok() {
                println!("cargo:rerun-if-changed={}", path.display());
            }
        }
    }
}

fn embed_comctl32_v6_for_windows_tests() {
    let windows = std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows");
    if !windows {
        return;
    }
    println!(
        "cargo:rustc-link-arg=/MANIFESTDEPENDENCY:type='win32' \
         name='Microsoft.Windows.Common-Controls' version='6.0.0.0' \
         processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'"
    );
}

/// UTC build date without shelling out to `date`, which does not take
/// `-u +%Y-%m-%d` on Windows.
fn build_date() -> String {
    let Ok(now) = SystemTime::now().duration_since(UNIX_EPOCH) else {
        return "unknown".to_string();
    };
    let (year, month, day) = civil_from_days((now.as_secs() / 86_400) as i64);
    format!("{year:04}-{month:02}-{day:02}")
}

/// Howard Hinnant's `civil_from_days`: days since 1970-01-01 to a calendar date.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}
