export type UiLang = "en" | "ru";

const NAV_EN: { id: string; label: string }[] = [
  { id: "home", label: "Pipeline" },
  { id: "settings", label: "Settings" },
  { id: "models", label: "Models" },
  { id: "dictionary", label: "Dictionary" },
  { id: "snippets", label: "Snippets" },
  { id: "profiles", label: "Profiles" },
  { id: "personalization", label: "Personalization" },
  { id: "history", label: "History" },
  { id: "diagnostics", label: "Diagnostics" },
  { id: "privacy", label: "Privacy" },
];

const NAV_RU: { id: string; label: string }[] = [
  { id: "home", label: "Конвейер" },
  { id: "settings", label: "Настройки" },
  { id: "models", label: "Модели" },
  { id: "dictionary", label: "Словарь" },
  { id: "snippets", label: "Фрагменты" },
  { id: "profiles", label: "Профили" },
  { id: "personalization", label: "Персонализация" },
  { id: "history", label: "История" },
  { id: "diagnostics", label: "Диагностика" },
  { id: "privacy", label: "Приватность" },
];

export const NAV = NAV_EN;

export function isRu(lang: string | undefined | null): boolean {
  return (lang ?? "en").trim().toLowerCase() === "ru";
}

export function navItems(lang: string): { id: string; label: string }[] {
  return isRu(lang) ? NAV_RU : NAV_EN;
}

export function formatBytes(size: number): string {
  if (size < 1024) return `${size} B`;
  if (size < 1024 * 1024) return `${(size / 1024).toFixed(1)} KB`;
  if (size < 1024 * 1024 * 1024) return `${(size / 1024 / 1024).toFixed(1)} MB`;
  return `${(size / 1024 / 1024 / 1024).toFixed(1)} GB`;
}

