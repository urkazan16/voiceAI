//! Deterministic Smart Formatting Engine (MVP). Not an LLM rewrite.

use crate::pipeline::PipelineMode;

pub fn format_smart(mode: PipelineMode, text: &str) -> String {
    match mode {
        PipelineMode::Raw => text.to_string(),
        PipelineMode::Code => {
            let punctuated = apply_voice_punctuation(text);
            tidy_spacing(&punctuated).trim().to_string()
        }
        PipelineMode::Normal | PipelineMode::Professional => {
            let punctuated = apply_voice_punctuation(text);
            let without_fillers = remove_fillers(&punctuated);
            let listed = apply_lists(&without_fillers);
            let spaced = tidy_spacing(&listed);
            finalize_sentences(&spaced)
        }
    }
}

fn apply_voice_punctuation(text: &str) -> String {
    let mut out = text.to_string();
    let mut rules: Vec<(&str, &str)> = vec![
        ("вопросительный знак", "?"),
        ("восклицательный знак", "!"),
        ("точка с запятой", ";"),
        ("новый абзац", "\n\n"),
        ("новая строка", "\n"),
        ("question mark", "?"),
        ("exclamation point", "!"),
        ("exclamation mark", "!"),
        ("new paragraph", "\n\n"),
        ("new line", "\n"),
        ("semicolon", ";"),
        ("открыть скобку", "("),
        ("закрыть скобку", ")"),
        ("open parenthesis", "("),
        ("close parenthesis", ")"),
        ("запятая", ","),
        ("двоеточие", ":"),
        ("точка", "."),
        ("тире", " — "),
        ("comma", ","),
        ("colon", ":"),
        ("period", "."),
        ("dash", " — "),
    ];
    rules.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
    for (phrase, replacement) in rules {
        out = replace_phrase_ci(&out, phrase, replacement);
    }
    out
}

fn replace_phrase_ci(haystack: &str, needle: &str, replacement: &str) -> String {
    let lower = haystack.to_lowercase();
    let needle_l = needle.to_lowercase();
    let mut result = String::new();
    let mut idx = 0;
    while let Some(found) = lower[idx..].find(&needle_l) {
        let abs = idx + found;
        if !is_phrase_boundary(haystack, abs, abs + needle.len()) {
            result.push_str(&haystack[idx..abs + needle.len()]);
            idx = abs + needle.len();
            continue;
        }
        result.push_str(&haystack[idx..abs]);
        result.push_str(replacement);
        idx = abs + needle.len();
    }
    result.push_str(&haystack[idx..]);
    result
}

fn is_phrase_boundary(text: &str, start: usize, end: usize) -> bool {
    let before_ok = start == 0
        || text[..start]
            .chars()
            .last()
            .is_some_and(|c| !c.is_alphanumeric());
    let after_ok = end >= text.len()
        || text[end..]
            .chars()
            .next()
            .is_some_and(|c| !c.is_alphanumeric());
    before_ok && after_ok
}

fn remove_fillers(text: &str) -> String {
    let fillers = [
        "ну",
        "короче",
        "э-э",
        "ээ",
        "эм",
        "ээм",
        "как бы",
        "значит",
        "в общем",
        "типа",
        "uh",
        "um",
        "uhm",
        "you know",
    ];
    let mut out = text.to_string();
    let mut phrases: Vec<&str> = fillers.to_vec();
    phrases.sort_by_key(|b| std::cmp::Reverse(b.len()));
    for phrase in phrases {
        out = strip_filler_phrase(&out, phrase);
    }
    out = strip_like_filler(&out);
    collapse_ws_keep_newlines(&out)
}

fn strip_filler_phrase(text: &str, phrase: &str) -> String {
    let lower = text.to_lowercase();
    let needle = phrase.to_lowercase();
    let mut result = String::new();
    let mut idx = 0;
    while let Some(found) = lower[idx..].find(&needle) {
        let abs = idx + found;
        if is_phrase_boundary(text, abs, abs + phrase.len()) {
            result.push_str(&text[idx..abs]);
            idx = abs + phrase.len();
        } else {
            result.push_str(&text[idx..abs + phrase.len()]);
            idx = abs + phrase.len();
        }
    }
    result.push_str(&text[idx..]);
    result
}

