# LocalFlow

Local, private, CPU-first voice-to-text for macOS.

```text
1. Capture   — hold Control+Shift+Space, microphone on only while held
2. Recognize — local Whisper ggml (SHA-256 + format check before use)
3. Format    — dictionary, backtrack, digits / dates / HH:MM, optional LLM
4. Insert    — paste into the frontmost app, restore the previous clipboard
```

Speech-to-text, dictionary, personalization, and optional local formatting run on this machine. Inserted text is never executed.

License: MIT. See `LICENSE`.

```text
./install.sh
npm run tauri dev
```

Hold **Control+Shift+Space**, speak, release. Do not use Option+Space.

One-file uninstall: `scripts/uninstall.sh` (asks whether to keep history). The Privacy screen has the same Uninstall button.

## Prerequisites (minimum versions)

| Tool                     | Minimum                        |
| ------------------------ | ------------------------------ |
| macOS                    | 12                             |
| Node.js                  | 20.19.0 (see `.nvmrc`)         |
| npm                      | 10                             |
| Rust / Cargo             | 1.88.0 (`rust-toolchain.toml`) |
| CMake                    | 3.16                           |
| Xcode Command Line Tools | current for the OS             |
| Git                      | 2.30                           |

Tauri CLI is installed via `npm install` (`@tauri-apps/cli@2.2.7`). Do not use a globally installed `latest` CLI.

After installing Rust, add Cargo to your shell (or open a new terminal):

```bash
source "$HOME/.cargo/env"
```

`npm run check` and `npm run tauri` also look in `~/.cargo/bin` so they work if rustup is installed but not sourced.

Microphone / Accessibility strings live in `src-tauri/Info.plist` (merged by Tauri). Do not put `infoPlist` under `bundle.macOS` — CLI 2.2 rejects that key.

There are no secret environment variables and no absolute developer paths in the build.

## Commands

| Command                 | What it does                                                     |
| ----------------------- | ---------------------------------------------------------------- |
| `npm install`           | Install JS dependencies from `package-lock.json`                 |
| `npm run check`         | TypeScript, ESLint, Prettier, `cargo check`, `cargo fmt`, Clippy |
| `npm test`              | Frontend + Rust unit + integration tests                         |
| `npm run test:all`      | Unit, integration, UI, pipeline, dictionary, personalization     |
| `npm run test:ai`       | AI benchmark profile (requires catalog + optional local models)  |
| `npm run build`         | Frontend production bundle + debug Rust binary                   |
| `npm run build:release` | Checks UI, builds Rust, packages `.app`/`.dmg`, SBOM, SHA-256    |
| `npm run check:local`   | Offline checker (WER + VAD SNR 15 dB), no network                |
| `npm run license:check` | Dependency license allowlist                                     |
| `npm run uniqueness:check` | Confirms `docs/evaluation/UNIQUENESS.md` is attached           |

Headless CLI (no window):

```bash
cargo run --manifest-path src-tauri/Cargo.toml -- --help
cargo run --manifest-path src-tauri/Cargo.toml -- check
cargo run --manifest-path src-tauri/Cargo.toml -- transcribe --json --language ru speech.wav
cargo run --manifest-path src-tauri/Cargo.toml -- transcribe --dir ./clips --no-postprocess
ffmpeg -f avfoundation -i ":0" -t 3 -f wav - | cargo run --manifest-path src-tauri/Cargo.toml -- transcribe --stdin
```

Settings live in `~/Library/Application Support/LocalFlow/config/settings.json` (JSON). Edits apply within a couple of seconds without rebuilding. Schema of the replica journal: `docs/journal/UTTERANCE.md`.

## Settings

| Key                                     | Meaning                                                       |
| --------------------------------------- | ------------------------------------------------------------- |
| `hotkey`                                | Push-to-talk shortcut                                         |
| `microphone_name`                       | Input device, or `null` for the OS default                    |
| `active_stt_model` / `active_llm_model` | Catalog ids (see Model Manager)                               |
| `stt_language`                          | `ru`, `en`, or `auto`                                         |
| `mode`                                  | Fallback pipeline: `raw` / `normal` / `professional` / `code` |
| `autostart`                             | Launch at login                                               |
| `history_enabled`                       | SQLite history + JSONL journal                                |
| `sound_cues`                            | Start/end beeps                                               |
| `sound_cue_volume`                      | Cue loudness 0.05–1.0 (default 0.25)                          |
| `insert_delay_ms`                       | Pause before paste                                            |
| `hands_free`                            | Press-to-toggle listen; off = hold-to-talk                    |
| `digits_from_speech`                    | Spoken numbers become digits                                  |
| `date_format`                           | `DMY` (DD.MM.YYYY) or `ISO`                                   |
| `compute_device`                        | Inference device; this build is CPU only                      |
| `postprocess_timeout_ms`                | Cap on formatting                                             |
| `restore_clipboard`                     | Restore clipboard after paste                                 |
| `vad_threshold`                         | Silence trim sensitivity (default 0.012)                      |
| `history_max_items`                     | SQLite history rotation cap (default 500)                     |
| `log_max_bytes`                         | Size rotation for `localflow.log`                             |

Replace the recognizer by downloading another Whisper ggml in Model Manager, or set `active_stt_model` in `settings.json` to a catalog id whose file is already verified.

`npm run sbom` writes a CycloneDX SBOM.

First Cargo fetch needs network. After `src-tauri/Cargo.lock` is present, crates resolve reproducibly.

## Models

Weights are **not** inside the `.app`. On `./install.sh` and on first GUI launch LocalFlow downloads the **active speech model** (default Whisper Medium, ~1.5 GB, `ggml-medium.bin`) from Hugging Face, then verifies SHA-256 and ggml magic before activation. Qwen formatting models stay optional in Model Manager.

Skip the network step with `LOCALFLOW_SKIP_MODEL_DOWNLOAD=1`. Retry anytime: `npm run download:stt` or `localflow download --model whisper-medium`.

Before a model is used:

1. SHA-256 verification
2. Format validation (GGUF / ggml)
3. Activation

Mismatch raises `MODEL_CHECKSUM_MISMATCH` and the model is not loaded.

User data lives in `~/Library/Application Support/LocalFlow/` (override with `LOCALFLOW_DATA_DIR` for tests).

## License

MIT. See `LICENSE`, `NOTICE`, `licenses/`, and `docs/licensing/`.

Uniqueness report (attached to this tree): `docs/evaluation/UNIQUENESS.md`.
