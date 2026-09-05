import { invoke } from "@tauri-apps/api/core";
import catalogJson from "../src-tauri/resources/model-catalog.json";

export type ViewId =
  | "home"
  | "onboarding"
  | "settings"
  | "models"
  | "dictionary"
  | "snippets"
  | "profiles"
  | "personalization"
  | "history"
  | "diagnostics"
  | "privacy";

export interface BuildInfo {
  application: string;
  version: string;
  git_sha: string;
  platform: string;
  architecture: string;
  build_date: string;
  tauri_version: string;
  rustc_version: string;
  native_runtime: string;
}

export interface AppSettings {
  hotkey: string;
  mode: "raw" | "normal" | "professional" | "code";
  microphone_name: string | null;
  active_stt_model: string | null;
  active_llm_model: string | null;
  restore_clipboard: boolean;
  onboarding_complete: boolean;
  copy_last_hotkey: string;
  paste_last_hotkey: string;
  show_flow_bar: boolean;
  profile_override: string | null;
  personalization_enabled: boolean;
  learn_from_corrections: boolean;
  stt_language: string;
  insert_delay_ms: number;
  postprocess_timeout_ms: number;
  sound_cues: boolean;
  log_max_bytes: number;
  autostart: boolean;
  history_enabled: boolean;
  vad_threshold: number;
  history_max_items: number;
  hands_free: boolean;
  digits_from_speech: boolean;
  date_format: string;
  compute_device: string;
}

export interface ModelRecord {
  model_id: string;
  display_name: string;
  version: string;
  filename: string;
  format: string;
  quantization: string;
  kind: string;
  source: string;
  source_url: string;
  download_url?: string;
  sha256: string;
  size: number;
  license: string;
  license_url: string;
  network_required_to_obtain: boolean;
  checksum_pinned?: boolean;
  notes: string;
}

export interface ModelInstallStatus {
  model_id: string;
  state: "missing" | "downloading" | "incomplete" | "unverified" | "verified" | string;
  installed: boolean;
  verified: boolean;
  local_path: string | null;
  bytes_on_disk: number;
  expected_bytes: number;
  active: boolean;
}

export interface ModelDownloadProgress {
  model_id: string;
  phase: string;
  bytes_downloaded: number;
  total_bytes: number;
}

export interface DictionaryEntry {
  id: string;
  kind: "vocabulary" | "replacement";
  canonical: string;
  aliases: string[];
  source: string;
  replacement: string;
  case_sensitive: boolean;
  enabled: boolean;
  builtin: boolean;
}

export interface Snippet {
  id: string;
  trigger: string;
  content: string;
  language: string;
  profile: string;
  enabled: boolean;
  created_at: string;
  updated_at: string;
}

export interface Profile {
  id: string;
  name: string;
  mode: AppSettings["mode"];
  style: string;
  dictionary_ids: string[];
  apps: string[];
  group: string;
}

export interface ResolvedContext {
  app_name: string;
  profile_id: string;
  profile_name: string;
  style: string;
  mode: AppSettings["mode"];
  source: string;
}

export interface LearnedCandidate {
  id: string;
  pattern: string;
  replacement: string;
  weight: number;
  accepted: boolean;
}

export interface PipelineOutput {
  raw_transcript: string;
  dictionary_text: string;
  backtrack_text: string;
  formatted_text: string;
  personalized_text: string;
  final_text: string;
  mode: AppSettings["mode"];
  insert_ok: boolean;
  cues?: { start_ms: number; end_ms: number; text: string }[];
}

export interface HistoryItem {
  id: string;
  created_at: string;
  mode: string;
  transcript: string;
  output: string;
  application: string;
  profile: string;
  model: string;
  processing_time_ms: number;
  timecodes?: string;
}

export interface PrivacySummary {
  audio_local: boolean;
  stt_local: boolean;
  llm_local: boolean;
  dictionary_local: boolean;
  personalization_local: boolean;
  history_local: boolean;
  cloud_account_required: boolean;
  network_operations: string[];
  data_root: string;
}

export interface DiskUsage {
  data_root: string;
  free_bytes: number | null;
  used_models_bytes: number;
  overhead_bytes: number;
  stt_name: string;
  stt_required_bytes: number;
  stt_on_disk_bytes: number;
  stt_still_needed_bytes: number;
  llm_name: string;
  llm_required_bytes: number;
  llm_on_disk_bytes: number;
  llm_still_needed_bytes: number;
  enough_for_speech: boolean;
  enough_for_speech_and_formatting: boolean;
  messages: string[];
}

type TauriWindow = Window & {
  __TAURI_INTERNALS__?: { invoke?: unknown };
};

export function isTauriRuntime(): boolean {
  if (typeof window === "undefined") {
    return false;
  }
  return typeof (window as TauriWindow).__TAURI_INTERNALS__?.invoke === "function";
}

export const bundledCatalog: ModelRecord[] = catalogJson.models as ModelRecord[];

export interface DictationState {
  phase: string;
  message: string;
  transcript: string | null;
  raw_transcript: string | null;
  duration_ms: number;
  insert_ok?: boolean;
  rms?: number;
  wpm?: number | null;
}

export interface PermissionStatus {
  microphone_device_count: number;
  accessibility_trusted: boolean;
}

export interface AudioDevice {
  name: string;
  is_default: boolean;
}

export interface StatsSnapshot {
  recordings: number;
  words_today: number;
  words_total: number;
  wpm_avg_today: number;
  wpm_avg_all: number;
  wpm_best: number;
  last_wpm: number;
}

export interface HotkeyStatus {
  requested: string;
  registered: string | null;
  error: string | null;
}

