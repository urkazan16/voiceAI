# LocalFlow — Private Offline Voice Typing for Windows, macOS & Linux

[![ci](https://github.com/urkazan16/voiceAI/actions/workflows/ci.yml/badge.svg)](https://github.com/urkazan16/voiceAI/actions/workflows/ci.yml)
[![license](https://img.shields.io/badge/license-MIT-blue)](LICENSE)

**English** · [Русский](README.ru.md)

LocalFlow is an open-source, private voice typing and local speech-to-text app for Windows, macOS, and Linux. Hold a global hotkey, speak, release it, and LocalFlow transcribes your voice and pastes the text into the active app — browser, editor, messenger, terminal, or IDE.

Speech recognition, text formatting, and clipboard insertion run on your computer. After downloading a speech recognition model and any optional formatting models, dictation works offline: no account, cloud transcription, or audio upload is required. LocalFlow supports Russian and English dictation, mixed technical speech, code identifiers, commit hashes, GUIDs, file names, and URLs.

## Private voice typing features

- **Offline speech-to-text** after the first model download, with local Whisper recognition
- **Dictate into any app** using a configurable global push-to-talk hotkey
- **Private by design**: microphone capture only while dictating; no cloud account or audio upload
- **Russian and English voice typing**, including technical terms and mixed-language phrases
- **Windows 10/11 x64, macOS 12+ (Apple Silicon and Intel), and Ubuntu 22.04+ x64** installers
- **Local text formatting** with dictionaries, snippets, personalization, and optional local LLMs

## Supported platforms

| Platform                    | Installer                      | Notes                                                                   |
| --------------------------- | ------------------------------ | ----------------------------------------------------------------------- |
| Windows 10 / 11 (x64)       | NSIS `.exe`                    | WebView2 Evergreen required (included with Windows 11)                  |
| macOS 12+                   | Apple Silicon and Intel `.dmg` | Microphone and Accessibility permissions required                       |
| Ubuntu 22.04+ / Linux (x64) | `.deb` and AppImage            | X11 supports automatic paste; Wayland support depends on the compositor |

## Download and install LocalFlow

Download an installer from the [latest GitHub Release](https://github.com/urkazan16/voiceAI/releases/latest). Models are downloaded separately: the default selection is Whisper Medium Q8_0 (about 820 MB) and Qwen3 4B Instruct 2507 (about 2.5 GB). The local language model is optional for basic dictation.

| Platform                           | File                                                                                                                       |
| ---------------------------------- | -------------------------------------------------------------------------------------------------------------------------- |
| macOS Apple Silicon (M1 and later) | [LocalFlow-macos-arm64.dmg](https://github.com/urkazan16/voiceAI/releases/latest/download/LocalFlow-macos-arm64.dmg)       |
| macOS Intel                        | [LocalFlow-macos-x64.dmg](https://github.com/urkazan16/voiceAI/releases/latest/download/LocalFlow-macos-x64.dmg)           |
| Windows 10 / 11                    | [LocalFlow-windows-x64.exe](https://github.com/urkazan16/voiceAI/releases/latest/download/LocalFlow-windows-x64.exe)       |
| Linux Debian / Ubuntu              | [LocalFlow-linux-x64.deb](https://github.com/urkazan16/voiceAI/releases/latest/download/LocalFlow-linux-x64.deb)           |
| Linux AppImage                     | [LocalFlow-linux-x64.AppImage](https://github.com/urkazan16/voiceAI/releases/latest/download/LocalFlow-linux-x64.AppImage) |
| Checksums                          | [SHA256SUMS](https://github.com/urkazan16/voiceAI/releases/latest/download/SHA256SUMS)                                     |

All versions: [github.com/urkazan16/voiceAI/releases](https://github.com/urkazan16/voiceAI/releases)

### System requirements and permissions

- **macOS:** macOS 12 or later. Grant Microphone and Accessibility access in System Settings so LocalFlow can record and paste. Apple Silicon builds support Metal acceleration; Intel builds use the CPU.
- **Windows:** Windows 10 or 11, x64, with WebView2 Evergreen. Allow microphone access in Windows privacy settings.
- **Linux:** Ubuntu 22.04 or later is the CI build environment. Clipboard integration uses `xclip` on X11 or `wl-clipboard` on Wayland. Other distributions may require additional system packages; on Wayland, manual paste may be necessary.
- **Model storage:** reserve space for your selected speech model and optional language model. Models are not bundled in the installer; download them before using LocalFlow offline.

For macOS Gatekeeper troubleshooting, Linux build dependencies, and uninstall instructions, see the [detailed installation guide in Russian](README.ru.md#установка).

## Start dictating into any app

1. Install LocalFlow and complete the initial model download.
2. Grant the operating system permissions listed above.
3. Choose Russian (`ru`), English (`en`), or automatic language selection (`auto`) in Settings. The default recognition language is Russian.
4. Focus a text field, hold **Control+Shift+Space**, speak, and release. LocalFlow transcribes and pastes your text.

Enable **hands-free** mode to press once to start and again to stop. Press **Escape** to cancel. You can change the global hotkey if another application uses it. Password fields and Secure Input are skipped.

## How local speech-to-text works

1. **Record:** the microphone stream opens during dictation and closes when recording stops or is cancelled. Each recording is limited to 20 minutes.
2. **Transcribe:** local Whisper recognition converts speech into text. Downloaded model files are checked before use.
3. **Format:** dictionaries, snippets, personalization, and technical dictation rules process the text. Optional local Qwen models provide additional formatting.
4. **Insert:** LocalFlow pastes into the active text field with `Cmd+V` or `Ctrl+V` and can restore the previous clipboard contents. Inserted text is never executed automatically.

The tray shows recording and processing status. The optional LocalFlow Bar displays the audio level and pipeline progress. The main window contains settings, dictionaries, snippets, history, model management, and privacy controls.

See the [architecture overview](docs/architecture/OVERVIEW.md) for implementation details.

## Voice typing for developers and bilingual users

Use local speech recognition to draft messages, write documentation, and dictate into an editor or IDE. Add project terminology to the dictionary and reusable phrases to snippets. Technical dictation rules handle spoken URLs, file names, versions, commit hashes, and GUIDs without requiring an LLM.

For example, Russian dictation `версия два точка ноль точка один` produces `2.0.1`, and `коммит пять три це три девять шесть три` produces `53c3963`. See the [technical dictation examples](README.ru.md#оригинальная-фича) for spelling and identifier rules.

You can also select text and dictate its replacement using the Edit shortcut: `Cmd+Ctrl+E` on macOS or `Ctrl+Alt+E` on Windows and Linux.

## Privacy and offline dictation

Audio capture, speech recognition, formatting, dictionaries, personalization, and history run locally. No cloud transcription account is required. Network access is used for model downloads and optional application updates.

Local storage still matters: history is enabled by default, and the last audio recording is retained by default for retry. Manage these options in Settings and use the privacy controls to clear history or reset personalization. The microphone remains open during a hands-free recording until you stop it.

Read the [privacy overview](docs/privacy/OVERVIEW.md) and [settings reference in Russian](README.ru.md#настройки).

## Frequently asked questions

### Does LocalFlow work as offline voice typing software?

Yes. LocalFlow downloads the selected speech and optional formatting models on first use. After that, voice recognition, dictation, formatting, and insertion run locally without a cloud transcription service.

### Can I dictate text into browsers, messengers, editors, and IDEs?

Yes. Press and hold the global hotkey, dictate, and release it. LocalFlow pastes the transcribed text into the currently active text field. On Linux Wayland, the compositor may require you to paste from the clipboard manually.

### Does LocalFlow send microphone audio or dictated text to the cloud?

No. The microphone is opened only while recording, and speech recognition is performed locally. Network access is used only for user-initiated model downloads and optional application updates.

### Which languages does LocalFlow support for voice dictation?

LocalFlow supports Russian, English, and automatic language selection. Use `auto` to switch languages; full sentences in different languages within one utterance may be unreliable. It also has deterministic formatting for technical dictation, including URLs, file names, commit hashes, GUIDs, and common programming terms.

### Do I need a local LLM for voice typing?

No. The `raw` and `normal` modes work without an LLM. Dictionaries, snippets, personalization, and deterministic formatting rules also work without one. The `professional` and `code` styles can use a downloaded local language model.

### Does LocalFlow work on Apple Silicon and Intel Macs?

The project builds separate installers for Apple Silicon and Intel Macs running macOS 12 or later. Metal acceleration is enabled for Apple Silicon builds; Intel builds use the CPU.

## Build LocalFlow from source

The project uses Tauri 2, Rust, and React. Install Node.js 20.19 or later within the supported Node 20–22 range, npm 10 or later, Rust 1.88, Git 2.30 or later, and the platform build dependencies in the [development setup guide](docs/development/SETUP.md).

macOS / Linux:

```sh
./install.sh
npm run tauri dev
```

Windows PowerShell:

```powershell
.\install.ps1
npm run tauri dev
```

The installation script downloads the speech model. On macOS or Linux, use `LOCALFLOW_SKIP_MODEL_DOWNLOAD=1 ./install.sh` to skip that download and install a model later. Use the pinned Tauri CLI through `npm run tauri`.

### Command-line transcription

```sh
cargo run --manifest-path src-tauri/Cargo.toml -- --help
cargo run --manifest-path src-tauri/Cargo.toml -- devices
cargo run --manifest-path src-tauri/Cargo.toml -- transcribe --json --language ru speech.wav
cargo run --manifest-path src-tauri/Cargo.toml -- transcribe --dir ./clips --no-postprocess
```

### Development checks

```sh
npm test                 # frontend, Rust unit and integration tests
npm run test:all         # additional UI and pipeline checks
npm run check            # project gate, cargo check, fmt and clippy
npm run check:local      # local offline evaluator
```

See the [detailed Russian guide](README.ru.md) for configuration, model selection, technical dictation, and additional CLI examples.

## Contributing

Read [CONTRIBUTING.md](CONTRIBUTING.md) and the [development setup](docs/development/SETUP.md). Repository maintainers can find prepared GitHub About text and topics in the [repository discovery guide](.github/DISCOVERABILITY.md).

## License

LocalFlow is released under the [MIT License](LICENSE). See [NOTICE](NOTICE) and the [licensing policy](docs/licensing/POLICY.md) for third-party components and separately licensed model weights.
