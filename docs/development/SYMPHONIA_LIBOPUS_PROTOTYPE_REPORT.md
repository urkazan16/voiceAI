# Symphonia/libopus prototype — implementation report

Updated: 2026-10-09
Scope: stages 1–5 of `SYMPHONIA_LIBOPUS_BACKEND_SPEC.md`
Decision: **accept as an opt-in engineering prototype; do not distribute or enable by default yet**

## Executive result

The mandatory Ogg/Opus v1 route is implemented behind the non-default
`audio-symphonia-opus` Cargo feature and a separate runtime opt-in. It decodes
in process with Symphonia 0.6.1 and bundled libopus 1.6.1, does not invoke an
external converter, produces finite mono 16 kHz `f32`, and preserves the
existing default route when the feature is off.

The crates.io adapter 0.3.0 did not compile on the project's Rust 1.88
toolchain and had timing/gain gaps. After explicit approval to continue, the
prototype pinned a local Apache-2.0-selected patch. The patch replaces the
Rust-1.89-only syntax, carries pre-skip across packets, restores it on reset,
and applies the Opus header gain exactly once.

This is not a production/distribution approval. Runtime installer smoke tests
on every target, the project's MPL review for Symphonia, and complete packaged
license/source-offer verification remain release gates.

## Stage ledger

| Stage                               | Result                                      | Evidence                                                                                                                                                           |
| ----------------------------------- | ------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 1. Build spike and licensing review | PASS with release blockers                  | Rust 1.88 incompatibility reproduced; local patch approved and pinned; dependency/native tree and SBOM provenance recorded                                         |
| 2. Isolated decoder                 | PASS on the host                            | Profile validator, strict decoder, trim/gain/downmix, stateful FIR resampler, reference PCM and signal tests pass                                                  |
| 3. Resources and integration        | PASS for Whisper; partial for alternate STT | Streamed input, budgets/deadline, independent cancellation, RAII staging cleanup, UI/CLI route and errors; T-One/Sherpa cannot abort inside their synchronous call |
| 4. Robustness and distribution      | PARTIAL                                     | Expanded fixtures, packaged license texts and local dependency inspection pass; remote target/installed-app smoke and legal review remain pending                  |
| 5. Candidate decision               | ACCEPT PROTOTYPE ONLY                       | Keep feature and runtime route off by default; do not advertise or ship until remaining gates pass                                                                 |

## Implemented contract

- Accepts one Ogg logical stream containing Opus version 1, mapping family 0,
  mono or stereo.
- Rejects chained/multiplexed streams, bad page sequence or CRC, inconsistent
  continuation, decreasing/invalid granules, malformed headers/lacing,
  oversized packets, unsupported mapping/channels, and partial input.
- Validates the complete Ogg stream before decoding, so a late corruption
  never returns partial PCM to STT.
- Uses a streamed `File` source for paths; byte callers use a cursor.
- Decodes Opus at 48 kHz, applies trim/gain once, downmixes stereo as
  `(L + R) / 2`, then uses a stateful 127-tap Blackman-window FIR 48→16 kHz
  resampler with compensated flush.
- Enforces 80 MiB input, configured output-sample budget, deadline and an
  independent atomic cancellation token. Cancellation is checked during page
  validation, packet decode, and at least every 1024 resampler outputs.
- Emits stable `AUDIO_*` error categories and avoids user path/audio/tag/text
  logging on the new command route.
- Cleans staged uploads with an owning RAII guard on success, error,
  cancellation and Rust unwind. Abrupt process termination remains subject to
  normal OS/application temp cleanup.

Whisper file recognition now carries the independent file token into
whisper.cpp's abort callback. A queued task polls the token every 50 ms, and a
unit test verifies return within one second. Dictation continues to use its own
global token. The T-One, GigaAM and Parakeet wrappers do not expose a comparable
mid-inference abort callback; file cancellation is checked before and after
those synchronous calls, so the sub-second end-to-end gate remains open for
those alternate engines.

## Audio and robustness evidence

The synthetic CC0 fixtures and their SHA-256 hashes, generator commands,
profiles and expected lengths are recorded in
`tests/fixtures/ogg_opus/manifest.json`. Reference 48 kHz float PCM was produced
with FFmpeg 8.1.2, explicitly selecting the libopus 1.6.1 decoder and requesting
float output, and stored separately from candidate output. This avoids both
FFmpeg's native Opus decoder and an intermediate `s16` quantization step.

Host tests cover:

- mono, stereo, silence, public-domain NASA speech, exact 16 kHz output lengths
  and finite samples;
- exact 48 kHz frame count and maximum absolute reference difference ≤ `1e-5`;
- pre-skip spanning packets, positive/negative output gain applied once,
  decoder reset, valid nonzero initial granule and informational input rates;
- VBR 60 ms packets, exact reference PCM and natural final-page end trim;
- content probing and a Unicode/space-containing path;
- wrong CRC, truncation, decreasing granule, inconsistent continuation,
  oversized comments, chained streams, mapping-family rejection, random bytes
  and mutation smoke without panic;
