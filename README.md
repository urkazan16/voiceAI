# LocalFlow

LocalFlow is a desktop dictation app: you hold a hotkey, speak, and the transcript is pasted into the app that has focus. Recognition, formatting, and insertion run on this machine. There is no cloud account. Inserted text is never executed.

It targets people who dictate into editors, browsers, messengers, and IDEs — including mixed Russian/English and technical identifiers (commits, GUIDs, file names, URLs).

LocalFlow — десктопное приложение для диктовки: удерживаете горячую клавишу, говорите, и текст вставляется в то приложение, которое в фокусе. Распознавание, форматирование и вставка выполняются на этой машине. Облачного аккаунта нет. Вставленный текст никогда не выполняется.

Рассчитано на тех, кто диктует в редакторы, браузеры, мессенджеры и IDE — в том числе смешанную русско-английскую речь и технические идентификаторы (коммиты, GUID, имена файлов, URL).

```text
1. Capture   / Запись     — hold Control+Shift+Space; mic on only while held
                            удерживайте Control+Shift+Space; микрофон только на время удержания
2. Recognize / Распознать — local Whisper ggml (SHA-256 + format check before use)
                            локальный Whisper ggml (SHA-256 и проверка формата до загрузки)
3. Format    / Оформить   — dictionary, snippets, backtrack, digits / dates / HH:MM, optional LLM
                            словарь, сниппеты, откат, цифры / даты / ЧЧ:ММ, опциональная локальная LLM
4. Insert    / Вставить   — paste into the frontmost app, then restore the previous clipboard
                            вставка в активное приложение, затем восстановление буфера
```

License / Лицензия: MIT. See `LICENSE`.

## What it does / Что умеет

