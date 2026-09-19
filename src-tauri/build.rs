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
/// sherpa-onnx-sys may extract those under `target/sherpa-onnx-prebuilt` and
/// copy them next to the profile exe. This crate's `build.rs` runs *before*
/// that extract on `cargo check`, so missing DLLs are a warning, not a panic.
/// NSIS picks them up from `scripts/build-release.mjs` after a release compile.
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
    copy_sherpa_runtime_dlls(profile_dir.join("examples").as_path(), profile_dir, false);
    if let Some(target_dir) = out_dir
        .ancestors()
        .find(|path| path.file_name().and_then(|name| name.to_str()) == Some("target"))
    {
        let prebuilt = target_dir.join("sherpa-onnx-prebuilt");
        println!("cargo:rerun-if-changed={}", prebuilt.display());
        restore_sherpa_windows_prebuilt(&prebuilt);
        copy_sherpa_runtime_dlls(&prebuilt, profile_dir, true);
        stage_sherpa_windows_import_libs(&prebuilt, &out_dir);
    }
}

const SHERPA_WIN_ARCHIVE_STEM: &str = "sherpa-onnx-v1.13.8-win-x64-shared-MT-Release-lib";
const SHERPA_WIN_ARCHIVE: &str = "sherpa-onnx-v1.13.8-win-x64-shared-MT-Release-lib.tar.bz2";
const SHERPA_WIN_ARCHIVE_URL: &str =
    "https://github.com/k2-fsa/sherpa-onnx/releases/download/v1.13.8/sherpa-onnx-v1.13.8-win-x64-shared-MT-Release-lib.tar.bz2";
const SHERPA_WIN_IMPORT_LIBS: [&str; 2] = ["sherpa-onnx-c-api.lib", "onnxruntime.lib"];

fn restore_sherpa_windows_prebuilt(prebuilt: &std::path::Path) {
    if find_named_file(prebuilt, "sherpa-onnx-c-api.lib").is_some() {
        return;
    }
    let _ = std::fs::create_dir_all(prebuilt);
    let archive_path = prebuilt.join(SHERPA_WIN_ARCHIVE);
    if !download_url_to_file(SHERPA_WIN_ARCHIVE_URL, &archive_path) {
        println!("cargo:warning=could not download {SHERPA_WIN_ARCHIVE_URL}");
        return;
    }
    let extracted = prebuilt.join(SHERPA_WIN_ARCHIVE_STEM);
    let _ = std::fs::remove_dir_all(&extracted);
    if let Err(err) = unpack_tar_bz2(&archive_path, prebuilt) {
        println!("cargo:warning=failed to unpack sherpa-onnx Windows libs: {err}");
        let _ = std::fs::remove_dir_all(&extracted);
    }
}

fn stage_sherpa_windows_import_libs(prebuilt: &std::path::Path, out_dir: &std::path::Path) {
    let mut staged = false;
    for name in SHERPA_WIN_IMPORT_LIBS {
        let Some(src) = find_named_file(prebuilt, name) else {
            continue;
        };
        let dest = out_dir.join(name);
        if std::fs::copy(&src, &dest).is_ok() {
            staged = true;
        }
        if let Some(parent) = src.parent() {
            println!("cargo:rustc-link-search=native={}", parent.display());
        }
    }
    if staged {
        println!("cargo:rustc-link-search=native={}", out_dir.display());
        for name in SHERPA_WIN_IMPORT_LIBS {
            let path = out_dir.join(name);
            if path.is_file() {
                println!("cargo:rustc-link-arg={}", path.display());
            }
        }
    } else {
        println!(
            "cargo:warning=sherpa-onnx-c-api.lib is still missing; Windows link will fail with LNK1181"
        );
    }
}

fn download_url_to_file(url: &str, dest: &std::path::Path) -> bool {
    if dest.is_file()
        && dest
            .metadata()
            .map(|meta| meta.len() > 1_000)
            .unwrap_or(false)
    {
        return true;
    }
    for cmd in ["curl", "curl.exe"] {
        if let Ok(status) = Command::new(cmd)
            .args(["-fsSL", "--retry", "3", "-o"])
            .arg(dest)
            .arg(url)
            .status()
        {
            if status.success() && dest.is_file() {
                return true;
            }
        }
    }
    false
}

fn unpack_tar_bz2(
    archive: &std::path::Path,
    dest: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let file = std::fs::File::open(archive)?;
    let decoder = bzip2::read::BzDecoder::new(file);
    tar::Archive::new(decoder).unpack(dest)?;
    Ok(())
}

fn find_named_file(root: &std::path::Path, name: &str) -> Option<std::path::PathBuf> {
    let mut stack = vec![(root.to_path_buf(), 0usize)];
    let want = name.to_ascii_lowercase();
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
                stack.push((path, depth + 1));
                continue;
            }
            let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if file_name.to_ascii_lowercase() == want {
                return Some(path);
            }
        }
    }
    None
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