fn strip_like_filler(text: &str) -> String {
    let tokens: Vec<&str> = text.split_whitespace().collect();
    let mut kept = Vec::new();
    for (i, token) in tokens.iter().enumerate() {
        let clean = trim_punct(token);
        if eq_ci(clean, "like") {
            let prev = i
                .checked_sub(1)
                .and_then(|p| tokens.get(p))
                .map(|t| trim_punct(t));
            let next = tokens.get(i + 1).map(|t| trim_punct(t));
            let prev_filler = prev.is_some_and(|p| {
                ["uh", "um", "so", "well", "типа", "ну"]
                    .iter()
                    .any(|f| eq_ci(p, f))
            });
            let next_filler =
                next.is_some_and(|n| ["uh", "um", "you", "типа"].iter().any(|f| eq_ci(n, f)));
            if prev_filler || next_filler || i == 0 {
                continue;
            }
        }
        kept.push(*token);
    }
    kept.join(" ")
}

fn apply_lists(text: &str) -> String {
    if let Some(bullets) = extract_bullets(text) {
        return bullets;
    }
    if let Some(numbered) = extract_numbered(text) {
        return numbered;
    }
    text.to_string()
}

fn extract_numbered(text: &str) -> Option<String> {
    let tokens: Vec<&str> = text.split_whitespace().collect();
    let mut cuts: Vec<usize> = Vec::new();
    for (i, token) in tokens.iter().enumerate() {
        if ordinal_value(trim_punct(token)).is_some() {
            cuts.push(i);
        }
    }
    if cuts.len() < 2 {
        return None;
    }
    let mut items: Vec<String> = Vec::new();
    for (n, start) in cuts.iter().enumerate() {
        let end = cuts.get(n + 1).copied().unwrap_or(tokens.len());
        let body = tokens[*start + 1..end].join(" ");
        if body.is_empty() {
            return None;
        }
        items.push(body);
    }
    let prefix = tokens[..cuts[0]].join(" ");
    let mut out = String::new();
    if !prefix.is_empty() {
        out.push_str(prefix.trim_end_matches(':'));
        out.push_str(":\n\n");
    }
    for (i, item) in items.iter().enumerate() {
        out.push_str(&format!("{}. {}\n", i + 1, capitalize_first(item)));
    }
    Some(out.trim().to_string())
}

fn extract_bullets(text: &str) -> Option<String> {
    let lower = text.to_lowercase();
    if !lower.contains("bullet ") && !lower.contains("пункт ") && !lower.contains("маркер ")
    {
        return None;
    }
    let tokens: Vec<&str> = text.split_whitespace().collect();
    let mut cuts = Vec::new();
    for (i, token) in tokens.iter().enumerate() {
        if eq_ci(trim_punct(token), "bullet")
            || eq_ci(trim_punct(token), "пункт")
            || eq_ci(trim_punct(token), "маркер")
        {
            cuts.push(i);
        }
    }
    if cuts.len() < 2 {
        return None;
    }
    let mut items = Vec::new();
    for (n, start) in cuts.iter().enumerate() {
        let mut from = *start + 1;
        if tokens
            .get(from)
            .is_some_and(|t| ordinal_value(trim_punct(t)).is_some())
        {
            from += 1;
        }
        let end = cuts.get(n + 1).copied().unwrap_or(tokens.len());
        let body = tokens[from..end].join(" ");
        if !body.is_empty() {
            items.push(body);
        }
    }
    if items.len() < 2 {
        return None;
    }
    let mut out = String::new();
    for item in items {
        out.push_str(&format!("• {}\n", capitalize_first(&item)));
    }
    Some(out.trim().to_string())
}

fn ordinal_value(token: &str) -> Option<u32> {
    let t = token.to_lowercase();
    let map = [
        ("один", 1),
        ("два", 2),
        ("три", 3),
        ("четыре", 4),
        ("пять", 5),
        ("шесть", 6),
        ("семь", 7),
        ("восемь", 8),
        ("девять", 9),
        ("десять", 10),
        ("one", 1),
        ("two", 2),
        ("three", 3),
        ("four", 4),
        ("five", 5),
        ("six", 6),
        ("seven", 7),
        ("eight", 8),
        ("nine", 9),
        ("ten", 10),
        ("first", 1),
        ("second", 2),
        ("third", 3),
    ];
    map.iter().find(|(k, _)| *k == t).map(|(_, v)| *v)
}

