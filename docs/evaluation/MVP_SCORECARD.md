# MVP scorecard

Status: **block 0 started**, not a release score. Host measurement: `docs/evaluation/BLOCK0.md` (2026-09-05, macOS 15.7.9 x86_64). Do not treat TTS WER as Accuracy.

```text
Accuracy                         --/25   (live mic WER not run; TTS Milena+whisper-base mean 0.73)
Speed                            --/20
Reliability                      --/20   (TextEdit paste skipped: Automation timeout)
Product completeness             --/15
Engineering quality               --/10
Reproducibility/privacy/license   --/10
────────────────────────────────────
TOTAL                            --/100
```

Repeat: `npm run measure:block0`. Still need a person for D-07/D-08, Telegram/Safari/Cursor insert, Bluetooth, airplane-mode sniffer, live microphone WER.

Gates currently implemented in-tree (engineering, not accuracy/speed benches):

- LICENSE, NOTICE, third-party notices
- lockfiles generated on `npm install` / first `cargo build`
- SHA-256 model activation (`MODEL_CHECKSUM_MISMATCH`)
- local data boundary and history/personalization reset
- no cloud account in privacy summary
