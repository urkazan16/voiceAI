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
        println!("cargo:rustc-link-lib=framework=CoreGraphics");
        println!("cargo:rustc-link-lib=framework=CoreFoundation");
        cc::Build::new()
            .file("native/src/speech.m")
            .file("native/src/lock.m")
            .include("native/include")
            .flag("-fobjc-arc")
            .compile("localflow_speech");
    }

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