const EN = {
  setupNav: "Setup",
  continue: "Continue",
  onboardingTitle: "Speak. Release. Insert — entirely on this Mac.",
  onboarding1: "1. Allow Microphone and Accessibility (paste into other apps).",
  onboarding2:
    "2. Whisper Medium (~1.5 GB) downloads automatically on this screen (Hugging Face, checksum checked).",
  onboarding3: "3. Hold Control+Shift+Space over a text field, talk, release.",
  openMicSettings: "Open Microphone settings",
  openAccessSettings: "Open Accessibility settings",
  microphone: "Microphone",
  systemDefault: "System default",
  osDefault: " (OS default)",
  sttReady: "is installed and will be used for dictation.",
  sttDownloading: "Downloading from Hugging Face…",
  sttWillDownload:
    "Whisper will download automatically. You can continue and let it finish in the background.",
  accessibilityTrusted: " Accessibility: trusted.",
  accessibilityNotTrusted: " Accessibility: not trusted yet.",
  browserHint:
    "This browser tab cannot talk to Rust. Keep npm run tauri dev running and use the LocalFlow window (it should open itself).",
  homeTitle: "Dictation pipeline",
  homeHelp:
    "Type a sample and click Process locally, or hold the hotkey over a field. Whisper.cpp transcribes when the model is installed. Escape cancels. Cmd+Ctrl+C/V copy or paste the last transcript.",
  homePlaceholder: "Preview a transcript without the microphone",
  processLocally: "Process locally",
  transcript: "Transcript",
  afterDictionary: "After dictionary",
  formedText: "Formed text",
  downloadBusy: "Downloading",
  downloadWait: "Dictation starts when the checksum passes.",
  whisperNotReady:
    "Whisper is not ready. LocalFlow downloads it automatically — stay online, or open Models to retry.",
  openModels: "Open Models",
  currentApp: "Current app",
  profile: "Profile",
  settingsTitle: "Settings",
  diskSpace: "Disk space",
  free: "free",
  speechFits: "speech model fits",
  speechNeeds: "speech still needs",
  hotkeyLabel: "Hotkey (Tauri syntax, e.g. Control+Shift+Space)",
  hotkeyHelp:
    "Option+Space and Control+Space are often taken by macOS (Spotlight / input source). Check System Settings → Keyboard → Keyboard Shortcuts. Changing the hotkey here re-registers it immediately.",
  speechLanguage: "Speech language",
  langRussian: "Russian",
  langEnglish: "English",
  langAuto: "Auto-detect",
  speechLangHelp:
    "Russian is more accurate for Russian-only speech. Use Auto-detect when a replica mixes Russian and English so Whisper can switch language inside the clip.",
  interfaceLanguage: "Interface language",
  refreshDevices: "Refresh devices",
  micPermission: "Microphone permission",
  accessPermission: "Accessibility permission",
  launchAtLogin: "Launch at login",
  keepHistory: "Keep utterance history and JSONL journal",
  keepLastWav: "Keep last utterance WAV for Repeat",
  resetSettings: "Reset settings to defaults",
  historySize: "History size (oldest rows are dropped)",
  silenceTrim: "Silence trim (VAD)",
  vadHelp: "Lower keeps quiet speech. Higher treats room noise as silence.",
  fallbackMode: "Fallback mode (used when no app profile matches)",
  profileOverride: "Profile override",
  autoFrontmost: "Auto (frontmost app)",
  restoreClipboard: "Restore clipboard after insert",
  clipboardHelp:
    "Keeps the previous pasteboard (text, RTF, images) after Cmd+V. If the app crashes mid-paste, the same snapshot is restored from disk. Password fields block paste — use Copy last after leaving the field.",
  showFlowBar: "Show LocalFlow Bar while listening",
  playCues: "Play start/end recording sounds",
  cueVolume: "Cue volume",
  pauseInsert: "Pause before insert (ms)",
  handsFree:
    "Hands-free (press to start, press again to stop). Hold-to-talk stays the default when this is off.",
  spokenDigits: "Write spoken numbers as digits",
  dateFormat: "Date format",
  acceleration: "Acceleration device",
  cpuBuild: "CPU (this build)",
  postTimeout: "Post-processing timeout (ms)",
  installMacro: "Install Dictate macro",
  copyLastHotkey: "Copy last transcript",
  pasteLastHotkey: "Paste last transcript",
  editHotkey: "Edit selection (same hold-to-talk; paste replaces the highlight)",
  copyLast: "Copy last",
  pasteLast: "Paste last",
  exportConfig: "Export configuration",
  importConfig: "Import configuration",
  exportPlaceholder: "Exported JSON appears here",
  modelsTitle: "Model Manager",
  modelsHelp:
    "Speech default is Whisper Medium. Download it (or pick another Whisper), then Use this for speech. Repeat re-runs the last recording through the speech model in use — it is not a separate download. Qwen models are only for text formatting, not speech.",
  inUseSpeech: "Currently in use · speech",
  inUseFormat: "Currently in use · formatting",
  modelNotSelected: "Not selected",
  modelChoose: "Choose a model below.",
  modelReady: "Ready on this Mac.",
  modelDownloading: "Download in progress — not used yet.",
  modelChecksum: "File failed checksum — not used.",
  modelNotInstalled: "Selected but not installed.",
  dictionaryTitle: "Dictionary 2.0",
  dictionaryHelp:
    "Vocabulary keeps a canonical term plus aliases. Replacement Rule maps spoken phrases to written text. Built-in developer terms (RestAssured, JUnit, …) are seeded automatically.",
  searchPlaceholder: "Search canonical or alias",
  replacementRule: "Replacement Rule",
  vocabulary: "Vocabulary",
  snippetsTitle: "Snippets",
  snippetsHelp: "Exact trigger expands before the LLM. Priority: Command → Snippet → Dictionary.",
  profilesTitle: "Styles + application profiles",
  saveProfiles: "Save profiles",
  personalizationTitle: "Personalization",
  personalizationOn: "Personalization ON",
  learnCorrections: "Learn from corrections",
  personalizationHelp:
    "First correction is a candidate. Repeat it to get a suggestion. Accept writes a dictionary replacement rule.",
  historyTitle: "History",
  diagnosticsTitle: "Diagnostics",
  privacyTitle: "Privacy",
  privacyIntro: "Core pipeline is local. Cloud accounts are not required.",
  privacyNetwork: "Network is used only for:",
  privacyLogs: "Audio cache uses a private 0700 folder. Logs rotate by size and never store tokens.",
  holdHint: "Hold Control+Shift+Space, speak, release.",
  recording: "Recording… keep holding, then release to process.",
  processing: "Processing recording…",
  devicesRefreshed: "Microphone list refreshed.",
  settingsReset: "Settings restored to defaults.",
};

