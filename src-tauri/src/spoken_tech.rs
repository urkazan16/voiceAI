//! Reconstruct technical literals from dictated speech.
//!
//! Whisper hears identifiers as ordinary words, so `436a2969-ca7e-…` arrives as
//! `четыре три шесть а два девять шесть девять дефис це а семь и …`. This stage
//! turns those runs back into literals. It has to run *before* the punctuation
//! pass, because that pass rewrites the word "точка" into "." and would destroy
//! domains, versions, and file names on the way through.
//!
//! Everything here is deterministic and reversible by inspection — no model is
//! involved, so a run either matches a shape we recognise or is left untouched.

/// Rebuild identifiers, hashes, GUIDs, domains, versions, and file names.
pub fn apply(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }
    text.split('\n')
        .map(apply_line)
        .collect::<Vec<_>>()
        .join("\n")
}

fn apply_line(line: &str) -> String {
    let tokens: Vec<String> = line.split_whitespace().map(str::to_string).collect();
    if tokens.is_empty() {
        return String::new();
    }
    let tokens = expand_spelled_runs(tokens);
    let tokens = join_hex_runs(tokens);
    let tokens = join_versions(tokens);
    let tokens = join_dotted_names(tokens);
    tokens.join(" ")
}

/// A word the sentence formatter must not capitalize or end with a period.
pub fn looks_technical(word: &str) -> bool {
    // Sentence punctuation at the end is not part of the token: "готово." is
    // ordinary prose, while "script.sh" and ".NET" are not.
    let trimmed = word
        .trim_matches(|c: char| matches!(c, ',' | '!' | '?' | ';' | '«' | '»' | '"'))
        .trim_end_matches(['.', ':']);
    if trimmed.len() < 2 {
        return false;
    }
    let has_digit = trimmed.chars().any(|c| c.is_ascii_digit());
    let has_alpha = trimmed.chars().any(|c| c.is_alphabetic());
    let has_glue = trimmed
        .chars()
        .any(|c| matches!(c, '.' | '/' | '_' | '@' | ':' | '\\'));
    if has_glue && has_alpha {
        return true;
    }
    if has_digit && has_alpha && trimmed.contains('-') {
        return true;
    }
    is_guid(trimmed) || is_hash_like(trimmed)
}

fn is_hash_like(word: &str) -> bool {
    let core = word.trim_end_matches(['.', ',']);
    core.len() >= 7
        && core.chars().all(|c| c.is_ascii_hexdigit())
        && core.chars().any(|c| c.is_ascii_alphabetic())
        && core.chars().any(|c| c.is_ascii_digit())
}

// ---------------------------------------------------------------- token affixes

/// Split a token into leading punctuation, the word itself, and trailing punctuation.
fn split_affixes(token: &str) -> (&str, &str, &str) {
    let inner = |c: char| c.is_alphanumeric() || c == '_';
    let Some(start) = token.find(inner) else {
        return (token, "", "");
    };
    let end = token
        .char_indices()
        .filter(|(_, c)| inner(*c))
        .next_back()
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(start);
    (&token[..start], &token[start..end], &token[end..])
}

fn core(token: &str) -> String {
    split_affixes(token).1.to_lowercase()
}

fn trailing(token: &str) -> &str {
    split_affixes(token).2
}

// ---------------------------------------------------------------- spoken tables

