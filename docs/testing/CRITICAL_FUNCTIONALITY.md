# Критический функционал — полный список проверок

Дата: 2026-09-06.  
Продукт: **LocalFlow** — удержал клавишу → локальный Whisper → вставил текст в чужое окно.

Это список **критического** поведения: без него диктант непригоден. Не путать с полной матрицей CPU (`OPEN_SOURCE_CHECKS.md`) и с UI-полировкой.

Правило отсечения (как у Paraspeech / Unspoken): сначала ломается ли цикл «хоткей → звук → текст в курсоре», потом уже WER и темы.

Каждая строка: **ID · что проверить · критерий pass · откуда взято**.

---

## Источники

| Источник | Что даёт для «критичности» |
| --- | --- |
| [Unspoken — 15 минут](https://tryunspoken.com/blog/how-to-test-a-mac-dictation-app-in-fifteen-minutes/) | Одинаковый микрофон и текст; вставка в почту / заметки / Slack / браузер; recovery после сбоя |
| [Paraspeech — how to choose](https://paraspeech.com/blog/choosing-your-best-mac-dictation-software) | Сначала offline, потом каждое поле ввода, потом clipboard fallback, имена/цифры/код |
| [Wispr Flow — paste fails](https://docs.wisprflow.ai/articles/4783062859-dictation-appears-in-flow-but-does-not-paste-correctly-in-text-field-text-insertion-fails) | Текст есть в приложении, но не в целевом поле; Copy last / Cmd+V |
| [TypeWhisper troubleshooting](https://www.typewhisper.com/en/docs/mac/troubleshooting) | Микрофон и Accessibility — **два** права TCC |
| [VocaMac](https://vocamac.com/) | Hold-to-talk, индикатор записи, Cmd+V + восстановление буфера, любое приложение |
| [Hold To Talk](https://holdtotalk.ai/) | Модель один раз, дальше offline; paste или CGEvent; без сети после setup |
| [Dictly / voice-dictate READMEs](https://github.com/vlr-code/dictly) | Fallback: нет Accessibility → буфер, не тихий отказ |
| [Voibe — sleep](https://www.getvoibe.com/resources/dictation-app-stops-working-after-sleep/) | После сна захват живой |
| [SnailText / offline dictation](https://snailtext.app/offline-dictation/) | Вставка в браузер, редактор, терминал; Enhanced Dictation часто не туда |
| [JustVoice privacy audit](https://justvoice.ai/blog/mac-dictation-privacy-audit-2026) | Во время диктовки нет исходящего STT |
| [LumeVoice / Dictato / MetaWhisp](https://lumevoice.com/blog/ai-dictation-accuracy-benchmarks-2026/) | WER на своём голосе; RAM; граница замера «отпустил → текст в поле» |
| [Apple universal binary](https://developer.apple.com/documentation/apple-silicon/building-a-universal-macos-binary) | Нативный слайс CPU |
| LocalFlow README | 4 блока: Capture / Recognize / Format / Insert |

---

## Как считать провал

**P0 (блокер релиза):** любой fail в разделах 1–4.  
**P1 (критично для заявленного продукта, не «красиво»):** раздел 5.  
**P2:** удобство, не ломает цикл — не в этом файле.

На каждом прогоне записать: git SHA, `.app` vs `tauri dev`, `brand_string` CPU, модель Whisper, язык, микрофон.

---

## 1. Capture — захват (P0)

Без этого нет диктанта (VocaMac, Hold To Talk, TypeWhisper, Wispr mic errors).

| ID | Проверка | Pass |
| --- | --- | --- |
| CF-C01 | Право **Microphone** запрошено, строка в Info.plist | prompt или строка в Системных настройках |
| CF-C02 | Отказ в микрофоне | не тишина: экран «открой Privacy», запись не стартует |
| CF-C03 | Список устройств совпадает с Sound settings | встроенный / USB / Continuity видны |
| CF-C04 | Выбор не-дефолтного микрофона | следующая реплика с этого входа |
| CF-C05 | **Hold-to-talk:** Control+Shift+Space удерживать ≥ **500 ms** | запись стартует; короче 500 ms — отброс, **не** hands-free |
| CF-C06 | Hands-free только если галка в настройках | press-to-toggle не включается тапом 320 ms |
| CF-C07 | Микрофон **включён только во время захвата** | оранжевая точка / CPAL drop после реплики |
| CF-C08 | Индикатор записи (bar / tray ● / Listening) | виден всё время hold; плоская волна = нет сигнала (Superwhisper) |
| CF-C09 | Нет устройства / Zoom занял вход | ошибка, не hang |
| CF-C10 | USB выдернули в записи | ошибка + можно выбрать другой вход |
| CF-C11 | Короткий клип / тишина | **не** вставлять галлюцинацию; «No mic signal» / ничего не вставлено |
| CF-C12 | Сон 5 мин → просыпание → одна реплика (Voibe) | захват без перезапуска приложения |
| CF-C13 | Хоткей при **другом** приложении в фокусе | запись стартует (глобальный shortcut) |
| CF-C14 | Полноэкран чужого приложения | хоткей жив |
| CF-C15 | Экран заблокирован | записи и вставки нет |
| CF-C16 | Конфликт хоткея | UI: registered vs error, не «ничего» |

---

## 2. Recognize — модель и STT (P0)

Без проверенной локальной модели цикл врёт (каталог, Paraspeech offline, JustVoice).

| ID | Проверка | Pass |
| --- | --- | --- |
| CF-R01 | Первый запуск качает **активную** модель (по умолчанию whisper-medium) или честный skip | не пустой STT |
| CF-R02 | Перед активацией: **SHA-256** каталога | mismatch → `MODEL_CHECKSUM_MISMATCH`, файл не грузится |
| CF-R03 | Перед активацией: magic ggml/GGUF | битый файл не activate |
| CF-R04 | Неполный download: размер ≠ каталог | не skip HTTP; resume или ошибка incomplete |
| CF-R05 | После установки моделей: **Wi‑Fi off**, hold → текст (Paraspeech / Hold To Talk) | сеть не нужна |
| CF-R06 | Во время реплики нет upload аудио (Little Snitch / pcap) | 0 байт STT наружу |
| CF-R07 | Язык `ru` / `en` / `auto` из настройки | переключение без пересборки |
| CF-R08 | Живой микрофон, одна фраза на языке настройки | непустой transcript, не только UI |
| CF-R09 | Тот же Voice Memo 30 с (MetaWhisp) | WER записан; TTS `say` **не** замена F-01 |
| CF-R10 | Теги модели `[BLANK_AUDIO]`, `<|en|>` | **нет** в финальном тексте |
| CF-R11 | Нет Apple Speech fallback при отсутствии ggml | `MODEL_MISSING`, не облако |
| CF-R12 | `compute_device=cpu` в этом билде | не падает из‑за GPU picker |
| CF-R13 | Мало места на диске при download Medium | отказ, не битый `.bin` |
| CF-R14 | Repeat last: тот же wav, текущая модель | повтор без нового захвата |

---

## 3. Format — текст, который пользователь правит руками (P0 для «готового» ввода)

Unspoken: судить **отредактированный** результат. Paraspeech: имена, цифры, код до оплаты.

| ID | Проверка | Pass |
| --- | --- | --- |
| CF-F01 | Режим Normal: заглавная, точка, без филлеров «ну короче э-э» где заявлено | осмысленный черновик |
| CF-F02 | Режим Raw: слова как сказаны | без списков/переписывания |
| CF-F03 | Режим Code: текст **не исполняется** | только вставка символов |
| CF-F04 | `digits_from_speech=true` | «двадцать пять» → `25` |
| CF-F05 | Часы **ЧЧ:ММ** | «пятнадцать часов тридцать минут» / «9 часов 5 минут» → `15:30` / `09:05` |
| CF-F06 | Даты по `date_format` | DMY `05.03.2026` или ISO `2026-03-05`; Smart Format **не** рвёт `5.3.26` в `5. 3. 26` |
| CF-F07 | `digits_from_speech=false` | числительные остаются словами |
| CF-F08 | Словарь (sql → SELECT и т.п.) | срабатывает в scripted и в живой реплике |
| CF-F09 | Backtrack « нет » / scratch that | замена значения / сброс фразы |
| CF-F10 | Две подряд реплики | пробел: `Привет. Мир`, не `ПриветМир` |
| CF-F11 | Имена и бренды (Unspoken messy sentence) | словарь/персонализация или приемлемый WER |
| CF-F12 | Таймаут постпроцесса | ошибка в UI, не вечный «Processing» |

---

## 4. Insert — текст в чужом окне (P0)

Wispr/VocaMac/Dictly: это самая частая «приложение живое, курсор мёртв».

| ID | Проверка | Pass |
| --- | --- | --- |
| CF-I01 | Право **Accessibility** | без него не тихий fail: Copy last / буфер (Dictly fallback) |
| CF-I02 | TextEdit / Заметки, курсор в документе | текст в поле, не только в LocalFlow |
| CF-I03 | Mail / почтовый compose | то же |
| CF-I04 | Slack или Telegram | то же |
| CF-I05 | Safari / Chrome, обычный input | то же |
| CF-I06 | Редактор кода (Cursor, VS Code, Xcode) | то же |
| CF-I07 | Google Docs / contenteditable | то же или честный fail + буфер |
| CF-I08 | 2000 символов | без обрезки |
| CF-I09 | `restore_clipboard=true` | после успешного Cmd+V прежний буфер (текст; RTF/картинка если заявлено) |
| CF-I10 | Вставка **не удалась** | диктант остаётся для Copy last / Cmd+V; прошлый буфер можно не вернуть (Wispr) |
| CF-I11 | Пауза `insert_delay_ms` | изменение настройки меняет задержку (≥ 40 ms) |
| CF-I12 | Secure Input / поле пароля | paste блокируется; Copy last работает после выхода из поля |
| CF-I13 | Нет активного поля | пользователь видит отказ, текст не потерян |
| CF-I14 | Удалённый рабочий стол (Wispr) | LocalFlow **на хосте**; если clipboard sharing выключен — документированный fail |
| CF-I15 | Вставленный текст не запускается как код | validate / code mode |

---

## 5. Критично для заявленного LocalFlow (P1)

Без этого продукт «как в README» не выполнен, хотя сырой цикл иногда жив.

| ID | Проверка | Pass |
| --- | --- | --- |
| CF-P01 | Второй экземпляр не ломает первый | single instance |
| CF-P02 | `settings.json` читаемый JSON, правка без rebuild | apply за секунды |
| CF-P03 | Трей: recording vs idle, **не** мигает на каждый RMS | |
| CF-P04 | Журнал: секреты/ключи замаскированы | |
| CF-P05 | Каталог данных `~/Library/Application Support/LocalFlow` | audio **0o700** |
| CF-P06 | Uninstall скрипт / кнопка Privacy | спрашивает, хранить ли историю |
| CF-P07 | Copy last / Paste last хоткеи | после неудачной вставки |
| CF-P08 | Макрос Dictate (`osascript` Control+Shift+Space) | эмулирует хоткей приложения |
| CF-P09 | История / журнал выключается настройкой | |
| CF-P10 | Лицензии зависимостей `npm run license:check` | allowlist |
| CF-P11 | Нет cloud API key в продукте | нечего класть в Keychain |
| CF-P12 | После обновления `.app` TCC не «залип» (TypeWhisper) | toggle или `tccutil reset` + повторный prompt |

---

## 6. Минимальный живой сценарий (15 минут, Unspoken)

На **одном** микрофоне, **одном** тексте:

1. Письмо в Mail или Notes.  
2. Заметка со списком.  
3. Slack/браузер **и** редактор кода.  
4. Грязная фраза: имя + `09:05` + дата.  
5. Срыв: mute микрофона → нет ложной вставки → Copy last / повтор.

Если шаг 3 или 5 падает — P0, даже при красивом WER на шаге 1.

---

## 7. Автомат vs человек

| Можно закрыть `npm test` / CLI | Только человек на Mac |
| --- | --- |
| CF-F01–F10, CF-R02–R04, CF-R10, CF-I15, CF-P01, CF-P10 | CF-C05–C16, CF-R05–R09, CF-I02–I14, CF-C12, сценарий §6 |
| `localflow check`, `transcribe --json` | Airplane mode + pcap, Bluetooth, сон, полноэкран |

TTS `say` → Whisper **не** закрывает CF-R08.

---

## Связанные файлы

- Поле + CPU SKU: `docs/testing/OPEN_SOURCE_CHECKS.md`  
- Гейт релиза: `docs/testing/RELEASE_GATE.md`  
- Прогон на i7-9750H: `docs/evaluation/HOST_RUN.md`
