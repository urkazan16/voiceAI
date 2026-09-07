use crate::platform::Cue;

pub fn play_start(volume: f32) {
    crate::platform::current().play_cue(Cue::Start, volume);
}

pub fn play_end(volume: f32) {
    crate::platform::current().play_cue(Cue::End, volume);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cue_volume_is_clamped_before_it_reaches_the_host() {
        assert!((crate::config::clamp_cue_volume(0.25) - 0.25).abs() < f32::EPSILON);
        assert!((crate::config::clamp_cue_volume(0.0) - 0.05).abs() < f32::EPSILON);
        assert!((crate::config::clamp_cue_volume(2.0) - 1.0).abs() < f32::EPSILON);
        play_start(f32::NAN);
    }
}
