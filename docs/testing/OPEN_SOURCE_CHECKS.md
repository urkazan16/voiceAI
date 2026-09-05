# Full check list — Mac dictation (open sources + CPU generations)

Inventory date: 2026-09-06. Product under test: **LocalFlow** (local Whisper, hold Control+Shift+Space, paste into the frontmost app).

This list merges public field tests and vendor troubleshooting for similar apps (Wispr Flow, Superwhisper, MacWhisper, Apple Dictation, VoiceInk/Unspoken-class tools, whisper.cpp on Mac) with Apple’s own rules for Intel vs Apple silicon. It is a **lab protocol**, not an automated `npm test` suite. Checks already covered in-tree keep their existing IDs (A-17, D-08, K-16…). New items use `OS-` (open-source field) and `HW-` (hardware generation).

Record on every run: git SHA, `.app` vs `tauri dev`, model id, `stt_language`, mic name, host (`sysctl -n machdep.cpu.brand_string` / `uname -m`), RAM, macOS version, Activity Monitor **Kind** (Apple / Intel).

## Sources

| Source | What it contributes |
| --- | --- |
| [Unspoken — 15-minute Mac dictation test](https://tryunspoken.com/blog/how-to-test-a-mac-dictation-app-in-fifteen-minutes/) | Same mic, same text, four real tasks, judge **edited** result, recovery after a bad take |
| [MetaWhisp — same Voice Memo through two engines](https://metawhisp.com/blog/macos-tahoe-dictation/) | One 30 s recording as ground truth; WER on **your** accent |
| [LumeVoice — 10k-word Mac benchmark](https://lumevoice.com/blog/ai-dictation-accuracy-benchmarks-2026/) | WER by domain, end-to-end latency, RAM on M3 Max **and** M1 8 GB |
| [Dictato — 13k recordings / 7 situations](https://dicta.to/blog/speech-to-text-engine-comparison-mac-2026/) | Clean read, accent, disfluency, jargon, longform, brands, proper nouns |
| [IraVoice / DEV — SpeechAnalyzer vs whisper.cpp](https://dev.to/iravoice/apple-speechanalyzer-vs-whispercpp-a-40-speaker-mac-benchmark-40i4) | Publish machine, OS, model, timing boundary; do not mix streaming vs batch |
| [Wispr Flow help](https://docs.wisprflow.ai/articles/4783062859-dictation-appears-in-flow-but-does-not-paste-correctly-in-text-field-text-insertion-fails) | Paste vs transcript-in-app, clipboard restore, copy-last fallback, RDP |
| [TypeWhisper macOS troubleshooting](https://www.typewhisper.com/en/docs/mac/troubleshooting) | Mic + Accessibility as **two** TCC grants; stale TCC after rebuild |
| [Voibe — dictation after sleep](https://www.getvoibe.com/resources/dictation-app-stops-working-after-sleep/) | Sleep/wake, lid, Bluetooth reconnect |
| [Eclectic Light — TCC / Info.plist](https://eclecticlight.co/2025/03/03/managing-privacy-protected-devices/) | `NSMicrophoneUsageDescription` + audio-input entitlement |
| [Apple — universal binaries](https://developer.apple.com/documentation/apple-silicon/building-a-universal-macos-binary) | Native slice on each CPU; Intel Mac cannot run arm64; Rosetta ≠ Intel hardware |
| [WWDC20 10214](https://developer.apple.com/videos/play/wwdc2020/10214/) | CI: test `arch=arm64` and `arch=x86_64`; perf numbers differ under Rosetta |
| [whisper.cpp / Metal notes](https://github.com/ggml-org/whisper.cpp) | M-series vs Intel SIMD; Metal/Core ML vs CPU; do not treat one chip as all chips |
| LocalFlow in-tree | `docs/testing/RELEASE_GATE.md`, `docs/evaluation/BLOCK0.md`, `docs/evaluation/BENCHMARK.md` |

---

## 0. How to score a machine

For each host below, run **block A (once)** then **blocks B–G (smoke)** then **block H (matrix row)**. Fail the host if: app does not launch natively (or documented Intel-only), hotkey does not start capture, or paste fails in TextEdit.

**Minimum ship matrix (3 hosts):**

1. Intel MacBook (x86_64, native — not Rosetta-on-M-series).
2. Apple silicon **8 GB** (typically M1/M2 Air) — RAM cliff from LumeVoice / Dragon-on-8GB reports.
3. Apple silicon **16 GB+** Pro/Max (M1 Pro or newer).

**Full generation matrix** is section H. Do not mark “works on Mac” after a single M-series laptop.

Rosetta (Finder → Get Info → Open using Rosetta) is **extra**: it finds some Intel bugs on an M-chip. It does **not** replace a real Intel Mac ([Apple Forums](https://developer.apple.com/forums/thread/733989), [Apple silicon docs](https://developer.apple.com/documentation/apple-silicon/building-a-universal-macos-binary)).

---

## A. Binary, OS, first launch

| ID | Check | Pass |
| --- | --- | --- |
| A-13 | `npm test` (or documented one-command tests) on that Mac | green |
| A-17 | Build and launch on that macOS (see H for OS × CPU) | window + tray |
| OS-A1 | `file` / `lipo -archs` on the shipped binary: expected `x86_64`, `arm64`, or universal | matches host |
| OS-A2 | Activity Monitor → Kind = **Apple** on M-series, **Intel** on Intel Mac (not translated unless Rosetta test) | native |
| OS-A3 | Gatekeeper: first open of `.app` / `.dmg` | no unexplained crash |
| OS-A4 | Info.plist: microphone usage string present | prompt or Settings row |
| OS-A5 | Second instance does not corrupt the first (B-13) | single instance |
| B-01 | First launch shows setup / model download, not a blank window | onboarding or Models |
| B-03 / B-04 | `~/Library/Application Support/LocalFlow/config/settings.json` exists and is editable JSON | file + apply without rebuild |
| OS-A6 | Autostart login item (if enabled) survives reboot | launches |
| OS-A7 | After `tccutil reset Microphone <bundle>` and `tccutil reset Accessibility <bundle>`, prompts return (TypeWhisper / TCC) | re-prompt |

Bundle id: `app.localflow.desktop`.

---

## B. Microphone and capture (Wispr / Superwhisper / TypeWhisper / Voibe)

| ID | Check | Pass |
| --- | --- | --- |
| C-01 | List built-in mic, USB, Continuity (iPhone) | names match Sound settings |
| C-02 | Select non-default mic and dictate | transcript uses that device |
| C-08 | Unplug selected USB mic mid-session | error, not hang; can pick another |
| OS-B1 | No mic / permission denied: UI tells user to open Privacy settings (Wispr “Microphone Permission Required”) | guidance, no silent fail |
| OS-B2 | Exclusive capture by another app (Zoom) then LocalFlow | clear error or retry |
| OS-B3 | Waveform / bar / tray **●** moves while speaking; flat = no audio (Superwhisper) | indicator live |
| OS-B4 | Hold ≥ 500 ms starts record; short tap does not start hands-free unless checkbox on | matches product rule |
| OS-B5 | Mic LED / Privacy orange dot only while held (or while hands-free on) | off when idle |
| OS-B6 | Sleep 5 min → wake → one utterance (Voibe) | capture works without relaunch |
| OS-B7 | Close lid 30 s → open → utterance | same |
| OS-B8 | Bluetooth headset connect after launch; switch input; dictate; disconnect | no stuck “device gone” without message |
| AM-12 | Same 30 s Voice Memo played to **same** mic for all apps (Unspoken / MetaWhisp) | comparable WER |
| OS-B9 | Quiet room vs fan/AC vs cafe-level noise | log WER, do not mix into one score |
| OS-B10 | Far-field (arm’s length) vs close (20 cm) | both recorded |

---

## C. Hotkey, focus, overlay

| ID | Check | Pass |
| --- | --- | --- |
| D-08 | LocalFlow unfocused; TextEdit focused; Control+Shift+Space | record starts |
| D-07 | Target app fullscreen | hotkey still works |
| OS-C1 | Shortcut conflict (another app owns the combo) | settings show registered vs error |
| OS-C2 | Secure input / password field | paste blocked; Copy last still works |
| OS-C3 | Screen lock: no capture / no insert | idle |
| OS-C4 | Menu bar / tray tooltip: recording vs idle; icon does not flash every RMS tick | stable |
| OS-C5 | Flow bar visible while listening if setting on | animation on screen |
| OS-C6 | Hands-free checkbox off = hold; on = press-toggle; both exist, not the same 320 ms tap | documented behaviour |

---

## D. Recognition quality (Unspoken, MetaWhisp, LumeVoice, Dictato)

Use **one** 30 s Voice Memo of the tester (MetaWhisp). Then live mic for the rest. Measure WER after your own formatter (digits/dates), and optionally raw STT separately (IraVoice: do not mix timing definitions).

| ID | Situation (Dictato 7 + Unspoken) | Example content |
| --- | --- | --- |
| OS-D1 | Clean read-aloud | paragraph of the tester’s language |
| OS-D2 | Accent / non-native (LumeVoice column) | same paragraph |
| OS-D3 | Disfluency | fillers, restarts, « э-э », « ну » |
| OS-D4 | Technical jargon | API, SQL, git, LocalFlow |
| OS-D5 | Names / brands | people, product names (Unspoken “messy sentence”) |
| OS-D6 | Numbers and clock | « встреча в девять часов пять минут » → `09:05` |
| OS-D7 | Dates | `5.3.26` / spoken date → DMY or ISO per setting |
| OS-D8 | Longform > 30 s | one take, no mid-cut |
| OS-D9 | Email task (Unspoken min 0–3) | greeting, ask, sign-off |
| OS-D10 | Notes task | bullets / list voice commands |
| OS-D11 | Language `ru` vs `en` vs `auto` | switch in settings, one clip each |
| F-01 | Live mic WER (not TTS `say`) | log hypothesis vs reference |
| OS-D12 | Hallucination on silence / near-silence | no insert of ghost names |
| OS-D13 | Model tags `[BLANK_AUDIO]` never in paste | stripped |
| OS-D14 | Dictionary / snippet expansion | known replacement appears |
| OS-D15 | Repeat last recording with another model | same wav, different STT |

Do not compare two apps with different mics or different scripts (Unspoken).

---

## E. Insert, clipboard, target apps (Wispr + Unspoken app-switch)

| ID | Target / behaviour | Pass |
| --- | --- | --- |
| OS-E1 | TextEdit / Notes | caret insert, not only in LocalFlow UI |
| OS-E2 | Mail / Outlook compose | |
| OS-E3 | Slack / Telegram desktop | K-16 class |
| OS-E4 | Safari / Chrome text field | K-21 class |
| OS-E5 | Code editor (Cursor, VS Code, Xcode) | |
| OS-E6 | Browser contenteditable / Google Docs | |
| OS-E7 | Terminal | document if blocked |
| OS-E8 | 2000-character paste (K-21) | no truncation |
| OS-E9 | Failed paste: text stays on clipboard **or** Copy last works (Wispr) | recovery |
| OS-E10 | Successful paste restores previous clipboard (text, and if claimed: RTF/image) | restore on |
| OS-E11 | Rapid two utterances: space between (`Привет. Мир`) | no glue |
| OS-E12 | Pause before insert = `insert_delay_ms` | change setting, remeasure |
| OS-E13 | Remote desktop / VDI (Wispr): app runs **locally**; clipboard sharing on guest | document fail if guest blocks |
| OS-E14 | Banking / password manager fields | expected block + fallback |
| OS-E15 | No focused field | user-visible fail, transcript recoverable |

---

## F. Privacy, network, resources (Unspoken privacy + LumeVoice RAM)

| ID | Check | Pass |
| --- | --- | --- |
| P-01 | Airplane mode after models installed: full hold → insert | no network required |
| P-02 | Packet capture during one utterance | no audio/STT upload |
| OS-F1 | Privacy copy matches behaviour (local STT) | |
| OS-F2 | History / journal / uninstall delete | data gone if chosen |
| OS-F3 | Logs: no API keys, emails, full clipboard dumps | redaction |
| OS-F4 | Activity Monitor: idle RSS vs peak during Medium STT | log MB |
| OS-F5 | M1/M2 **8 GB**: Medium model load + one 10 s clip without jetsam | or documented “use Small” |
| OS-F6 | Thermal: three 60 s clips back-to-back | no thermal kill; latency logged |
| OS-F7 | Disk: download Medium (~1.5 GB) with low free space | `ENOSPC` / UI, no corrupt file |
| OS-F8 | Checksum mismatch refuses activation | `MODEL_CHECKSUM_MISMATCH` |

---

## G. Performance numbers to log (LumeVoice latency + LocalFlow BENCHMARK)

Timing boundary: **hotkey release (or end of speech) → text in target app**. Do not mix with “model returned string” only (IraVoice).

| Metric | Target (LocalFlow bench doc) | Log |
| --- | --- | --- |
| Hotkey P95 | ≤ 150 ms | |
| Recording start P95 | ≤ 300 ms | |
| Insertion P95 | ≤ 200 ms | |
| E2E 5 / 10 / 30 s audio | RTF, P50/P95 | |
| Peak RAM | vs 8 GB and 16 GB hosts | |
| WPM by application | stats CSV / UI | |

---

## H. Launch matrix — MacBook CPU generations

Apple: a universal binary contains `x86_64` + `arm64`. The OS runs the **native** slice. An Intel Mac never executes `arm64`. An M-series Mac running the Intel slice is **Rosetta**, not “Intel qualification.”

LocalFlow today: settings force `compute_device=cpu` (no GPU picker). Still re-run STT on each **ISA + RAM class**, because whisper.cpp uses different SIMD (AVX vs NEON) and memory pressure differs.

Identify the **processor model** before scoring (do not write “M-series” or “Intel” alone):

```bash
sysctl -n machdep.cpu.brand_string
uname -m
system_profiler SPHardwareDataType | grep -E 'Chip|Processor Name|Processor Speed|Memory|Model Identifier'
```

On Apple silicon `brand_string` is like `Apple M1 Pro`. On Intel it is the Core SKU, e.g. `Intel(R) Core(TM) i7-9750H CPU @ 2.60GHz`. LocalFlow requires macOS 12+; Sequoia 15 typically needs MacBook Pro/Air **2018+**.

### H1. Architectures (must)

| HW id | Machine class | ISA to run | Notes |
| --- | --- | --- | --- |
| HW-INTEL | One SKU from H1a (Coffee Lake U/H) | native `x86_64` | Required; BLOCK0 was already `x86_64` |
| HW-INTEL-Y | Air **Amber Lake Y** `i5-8210Y` if you claim Air Intel | native `x86_64` | ~7 W; thermal ≠ Pro H |
| HW-INTEL-ICL | Ice Lake `i5-1038NG7` / `i7-1068NG7` (13" Pro 2020, 4× TB3) | native `x86_64` | 10th-gen vs 8th-gen |
| HW-M1-8 | `Apple M1` 8 GB (Air 2020 or 13" Pro 2020) | native `arm64` | RAM cliff |
| HW-M1P | `Apple M1 Pro` or `Apple M1 Max` 16 GB+ | native `arm64` | |
| HW-M2-8 | `Apple M2` 8 GB Air | native `arm64` | |
| HW-M2P | `Apple M2 Pro` or `Apple M2 Max` | native `arm64` | |
| HW-M3 | `Apple M3` (log 8 vs 16 GB) | native `arm64` | |
| HW-M3P | `Apple M3 Pro` or `Apple M3 Max` | native `arm64` | |
| HW-M4 | `Apple M4` | native `arm64` | whisper.cpp M4 quirks |
| HW-M4P | `Apple M4 Pro` or `Apple M4 Max` | native `arm64` | |
| HW-M5 | `Apple M5` Air/Pro 2026 if in lab | native `arm64` | optional |
| HW-ROSETTA | Any M-chip, Open using Rosetta | translated `x86_64` | Extra only |
| HW-ASAN | Same CPU, two macOS majors | | TCC after OS upgrade |

Ultra (`M1 Ultra` …) is Mac Studio / Mac Pro, not a MacBook. Optional desktop row; not a substitute for 8 GB Air.

### H1a. Intel MacBook — processor SKUs

Pick **at least one U or H** and, if you ship Intel Air, **one Y**. Log the exact SKU from `brand_string`. Specs: EveryMac / Apple technical specifications.

| Mac | Year | Processor models (log one) | Microarch | TDP class |
| --- | --- | --- | --- | --- |
| Air 13" | 2018, 2019 | **i5-8210Y** | Amber Lake Y | ~7 W |
| Air 13" | 2020 | **i3-1000NG4**, **i5-1030NG7**, **i7-1060NG7** | Ice Lake | 10th-gen Air |
| Pro 13" 4× TB3 | 2018 | **i5-8259U** (2.3 GHz quad), **i7-8559U** (2.7 GHz) | Coffee Lake U | 15–28 W |
| Pro 13" 4× TB3 | 2019 | **i5-8279U**, **i7-8569U** | Coffee Lake U | |
| Pro 13" 2× TB3 | 2019–2020 | **i5-8257U**, **i7-8557U** | Coffee Lake U | |
| Pro 13" 4× TB3 | 2020 | **i5-1038NG7**, **i7-1068NG7** | Ice Lake | 10th-gen Pro 13" |
| Pro 15" | 2018 | **i7-8750H**, **i7-8850H**, **i9-8950HK** | Coffee Lake H | 45 W |
| Pro 15" | 2019 | **i7-9750H**, **i9-9880H**, **i9-9980HK** | Coffee Lake H | 45 W |
| Pro 16" | 2019 | **i7-9750H**, **i9-9880H**, **i9-9980HK** | Coffee Lake H | 45 W |

Older Intel (2016–2017 `i5-6360U`, `i7-7567U`, …) only if you still support macOS 12/13 on those chassis. Sequoia 15: Pro/Air **2018+**.

### H1b. Apple silicon MacBook — chip models

Log **chip name + CPU/GPU cores + unified RAM**. Air never ships Pro/Max/Ultra. Do not treat **M3** and **M3 Max** as one pass.

| Chip (`brand_string`) | Typical MacBook | CPU cores (P+E) | GPU cores (common) | Unified RAM to log |
| --- | --- | --- | --- | --- |
| **Apple M1** | Air 2020; Pro 13" 2020 | 8 (4+4) | 7 or 8 | **8 GB** required row; 16 GB extra |
| **Apple M1 Pro** | Pro 14/16 2021 | 8 or 10 | 14 or 16 | 16 / 32 GB |
| **Apple M1 Max** | Pro 14/16 2021 | 10 | 24 or 32 | 32 / 64 GB |
| **Apple M2** | Air 13" 2022, Air 15" 2023; Pro 13" 2022 | 8 (4+4) | 8 or 10 | **8 GB** Air row; 16/24 GB |
| **Apple M2 Pro** | Pro 14/16 2023 | 10 or 12 | 16 or 19 | 16+ GB |
| **Apple M2 Max** | Pro 14/16 2023 | 12 | 30 or 38 | 32+ GB |
| **Apple M3** | Air 13/15 2024; Pro 14" base | 8 (4+4) | 8 or 10 | 8 / 16 / 24 GB |
| **Apple M3 Pro** | Pro 14/16 2023 | 11 or 12 | 14 or 18 | 18+ GB |
| **Apple M3 Max** | Pro 14/16 2023 | 14 or 16 | 30 or 40 | 36+ GB |
| **Apple M4** | Air 13/15 2025; Pro 14" base | 10 (4+6 typical Air) | 8 or 10 | 16 GB base on later Air |
| **Apple M4 Pro** | Pro 14/16 2024 | 12 or 14 | 16 or 20 | 24+ GB |
| **Apple M4 Max** | Pro 14/16 2024 | 14 or 16 | 32 or 40 | 36+ GB |
| **Apple M5** | Air 13/15 2026; Pro if sold | 10 (Air: 4+6) | 8 or 10 | 16 / 24 / 32 GB |

### H2. Per-host smoke (copy this table per machine)

Fill Pass/Fail. Do not skip ISA.

| Step | What |
| --- | --- |
| 1 | `uname -m`, exact `brand_string` (SKU or `Apple M3 Pro`), CPU/GPU cores, RAM GB, macOS, Model Identifier |
| 2 | Confirm Kind in Activity Monitor |
| 3 | First launch, grant Mic + Accessibility |
| 4 | One hold-to-talk into TextEdit (OS-E1) |
| 5 | Same 30 s Voice Memo WER (OS-D1) |
| 6 | Peak RAM during Medium (or document fallback to Small) |
| 7 | Airplane-mode one cycle (P-01) if models already on disk |
| 8 | Sleep/wake (OS-B6) |

### H3. What “different generations” is for

| Risk | Why another generation |
| --- | --- |
| AVX vs NEON / CPU whisper | Intel vs all M-series |
| 8 GB jetsam / swap | M1/M2 Air 8 GB vs 16 GB+ |
| Thermal on thin Air vs Pro | Air M1–M4 vs 14/16" Pro |
| whisper.cpp M4 quirks | M4 vs M1/M2 (public issues with beam/fallback) |
| Rosetta perf | WWDC: same test slower/wrong under translation |
| Continuity mic / Bluetooth | varies by Bluetooth stack, not only CPU |

---

## I. Release gate (already in-tree)

Run on **at least** HW-INTEL and one M-series 16 GB+ before tag:

1. Clean checkout, `npm ci`, `npm run check`, `npm test` / `test:all`
2. `npm run license:check`, SBOM, `.app` / `.dmg` hashes
3. `npm run measure:block0` → `docs/evaluation/BLOCK0.md`
4. Fill `docs/evaluation/MVP_SCORECARD.md` with **live mic** numbers, not TTS
5. Offline record→insert + uniqueness report `docs/evaluation/UNIQUENESS.md`

---

## J. Explicitly out of scope for this product (do not fake pass)

| Item | Why |
| --- | --- |
| Wayland / X11 | macOS app |
| Cloud API key in Keychain | no cloud STT account |
| Hold+toggle on one 320 ms key | product forbids that dual-use |
| GPU device picker | this build is CPU-only; still **run** on each CPU generation |
| Dragon-class medical WER | optional corpus, not a ship blocker unless claimed |

---

## K. 15-minute Unspoken protocol (same script on every host)

1. Minutes 0–3: one email into Mail or Notes.  
2. Minutes 3–6: one note with a list.  
3. Minutes 6–9: Slack or browser field + a code editor.  
4. Minutes 9–12: messy sentence (names, `09:05`, a date).  
5. Minutes 12–15: **recovery** — force a bad take (mute mic), confirm Copy last / retry.

Judge whether you would use the app tomorrow (Unspoken), not only WER of the first line.
