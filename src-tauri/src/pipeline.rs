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
    pub insert_error: Option<String>,
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

pub const SPEAKER_TURN_PAUSE_MS: u64 = 900;
const TURN_NEWLINE_PAUSE_MS: u64 = 550;

/// Pause-based speaker turns (not neural diarization). A long gap, or a short
/// reply after a medium gap, starts the next speaker. Two-party interviews
/// alternate `speaker_0` / `speaker_1`.
pub fn format_diarized_transcript(cues: &[TranscriptCue]) -> String {
    let cues = merge_long_form_cues(cues);
    let turns = group_speaker_turns(&cues);
    let mut out = String::new();
    for (speaker, start_ms, end_ms, text) in turns {
        if text.trim().is_empty() {
            continue;
        }
        let body = crate::sanitize::collapse_long_form_text(text.trim());
        if body.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push_str("\n\n");
        }
        out.push_str(&format!(
            "[speaker_{speaker} {} – {}]\n{}",
            format_clock(start_ms),
            format_clock(end_ms),
            body
        ));
    }
    out
}

pub fn format_clock(ms: u64) -> String {
    let s = (ms + 500) / 1000;
    let h = s / 3600;
    let m = (s % 3600) / 60;
    let sec = s % 60;
    if h > 0 {
        format!("{h}:{m:02}:{sec:02}")
    } else {
        format!("{m}:{sec:02}")
    }
}

pub fn merge_long_form_cues(cues: &[TranscriptCue]) -> Vec<TranscriptCue> {
    let mut merged: Vec<TranscriptCue> = Vec::new();
    for cue in cues {
        let text = crate::sanitize::collapse_long_form_text(cue.text.trim());
        if text.is_empty() || crate::sanitize::is_likely_hallucination(&text) {
            continue;
        }
        if let Some(prev) = merged.last_mut() {
            let trimmed = crate::sanitize::strip_overlapping_prefix(&prev.text, &text);
            let overlaps = cue.start_ms <= prev.end_ms.saturating_add(800);
            if trimmed.is_empty() {
                prev.end_ms = prev.end_ms.max(cue.end_ms.max(cue.start_ms));
                continue;
            }
            if overlaps || trimmed != text {
                if !prev.text.ends_with([' ', '\n']) {
                    prev.text.push(' ');
                }
                prev.text.push_str(&trimmed);
                prev.end_ms = prev.end_ms.max(cue.end_ms.max(cue.start_ms));
                continue;
            }
            if crate::sanitize::is_same_or_truncated_echo(&prev.text, &text)
                && text.chars().count() <= prev.text.chars().count()
            {
                prev.end_ms = prev.end_ms.max(cue.end_ms.max(cue.start_ms));
                continue;
            }
        }
        merged.push(TranscriptCue {
            start_ms: cue.start_ms,
            end_ms: cue.end_ms.max(cue.start_ms),
            text,
        });
    }
    drop_trailing_junk(&mut merged);
    merged
}

fn drop_trailing_junk(cues: &mut Vec<TranscriptCue>) {
    while cues.len() > 1 {
        let last = cues.last().expect("len > 1");
        let prev = &cues[cues.len() - 2];
        let gap = last.start_ms.saturating_sub(prev.end_ms);
        let words = last.text.split_whitespace().count();
        let junk = crate::sanitize::is_likely_hallucination(&last.text)
            || crate::sanitize::is_degenerate_repetition(&last.text)
            || (gap >= 6_000 && words <= 10);
        if junk {
            cues.pop();
            continue;
        }
        break;
    }
}

