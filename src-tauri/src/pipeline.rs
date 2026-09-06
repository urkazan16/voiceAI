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
    Backtrack,
    Formatting,
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
            | (Dictionary, Backtrack)
            | (Backtrack, Formatting)
            | (Formatting, Personalization)
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
    pub backtrack_text: String,
    pub formatted_text: String,
    pub personalized_text: String,
    pub final_text: String,
    pub mode: PipelineMode,
    #[serde(default)]
    pub insert_ok: bool,
    #[serde(default)]
    pub cues: Vec<TranscriptCue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TranscriptCue {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

pub fn cues_to_srt(cues: &[TranscriptCue]) -> String {
    let mut out = String::new();
    for (idx, cue) in cues.iter().enumerate() {
        out.push_str(&format!(
            "{}\n{} --> {}\n{}\n\n",
            idx + 1,
            ms_stamp(cue.start_ms),
            ms_stamp(cue.end_ms),
            cue.text.trim()
        ));
    }
    out
}

pub const PARAGRAPH_PAUSE_MS: u64 = 2000;

pub fn join_cues_with_pauses(cues: &[TranscriptCue], pause_ms: u64) -> String {
    let mut out = String::new();
    let mut prev_end: Option<u64> = None;
    for cue in cues {
        let text = cue.text.trim();
        if text.is_empty() {
            continue;
        }
        if let Some(end) = prev_end {
            let gap = cue.start_ms.saturating_sub(end);
            if gap >= pause_ms {
                while out.ends_with(' ') {
                    out.pop();
                }
                out.push_str("\n\n");
            } else if !out.is_empty() && !out.ends_with([' ', '\n']) {
                out.push(' ');
            }
        }
        out.push_str(text);
        prev_end = Some(cue.end_ms.max(cue.start_ms));
    }
    out
}

fn ms_stamp(ms: u64) -> String {
    let s = ms / 1000;
    let rem = ms % 1000;
    let m = s / 60;
    let h = m / 60;
    format!("{:02}:{:02}:{:02},{:03}", h, m % 60, s % 60, rem)
}

pub fn format_without_remote_llm(mode: PipelineMode, text: &str) -> String {
    crate::format::format_smart(mode, text)
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
        snap.transition(PipelineState::Backtrack).unwrap();
        snap.transition(PipelineState::Formatting).unwrap();
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

    #[test]
    fn srt_export_has_timecodes() {
        let srt = cues_to_srt(&[TranscriptCue {
            start_ms: 0,
            end_ms: 1500,
            text: "Привет".into(),
        }]);
        assert!(srt.contains("00:00:00,000 --> 00:00:01,500"));
        assert!(srt.contains("Привет"));
    }

    #[test]
    fn pause_over_two_seconds_becomes_paragraph() {
        let text = join_cues_with_pauses(
            &[
                TranscriptCue {
                    start_ms: 0,
                    end_ms: 800,
                    text: "Первый абзац".into(),
                },
                TranscriptCue {
                    start_ms: 3100,
                    end_ms: 4000,
                    text: "Второй абзац".into(),
                },
            ],
            PARAGRAPH_PAUSE_MS,
        );
        assert_eq!(text, "Первый абзац\n\nВторой абзац");
    }

    #[test]
    fn fail_is_always_allowed() {
        let mut snap = PipelineSnapshot::default();
        snap.fail("mic");
        assert_eq!(snap.state, PipelineState::Failed);
        assert_eq!(snap.last_error.as_deref(), Some("mic"));
        snap.reset();
        assert_eq!(snap.state, PipelineState::Idle);
    }

    #[test]
    fn cannot_skip_from_completed_to_recording() {
        let mut snap = PipelineSnapshot::default();
        snap.transition(PipelineState::Recording).unwrap();
        snap.transition(PipelineState::ProcessingStt).unwrap();
        snap.transition(PipelineState::Dictionary).unwrap();
        snap.transition(PipelineState::Backtrack).unwrap();
        snap.transition(PipelineState::Formatting).unwrap();
        snap.transition(PipelineState::Personalization).unwrap();
        snap.transition(PipelineState::Llm).unwrap();
        snap.transition(PipelineState::Validate).unwrap();
        snap.transition(PipelineState::Injecting).unwrap();
        snap.transition(PipelineState::Completed).unwrap();
        let err = snap.transition(PipelineState::Recording).unwrap_err();
        assert_eq!(err.code(), "PIPELINE_INVALID_STATE");
    }
}
