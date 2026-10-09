# Symphonia/libopus prototype — implementation report

Updated: 2026-10-09
Scope: stages 1–5 of `SYMPHONIA_LIBOPUS_BACKEND_SPEC.md`
Decision: **approved for optional distribution; do not enable by default yet**

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

The manual project MPL-2.0 review and exact-source access path are approved.
The cancellation requirement is formally scoped to decode/resample and
Whisper. Local macOS x64 package measurement is complete; retained package
smoke and measurements on the other declared targets remain release gates.

## Stage ledger

| Stage                               | Result                                  | Evidence                                                                                                                                                    |
| ----------------------------------- | --------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1. Build spike and licensing review | PASS                                    | Rust 1.88 incompatibility reproduced; local patch approved and pinned; MPL review and exact-source manifest recorded                                        |
| 2. Isolated decoder                 | PASS on the host                        | Profile validator, strict decoder, trim/gain/downmix, stateful FIR resampler, reference PCM and signal tests pass                                           |
| 3. Resources and integration        | PASS under scoped cancellation contract | Streamed input, budgets/deadline, independent cancellation, RAII staging cleanup, UI/CLI route and documented result suppression for synchronous native STT |
| 4. Robustness and distribution      | PARTIAL                                 | Expanded fixtures, bundled notices/source manifest, local macOS package measurement and inspection pass; remote target measurements remain pending          |
| 5. Candidate decision               | APPROVE OPTIONAL DISTRIBUTION           | Keep feature and runtime route off by default until retained target-matrix evidence is green                                                                |

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
- Provides a model-free `opus-decode-smoke` CLI diagnostic for packaged-runtime
  verification; it reports backend, sample rate, sample count, decode time and
  process peak RSS.
- Cleans staged uploads with an owning RAII guard on success, error,
  cancellation and Rust unwind. Abrupt process termination remains subject to
  normal OS/application temp cleanup.

Whisper file recognition now carries the independent file token into
whisper.cpp's abort callback. A queued task polls the token every 50 ms, and a
unit test verifies return within one second. Dictation continues to use its own
global token. The T-One, GigaAM and Parakeet wrappers do not expose a comparable
mid-inference abort callback. Their accepted contract is immediate cancellation
state plus result suppression before/after synchronous native inference; it is
not a sub-second worker-release guarantee. New native engines conservatively
default to the same mode. The complete decision is recorded in
`STT_CANCELLATION_CONTRACT.md`.

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

The benchmark executable was 810,344 bytes. The 60-second file was generated
only for local performance measurement and is not committed as a fixture.

The final candidate was measured again as two otherwise identical release
builds from the same checkout and toolchain. The default executable was
36,731,776 bytes and the `audio-symphonia-opus` executable was 37,195,680
bytes: a 463,904-byte (1.263%) increase. The earlier pre-instrumentation result
was +459,752 bytes (+1.231%). `otool -L` listed no libopus dylib.

Unsigned headless DMGs were then created from those apps with the same Tauri
`create-dmg` script, fixed 150 MiB intermediate image and compression settings.
The default DMG was 32,146,123 bytes and the feature DMG was 32,400,412 bytes:
a 254,289-byte (0.791%) increase. This is a controlled local payload comparison,
not a signed/notarized release-size claim.

Running the packaged one-second fixture from the final feature app took 9.964
ms and reported 7,155,712 bytes peak RSS. Repeated process launches vary, so
the retained per-target CI reports, rather than this single sample, are the
release comparison record.

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
cargo build --manifest-path src-tauri/Cargo.toml --locked --release --bin localflow
cargo build --manifest-path src-tauri/Cargo.toml --locked --release \
  --bin localflow --features audio-symphonia-opus
npx tauri build --bundles app \
  --config '{"bundle":{"macOS":{"signingIdentity":null}}}' -- \
  --locked --features audio-symphonia-opus
LOCALFLOW_SKIP_BUILD=1 LOCALFLOW_AUDIO_SYMPHONIA_OPUS=1 \
  npm run build:release
```

The full default Rust library suite completed with 301 passed, 3 ignored and 0
failed tests. With the feature enabled, 328 passed, 3 were ignored and 0
failed; both runs also passed all 46 integration, acceptance and performance
tests. The standalone patched adapter adds 2 passing tests. The frontend suite
passes 28 tests. The SBOM
includes the local adapter, `opusic-sys`, and bundled libopus 1.6.1 with the
locked source checksum. CI now builds/tests the feature on the declared macOS,
Windows and Linux targets, runs a real decode, and rejects a dynamic libopus
dependency. Local `otool -L` inspection found no libopus dylib. Tauri is
configured to embed `NOTICE` and the separate license texts in every installed
application, and the package job retains those files, the SBOM and changelog as
CI artifacts. `SHA256SUMS` covers the SBOM, NOTICE, changelog and every separate
license and source-manifest file as well as top-level installer files. The
manual review in `docs/licensing/SYMPHONIA_MPL_REVIEW.md` approves the five
unmodified Symphonia 0.6.1 crates and records exact crates.io archive URLs and
Cargo.lock hashes. A committed workflow definition is not evidence that remote
jobs or installed binaries have passed.

A local unsigned macOS x64 experimental `.app` was also built in release mode.
Its actual `Contents/Resources` contains `NOTICE`, the complete
`THIRD_PARTY_LICENSES` directory and the model catalog at their configured
paths. The packaged executable contains the Symphonia/libopus route and has no
dynamic libopus dependency. The release wrapper executed `opus-decode-smoke`
from that packaged executable and verified exactly 16,000 mono samples at 16
kHz from the pinned one-second fixture. This host bundle inspection is green,
but it is not evidence for Windows and Linux installers. The controlled local
unsigned/headless DMG size comparison is recorded above.

CI has a separate manual `opus_package_smoke` dispatch. It builds all four
declared target packages, executes the same packaged-runtime smoke, and does
not upload or publish the experimental installers. The workflow definition is
committed, but a remote run must still be retained before release acceptance.

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

The release wrapper can build the experimental artifact and matching SBOM with:

```sh
LOCALFLOW_AUDIO_SYMPHONIA_OPUS=1 npm run build:release
```

Without that build-time variable, `npm run build:release` remains a default
feature-off build.

## Unsupported inputs

WebM/Matroska/MP4 Opus, raw Opus, Vorbis-in-Ogg, mapping families other than 0,
more than two channels, chained or multiplexed Ogg, live streams, packet-loss
concealment and partial recovery remain unsupported. This work does not add or
promise native AAC/M4A, ALAC, MP3, FLAC, AIFF or Vorbis support.

## Remaining production gates

1. Run and retain green CI/runtime evidence on Windows x64, macOS x64/arm64 and
   Linux x64 using the manual `opus_package_smoke` matrix, without FFmpeg or
   system libopus, and inspect their dynamic dependencies. The macOS x64 local
   packaged-runtime smoke is green; the remote matrix has not been run here.
2. Retain installer-size and packaged performance results from that matrix for
   macOS arm64, Windows x64 and Linux x64. The fixed-host macOS x64 measurement
   is complete and recorded above.

Until these cross-target checks are retained, the backend remains optional and
off by default; the existing default audio behavior remains unchanged.
