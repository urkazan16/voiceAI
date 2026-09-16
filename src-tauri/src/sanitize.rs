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

const HALLUCINATION_PHRASES: &[&str] = &[
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
    "подписывайтесь на канал",
    "подписывайтесь",
    "ставьте лайки",
    "ставьте лайк",
    "пишите комментарии",
    "спасибо за просмотр",
    "делайте комментарии",
    "поделитесь видео",
];

/// Whisper often invents podcast outros on silence. Do not insert those.
pub fn is_likely_hallucination(text: &str) -> bool {
    let t = norm_key(text);
    if t.is_empty() {
        return true;
    }
    const EXACT: &[&str] = &["music", "applause", "конец", "слышен звук"];
    if EXACT.iter().any(|p| t == *p) {
        return true;
    }
    if HALLUCINATION_PHRASES
        .iter()
        .any(|p| t == *p || t.starts_with(&format!("{p} ")))
    {
        return true;
    }
    let words = t.split_whitespace().count();
    if words > 0 && words <= 16 {
        let spam_hits = HALLUCINATION_PHRASES
            .iter()
            .filter(|p| t.contains(*p))
            .count();
        if spam_hits >= 2 {
            return true;
        }
    }
    is_degenerate_repetition(&t)
}

/// Looped n-grams ("подписывайтесь на канал" × N, "и и и") with almost no
/// other vocabulary. Real interview speech has a much higher unique-word ratio.
pub fn is_degenerate_repetition(text: &str) -> bool {
    let key = norm_key(text);
    let words: Vec<&str> = key.split_whitespace().collect();
    if words.len() < 12 {
        return false;
    }
    let mut unique = words.clone();
    unique.sort_unstable();
    unique.dedup();
    if unique.len() * 20 <= words.len() * 3 {
        return true;
    }
    for n in 2..=8.min(words.len() / 3) {
        let phrase = &words[..n];
        let mut copies = 0u32;
        let mut i = 0usize;
        while i + n <= words.len() {
            if words_match_ci(&words[i..i + n], phrase) {
                copies += 1;
                i += n;
            } else {
                i += 1;
            }
        }
        if copies >= 4 && copies as usize * n * 10 >= words.len() * 6 {
            return true;
        }
    }
    false
}