const RU: typeof EN = {
  setupNav: "Начало",
  continue: "Продолжить",
  onboardingTitle: "Говорите. Отпустите. Вставка — полностью на этом Mac.",
  onboarding1: "1. Разрешите микрофон и Универсальный доступ (вставка в другие приложения).",
  onboarding2:
    "2. Whisper Medium (~1,5 ГБ) скачивается на этом экране (Hugging Face, проверка контрольной суммы).",
  onboarding3: "3. Удерживайте Control+Shift+Space над текстовым полем, говорите, отпустите.",
  openMicSettings: "Открыть настройки микрофона",
  openAccessSettings: "Открыть Универсальный доступ",
  microphone: "Микрофон",
  systemDefault: "Системный по умолчанию",
  osDefault: " (системный)",
  sttReady: "установлен и будет использоваться для диктовки.",
  sttDownloading: "Загрузка с Hugging Face…",
  sttWillDownload:
    "Whisper скачается сам. Можно продолжить — загрузка пойдёт в фоне.",
  accessibilityTrusted: " Универсальный доступ: разрешён.",
  accessibilityNotTrusted: " Универсальный доступ: ещё не разрешён.",
  browserHint:
    "Эта вкладка браузера не связана с Rust. Оставьте npm run tauri dev и работайте в окне LocalFlow.",
  homeTitle: "Конвейер диктовки",
  homeHelp:
    "Введите пример и нажмите «Обработать локально» или удерживайте хоткей над полем. Whisper.cpp распознаёт речь, когда модель установлена. Escape отменяет. Cmd+Ctrl+C/V копируют или вставляют последний текст.",
  homePlaceholder: "Предпросмотр транскрипта без микрофона",
  processLocally: "Обработать локально",
  transcript: "Транскрипт",
  afterDictionary: "После словаря",
  formedText: "Готовый текст",
  downloadBusy: "Загрузка",
  downloadWait: "Диктовка начнётся после проверки контрольной суммы.",
  whisperNotReady:
    "Whisper ещё не готов. LocalFlow скачивает его сам — оставайтесь онлайн или откройте «Модели».",
  openModels: "Открыть модели",
  currentApp: "Текущее приложение",
  profile: "Профиль",
  settingsTitle: "Настройки",
  diskSpace: "Место на диске",
  free: "свободно",
  speechFits: "модель речи помещается",
  speechNeeds: "для речи ещё нужно",
  hotkeyLabel: "Хоткей (синтаксис Tauri, например Control+Shift+Space)",
  hotkeyHelp:
    "Option+Space и Control+Space часто заняты macOS (Spotlight / раскладка). Проверьте Системные настройки → Клавиатура → Сочетания клавиш. Смена хоткея здесь перерегистрирует его сразу.",
  speechLanguage: "Язык речи",
  langRussian: "Русский",
  langEnglish: "Английский",
  langAuto: "Автоопределение",
  speechLangHelp:
    "Русский точнее для только русской речи. Автоопределение нужно, когда в одной реплике смешаны русский и английский.",
  interfaceLanguage: "Язык интерфейса",
  refreshDevices: "Обновить устройства",
  micPermission: "Право на микрофон",
  accessPermission: "Право Универсального доступа",
  launchAtLogin: "Запускать при входе",
  keepHistory: "Хранить историю реплик и JSONL-журнал",
  keepLastWav: "Хранить последний WAV для «Повторить»",
  resetSettings: "Сбросить настройки",
  historySize: "Размер истории (старые записи удаляются)",
  silenceTrim: "Обрезка тишины (VAD)",
  vadHelp: "Ниже — сохраняет тихую речь. Выше — считает шум тишиной.",
  fallbackMode: "Режим по умолчанию (если нет профиля приложения)",
  profileOverride: "Принудительный профиль",
  autoFrontmost: "Авто (активное приложение)",
  restoreClipboard: "Восстанавливать буфер после вставки",
  clipboardHelp:
    "Возвращает прежний буфер (текст, RTF, картинки) после Cmd+V. Если приложение упадёт во время вставки, снимок восстановится с диска. Поля пароля блокируют вставку — используйте «Копировать последнее» после выхода из поля.",
  showFlowBar: "Показывать LocalFlow Bar во время записи",
  playCues: "Звуки начала и конца записи",
  cueVolume: "Громкость сигналов",
  pauseInsert: "Пауза перед вставкой (мс)",
  handsFree:
    "Hands-free (нажали — запись, нажали снова — стоп). Если выключено, работает удержание хоткея.",
  spokenDigits: "Писать произнесённые числа цифрами",
  dateFormat: "Формат даты",
  acceleration: "Ускорение",
  cpuBuild: "CPU (эта сборка)",
  postTimeout: "Таймаут постобработки (мс)",
  installMacro: "Установить макрос Dictate",
  copyLastHotkey: "Копировать последний транскрипт",
  pasteLastHotkey: "Вставить последний транскрипт",
  editHotkey: "Правка выделения (тот же hold-to-talk; вставка заменяет выделение)",
  copyLast: "Копировать последнее",
  pasteLast: "Вставить последнее",
  exportConfig: "Экспорт конфигурации",
  importConfig: "Импорт конфигурации",
  exportPlaceholder: "Здесь появится экспортированный JSON",
  modelsTitle: "Менеджер моделей",
  modelsHelp:
    "По умолчанию для речи — Whisper Medium. Скачайте его (или другой Whisper), затем «Использовать для речи». «Повторить» прогоняет последнюю запись через текущую речевую модель — это не отдельная загрузка. Модели Qwen только форматируют текст, не распознают речь.",
  inUseSpeech: "Сейчас используется · речь",
  inUseFormat: "Сейчас используется · форматирование",
  modelNotSelected: "Не выбрано",
  modelChoose: "Выберите модель ниже.",
  modelReady: "Готова на этом Mac.",
  modelDownloading: "Идёт загрузка — пока не используется.",
  modelChecksum: "Контрольная сумма не совпала — не используется.",
  modelNotInstalled: "Выбрана, но не установлена.",
  dictionaryTitle: "Словарь 2.0",
  dictionaryHelp:
    "Vocabulary хранит канонический термин и алиасы. Replacement Rule заменяет произнесённые фразы на написание. Встроенные термины разработчика (RestAssured, JUnit, …) добавляются сами.",
  searchPlaceholder: "Поиск канонического имени или алиаса",
  replacementRule: "Правило замены",
  vocabulary: "Словарь",
  snippetsTitle: "Фрагменты",
  snippetsHelp:
    "Точный триггер раскрывается до LLM. Приоритет: Команда → Фрагмент → Словарь.",
  profilesTitle: "Стили и профили приложений",
  saveProfiles: "Сохранить профили",
  personalizationTitle: "Персонализация",
  personalizationOn: "Персонализация включена",
  learnCorrections: "Учиться на исправлениях",
  personalizationHelp:
    "Первое исправление — кандидат. Повтор даёт предложение. Принятие пишет правило в словарь.",
  historyTitle: "История",
  diagnosticsTitle: "Диагностика",
  privacyTitle: "Приватность",
  privacyIntro: "Основной конвейер локальный. Облачный аккаунт не нужен.",
  privacyNetwork: "Сеть используется только для:",
  privacyLogs:
    "Кэш аудио лежит в закрытой папке 0700. Логи ротируются по размеру и не хранят токены.",
  holdHint: "Удерживайте Control+Shift+Space, говорите, отпустите.",
  recording: "Запись… удерживайте, затем отпустите для обработки.",
  processing: "Обработка записи…",
  devicesRefreshed: "Список микрофонов обновлён.",
  settingsReset: "Настройки сброшены к значениям по умолчанию.",
};

export type UiCopy = typeof EN;

export function copy(lang: string | undefined | null): UiCopy {
  return isRu(lang) ? RU : EN;
}
