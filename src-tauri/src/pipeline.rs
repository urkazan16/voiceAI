use crate::error::{LfError, LfResult};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PipelineMode {
    Raw,
    Normal,
    Professional,
    Code,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PipelineState {
    Idle,
    Recording,
    ProcessingStt,
    Dictionary,
    Personalization,
    Llm,
    Validate,
    Injecting,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PipelineSnapshot {
    pub state: PipelineState,
    pub mode: PipelineMode,
    pub last_error: Option<String>,
}

impl Default for PipelineSnapshot {
    fn default() -> Self {
        Self {
            state: PipelineState::Idle,
            mode: PipelineMode::Normal,
            last_error: None,
        }
    }
}

impl PipelineSnapshot {
    pub fn transition(&mut self, to: PipelineState) -> LfResult<()> {
        if !is_allowed(self.state, to) {
            return Err(LfError::PipelineInvalidState {
                from: format!("{:?}", self.state),
                to: format!("{to:?}"),
            });
        }
        self.state = to;
        if to != PipelineState::Failed {
            self.last_error = None;
        }
        Ok(())
    }

    pub fn fail(&mut self, message: impl Into<String>) {
        self.state = PipelineState::Failed;
        self.last_error = Some(message.into());
    }

    pub fn reset(&mut self) {
        self.state = PipelineState::Idle;
        self.last_error = None;
    }
}

fn is_allowed(from: PipelineState, to: PipelineState) -> bool {
    use PipelineState::*;
    matches!(
        (from, to),
        (Idle, Recording)
            | (Recording, ProcessingStt)
            | (Recording, Idle)
            | (ProcessingStt, Dictionary)
            | (Dictionary, Personalization)
            | (Personalization, Llm)
            | (Llm, Validate)
            | (Validate, Injecting)
            | (Injecting, Completed)
            | (Completed, Idle)
            | (Failed, Idle)
            | (Idle, Idle)
    ) || to == Failed
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PipelineOutput {
    pub raw_transcript: String,
    pub dictionary_text: String,
    pub personalized_text: String,
    pub final_text: String,
    pub mode: PipelineMode,
}

pub fn format_without_remote_llm(mode: PipelineMode, text: &str) -> String {
    match mode {
        PipelineMode::Raw => text.to_string(),
        PipelineMode::Normal => simple_punctuate(text),
        PipelineMode::Professional => professional_shape(text),
        PipelineMode::Code => text.trim().to_string(),
    }
}

fn simple_punctuate(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let mut chars: Vec<char> = trimmed.chars().collect();
    if let Some(first) = chars.first_mut() {
        *first = first.to_uppercase().next().unwrap_or(*first);
    }
    let last = chars.last().copied().unwrap_or(' ');
    if !matches!(last, '.' | '!' | '?') {
        chars.push('.');
    }
    chars.into_iter().collect()
}

fn professional_shape(text: &str) -> String {
    simple_punctuate(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn happy_path_state_machine() {
        let mut snap = PipelineSnapshot::default();
        snap.transition(PipelineState::Recording).unwrap();
        snap.transition(PipelineState::ProcessingStt).unwrap();
        snap.transition(PipelineState::Dictionary).unwrap();
        snap.transition(PipelineState::Personalization).unwrap();
        snap.transition(PipelineState::Llm).unwrap();
        snap.transition(PipelineState::Validate).unwrap();
        snap.transition(PipelineState::Injecting).unwrap();
        snap.transition(PipelineState::Completed).unwrap();
        snap.transition(PipelineState::Idle).unwrap();
    }

    #[test]
    fn rejects_illegal_jumps() {
        let mut snap = PipelineSnapshot::default();
        let err = snap.transition(PipelineState::Injecting).unwrap_err();
        assert_eq!(err.code(), "PIPELINE_INVALID_STATE");
    }

    #[test]
    fn raw_mode_does_not_rewrite() {
        assert_eq!(
            format_without_remote_llm(PipelineMode::Raw, "api sql"),
            "api sql"
        );
    }
}
