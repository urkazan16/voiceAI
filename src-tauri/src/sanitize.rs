//! Strip whisper/llama special tokens so they never reach inserted text.

pub fn strip_model_tags(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '[' {
            if let Some(end) = chars[i..].iter().position(|c| *c == ']') {
                let inner: String = chars[i + 1..i + end].iter().collect();
                if is_service_tag(&inner) {
                    i += end + 1;
                    continue;
                }
            }
        }
        if chars[i] == '<' && i + 1 < chars.len() && chars[i + 1] == '|' {
            if let Some(end) = chars[i..].iter().position(|c| *c == '>') {
                i += end + 1;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

/// Whisper often invents podcast outros on silence. Do not insert those.
pub fn is_likely_hallucination(text: &str) -> bool {
    let t = text
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c.is_whitespace() {
                c
            } else {
                ' '
            }
        })
        .collect::<String>();
    let t = t.split_whitespace().collect::<Vec<_>>().join(" ");
    if t.is_empty() {
        return true;
    }
    const EXACT: &[&str] = &["music", "applause"];
    if EXACT.iter().any(|p| t == *p) {
        return true;
    }
    const PREFIXES: &[&str] = &[
        "с вами был",
        "с вами была",
        "thank you for watching",
        "thanks for watching",
        "thanks for listening",
        "please subscribe",
        "like and subscribe",
        "subtitles by",
        "transcript by",
        "продолжение следует",
        "подписывайтесь",
        "ставьте лайки",
    ];
    PREFIXES
        .iter()
        .any(|p| t == *p || t.starts_with(&format!("{p} ")))
}

fn is_service_tag(inner: &str) -> bool {
    let t = inner.trim();
    if t.is_empty() {
        return true;
    }
    if t.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    let upper = t.to_ascii_uppercase();
    upper.contains("BLANK")
        || upper.contains("AUDIO")
        || upper.starts_with('_')
        || t.chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_blank_audio_and_specials() {
        assert_eq!(
            strip_model_tags("[BLANK_AUDIO] привет [_BEG_] мир <|en|>"),
            "привет мир"
        );
    }

    #[test]
    fn keeps_real_brackets() {
        assert_eq!(strip_model_tags("массив [0]"), "массив [0]");
    }

    #[test]
    fn does_not_drop_first_letter() {
        assert_eq!(strip_model_tags("Привет"), "Привет");
    }

    #[test]
    fn flags_silence_hallucination() {
        assert!(is_likely_hallucination("С вами был Игорь Негода."));
        assert!(is_likely_hallucination("Thank you for watching!"));
        assert!(!is_likely_hallucination("Привет это проверка диктовки."));
        assert!(!is_likely_hallucination(
            "подпишись на канал новостей завтра"
        ));
    }
}