fn group_speaker_turns(cues: &[TranscriptCue]) -> Vec<(u32, u64, u64, String)> {
    let mut turns: Vec<(u32, u64, u64, String)> = Vec::new();
    let mut speaker: u32 = 0;
    for cue in cues {
        let text = cue.text.trim();
        if text.is_empty() {
            continue;
        }
        if let Some((sp, _start, end, body)) = turns.last_mut() {
            if cue.start_ms < *end {
                let trimmed = crate::sanitize::strip_overlapping_prefix(body, text);
                let new_is_shorter_echo = crate::sanitize::is_same_or_truncated_echo(body, text)
                    && text.chars().count() <= body.chars().count();
                if new_is_shorter_echo || trimmed.is_empty() {
                    *end = (*end).max(cue.end_ms);
                    continue;
                }
                if trimmed != text {
                    if !body.ends_with([' ', '\n']) {
                        body.push(' ');
                    }
                    body.push_str(&trimmed);
                    *end = (*end).max(cue.end_ms);
                    continue;
                }
                if !body.contains(text) {
                    if text.contains(body.trim()) {
                        *body = text.to_string();
                    } else if !body.ends_with([' ', '\n']) {
                        body.push(' ');
                        body.push_str(text);
                    } else {
                        body.push_str(text);
                    }
                }
                *end = (*end).max(cue.end_ms);
                continue;
            }
            let gap = cue.start_ms.saturating_sub(*end);
            if should_switch_speaker(gap, text) {
                speaker = if *sp == 0 { 1 } else { 0 };
                turns.push((
                    speaker,
                    cue.start_ms,
                    cue.end_ms.max(cue.start_ms),
                    text.to_string(),
                ));
            } else {
                let trimmed = crate::sanitize::strip_overlapping_prefix(body, text);
                if trimmed.is_empty()
                    || (crate::sanitize::is_same_or_truncated_echo(body, text)
                        && text.chars().count() <= body.chars().count())
                {
                    *end = cue.end_ms.max(*end);
                    continue;
                }
                if gap >= TURN_NEWLINE_PAUSE_MS {
                    body.push('\n');
                } else if !body.ends_with([' ', '\n']) {
                    body.push(' ');
                }
                body.push_str(&trimmed);
                *end = cue.end_ms.max(*end);
            }
        } else {
            turns.push((
                speaker,
                cue.start_ms,
                cue.end_ms.max(cue.start_ms),
                text.to_string(),
            ));
        }
    }
    turns
}

fn should_switch_speaker(gap_ms: u64, incoming: &str) -> bool {
    if gap_ms >= 2_500 {
        return true;
    }
    if gap_ms < SPEAKER_TURN_PAUSE_MS {
        return false;
    }
    incoming.split_whitespace().count() <= 16
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
    fn diarized_transcript_uses_pause_turns_and_clock() {
        let text = format_diarized_transcript(&[
            TranscriptCue {
                start_ms: 1_000,
                end_ms: 17_000,
                text: "Я Миша.".into(),
            },
            TranscriptCue {
                start_ms: 18_500,
                end_ms: 19_200,
                text: "Да, давайте.".into(),
            },
            TranscriptCue {
                start_ms: 19_400,
                end_ms: 20_000,
                text: "Хорошо.".into(),
            },
        ]);
        assert!(text.contains("[speaker_0 0:01 – 0:17]"), "{text}");
        assert!(text.contains("Я Миша."), "{text}");
        assert!(text.contains("[speaker_1 0:19 – 0:20]"), "{text}");
        assert!(text.contains("Да, давайте. Хорошо."), "{text}");
        assert!(!text.contains("speaker_0 0:19"), "{text}");
    }

    #[test]
    fn overlapping_windows_do_not_repeat_the_tail() {
        let text = format_diarized_transcript(&[
            TranscriptCue {
                start_ms: 0,
                end_ms: 30_000,
                text: "Я участвовал в тестировании интеграционном то есть проверял работу микросервиса".into(),
            },
            TranscriptCue {
                start_ms: 27_000,
                end_ms: 40_000,
                text: "Я участвовал в тестировании интеграционном то есть проверял работу микросервиса и смотрел логи".into(),
            },
        ]);
        assert_eq!(
            text.to_lowercase().matches("участвовал").count(),
            1,
            "{text}"
        );
        assert!(text.contains("логи"), "{text}");
        assert!(!text.to_lowercase().contains("подписывайтесь"), "{text}");
    }

    #[test]
    fn drops_looped_subscribe_spam() {
        let text = format_diarized_transcript(&[TranscriptCue {
            start_ms: 0,
            end_ms: 8_000,
            text: format!(
                "выбирал данные {} и отправлял",
                "подписывайтесь на канал ".repeat(12)
            ),
        }]);
        assert!(text.contains("отправлял"), "{text}");
        assert!(!text.to_lowercase().contains("подписывайтесь"), "{text}");
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
