# Testing

| Command               | Coverage                                                             |
| --------------------- | -------------------------------------------------------------------- |
| `npm test`            | Frontend unit, Rust lib, integration                                 |
| `npm run test:all`    | Adds UI, pipeline, dictionary, personalization                       |
| `npm run test:ai`     | AI catalog / future inference bench                                  |
| `npm run check:local` | Offline checker: WER identity + VAD at SNR 15 dB (`localflow check`) |
| `npm run measure:block0` | Host gate: OS, mics, single-instance, TTS WER → `docs/evaluation/BLOCK0.md` |

CLI (no GUI):

```bash
cargo run --manifest-path src-tauri/Cargo.toml -- check
cargo run --manifest-path src-tauri/Cargo.toml -- transcribe --json --language ru file.wav
```

Corpus lives in `tests/corpus/` with `expected.json` per domain. Audio fixtures are tiny and synthetic; large recordings are not stored in git.

Reliability and network-privacy tests that need a full `.app` are described in `docs/testing/RELEASE_GATE.md`.