/// Latin letters as they are dictated, in preference order.
///
/// Several names are genuinely ambiguous — Russian "и" is the English name of
/// `e` but also the letter `i` — so callers pick the candidate that fits the
/// context they are in (hexadecimal runs only accept `a`–`f`).
fn spoken_letters(word: &str) -> &'static [char] {
    match word {
        "a" | "эй" | "alpha" | "alfa" | "альфа" => &['a'],
        "а" => &['a'],
        "b" | "би" | "бэ" | "бе" | "б" | "bravo" | "браво" => &['b'],
        "c" | "си" | "цэ" | "це" | "ц" | "charlie" | "чарли" => &['c'],
        "d" | "ди" | "дэ" | "де" | "д" | "delta" | "дельта" => &['d'],
        "e" | "е" | "echo" | "эхо" => &['e'],
        "и" => &['i', 'e'],
        "f" | "эф" | "ф" | "foxtrot" | "фокстрот" => &['f'],
        "g" | "джи" | "гэ" | "ге" | "г" | "golf" | "гольф" => &['g'],
        "h" | "эйч" | "аш" | "ха" | "х" | "hotel" | "отель" => &['h'],
        "i" | "ай" | "india" | "индия" => &['i'],
        "j" | "джей" | "йот" | "juliet" | "джульетта" => &['j'],
        "k" | "кей" | "ка" | "к" | "kilo" | "кило" => &['k'],
        "l" | "эль" | "эл" | "л" | "lima" | "лима" => &['l'],
        "m" | "эм" | "м" | "mike" | "майк" => &['m'],
        "n" | "эн" | "н" | "november" | "ноябрь" => &['n'],
        "o" | "оу" | "о" | "oscar" | "оскар" => &['o'],
        "p" | "пи" | "пэ" | "пе" | "п" | "papa" | "папа" => &['p'],
        "q" | "кью" | "ку" | "quebec" | "квебек" => &['q'],
        "r" | "ар" | "эр" | "р" | "romeo" | "ромео" => &['r'],
        "s" | "эс" | "с" | "sierra" | "сьерра" => &['s'],
        "t" | "ти" | "тэ" | "те" | "т" | "tango" | "танго" => &['t'],
        "u" | "ю" | "у" | "uniform" | "униформа" => &['u'],
        "v" | "ви" | "вэ" | "ве" | "в" | "victor" | "виктор" => &['v'],
        "w" | "дабл-ю" | "даблъю" | "даблю" | "дубль-вэ" | "whiskey" | "виски" => {
            &['w']
        }
        "x" | "икс" | "xray" | "x-ray" | "рентген" => &['x'],
        "y" | "уай" | "игрек" | "yankee" | "янки" => &['y'],
        "z" | "зет" | "зед" | "зэт" | "з" | "zulu" | "зулу" => &['z'],
        _ => &[],
    }
}

fn spoken_digit(word: &str) -> Option<char> {
    match word {
        "0" | "ноль" | "нуль" | "zero" | "oh" => Some('0'),
        "1" | "один" | "одна" | "одно" | "one" => Some('1'),
        "2" | "два" | "две" | "two" => Some('2'),
        "3" | "три" | "three" => Some('3'),
        "4" | "четыре" | "four" => Some('4'),
        "5" | "пять" | "five" => Some('5'),
        "6" | "шесть" | "six" => Some('6'),
        "7" | "семь" | "seven" => Some('7'),
        "8" | "восемь" | "eight" => Some('8'),
        "9" | "девять" | "nine" => Some('9'),
        _ => None,
    }
}

fn spoken_separator(word: &str) -> Option<char> {
    match word {
        "дефис" | "тире" | "минус" | "hyphen" | "dash" | "minus" => Some('-'),
        "точка" | "точку" | "dot" | "point" | "period" => Some('.'),
        "слэш" | "слеш" | "slash" => Some('/'),
        "подчёркивание" | "подчеркивание" | "андерскор" | "underscore" => {
            Some('_')
        }
        "двоеточие" | "colon" => Some(':'),
        "собака" | "at" => Some('@'),
        "плюс" | "plus" => Some('+'),
        _ => None,
    }
}

/// Single characters a spelled-out run may contain.
fn spelled_char(token: &str) -> Option<char> {
    let word = core(token);
    if word.is_empty() {
        let raw = token.trim();
        return raw
            .chars()
            .next()
            .filter(|_| raw.chars().count() == 1)
            .filter(|c| matches!(c, '-' | '.' | '/' | '_' | ':' | '@' | '+'));
    }
    spoken_digit(&word)
        .or_else(|| spoken_letters(&word).first().copied())
        .or_else(|| spoken_separator(&word))
}