const BROWSER_HINT =
  "IPC is unavailable. Use the LocalFlow window from the menu bar (npm run tauri dev), not a browser tab on localhost:1420.";

export function formatInvokeError(error: unknown): string {
  if (error instanceof Error && error.message && error.message !== "[object Object]") {
    return error.message;
  }
  if (typeof error === "string") {
    return error;
  }
  if (error && typeof error === "object") {
    const value = error as Record<string, unknown>;
    const nested =
      value.error && typeof value.error === "object"
        ? (value.error as Record<string, unknown>)
        : value;
    const code = typeof nested.code === "string" ? nested.code : "";
    const message =
      typeof nested.message === "string"
        ? nested.message
        : typeof value.message === "string"
          ? value.message
          : "";
    if (code && message) {
      return `${code}: ${message}`;
    }
    if (message) {
      return message;
    }
    try {
      return JSON.stringify(error);
    } catch {
      return "Unknown error";
    }
  }
  return String(error);
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!isTauriRuntime()) {
    throw new Error(BROWSER_HINT);
  }
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    throw new Error(formatInvokeError(error));
  }
}

export const api = {
  getBuildInfo: () => call<BuildInfo>("get_build_info"),
  getSettings: () => call<AppSettings>("get_settings"),
  saveSettings: (settings: AppSettings) => call<void>("save_settings", { settings }),
  listModels: () => call<ModelRecord[]>("list_models"),
  listMicrophones: () => call<AudioDevice[]>("list_microphones"),
  listDictionary: () => call<DictionaryEntry[]>("list_dictionary"),
  upsertDictionary: (entry: DictionaryEntry) => call<void>("upsert_dictionary_entry", { entry }),
  removeDictionary: (id: string) => call<void>("remove_dictionary_entry", { id }),
  searchDictionary: (query: string) => call<DictionaryEntry[]>("search_dictionary", { query }),
  importDictionary: (json: string) => call<number>("import_dictionary", { json }),
  listSnippets: () => call<Snippet[]>("list_snippets"),
  upsertSnippet: (snippet: Snippet) => call<void>("upsert_snippet", { snippet }),
  removeSnippet: (id: string) => call<void>("remove_snippet", { id }),
  listProfiles: () => call<Profile[]>("list_profiles"),
  saveProfiles: (profiles: Profile[]) => call<void>("save_profiles", { profiles }),
  getActiveContext: () => call<ResolvedContext>("get_active_context"),
  recordCorrection: (original: string, corrected: string) =>
    call<LearnedCandidate[]>("record_correction", { original, corrected }),
  listSuggestions: () => call<LearnedCandidate[]>("list_suggestions"),
  acceptSuggestion: (id: string) => call<void>("accept_suggestion", { id }),
  dismissSuggestion: (id: string) => call<void>("dismiss_suggestion", { id }),
  deleteHistoryItem: (id: string) => call<void>("delete_history_item", { id }),
  updateHistoryOutput: (id: string, output: string) =>
    call<void>("update_history_output", { id, output }),
  retryHistory: (transcript: string) => call<PipelineOutput>("retry_history", { transcript }),
  historyToSnippet: (trigger: string, content: string) =>
    call<void>("history_to_snippet", { trigger, content }),
  copyText: (text: string) => call<void>("copy_text", { text }),
  pasteText: (text: string) => call<void>("paste_text", { text }),
  exportConfiguration: () => call<string>("export_configuration"),
  importConfiguration: (json: string) => call<void>("import_configuration", { json }),
  listHistory: () => call<HistoryItem[]>("list_history"),
  deleteHistory: () => call<void>("delete_history"),
  resetPersonalization: () => call<void>("reset_personalization"),
  processTranscript: (transcript: string) =>
    call<PipelineOutput>("process_transcript", { transcript }),
  completeOnboarding: () => call<void>("complete_onboarding"),
  dictationStop: () => call<void>("dictation_stop"),
  dictationCancel: () => call<void>("dictation_cancel"),
  getLastTranscript: () => call<PipelineOutput | null>("get_last_transcript"),
  copyLastTranscript: () => call<string>("copy_last_transcript"),
  pasteLastTranscript: () => call<string>("paste_last_transcript"),
  clearLastTranscript: () => call<void>("clear_last_transcript"),
  privacySummary: () => call<PrivacySummary>("privacy_summary"),
  diskUsage: () => call<DiskUsage>("disk_usage"),
  verifyModel: (modelId: string) => call<string>("verify_model", { modelId }),
  downloadModel: (modelId: string) => call<string>("download_model", { modelId }),
  listModelStatus: () => call<ModelInstallStatus[]>("list_model_status"),
  setActiveModel: (modelId: string) => call<string>("set_active_model", { modelId }),
  lastUtteranceReady: () => call<boolean>("last_utterance_ready"),
  repeatLastUtterance: () => call<PipelineOutput>("repeat_last_utterance"),
  getHotkeyStatus: () => call<HotkeyStatus>("get_hotkey_status"),
  getStats: () => call<StatsSnapshot>("get_stats"),
  resetStats: () => call<void>("reset_stats"),
  exportStatsCsv: () => call<string>("export_stats_csv"),
  isScreenLocked: () => call<boolean>("is_screen_locked"),
  exportHistoryTimecodes: () => call<string>("export_history_timecodes"),
  uninstallLocalflow: (keepHistory: boolean) =>
    call<{ kept_history: boolean; removed: string[]; skipped: string[] }>("uninstall_localflow", {
      keepHistory,
    }),
  installDictateMacro: () => call<string>("install_dictate_macro"),
  permissionStatus: () => call<PermissionStatus>("permission_status"),
  openPrivacyPane: (kind: "microphone" | "accessibility" | "speech") =>
    call<void>("open_privacy_pane", { kind }),
};
