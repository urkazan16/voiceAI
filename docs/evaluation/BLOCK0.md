# Block 0 measurement

Generated: 2026-09-05T17:51:49Z

## A-17 environment
```
ProductName:		macOS
ProductVersion:		15.7.9
BuildVersion:		24G830
uname: x86_64 Darwin 24.6.0
```

## CLI check (T-16, B-13, C-06, AG-07 query)
```
os: macos x86_64
macos: 15.7.9
screen_locked: false
mic_count: 2
mic_default: Микрофон MacBook Pro
settings_file: /Users/user/Library/Application Support/LocalFlow/config/settings.json (present)
whisper_ready: whisper-base
wer_identity: PASS
vad_snr_15db: PASS
settings_json: PASS
single_instance: PASS
offline: PASS (checker uses no network)
```

## Devices (C-01)
```
Микрофон MacBook Pro (default)
Микрофон (iPhone 13)
```

## K paste smoke (ClipboardInjector → TextEdit only)
- **SKIP** could not open TextEdit document: Command '['osascript', '-e', '\ntell application "TextEdit"\n  activate\n  make new document with properties {text:""}\nend tell\n']' timed out after 8 seconds
- K-21 2000-char paste into Telegram/Safari/Cursor: **not run** (would steal focus)

## P-01 / P-02 network during check
- established TCP (non-loopback) before check: 101
- after check: 101
- P-01 (checker): no download; not a full record→insert airplane-mode run
- P-02: socket count is not byte volume; packet capture still **human**

## F-01 say → transcribe (needs Whisper on disk + Russian voice)
- voice: Milena  model: whisper-base

| Reference | Hypothesis | WER |
| --- | --- | --- |
| Привет это проверка диктовки | Привет и это проверка диктовки. | 0.5000 |
| Открой файл настроек | Открой файл на стройк. | 0.6667 |
| Собери проект без сети | Собери проехать без сети. | 0.5000 |
| Вставь текст в заметки | Вставьте их в ду заметке. | 1.0000 |
| Один два три четыре пять | 1, 2, 3, 4, 5. | 1.0000 |

- mean WER on 5 TTS phrases: **0.7333** (TTS≠mic; F-01 still needs live speech)

## Not run here (need a person at the Mac)

| ID | Why |
| --- | --- |
| D-07 | fullscreen of another app |
| D-08 | global hotkey while LocalFlow unfocused — start `npm run tauri dev` |
| K-16…K-21 | Telegram, Safari, Cursor |
| AM-05 | tail of a live utterance |
| AM-12 | Bluetooth headset |
| P-01/P-02 full | airplane mode + packet capture of record→insert |
| F-01 live | microphone, not `say` |
| A-17 CI | this host is recorded above; GitHub Actions is still macos-13 |