/// One or more hexadecimal characters, as heard.
fn hex_fragment(token: &str) -> Option<String> {
    let word = core(token);
    if word.is_empty() {
        return None;
    }
    if word.chars().all(|c| c.is_ascii_hexdigit()) {
        return Some(word);
    }
    if let Some(digit) = spoken_digit(&word) {
        return Some(digit.to_string());
    }
    spoken_letters(&word)
        .iter()
        .find(|c| c.is_ascii_hexdigit())
        .map(|c| c.to_string())
}

/// Words that are letters of the alphabet *and* very common Russian function
/// words. A spelled run stops before one of these when the next token is
/// ordinary prose, so "эй би си и потом" does not become "abci потом".
fn is_ambiguous_short_word(word: &str) -> bool {
    matches!(
        word,
        "а" | "и" | "о" | "у" | "я" | "с" | "в" | "к" | "е" | "ю" | "не" | "то"
    )
}

// ---------------------------------------------------------------- spelled runs

const SPELL_TRIGGERS: &[&[&str]] = &[
    &["по", "буквам"],
    &["по", "символам"],
    &["по", "символьно"],
    &["буквами"],
    &["символами"],
    &["spell"],
    &["spelled"],
];

const RUN_TERMINATORS: &[&str] = &["конец", "стоп", "всё", "все", "end", "stop"];

fn match_trigger(tokens: &[String], at: usize, table: &[&[&str]]) -> Option<usize> {
    table
        .iter()
        .filter(|phrase| {
            phrase
                .iter()
                .enumerate()
                .all(|(k, want)| tokens.get(at + k).map(|t| core(t)).as_deref() == Some(*want))
        })
        .map(|phrase| phrase.len())
        .max()
}

fn expand_spelled_runs(tokens: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(tokens.len());
    let mut i = 0;
    while i < tokens.len() {
        let Some(trigger_len) = match_trigger(&tokens, i, SPELL_TRIGGERS) else {
            out.push(tokens[i].clone());
            i += 1;
            continue;
        };
        let mut literal = String::new();
        let mut j = i + trigger_len;
        while j < tokens.len() {
            let word = core(&tokens[j]);
            if RUN_TERMINATORS.contains(&word.as_str()) {
                j += 1;
                break;
            }
            if stops_run(&tokens, j, spelled_char) {
                break;
            }
            let Some(ch) = spelled_char(&tokens[j]) else {
                break;
            };
            literal.push(ch);
            let trail = trailing(&tokens[j]);
            j += 1;
            if !trail.is_empty() {
                literal.push_str(trail);
                break;
            }
        }
        if literal.chars().filter(|c| c.is_alphanumeric()).count() < 2 {
            out.push(tokens[i].clone());
            i += 1;
            continue;
        }
        out.push(reshape_guid(&literal));
        i = j;
    }
    out
}

/// True when the token at `at` should not be swallowed by a spelled run: it is
/// an everyday word that merely happens to also be a letter name, and what
/// follows it is prose rather than more letters.
fn stops_run(tokens: &[String], at: usize, mapper: impl Fn(&str) -> Option<char>) -> bool {
    if !is_ambiguous_short_word(&core(&tokens[at])) {
        return false;
    }
    match tokens.get(at + 1) {
        None => false,
        Some(next) => mapper(next).is_none() && !RUN_TERMINATORS.contains(&core(next).as_str()),
    }
}

// ---------------------------------------------------------------- hex runs

const HEX_TRIGGERS: &[&[&str]] = &[
    &["коммит"],
    &["коммита"],
    &["коммите"],
    &["commit"],
    &["хэш"],
    &["хеш"],
    &["хеша"],
    &["хэша"],
    &["hash"],
    &["sha"],
    &["гуид"],
    &["guid"],
    &["uuid"],
    &["ууид"],
    &["айди"],
    &["id"],
    &["идентификатор"],
    &["hex"],
];

fn join_hex_runs(tokens: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(tokens.len());
    let mut i = 0;
    while i < tokens.len() {
        let triggered = match_trigger(&tokens, i, HEX_TRIGGERS).is_some();
        let start = if triggered { i + 1 } else { i };
        let (run, trail, end) = collect_hex_run(&tokens, start);
        let accepted = if triggered {
            accept_hex(&run)
        } else {
            is_guid(&run)
        };
        if accepted && end > start {
            if triggered {
                out.push(tokens[i].clone());
            }
            out.push(format!("{}{trail}", reshape_guid(&run)));
            i = end;
            continue;
        }
        out.push(tokens[i].clone());
        i += 1;
    }
    out
}

