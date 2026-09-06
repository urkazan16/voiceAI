//! Tray title has three states: idle, recording, processing.
//! CF-C08: the menu-bar mark must change for hold vs process vs rest.

use localflow_lib::dictation::{
    tray_appearance, tray_kind_for_phase, tray_mark_for_phase, tray_tooltip_for_phase, TrayKind,
    TRAY_MARK_IDLE, TRAY_MARK_PROCESSING, TRAY_MARK_RECORDING,
};

struct Case {
    phase: &'static str,
    kind: TrayKind,
    mark: &'static str,
    tooltip: &'static str,
}

fn cases() -> [Case; 12] {
    [
        Case {
            phase: "pressed",
            kind: TrayKind::Recording,
            mark: TRAY_MARK_RECORDING,
            tooltip: "LocalFlow — recording",
        },
        Case {
            phase: "recording",
            kind: TrayKind::Recording,
            mark: TRAY_MARK_RECORDING,
            tooltip: "LocalFlow — recording",
        },
        Case {
            phase: "released",
            kind: TrayKind::Processing,
            mark: TRAY_MARK_PROCESSING,
            tooltip: "LocalFlow — processing",
        },
        Case {
            phase: "processing",
            kind: TrayKind::Processing,
            mark: TRAY_MARK_PROCESSING,
            tooltip: "LocalFlow — processing",
        },
        Case {
            phase: "done",
            kind: TrayKind::Idle,
            mark: TRAY_MARK_IDLE,
            tooltip: "LocalFlow",
        },
        Case {
            phase: "idle",
            kind: TrayKind::Idle,
            mark: TRAY_MARK_IDLE,
            tooltip: "LocalFlow",
        },
        Case {
            phase: "cancelled",
            kind: TrayKind::Idle,
            mark: TRAY_MARK_IDLE,
            tooltip: "LocalFlow",
        },
        Case {
            phase: "error",
            kind: TrayKind::Idle,
            mark: TRAY_MARK_IDLE,
            tooltip: "LocalFlow",
        },
        Case {
            phase: "hotkey-up",
            kind: TrayKind::Idle,
            mark: TRAY_MARK_IDLE,
            tooltip: "LocalFlow",
        },
        Case {
            phase: "failed",
            kind: TrayKind::Idle,
            mark: TRAY_MARK_IDLE,
            tooltip: "LocalFlow",
        },
        Case {
            phase: "",
            kind: TrayKind::Idle,
            mark: TRAY_MARK_IDLE,
            tooltip: "LocalFlow",
        },
        Case {
            phase: "unknown",
            kind: TrayKind::Idle,
            mark: TRAY_MARK_IDLE,
            tooltip: "LocalFlow",
        },
    ]
}

#[test]
fn every_emitted_phase_maps_to_exactly_one_of_three_kinds() {
    for case in cases() {
        assert_eq!(
            tray_kind_for_phase(case.phase),
            case.kind,
            "kind {}",
            case.phase
        );
        assert_eq!(
            tray_mark_for_phase(case.phase),
            case.mark,
            "mark {}",
            case.phase
        );
        assert_eq!(
            tray_tooltip_for_phase(case.phase),
            case.tooltip,
            "tip {}",
            case.phase
        );
        let (mark, tip) = tray_appearance(case.phase);
        assert_eq!(mark, case.mark);
        assert_eq!(tip, case.tooltip);
    }
}

#[test]
fn recording_and_processing_marks_are_visible_and_different() {
    assert!(!TRAY_MARK_RECORDING.is_empty());
    assert!(!TRAY_MARK_PROCESSING.is_empty());
    assert_ne!(TRAY_MARK_RECORDING, TRAY_MARK_PROCESSING);
    assert_eq!(TRAY_MARK_RECORDING.chars().count(), 1);
    assert_eq!(TRAY_MARK_PROCESSING.chars().count(), 1);
}

#[test]
fn idle_does_not_reuse_a_busy_mark() {
    assert_ne!(TRAY_MARK_IDLE, TRAY_MARK_RECORDING);
    assert_ne!(TRAY_MARK_IDLE, TRAY_MARK_PROCESSING);
    assert!(TRAY_MARK_IDLE.is_empty());
}

#[test]
fn tooltips_are_stable_per_kind() {
    assert_eq!(
        tray_tooltip_for_phase("recording"),
        tray_tooltip_for_phase("pressed")
    );
    assert_eq!(
        tray_tooltip_for_phase("processing"),
        tray_tooltip_for_phase("released")
    );
    assert_ne!(
        tray_tooltip_for_phase("recording"),
        tray_tooltip_for_phase("processing")
    );
    assert_ne!(
        tray_tooltip_for_phase("processing"),
        tray_tooltip_for_phase("idle")
    );
}

#[test]
fn hold_to_talk_never_shows_idle_mark_while_the_mic_is_open() {
    for phase in ["pressed", "recording"] {
        assert_eq!(tray_kind_for_phase(phase), TrayKind::Recording);
        assert_eq!(tray_mark_for_phase(phase), "●");
    }
}

#[test]
fn release_starts_processing_mark_before_done() {
    assert_eq!(tray_mark_for_phase("released"), "◐");
    assert_eq!(tray_mark_for_phase("processing"), "◐");
    assert_eq!(tray_mark_for_phase("done"), "");
}

#[test]
fn three_kinds_never_share_a_mark() {
    let idle = tray_appearance("idle");
    let rec = tray_appearance("recording");
    let proc = tray_appearance("processing");
    assert_ne!(idle.0, rec.0);
    assert_ne!(idle.0, proc.0);
    assert_ne!(rec.0, proc.0);
    assert_ne!(idle.1, rec.1);
    assert_ne!(idle.1, proc.1);
    assert_ne!(rec.1, proc.1);
}

#[test]
fn cancel_and_error_return_to_idle_mark() {
    for phase in ["cancelled", "error", "done"] {
        assert_eq!(tray_kind_for_phase(phase), TrayKind::Idle);
        assert_eq!(tray_mark_for_phase(phase), "");
        assert_eq!(tray_tooltip_for_phase(phase), "LocalFlow");
    }
}

#[test]
fn pressed_is_recording_not_processing() {
    assert_eq!(tray_kind_for_phase("pressed"), TrayKind::Recording);
    assert_ne!(tray_kind_for_phase("pressed"), TrayKind::Processing);
    assert_eq!(tray_mark_for_phase("pressed"), TRAY_MARK_RECORDING);
}

#[test]
fn processing_tooltip_names_the_busy_state() {
    let tip = tray_tooltip_for_phase("processing");
    assert!(tip.contains("processing"));
    assert!(tray_tooltip_for_phase("recording").contains("recording"));
    assert_eq!(tray_tooltip_for_phase("idle"), "LocalFlow");
}

#[test]
fn appearance_tuple_matches_helpers() {
    for phase in ["pressed", "recording", "released", "processing", "done", "idle"] {
        let (mark, tip) = tray_appearance(phase);
        assert_eq!(mark, tray_mark_for_phase(phase), "{phase}");
        assert_eq!(tip, tray_tooltip_for_phase(phase), "{phase}");
    }
}

#[test]
fn recording_tooltip_is_not_the_idle_label() {
    assert_eq!(tray_tooltip_for_phase("recording"), "LocalFlow — recording");
    assert_ne!(tray_tooltip_for_phase("recording"), "LocalFlow");
    assert_ne!(tray_tooltip_for_phase("processing"), "LocalFlow");
    assert!(tray_tooltip_for_phase("processing").starts_with("LocalFlow"));
}
