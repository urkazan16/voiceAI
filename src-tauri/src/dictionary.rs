use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DictionaryEntry {
    pub id: String,
    pub source: String,
    pub replacement: String,
    pub case_sensitive: bool,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Dictionary {
    pub entries: Vec<DictionaryEntry>,
}

impl Dictionary {
    pub fn apply(&self, input: &str) -> String {
        let mut output = input.to_string();
        let mut ordered = self.entries.clone();
        ordered.sort_by(|a, b| b.source.len().cmp(&a.source.len()));
        for entry in ordered {
            if entry.source.is_empty() {
                continue;
            }
            if entry.case_sensitive {
                output = output.replace(&entry.source, &entry.replacement);
            } else {
                output = replace_case_insensitive(&output, &entry.source, &entry.replacement);
            }
        }
        output
    }

    pub fn upsert(&mut self, entry: DictionaryEntry) {
        if let Some(existing) = self.entries.iter_mut().find(|e| e.id == entry.id) {
            *existing = entry;
        } else {
            self.entries.push(entry);
        }
    }

    pub fn remove(&mut self, id: &str) {
        self.entries.retain(|e| e.id != id);
    }
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

    fn entry(id: &str, source: &str, replacement: &str, case_sensitive: bool) -> DictionaryEntry {
        DictionaryEntry {
            id: id.into(),
            source: source.into(),
            replacement: replacement.into(),
            case_sensitive,
        }
    }

    #[test]
    fn replaces_technical_terms() {
        let dict = Dictionary {
            entries: vec![entry("1", "пострес", "Postgres", false)],
        };
        assert_eq!(
            dict.apply("Подними пострес локально"),
            "Подними Postgres локально"
        );
    }

    #[test]
    fn prefers_longer_matches() {
        let dict = Dictionary {
            entries: vec![
                entry("1", "junit", "JUnit", false),
                entry("2", "junit 5", "JUnit 5", false),
            ],
        };
        assert_eq!(dict.apply("use junit 5"), "use JUnit 5");
    }

    #[test]
    fn qa_and_sql_terms() {
        let dict = Dictionary {
            entries: vec![
                entry("1", "ресташуред", "RestAssured", false),
                entry("2", "селект", "SELECT", false),
            ],
        };
        let out = dict.apply("напиши селект в ресташуред");
        assert!(out.contains("SELECT"));
        assert!(out.contains("RestAssured"));
    }
}
