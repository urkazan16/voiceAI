use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum DictionaryKind {
    #[default]
    Vocabulary,
    Replacement,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DictionaryEntry {
    pub id: String,
    #[serde(default)]
    pub kind: DictionaryKind,
    #[serde(default)]
    pub canonical: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub replacement: String,
    #[serde(default)]
    pub case_sensitive: bool,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub builtin: bool,
}

fn default_true() -> bool {
    true
}

impl DictionaryEntry {
    pub fn rule(id: &str, spoken: &str, canonical: &str) -> Self {
        Self {
            id: id.into(),
            kind: DictionaryKind::Replacement,
            canonical: canonical.into(),
            aliases: vec![spoken.into()],
            source: spoken.into(),
            replacement: canonical.into(),
            case_sensitive: false,
            enabled: true,
            builtin: false,
        }
    }

    pub fn vocabulary(id: &str, canonical: &str, aliases: &[&str]) -> Self {
        Self {
            id: id.into(),
            kind: DictionaryKind::Vocabulary,
            canonical: canonical.into(),
            aliases: aliases.iter().map(|s| (*s).to_string()).collect(),
            source: aliases.first().copied().unwrap_or(canonical).into(),
            replacement: canonical.into(),
            case_sensitive: false,
            enabled: true,
            builtin: true,
        }
    }

    pub fn target(&self) -> &str {
        if !self.canonical.is_empty() {
            &self.canonical
        } else {
            &self.replacement
        }
    }

    pub fn patterns(&self) -> Vec<String> {
        let mut out = self.aliases.clone();
        if !self.source.is_empty() && !out.iter().any(|a| a == &self.source) {
            out.push(self.source.clone());
        }
        out.retain(|p| !p.is_empty());
        out
    }
}

/// Compiled alternation plus the alias lookup it resolves matches against.
/// Both are derived from `entries` and are rebuilt only when entries change —
/// a 5000-term dictionary would otherwise rebuild a 10000-key map on every
/// single utterance.
#[derive(Debug, Default)]
struct MatchEngine {
    regex: Option<Regex>,
    targets: HashMap<String, String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Dictionary {
    pub entries: Vec<DictionaryEntry>,
    #[serde(skip)]
    cache: Mutex<Option<Arc<MatchEngine>>>,
}

impl Default for Dictionary {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            cache: Mutex::new(None),
        }
    }
}

impl Clone for Dictionary {
    fn clone(&self) -> Self {
        Self {
            entries: self.entries.clone(),
            cache: Mutex::new(None),
        }
    }
}

impl PartialEq for Dictionary {
    fn eq(&self, other: &Self) -> bool {
        self.entries == other.entries
    }
}

impl Eq for Dictionary {}

impl Dictionary {
    pub fn from_entries(entries: Vec<DictionaryEntry>) -> Self {
        Self {
            entries,
            cache: Mutex::new(None),
        }
    }

    pub fn apply(&self, input: &str) -> String {
        let engine = self.engine();
        if let Some(re) = &engine.regex {
            return re
                .replace_all(input, |caps: &regex::Captures| {
                    let matched = caps.get(0).map(|m| m.as_str()).unwrap_or("");
                    canonical_for_match(matched, &engine.targets)
                })
                .into_owned();
        }
        self.apply_linear(input)
    }

    /// Terms to bias the recognizer towards, most specific first. Whisper only
    /// reads a couple of hundred tokens of prompt, so this is deliberately a
    /// short list rather than the whole dictionary.
    pub fn recognition_hints(&self, max_chars: usize) -> String {
        let mut hints: Vec<&str> = Vec::new();
        for entry in self.entries.iter().filter(|e| e.enabled) {
            let target = entry.target();
            if !target.is_empty() && !hints.contains(&target) {
                hints.push(target);
            }
        }
        let mut out = String::new();
        for hint in hints {
            if out.len() + hint.len() + 2 > max_chars {
                break;
            }
            if !out.is_empty() {
                out.push_str(", ");
            }
            out.push_str(hint);
        }
        out
    }

    fn apply_linear(&self, input: &str) -> String {
        let mut output = input.to_string();
        for (pattern, target, case_sensitive) in self.pattern_list() {
            if let Ok(re) = Regex::new(&bounded_alt(&pattern, case_sensitive)) {
                output = re.replace_all(&output, target.as_str()).into_owned();
            } else if case_sensitive {
                output = output.replace(&pattern, &target);
            } else {
                output = replace_case_insensitive(&output, &pattern, &target);
            }
        }
        output
    }

