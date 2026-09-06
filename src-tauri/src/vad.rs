//! Energy-based VAD for PTT trim and hands-free silence stop.

const FRAME_MS: u32 = 20;
const PAD_FRAMES: usize = 16;

pub fn default_threshold() -> f32 {
    0.012
}

pub fn clamp_threshold(value: f32) -> f32 {
    value.clamp(0.002, 0.08)
}

pub fn trim_silence(pcm: &[f32], sample_rate: u32) -> Vec<f32> {
    trim_silence_at(pcm, sample_rate, default_threshold())
}

pub fn trim_silence_at(pcm: &[f32], sample_rate: u32, threshold: f32) -> Vec<f32> {
    if pcm.is_empty() || sample_rate == 0 {
        return Vec::new();
    }
    let threshold = clamp_threshold(threshold);
    let frame_len = ((sample_rate * FRAME_MS) / 1000).max(1) as usize;
    let mut voiced = Vec::new();
    for (idx, frame) in pcm.chunks(frame_len).enumerate() {
        if rms(frame) >= threshold {
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

pub fn split_on_internal_silence(
    pcm: &[f32],
    sample_rate: u32,
    threshold: f32,
    gap_ms: u64,
) -> Vec<Vec<f32>> {
    if pcm.is_empty() || sample_rate == 0 {
        return Vec::new();
    }
    let threshold = clamp_threshold(threshold);
    let frame_len = ((sample_rate * FRAME_MS) / 1000).max(1) as usize;
    let gap_frames = (gap_ms / u64::from(FRAME_MS)).max(1) as usize;
    let frames: Vec<bool> = pcm
        .chunks(frame_len)
        .map(|frame| rms(frame) >= threshold)
        .collect();
    let voiced: Vec<usize> = frames
        .iter()
        .enumerate()
        .filter_map(|(i, voiced)| voiced.then_some(i))
        .collect();
    if voiced.is_empty() {
        return Vec::new();
    }
    let mut clusters: Vec<(usize, usize)> = vec![(voiced[0], voiced[0])];
    for &idx in &voiced[1..] {
        let last = clusters.last_mut().expect("cluster");
        if idx.saturating_sub(last.1) > gap_frames {
            clusters.push((idx, idx));
        } else {
            last.1 = idx;
        }
    }
    if clusters.len() <= 1 {
        return Vec::new();
    }
    clusters
        .into_iter()
        .map(|(first, last)| {
            let start = first.saturating_sub(PAD_FRAMES) * frame_len;
            let end = ((last + 1 + PAD_FRAMES) * frame_len).min(pcm.len());
            pcm[start..end].to_vec()
        })
        .collect()
}

pub fn had_speech(pcm: &[f32], sample_rate: u32) -> bool {
    had_speech_at(pcm, sample_rate, default_threshold())
}

pub fn had_speech_at(pcm: &[f32], sample_rate: u32, threshold: f32) -> bool {
    !trim_silence_at(pcm, sample_rate, threshold).is_empty()
}

pub fn trailing_silence_ms(pcm: &[f32], sample_rate: u32) -> u64 {
    trailing_silence_ms_at(pcm, sample_rate, default_threshold())
}

pub fn trailing_silence_ms_at(pcm: &[f32], sample_rate: u32, threshold: f32) -> u64 {
    if pcm.is_empty() || sample_rate == 0 {
        return 0;
    }
    let threshold = clamp_threshold(threshold);
    let frame_len = ((sample_rate * FRAME_MS) / 1000).max(1) as usize;
    let mut silent_frames = 0u64;
    for frame in pcm.chunks(frame_len).rev() {
        if rms(frame) < threshold {
            silent_frames += 1;
        } else {
            break;
        }
    }
    silent_frames * u64::from(FRAME_MS)
}

pub fn rms(frame: &[f32]) -> f32 {
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

    #[test]
    fn higher_threshold_treats_quiet_speech_as_silence() {
        let pcm = vec![0.02; 8_000];
        assert!(!trim_silence_at(&pcm, 16_000, 0.012).is_empty());
        assert!(trim_silence_at(&pcm, 16_000, 0.05).is_empty());
    }

    #[test]
    fn splits_when_internal_pause_exceeds_two_seconds() {
        let sr = 16_000u32;
        let mut pcm = vec![0.0; sr as usize * 6];
        for sample in pcm.iter_mut().take(sr as usize / 2) {
            *sample = 0.2;
        }
        for sample in pcm.iter_mut().skip(sr as usize * 4) {
            *sample = 0.2;
        }
        let chunks = split_on_internal_silence(&pcm, sr, 0.012, 2000);
        assert_eq!(chunks.len(), 2, "{}", chunks.len());
    }

    #[test]
    fn trailing_silence_counts_quiet_tail() {
        let sr = 16_000u32;
        let mut pcm = vec![0.2; sr as usize];
        pcm.extend(std::iter::repeat(0.0).take(sr as usize));
        let ms = trailing_silence_ms(&pcm, sr);
        assert!(ms >= 900, "{ms}");
        assert_eq!(trailing_silence_ms(&[], sr), 0);
        assert_eq!(trailing_silence_ms(&[0.2; 320], 0), 0);
    }

    #[test]
    fn had_speech_false_on_silence() {
        assert!(!had_speech(&[0.0; 8_000], 16_000));
        assert!(had_speech(&[0.2; 8_000], 16_000));
        assert_eq!(clamp_threshold(0.0), 0.002);
        assert_eq!(clamp_threshold(1.0), 0.08);
    }
}
