# Symphonia/libopus prototype — stage 1 report

Date: 2026-10-08  
Scope: stage 1 of `SYMPHONIA_LIBOPUS_BACKEND_SPEC.md`  
Decision: **BLOCKED before integration**

## Result

The requested starting pair cannot be built with LocalFlow's pinned Rust 1.88 toolchain:

- `symphonia 0.6.1` supports Rust 1.85 and is compatible with the project.
- `symphonia-adapter-libopus 0.3.0` declares `rust-version = "1.89"` and uses syntax that Rust 1.88 rejects.
- `cargo check --features audio-symphonia-opus --lib` stops at the MSRV gate.
- `cargo check --features audio-symphonia-opus --lib --ignore-rust-version` also fails, so ignoring package metadata is not a workaround. Rust 1.88 rejects `pcm: [0.0; _]` in the adapter with `E0658`.

The base release toolchain was not changed. No decoder, route, UI claim, notice, or production default was added after this failure.
The existing default build remains healthy: `cargo check --manifest-path src-tauri/Cargo.toml --locked --lib` passes with the experimental feature disabled.

## Reproduction

Run from the repository root:

```sh
rustc --version
cargo check --manifest-path src-tauri/Cargo.toml \
  --features audio-symphonia-opus --lib
cargo check --manifest-path src-tauri/Cargo.toml \
  --features audio-symphonia-opus --lib --ignore-rust-version
```

Observed toolchain:

```text
rustc 1.88.0 (6b00bc388 2025-06-23)
cargo 1.88.0 (873a06493 2025-05-10)
```

The normal check reports:

```text
symphonia-adapter-libopus@0.3.0 requires rustc 1.89
```

The diagnostic check that ignores package metadata reports:

```text
error[E0658]: using `_` for array lengths is unstable
.../symphonia-adapter-libopus-0.3.0/src/lib.rs:113:24
```

## Pinned feature and component tree

The spike adds an opt-in feature that remains outside `default`:

```toml
audio-symphonia-opus = ["dep:symphonia", "dep:symphonia-adapter-libopus"]
symphonia = { version = "=0.6.1", default-features = false, features = ["ogg"], optional = true }
symphonia-adapter-libopus = { version = "=0.3.0", default-features = false, features = ["bundled"], optional = true }
```

Selected new runtime/build components:

| Component | Version | Role | License / source evidence |
| --- | ---: | --- | --- |
| `symphonia` | 0.6.1 | facade, Ogg probe registration | MPL-2.0; crates.io checksum `a7edef6a96b696d4e0cab5ee9ebb7ca155ed95f30a6b45bbb8b97d2727f02424` |
| `symphonia-core` | 0.6.1 | codec registry and audio buffers | MPL-2.0; locked in `Cargo.lock` |
| `symphonia-common` | 0.6.1 | Ogg/Opus header parsing | MPL-2.0; locked in `Cargo.lock` |
| `symphonia-format-ogg` | 0.6.1 | Ogg demuxer | MPL-2.0; locked in `Cargo.lock` |
| `symphonia-metadata` | 0.6.1 | metadata support required by the Ogg feature | MPL-2.0; locked in `Cargo.lock` |
| `symphonia-adapter-libopus` | 0.3.0 | Symphonia audio decoder adapter | MIT OR Apache-2.0; crates.io checksum `c6febe6f88f9a9483db7e5b72a2dad916d8f8eb588d18905a39c861319fc7fa1` |
| `opusic-sys` | 0.7.5 | libopus FFI and bundled build | BSD-3-Clause; crates.io checksum `c9d1ecdf206421bc74343ab3bb2f30ad2abbfee41fa341f7181fecbaf957769a` |
| vendored libopus | 1.6.1 | native Opus decoder | BSD-3-Clause plus the published patent grant in its `COPYING` file |
| `cmake` crate | 0.1.58 | native build driver | build dependency selected by `bundled` |

