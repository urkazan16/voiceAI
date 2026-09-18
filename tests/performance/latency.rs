//! Deterministic latency guard for the local post-processing stage.
//!
//! STT and system insertion depend on hardware and require the packaged-app
//! benchmark. This test intentionally measures only the CPU-only formatter and
//! must not be presented as end-to-end latency.

use localflow_lib::pipeline::{format_without_remote_llm, PipelineMode};
use std::time::Instant;

fn percentile(samples: &mut [u128], percentile: usize) -> u128 {
    samples.sort_unstable();
    let index = (samples.len() - 1) * percentile / 100;
    samples[index]
}

#[test]
fn deterministic_postprocess_p95_stays_below_20_ms() {
    let input = "ну короче подготовь отчет запятая добавь результаты тестирования и следующие шаги";
    let mut samples = Vec::with_capacity(200);
    let mut last = String::new();
    for _ in 0..200 {
        let started = Instant::now();
        last = format_without_remote_llm(PipelineMode::Professional, input);
        samples.push(started.elapsed().as_micros());
    }
    assert!(!last.is_empty());
    let p95_us = percentile(&mut samples, 95);
    assert!(
        p95_us < 20_000,
        "deterministic formatter p95 regressed to {p95_us} µs"
    );
}
