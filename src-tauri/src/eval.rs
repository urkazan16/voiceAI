//! Word error rate and synthetic SNR checks. No network.

pub fn wer(reference: &str, hypothesis: &str) -> f64 {
    let ref_w: Vec<&str> = tokenize(reference);
    let hyp_w: Vec<&str> = tokenize(hypothesis);
    if ref_w.is_empty() {
        return if hyp_w.is_empty() { 0.0 } else { 1.0 };
    }
    levenshtein(&ref_w, &hyp_w) as f64 / ref_w.len() as f64
}

fn tokenize(text: &str) -> Vec<&str> {
    text.split_whitespace().collect()
}

fn levenshtein(a: &[&str], b: &[&str]) -> usize {
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0; b.len() + 1];
    for (i, wa) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, wb) in b.iter().enumerate() {
            let cost = usize::from(wa != wb);
            cur[j + 1] = (prev[j + 1] + 1).min(cur[j] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

/// Mix a burst of speech-shaped signal with white noise at the given SNR (dB).
pub fn mix_snr(speech: &[f32], snr_db: f32) -> Vec<f32> {
    let speech_pow = rms(speech).max(1e-8);
    let noise_pow = speech_pow / 10f32.powf(snr_db / 20.0);
    speech
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let n = ((((i as u32).wrapping_mul(1103515245).wrapping_add(12345)) >> 16) as f32
                / 32768.0)
                - 1.0;
            s + n * noise_pow
        })
        .collect()
}

fn rms(frame: &[f32]) -> f32 {
    if frame.is_empty() {
        return 0.0;
    }
    (frame.iter().map(|s| s * s).sum::<f32>() / frame.len() as f32).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vad;

    #[test]
    fn identity_has_zero_wer() {
        assert_eq!(wer("привет мир", "привет мир"), 0.0);
    }

    #[test]
    fn substitution_is_half() {
        assert!((wer("a b", "a c") - 0.5).abs() < 1e-9);
    }

    #[test]
    fn vad_survives_snr_15db() {
        let mut speech = vec![0.0; 16_000];
        for (i, sample) in speech.iter_mut().skip(4_000).take(8_000).enumerate() {
            *sample = 0.25 * (i as f32 * 0.12).sin();
        }
        let mixed = mix_snr(&speech, 15.0);
        assert!(vad::had_speech(&mixed, 16_000));
    }
}
