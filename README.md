# LocalFlow

Local, private, CPU-first voice-to-text for macOS. Hold **Option+Space**, speak, release. Speech-to-text, dictionary, personalization, and optional local LLM formatting run on this machine. Inserted text is never executed.

```text
./install.sh
npm run tauri dev
```

Hold **Control+Shift+Space**, speak, release. Speech-to-text, dictionary, personalization, and optional local formatting run on this machine. Inserted text is never executed.

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

| Command                 | What it does                                                                           |
| ----------------------- | -------------------------------------------------------------------------------------- |
| `npm install`           | Install JS dependencies from `package-lock.json`                                       |
| `npm run check`         | TypeScript, ESLint, Prettier, `cargo check`, `cargo fmt`, Clippy                       |
| `npm test`              | Frontend + Rust unit + integration tests                                               |
| `npm run test:all`      | Unit, integration, UI, pipeline, dictionary, personalization                           |
| `npm run test:ai`       | AI benchmark profile (requires catalog + optional local models)                        |
| `npm run build`         | Frontend production bundle + debug Rust binary                                         |
| `npm run build:release` | Checks UI, builds Rust, packages `.app`/`.dmg`, SBOM, SHA-256 |
| `npm run license:check` | Dependency license allowlist                                                           |
| `npm run sbom`          | CycloneDX SBOM                                                                         |

First Cargo fetch needs network. After `src-tauri/Cargo.lock` is present, crates resolve reproducibly.

## Models

Installer does **not** ship multi-gigabyte weights. Download is an explicit, labeled network action in Model Manager. Before activation:

1. SHA-256 verification
2. Format validation (GGUF / ggml)
3. Activation

Mismatch raises `MODEL_CHECKSUM_MISMATCH` and the model is not loaded.

User data lives in `~/Library/Application Support/LocalFlow/` (override with `LOCALFLOW_DATA_DIR` for tests).

## License

MIT. See `LICENSE`, `NOTICE`, `licenses/`, and `docs/licensing/`.