fn norm_key(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c.is_whitespace() {
                c
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn sentence_parts(text: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut buf = String::new();
    for ch in text.chars() {
        buf.push(ch);
        if matches!(ch, '.' | '!' | '?' | '…') {
            let piece = buf.trim();
            if !piece.is_empty() {
                parts.push(piece.to_string());
            }
            buf.clear();
        }
    }
    let rest = buf.trim();
    if !rest.is_empty() {
        parts.push(rest.to_string());
    }
    parts
}

pub fn is_same_or_truncated_echo(previous: &str, next: &str) -> bool {
    let prev = norm_key(previous);
    let cur = norm_key(next);
    if prev.is_empty() || cur.is_empty() {
        return false;
    }
    prev == cur || is_truncated_echo(previous, next) || is_truncated_echo(next, previous)
}

fn is_truncated_echo(previous: &str, next: &str) -> bool {
    let prev = norm_key(previous);
    let cur = norm_key(next);
    if cur.is_empty() || prev.is_empty() {
        return false;
    }
    if prev == cur {
        return true;
    }
    // Cut-off copy of the same utterance.
    if prev.starts_with(&cur) && cur.len() * 10 >= prev.len() * 6 {
        return true;
    }
    // Trailing stump after a looped sentence ("с декаб.").
    let cur_words = cur.split_whitespace().count();
    cur_words <= 2 && cur.len() >= 6 && prev.contains(&cur)
}

fn collapse_consecutive_sentences(text: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    for part in sentence_parts(text) {
        let key = norm_key(&part);
        if key.is_empty() || is_spam_sentence(&key) {
            continue;
        }
        if let Some(prev) = out.last_mut() {
            if sentences_are_echo(prev, &part) {
                if part.chars().count() > prev.chars().count() {
                    *prev = part;
                }
                continue;
            }
        }
        out.push(part);
    }
    out.join(" ")
}

fn is_spam_sentence(key: &str) -> bool {
    HALLUCINATION_PHRASES
        .iter()
        .any(|p| key == *p || key.starts_with(&format!("{p} ")))
        || key == "конец"
}

fn sentences_are_echo(previous: &str, next: &str) -> bool {
    let prev = norm_key(previous);
    let cur = norm_key(next);
    if prev.is_empty() || cur.is_empty() {
        return false;
    }
    if prev == cur || is_truncated_echo(previous, next) || is_truncated_echo(next, previous) {
        return true;
    }
    let wa: Vec<&str> = prev.split_whitespace().collect();
    let wb: Vec<&str> = cur.split_whitespace().collect();
    if wa.len() < 5 || wb.len() < 5 {
        return false;
    }
    let overlap = wa.iter().filter(|w| wb.contains(w)).count();
    let min_len = wa.len().min(wb.len());
    let max_len = wa.len().max(wb.len());
    overlap * 10 >= min_len * 8 && max_len - min_len <= min_len / 2 + 2
}

/// Drop the prefix of `next` that repeats the tail of `previous` (overlapping
/// Whisper windows restating the last few seconds).
pub fn strip_overlapping_prefix(previous: &str, next: &str) -> String {
    let prev: Vec<&str> = previous.split_whitespace().collect();
    let nxt: Vec<&str> = next.split_whitespace().collect();
    if prev.is_empty() || nxt.is_empty() {
        return next.trim().to_string();
    }
    let max = prev.len().min(nxt.len()).min(48);
    for n in (4..=max).rev() {
        if words_match_ci(&prev[prev.len() - n..], &nxt[..n]) {
            return nxt[n..].join(" ");
        }
    }
    for n in (6..=max).rev() {
        if fuzzy_word_run(&prev[prev.len() - n..], &nxt[..n]) {
            return nxt[n..].join(" ");
        }
    }
    next.trim().to_string()
}

fn fuzzy_word_run(a: &[&str], b: &[&str]) -> bool {
    if a.len() != b.len() || a.len() < 6 {
        return false;
    }
    let mismatches = a
        .iter()
        .zip(b)
        .filter(|(l, r)| l.to_lowercase() != r.to_lowercase())
        .count();
    mismatches <= a.len() / 6
}

fn collapse_word_runs(text: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    let mut run_word = "";
    let mut run_len = 0u32;
    for word in text.split_whitespace() {
        if !out.is_empty() && word.eq_ignore_ascii_case(run_word) {
            run_len += 1;
            if run_len < 3 && word.chars().count() > 3 {
                out.push(word);
            }
            continue;
        }
        run_word = word;
        run_len = 1;
        out.push(word);
    }
    out.join(" ")
}

fn collapse_repeated_phrases(text: &str) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() < 4 {
        return text.trim().to_string();
    }
    let mut out: Vec<&str> = Vec::new();
    let mut i = 0usize;
    while i < words.len() {
        let remaining = words.len() - i;
        let mut skipped = false;
        let max_n = remaining.min(12);
        for n in (2..=max_n).rev() {
            if i + n * 2 > words.len() {
                continue;
            }
            let phrase = &words[i..i + n];
            let mut copies = 1usize;
            let mut j = i + n;
            while j + n <= words.len() && words_match_ci(&words[j..j + n], phrase) {
                copies += 1;
                j += n;
            }
            let looped = (n >= 3 && copies >= 2) || copies >= 3;
            if looped {
                out.extend_from_slice(phrase);
                i = j;
                skipped = true;
                break;
            }
        }
        if !skipped {
            out.push(words[i]);
            i += 1;
        }
    }
    out.join(" ")
}

/// File / interview cleanup: n-gram loops, overlapping restatements, YouTube spam.
pub fn collapse_long_form_text(text: &str) -> String {
    let mut paragraphs: Vec<String> = Vec::new();
    for para in text.split("\n") {
        let mut piece = collapse_word_runs(para);
        piece = collapse_repeated_phrases(&piece);
        piece = remove_spam_spans(&piece);
        piece = collapse_consecutive_sentences(&piece);
        piece = strip_overlapping_inside(&piece);
        if piece.is_empty() || is_likely_hallucination(&piece) {
            continue;
        }
        paragraphs.push(piece);
    }
    paragraphs.join("\n")
}

fn remove_spam_spans(text: &str) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return String::new();
    }
    let phrases: Vec<Vec<&str>> = HALLUCINATION_PHRASES
        .iter()
        .map(|p| p.split_whitespace().collect())
        .filter(|p: &Vec<&str>| p.len() >= 3)
        .collect();
    let mut out: Vec<&str> = Vec::new();
    let mut i = 0usize;
    while i < words.len() {
        let mut skip = 0usize;
        for phrase in &phrases {
            if i + phrase.len() <= words.len()
                && words_match_ci(&words[i..i + phrase.len()], phrase)
            {
                skip = phrase.len();
                break;
            }
        }
        if skip > 0 {
            i += skip;
            continue;
        }
        out.push(words[i]);
        i += 1;
    }
    out.join(" ")
}