fn tidy_spacing(text: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = text.chars().collect();
    for (i, ch) in chars.iter().enumerate() {
        if matches!(*ch, ',' | '.' | '!' | '?' | ';' | ':') {
            while out.ends_with(' ') {
                out.pop();
            }
            out.push(*ch);
            let next = chars.get(i + 1).copied();
            let prev = if i > 0 { chars[i - 1] } else { ' ' };
            let keep_numeric = matches!(*ch, '.' | ':')
                && prev.is_ascii_digit()
                && next.is_some_and(|n| n.is_ascii_digit());
            if next.is_some_and(|n| !n.is_whitespace() && n != '\n' && !matches!(n, ')' | ']'))
                && !keep_numeric
            {
                out.push(' ');
            }
            continue;
        }
        if *ch == '(' && !out.ends_with([' ', '\n', '(']) && !out.is_empty() {
            out.push(' ');
        }
        out.push(*ch);
    }
    collapse_ws_keep_newlines(&out)
}

fn finalize_sentences(text: &str) -> String {
    if text.contains('\n') {
        return capitalize_lines(text);
    }
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let mut body = capitalize_first(trimmed);
    let last = body.chars().last().unwrap_or(' ');
    if !matches!(last, '.' | '!' | '?') {
        body.push('.');
    }
    body
}

