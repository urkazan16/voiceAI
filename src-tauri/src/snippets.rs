use chrono::Utc;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Snippet {
    pub id: String,
    pub trigger: String,
    pub content: String,
    #[serde(default)]
    pub language: String,
    #[serde(default)]
    pub profile: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
}

fn default_true() -> bool {
    true
}

impl Snippet {
    pub fn new(id: &str, trigger: &str, content: &str) -> Self {
        let now = Utc::now().to_rfc3339();
        Self {
            id: id.into(),
            trigger: trigger.into(),
            content: content.into(),
            language: String::new(),
            profile: String::new(),
            enabled: true,
            created_at: now.clone(),
            updated_at: now,
        }
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SnippetBook {
    pub items: Vec<Snippet>,
}

impl SnippetBook {
    pub fn upsert(&mut self, mut snippet: Snippet) {
        snippet.trigger = snippet.trigger.trim().chars().take(60).collect();
        snippet.content = snippet.content.chars().take(4000).collect();
        snippet.updated_at = Utc::now().to_rfc3339();
        if snippet.created_at.is_empty() {
            snippet.created_at = snippet.updated_at.clone();
        }
        if let Some(existing) = self.items.iter_mut().find(|s| s.id == snippet.id) {
            *existing = snippet;
        } else {
            self.items.push(snippet);
        }
    }

    pub fn remove(&mut self, id: &str) {
        self.items.retain(|s| s.id != id);
    }

    /// Exact trigger replacement. `skip_llm` is true when a snippet fired.
    pub fn expand(&self, text: &str, profile_id: &str) -> Option<(String, bool)> {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return None;
        }
        let mut ranked: Vec<&Snippet> = self
            .items
            .iter()
            .filter(|s| s.enabled && !s.trigger.trim().is_empty())
            .filter(|s| s.profile.is_empty() || s.profile == profile_id)
            .collect();
        ranked.sort_by(|a, b| b.trigger.len().cmp(&a.trigger.len()));
        let lower = trimmed.to_lowercase();
        for snippet in ranked {
            let trigger = snippet.trigger.trim();
            let tlow = trigger.to_lowercase();
            if lower == tlow {
                return Some((snippet.content.clone(), true));
            }
            if let Some(idx) = find_phrase(&lower, &tlow) {
                let mut out = String::new();
                out.push_str(&trimmed[..idx]);
                out.push_str(&snippet.content);
                out.push_str(&trimmed[idx + trigger.len()..]);
                return Some((out, true));
            }
        }
        None
    }

    pub fn ensure_defaults(&mut self) {
        if self
            .items
            .iter()
            .any(|s| s.trigger == "баг репорт" || s.trigger == "мой баг репорт")
        {
            return;
        }
        self.upsert(Snippet::new(
            "builtin-bug-report",
            "мой баг репорт",
            "[BUG]\nEnvironment:\nSteps:\nExpected:\nActual:",
        ));
        self.upsert(Snippet::new(
            "builtin-bug-report-short",
            "баг репорт",
            "[BUG]\nEnvironment:\nSteps:\nExpected:\nActual:",
        ));
    }
}

fn find_phrase(haystack: &str, needle: &str) -> Option<usize> {
    let mut start = 0;
    while let Some(found) = haystack[start..].find(needle) {
        let abs = start + found;
        let before_ok = abs == 0
            || haystack[..abs]
                .chars()
                .last()
                .is_some_and(|c| !c.is_alphanumeric());
        let end = abs + needle.len();
        let after_ok = end >= haystack.len()
            || haystack[end..]
                .chars()
                .next()
                .is_some_and(|c| !c.is_alphanumeric());
        if before_ok && after_ok {
            return Some(abs);
        }
        start = abs + needle.len();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snippet_benchmark_section_126() {
        let mut book = SnippetBook::default();
        book.ensure_defaults();
        let (out, skip) = book.expand("мой баг репорт", "").unwrap();
        assert!(skip);
        assert_eq!(out, "[BUG]\nEnvironment:\nSteps:\nExpected:\nActual:");
    }

    #[test]
    fn does_not_fire_on_partial_word() {
        let mut book = SnippetBook::default();
        book.upsert(Snippet::new("s", "mail", "x@y.z"));
        assert!(book.expand("email tomorrow", "").is_none());
    }

    #[test]
    fn remove_snippet_stops_expansion() {
        let mut book = SnippetBook::default();
        book.upsert(Snippet::new("s", "sig", "Best regards"));
        assert!(book.expand("sig", "").is_some());
        book.remove("s");
        assert!(book.expand("sig", "").is_none());
    }

    #[test]
    fn in_sentence_trigger_still_skips_llm() {
        let mut book = SnippetBook::default();
        book.upsert(Snippet::new("s", "мой адрес", "ул. Ленина, 1"));
        let (out, skip) = book.expand("добавь мой адрес в письмо", "").unwrap();
        assert!(skip);
        assert!(out.contains("ул. Ленина, 1"));
        assert!(out.contains("добавь"));
        assert!(out.contains("в письмо"));
    }

    #[test]
    fn profile_scoped_snippet_does_not_fire_on_other_profile() {
        let mut book = SnippetBook::default();
        let mut work = Snippet::new("w", "sig", "Work signature");
        work.profile = "work".into();
        book.upsert(work);
        assert!(book.expand("sig", "personal").is_none());
        let (out, skip) = book.expand("sig", "work").unwrap();
        assert!(skip);
        assert_eq!(out, "Work signature");
    }

    #[test]
    fn trigger_and_content_are_capped() {
        let mut book = SnippetBook::default();
        let long_trigger: String = "a".repeat(80);
        let long_content: String = "b".repeat(5000);
        book.upsert(Snippet::new("cap", &long_trigger, &long_content));
        let item = &book.items[0];
        assert!(item.trigger.chars().count() <= 60);
        assert!(item.content.chars().count() <= 4000);
    }
}