- **Hold-to-talk / Удержание клавиши** — default `Control+Shift+Space`. Hands-free is press-to-start / press-to-stop. Escape cancels. По умолчанию `Control+Shift+Space`. Режим hands-free — нажал, чтобы начать / нажал, чтобы остановить. Escape отменяет текущую фразу.
- **Local speech-to-text / Локальное распознавание** — Whisper.cpp on CPU. Language: Russian, English, or auto-detect. Default model is Whisper Medium (~1.5 GB), downloaded on first launch and verified before use. Whisper.cpp на CPU. Язык: русский, английский или автоопределение. Модель по умолчанию — Whisper Medium (~1.5 ГБ), скачивается при первом запуске и проверяется перед использованием.
- **Insert into other apps / Вставка в другие приложения** — synthetic paste (`Cmd+V` on macOS, `Ctrl+V` elsewhere). Previous clipboard (text, RTF, images on macOS) is restored. Password fields are skipped. Синтетическая вставка (`Cmd+V` на macOS, `Ctrl+V` на других ОС). Предыдущий буфер восстанавливается. Поля пароля пропускаются; «Копировать последнее» / «Вставить последнее» работают после выхода из поля.
- **Copy last, paste last, edit selection / Копировать, вставить, править** — extra hotkeys (`Cmd+Ctrl+C/V/E` on macOS, `Ctrl+Alt+C/V/E` elsewhere). Edit re-runs hold-to-talk and replaces the selection. Дополнительные горячие клавиши. «Править» снова запускает диктовку и заменяет выделенный текст.
- **Dictionary / Словарь** — canonical terms, aliases, and spoken → written rules. Built-in developer vocabulary is seeded. Terms are also passed to Whisper as a prompt. Канонические термины, синонимы и правила «как сказано → как пишется». Вшит базовый словарь разработчика. Термины также уходят в Whisper как подсказка декодеру.
- **Snippets / Сниппеты** — exact triggers expand before the LLM (command → snippet → dictionary). Точные триггеры раскрываются до LLM (команда → сниппет → словарь).
- **Profiles / Профили** — per-app styles: `raw`, `normal`, `professional`, `code`. Стили по приложению; активное окно выбирает профиль, его можно переопределить.
- **Personalization / Персонализация** — repeated corrections become suggestions; accepting one writes a dictionary rule. Повторяющиеся правки становятся подсказками; принятие записывает правило в словарь.
- **Technical dictation / Техническая диктовка** — hashes, GUIDs, domains, file names, versions, and spell-out (`по буквам` / `air bat cap`). See [Dictating technical text](#dictating-technical-text--техническая-диктовка). Хэши, GUID, домены, имена файлов, версии и режим по буквам.
- **Numbers and dates / Числа и даты** — spoken numbers as digits, DMY or ISO dates, clock times, list and punctuation voice commands, “scratch that”. Числа цифрами, даты DMY или ISO, время, голосовые знаки препинания, откат «зачеркни».
- **Repeat / Повтор** — re-runs the last recording through the current speech model. Повтор последней записи через текущую модель (опциональный WAV).
- **Model Manager / Менеджер моделей** — download, checksum, activate, or delete Whisper and optional Qwen models. Dictation works without an LLM. Скачать, проверить, активировать или удалить модели. Диктовка работает без LLM.
- **History and journal / История и журнал** — SQLite history plus a JSONL utterance log; export, retry, turn a replica into a snippet. История SQLite и журнал JSONL; экспорт, повтор, сниппет из реплики.
- **Tray and LocalFlow Bar / Трей и панель** — idle / recording / processing in the menu bar; optional floating bar; sound cues; launch at login. Индикаторы в строке меню, плавающая панель при записи, звуковые сигналы, автозапуск.
- **Privacy / Конфиденциальность** — audio, STT, dictionary, personalization, and history stay on disk. Network is only for user-initiated model download (and optional updates). Uninstall from Privacy or `scripts/uninstall.sh` / `scripts/uninstall.ps1`. Аудио, распознавание, словарь, персонализация и история остаются на диске. Сеть — только для скачивания моделей по действию пользователя (и опциональных обновлений). Удаление — с экрана «Конфиденциальность» или скриптами.
- **Headless CLI** — transcribe a file, a directory, or stdin without opening the window. Транскрибация файла, каталога или stdin без окна.

UI language is English or Russian. / Язык интерфейса — английский или русский. Stack: Tauri 2 + Rust + React. This build is CPU-only. / Эта сборка только на CPU.

## Download / Скачать

Ready-made installers from the [latest GitHub Release](https://github.com/urkazan16/voiceAI/releases/latest). Speech models are **not** inside the installer; LocalFlow downloads Whisper Medium (~1.5 GB) on first launch.

Готовые установщики из [последнего GitHub Release](https://github.com/urkazan16/voiceAI/releases/latest). Моделей в установщике **нет**: Whisper Medium (~1.5 ГБ) скачивается при первом запуске.

| Platform / Платформа               | File / Файл                                                                                                                |
| ---------------------------------- | -------------------------------------------------------------------------------------------------------------------------- |
| macOS Apple Silicon (M1 and later) | [LocalFlow-macos-arm64.dmg](https://github.com/urkazan16/voiceAI/releases/latest/download/LocalFlow-macos-arm64.dmg)       |
| macOS Intel                        | [LocalFlow-macos-x64.dmg](https://github.com/urkazan16/voiceAI/releases/latest/download/LocalFlow-macos-x64.dmg)           |
| Windows 10 / 11                    | [LocalFlow-windows-x64.exe](https://github.com/urkazan16/voiceAI/releases/latest/download/LocalFlow-windows-x64.exe)       |
| Linux Debian / Ubuntu              | [LocalFlow-linux-x64.deb](https://github.com/urkazan16/voiceAI/releases/latest/download/LocalFlow-linux-x64.deb)           |
| Linux AppImage                     | [LocalFlow-linux-x64.AppImage](https://github.com/urkazan16/voiceAI/releases/latest/download/LocalFlow-linux-x64.AppImage) |
| Checksums                          | [SHA256SUMS](https://github.com/urkazan16/voiceAI/releases/latest/download/SHA256SUMS)                                     |

All versions / Все версии: [github.com/urkazan16/voiceAI/releases](https://github.com/urkazan16/voiceAI/releases)

After installing on macOS, enable **System Settings → Privacy & Security → Accessibility** and **Microphone** for LocalFlow, or dictated text stays on the clipboard.  
После установки на macOS включите **Системные настройки → Конфиденциальность и безопасность → Универсальный доступ** и **Микрофон**, иначе текст останется в буфере.

## Install from source / Сборка из исходников

macOS / Linux:

```text
./install.sh
npm run tauri dev
```

Windows (PowerShell):

```text
.\install.ps1
npm run tauri dev
```

Hold **Control+Shift+Space**, speak, release.  
Удерживайте **Control+Shift+Space**, говорите, отпустите.

| Host    | Avoid these system shortcuts / Не занимайте            | Paste / Вставка                                           |
| ------- | ------------------------------------------------------ | --------------------------------------------------------- |
| macOS   | Option+Space, Control+Space (Spotlight / input source) | Cmd+V                                                     |
| Windows | Win+Space (input language)                             | Ctrl+V                                                    |
| Linux   | Super+Space (desktop layout switcher)                  | Ctrl+V; on Wayland press it if automatic paste is blocked |

Packaged builds are in [Download](#download--скачать): `.dmg` (macOS), NSIS `.exe` (Windows, current user), `.deb` and AppImage (Linux).  
Готовые сборки — в [Скачать](#download--скачать).

One-file uninstall: `scripts/uninstall.sh` on macOS/Linux, `scripts/uninstall.ps1` on Windows (asks whether to keep history). The Privacy screen has the same Uninstall button.  
Удаление одним скриптом: `scripts/uninstall.sh` на macOS/Linux, `scripts/uninstall.ps1` на Windows (спросит, оставлять ли историю). Та же кнопка есть на экране «Конфиденциальность».

## Prerequisites (minimum versions)

| Tool         | Minimum                        |
| ------------ | ------------------------------ |
| Node.js      | 20.19.0 (see `.nvmrc`)         |
| npm          | 10                             |
| Rust / Cargo | 1.88.0 (`rust-toolchain.toml`) |
| Git          | 2.30                           |

Tauri CLI is installed via `npm install` (`@tauri-apps/cli@2.2.7`). Do not use a globally installed `latest` CLI.

After installing Rust, add Cargo to your shell (or open a new terminal):

```bash
source "$HOME/.cargo/env"
```

`npm run check` and `npm run tauri` also look in `~/.cargo/bin` so they work if rustup is installed but not sourced.

### macOS

| Tool                     | Minimum            |
| ------------------------ | ------------------ |
| macOS                    | 12                 |
| CMake                    | 3.16               |
| Xcode Command Line Tools | current for the OS |

Microphone / Accessibility strings live in `src-tauri/Info.plist` (merged by Tauri). Do not put `infoPlist` under `bundle.macOS` — CLI 2.2 rejects that key.

A packaged `.app` is a new TCC identity. Enable **System Settings → Privacy & Security → Accessibility** for LocalFlow, or paste stays on the clipboard and never reaches the focused field.  
Установленный `.app` — новая запись TCC. Включите **Системные настройки → Конфиденциальность и безопасность → Универсальный доступ** для LocalFlow, иначе текст останется в буфере и не попадёт в поле.

### Windows

| Tool                      | Minimum                                          |
| ------------------------- | ------------------------------------------------ |
| Windows                   | 10 / 11                                          |
| Visual Studio Build Tools | 2022, with the C++ workload (whisper.cpp / `cc`) |
| WebView2 Runtime          | Evergreen (Windows 11 includes it)               |

NSIS installers are per-user (`%LOCALAPPDATA%`). Grant LocalFlow the microphone in Windows Settings → Privacy & security → Microphone.  
Разрешите микрофон: Параметры → Конфиденциальность и безопасность → Микрофон.

### Linux (Ubuntu 22.04+)

Build packages:

```bash
sudo apt-get install --no-install-recommends -y \
  build-essential cmake libasound2-dev libayatana-appindicator3-dev \
  libgtk-3-dev librsvg2-dev libssl-dev libwebkit2gtk-4.1-dev libclang-dev patchelf
```

Runtime helpers for paste: `xclip` on X11, `wl-clipboard` on Wayland. Automatic paste into other windows works on X11 (XTEST). On Wayland, LocalFlow copies the transcript and you press Ctrl+V if the compositor blocks synthetic keys.  
Для вставки: `xclip` на X11, `wl-clipboard` на Wayland. Автоматическая вставка в другие окна работает на X11 (XTEST). На Wayland текст копируется в буфер — нажмите Ctrl+V, если композитор блокирует синтетические клавиши.

There are no secret environment variables and no absolute developer paths in the build.

## Commands

| Command                    | What it does                                                                                  |
| -------------------------- | --------------------------------------------------------------------------------------------- |
| `npm install`              | Install JS dependencies from `package-lock.json`                                              |
| `npm run check:gate`       | Same as CI **quality** + **license** + **security** (tsc, ESLint, Prettier, licenses, audit)  |
| `npm run check`            | `check:gate` plus `cargo check`, `cargo fmt`, Clippy                                          |
| `npm test`                 | Frontend + Rust unit + integration tests                                                      |
| `npm run test:all`         | Unit, integration, UI, pipeline, dictionary, personalization                                  |
| `npm run test:ai`          | AI benchmark profile (requires catalog + optional local models)                               |
| `npm run build`            | Frontend production bundle + debug Rust binary                                                |
| `npm run build:release`    | Checks UI, builds Rust, packages host installers (dmg/app, nsis, deb/AppImage), SBOM, SHA-256 |
| `npm run check:local`      | Offline checker (WER + VAD SNR 15 dB), no network                                             |
| `npm run license:check`    | Dependency license allowlist                                                                  |
| `npm run uniqueness:check` | Confirms `docs/evaluation/UNIQUENESS.md` is attached                                          |

Headless CLI (no window):

```bash
cargo run --manifest-path src-tauri/Cargo.toml -- --help
cargo run --manifest-path src-tauri/Cargo.toml -- check
cargo run --manifest-path src-tauri/Cargo.toml -- transcribe --json --language ru speech.wav
cargo run --manifest-path src-tauri/Cargo.toml -- transcribe --dir ./clips --no-postprocess
ffmpeg -f avfoundation -i ":0" -t 3 -f wav - | cargo run --manifest-path src-tauri/Cargo.toml -- transcribe --stdin
```

## Dictating technical text / Техническая диктовка

Identifiers are rebuilt deterministically, before punctuation is applied, so the
word "точка" holding a name together does not become a full stop.

Идентификаторы собираются детерминированно, до расстановки пунктуации: слово
«точка» внутри имени не превращается в конец предложения.

| Say / Сказать                                   | Get / Получить                         |
| ----------------------------------------------- | -------------------------------------- |
| `гуид четыре три шесть а … дефис це а семь и …` | `436a2969-ca7e-47ab-b0f3-72a534d744b6` |
| `коммит пять три це три девять шесть три`       | `53c3963`                              |
| `открой эльма 365 точка ком`                    | `открой elma365.com`                   |
| `запусти скрипт точка sh`                       | `запусти скрипт.sh`                    |
| `версия два точка ноль точка один`              | `2.0.1`                                |
| `установи дот нет фреймворк`                    | `.NET Framework`                       |
| `по буквам air bat cap` / `по буквам эй би си`  | `abc`                                  |

- Say `коммит`, `хеш`, `гуид`, `uuid`, or `id` before a hash to have the characters
  joined; 32 hexadecimal characters are regrouped as `8-4-4-4-12`. A GUID-shaped
  run needs no lead-in word. Произнесите `коммит`, `хеш`, `гуид`, `uuid` или `id`
  перед хэшем — символы склеятся; 32 шестнадцатеричных символа группируются как
  `8-4-4-4-12`. GUID такой формы можно диктовать без вводного слова.
- Say `по буквам` to spell anything else out. One-syllable code words (`air bat cap
drum each…`, or Russian `аз цап дэт ель…`) are preferred in a stream; letter
  names and `дефис` / `точка` / `слэш` / `подчёркивание` still work. The run stays
  open across push-to-talk presses until `конец` or ordinary speech. Режим «по
  буквам» остаётся открытым между нажатиями PTT, пока не скажете `конец` или
  обычную фразу.
- A GUID can be dictated in `8-4-4-4-12` groups. After the first group, say `дефис`
  and the next group: it is appended without a space. A finished 7-character
  commit hash is not continued, so "коммит …" then "и проверь" stays two phrases.
  GUID можно диктовать группами `8-4-4-4-12`: после первой скажите `дефис` и
  следующую — она допишется без пробела. Законченный 7-символьный хэш коммита
  не продолжается.
- A run of digits alone stays a number, so "коммит 2024 года" is left as spoken.
  Одна цепочка цифр остаётся числом: «коммит 2024 года» не склеивается в хэш.
- `.NET` is a dictionary term rather than a spoken-dot rule, because "нет" is a
  Russian word and gluing it to a dot would corrupt ordinary speech. `.NET` —
  словарный термин, а не правило «точка + нет»: иначе обычная речь ломалась бы.

Dictionary terms are also fed to Whisper as a decoding prompt, so the recognizer
is biased towards your project vocabulary instead of guessing phonetically. In
`code` mode symbol suppression is lifted so `/`, `_`, and `#` can be dictated.

Settings live in `config/settings.json` under the data root (JSON). Edits apply within a couple of seconds without rebuilding. Schema of the replica journal: `docs/journal/UTTERANCE.md`.

| Host    | Data root                                                       |
| ------- | --------------------------------------------------------------- |
| macOS   | `~/Library/Application Support/LocalFlow/`                      |
| Windows | `%APPDATA%\LocalFlow\`                                          |
| Linux   | `~/.local/share/LocalFlow/` (`$XDG_DATA_HOME/LocalFlow` if set) |

## Settings / Настройки

| Key                                     | Meaning                                                       |
| --------------------------------------- | ------------------------------------------------------------- |
| `hotkey`                                | Push-to-talk shortcut                                         |
| `microphone_name`                       | Input device, or `null` for the OS default                    |
| `active_stt_model` / `active_llm_model` | Catalog ids (see Model Manager)                               |
| `stt_language`                          | `ru`, `en`, or `auto`                                         |
| `mode`                                  | Fallback pipeline: `raw` / `normal` / `professional` / `code` |
| `autostart`                             | Launch at login                                               |
| `history_enabled`                       | SQLite history + JSONL journal                                |
| `sound_cues`                            | Start/end beeps                                               |
| `sound_cue_volume`                      | Cue loudness 0.05–1.0 (default 0.25)                          |
| `insert_delay_ms`                       | Pause before paste                                            |
| `hands_free`                            | Press-to-toggle listen; off = hold-to-talk                    |
| `digits_from_speech`                    | Spoken numbers become digits                                  |
| `date_format`                           | `DMY` (DD.MM.YYYY) or `ISO`                                   |
| `compute_device`                        | Inference device; this build is CPU only                      |
| `postprocess_timeout_ms`                | Cap on formatting                                             |
| `restore_clipboard`                     | Restore clipboard after paste                                 |
| `vad_threshold`                         | Silence trim sensitivity (default 0.012)                      |
| `history_max_items`                     | SQLite history rotation cap (default 500)                     |
| `log_max_bytes`                         | Size rotation for `localflow.log`                             |

Replace the recognizer by downloading another Whisper ggml in Model Manager, or set `active_stt_model` in `settings.json` to a catalog id whose file is already verified.

`npm run sbom` writes a CycloneDX SBOM.

First Cargo fetch needs network. After `src-tauri/Cargo.lock` is present, crates resolve reproducibly.

## Models / Модели

Weights are **not** inside the app bundle. On `./install.sh` / `.\install.ps1` and on first GUI launch LocalFlow downloads the **active speech model** (default Whisper Medium, ~1.5 GB, `ggml-medium.bin`) from Hugging Face, then verifies SHA-256 and ggml magic before activation. Qwen formatting models stay optional in Model Manager.

Веса **не** лежат в бандле. При `./install.sh` / `.\install.ps1` и при первом запуске GUI LocalFlow скачивает **активную речевую модель** (по умолчанию Whisper Medium, ~1.5 ГБ, `ggml-medium.bin`) с Hugging Face, затем проверяет SHA-256 и ggml magic. Модели оформления Qwen остаются опциональными в Менеджере моделей.

Skip the network step with `LOCALFLOW_SKIP_MODEL_DOWNLOAD=1`. Retry anytime: `npm run download:stt` or `localflow download --model whisper-medium`.

Before a model is used:

1. SHA-256 verification
2. Format validation (GGUF / ggml)
3. Activation

Mismatch raises `MODEL_CHECKSUM_MISMATCH` and the model is not loaded.

User data lives in the data root in the table above (override with `LOCALFLOW_DATA_DIR` for tests).

## License / Лицензия

MIT. See `LICENSE`, `NOTICE`, `licenses/`, and `docs/licensing/`.

Uniqueness report (attached to this tree): `docs/evaluation/UNIQUENESS.md`.
