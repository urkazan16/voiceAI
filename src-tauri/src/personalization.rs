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
        let mut learned = self.learned.clone();
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

    pub fn record_correction(&mut self, event: CorrectionEvent) {
        if event.accepted && event.original != event.corrected {
            let id = format!("learned-{}", event.id);
            if let Some(existing) = self
                .learned
                .iter_mut()
                .find(|c| c.pattern == event.original)
            {
                existing.replacement = event.corrected.clone();
                existing.weight += 1;
            } else {
                self.learned.push(LearnedCandidate {
                    id,
                    pattern: event.original.clone(),
                    replacement: event.corrected.clone(),
                    weight: 1,
                });
            }
        }
        self.corrections.push(event);
    }

    pub fn accept_preference(&mut self, id: &str) {
        if let Some(pref) = self.preferences.iter_mut().find(|p| p.id == id) {
            pref.accepted = true;
        }
    }

    pub fn reset(&mut self) {
        self.corrections.clear();
        self.learned.clear();
        self.preferences.retain(|p| !p.accepted);
        self.preferences.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepted_correction_becomes_learned_candidate() {
        let mut state = PersonalizationState::default();
        state.record_correction(CorrectionEvent {
            id: "c1".into(),
            original: "локалфлоу".into(),
            corrected: "LocalFlow".into(),
            accepted: true,
        });
        assert_eq!(state.apply("запусти локалфлоу"), "запусти LocalFlow");
        assert_eq!(state.learned.len(), 1);
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