fn collect_hex_run(tokens: &[String], start: usize) -> (String, String, usize) {
    let mut run = String::new();
    let mut trail = String::new();
    let mut j = start;
    while j < tokens.len() {
        let word = core(&tokens[j]);
        if RUN_TERMINATORS.contains(&word.as_str()) {
            j += 1;
            break;
        }
        if stops_run(tokens, j, hex_or_separator) {
            break;
        }
        let piece = match hex_fragment(&tokens[j]) {
            Some(piece) => piece,
            None if spoken_separator(&word) == Some('-') || tokens[j].trim() == "-" => {
                "-".to_string()
            }
            None => break,
        };
        run.push_str(&piece);
        let token_trail = trailing(&tokens[j]);
        j += 1;
        if !token_trail.is_empty() {
            trail = token_trail.to_string();
            break;
        }
    }
    (run, trail, j)
}

fn hex_or_separator(token: &str) -> Option<char> {
    hex_fragment(token)
        .and_then(|piece| piece.chars().next())
        .or_else(|| spoken_separator(&core(token)).filter(|c| *c == '-'))
}

/// A hash needs both letters and digits. Digits alone are a number, and letters
/// alone are an English word that happens to be spelled out of `a`–`f`
/// ("added", "face beef") — a real 7+ character hash without a single digit is
/// a one-in-a-thousand event, so the rule costs nothing and buys precision.
fn accept_hex(run: &str) -> bool {
    if is_guid(run) {
        return true;
    }
    let body: String = run.chars().filter(|c| *c != '-').collect();
    body.len() >= 4
        && body.chars().all(|c| c.is_ascii_hexdigit())
        && body.chars().any(|c| c.is_ascii_alphabetic())
        && body.chars().any(|c| c.is_ascii_digit())
}

fn is_guid(text: &str) -> bool {
    let groups: Vec<usize> = text.split('-').map(str::len).collect();
    groups == [8, 4, 4, 4, 12] && text.chars().all(|c| c == '-' || c.is_ascii_hexdigit())
}

/// 32 hexadecimal characters in any grouping become canonical 8-4-4-4-12.
fn reshape_guid(run: &str) -> String {
    let body: String = run.chars().filter(|c| *c != '-').collect();
    if body.len() != 32 || !body.chars().all(|c| c.is_ascii_hexdigit()) {
        return run.to_string();
    }
    let lower = body.to_lowercase();
    format!(
        "{}-{}-{}-{}-{}",
        &lower[0..8],
        &lower[8..12],
        &lower[12..16],
        &lower[16..20],
        &lower[20..32]
    )
}

// ---------------------------------------------------------------- versions

const VERSION_KEYWORDS: &[&str] = &[
    "версия",
    "версии",
    "версию",
    "верси",
    "version",
    "релиз",
    "release",
    "v",
    "тег",
    "tag",
];

fn dot_word(word: &str) -> bool {
    matches!(word, "точка" | "точку" | "dot" | "point")
}

fn number_word(word: &str) -> Option<String> {
    if !word.is_empty() && word.chars().all(|c| c.is_ascii_digit()) {
        return Some(word.to_string());
    }
    spoken_digit(word).map(|c| c.to_string()).or_else(|| {
        match word {
            "десять" | "ten" => Some("10"),
            "одиннадцать" | "eleven" => Some("11"),
            "двенадцать" | "twelve" => Some("12"),
            "тринадцать" => Some("13"),
            "четырнадцать" => Some("14"),
            "пятнадцать" => Some("15"),
            "шестнадцать" => Some("16"),
            "семнадцать" => Some("17"),
            "восемнадцать" => Some("18"),
            "девятнадцать" => Some("19"),
            "двадцать" | "twenty" => Some("20"),
            _ => None,
        }
        .map(str::to_string)
    })
}