    fn pattern_list(&self) -> Vec<(String, String, bool)> {
        let mut patterns: Vec<(String, String, bool)> = Vec::new();
        for entry in &self.entries {
            if !entry.enabled {
                continue;
            }
            let target = entry.target().to_string();
            if target.is_empty() {
                continue;
            }
            for pattern in entry.patterns() {
                if !pattern.is_empty() {
                    patterns.push((pattern, target.clone(), entry.case_sensitive));
                }
            }
        }
        patterns.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then(a.0.cmp(&b.0)));
        patterns
    }

    fn engine(&self) -> Arc<MatchEngine> {
        let mut slot = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(cached) = slot.as_ref() {
            return cached.clone();
        }
        let patterns = self.pattern_list();
        let mut targets = HashMap::with_capacity(patterns.len() * 2);
        for (pattern, target, _) in &patterns {
            targets.insert(pattern.clone(), target.clone());
            targets.insert(pattern.to_lowercase(), target.clone());
        }
        let built = Arc::new(MatchEngine {
            regex: compile_pattern_regex(&patterns),
            targets,
        });
        *slot = Some(built.clone());
        built
    }

    fn invalidate(&self) {
        if let Ok(mut slot) = self.cache.lock() {
            *slot = None;
        }
    }

    pub fn upsert(&mut self, mut entry: DictionaryEntry) {
        if entry.canonical.is_empty() {
            entry.canonical = entry.replacement.clone();
        }
        if entry.replacement.is_empty() {
            entry.replacement = entry.canonical.clone();
        }
        if entry.source.is_empty() {
            entry.source = entry.aliases.first().cloned().unwrap_or_default();
        }
        if entry.aliases.is_empty() && !entry.source.is_empty() {
            entry.aliases.push(entry.source.clone());
        }
        if let Some(existing) = self.entries.iter_mut().find(|e| e.id == entry.id) {
            *existing = entry;
        } else {
            self.entries.push(entry);
        }
        self.invalidate();
    }

    pub fn remove(&mut self, id: &str) {
        self.entries.retain(|e| e.id != id);
        self.invalidate();
    }

    pub fn search(&self, query: &str) -> Vec<DictionaryEntry> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return self.entries.clone();
        }
        self.entries
            .iter()
            .filter(|e| {
                e.target().to_lowercase().contains(&q)
                    || e.patterns().iter().any(|p| p.to_lowercase().contains(&q))
            })
            .cloned()
            .collect()
    }

    pub fn import_entries(&mut self, entries: Vec<DictionaryEntry>) {
        for entry in entries {
            if entry.target().is_empty() {
                continue;
            }
            if self
                .entries
                .iter()
                .any(|e| e.id == entry.id || e.target() == entry.target() && e.kind == entry.kind)
            {
                if let Some(existing) = self.entries.iter_mut().find(|e| e.id == entry.id) {
                    *existing = entry;
                }
                continue;
            }
            self.upsert(entry);
        }
        self.invalidate();
    }

    pub fn ensure_builtins(&mut self) {
        for builtin in builtin_developer_terms() {
            if self
                .entries
                .iter()
                .any(|e| e.target().eq_ignore_ascii_case(builtin.target()))
            {
                continue;
            }
            self.entries.push(builtin);
        }
        self.invalidate();
    }
}

