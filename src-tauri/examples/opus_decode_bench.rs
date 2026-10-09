#[cfg(feature = "audio-symphonia-opus")]
fn main() {
    let path = std::env::args_os()
        .nth(1)
        .map(std::path::PathBuf::from)
        .expect("usage: opus_decode_bench FILE");
    let input_bytes = std::fs::metadata(&path).expect("input metadata").len();
    let started = std::time::Instant::now();
    let pcm = localflow_lib::symphonia_opus::decode_path(&path, &Default::default())
        .expect("decode failed");
    println!(
        "backend={} input_bytes={} output_samples={} duration_seconds={:.3} decode_ms={}",
        localflow_lib::symphonia_opus::BACKEND_NAME,
        input_bytes,
        pcm.len(),
        pcm.len() as f64 / 16_000.0,
        started.elapsed().as_millis()
    );
}

#[cfg(not(feature = "audio-symphonia-opus"))]
fn main() {
    eprintln!("build with --features audio-symphonia-opus");
    std::process::exit(2);
}