fn join_versions(tokens: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(tokens.len());
    let mut i = 0;
    while i < tokens.len() {
        let keyed = out
            .last()
            .map(|prev| VERSION_KEYWORDS.contains(&core(prev).as_str()))
            .unwrap_or(false);
        let mut parts = Vec::new();
        let mut j = i;
        let mut trail = String::new();
        while let Some(part) = tokens.get(j).map(|t| core(t)).and_then(|w| number_word(&w)) {
            parts.push(part);
            trail = trailing(&tokens[j]).to_string();
            let next_is_dot = tokens
                .get(j + 1)
                .map(|t| core(t))
                .is_some_and(|w| dot_word(&w));
            let after_dot_is_number = tokens
                .get(j + 2)
                .map(|t| core(t))
                .and_then(|w| number_word(&w))
                .is_some();
            if trail.is_empty() && next_is_dot && after_dot_is_number {
                j += 2;
            } else {
                j += 1;
                break;
            }
        }
        // Two-part numbers are ordinary speech ("два точка ноль" in prose) unless
        // a version keyword introduced them; three parts are unambiguous.
        if parts.len() >= 3 || (parts.len() == 2 && keyed) {
            out.push(format!("{}{trail}", parts.join(".")));
            i = j;
            continue;
        }
        out.push(tokens[i].clone());
        i += 1;
    }
    out
}

// ---------------------------------------------------------------- dotted names

/// Top-level domains as they are dictated. `.net` is deliberately absent:
/// "нет" is the Russian word for "no" and gluing it here would corrupt ordinary
/// speech. ".NET" is handled as a dictionary term ("дот нет") instead.
fn tld(word: &str) -> Option<&'static str> {
    Some(match word {
        "ком" | "com" => "com",
        "ру" | "ru" => "ru",
        "рф" => "рф",
        "орг" | "org" => "org",
        "ио" | "io" => "io",
        "дев" | "dev" => "dev",
        "ко" | "co" => "co",
        "инфо" | "info" => "info",
        "биз" | "biz" => "biz",
        "юкей" | "uk" => "uk",
        "гов" | "gov" => "gov",
        "эдю" | "edu" => "edu",
        "апп" | "app" => "app",
        "клауд" | "cloud" => "cloud",
        "тек" | "tech" => "tech",
        "онлайн" | "online" => "online",
        "сайт" | "site" => "site",
        "стор" | "store" => "store",
        "аи" | "ai" => "ai",
        "ме" | "me" => "me",
        _ => return None,
    })
}

const FILE_EXTENSIONS: &[&str] = &[
    "sh",
    "py",
    "js",
    "ts",
    "tsx",
    "jsx",
    "rs",
    "go",
    "java",
    "kt",
    "json",
    "yaml",
    "yml",
    "toml",
    "md",
    "txt",
    "csv",
    "xml",
    "html",
    "css",
    "scss",
    "sql",
    "log",
    "lock",
    "env",
    "exe",
    "dll",
    "cs",
    "cpp",
    "hpp",
    "rb",
    "php",
    "zip",
    "tar",
    "gz",
    "pdf",
    "png",
    "jpg",
    "jpeg",
    "svg",
    "ini",
    "conf",
    "cfg",
    "bat",
    "ps1",
    "properties",
    "gradle",
    "podspec",
    "plist",
];

fn file_extension(word: &str) -> Option<&'static str> {
    FILE_EXTENSIONS.iter().copied().find(|ext| *ext == word)
}

