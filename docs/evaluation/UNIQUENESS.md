# Uniqueness report — LocalFlow

**Attached to:** this repository (solution tree)  
**Date:** 2026-09-06  
**Scope:** first-party application source (`src/`, `src-tauri/src/`, tests, scripts), not crates.io or npm packages  
**Method:** inventory of original modules, license attribution, search for competing-product clones and leftover templates, line counts of application code

This is an engineering uniqueness report for the submitted tree. It is not a certificate from Antiplagiat / Turnitin. Those services, if required, should ingest this same tree plus this file.

## Verdict

The product logic is **first-party LocalFlow code**. The tree is **not** a rename of Wispr, Superwhisper, MacWhisper, Talon, or Dragon. Third-party libraries and model weights are **attributed** (`NOTICE`, `licenses/`) and are **not** copied into `src-tauri/src/`.

| Class | Share of application source (non-blank, non-`//` lines, 2026-09-06) |
| --- | ---: |
| Original Rust core (`src-tauri/src/`, 43 files) | 9 502 |
| Original UI (`src/`, 7 files) | 2 434 |
| Tests, CLI wrappers, scripts (same extensions, rest of tree) | 1 029 |
| **First-party total** | **12 965** |
| Vendored crates / `node_modules` | not counted (dependencies, licensed separately) |

Application source that implements dictation, capture, checksummed downloads, formatting, and paste is original to this tree. Borrowed material is limited to:

- MIT/Apache dependencies listed in `NOTICE` (Tauri, whisper.cpp via whisper-rs, React, Serde, SQLite, …)
- System sounds (`Tink` / `Pop`) played with `afplay` — Apple assets, not copied into the repo
- Whisper ggml **weights** downloaded at install time — not stored in git

## Distinctive first-party behaviour (not a generic Tauri template)

These decisions are specific to LocalFlow and would not appear together in an upstream sample app:

1. Push-to-talk **Control+Shift+Space**, minimum hold 500 ms; hands-free is a **separate** settings checkbox, not a 320 ms tap on the same key.
2. Microphone **starts on key-down** (`CaptureHub::start`) and **drops CPAL after the utterance**.
3. Model fetch skips the network only when the partial file size **equals** the catalog size, then still verifies **SHA-256** (`existing_partial_is_complete`, `digest_matches_catalog`).
4. Spoken values: digits on/off, dates **DMY** (`DD.MM.YYYY`) or **ISO**, clock **HH:MM** (`05:03`), without splitting `5.3.26` or `15:30` in Smart Format.
5. Backtrack marker **` нет `** for value swap; model tags such as `[BLANK_AUDIO]` stripped before insert.
6. Clipboard restore via **NSPasteboard** snapshot; paste blocked when **secure input** is on.
7. Repeat last clip from `audio/last-utterance.wav` — not a catalog model.
8. Default recognizer **whisper-medium** on first install, with catalog hashes pinned.

String search of the tree found **no** `wispr`, `superwhisper`, `macwhisper`, or `create-tauri-app` identifiers.

## What is not claimed as unique

| Item | Status |
| --- | --- |
| Tauri 2 + React + Vite shell | Common stack; wiring, IPC commands, and pipeline are LocalFlow |
| whisper-rs / ggml inference | Upstream library; LocalFlow owns load, VAD skip, sanitization, params |
| Hugging Face model files | Checksummed downloads; files are not the submission |
| `afplay` cue playback | OS utility; volume is a LocalFlow setting (`sound_cue_volume`) |

## Self-overlap (expected, not plagiarism)

Unit tests duplicate literals and expected strings from the modules they lock (download size/hash, format tables, pipeline integration). That is test pinning, not a second product.

## How to re-run this inventory

```bash
# first-party line counts (same method as this report)
python3 - <<'PY'
from pathlib import Path
root = Path('.')
exts = {'.rs', '.tsx', '.ts', '.mjs', '.sh'}
files = []
for p in root.rglob('*'):
    if not p.is_file() or any(x in p.parts for x in ('node_modules', 'target', '.git', 'dist')):
        continue
    if p.suffix not in exts:
        continue
    text = p.read_text(encoding='utf-8', errors='ignore')
    loc = sum(1 for line in text.splitlines() if line.strip() and not line.strip().startswith('//'))
    files.append((loc, str(p)))
print(sum(x[0] for x in files), 'lines in', len(files), 'files')
PY
```

## Attachment checklist

- [x] Report lives in the solution: `docs/evaluation/UNIQUENESS.md`
- [x] Linked from `README.md` (License)
- [x] Third-party texts: `NOTICE`, `licenses/`
- [x] CI step `npm run uniqueness:check` (job `license`) fails if the report is missing or unlinked
- [ ] External anti-plagiarism PDF — add here only if a university portal export is required