fn strip_overlapping_inside(text: &str) -> String {
    let sentences = sentence_parts(text);
    if sentences.len() < 2 {
        return text.trim().to_string();
    }
    let mut out: Vec<String> = Vec::new();
    for part in sentences {
        if let Some(prev) = out.last_mut() {
            let trimmed = strip_overlapping_prefix(prev, &part);
            if trimmed.is_empty() || sentences_are_echo(prev, &part) {
                if part.chars().count() > prev.chars().count() {
                    *prev = part;
                }
                continue;
            }
            if trimmed != part.trim() {
                if !prev.ends_with([' ', '\n']) {
                    prev.push(' ');
                }
                prev.push_str(&trimmed);
                continue;
            }
        }
        out.push(part);
    }
    out.join(" ")
}

fn words_match_ci(a: &[&str], b: &[&str]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(left, right)| left.to_lowercase() == right.to_lowercase())
}

/// Whisper often loops the last phrase on silence or overlapping windows.
pub fn collapse_echoed_transcript(text: &str) -> String {
    let sentences = collapse_long_form_text(text);
    let words: Vec<&str> = sentences.split_whitespace().collect();
    if words.len() < 12 {
        return sentences;
    }
    let max_period = (words.len() / 3).max(1);
    for period in 1..=max_period {
        let pattern = &words[..period];
        let mut i = 0;
        let mut copies = 0u32;
        let mut ok = true;
        while i < words.len() {
            let take = (words.len() - i).min(period);
            if !words_match_ci(&words[i..i + take], &pattern[..take]) {
                ok = false;
                break;
            }
            copies += 1;
            i += take;
        }
        if ok && copies >= 3 {
            return pattern.join(" ");
        }
    }
    sentences
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
        assert!(is_degenerate_repetition(
            &"подписывайтесь на канал ".repeat(20)
        ));
        assert!(is_likely_hallucination(
            "Делайте комментарии, пишите комментарии, подписывайтесь на канал, ставьте лайки"
        ));
    }

    #[test]
    fn collapses_phrase_loops_and_window_overlap() {
        let spam = "выбирал необходимые данные ".to_string()
            + &"подписывайтесь на канал ".repeat(18)
            + "выбирал необходимые данные и отправлял.";
        let out = collapse_long_form_text(&spam);
        assert!(out.contains("отправлял"), "{out}");
        assert!(
            out.to_lowercase().matches("подписывайтесь").count() <= 1,
            "{out}"
        );

        let overlap = "Я участвовал в тестировании интеграционном, то есть проверял конкретную работу микросервиса. Я участвовал в тестировании интеграционном, то есть проверял конкретную работу микросервиса, анализ бизнес-сценариев.";
        let out = collapse_long_form_text(overlap);
        assert_eq!(out.to_lowercase().matches("участвовал").count(), 1, "{out}");
        assert!(
            out.contains("бизнес-сценариев") || out.contains("бизнес"),
            "{out}"
        );

        let ands = format!("{} всё.", vec!["и"; 40].join(" "));
        let out = collapse_long_form_text(&ands);
        assert!(out.matches(" и ").count() <= 1, "{out}");

        let restated = strip_overlapping_prefix(
            "на том проекте это личный кабинет потребителя по поставке газа",
            "на том проекте это личный кабинет потребителя по поставке газа в жилые дома",
        );
        assert_eq!(restated, "в жилые дома");
    }

    #[test]
    fn collapses_whisper_looped_sentence() {
        let looped = "Я работаю над проектом с декабря 2022 года. ".repeat(14)
            + "Я работаю над проектом с декабря 2022 года. Я работаю над проектом с декаб.";
        let out = collapse_echoed_transcript(&looped);
        assert_eq!(out.matches("работаю").count(), 1, "{out}");
        assert!(out.contains("декабря 2022"), "{out}");
        assert!(!out.contains("декаб."), "{out}");
    }

    #[test]
    fn does_not_drop_a_new_sentence_that_reuses_last_words() {
        let text = "Я занимался тестированием API. Тестированием WebSocket занимался отдельно.";
        let out = collapse_echoed_transcript(text);
        assert!(out.to_lowercase().contains("websocket"), "{out}");
        assert!(out.to_lowercase().contains("отдельно"), "{out}");
    }

    #[test]
    fn keeps_two_different_sentences() {
        let text = "Я работаю над проектом. Завтра встретимся в офисе.";
        assert_eq!(collapse_echoed_transcript(text), text);
    }
}