- input/output budget, deadline, cancellation and independent jobs;
- file-task token isolation, queued-Whisper cancellation latency and staging
  cleanup on normal return and Rust unwind;
- FIR packet-state continuity and ≥40 dB suppression of a 10 kHz tone.

The committed file corpus now includes VBR 60 ms packets, positive and negative
gain with independent references, informational 44.1 kHz input-rate metadata,
and a valid nonzero initial granule. The generator also tests other
informational rates in memory. Malformed granule, continuation and oversized
comment cases are deterministically generated by tests instead of stored as
redundant corrupt binaries. The real-speech case is a three-second excerpt from
NASA's 1969 Apollo 11 recording, with source URL, source hash and public-domain
status recorded in the manifest.

## Performance snapshot

Host: MacBook Pro (MacBookPro15,1), Intel Core i7 2.6 GHz, 6 cores, 16 GB,
macOS 15.7.9, x86_64. Build: Cargo `release`, LTO as configured by the project,
Rust 1.88, `audio-symphonia-opus` enabled.

| Input               | Encoded size |      PCM output | Decoder-reported time |          Wall time |     Peak RSS |
| ------------------- | -----------: | --------------: | --------------------: | -----------------: | -----------: |
| 1 s mono fixture    |     10,131 B |  16,000 samples |                 11 ms | 0.51 s cold launch | not captured |
| 60 s synthetic mono |    662,320 B | 960,000 samples |                538 ms |             0.55 s |  7,811,072 B |

The benchmark executable was 810,344 bytes. This is not an installer-size
delta or a comparison with the legacy route; those measurements remain for the
installed-build matrix. The 60-second file was generated only for local
performance measurement and is not committed as a fixture.

## Build, test and supply-chain evidence

Completed locally:

```sh
cargo check --manifest-path src-tauri/Cargo.toml --locked --all-targets
cargo check --manifest-path src-tauri/Cargo.toml --locked \
  --all-targets --features audio-symphonia-opus
cargo test --manifest-path src-tauri/Cargo.toml --locked --lib
cargo test --manifest-path src-tauri/Cargo.toml --locked \
  --features audio-symphonia-opus symphonia_opus
cargo clippy --manifest-path src-tauri/Cargo.toml --locked \
  --all-targets --features audio-symphonia-opus -- -D warnings
npm run build:ui
npm run check:js
npm run license:check
npm run sbom:opus -- /tmp/localflow-opus-sbom
```

The full default Rust library suite completed with 298 passed, 3 ignored and 0
failed tests. With the feature enabled, 325 passed, 3 were ignored and 0
failed; the standalone patched adapter adds 2 passing tests. The frontend suite
passes 28 tests. The SBOM
includes the local adapter, `opusic-sys`, and bundled libopus 1.6.1 with the
locked source checksum. CI now builds/tests the feature on the declared macOS,
Windows and Linux targets, runs a real decode, and rejects a dynamic libopus
dependency. Local `otool -L` inspection found no libopus dylib. The MPL-2.0,
selected Apache-2.0 and libopus license texts are packaged. A committed workflow
definition is not evidence that remote jobs or installed binaries have passed,
and the Symphonia MPL decision remains a manual project review.

## Enable and disable

The backend requires both compile-time and runtime opt-in.

```sh
cargo run --manifest-path src-tauri/Cargo.toml \
  --features audio-symphonia-opus -- \
  --experimental-opus path/to/audio.opus
```

For the desktop route, build with `audio-symphonia-opus` and start the process
with `LOCALFLOW_EXPERIMENTAL_OPUS=1`. Omitting either the Cargo feature or the
environment variable disables the desktop route. The CLI flag is explicit and
does not require the environment variable.

## Unsupported inputs

WebM/Matroska/MP4 Opus, raw Opus, Vorbis-in-Ogg, mapping families other than 0,
more than two channels, chained or multiplexed Ogg, live streams, packet-loss
concealment and partial recovery remain unsupported. This work does not add or
promise native AAC/M4A, ALAC, MP3, FLAC, AIFF or Vorbis support.

## Remaining production gates

1. Run and retain green CI/runtime evidence on Windows x64, macOS x64/arm64 and
   Linux x64, then smoke-test installed artifacts without FFmpeg or system
   libopus and inspect their dynamic dependencies.
2. Complete the project's manual MPL-2.0 review for Symphonia and verify the
   corresponding-source access path in the final packaged artifact. Required
   license texts and notices are now present in the repository.
3. Either add cooperative cancellation inside T-One/GigaAM/Parakeet or formally
   narrow the sub-second cancellation contract to decoding and Whisper
   recognition.
4. Measure installer-size delta and compare release performance against the
   existing baseline on fixed hardware for every target.

Until these are complete, the correct outcome is an available engineering
prototype whose default behavior and production packaging remain unchanged.