fn transliterate(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.to_lowercase().chars() {
        match ch {
            'а' => out.push('a'),
            'б' => out.push('b'),
            'в' => out.push('v'),
            'г' => out.push('g'),
            'д' => out.push('d'),
            'е' | 'ё' | 'э' => out.push('e'),
            'ж' => out.push_str("zh"),
            'з' => out.push('z'),
            'и' | 'й' => out.push('i'),
            'к' => out.push('k'),
            'л' => out.push('l'),
            'м' => out.push('m'),
            'н' => out.push('n'),
            'о' => out.push('o'),
            'п' => out.push('p'),
            'р' => out.push('r'),
            'с' => out.push('s'),
            'т' => out.push('t'),
            'у' => out.push('u'),
            'ф' => out.push('f'),
            'х' => out.push('h'),
            'ц' => out.push('c'),
            'ч' => out.push_str("ch"),
            'ш' => out.push_str("sh"),
            'щ' => out.push_str("sch"),
            'ы' => out.push('y'),
            'ю' => out.push_str("yu"),
            'я' => out.push_str("ya"),
            'ъ' | 'ь' => {}
            other => out.push(other),
        }
    }
    out
}

fn has_cyrillic(text: &str) -> bool {
    text.chars().any(|c| ('\u{0400}'..='\u{04FF}').contains(&c))
}

/// Glue `<label> точка <tld|extension>` into `label.tld`, transliterating a
/// Cyrillic label so a dictated Russian brand name becomes a real host name.
fn join_dotted_names(tokens: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(tokens.len());
    let mut i = 0;
    while i < tokens.len() {
        let is_dot = core(&tokens[i]);
        let suffix = tokens.get(i + 1).map(|t| core(t)).and_then(|w| {
            tld(&w)
                .map(|t| (t, true))
                .or_else(|| file_extension(&w).map(|e| (e, false)))
        });
        match (
            dot_word(&is_dot) || is_dot == "дот",
            suffix,
            out.last().is_some(),
        ) {
            (true, Some((suffix, is_domain)), true) => {
                let label = take_label(&mut out, is_domain);
                if label.is_empty() {
                    out.push(tokens[i].clone());
                    i += 1;
                    continue;
                }
                let trail = trailing(&tokens[i + 1]);
                out.push(format!("{label}.{suffix}{trail}"));
                i += 2;
            }
            _ => {
                out.push(tokens[i].clone());
                i += 1;
            }
        }
    }
    out
}