pub fn builtin_developer_terms() -> Vec<DictionaryEntry> {
    vec![
        DictionaryEntry::vocabulary(
            "builtin-restassured",
            "RestAssured",
            &["рест ашуред", "рест ашюред", "rest assured", "ресташуред"],
        ),
        DictionaryEntry::vocabulary(
            "builtin-junit5",
            "JUnit 5",
            &["жюнит 5", "junit 5", "жюнит"],
        ),
        DictionaryEntry::vocabulary("builtin-junit", "JUnit", &["junit"]),
        DictionaryEntry::vocabulary(
            "builtin-postgres",
            "PostgreSQL",
            &["пострес", "postgres", "постгрес"],
        ),
        DictionaryEntry::vocabulary("builtin-selenide", "Selenide", &["селенид", "selenide"]),
        DictionaryEntry::vocabulary(
            "builtin-localflow",
            "LocalFlow",
            &["локалфлоу", "local flow"],
        ),
        DictionaryEntry::vocabulary("builtin-select", "SELECT", &["селект", "select"]),
        DictionaryEntry::vocabulary("builtin-intellij", "IntelliJ IDEA", &["интелидж", "idea"]),
        DictionaryEntry::vocabulary("builtin-kubernetes", "Kubernetes", &["кубернетис", "k8s"]),
        DictionaryEntry::vocabulary("builtin-github", "GitHub", &["гитхаб", "github"]),
        // ".NET" cannot be rebuilt from "точка нет" downstream: "нет" is the
        // Russian word for "no", and gluing it to a dot would corrupt ordinary
        // speech. Recognising the spoken name here is unambiguous instead.
        DictionaryEntry::vocabulary(
            "builtin-dotnet-framework",
            ".NET Framework",
            &[
                "дот нет фреймворк",
                "дотнет фреймворк",
                "точка нет фреймворк",
                "dot net framework",
            ],
        ),
        DictionaryEntry::vocabulary(
            "builtin-dotnet",
            ".NET",
            &["дот нет", "дотнет", "dot net", "донет"],
        ),
        DictionaryEntry::vocabulary("builtin-nuget", "NuGet", &["нюгет", "нуget", "nuget"]),
        DictionaryEntry::vocabulary("builtin-csharp", "C#", &["си шарп", "c sharp", "сишарп"]),
        DictionaryEntry::vocabulary("builtin-docker", "Docker", &["докер", "docker"]),
        DictionaryEntry::vocabulary("builtin-nginx", "nginx", &["энджинкс", "нжинкс"]),
        DictionaryEntry::vocabulary("builtin-elma365", "ELMA365", &["эльма 365", "элма 365"]),
    ]
}

/// Pack Latin literals into one alternation so the regex crate can use
/// Aho-Corasick. Wrapping each of 5000 terms in its own `(?i:...)` group makes
/// `apply` tens of milliseconds in debug on CI.
fn compile_pattern_regex(patterns: &[(String, String, bool)]) -> Option<Regex> {
    if patterns.is_empty() {
        return None;
    }
    let mut ci_latin = Vec::new();
    let mut cs_latin = Vec::new();
    let mut inflected = Vec::new();
    for (pattern, _, case_sensitive) in patterns {
        if pattern.is_empty() {
            continue;
        }
        if !*case_sensitive && pattern.chars().any(is_cyrillic) {
            inflected.push(bounded_alt(pattern, false));
        } else if *case_sensitive {
            cs_latin.push(regex::escape(pattern));
        } else {
            ci_latin.push(regex::escape(pattern));
        }
    }
    let mut alts = Vec::new();
    if !ci_latin.is_empty() {
        alts.push(format!(r"(?i:\b(?:{})\b)", ci_latin.join("|")));
    }
    if !cs_latin.is_empty() {
        alts.push(format!(r"\b(?:{})\b", cs_latin.join("|")));
    }
    alts.extend(inflected);
    if alts.is_empty() {
        return None;
    }
    RegexBuilder::new(&format!("(?:{})", alts.join("|")))
        .size_limit(64 * 1024 * 1024)
        .dfa_size_limit(64 * 1024 * 1024)
        .build()
        .ok()
}

fn bounded_alt(pattern: &str, case_sensitive: bool) -> String {
    let escaped = regex::escape(pattern);
    let body = if case_sensitive {
        escaped
    } else {
        format!("(?i:{escaped})")
    };
    let inflect = if !case_sensitive && pattern.chars().any(is_cyrillic) {
        r"(?:ами|ями|ах|ях|ов|ев|ам|ям|ом|ем|ой|ей|[аяуюоыиеё])?"
    } else {
        ""
    };
    format!(r"\b{body}{inflect}\b")
}

fn is_cyrillic(ch: char) -> bool {
    ('\u{0400}'..='\u{04FF}').contains(&ch)
}

