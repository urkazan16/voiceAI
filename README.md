# LocalFlow

[![ci](https://github.com/urkazan16/voiceAI/actions/workflows/ci.yml/badge.svg)](https://github.com/urkazan16/voiceAI/actions/workflows/ci.yml)
[![license](https://img.shields.io/badge/license-MIT-blue)](LICENSE)

Открытый системный голосовой ввод: зажал клавишу, сказал, отпустил — текст появился в активном поле любого приложения. Распознавание, оформление и вставка идут на вашем компьютере. Облачного аккаунта нет. Вставленный текст никогда не выполняется.

Рассчитано на тех, кто диктует в редакторы, браузеры, мессенджеры и IDE — в том числе смешанную русско-английскую речь и технические идентификаторы (коммиты, GUID, имена файлов, URL).

> [!NOTE]
> Версия `0.1.0`. Стек: Tauri 2 + Rust + React. Язык интерфейса — английский или русский. Чем проект отличается от аналогов — [docs/evaluation/UNIQUENESS.md](docs/evaluation/UNIQUENESS.md)

## Как это работает

- **Захват** — микрофон через `cpal`; поток открывается только на время записи и закрывается после отпускания, короткого удержания или отмены. Звук приводится к моно 16 кГц. Одна реплика не длиннее 120 с
- **Распознавание** — `whisper.cpp` локально (`whisper-rs`). На Apple Silicon можно Metal; Intel, Windows и Linux считают на CPU. Перед загрузкой весов проверяются SHA-256 и формат ggml
- **Постобработка** — словарь терминов, сниппеты, персонализация, числа и даты, техническая диктовка — без модели. По желанию локальная LLM (Qwen, GGUF) из менеджера моделей
- **Вставка** — синтетическая вставка (`Cmd+V` на macOS, `Ctrl+V` на Windows и Linux) с восстановлением прежнего буфера (текст, RTF, изображения на macOS). Поля пароля и Secure Input пропускаются

Подробнее — [docs/architecture/OVERVIEW.md](docs/architecture/OVERVIEW.md), схема журнала реплик — [docs/journal/UTTERANCE.md](docs/journal/UTTERANCE.md), приватность — [docs/privacy/OVERVIEW.md](docs/privacy/OVERVIEW.md)

## Как это выглядит

В трее — состояние: покой, запись, обработка. Плавающая панель LocalFlow Bar (включается настройкой) показывает уровень сигнала и текущий шаг пайплайна. Главное окно — настройки, словарь, сниппеты, история, менеджер моделей и экран конфиденциальности. История читает ту же SQLite-базу и JSONL-журнал, что и экспорт из приложения.

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

## Установка

Готовые сборки — в [Download / Скачать](#download--скачать). Веса Whisper в установщик не входят: при первом запуске скачивается Whisper Medium (~1.5 ГБ) с Hugging Face, затем проверяются SHA-256 и magic ggml.

### macOS и Gatekeeper

Неподписанный `.dmg` вызывает «не удалось подтвердить, что LocalFlow не содержит вредоносного ПО». Это Gatekeeper, не вирус. Новые job `package` на `main` подписывают Developer ID и нотаризуют; после зелёного прогона скачайте **новый** `.dmg`.

Уже скачанный файл открывается через **Открыть всё равно**:

1. **Системные настройки → Конфиденциальность и безопасность** (вниз страницы) → **Открыть всё равно**
2. Подтвердите паролем или Touch ID
3. Или в Терминале:

```bash
xattr -cr /Applications/LocalFlow.app
open /Applications/LocalFlow.app
```

Затем включите **Универсальный доступ** и **Микрофон** для LocalFlow, иначе текст останется в буфере и не попадёт в поле.

> [!NOTE]
> Нотаризация в CI читает секреты из GitHub Environment `APPLE_CERTIFICATE`, не из обычных Repository secrets. Значения сертификата в git и в этот файл не кладутся

### Из исходников

Минимум: Node.js 20.19 (`engines` / `.nvmrc`), npm 10, Rust 1.88 (`rust-toolchain.toml`), Git 2.30. Tauri CLI ставится через `npm install` (`@tauri-apps/cli@2.2.7`) — глобальный `latest` не используйте.

macOS / Linux:

```sh
./install.sh
npm run tauri dev
```

Windows (PowerShell):

```powershell
.\install.ps1
npm run tauri dev
```

`install.sh` ставит JS-зависимости и скачивает Whisper Medium с проверкой SHA-256. Без сети: `LOCALFLOW_SKIP_MODEL_DOWNLOAD=1 ./install.sh` — веса подтянутся при первом запуске GUI или командой `npm run download:stt`.

После установки Rust в новой оболочке:

```bash
source "$HOME/.cargo/env"
```

`npm run check` и `npm run tauri` смотрят и в `~/.cargo/bin`, если rustup есть, но не в `PATH`.

#### macOS

macOS 12+, CMake 3.16+, актуальные Xcode Command Line Tools. Строки микрофона и универсального доступа живут в `src-tauri/Info.plist` (их подмешивает Tauri). Не кладите `infoPlist` под `bundle.macOS` — CLI 2.2 этот ключ отвергает.

Установленный `.app` — новая запись TCC. Включите универсальный доступ, иначе вставка не дойдёт до поля.

#### Windows

Windows 10/11, Visual Studio Build Tools 2022 с рабочей нагрузкой C++ (whisper.cpp / `cc`), WebView2 Evergreen (на Windows 11 уже есть). Установщик NSIS — per-user (`%LOCALAPPDATA%`). Микрофон: Параметры → Конфиденциальность и безопасность → Микрофон.

#### Linux (Ubuntu 22.04+)

```bash
sudo apt-get install --no-install-recommends -y \
  build-essential cmake libasound2-dev libayatana-appindicator3-dev \
  libgtk-3-dev librsvg2-dev libssl-dev libwebkit2gtk-4.1-dev libclang-dev patchelf
```

Для вставки: `xclip` на X11, `wl-clipboard` на Wayland. Автоматическая вставка в чужие окна работает на X11 (XTEST). На Wayland текст копируется в буфер — нажмите Ctrl+V, если композитор блокирует синтетические клавиши.

### Удаление

Снимается то, что лежит в каталоге данных, плюс автозапуск. Модели, настройки и история уходят вместе, если не попросить оставить базу:

```sh
scripts/uninstall.sh                  # спросит про историю
scripts/uninstall.sh --keep-history   # оставить только базу
```

На Windows — `scripts/uninstall.ps1`. В GUI: Настройки или Приватность → **Удалить LocalFlow полностью**.

## Быстрый старт

Удерживайте **Control+Shift+Space**, говорите, отпустите. Hands-free — отдельный флажок в настройках: нажал, чтобы начать / нажал, чтобы остановить. Escape отменяет текущую фразу.

| Хост    | Не занимайте                                        | Вставка                                                       |
| ------- | --------------------------------------------------- | ------------------------------------------------------------- |
| macOS   | Option+Space, Control+Space (Spotlight / раскладка) | Cmd+V                                                         |
| Windows | Win+Space (язык ввода)                              | Ctrl+V                                                        |
| Linux   | Super+Space (переключатель раскладки)               | Ctrl+V; на Wayland нажмите сами, если синтетика заблокирована |

Дополнительные сочетания: копировать / вставить последнюю реплику и править выделение — `Cmd+Ctrl+C/V/E` на macOS, `Ctrl+Alt+C/V/E` на Windows и Linux. Если основная клавиша занята системой, приложение пробует запасную (`Command+Shift+D` / `Control+Shift+D`).

Headless CLI без окна:

```bash
cargo run --manifest-path src-tauri/Cargo.toml -- --help
cargo run --manifest-path src-tauri/Cargo.toml -- check
cargo run --manifest-path src-tauri/Cargo.toml -- devices
cargo run --manifest-path src-tauri/Cargo.toml -- download --model whisper-medium
cargo run --manifest-path src-tauri/Cargo.toml -- transcribe --json --language ru speech.wav
cargo run --manifest-path src-tauri/Cargo.toml -- transcribe --dir ./clips --no-postprocess
ffmpeg -f avfoundation -i ":0" -t 3 -f wav - | cargo run --manifest-path src-tauri/Cargo.toml -- transcribe --stdin
```

`check` гоняет локальный офлайн-проверщик (WER на фикстуре + VAD SNR 15 дБ) и не ходит в сеть. `paste-smoke` проверяет, что текст вообще доходит до поля. Код выхода CLI: `0` — получилось, иначе ошибка; прогресс в stderr, расшифровка в stdout.

## Приватность микрофона

Поток микрофона открывается в момент нажатия клавиши и закрывается сразу после отпускания, отмены или сбоя захвата: вне записи LocalFlow ничего не слушает. Hands-free держит поток, пока сессия не остановлена, но буфер ограничен `MAX_CAPTURE_SECS` (120 с), чтобы запись не росла без верхней границы.

Список устройств (`devices` / экран настроек) перечисляет Bluetooth и виртуальные источники **без** открытия потока.

Аудио, распознавание, словарь, персонализация и история остаются на диске. Сеть нужна только когда вы сами скачиваете модель (и опционально для обновления приложения). Оба случая подписаны в интерфейсе.

## Настройки

Файл `config/settings.json` в каталоге данных создаётся при первом запуске. Правка применяется без пересборки, за секунды. Неверное значение даёт ошибку с кодом `CONFIG_INVALID`.

| Хост    | Каталог данных                                                       |
| ------- | -------------------------------------------------------------------- |
| macOS   | `~/Library/Application Support/LocalFlow/`                           |
| Windows | `%APPDATA%\LocalFlow\`                                               |
| Linux   | `~/.local/share/LocalFlow/` (`$XDG_DATA_HOME/LocalFlow`, если задан) |

Для тестов каталог можно подменить `LOCALFLOW_DATA_DIR`.

| Ключ                      | По умолчанию                          | Что делает                                                               |
| ------------------------- | ------------------------------------- | ------------------------------------------------------------------------ |
| `hotkey`                  | `Control+Shift+Space`                 | удержание записи; синтаксис Tauri                                        |
| `copy_last_hotkey`        | `Command+Control+C` / `Control+Alt+C` | копировать последнюю реплику                                             |
| `paste_last_hotkey`       | `Command+Control+V` / `Control+Alt+V` | вставить последнюю реплику                                               |
| `edit_hotkey`             | `Command+Control+E` / `Control+Alt+E` | диктовка с заменой выделения                                             |
| `microphone_name`         | пусто                                 | имя из списка устройств; пусто — системный по умолчанию                  |
| `active_stt_model`        | `whisper-medium`                      | id из каталога моделей                                                   |
| `active_llm_model`        | пусто                                 | id LLM; пусто — диктовка без языковой модели                             |
| `stt_language`            | `ru`                                  | `ru`, `en` или `auto`                                                    |
| `mode`                    | `normal`                              | запасной стиль пайплайна: `raw` / `normal` / `professional` / `code`     |
| `profile_override`        | пусто                                 | зафиксировать профиль, не смотря на активное окно                        |
| `ui_language`             | `en`                                  | язык интерфейса: `en` или `ru`                                           |
| `hands_free`              | `false`                               | `true` — нажал/нажал, а не удерживай                                     |
| `restore_clipboard`       | `true`                                | вернуть прежний буфер после вставки                                      |
| `insert_delay_ms`         | `40`                                  | пауза перед вставкой (40…5000), чтобы фокус успел вернуться              |
| `postprocess_timeout_ms`  | `45000`                               | лимит оформления; не меньше 1000                                         |
| `sound_cues`              | `true`                                | звуковые метки начала и конца                                            |
| `sound_cue_volume`        | `0.25`                                | громкость меток, 0.05…1.0                                                |
| `autostart`               | `false`                               | запускать при входе в систему                                            |
| `history_enabled`         | `true`                                | SQLite-история и JSONL-журнал                                            |
| `history_max_items`       | `500`                                 | сколько реплик хранить (50…10000)                                        |
| `vad_threshold`           | `0.012`                               | чувствительность обрезки тишины                                          |
| `digits_from_speech`      | `true`                                | числительные цифрами                                                     |
| `date_format`             | `DMY`                                 | `DMY` (`ДД.ММ.ГГГГ`) или `ISO`                                           |
| `compute_device`          | `auto`                                | `auto` / `cpu` / `gpu` (`metal`). GPU есть только в сборке Apple Silicon |
| `keep_last_audio`         | `true`                                | сохранить последнюю реплику как WAV для «Повторить»                      |
| `show_flow_bar`           | `true`                                | плавающая панель во время записи                                         |
| `personalization_enabled` | `true`                                | учитывать правки                                                         |
| `learn_from_corrections`  | `true`                                | предлагать правила из повторяющихся правок                               |
| `onboarding_complete`     | `false`                               | мастер первого запуска пройден                                           |
| `log_max_bytes`           | `2 MiB`                               | ротация `localflow.log`                                                  |

Полный список полей — в `src-tauri/src/config.rs`. Профили приложений (`raw` / `normal` / `professional` / `code`) живут рядом и выбираются по активному окну.

## Замена модели распознавания

Модель — файл весов из каталога. Меняется в Менеджере моделей или ключом `active_stt_model`:

```bash
npm run download:stt
cargo run --manifest-path src-tauri/Cargo.toml -- download --model whisper-large-v3-turbo
```

`whisper-base` быстрее и заметно хуже на русском, `whisper-small` — черновик, `whisper-medium` — компромисс по умолчанию (~1.5 ГБ F16). На Intel-CPU удобнее `whisper-medium-q8_0` (~820 МБ). `whisper-large-v3-turbo` обычно быстрее Medium при том же порядке размера на диске.

Перед использованием:

1. SHA-256
2. Проверка формата (ggml / GGUF)
3. Активация

Несовпадение даёт `MODEL_CHECKSUM_MISMATCH`, модель не загружается. Qwen для оформления — отдельные записи каталога, диктовка без них работает. Каталог: `src-tauri/resources/model-catalog.json`. Веса в репозиторий не входят и лицензией приложения не покрываются — см. [docs/licensing/POLICY.md](docs/licensing/POLICY.md).

## Язык реплики

По умолчанию язык жёсткий: `stt_language = "ru"`. Это самый быстрый путь — модель не выбирает язык, и английская речь в этом режиме часто записывается по-русски.

Двуязычная диктовка:

```json
"stt_language": "auto"
```

Допустимы только `ru`, `en` и `auto`. Смешанная речь внутри одной реплики остаётся в одном прочтении: латинские вставки в русской фразе распознаются, целое английское предложение посреди русской реплики — ненадёжно. В режиме `code` снимается подавление символов Whisper, чтобы диктовать `/`, `_` и `#`.

Словарь уходит в Whisper как подсказка декодеру, поэтому проектные термины предпочтительнее фонетической догадки.

## Языковая модель

По умолчанию текст правят словарь, сниппеты, персонализация и правила. LLM выключена, пока в Менеджере моделей не активирован `active_llm_model`. Стили `professional` и `code` зовут модель; `raw` и `normal` обходятся без неё. Если модель не отвечает или не укладывается в `postprocess_timeout_ms`, в поле уходит текст после правил — реплика не теряется. `--no-postprocess` на CLI выключает оформление на один запуск.

Ключ облачного API в приложении не хранится: LLM считается локально из GGUF. Веса Qwen скачиваются по действию пользователя после показа лицензии.

## Работа без интернета

Сеть нужна, когда вы сами скачиваете веса с Hugging Face (и опционально для обновления). Всё остальное работает с выдернутым кабелем: захват, распознавание, словарь, правила, вставка, журнал, GUI. `localflow check` и автотесты в сеть не ходят. Без сети нельзя скачать новую модель; стили с LLM работают только если GGUF уже на диске.

## Оригинальная фича

**Техническая диктовка.** Идентификаторы собираются детерминированно, до расстановки пунктуации: слово «точка» внутри имени не превращается в конец предложения. Модель для этого не нужна.

| Сказать                                         | Получить                               |
| ----------------------------------------------- | -------------------------------------- |
| `гуид четыре три шесть а … дефис це а семь и …` | `436a2969-ca7e-47ab-b0f3-72a534d744b6` |
| `коммит пять три це три девять шесть три`       | `53c3963`                              |
| `открой эльма 365 точка ком`                    | `открой elma365.com`                   |
| `запусти скрипт точка sh`                       | `запусти скрипт.sh`                    |
| `версия два точка ноль точка один`              | `2.0.1`                                |
| `установи дот нет фреймворк`                    | `.NET Framework`                       |
| `по буквам air bat cap` / `по буквам эй би си`  | `abc`                                  |

- Произнесите `коммит`, `хеш`, `гуид`, `uuid` или `id` перед хэшем — символы склеятся; 32 шестнадцатеричных символа группируются как `8-4-4-4-12`. GUID такой формы можно диктовать без вводного слова
- `по буквам` открывает режим спеллинга между нажатиями PTT, пока не скажете `конец` или обычную фразу. Кодовые слова (`air bat cap…` или `аз цап дэт…`) устойчивее имён букв
- GUID можно диктовать группами: после первой скажите `дефис` и следующую — она допишется без пробела. Законченный 7-символьный хэш коммита не продолжается
- Одна цепочка цифр остаётся числом: «коммит 2024 года» не склеивается в хэш
- `.NET` — словарный термин, а не правило «точка + нет»: иначе ломалась бы обычная речь

Реализация: `src-tauri/src/spoken_tech.rs`. Рядом — **правка выделения**: выделите текст, нажмите клавишу Edit, продиктуйте замену, отпустите — выделение сменяется новой репликой на месте, без копирования в чат.

## Тесты

```sh
npm test                 # фронтенд + Rust unit + integration
npm run test:all         # плюс UI, pipeline, dictionary, personalization
npm run check:gate       # как CI jobs quality + license + security
npm run check            # gate + cargo check/fmt/clippy
npm run check:local      # офлайн-проверщик, без сети
npm run test:ai          # бенчмарк (каталог + опционально локальные модели)
```

Юнит-тесты не требуют микрофона: железо подменяется, фикстуры лежат в дереве. GUI-крейту нужны системные библиотеки Tauri (на Debian — список в разделе Linux выше).

## Разработка

```sh
npm install
npm run check
npm test
npm run tauri dev
```

| Команда                    | Что делает                                                                              |
| -------------------------- | --------------------------------------------------------------------------------------- |
| `npm run build`            | UI + debug-бинарник Rust                                                                |
| `npm run build:release`    | проверки UI, релиз Rust, установщики хоста (dmg/app, nsis, deb/AppImage), SBOM, SHA-256 |
| `npm run sbom`             | CycloneDX SBOM                                                                          |
| `npm run audit`            | npm audit + cargo audit                                                                 |
| `npm run uniqueness:check` | что отчёт уникальности приложен к дереву                                                |

CI гоняет те же скрипты, а не копии их команд: зелёный `quality` / `lint` / `test` / `license` / `security` в Actions означает то же, что локальный `npm run check` + `npm run test:all`. Job `package` собирает установщики на `main`, теге `v*` или `workflow_dispatch`.

Первый `cargo fetch` нужен сети. После `src-tauri/Cargo.lock` крейты резолвятся воспроизводимо. В сборке нет секретов и абсолютных путей разработчика.

Окружение закреплено в [docs/development/SETUP.md](docs/development/SETUP.md). Вклад — [CONTRIBUTING.md](CONTRIBUTING.md).

## Лицензия

MIT. См. `LICENSE`, `NOTICE`, `licenses/`, [docs/licensing/](docs/licensing/).
