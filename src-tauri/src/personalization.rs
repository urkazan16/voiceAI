use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CorrectionEvent {
    pub id: String,
    pub original: String,
    pub corrected: String,
    pub accepted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LearnedCandidate {
    pub id: String,
    pub pattern: String,
    pub replacement: String,
    pub weight: i32,
    #[serde(default)]
    pub accepted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InferredPreference {
    pub id: String,
    pub key: String,
    pub value: String,
    pub accepted: bool,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PersonalizationState {
    pub corrections: Vec<CorrectionEvent>,
    pub learned: Vec<LearnedCandidate>,
    pub preferences: Vec<InferredPreference>,
}

impl PersonalizationState {
    pub fn apply(&self, input: &str) -> String {
        let mut output = input.to_string();
        let mut learned: Vec<&LearnedCandidate> =
            self.learned.iter().filter(|c| c.accepted).collect();
        learned.sort_by(|a, b| {
            b.weight
                .cmp(&a.weight)
                .then(b.pattern.len().cmp(&a.pattern.len()))
        });
        for item in learned {
            if item.pattern.is_empty() {
                continue;
            }
            output = output.replace(&item.pattern, &item.replacement);
        }
        for pref in self.preferences.iter().filter(|p| p.accepted) {
            if pref.key == "signature_replace" {
                output = output.replace(&pref.value, "");
            }
        }
        output
    }

    pub fn record_correction(&mut self, event: CorrectionEvent, learn: bool) {
        if learn && event.original != event.corrected {
            let id = format!("learned-{}", event.original.to_lowercase());
            if let Some(existing) = self
                .learned
                .iter_mut()
                .find(|c| c.pattern.eq_ignore_ascii_case(&event.original))
            {
                existing.replacement = event.corrected.clone();
                existing.weight += 1;
            } else {
                self.learned.push(LearnedCandidate {
                    id,
                    pattern: event.original.clone(),
                    replacement: event.corrected.clone(),
                    weight: 1,
                    accepted: false,
                });
            }
        }
        self.corrections.push(event);
    }

    pub fn suggestions(&self) -> Vec<LearnedCandidate> {
        self.learned
            .iter()
            .filter(|c| !c.accepted && c.weight >= 2)
            .cloned()
            .collect()
    }

    pub fn accept_suggestion(&mut self, id: &str) -> Option<LearnedCandidate> {
        let item = self.learned.iter_mut().find(|c| c.id == id)?;
        item.accepted = true;
        Some(item.clone())
    }

    pub fn dismiss_suggestion(&mut self, id: &str) {
        self.learned.retain(|c| c.id != id);
    }

    pub fn accept_preference(&mut self, id: &str) {
        if let Some(pref) = self.preferences.iter_mut().find(|p| p.id == id) {
            pref.accepted = true;
        }
    }

    pub fn reset(&mut self) {
        self.corrections.clear();
        self.learned.clear();
        self.preferences.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_correction_is_only_a_candidate() {
        let mut state = PersonalizationState::default();
        state.record_correction(
            CorrectionEvent {
                id: "c1".into(),
                original: "локалфлоу".into(),
                corrected: "LocalFlow".into(),
                accepted: true,
            },
            true,
        );
        assert_eq!(state.apply("запусти локалфлоу"), "запусти локалфлоу");
        assert_eq!(state.learned.len(), 1);
        assert!(state.suggestions().is_empty());
    }

    #[test]
    fn repeated_correction_becomes_suggestion_then_accept() {
        let mut state = PersonalizationState::default();
        let event = CorrectionEvent {
            id: "c".into(),
            original: "локалфлоу".into(),
            corrected: "LocalFlow".into(),
            accepted: true,
        };
        state.record_correction(event.clone(), true);
        state.record_correction(event, true);
        assert_eq!(state.suggestions().len(), 1);
        let id = state.suggestions()[0].id.clone();
        state.accept_suggestion(&id);
        assert_eq!(state.apply("запусти локалфлоу"), "запусти LocalFlow");
    }

    #[test]
    fn reset_clears_corrections_learned_and_accepted_preferences() {
        let mut state = PersonalizationState {
            corrections: vec![CorrectionEvent {
                id: "c".into(),
                original: "a".into(),
                corrected: "b".into(),
                accepted: true,
            }],
            learned: vec![LearnedCandidate {
                id: "l".into(),
                pattern: "a".into(),
                replacement: "b".into(),
                weight: 2,
                accepted: true,
            }],
            preferences: vec![InferredPreference {
                id: "p".into(),
                key: "tone".into(),
                value: "formal".into(),
                accepted: true,
            }],
        };
        state.reset();
        assert!(state.corrections.is_empty());
        assert!(state.learned.is_empty());
        assert!(state.preferences.is_empty());
    }
}