fn canonical_for_match(matched: &str, map: &HashMap<String, String>) -> String {
    if let Some(target) = map
        .get(matched)
        .or_else(|| map.get(&matched.to_lowercase()))
    {
        return target.clone();
    }
    let lower = matched.to_lowercase();
    const SUFFIXES: &[&str] = &[
        "ами", "ями", "ах", "ях", "ов", "ев", "ам", "ям", "ом", "ем", "ой", "ей", "а", "я", "у",
        "ю", "о", "е", "и", "ы", "ё",
    ];
    for suffix in SUFFIXES {
        if let Some(stem) = lower.strip_suffix(suffix) {
            if stem.chars().count() >= 3 {
                if let Some(target) = map.get(stem) {
                    return target.clone();
                }
            }
        }
    }
    matched.to_string()
}

fn replace_case_insensitive(haystack: &str, needle: &str, replacement: &str) -> String {
    let mut result = String::with_capacity(haystack.len());
    let mut idx = 0;
    while let Some((start, end)) = crate::textscan::find_ci(haystack, needle, idx) {
        result.push_str(&haystack[idx..start]);
        result.push_str(replacement);
        idx = end;
    }
    result.push_str(&haystack[idx..]);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_technical_terms() {
        let dict =
            Dictionary::from_entries(vec![DictionaryEntry::rule("1", "пострес", "Postgres")]);
        assert_eq!(
            dict.apply("Подними пострес локально"),
            "Подними Postgres локально"
        );
    }

    #[test]
    fn prefers_longer_matches() {
        let dict = Dictionary::from_entries(vec![
            DictionaryEntry::rule("1", "junit", "JUnit"),
            DictionaryEntry::rule("2", "junit 5", "JUnit 5"),
        ]);
        assert_eq!(dict.apply("use junit 5"), "use JUnit 5");
    }

    #[test]
    fn qa_and_sql_terms() {
        let dict = Dictionary::from_entries(vec![
            DictionaryEntry::rule("1", "ресташуред", "RestAssured"),
            DictionaryEntry::rule("2", "селект", "SELECT"),
        ]);
        let out = dict.apply("напиши селект в ресташуред");
        assert!(out.contains("SELECT"));
        assert!(out.contains("RestAssured"));
    }

    #[test]
    fn aliases_map_to_canonical() {
        let mut dict = Dictionary::default();
        dict.upsert(DictionaryEntry::vocabulary(
            "ra",
            "RestAssured",
            &["рест ашуред", "rest assured"],
        ));
        assert_eq!(
            dict.apply("создай тест на рест ашуред"),
            "создай тест на RestAssured"
        );
    }

    #[test]
    fn inflected_cyrillic_alias_maps_to_canonical() {
        let dict =
            Dictionary::from_entries(vec![DictionaryEntry::rule("1", "пострес", "PostgreSQL")]);
        assert_eq!(dict.apply("Подними пострес"), "Подними PostgreSQL");
        assert_eq!(dict.apply("в постресе"), "в PostgreSQL");
        assert_eq!(dict.apply("без постреса"), "без PostgreSQL");
        assert_eq!(dict.apply("суперпострес рядом"), "суперпострес рядом");
    }

    #[test]
    fn latin_identifier_does_not_match_inside_words() {
        let mut dict = Dictionary::default();
        dict.ensure_builtins();
        assert!(dict.apply("select from users").contains("SELECT"));
        assert_eq!(dict.apply("ideal candidate"), "ideal candidate");
    }

    #[test]
    fn search_matches_alias() {
        let mut dict = Dictionary::default();
        dict.ensure_builtins();
        assert!(dict
            .search("рест")
            .iter()
            .any(|e| e.target() == "RestAssured"));
    }

    #[test]
    fn five_thousand_entries_use_one_regex() {
        let mut dict = Dictionary::default();
        for i in 0..5_000 {
            dict.upsert(DictionaryEntry::rule(
                &format!("id-{i}"),
                &format!("term{i}zzzz"),
                &format!("T{i}"),
            ));
        }
        let _ = dict.apply("warmup");
        assert!(
            dict.engine().regex.is_some(),
            "5000 entries should compile to one regex"
        );
        let sample = "prefix term42zzzz suffix term4999zzzz";
        let mut best = std::time::Duration::from_secs(60);
        let mut out = String::new();
        for _ in 0..8 {
            let started = std::time::Instant::now();
            out = dict.apply(sample);
            best = best.min(started.elapsed());
        }
        assert!(out.contains("T42"), "{out}");
        assert!(out.contains("T4999"), "{out}");
        assert!(best.as_millis() < 80, "dictionary apply took {best:?}");
    }
}
