//! Energy-based VAD for PTT trim and hands-free silence stop.

const FRAME_MS: u32 = 20;
const THRESHOLD: f32 = 0.012;
const PAD_FRAMES: usize = 8;

pub fn trim_silence(pcm: &[f32], sample_rate: u32) -> Vec<f32> {
    if pcm.is_empty() || sample_rate == 0 {
        return Vec::new();
    }
    let frame_len = ((sample_rate * FRAME_MS) / 1000).max(1) as usize;
    let mut voiced = Vec::new();
    for (idx, frame) in pcm.chunks(frame_len).enumerate() {
        if rms(frame) >= THRESHOLD {
            voiced.push(idx);
        }
    }
    if voiced.is_empty() {
        return Vec::new();
    }
    let first = voiced[0].saturating_sub(PAD_FRAMES);
    let last = (voiced[voiced.len() - 1] + PAD_FRAMES)
        .min(pcm.chunks(frame_len).count().saturating_sub(1));
    let start = first * frame_len;
    let end = ((last + 1) * frame_len).min(pcm.len());
    pcm[start..end].to_vec()
}

pub fn had_speech(pcm: &[f32], sample_rate: u32) -> bool {
    !trim_silence(pcm, sample_rate).is_empty()
}

pub fn trailing_silence_ms(pcm: &[f32], sample_rate: u32) -> u64 {
    if pcm.is_empty() || sample_rate == 0 {
        return 0;
    }
    let frame_len = ((sample_rate * FRAME_MS) / 1000).max(1) as usize;
    let mut silent_frames = 0u64;
    for frame in pcm.chunks(frame_len).rev() {
        if rms(frame) < THRESHOLD {
            silent_frames += 1;
        } else {
            break;
        }
    }
    silent_frames * u64::from(FRAME_MS)
}

fn rms(frame: &[f32]) -> f32 {
    if frame.is_empty() {
        return 0.0;
    }
    let sum: f32 = frame.iter().map(|s| s * s).sum();
    (sum / frame.len() as f32).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_leading_and_trailing_silence() {
        let mut pcm = vec![0.0; 16_000];
        for sample in pcm.iter_mut().skip(8_000).take(1_600) {
            *sample = 0.2;
        }
        let trimmed = trim_silence(&pcm, 16_000);
        assert!(trimmed.len() < pcm.len());
        assert!(trimmed.iter().any(|s| *s > 0.1));
    }

    #[test]
    fn silence_only_is_empty() {
        assert!(trim_silence(&[0.0; 3200], 16_000).is_empty());
    }
}
