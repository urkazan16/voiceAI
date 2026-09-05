use std::process::Command;

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
    let date = std::env::var("LOCALFLOW_BUILD_DATE").unwrap_or_else(|_| chrono_like_date());
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
}

fn chrono_like_date() -> String {
    Command::new("date")
        .args(["-u", "+%Y-%m-%d"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}
