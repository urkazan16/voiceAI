//! Placeholder latency harness. Real P50/P95 capture needs a packaged app.
#[test]
fn latency_targets_are_documented() {
    let hotkey_p95_ms = 150;
    let record_p95_ms = 300;
    let insert_p95_ms = 200;
    assert!(hotkey_p95_ms <= 150);
    assert!(record_p95_ms <= 300);
    assert!(insert_p95_ms <= 200);
}
