//! Session-aware self-correction (Wispr-style Backtrack), not a generic rewrite.

#[derive(Debug, Default, Clone)]
pub struct BacktrackEngine {
    pub session: String,
}

const PREPS: &[&str] = &[
    "в", "во", "на", "к", "ко", "с", "со", "у", "о", "об", "от", "до", "из", "по", "за", "для",
    "at", "in", "on", "to", "from", "about",
];

pub fn apply(text: &str, session: &str) -> String {
    let trimmed = collapse_ws(text);
    if trimmed.is_empty() {
        return String::new();
    }
    let working = if is_correction_only(&trimmed) && !session.trim().is_empty() {
        format!(
            "{}, {}",
            session.trim().trim_end_matches(['.', '!', '?']),
            trimmed
        )
    } else {
        trimmed
    };
    let mut out = working;
    for _ in 0..4 {
        let next = apply_once(&out);
        if next == out {
            break;
        }
        out = next;
    }
    collapse_ws(&out)
}

fn apply_once(text: &str) -> String {
    if let Some(out) = apply_scratch(text) {
        return out;
    }
    if let Some((left, right)) = split_correction(text) {
        return align_replace(&left, &right);
    }
    text.to_string()
}

fn apply_scratch(text: &str) -> Option<String> {
    let lower = text.to_lowercase();
    for marker in ["scratch that", "never mind", "забудь это", "забудь"] {
        if let Some(idx) = find_phrase(&lower, marker) {
            if idx == 0 {
                return Some(String::new());
            }
            let before = text[..idx].trim_end_matches([' ', ',', ';', '—', '-']);
            if let Some(sentence_start) = before.rfind(['.', '!', '?', '\n']) {
                return Some(before[..=sentence_start].trim().to_string());
            }
            return Some(String::new());
        }
    }
    None
}

fn split_correction(text: &str) -> Option<(String, String)> {
    let lower = text.to_lowercase();
    let markers = [
        ", нет,",
        ", нет ",
        " нет, ",
        " нет ",
        ", точнее ",
        " точнее ",
        ", вернее ",
        " вернее ",
        ", actually, ",
        ", actually ",
        " actually ",
        ", rather ",
        " rather ",
        ", то есть ",
        " я хотел сказать ",
    ];
    let mut best: Option<(usize, usize)> = None;
    for marker in markers {
        if marker.contains("точнее") && lower.contains("точнее говоря") {
            continue;
        }
        if let Some(idx) = find_phrase(&lower, marker) {
            if !is_plausible_correction(&lower, idx, marker.len()) {
                continue;
            }
            if is_weak_marker(marker)
                && !looks_like_value_swap(&text[..idx], &text[idx + marker.len()..])
            {
                continue;
            }
            if best.map(|(i, _)| idx < i).unwrap_or(true) {
                best = Some((idx, marker.len()));
            }
        }
    }
    let (idx, len) = best?;
    let left = text[..idx].trim().trim_end_matches(',').trim().to_string();
    let right = text[idx + len..]
        .trim()
        .trim_start_matches(',')
        .trim()
        .to_string();
    if left.is_empty() || right.is_empty() {
        return None;
    }
    Some((left, right))
}

fn is_weak_marker(marker: &str) -> bool {
    matches!(
        marker,
        " нет "
            | " точнее "
            | " вернее "
            | " actually "
            | " rather "
            | ", то есть "
            | " я хотел сказать "
    )
}

fn looks_like_value_swap(left: &str, right: &str) -> bool {
    let left_last = left
        .split_whitespace()
        .last()
        .unwrap_or("")
        .trim_matches(|c: char| !c.is_alphanumeric());
    let right_toks: Vec<&str> = right.split_whitespace().collect();
    let right_focus = right_toks
        .last()
        .unwrap_or(&"")
        .trim_matches(|c: char| !c.is_alphanumeric());
    is_value_token(left_last) && is_value_token(right_focus)
        || PREPS.iter().any(|p| {
            left.split_whitespace().any(|t| eq_ci(t, p))
                && right_toks.first().is_some_and(|t| eq_ci(t, p))
        })
}

fn is_value_token(token: &str) -> bool {
    if token.is_empty() {
        return false;
    }
    if token.chars().all(|c| c.is_ascii_digit()) {
        return true;
    }
    const NUM: &[&str] = &[
        "один",
        "два",
        "три",
        "четыре",
        "пять",
        "шесть",
        "семь",
        "восемь",
        "девять",
        "десять",
        "one",
        "two",
        "three",
        "four",
        "five",
        "six",
        "seven",
        "eight",
        "nine",
        "ten",
    ];
    NUM.iter().any(|n| eq_ci(n, token))
}

