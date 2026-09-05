import { invoke } from "@tauri-apps/api/core";
import catalogJson from "../src-tauri/resources/model-catalog.json";

export type ViewId =
  | "home"
  | "onboarding"
  | "settings"
  | "models"
  | "dictionary"
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
  source: string;
  replacement: string;
  case_sensitive: boolean;
}

export interface HistoryItem {
  id: string;
  created_at: string;
  mode: string;
  transcript: string;
  output: string;
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
  listDictionary: () => call<DictionaryEntry[]>("list_dictionary"),
  upsertDictionary: (entry: DictionaryEntry) => call<void>("upsert_dictionary_entry", { entry }),
  removeDictionary: (id: string) => call<void>("remove_dictionary_entry", { id }),
  exportConfiguration: () => call<string>("export_configuration"),
  importConfiguration: (json: string) => call<void>("import_configuration", { json }),
  listHistory: () => call<HistoryItem[]>("list_history"),
  deleteHistory: () => call<void>("delete_history"),
  resetPersonalization: () => call<void>("reset_personalization"),
  processTranscript: (transcript: string) => call("process_transcript", { transcript }),
  completeOnboarding: () => call<void>("complete_onboarding"),
  privacySummary: () => call<PrivacySummary>("privacy_summary"),
  verifyModel: (modelId: string) => call<string>("verify_model", { modelId }),
  downloadModel: (modelId: string) => call<string>("download_model", { modelId }),
  listModelStatus: () => call<ModelInstallStatus[]>("list_model_status"),
  setActiveModel: (modelId: string) => call<string>("set_active_model", { modelId }),
  getHotkeyStatus: () => call<HotkeyStatus>("get_hotkey_status"),
};
