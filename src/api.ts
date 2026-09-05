import { invoke } from "@tauri-apps/api/core";

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
  sha256: string;
  size: number;
  license: string;
  license_url: string;
  network_required_to_obtain: boolean;
  notes: string;
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

export const api = {
  getBuildInfo: () => invoke<BuildInfo>("get_build_info"),
  getSettings: () => invoke<AppSettings>("get_settings"),
  saveSettings: (settings: AppSettings) => invoke<void>("save_settings", { settings }),
  listModels: () => invoke<ModelRecord[]>("list_models"),
  listDictionary: () => invoke<DictionaryEntry[]>("list_dictionary"),
  upsertDictionary: (entry: DictionaryEntry) => invoke<void>("upsert_dictionary_entry", { entry }),
  removeDictionary: (id: string) => invoke<void>("remove_dictionary_entry", { id }),
  exportConfiguration: () => invoke<string>("export_configuration"),
  importConfiguration: (json: string) => invoke<void>("import_configuration", { json }),
  listHistory: () => invoke<HistoryItem[]>("list_history"),
  deleteHistory: () => invoke<void>("delete_history"),
  resetPersonalization: () => invoke<void>("reset_personalization"),
  processTranscript: (transcript: string) => invoke("process_transcript", { transcript }),
  completeOnboarding: () => invoke<void>("complete_onboarding"),
  privacySummary: () => invoke<PrivacySummary>("privacy_summary"),
  verifyModel: (modelId: string) => invoke<string>("verify_model", { modelId }),
};