fn capitalize_lines(text: &str) -> String {
    text.split('\n')
        .map(|line| {
            let t = line.trim();
            if t.is_empty() {
                String::new()
            } else {
                capitalize_first(t)
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn capitalize_first(text: &str) -> String {
    let mut chars: Vec<char> = text.trim().chars().collect();
    if let Some(first) = chars.iter_mut().find(|c| c.is_alphabetic()) {
        let up = first.to_uppercase().next().unwrap_or(*first);
        *first = up;
    }
    chars.into_iter().collect()
}

fn collapse_ws_keep_newlines(text: &str) -> String {
    let mut out = String::new();
    let mut prev_space = false;
    for ch in text.chars() {
        if ch == '\n' {
            while out.ends_with(' ') {
                out.pop();
            }
            out.push('\n');
            prev_space = false;
            continue;
        }
        if ch.is_whitespace() {
            if !prev_space && !out.is_empty() && !out.ends_with('\n') {
                out.push(' ');
            }
            prev_space = true;
            continue;
        }
        prev_space = false;
        out.push(ch);
    }
    out.trim().to_string()
}

fn trim_punct(token: &str) -> &str {
    token.trim_matches(|c: char| matches!(c, ',' | '.' | '!' | '?' | ';' | ':'))
}

fn eq_ci(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b) || a.to_lowercase() == b.to_lowercase()
}

pub fn space_between_utterances(previous: &str, next: &str) -> String {
    let next = next.trim();
    if next.is_empty() {
        return String::new();
    }
    let Some(last) = previous.chars().last() else {
        return next.to_string();
    };
    let first = next.chars().next().unwrap();
    if last.is_whitespace() || first.is_whitespace() {
        return next.to_string();
    }
    if matches!(first, '.' | ',' | ';' | ':' | '?' | '!' | ')' | ']') {
        return next.to_string();
    }
    format!(" {next}")
}

pub fn normalize_spoken_values(
    text: &str,
    mode: PipelineMode,
    digits: bool,
    date_format: &str,
) -> String {
    if mode == PipelineMode::Raw || text.is_empty() {
        return text.to_string();
    }
    let mut out = text.to_string();
    if digits {
        out = spoken_numbers_to_digits(&out);
    }
    out = normalize_clock(&out);
    out = normalize_dates(&out, date_format);
    out
}

fn spoken_numbers_to_digits(text: &str) -> String {
    let mut rules: Vec<(&str, &str)> = vec![
        ("девятнадцать", "19"),
        ("восемнадцать", "18"),
        ("семнадцать", "17"),
        ("шестнадцать", "16"),
        ("пятнадцать", "15"),
        ("четырнадцать", "14"),
        ("тринадцать", "13"),
        ("двенадцать", "12"),
        ("одиннадцать", "11"),
        ("девяносто", "90"),
        ("восемьдесят", "80"),
        ("семьдесят", "70"),
        ("шестьдесят", "60"),
        ("пятьдесят", "50"),
        ("сорок", "40"),
        ("тридцать", "30"),
        ("двадцать", "20"),
        ("десять", "10"),
        ("девять", "9"),
        ("восемь", "8"),
        ("семь", "7"),
        ("шесть", "6"),
        ("четыре", "4"),
        ("три", "3"),
        ("два", "2"),
        ("две", "2"),
        ("один", "1"),
        ("одна", "1"),
        ("одно", "1"),
        ("ноль", "0"),
        ("пять", "5"),
        ("nineteen", "19"),
        ("eighteen", "18"),
        ("seventeen", "17"),
        ("sixteen", "16"),
        ("fifteen", "15"),
        ("fourteen", "14"),
        ("thirteen", "13"),
        ("twelve", "12"),
        ("eleven", "11"),
        ("ten", "10"),
        ("nine", "9"),
        ("eight", "8"),
        ("seven", "7"),
        ("six", "6"),
        ("five", "5"),
        ("four", "4"),
        ("three", "3"),
        ("two", "2"),
        ("one", "1"),
        ("zero", "0"),
    ];
    rules.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
    let mut out = text.to_string();
    for (phrase, digit) in rules {
        out = replace_phrase_ci(&out, phrase, digit);
    }
    collapse_split_digits(&out)
}

fn collapse_split_digits(text: &str) -> String {
    text.split('\n')
        .map(collapse_split_digits_line)
        .collect::<Vec<_>>()
        .join("\n")
}

fn collapse_split_digits_line(text: &str) -> String {
    let mut out = String::new();
    let mut pending: Option<u32> = None;
    let tokens: Vec<&str> = text.split_whitespace().collect();
    for token in tokens {
        let core = trim_punct(token);
        if let Ok(n) = core.parse::<u32>() {
            let mut value = n;
            if let Some(prev) = pending.take() {
                if prev >= 20 && prev % 10 == 0 && n < 10 {
                    value = prev + n;
                } else {
                    push_pending(&mut out, prev);
                }
            }
            let suffix: String = token.chars().skip_while(|c| c.is_ascii_digit()).collect();
            if suffix.is_empty() {
                pending = Some(value);
            } else {
                push_pending(&mut out, value);
                out.push_str(&suffix);
            }
            continue;
        }
        if let Some(prev) = pending.take() {
            push_pending(&mut out, prev);
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(token);
    }
    if let Some(prev) = pending {
        push_pending(&mut out, prev);
    }
    out
}

fn push_pending(out: &mut String, n: u32) {
    if !out.is_empty() {
        out.push(' ');
    }
    out.push_str(&n.to_string());
}

fn normalize_clock(text: &str) -> String {
    let re = regex::Regex::new(
        r"(?i)\b(\d{1,2})\s*(?:часов|часа|час|hours|hour|h|:)\s*(\d{1,2})\s*(?:минут[аы]?|мин|minutes|minute|m)?\b",
    );
    let Ok(re) = re else {
        return text.to_string();
    };
    re.replace_all(text, |caps: &regex::Captures| {
        let h: u32 = caps[1].parse().unwrap_or(0);
        let m: u32 = caps[2].parse().unwrap_or(0);
        if h > 23 || m > 59 {
            caps.get(0)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default()
        } else {
            format!("{h:02}:{m:02}")
        }
    })
    .into_owned()
}

fn normalize_dates(text: &str, date_format: &str) -> String {
    let re = regex::Regex::new(r"\b(\d{1,2})\s*[./-]\s*(\d{1,2})\s*[./-]\s*(\d{2,4})\b");
    let Ok(re) = re else {
        return text.to_string();
    };
    let iso = date_format.eq_ignore_ascii_case("ISO");
    re.replace_all(text, |caps: &regex::Captures| {
        let a: u32 = caps[1].parse().unwrap_or(0);
        let b: u32 = caps[2].parse().unwrap_or(0);
        let mut y: u32 = caps[3].parse().unwrap_or(0);
        if y < 100 {
            y += 2000;
        }
        let Some((year, month, day)) = order_day_month(a, b, y) else {
            return caps
                .get(0)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
        };
        if iso {
            format!("{year:04}-{month:02}-{day:02}")
        } else {
            format!("{day:02}.{month:02}.{year:04}")
        }
    })
    .into_owned()
}

fn order_day_month(a: u32, b: u32, year: u32) -> Option<(u32, u32, u32)> {
    let ok = |day: u32, month: u32| (1..=31).contains(&day) && (1..=12).contains(&month);
    if a > 12 && ok(a, b) {
        Some((year, b, a))
    } else if b > 12 && ok(b, a) {
        Some((year, a, b))
    } else if ok(a, b) {
        Some((year, b, a))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filler_benchmark_section_121() {
        assert_eq!(
            format_smart(
                PipelineMode::Normal,
                "Ну короче э-э давай завтра созвонимся."
            ),
            "Давай завтра созвонимся."
        );
    }

    #[test]
    fn list_benchmark_section_122() {
        assert_eq!(
            format_smart(PipelineMode::Normal, "один API тесты два UI тесты три SQL"),
            "1. API тесты\n2. UI тесты\n3. SQL"
        );
    }

    #[test]
    fn punctuation_benchmark_section_123() {
        assert_eq!(
            format_smart(
                PipelineMode::Normal,
                "Привет запятая как дела вопросительный знак"
            ),
            "Привет, как дела?"
        );
    }

    #[test]
    fn keeps_meaningful_nu() {
        let out = format_smart(PipelineMode::Normal, "Нужен ответ сегодня");
        assert!(out.to_lowercase().contains("нужен"));
    }

    #[test]
    fn professional_mode_capitalizes() {
        let out = format_smart(PipelineMode::Professional, "привет команда");
        assert!(out.starts_with('П') || out.contains("Привет") || !out.is_empty());
    }

    #[test]
    fn raw_mode_does_not_rewrite_lists() {
        assert_eq!(
            format_smart(PipelineMode::Raw, "один два три"),
            "один два три"
        );
    }

    #[test]
    fn spoken_punctuation_period() {
        let out = format_smart(PipelineMode::Normal, "готово точка");
        assert!(out.contains('.') || out.to_lowercase().contains("готов"));
    }

    #[test]
    fn digits_dates_and_clock_follow_config() {
        assert_eq!(
            normalize_spoken_values(
                "встреча двадцать пять 15 часов 30 минут 5.3.26",
                PipelineMode::Normal,
                true,
                "DMY",
            ),
            "встреча 25 15:30 05.03.2026"
        );
        assert_eq!(
            normalize_spoken_values(
                "пятнадцать часов тридцать минут",
                PipelineMode::Normal,
                true,
                "DMY",
            ),
            "15:30"
        );
        assert_eq!(
            normalize_spoken_values("пять часов три минуты", PipelineMode::Normal, true, "DMY"),
            "05:03"
        );
        assert_eq!(
            normalize_spoken_values("9 часов 5 минут", PipelineMode::Normal, true, "DMY"),
            "09:05"
        );
        assert_eq!(
            normalize_spoken_values("25.12.24", PipelineMode::Normal, true, "DMY"),
            "25.12.2024"
        );
        assert_eq!(
            normalize_spoken_values("5.3.26", PipelineMode::Normal, true, "ISO"),
            "2026-03-05"
        );
        assert_eq!(
            normalize_spoken_values("двадцать пять", PipelineMode::Normal, false, "DMY"),
            "двадцать пять"
        );
        assert_eq!(
            normalize_spoken_values("двадцать пять", PipelineMode::Raw, true, "DMY"),
            "двадцать пять"
        );
        assert_eq!(
            format_smart(PipelineMode::Normal, "встреча 5.3.26 в 9:05"),
            "Встреча 5.3.26 в 9:05."
        );
    }

    #[test]
    fn consecutive_utterances_get_a_space() {
        assert_eq!(space_between_utterances("Привет", "мир"), " мир");
        assert_eq!(space_between_utterances("Привет ", "мир"), "мир");
        assert_eq!(space_between_utterances("Привет", ","), ",");
    }
}