/// Pop the words that form the name in front of the dot. A bare number is part
/// of the name ("эльма 365" is one label), a word before it is too.
fn take_label(out: &mut Vec<String>, is_domain: bool) -> String {
    let Some(last) = out.pop() else {
        return String::new();
    };
    let (lead, last_core, _) = split_affixes(&last);
    if last_core.is_empty() {
        out.push(last.clone());
        return String::new();
    }
    let mut label = last_core.to_string();
    if last_core.chars().all(|c| c.is_ascii_digit()) {
        let prefix = out.last().map(|t| split_affixes(t).1.to_string());
        if let Some(prefix) = prefix.filter(|p| !p.is_empty() && p.chars().all(char::is_alphabetic))
        {
            out.pop();
            label = format!("{prefix}{label}");
        }
    }
    if !lead.is_empty() {
        out.push(lead.to_string());
    }
    if is_domain && has_cyrillic(&label) {
        transliterate(&label)
    } else if is_domain {
        label.to_lowercase()
    } else {
        label
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rebuilds_a_guid_from_dictated_characters() {
        let heard = "гуид четыре три шесть а два девять шесть девять дефис це а семь и \
                     дефис четыре семь а б дефис б ноль эф три дефис семь два а пять \
                     три четыре д семь четыре четыре б шесть";
        assert_eq!(apply(heard), "гуид 436a2969-ca7e-47ab-b0f3-72a534d744b6");
    }

    #[test]
    fn regroups_a_guid_that_arrived_as_one_hex_blob() {
        assert_eq!(
            apply("гуид 436a2969ca7e47abb0f372a534d744b6"),
            "гуид 436a2969-ca7e-47ab-b0f3-72a534d744b6"
        );
    }

    #[test]
    fn a_guid_shape_needs_no_trigger_word() {
        assert_eq!(
            apply("открой 436a2969 - ca7e - 47ab - b0f3 - 72a534d744b6 в базе"),
            "открой 436a2969-ca7e-47ab-b0f3-72a534d744b6 в базе"
        );
    }

    #[test]
    fn rebuilds_a_short_commit_hash() {
        assert_eq!(
            apply("коммит пять три це три девять шесть три"),
            "коммит 53c3963"
        );
        assert_eq!(apply("коммит d 6 b 0 2 0 4"), "коммит d6b0204");
        assert_eq!(
            apply("хеш a 9 5 c 5 2 8 c c a 5 4 5 4 f 9 4 3 0 4 9 e 2 7 d 1 2 2 4 7 7 6 6 e 0 b c e 2 c"),
            "хеш a95c528cca5454f943049e27d12247766e0bce2c"
        );
    }

    #[test]
    fn a_number_after_commit_stays_a_number() {
        assert_eq!(apply("коммит два дня назад"), "коммит два дня назад");
        assert_eq!(apply("коммит 2024 года"), "коммит 2024 года");
        assert_eq!(apply("один родитель"), "один родитель");
    }

    #[test]
    fn spelled_runs_become_literals() {
        assert_eq!(apply("файл по буквам эй би си"), "файл abc");
        assert_eq!(apply("логин по буквам эс ю дабл-ю точка эй"), "логин suw.a");
        assert_eq!(apply("по буквам эй би си конец дальше"), "abc дальше");
    }

    #[test]
    fn a_spelled_run_stops_before_ordinary_prose() {
        assert_eq!(
            apply("по буквам эй би си и потом дальше"),
            "abc и потом дальше"
        );
    }

    #[test]
    fn a_run_too_short_to_be_a_literal_is_left_alone() {
        assert_eq!(apply("по буквам привет"), "по буквам привет");
    }

    #[test]
    fn rebuilds_domains_and_transliterates_the_label() {
        assert_eq!(apply("открой эльма 365 точка ком"), "открой elma365.com");
        // The dictionary has already canonicalised "гитхаб" to "GitHub" upstream.
        assert_eq!(apply("зайди на GitHub точка ком"), "зайди на github.com");
        assert_eq!(apply("сайт example точка org"), "сайт example.org");
    }

    #[test]
    fn rebuilds_file_names_without_transliterating() {
        assert_eq!(apply("запусти скрипт точка sh"), "запусти скрипт.sh");
        assert_eq!(apply("открой config точка json"), "открой config.json");
    }

    #[test]
    fn rebuilds_version_numbers() {
        assert_eq!(apply("версия два точка ноль точка один"), "версия 2.0.1");
        assert_eq!(apply("версия два точка ноль"), "версия 2.0");
        assert_eq!(apply("обнови до 1 точка 2 точка 3"), "обнови до 1.2.3");
    }

    #[test]
    fn two_part_numbers_without_a_keyword_stay_prose() {
        assert_eq!(apply("пять точка ноль"), "пять точка ноль");
    }

    #[test]
    fn ordinary_speech_is_untouched() {
        for text in [
            "точка зрения важна",
            "нет, я не согласен",
            "давай встретимся в пять",
            "поставь точку в конце",
            "",
        ] {
            assert_eq!(
                apply(text),
                text.split_whitespace().collect::<Vec<_>>().join(" ")
            );
        }
    }

    #[test]
    fn technical_words_are_recognised_for_the_formatter() {
        assert!(looks_technical("436a2969-ca7e-47ab-b0f3-72a534d744b6"));
        assert!(looks_technical("elma365.com"));
        assert!(looks_technical("скрипт.sh"));
        assert!(looks_technical("a95c528cca5454f943049e27d12247766e0bce2c"));
        assert!(looks_technical(".NET"));
        assert!(!looks_technical("привет"));
        assert!(!looks_technical("готово."));
        assert!(!looks_technical("дела"));
        assert!(!looks_technical("9:05"));
        assert!(!looks_technical("5"));
    }

    #[test]
    fn transliteration_matches_common_host_names() {
        assert_eq!(transliterate("эльма"), "elma");
        assert_eq!(transliterate("гитхаб"), "githab");
        assert_eq!(transliterate("яндекс"), "yandeks");
    }

    #[test]
    fn multiline_input_keeps_its_lines() {
        assert_eq!(
            apply("версия 1 точка 2 точка 3\nкоммит d6b0204"),
            "версия 1.2.3\nкоммит d6b0204"
        );
    }
}