fn is_plausible_correction(lower: &str, idx: usize, marker_len: usize) -> bool {
    let before = lower[..idx].trim();
    let after = lower[idx + marker_len..].trim();
    if before.split_whitespace().count() < 2 || after.split_whitespace().count() < 1 {
        return false;
    }
    let before_last = before.split_whitespace().last().unwrap_or("");
    let after_first = after.split_whitespace().next().unwrap_or("");
    if before_last == after_first {
        return false;
    }
    true
}

fn is_correction_only(text: &str) -> bool {
    let lower = text.to_lowercase();
    let prefixes = [
        "нет,",
        "нет ",
        "точнее ",
        "вернее ",
        "actually ",
        "scratch that",
        "rather ",
    ];
    prefixes.iter().any(|p| lower.starts_with(p)) && lower.split_whitespace().count() <= 6
}

fn align_replace(left: &str, right: &str) -> String {
    let left_tokens: Vec<&str> = left.split_whitespace().collect();
    let right_tokens: Vec<&str> = right.split_whitespace().collect();
    if left_tokens.is_empty() {
        return right.to_string();
    }
    if let Some(prep) = right_tokens.first() {
        if PREPS.iter().any(|p| eq_ci(p, prep)) {
            if let Some(pos) = left_tokens.iter().rposition(|t| eq_ci(t, prep)) {
                let mut out = left_tokens[..pos].join(" ");
                if !out.is_empty() {
                    out.push(' ');
                }
                out.push_str(right.trim_end_matches(['.', '!', '?']));
                let end = match trailing_punct(right) {
                    "" => trailing_punct(left),
                    other => other,
                };
                if !out.ends_with(['.', '!', '?']) {
                    out.push_str(end);
                }
                return collapse_ws(&out);
            }
        }
    }
    let mut out = left_tokens[..left_tokens.len().saturating_sub(1)].join(" ");
    if !out.is_empty() {
        out.push(' ');
    }
    out.push_str(right.trim_end_matches(['.', '!', '?']));
    let end = match trailing_punct(right) {
        "" => trailing_punct(left),
        other => other,
    };
    if !out.ends_with(['.', '!', '?']) {
        out.push_str(end);
    }
    collapse_ws(&out)
}

fn trailing_punct(text: &str) -> &'static str {
    match text.trim().chars().last() {
        Some('.') => ".",
        Some('!') => "!",
        Some('?') => "?",
        _ => "",
    }
}

fn find_phrase(haystack: &str, needle: &str) -> Option<usize> {
    haystack.find(needle)
}

fn eq_ci(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b) || a.to_lowercase() == b.to_lowercase()
}

fn collapse_ws(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backtrack_benchmark_section_120() {
        assert_eq!(
            apply("Давай встретимся в пять, нет, в шесть.", ""),
            "Давай встретимся в шесть."
        );
        assert_eq!(
            apply("Давай встретимся в 5 нет в 6.", ""),
            "Давай встретимся в 6."
        );
    }

    #[test]
    fn ignores_isolated_tochnee() {
        let text = "Нужно сказать точнее что имеется в виду";
        assert_eq!(apply(text, ""), collapse_ws(text));
    }

    #[test]
    fn scratch_that_drops_last_sentence() {
        assert_eq!(
            apply("Первая мысль. Вторая мысль scratch that", ""),
            "Первая мысль."
        );
    }

    #[test]
    fn actually_and_rather_replace_the_tail() {
        assert_eq!(apply("Meet at five, actually six", ""), "Meet at six");
        assert_eq!(apply("Meet at five rather six", ""), "Meet at six");
        assert_eq!(apply("забудь это", "Черновик письма"), "");
        assert_eq!(
            apply("Первая мысль. Вторая мысль never mind", ""),
            "Первая мысль."
        );
    }

    #[test]
    fn russian_tochnee_replaces_time() {
        assert_eq!(
            apply("Давай встретимся в пять, точнее в семь.", ""),
            "Давай встретимся в семь."
        );
        assert_eq!(
            apply("Сегодня встречаемся в четыре, нет, в пять.", ""),
            "Сегодня встречаемся в пять."
        );
    }
}
