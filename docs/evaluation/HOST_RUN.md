# Host run — open-source checklist (one Mac)

**When:** 2026-09-06 (local ~01:20)  
**Git:** `440cfee` + uncommitted working tree  
**Method:** automated checks from `docs/testing/OPEN_SOURCE_CHECKS.md` on **this** machine only. Live PTT into other apps was **not** run (would steal focus from the IDE).

## Hardware (H / H1a)

| Field | Value |
| --- | --- |
| Model | MacBook Pro 15" (`MacBookPro15,1`) |
| Processor | **Intel(R) Core(TM) i7-9750H CPU @ 2.60GHz** (Coffee Lake H, 6 cores / 12 threads) |
| ISA | native **x86_64** (`file` on debug `localflow`: Mach-O 64-bit executable x86_64) |
| RAM | 16 GB |
| Disk free | ~1.6 GB (volume ~100% full) |
| OS | macOS **15.7.9** (24G830) |
| Matrix row | **HW-INTEL** / Pro 15" 2019 SKU `i7-9750H` |
| Not covered | Air Y (`i5-8210Y`), Ice Lake (`i5-1038NG7`), any `Apple M1`…`M5`, Rosetta |

This host satisfies the **Intel H-class** cell of the minimum ship matrix. It does **not** satisfy M1 8 GB or M-series 16 GB+ rows.

## Automated results

| ID | Result |
| --- | --- |
| A-13 lib tests | **127/128 then 128/128** after equal-length poison file in download checksum test |
| A-13 frontend | **4/4** Vitest |
| A-13 integration | `integration_dictation_pipeline_values_tags_and_spacing` **PASS** (`25`, `15:30`, `05.03.2026`, tags stripped, `Привет. Мир`) |
| A-13 acceptance | 25/26 then empty-transcript test aligned with no-speech fail (**Failed**, not idle insert) |
| `npm run license:check` | **PASS** |
| CLI `check` | whisper_ready **whisper-medium**; wer_identity PASS; vad_snr_15db PASS; settings_json PASS; single_instance PASS; offline PASS |
| C-01 devices | MacBook Pro mic (default); Continuity **iPhone 13** |
| B-03 settings | JSON present; hotkey Control+Shift+Space; `stt_language=ru`; `digits_from_speech=true`; `date_format=DMY`; `hands_free=false`; `compute_device=cpu` |
| Audio dir mode | `~/…/LocalFlow` and `audio/` **0o700** |
| Medium sidecar | SHA-256 `6c14d5ad…156208`, size 1 533 763 059 |
| P-01 checker TCP | established non-loopback 128 → 119 during `check` (not airplane mode; not a packet capture) |
| F-01 TTS (not live mic) | Milena → Medium, wav ~60 KB: reference «пятнадцать часов тридцать минут» → `raw` `15 часов 30 минут.` → `text` **`15:30.`** (~22 s wall including model load) |
| Screen lock | `screen_locked: false` |
| Binary Kind | Intel native, not Rosetta |

## Failures found and fixed on this run

1. `complete_partial_with_wrong_hash_fails_checksum_without_network` — catalog vs poison byte lengths 10 vs 11; poison body set to 10 bytes.  
2. `pipeline_empty_transcript_stays_idle_after_reset` — empty scripted path now returns **No mic signal** (correct); snapshot goes to **Failed**.

## Not run (need a person / other CPUs)

D-07 fullscreen, D-08 hotkey while unfocused, K-16…K-21 paste into Telegram/Safari/Cursor, AM-12 Bluetooth, OS-B6 sleep/wake, P-01 airplane record→insert, F-01 **live microphone**, Unspoken 15-minute four-app protocol, entire Apple silicon matrix (M1–M5).

TextEdit Automation paste: previously timed out; **not retried** (activates TextEdit over the IDE).