The resolved feature path is `audio-symphonia-opus -> symphonia-adapter-libopus/bundled -> opusic-sys/bundled`. The `opusic-sys` build script emits static linking for `opus`; it does not select a system libopus in this configuration. The inspected host used CMake 4.4.3 and Apple clang 17.0.0.

The adapter license choice is not yet made. If a patched adapter is approved, LocalFlow should record one option explicitly; Apache-2.0 is the proposed option because it carries an explicit patent grant. This is a release-review input, not legal approval.

## Source audit findings that remain after the MSRV blocker

These findings mean that merely raising Rust to 1.89 would not complete stage 2:

1. The adapter stores pre-skip, applies it to the first decoded packet, then unconditionally sets it to zero. Symphonia's buffer trim clears a packet when trim is greater than its frame count. Therefore pre-skip larger than the first packet is not carried into later packets, while the required v1 profile explicitly tests that case.
2. The Ogg parser reads the Opus header output gain, but neither the Ogg mapping nor adapter applies that value to decoded PCM. Positive and negative gain fixtures would fail the mandatory signal gate unless LocalFlow applies the gain exactly once after validating the header.
3. Symphonia accepts mapping family 1 for valid channel layouts, while v1 requires family 0 only. A LocalFlow profile validator is still required before decoding.
4. Symphonia 0.6.1 verifies Ogg page CRC and returns a decode error on mismatch. This part of the corruption requirement has an upstream implementation to exercise with fixtures.
5. The adapter is limited to one or two channels, as required for the accepted subset, but that does not by itself distinguish unsupported channel mapping from corrupt input.

## Target status

| Gate | Status | Evidence / reason |
| --- | --- | --- |
| Rust 1.88 host build | FAIL | Adapter requires 1.89 and uses syntax rejected by 1.88 |
| bundled libopus selection | PASS (configuration/source audit) | `bundled` reaches `opusic-sys`; static `opus` link emitted |
| bundled libopus C compilation on macOS arm64 host | PASS | `opusic-sys 0.7.5` compiled before the Rust adapter failure |
| macOS arm64 complete build/run | BLOCKED | Adapter Rust compilation fails |
| macOS x64 build/run | BLOCKED | Same toolchain blocker; target is installed but was not treated as runtime proof |
| Windows x64 build/run | BLOCKED | Same toolchain blocker; no Windows runtime available in this spike |
| Linux x64 build/run | BLOCKED | Same toolchain blocker; target/runtime not available in this spike |
| timing/gain/reference fixtures | BLOCKED | Decoder cannot compile; source audit already identifies required fixes |
| installer/runtime-library smoke | BLOCKED | No complete experimental binary exists |
| licensing approval for distribution | PENDING | Existing MPL exception names Servo/cssparser only; `NATIVE.md` disallows unreviewed codec backends |

## Decision required before stage 2

One of these paths must be explicitly approved:

1. Upgrade the project release toolchain to Rust 1.89 or newer, then still patch or replace the adapter for multi-packet pre-skip and output gain.
2. Keep Rust 1.88 and approve a pinned local/upstream adapter patch. At minimum it must replace the unstable inferred array length, declare the truthful MSRV, preserve remaining pre-skip across packets, and add tests. LocalFlow must still validate mapping family and apply output gain exactly once.
3. Wait for an upstream adapter release that supports Rust 1.88 and passes the required timing/gain tests.
4. Reject this candidate and evaluate a different decoder. A custom Ogg demuxer or automatic multistream implementation is outside the current specification.

Downgrading to the adapter 0.2 series is not a drop-in option because that series targets Symphonia 0.5, while the requested candidate uses Symphonia 0.6.

## Stage ledger

| Stage | State |
| --- | --- |
| 1. Build spike and licensing review | Completed with blocker; distribution review remains pending by design |
| 2. Isolated decoder | Not started — blocked by stage 1 |
| 3. Resources and integration | Not started — blocked by stage 1 |
| 4. Robustness and distribution | Not started — blocked by stage 1 |
| 5. Final candidate decision | Candidate cannot be accepted in the current pinned configuration |
