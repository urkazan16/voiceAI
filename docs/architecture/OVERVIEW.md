# Architecture

LocalFlow.app contains Tauri, the Rust core (whisper-rs STT), and the UI. Model weights stay in:

```text
~/Library/Application Support/LocalFlow/models/{whisper,llm}/
```

```text
Hotkey → Audio → STT → Dictionary → Personalization → LLM → Validate → Paste
```

Abstractions:

| Concern         | Module                             |
| --------------- | ---------------------------------- |
| Audio           | `src-tauri/src/audio.rs`           |
| STT             | `src-tauri/src/stt.rs`             |
| LLM             | `src-tauri/src/llm.rs`             |
| Injection       | `src-tauri/src/injection.rs`       |
| Personalization | `src-tauri/src/personalization.rs` |
| Models          | `catalog.rs` + `integrity.rs`      |
| Pipeline        | `pipeline.rs` + `engine.rs`        |

Generated code is inserted as text only. It is never executed.
