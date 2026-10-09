# Symphonia/libopus prototype — implementation report

Date: 2026-10-08  
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

| Stage                               | Result                                        | Evidence                                                                                                                                                             |
| ----------------------------------- | --------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1. Build spike and licensing review | PASS with release blockers                    | Rust 1.88 incompatibility reproduced; local patch approved and pinned; dependency/native tree and SBOM provenance recorded                                           |
| 2. Isolated decoder                 | PASS on the host                              | Profile validator, strict decoder, trim/gain/downmix, stateful FIR resampler, reference PCM and signal tests pass                                                    |
| 3. Resources and integration        | PASS for prototype, one documented limitation | Streamed file input, budgets/deadline, separate file-task cancellation, RAII staging cleanup, UI/CLI experimental route and error codes implemented                  |
| 4. Robustness and distribution      | PARTIAL                                       | Corruption/property smoke and regression suites pass locally; target CI was added but remote jobs and installed-app smoke have not run; legal review remains pending |
| 5. Candidate decision               | ACCEPT PROTOTYPE ONLY                         | Keep feature and runtime route off by default; do not advertise or ship until remaining gates pass                                                                   |

## Implemented contract

- Accepts one Ogg logical stream containing Opus version 1, mapping family 0,
  mono or stereo.
- Rejects chained/multiplexed streams, bad page sequence or CRC, malformed
  lacing, oversized packets, unsupported mapping/channels, and partial input.
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

Cancellation cannot interrupt a transcription engine call that has already
entered the existing synchronous STT implementation; it is checked before and
after that call. The sub-second cancellation requirement is proven for decode,
not for an already-running STT inference. Closing that gap requires a separate
cooperative STT cancellation change and is a production blocker if the gate is
interpreted end-to-end.

## Audio and robustness evidence

The synthetic CC0 fixtures and their SHA-256 hashes, generator commands,
profiles and expected lengths are recorded in
`tests/fixtures/ogg_opus/manifest.json`. Reference 48 kHz float PCM was produced
with FFmpeg 8.1.2/libopus and stored separately from candidate output.

Host tests cover:

- mono, stereo, silence, exact 16 kHz output lengths and finite samples;
- exact 48 kHz frame count and maximum absolute reference difference ≤ `1e-5`;
- pre-skip spanning packets, output gain applied once and decoder reset;
- content probing and a Unicode/space-containing path;
- wrong CRC, truncation, chained streams, mapping-family rejection, random
  bytes and mutation smoke without panic;
- input/output budget, deadline, cancellation and independent jobs;
- FIR packet-state continuity and ≥40 dB suppression of a 10 kHz tone.

The current corpus does not yet include a redistributable speech fixture,
explicit VBR/variable-packet-duration fixture, positive and negative gain files,
all informational input-rate variants, or a dedicated malformed-granule and
large-comment fixture. Equivalent low-level timing/gain/corruption behaviors
are unit-tested, but those file-level cases remain required before production.

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

The full default Rust library suite completed with 294 passed, 3 ignored and 0
failed tests. The experimental tests and patched-adapter tests pass. The SBOM
includes the local adapter, `opusic-sys`, and bundled libopus 1.6.1 with the
locked source checksum. CI now builds/tests the feature on the declared macOS,
Windows and Linux targets, but a committed workflow definition is not evidence
that those remote jobs or installed binaries have passed.

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
2. Complete the project's manual MPL-2.0 review for Symphonia, package all
   required notices/license texts and verify access to corresponding source.
3. Expand the file-level fixture corpus listed above and add UI command-level
   cancellation/cleanup tests, including cancellation during STT or formally
   narrow the accepted cancellation contract.
4. Measure installer-size delta and compare release performance against the
   existing baseline on fixed hardware for every target.

Until these are complete, the correct outcome is an available engineering
prototype whose default behavior and production packaging remain unchanged.
