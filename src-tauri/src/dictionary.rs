use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;

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

#[derive(Debug, Serialize, Deserialize)]
pub struct Dictionary {
    pub entries: Vec<DictionaryEntry>,
    #[serde(skip)]
    cache: Mutex<Option<Regex>>,
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
        let (re, map) = self.engine();
        if let Some(re) = re {
            return re
                .replace_all(input, |caps: &regex::Captures| {
                    let matched = caps.get(0).map(|m| m.as_str()).unwrap_or("");
                    map.get(matched)
                        .or_else(|| map.get(&matched.to_lowercase()))
                        .cloned()
                        .unwrap_or_else(|| matched.to_string())
                })
                .into_owned();
        }
        self.apply_linear(input)
    }

    fn apply_linear(&self, input: &str) -> String {
        let mut output = input.to_string();
        for (pattern, target, case_sensitive) in self.pattern_list() {
            if case_sensitive {
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

    fn engine(&self) -> (Option<Regex>, HashMap<String, String>) {
        let mut map = HashMap::new();
        let patterns = self.pattern_list();
        if patterns.is_empty() {
            return (None, map);
        }
        let mut alts = Vec::new();
        for (pattern, target, case_sensitive) in &patterns {
            map.insert(pattern.clone(), target.clone());
            map.insert(pattern.to_lowercase(), target.clone());
            let escaped = regex::escape(pattern);
            if *case_sensitive {
                alts.push(escaped);
            } else {
                alts.push(format!("(?i:{escaped})"));
            }
        }
        let compiled = {
            let mut slot = self.cache.lock().unwrap_or_else(|e| e.into_inner());
            if slot.is_none() {
                *slot = Regex::new(&format!("(?:{})", alts.join("|"))).ok();
            }
            slot.clone()
        };
        (compiled, map)
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
    ]
}

fn replace_case_insensitive(haystack: &str, needle: &str, replacement: &str) -> String {
    if needle.is_empty() {
        return haystack.to_string();
    }
    let lower = haystack.to_lowercase();
    let needle_l = needle.to_lowercase();
    let mut result = String::new();
    let mut idx = 0;
    let bytes = haystack.as_bytes();
    while let Some(found) = lower[idx..].find(&needle_l) {
        let abs = idx + found;
        result.push_str(&haystack[idx..abs]);
        result.push_str(replacement);
        idx = abs + needle.len();
        if idx > bytes.len() {
            break;
        }
    }
    result.push_str(&haystack[idx..]);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_technical_terms() {
        let dict = Dictionary::from_entries(vec![DictionaryEntry::rule("1", "пострес", "Postgres")]);
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
        let started = std::time::Instant::now();
        let out = dict.apply("prefix term42zzzz suffix term4999zzzz");
        assert!(out.contains("T42"), "{out}");
        assert!(out.contains("T4999"), "{out}");
        assert!(
            started.elapsed().as_millis() < 80,
            "dictionary apply took {:?}",
            started.elapsed()
        );
    }
}
