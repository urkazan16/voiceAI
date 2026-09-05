pub fn play_start(volume: f32) {
    play("Tink", volume);
}

pub fn play_end(volume: f32) {
    play("Pop", volume);
}

fn play(name: &str, volume: f32) {
    #[cfg(target_os = "macos")]
    {
        let path = format!("/System/Library/Sounds/{name}.aiff");
        let vol = crate::config::clamp_cue_volume(volume);
        let _ = std::process::Command::new("afplay")
            .arg("-v")
            .arg(format!("{vol:.2}"))
            .arg(&path)
            .spawn();
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (name, volume);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cue_volume_is_clamped_for_afplay() {
        assert!((crate::config::clamp_cue_volume(0.25) - 0.25).abs() < f32::EPSILON);
        assert!((crate::config::clamp_cue_volume(0.0) - 0.05).abs() < f32::EPSILON);
        assert!((crate::config::clamp_cue_volume(2.0) - 1.0).abs() < f32::EPSILON);
        play("Tink", f32::NAN);
    }
}
