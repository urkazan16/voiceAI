pub fn play_start() {
    play("Tink");
}

pub fn play_end() {
    play("Pop");
}

fn play(name: &str) {
    #[cfg(target_os = "macos")]
    {
        let path = format!("/System/Library/Sounds/{name}.aiff");
        let _ = std::process::Command::new("afplay")
            .arg("-v")
            .arg("0.25")
            .arg(&path)
            .spawn();
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = name;
    }
}
