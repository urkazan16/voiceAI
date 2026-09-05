# MVP scorecard

Status: **not scored**. Fill after the release-gate measurements in `RELEASE_GATE.md`. Do not invent passing totals.

```text
Accuracy                         --/25
Speed                            --/20
Reliability                      --/20
Product completeness             --/15
Engineering quality               --/10
Reproducibility/privacy/license   --/10
────────────────────────────────────
TOTAL                            --/100
```

Gates currently implemented in-tree (engineering, not accuracy/speed benches):

- LICENSE, NOTICE, third-party notices
- lockfiles generated on `npm install` / first `cargo build`
- SHA-256 model activation (`MODEL_CHECKSUM_MISMATCH`)
- local data boundary and history/personalization reset
- no cloud account in privacy summary
