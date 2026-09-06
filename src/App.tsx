import { useEffect, useRef, useState } from "react";
import {
  api,
  bundledCatalog,
  isTauriRuntime,
  type AppSettings,
  type AudioDevice,
  type BuildInfo,
  type DictationState,
  type DictionaryEntry,
  type HistoryItem,
  type LearnedCandidate,
  type Profile,
  type ResolvedContext,
  type Snippet,
  type StatsSnapshot,
  type ModelDownloadProgress,
  type ModelInstallStatus,
  type ModelRecord,
  type PipelineOutput,
  type PrivacySummary,
  type PermissionStatus,
  type DiskUsage,
  type ViewId,
} from "./api";
import { formatBytes, navItems } from "./ui";
import { listen } from "@tauri-apps/api/event";

const fallbackSettings = (): AppSettings => ({
  hotkey: "Control+Shift+Space",
  mode: "normal",
  microphone_name: null,
  active_stt_model: "whisper-medium",
  active_llm_model: "Qwen3-4B-Instruct-2507",
  restore_clipboard: true,
  onboarding_complete: false,
  copy_last_hotkey: "Command+Control+C",
  paste_last_hotkey: "Command+Control+V",
  show_flow_bar: true,
  profile_override: null,
  personalization_enabled: true,
  learn_from_corrections: true,
  stt_language: "ru",
  insert_delay_ms: 120,
  postprocess_timeout_ms: 45000,
  sound_cues: true,
  sound_cue_volume: 0.25,
  log_max_bytes: 2097152,
  autostart: false,
  history_enabled: true,
  vad_threshold: 0.012,
  history_max_items: 500,
  hands_free: false,
  digits_from_speech: true,
  date_format: "DMY",
  compute_device: "cpu",
  keep_last_audio: true,
  edit_hotkey: "Command+Control+E",
  ui_language: "en",
});

function isToday(iso: string): boolean {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) {
    return false;
  }
  const now = new Date();
  return date.toDateString() === now.toDateString();
}

function daysAgo(iso: string, days: number): boolean {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) {
    return false;
  }
  return Date.now() - date.getTime() <= days * 24 * 60 * 60 * 1000;
}

function describeSelectedModel(
  id: string | null | undefined,
  models: ModelRecord[],
  statuses: ModelInstallStatus[],
): { id: string | null; name: string; ready: boolean; detail: string; version: string } {
  if (!id) {
    return {
      id: null,
      name: "Not selected",
      ready: false,
      detail: "Choose a model below.",
      version: "",
    };
  }
  const record = models.find((model) => model.model_id === id);
  const status = statuses.find((item) => item.model_id === id);
  const name = record?.display_name ?? id;
  const version = record
    ? `${record.version} · ${record.filename}`
    : "";
  const ready = status?.state === "verified" || status?.state === "installed";
  if (ready) {
    return { id, name, ready: true, detail: "Ready on this Mac.", version };
  }
  if (status?.state === "downloading" || status?.state === "incomplete") {
    return { id, name, ready: false, detail: "Download in progress — not used yet.", version };
  }
  if (status?.state === "unverified") {
    return { id, name, ready: false, detail: "File failed checksum — not used.", version };
  }
  return { id, name, ready: false, detail: "Selected but not installed.", version };
}

export function App() {
  const [view, setView] = useState<ViewId>("onboarding");
  const [settings, setSettings] = useState<AppSettings>(fallbackSettings);
  const [models, setModels] = useState<ModelRecord[]>([]);
  const [dictionary, setDictionary] = useState<DictionaryEntry[]>([]);
  const [history, setHistory] = useState<HistoryItem[]>([]);
  const [build, setBuild] = useState<BuildInfo | null>(null);
  const [privacy, setPrivacy] = useState<PrivacySummary | null>(null);
  const [status, setStatus] = useState("Hold Control+Shift+Space, speak, release.");
  const [draft, setDraft] = useState("");
  const [pipelineOut, setPipelineOut] = useState<PipelineOutput | null>(null);
  const [term, setTerm] = useState("");
  const [replacement, setReplacement] = useState("");
  const [aliases, setAliases] = useState("");
  const [dictKind, setDictKind] = useState<DictionaryEntry["kind"]>("replacement");
  const [dictQuery, setDictQuery] = useState("");
  const [dictImport, setDictImport] = useState("");
  const [snippets, setSnippets] = useState<Snippet[]>([]);
  const [snippetTrigger, setSnippetTrigger] = useState("");
  const [snippetContent, setSnippetContent] = useState("");
  const [profiles, setProfiles] = useState<Profile[]>([]);
  const [context, setContext] = useState<ResolvedContext | null>(null);
  const [suggestions, setSuggestions] = useState<LearnedCandidate[]>([]);
  const [correctionOriginal, setCorrectionOriginal] = useState("");
  const [correctionFixed, setCorrectionFixed] = useState("");
  const [editingHistoryId, setEditingHistoryId] = useState<string | null>(null);
  const [editingHistoryText, setEditingHistoryText] = useState("");
  const [configText, setConfigText] = useState("");
  const [modelMessage, setModelMessage] = useState("");
  const [modelStatus, setModelStatus] = useState<ModelInstallStatus[]>([]);
  const [downloadProgress, setDownloadProgress] = useState<Record<string, ModelDownloadProgress>>(
    {},
  );
  const [microphones, setMicrophones] = useState<AudioDevice[]>([]);
  const [stats, setStats] = useState<StatsSnapshot | null>(null);
  const [permissions, setPermissions] = useState<PermissionStatus | null>(null);
  const [historyQuery, setHistoryQuery] = useState("");
  const [historyApp, setHistoryApp] = useState("");
  const [historyRange, setHistoryRange] = useState<"all" | "today" | "7d">("all");
  const [diskUsage, setDiskUsage] = useState<DiskUsage | null>(null);
  const [lastUtteranceReady, setLastUtteranceReady] = useState(false);
  const autoSttDownload = useRef(false);

  async function refresh() {
    try {
      const [
        nextSettings,
        nextModels,
        nextDict,
        nextHistory,
        nextBuild,
        nextPrivacy,
        hotkey,
        nextStatus,
        nextSnippets,
        nextProfiles,
        nextContext,
        nextSuggestions,
      ] = await Promise.all([
        api.getSettings(),
        api.listModels(),
        api.listDictionary(),
        api.listHistory().catch(() => [] as HistoryItem[]),
        api.getBuildInfo(),
        api.privacySummary(),
        api.getHotkeyStatus(),
        api.listModelStatus(),
        api.listSnippets(),
        api.listProfiles(),
        api.getActiveContext(),
        api.listSuggestions(),
      ]);
      setSettings(nextSettings);
      setModels(nextModels);
      setModelStatus(nextStatus);
      setDictionary(nextDict);
      setHistory(nextHistory);
      setSnippets(nextSnippets);
      setProfiles(nextProfiles);
      setContext(nextContext);
      setSuggestions(nextSuggestions);
      setBuild(nextBuild);
      setPrivacy(nextPrivacy);
      try {
        setMicrophones(await api.listMicrophones());
        setStats(await api.getStats());
        setPermissions(await api.permissionStatus());
        setDiskUsage(await api.diskUsage());
        setLastUtteranceReady(await api.lastUtteranceReady().catch(() => false));
        const last = await api.getLastTranscript();
        if (last) {
          setPipelineOut(last);
        }
      } catch {
        /* preview */
      }
      if (hotkey.registered) {
        setStatus(
          `Hold ${hotkey.registered.replace("Control", "Ctrl").replace("Command", "⌘")}, speak, release.`,
        );
      } else if (hotkey.error) {
        setStatus(`Hotkey not registered: ${hotkey.error}`);
      }
      setView((current) => {
        if (!nextSettings.onboarding_complete) {
          return "onboarding";
        }
        if (current === "onboarding") {
          return "home";
        }
        return current;
      });
    } catch {
      setModels(bundledCatalog);
    }
  }

  useEffect(() => {
    void refresh();
  }, []);

  useEffect(() => {
    if (!isTauriRuntime()) {
      return;
    }
    let unpressed: (() => void) | undefined;
    let unreleased: (() => void) | undefined;
    void listen("hotkey-pressed", () => {
      setStatus("Recording… keep holding, then release to process.");
    }).then((fn) => {
      unpressed = fn;
    });
    void listen("hotkey-released", () => {
      setStatus("Processing recording…");
    }).then((fn) => {
      unreleased = fn;
    });
    let unprogress: (() => void) | undefined;
    void listen<ModelDownloadProgress>("model-download-progress", (event) => {
      const progress = event.payload;
      setDownloadProgress((current) => ({ ...current, [progress.model_id]: progress }));
    }).then((fn) => {
      unprogress = fn;
    });
    let undictation: (() => void) | undefined;
    void listen<DictationState>("dictation-state", (event) => {
      const payload = event.payload;
      setStatus(payload.message);
      if (payload.transcript) {
        setDraft(payload.transcript);
      }
      if (payload.insert_ok === false && payload.transcript) {
        void api.getLastTranscript().then((last) => {
          if (last) {
            setPipelineOut(last);
          }
        });
      }
    }).then((fn) => {
      undictation = fn;
    });
    return () => {
      unpressed?.();
      unreleased?.();
      unprogress?.();
      undictation?.();
    };
  }, []);

  useEffect(() => {
    if (
      (view !== "models" && view !== "onboarding" && view !== "home") ||
      !isTauriRuntime()
    ) {
      return;
    }
    let cancelled = false;
    async function pullStatus() {
      try {
        const next = await api.listModelStatus();
        if (!cancelled) {
          setModelStatus(next);
        }
        if (view === "models") {
          const ready = await api.lastUtteranceReady().catch(() => false);
          if (!cancelled) {
            setLastUtteranceReady(ready);
          }
        }
      } catch {
        /* status poll is best-effort */
      }
    }
    void pullStatus();
    const timer = window.setInterval(() => void pullStatus(), 1000);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [view]);

  useEffect(() => {
    if (!isTauriRuntime() || autoSttDownload.current || modelStatus.length === 0) {
      return;
    }
    const id = settings.active_stt_model;
    if (!id) {
      return;
    }
    const status = modelStatus.find((item) => item.model_id === id);
    if (status?.verified || status?.installed || status?.state === "downloading") {
      autoSttDownload.current = true;
      return;
    }
    autoSttDownload.current = true;
    void api.downloadModel(id).catch((error) => {
      autoSttDownload.current = false;
      setModelMessage(error instanceof Error ? error.message : String(error));
    });
  }, [settings.active_stt_model, modelStatus]);

  useEffect(() => {
    if ((view !== "settings" && view !== "onboarding") || !isTauriRuntime()) {
      return;
    }
    const timer = window.setInterval(() => {
      void api
        .listMicrophones()
        .then(setMicrophones)
        .catch(() => {
          /* device poll is best-effort */
        });
    }, 2500);
    return () => window.clearInterval(timer);
  }, [view]);

  useEffect(() => {
    if (view !== "settings" || !isTauriRuntime()) {
      return;
    }
    let cancelled = false;
    async function pullDisk() {
      try {
        const next = await api.diskUsage();
        if (!cancelled) {
          setDiskUsage(next);
        }
      } catch {
        /* disk probe is best-effort */
      }
    }
    void pullDisk();
    const timer = window.setInterval(() => void pullDisk(), 4000);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [view, settings.active_stt_model, settings.active_llm_model]);

  async function save(next: AppSettings) {
    const previous = settings;
    setSettings(next);
    try {
      await api.saveSettings(next);
    } catch (err) {
      setSettings(previous);
      setStatus(err instanceof Error ? err.message : String(err));
    }
  }

  if (view === "onboarding") {
    const sttId = settings.active_stt_model;
    const sttRecord = models.find((model) => model.model_id === sttId);
    const sttStatus = modelStatus.find((item) => item.model_id === sttId);
    const sttProgress = sttId ? downloadProgress[sttId] : undefined;
    const sttReady = Boolean(
      sttStatus &&
        (sttStatus.verified || sttStatus.state === "verified" || sttStatus.state === "installed") &&
        sttStatus.active,
    );
    const sttBusy =
      sttStatus?.state === "downloading" ||
      sttStatus?.state === "incomplete" ||
      sttProgress?.phase === "downloading" ||
      sttProgress?.phase === "verifying" ||
      sttProgress?.phase === "installing";
    const bytes = Math.max(sttStatus?.bytes_on_disk ?? 0, sttProgress?.bytes_downloaded ?? 0);
    const total = sttStatus?.expected_bytes || sttRecord?.size || sttProgress?.total_bytes || 0;
    const percent = total > 0 ? Math.min(100, Math.round((bytes / total) * 100)) : 0;
    return (
      <div className="min-h-screen bg-ink px-10 py-12 text-paper">
        <p className="text-copper tracking-[0.3em] text-xs uppercase">LocalFlow</p>
        <h1 className="mt-4 max-w-2xl text-5xl leading-tight">
          Speak. Release. Insert — entirely on this Mac.
        </h1>
        <ol className="mt-8 max-w-xl space-y-3 text-lg text-paper/80">
          <li>1. Allow Microphone and Accessibility (paste into other apps).</li>
          <li>2. Whisper Medium (~1.5 GB) downloads automatically on this screen (Hugging Face, checksum checked).</li>
          <li>3. Hold Control+Shift+Space over a text field, talk, release.</li>
        </ol>
        <div className="mt-8 flex flex-wrap gap-3">
          <button
            className="rounded-full border border-paper/30 px-4 py-2"
            onClick={() => void api.openPrivacyPane("microphone")}
          >
            Open Microphone settings
          </button>
          <button
            className="rounded-full border border-paper/30 px-4 py-2"
            onClick={() => void api.openPrivacyPane("accessibility")}
          >
            Open Accessibility settings
          </button>
        </div>
        <label className="mt-8 block max-w-xl text-sm text-paper/70">
          Microphone
          <select
            className="mt-1 w-full rounded-lg bg-paper/10 p-2 text-paper"
            value={settings.microphone_name ?? ""}
            onChange={(e) =>
              void save({
                ...settings,
                microphone_name: e.target.value === "" ? null : e.target.value,
              })
            }
          >
            <option value="">System default</option>
            {microphones.map((device) => (
              <option key={device.name} value={device.name}>
                {device.name}
                {device.is_default ? " (OS default)" : ""}
              </option>
            ))}
          </select>
        </label>
        <p className="mt-3 max-w-xl text-sm text-paper/50">
          {sttReady
            ? `${sttRecord?.display_name ?? "Whisper"} is installed and will be used for dictation.`
            : sttBusy
              ? `Downloading ${sttRecord?.display_name ?? "Whisper"} from Hugging Face… ${percent}%`
              : "Whisper will download automatically. You can continue and let it finish in the background."}
          {permissions
            ? ` Accessibility: ${permissions.accessibility_trusted ? "trusted" : "not trusted yet"}.`
            : ""}
        </p>
        {sttBusy && (
          <div className="mt-3 max-w-xl">
            <div className="h-2 overflow-hidden rounded-full bg-paper/10">
              <div className="h-full bg-copper" style={{ width: `${percent}%` }} />
            </div>
          </div>
        )}
        <button
          className="mt-10 rounded-full bg-copper px-6 py-3 text-ink"
          onClick={async () => {
            try {
              await api.completeOnboarding();
            } catch {
              /* preview */
            }
            setView("home");
          }}
        >
          Continue
        </button>
      </div>
    );
  }

  return (
    <div className="flex min-h-screen bg-ink text-paper">
      <aside className="w-56 border-r border-paper/10 px-5 py-8">
        <p className="text-copper text-xs tracking-[0.25em] uppercase">LocalFlow</p>
        <p className="mt-3 text-xs leading-relaxed text-paper/70">{status}</p>
        {!isTauriRuntime() && (
          <p className="mt-4 rounded-lg bg-copper/20 p-3 text-xs leading-relaxed text-paper">
            This browser tab cannot talk to Rust. Keep <code>npm run tauri dev</code> running and
            use the LocalFlow window (it should open itself).
          </p>
        )}
        <nav className="mt-8 space-y-1">
          {navItems(settings.ui_language).map((item) => (
            <button
              key={item.id}
              onClick={() => setView(item.id as ViewId)}
              className={`block w-full rounded-lg px-3 py-2 text-left ${
                view === item.id ? "bg-paper/10 text-paper" : "text-paper/60 hover:text-paper"
              }`}
            >
              {item.label}
            </button>
          ))}
        </nav>
      </aside>
      <main className="flex-1 px-10 py-8">
        {view === "home" && (
          <section>
            <h1 className="text-4xl">Dictation pipeline</h1>
            {(() => {
              const sttId = settings.active_stt_model;
              const sttRecord = models.find((model) => model.model_id === sttId);
              const sttStatus = modelStatus.find((item) => item.model_id === sttId);
              const sttProgress = sttId ? downloadProgress[sttId] : undefined;
              const sttReady =
                sttStatus &&
                (sttStatus.verified || sttStatus.state === "installed") &&
                sttStatus.active;
              if (sttReady) {
                return null;
              }
              const bytes = Math.max(
                sttStatus?.bytes_on_disk ?? 0,
                sttProgress?.bytes_downloaded ?? 0,
              );
              const total =
                sttStatus?.expected_bytes || sttRecord?.size || sttProgress?.total_bytes || 0;
              const percent = total > 0 ? Math.min(100, Math.round((bytes / total) * 100)) : 0;
              const busy =
                sttStatus?.state === "downloading" ||
                sttStatus?.state === "incomplete" ||
                sttProgress?.phase === "downloading";
              return (
                <p className="mt-3 rounded-xl border border-copper/40 bg-copper/10 px-4 py-3 text-sm">
                  {busy
                    ? `Downloading ${sttRecord?.display_name ?? "Whisper"} (${percent}%). Dictation starts when the checksum passes.`
                    : "Whisper is not ready. LocalFlow downloads it automatically — stay online, or open Models to retry."}
                  <button className="ml-3 underline" onClick={() => setView("models")}>
                    Open Models
                  </button>
                </p>
              );
            })()}
            {context && (
              <p className="mt-3 rounded-xl bg-paper/5 px-4 py-2 text-sm text-paper/80">
                Current app: {context.app_name || "—"} / Profile: {context.profile_name} (
                {context.style || context.mode}, {context.source})
              </p>
            )}
            <p className="mt-2 text-paper/70">{status}</p>
            <p className="mt-1 text-sm text-paper/50">
              Type a sample and click Process locally, or hold the hotkey over a field. Whisper.cpp
              transcribes when the model is installed. Escape cancels. Cmd+Ctrl+C/V copy or paste
              the last transcript.
            </p>
            <textarea
              className="mt-6 h-32 w-full rounded-2xl border border-paper/15 bg-paper/5 p-4"
              placeholder="Preview a transcript without the microphone"
              value={draft}
              onChange={(e) => setDraft(e.target.value)}
            />
            <button
              className="mt-4 rounded-full bg-copper px-5 py-2 text-ink"
              onClick={async () => {
                if (!draft.trim()) {
                  setStatus("Type a sample transcript first.");
                  return;
                }
                try {
                  const output = await api.processTranscript(draft);
                  setPipelineOut(output);
                  setDraft(output.final_text);
                  setStatus(`Formed: ${output.final_text}`);
                  setHistory(await api.listHistory());
                } catch (error) {
                  setPipelineOut(null);
                  setStatus(error instanceof Error ? error.message : String(error));
                }
              }}
            >
              Process locally
            </button>
            {pipelineOut && (
              <dl className="mt-6 space-y-2 rounded-2xl border border-paper/10 p-4 text-sm">
                <div>
                  <dt className="text-paper/50">Transcript</dt>
                  <dd>{pipelineOut.raw_transcript}</dd>
                </div>
                <div>
                  <dt className="text-paper/50">After dictionary</dt>
                  <dd>{pipelineOut.dictionary_text}</dd>
                </div>
                <div>
                  <dt className="text-paper/50">Formed text</dt>
                  <dd className="text-lg">{pipelineOut.final_text}</dd>
                </div>
                {pipelineOut.insert_ok === false && pipelineOut.final_text && (
                  <div className="flex flex-wrap gap-2 pt-2">
                    <p className="w-full text-copper">
                      Last insert failed. Copy or paste the text, then dismiss.
                    </p>
                    <button
                      className="rounded-full border border-paper/30 px-3 py-1"
                      onClick={() =>
                        void api.copyLastTranscript().then(() => setStatus("Copied last transcript."))
                      }
                    >
                      Copy
                    </button>
                    <button
                      className="rounded-full border border-paper/30 px-3 py-1"
                      onClick={() =>
                        void api
                          .pasteLastTranscript()
                          .then(() => setStatus("Pasted last transcript."))
                      }
                    >
                      Paste last
                    </button>
                    <button
                      className="rounded-full border border-paper/30 px-3 py-1"
                      onClick={() => {
                        void api.clearLastTranscript();
                        setPipelineOut(null);
                        setStatus("Dismissed last transcript.");
                      }}
                    >
                      Dismiss
                    </button>
                  </div>
                )}
              </dl>
            )}
          </section>
        )}

        {view === "settings" && (
          <section className="max-w-xl space-y-4">
            <h1 className="text-4xl">Settings</h1>
            {diskUsage && (
              <div
                className={`rounded-2xl border p-4 text-sm ${
                  diskUsage.enough_for_speech
                    ? "border-paper/15 bg-paper/5"
                    : "border-copper/50 bg-copper/10"
                }`}
              >
                <p className="text-xs uppercase tracking-[0.2em] text-copper">Disk space</p>
                {diskUsage.free_bytes != null && (
                  <p className="mt-2 text-lg tabular-nums">
                    {formatBytes(diskUsage.free_bytes)} free
                    {diskUsage.stt_still_needed_bytes > 0
                      ? ` · speech still needs ${formatBytes(diskUsage.stt_still_needed_bytes)}`
                      : " · speech model fits"}
                  </p>
                )}
                <ul className="mt-2 space-y-1 text-paper/80">
                  {diskUsage.messages.map((line) => (
                    <li key={line}>{line}</li>
                  ))}
                </ul>
              </div>
            )}
            <label className="block text-sm text-paper/70">
              Hotkey (Tauri syntax, e.g. Control+Shift+Space)
              <input
                className="mt-1 w-full rounded-lg bg-paper/10 p-2"
                value={settings.hotkey}
                onChange={(e) => void save({ ...settings, hotkey: e.target.value })}
              />
            </label>
            <p className="text-xs text-paper/60">
              Option+Space and Control+Space are often taken by macOS (Spotlight / input source).
              Check System Settings → Keyboard → Keyboard Shortcuts. Changing the hotkey here
              re-registers it immediately.
            </p>
            <label className="block text-sm text-paper/70">
              Speech language
              <select
                className="mt-1 w-full rounded-lg bg-paper/10 p-2"
                value={settings.stt_language}
                onChange={(e) => void save({ ...settings, stt_language: e.target.value })}
              >
                <option value="ru">Russian</option>
                <option value="en">English</option>
                <option value="auto">Auto-detect</option>
              </select>
            </label>
            <p className="text-xs text-paper/60">
              Russian is more accurate for Russian-only speech. Use Auto-detect when a replica
              mixes Russian and English so Whisper can switch language inside the clip.
            </p>
            <label className="block text-sm text-paper/70">
              Interface language
              <select
                className="mt-1 w-full rounded-lg bg-paper/10 p-2"
                value={settings.ui_language ?? "en"}
                onChange={(e) => void save({ ...settings, ui_language: e.target.value })}
              >
                <option value="en">English</option>
                <option value="ru">Русский</option>
              </select>
            </label>
            <label className="block text-sm text-paper/70">
              Microphone
              <select
                className="mt-1 w-full rounded-lg bg-paper/10 p-2"
                value={settings.microphone_name ?? ""}
                onChange={(e) =>
                  void save({
                    ...settings,
                    microphone_name: e.target.value === "" ? null : e.target.value,
                  })
                }
              >
                <option value="">System default</option>
                {microphones.map((device) => (
                  <option key={device.name} value={device.name}>
                    {device.name}
                    {device.is_default ? " (OS default)" : ""}
                  </option>
                ))}
              </select>
            </label>
            <button
              className="rounded-full border border-paper/30 px-4 py-2 text-sm"
              onClick={async () => {
                try {
                  setMicrophones(await api.listMicrophones());
                  setStatus("Microphone list refreshed.");
                } catch (error) {
                  setStatus(error instanceof Error ? error.message : String(error));
                }
              }}
            >
              Refresh devices
            </button>
            <div className="flex flex-wrap gap-2">
              <button
                className="rounded-full border border-paper/30 px-4 py-2 text-sm"
                onClick={() => void api.openPrivacyPane("microphone")}
              >
                Microphone permission
              </button>
              <button
                className="rounded-full border border-paper/30 px-4 py-2 text-sm"
                onClick={() => void api.openPrivacyPane("accessibility")}
              >
                Accessibility permission
              </button>
            </div>
            {permissions && (
              <p className="text-xs text-paper/50">
                {permissions.microphone_device_count} input device
                {permissions.microphone_device_count === 1 ? "" : "s"} visible. Accessibility{" "}
                {permissions.accessibility_trusted
                  ? "is trusted"
                  : "is not trusted — paste may fail"}
                .
              </p>
            )}
            <label className="flex items-center gap-2 text-sm">
              <input
                type="checkbox"
                checked={settings.autostart}
                onChange={(e) => void save({ ...settings, autostart: e.target.checked })}
              />
              Launch at login
            </label>
            <label className="flex items-center gap-2 text-sm">
              <input
                type="checkbox"
                checked={settings.history_enabled}
                onChange={(e) => void save({ ...settings, history_enabled: e.target.checked })}
              />
              Keep utterance history and JSONL journal
            </label>
            <label className="flex items-center gap-2 text-sm">
              <input
                type="checkbox"
                checked={settings.keep_last_audio ?? true}
                onChange={(e) => void save({ ...settings, keep_last_audio: e.target.checked })}
              />
              Keep last utterance WAV for Repeat
            </label>
            <button
              className="rounded-full border border-paper/30 px-4 py-2 text-sm"
              onClick={async () => {
                try {
                  const next = await api.resetSettings();
                  setSettings(next);
                  setStatus(
                    settings.ui_language === "ru"
                      ? "Настройки сброшены к значениям по умолчанию."
                      : "Settings restored to defaults.",
                  );
                } catch (err) {
                  setStatus(err instanceof Error ? err.message : String(err));
                }
              }}
            >
              {settings.ui_language === "ru" ? "Сбросить настройки" : "Reset settings to defaults"}
            </button>
            <label className="block text-sm text-paper/70">
              History size (oldest rows are dropped)
              <input
                className="mt-1 w-full rounded-lg bg-paper/10 p-2"
                type="number"
                min={50}
                max={10000}
                value={settings.history_max_items}
                onChange={(e) =>
                  void save({
                    ...settings,
                    history_max_items: Number(e.target.value) || 500,
                  })
                }
              />
            </label>
            <label className="block text-sm text-paper/70">
              Silence trim (VAD): {(settings.vad_threshold ?? 0.012).toFixed(3)}
              <input
                className="mt-1 w-full"
                type="range"
                min={0.002}
                max={0.08}
                step={0.001}
                value={settings.vad_threshold ?? 0.012}
                onChange={(e) =>
                  void save({ ...settings, vad_threshold: Number(e.target.value) })
                }
              />
              <span className="text-xs text-paper/50">
                Lower keeps quiet speech. Higher treats room noise as silence.
              </span>
            </label>
            <label className="block text-sm text-paper/70">
              Fallback mode (used when no app profile matches)
              <select
                className="mt-1 w-full rounded-lg bg-paper/10 p-2"
                value={settings.mode}
                onChange={(e) =>
                  void save({ ...settings, mode: e.target.value as AppSettings["mode"] })
                }
              >
                <option value="raw">Raw</option>
                <option value="normal">Normal</option>
                <option value="professional">Professional</option>
                <option value="code">Code</option>
              </select>
            </label>
            <label className="block text-sm text-paper/70">
              Profile override
              <select
                className="mt-1 w-full rounded-lg bg-paper/10 p-2"
                value={settings.profile_override ?? ""}
                onChange={(e) =>
                  void save({
                    ...settings,
                    profile_override: e.target.value === "" ? null : e.target.value,
                  })
                }
              >
                <option value="">Auto (frontmost app)</option>
                {profiles.map((profile) => (
                  <option key={profile.id} value={profile.id}>
                    {profile.name}
                  </option>
                ))}
              </select>
            </label>
            <label className="flex items-center gap-2 text-sm">
              <input
                type="checkbox"
                checked={settings.restore_clipboard}
                onChange={(e) => void save({ ...settings, restore_clipboard: e.target.checked })}
              />
              Restore clipboard after insert
            </label>
            <p className="text-xs text-paper/50">
              Keeps the previous pasteboard (text, RTF, images) after Cmd+V. If the app
              crashes mid-paste, the same snapshot is restored from disk. Password fields
              block paste — use Copy last after leaving the field.
            </p>
            <label className="flex items-center gap-2 text-sm">
              <input
                type="checkbox"
                checked={settings.show_flow_bar}
                onChange={(e) => void save({ ...settings, show_flow_bar: e.target.checked })}
              />
              Show LocalFlow Bar while listening
            </label>
            <label className="flex items-center gap-2 text-sm">
              <input
                type="checkbox"
                checked={settings.sound_cues}
                onChange={(e) => void save({ ...settings, sound_cues: e.target.checked })}
              />
              Play start/end recording sounds
            </label>
            <label className="block text-sm text-paper/70">
              Cue volume
              <input
                className="mt-1 w-full"
                type="range"
                min={0.05}
                max={1}
                step={0.05}
                value={settings.sound_cue_volume}
                onChange={(e) =>
                  void save({ ...settings, sound_cue_volume: Number(e.target.value) || 0.25 })
                }
              />
            </label>
            <label className="block text-sm text-paper/70">
              Pause before insert (ms)
              <input
                className="mt-1 w-full rounded-lg bg-paper/10 p-2"
                type="number"
                min={40}
                max={800}
                value={settings.insert_delay_ms}
                onChange={(e) =>
                  void save({ ...settings, insert_delay_ms: Number(e.target.value) || 120 })
                }
              />
            </label>
            <label className="flex items-center gap-2 text-sm">
              <input
                type="checkbox"
                checked={settings.hands_free}
                onChange={(e) => void save({ ...settings, hands_free: e.target.checked })}
              />
              Hands-free (press to start, press again to stop). Hold-to-talk stays the default when
              this is off.
            </label>
            <label className="flex items-center gap-2 text-sm">
              <input
                type="checkbox"
                checked={settings.digits_from_speech}
                onChange={(e) => void save({ ...settings, digits_from_speech: e.target.checked })}
              />
              Write spoken numbers as digits
            </label>
            <label className="block text-sm text-paper/70">
              Date format
              <select
                className="mt-1 w-full rounded-lg bg-paper/10 p-2"
                value={settings.date_format}
                onChange={(e) => void save({ ...settings, date_format: e.target.value })}
              >
                <option value="DMY">DD.MM.YYYY</option>
                <option value="ISO">YYYY-MM-DD</option>
              </select>
            </label>
            <label className="block text-sm text-paper/70">
              Acceleration device
              <select
                className="mt-1 w-full rounded-lg bg-paper/10 p-2"
                value={settings.compute_device}
                disabled
              >
                <option value="cpu">CPU (this build)</option>
              </select>
            </label>
            <label className="block text-sm text-paper/70">
              Post-processing timeout (ms)
              <input
                className="mt-1 w-full rounded-lg bg-paper/10 p-2"
                type="number"
                min={5000}
                max={180000}
                value={settings.postprocess_timeout_ms}
                onChange={(e) =>
                  void save({
                    ...settings,
                    postprocess_timeout_ms: Number(e.target.value) || 45000,
                  })
                }
              />
            </label>
            <button
              className="rounded-full border border-paper/30 px-4 py-2"
              onClick={async () => {
                const path = await api.installDictateMacro();
                setStatus(`Macro installed: ${path}. Double-click it to fire the talk hotkey.`);
              }}
            >
              Install Dictate macro
            </button>
            <label className="block text-sm text-paper/70">
              Copy last transcript
              <input
                className="mt-1 w-full rounded-lg bg-paper/10 p-2"
                value={settings.copy_last_hotkey}
                onChange={(e) => void save({ ...settings, copy_last_hotkey: e.target.value })}
              />
            </label>
            <label className="block text-sm text-paper/70">
              Paste last transcript
              <input
                className="mt-1 w-full rounded-lg bg-paper/10 p-2"
                value={settings.paste_last_hotkey}
                onChange={(e) => void save({ ...settings, paste_last_hotkey: e.target.value })}
              />
            </label>
            <label className="block text-sm text-paper/70">
              Edit selection (same hold-to-talk; paste replaces the highlight)
              <input
                className="mt-1 w-full rounded-lg bg-paper/10 p-2"
                value={settings.edit_hotkey ?? "Command+Control+E"}
                onChange={(e) => void save({ ...settings, edit_hotkey: e.target.value })}
              />
            </label>
            <div className="flex gap-3">
              <button
                className="rounded-full border border-paper/30 px-4 py-2"
                onClick={async () => {
                  try {
                    setStatus(`Copied: ${await api.copyLastTranscript()}`);
                  } catch (error) {
                    setStatus(error instanceof Error ? error.message : String(error));
                  }
                }}
              >
                Copy last
              </button>
              <button
                className="rounded-full border border-paper/30 px-4 py-2"
                onClick={async () => {
                  try {
                    setStatus(`Pasted: ${await api.pasteLastTranscript()}`);
                  } catch (error) {
                    setStatus(error instanceof Error ? error.message : String(error));
                  }
                }}
              >
                Paste last
              </button>
            </div>
            <div className="flex gap-3">
              <button
                className="rounded-full border border-paper/30 px-4 py-2"
                onClick={async () => setConfigText(await api.exportConfiguration())}
              >
                Export configuration
              </button>
              <button
                className="rounded-full border border-paper/30 px-4 py-2"
                onClick={async () => {
                  await api.importConfiguration(configText);
                  await refresh();
                }}
              >
                Import configuration
              </button>
            </div>
            <textarea
              className="h-40 w-full rounded-xl bg-paper/5 p-3 font-mono text-xs"
              value={configText}
              onChange={(e) => setConfigText(e.target.value)}
              placeholder="Exported JSON appears here"
            />
          </section>
        )}

        {view === "models" && (
          <section>
            <h1 className="text-4xl">Model Manager</h1>
            <p className="mt-2 max-w-2xl text-paper/70">
              Speech default is Whisper Medium. Download it (or pick another Whisper), then Use this
              for speech. Repeat re-runs the last recording through the speech model in use — it is
              not a separate download. Qwen models are only for text formatting, not speech.
            </p>
            {(() => {
              const speech = describeSelectedModel(
                settings.active_stt_model,
                models,
                modelStatus,
              );
              const formatting = describeSelectedModel(
                settings.active_llm_model,
                models,
                modelStatus,
              );
              return (
                <div className="mt-4 grid gap-3 rounded-2xl border border-copper/50 bg-copper/10 p-4 sm:grid-cols-2">
                  <div>
                    <p className="text-xs uppercase tracking-[0.2em] text-copper">
                      Currently in use · speech
                    </p>
                    <p className="mt-1 text-lg">{speech.name}</p>
                    {speech.version && (
                      <p className="mt-1 font-mono text-xs text-paper/50">{speech.version}</p>
                    )}
                    <p className={`mt-1 text-sm ${speech.ready ? "text-moss" : "text-copper"}`}>
                      {speech.detail}
                    </p>
                    {speech.id && (
                      <p className="mt-1 font-mono text-xs text-paper/50">{speech.id}</p>
                    )}
                    <button
                      className="mt-3 rounded-full bg-copper px-4 py-1 text-ink disabled:opacity-40"
                      disabled={!lastUtteranceReady || !speech.ready}
                      onClick={async () => {
                        try {
                          const output = await api.repeatLastUtterance();
                          setPipelineOut(output);
                          setLastUtteranceReady(true);
                          setModelMessage(
                            `Repeat finished with ${speech.name}: ${output.final_text || output.raw_transcript}`,
                          );
                          await refresh();
                        } catch (error) {
                          setModelMessage(
                            error instanceof Error ? error.message : String(error),
                          );
                        }
                      }}
                    >
                      Repeat last dictation
                    </button>
                    <p className="mt-2 text-xs text-paper/50">
                      {lastUtteranceReady
                        ? "Uses the last held-hotkey recording and the speech model currently in use."
                        : "Dictate once (hold the hotkey) to enable Repeat."}
                    </p>
                  </div>
                  <div>
                    <p className="text-xs uppercase tracking-[0.2em] text-copper">
                      Currently in use · formatting
                    </p>
                    <p className="mt-1 text-lg">{formatting.name}</p>
                    {formatting.version && (
                      <p className="mt-1 font-mono text-xs text-paper/50">{formatting.version}</p>
                    )}
                    <p className={`mt-1 text-sm ${formatting.ready ? "text-moss" : "text-paper/60"}`}>
                      {formatting.ready
                        ? formatting.detail
                        : `${formatting.detail} Dictation still works without it.`}
                    </p>
                    {formatting.id && (
                      <p className="mt-1 font-mono text-xs text-paper/50">{formatting.id}</p>
                    )}
                  </div>
                </div>
              );
            })()}
            <p className="mt-2 text-copper">{modelMessage}</p>
            <div className="mt-6 grid gap-4">
              {models.map((model) => {
                const status = modelStatus.find((item) => item.model_id === model.model_id);
                const progress = downloadProgress[model.model_id];
                const state = status?.state ?? "missing";
                const ready = state === "verified" || state === "installed";
                const isSpeechActive = settings.active_stt_model === model.model_id;
                const isFormattingActive = settings.active_llm_model === model.model_id;
                const isActive = isSpeechActive || isFormattingActive || Boolean(status?.active);
                const busy =
                  state === "downloading" ||
                  progress?.phase === "downloading" ||
                  progress?.phase === "verifying" ||
                  progress?.phase === "installing";
                const bytes = Math.max(status?.bytes_on_disk ?? 0, progress?.bytes_downloaded ?? 0);
                const total = status?.expected_bytes || model.size || progress?.total_bytes || 0;
                const percent = total > 0 ? Math.min(100, Math.round((bytes / total) * 100)) : 0;
                const roleLabel = isSpeechActive
                  ? "In use · speech"
                  : isFormattingActive
                    ? "In use · formatting"
                    : null;
                const badge = roleLabel
                  ? { label: roleLabel, className: "bg-copper text-ink" }
                  : ready
                    ? {
                        label: "Installed",
                        className: "bg-moss text-ink",
                      }
                    : state === "downloading"
                      ? { label: `Downloading ${percent}%`, className: "bg-copper text-ink" }
                      : state === "incomplete"
                        ? {
                            label: `Incomplete ${percent}%`,
                            className: "bg-copper/30 text-copper",
                          }
                        : state === "unverified"
                          ? { label: "Checksum failed", className: "bg-red-900 text-paper" }
                          : { label: "Not installed", className: "bg-paper/15 text-paper/70" };
                const buttonLabel = ready
                  ? "Re-download & verify"
                  : state === "incomplete" || state === "downloading"
                    ? "Resume download"
                    : "Download & install";
                return (
                  <article
                    key={model.model_id}
                    className={`rounded-2xl border p-5 ${
                      isActive ? "border-copper/70 bg-copper/5" : "border-paper/10"
                    }`}
                  >
                    <div className="flex items-baseline justify-between gap-4">
                      <h2 className="text-2xl">{model.display_name}</h2>
                      <span
                        className={`rounded-full px-3 py-1 text-xs font-semibold uppercase tracking-wide ${badge.className}`}
                      >
                        {badge.label}
                      </span>
                    </div>
                    <p className="mt-2 text-sm text-paper/70">
                      {model.kind === "stt" ? "Speech" : "Formatting"} · {model.version} ·{" "}
                      {model.format} {model.quantization} · {formatBytes(model.size)}
                      {model.model_id === "whisper-medium" ? " · recommended default" : ""}
                    </p>
                    {(state === "downloading" || state === "incomplete") && (
                      <div className="mt-3">
                        <div className="h-2 overflow-hidden rounded-full bg-paper/10">
                          <div className="h-full bg-copper" style={{ width: `${percent}%` }} />
                        </div>
                        <p className="mt-2 text-sm text-copper">
                          {formatBytes(bytes)} of {formatBytes(total)}
                        </p>
                      </div>
                    )}
                    {ready && (
                      <p className="mt-2 text-sm text-moss">
                        {isSpeechActive
                          ? "This is the speech model LocalFlow uses for dictation. "
                          : isFormattingActive
                            ? "This is the formatting model used after speech-to-text. "
                            : ""}
                        Ready at {status?.local_path}
                      </p>
                    )}
                    {isActive && !ready && (
                      <p className="mt-2 text-sm text-copper">
                        Selected as the current {isSpeechActive ? "speech" : "formatting"} model, but
                        the file is not ready yet.
                      </p>
                    )}
                    {status?.local_path && !ready && (
                      <p className="mt-1 break-all font-mono text-xs text-paper/50">
                        {status.local_path}
                      </p>
                    )}
                    <p className="mt-1 text-sm">License: {model.license}</p>
                    <p className="mt-1 text-sm">Source: {model.source}</p>
                    {model.notes && <p className="mt-2 text-sm text-paper/70">{model.notes}</p>}
                    <p className="mt-1 break-all font-mono text-xs text-paper/50">
                      SHA-256: {model.sha256}
                    </p>
                    <div className="mt-3 flex flex-wrap gap-3">
                      <a
                        className="text-copper underline"
                        href={model.license_url}
                        target="_blank"
                        rel="noreferrer"
                      >
                        View License
                      </a>
                      <button
                        className="rounded-full bg-moss px-4 py-1 text-ink disabled:opacity-40"
                        disabled={busy || !model.download_url}
                        onClick={async () => {
                          setModelMessage(`Network download started for ${model.display_name}`);
                          try {
                            const path = await api.downloadModel(model.model_id);
                            setModelMessage(`Installed and verified at ${path}`);
                            await refresh();
                          } catch (error) {
                            setModelMessage(error instanceof Error ? error.message : String(error));
                            try {
                              setModelStatus(await api.listModelStatus());
                            } catch {
                              /* keep last known status */
                            }
                          }
                        }}
                      >
                        {buttonLabel}
                      </button>
                      <button
                        className="text-paper/80 underline"
                        onClick={async () => {
                          try {
                            const path = await api.verifyModel(model.model_id);
                            setModelMessage(`Verified at ${path}`);
                            await refresh();
                          } catch (error) {
                            setModelMessage(error instanceof Error ? error.message : String(error));
                          }
                        }}
                      >
                        Verify local file
                      </button>
                      {ready && isActive && (
                        <button
                          className="rounded-full border border-copper px-4 py-1 text-copper"
                          disabled
                        >
                          Currently in use
                        </button>
                      )}
                      {!isActive && (
                        <button
                          className="rounded-full border border-paper/30 px-4 py-1"
                          onClick={async () => {
                            try {
                              await api.setActiveModel(model.model_id);
                              setModelMessage(
                                `${model.display_name} is now the ${
                                  model.kind === "llm" ? "formatting" : "speech"
                                } model in use.${ready ? "" : " Download it to start dictation."}`,
                              );
                              await refresh();
                            } catch (error) {
                              setModelMessage(
                                error instanceof Error ? error.message : String(error),
                              );
                            }
                          }}
                        >
                          Use this {model.kind === "llm" ? "for formatting" : "for speech"}
                        </button>
                      )}
                    </div>
                    {model.network_required_to_obtain && !ready && (
                      <p className="mt-3 text-xs uppercase tracking-wide text-copper">
                        Network required to download (Hugging Face)
                      </p>
                    )}
                  </article>
                );
              })}
            </div>
          </section>
        )}

        {view === "dictionary" && (
          <section className="max-w-2xl">
            <h1 className="text-4xl">Dictionary 2.0</h1>
            <p className="mt-2 text-sm text-paper/60">
              Vocabulary keeps a canonical term plus aliases. Replacement Rule maps spoken phrases
              to written text. Built-in developer terms (RestAssured, JUnit, …) are seeded
              automatically.
            </p>
            <input
              className="mt-4 w-full rounded-lg bg-paper/10 p-2"
              placeholder="Search canonical or alias"
              value={dictQuery}
              onChange={async (e) => {
                const query = e.target.value;
                setDictQuery(query);
                try {
                  setDictionary(await api.searchDictionary(query));
                } catch {
                  /* preview */
                }
              }}
            />
            <div className="mt-4 flex flex-wrap gap-2">
              <select
                className="rounded-lg bg-paper/10 p-2"
                value={dictKind}
                onChange={(e) => setDictKind(e.target.value as DictionaryEntry["kind"])}
              >
                <option value="replacement">Replacement Rule</option>
                <option value="vocabulary">Vocabulary</option>
              </select>
              <input
                className="flex-1 rounded-lg bg-paper/10 p-2"
                placeholder="canonical"
                value={replacement}
                onChange={(e) => setReplacement(e.target.value)}
              />
              <input
                className="flex-1 rounded-lg bg-paper/10 p-2"
                placeholder="primary spoken / alias"
                value={term}
                onChange={(e) => setTerm(e.target.value)}
              />
              <input
                className="w-full rounded-lg bg-paper/10 p-2"
                placeholder="extra aliases, comma-separated"
                value={aliases}
                onChange={(e) => setAliases(e.target.value)}
              />
              <button
                className="rounded-lg bg-moss px-3 py-2 text-ink"
                onClick={async () => {
                  const extra = aliases
                    .split(",")
                    .map((item) => item.trim())
                    .filter(Boolean);
                  const allAliases = [term, ...extra].filter(Boolean);
                  await api.upsertDictionary({
                    id: crypto.randomUUID(),
                    kind: dictKind,
                    canonical: replacement,
                    aliases: allAliases,
                    source: term,
                    replacement,
                    case_sensitive: false,
                    enabled: true,
                    builtin: false,
                  });
                  setDictionary(await api.searchDictionary(dictQuery));
                  setTerm("");
                  setReplacement("");
                  setAliases("");
                }}
              >
                Add
              </button>
            </div>
            <textarea
              className="mt-4 h-24 w-full rounded-lg bg-paper/10 p-2 text-sm"
              placeholder='Import JSON array: [{"canonical":"JUnit 5","aliases":["жюнит"],"kind":"vocabulary"}]'
              value={dictImport}
              onChange={(e) => setDictImport(e.target.value)}
            />
            <button
              className="mt-2 rounded-full border border-paper/30 px-4 py-2"
              onClick={async () => {
                try {
                  const count = await api.importDictionary(dictImport);
                  setStatus(`Imported ${count} dictionary entries.`);
                  setDictImport("");
                  setDictionary(await api.searchDictionary(dictQuery));
                } catch (error) {
                  setStatus(error instanceof Error ? error.message : String(error));
                }
              }}
            >
              Import
            </button>
            <ul className="mt-6 space-y-2">
              {dictionary.map((entry) => (
                <li key={entry.id} className="rounded-lg bg-paper/5 px-3 py-2">
                  <div className="flex justify-between gap-3">
                    <span>
                      <span className="text-xs uppercase tracking-wide text-copper">
                        {entry.kind}
                        {entry.builtin ? " · built-in" : ""}
                      </span>
                      <br />
                      {(entry.aliases.length ? entry.aliases : [entry.source]).join(" / ")} →{" "}
                      {entry.canonical || entry.replacement}
                    </span>
                    {!entry.builtin && (
                      <button
                        onClick={async () => {
                          await api.removeDictionary(entry.id);
                          setDictionary(await api.searchDictionary(dictQuery));
                        }}
                      >
                        Remove
                      </button>
                    )}
                  </div>
                </li>
              ))}
            </ul>
          </section>
        )}

        {view === "snippets" && (
          <section className="max-w-2xl">
            <h1 className="text-4xl">Snippets</h1>
            <p className="mt-2 text-sm text-paper/60">
              Exact trigger expands before the LLM. Priority: Command → Snippet → Dictionary.
            </p>
            <input
              className="mt-4 w-full rounded-lg bg-paper/10 p-2"
              placeholder="trigger (≤ 60)"
              value={snippetTrigger}
              onChange={(e) => setSnippetTrigger(e.target.value)}
            />
            <textarea
              className="mt-2 h-28 w-full rounded-lg bg-paper/10 p-2"
              placeholder="content (≤ 4000)"
              value={snippetContent}
              onChange={(e) => setSnippetContent(e.target.value)}
            />
            <button
              className="mt-3 rounded-lg bg-moss px-3 py-2 text-ink"
              onClick={async () => {
                await api.upsertSnippet({
                  id: crypto.randomUUID(),
                  trigger: snippetTrigger,
                  content: snippetContent,
                  language: "",
                  profile: "",
                  enabled: true,
                  created_at: "",
                  updated_at: "",
                });
                setSnippets(await api.listSnippets());
                setSnippetTrigger("");
                setSnippetContent("");
              }}
            >
              Add snippet
            </button>
            <ul className="mt-6 space-y-2">
              {snippets.map((snippet) => (
                <li key={snippet.id} className="rounded-lg bg-paper/5 p-3">
                  <div className="flex justify-between">
                    <p className="font-medium">{snippet.trigger}</p>
                    <button
                      onClick={async () => {
                        await api.removeSnippet(snippet.id);
                        setSnippets(await api.listSnippets());
                      }}
                    >
                      Remove
                    </button>
                  </div>
                  <pre className="mt-2 whitespace-pre-wrap text-sm text-paper/80">
                    {snippet.content}
                  </pre>
                </li>
              ))}
            </ul>
          </section>
        )}

        {view === "profiles" && (
          <section className="max-w-2xl space-y-4">
            <h1 className="text-4xl">Styles + application profiles</h1>
            {context && (
              <p className="rounded-xl bg-paper/5 px-4 py-2 text-sm">
                Current app: {context.app_name || "—"} / Profile: {context.profile_name}
              </p>
            )}
            <p className="text-sm text-paper/60">
              Resolver: exact app → group → profile → global fallback mode.
            </p>
            {profiles.map((profile, index) => (
              <article key={profile.id} className="rounded-2xl border border-paper/10 p-4">
                <h2 className="text-xl">{profile.name}</h2>
                <label className="mt-2 block text-sm text-paper/70">
                  Style
                  <select
                    className="mt-1 w-full rounded-lg bg-paper/10 p-2"
                    value={profile.style}
                    onChange={(e) => {
                      const next = profiles.map((item, i) =>
                        i === index ? { ...item, style: e.target.value } : item,
                      );
                      setProfiles(next);
                    }}
                  >
                    <option value="personal">Personal</option>
                    <option value="work">Work</option>
                    <option value="email">Email</option>
                    <option value="other">Other</option>
                  </select>
                </label>
                <label className="mt-2 block text-sm text-paper/70">
                  Mode
                  <select
                    className="mt-1 w-full rounded-lg bg-paper/10 p-2"
                    value={profile.mode}
                    onChange={(e) => {
                      const next = profiles.map((item, i) =>
                        i === index
                          ? { ...item, mode: e.target.value as AppSettings["mode"] }
                          : item,
                      );
                      setProfiles(next);
                    }}
                  >
                    <option value="raw">Raw</option>
                    <option value="normal">Normal</option>
                    <option value="professional">Professional</option>
                    <option value="code">Code</option>
                  </select>
                </label>
                <label className="mt-2 block text-sm text-paper/70">
                  Apps (comma-separated)
                  <input
                    className="mt-1 w-full rounded-lg bg-paper/10 p-2"
                    value={profile.apps.join(", ")}
                    onChange={(e) => {
                      const apps = e.target.value
                        .split(",")
                        .map((item) => item.trim())
                        .filter(Boolean);
                      const next = profiles.map((item, i) =>
                        i === index ? { ...item, apps } : item,
                      );
                      setProfiles(next);
                    }}
                  />
                </label>
              </article>
            ))}
            <button
              className="rounded-full bg-copper px-5 py-2 text-ink"
              onClick={async () => {
                await api.saveProfiles(profiles);
                setContext(await api.getActiveContext());
                setStatus("Profiles saved.");
              }}
            >
              Save profiles
            </button>
          </section>
        )}

        {view === "personalization" && (
          <section className="max-w-xl space-y-4">
            <h1 className="text-4xl">Personalization</h1>
            <label className="flex items-center gap-2 text-sm">
              <input
                type="checkbox"
                checked={settings.personalization_enabled}
                onChange={(e) =>
                  void save({ ...settings, personalization_enabled: e.target.checked })
                }
              />
              Personalization ON
            </label>
            <label className="flex items-center gap-2 text-sm">
              <input
                type="checkbox"
                checked={settings.learn_from_corrections}
                onChange={(e) =>
                  void save({ ...settings, learn_from_corrections: e.target.checked })
                }
              />
              Learn from corrections
            </label>
            <p className="text-sm text-paper/60">
              First correction is a candidate. Repeat it to get a suggestion. Accept writes a
              dictionary replacement rule.
            </p>
            <div className="flex gap-2">
              <input
                className="flex-1 rounded-lg bg-paper/10 p-2"
                placeholder="original"
                value={correctionOriginal}
                onChange={(e) => setCorrectionOriginal(e.target.value)}
              />
              <input
                className="flex-1 rounded-lg bg-paper/10 p-2"
                placeholder="corrected"
                value={correctionFixed}
                onChange={(e) => setCorrectionFixed(e.target.value)}
              />
              <button
                className="rounded-lg bg-moss px-3 text-ink"
                onClick={async () => {
                  const next = await api.recordCorrection(correctionOriginal, correctionFixed);
                  setSuggestions(next);
                  setCorrectionOriginal("");
                  setCorrectionFixed("");
                  setStatus("Correction recorded.");
                }}
              >
                Record
              </button>
            </div>
            <h2 className="text-xl">Suggestions</h2>
            <ul className="space-y-2">
              {suggestions.map((item) => (
                <li
                  key={item.id}
                  className="flex items-center justify-between rounded-lg bg-paper/5 px-3 py-2"
                >
                  <span>
                    {item.pattern} → {item.replacement} (×{item.weight})
                  </span>
                  <span className="flex gap-3">
                    <button
                      onClick={async () => {
                        await api.acceptSuggestion(item.id);
                        setSuggestions(await api.listSuggestions());
                        setDictionary(await api.listDictionary());
                      }}
                    >
                      Accept
                    </button>
                    <button
                      onClick={async () => {
                        await api.dismissSuggestion(item.id);
                        setSuggestions(await api.listSuggestions());
                      }}
                    >
                      Dismiss
                    </button>
                  </span>
                </li>
              ))}
            </ul>
            <button
              className="rounded-full border border-copper px-4 py-2 text-copper"
              onClick={async () => {
                await api.resetPersonalization();
                setSuggestions([]);
                setStatus("Personalization reset.");
              }}
            >
              Reset personalization
            </button>
          </section>
        )}

        {view === "history" && (
          <section>
            <div className="flex items-center justify-between">
              <h1 className="text-4xl">History</h1>
              <button
                className="rounded-full border border-paper/30 px-4 py-2"
                onClick={async () => {
                  await api.deleteHistory();
                  setHistory([]);
                }}
              >
                Delete history
              </button>
            </div>
            <div className="mt-4 flex flex-wrap gap-3">
              <input
                className="min-w-[12rem] flex-1 rounded-lg bg-paper/10 p-2 text-sm"
                placeholder="Search transcript or output"
                value={historyQuery}
                onChange={(e) => setHistoryQuery(e.target.value)}
              />
              <select
                className="rounded-lg bg-paper/10 p-2 text-sm"
                value={historyApp}
                onChange={(e) => setHistoryApp(e.target.value)}
              >
                <option value="">All apps</option>
                {[...new Set(history.map((item) => item.application).filter(Boolean))].map(
                  (app) => (
                    <option key={app} value={app}>
                      {app}
                    </option>
                  ),
                )}
              </select>
              <select
                className="rounded-lg bg-paper/10 p-2 text-sm"
                value={historyRange}
                onChange={(e) => setHistoryRange(e.target.value as "all" | "today" | "7d")}
              >
                <option value="all">All dates</option>
                <option value="today">Today</option>
                <option value="7d">Last 7 days</option>
              </select>
            </div>
            {["today", "earlier"].map((bucket) => {
              const filtered = history.filter((item) => {
                const q = historyQuery.trim().toLowerCase();
                if (
                  q &&
                  !`${item.transcript} ${item.output} ${item.application}`.toLowerCase().includes(q)
                ) {
                  return false;
                }
                if (historyApp && item.application !== historyApp) {
                  return false;
                }
                if (historyRange === "today" && !isToday(item.created_at)) {
                  return false;
                }
                if (historyRange === "7d" && !daysAgo(item.created_at, 7)) {
                  return false;
                }
                return true;
              });
              const items = filtered.filter((item) =>
                bucket === "today" ? isToday(item.created_at) : !isToday(item.created_at),
              );
              if (items.length === 0) {
                return null;
              }
              return (
                <div key={bucket} className="mt-6">
                  <h2 className="text-sm uppercase tracking-[0.2em] text-copper">
                    {bucket === "today" ? "Today" : "Earlier"}
                  </h2>
                  <ul className="mt-3 space-y-3">
                    {items.map((item) => (
                      <li key={item.id} className="rounded-xl bg-paper/5 p-4">
                        <p className="text-xs text-paper/50">
                          {item.created_at} · {item.application || "—"} · {item.profile || "—"} ·{" "}
                          {item.model || "—"} · {item.processing_time_ms} ms · {item.mode}
                        </p>
                        <p className="mt-1 text-xs text-paper/40">{item.transcript}</p>
                        {editingHistoryId === item.id ? (
                          <textarea
                            className="mt-2 h-24 w-full rounded-lg bg-paper/10 p-2"
                            value={editingHistoryText}
                            onChange={(e) => setEditingHistoryText(e.target.value)}
                          />
                        ) : (
                          <p className="mt-2 whitespace-pre-wrap">{item.output}</p>
                        )}
                        <div className="mt-3 flex flex-wrap gap-3 text-sm text-copper">
                          <button
                            onClick={() =>
                              void api.copyText(item.output).then(() => setStatus("Copied."))
                            }
                          >
                            Copy
                          </button>
                          <button
                            onClick={() =>
                              void api.pasteText(item.output).then(() => setStatus("Pasted."))
                            }
                          >
                            Paste
                          </button>
                          {editingHistoryId === item.id ? (
                            <button
                              onClick={async () => {
                                await api.updateHistoryOutput(item.id, editingHistoryText);
                                setEditingHistoryId(null);
                                setHistory(await api.listHistory());
                              }}
                            >
                              Save
                            </button>
                          ) : (
                            <button
                              onClick={() => {
                                setEditingHistoryId(item.id);
                                setEditingHistoryText(item.output);
                              }}
                            >
                              Edit
                            </button>
                          )}
                          <button
                            onClick={async () => {
                              const output = await api.retryHistory(item.transcript);
                              setStatus(`Retry: ${output.final_text}`);
                              setHistory(await api.listHistory());
                            }}
                          >
                            Retry
                          </button>
                          <button
                            onClick={async () => {
                              await api.historyToSnippet(
                                item.transcript.slice(0, 60) || item.output.slice(0, 60),
                                item.output,
                              );
                              setSnippets(await api.listSnippets());
                              setStatus("Saved as snippet.");
                            }}
                          >
                            Use as Snippet
                          </button>
                          <button
                            onClick={async () => {
                              await api.upsertDictionary({
                                id: crypto.randomUUID(),
                                kind: "replacement",
                                canonical: item.output,
                                aliases: [item.transcript],
                                source: item.transcript,
                                replacement: item.output,
                                case_sensitive: false,
                                enabled: true,
                                builtin: false,
                              });
                              setDictionary(await api.listDictionary());
                              setStatus("Added to dictionary.");
                            }}
                          >
                            Add to Dictionary
                          </button>
                          <button
                            onClick={async () => {
                              await api.deleteHistoryItem(item.id);
                              setHistory(await api.listHistory());
                            }}
                          >
                            Delete
                          </button>
                        </div>
                      </li>
                    ))}
                  </ul>
                </div>
              );
            })}
          </section>
        )}

        {view === "diagnostics" && build && (
          <section className="space-y-2 font-mono text-sm">
            <h1 className="font-serif text-4xl">Diagnostics</h1>
            <p>
              Application: {build.application} {build.version}
            </p>
            <p>Build: {build.git_sha}</p>
            <p>Platform: {build.platform}</p>
            <p>Architecture: {build.architecture}</p>
            <p>Build date: {build.build_date}</p>
            <p>Tauri: {build.tauri_version}</p>
            <p>Rust: {build.rustc_version}</p>
            <p>Native runtime: {build.native_runtime}</p>
            {permissions && (
              <p>
                Permissions: mic devices={permissions.microphone_device_count}, accessibility=
                {String(permissions.accessibility_trusted)}
              </p>
            )}
            {stats && (
              <div className="mt-4 space-y-1 text-paper/80">
                <p>
                  WPM last / best / today avg: {stats.last_wpm.toFixed(0)} /{" "}
                  {stats.wpm_best.toFixed(0)} / {stats.wpm_avg_today.toFixed(0)}
                </p>
                <p>
                  Words today / all: {stats.words_today} / {stats.words_total}
                </p>
                <p>Utterances in journal: {stats.recordings}</p>
                {stats.wpm_by_application.length > 0 && (
                  <div className="pt-2">
                    <p className="text-paper/60">WPM by application</p>
                    {stats.wpm_by_application.map((row) => (
                      <p key={row.application}>
                        {row.application || "(unknown)"}: {row.wpm_avg.toFixed(0)} (
                        {row.utterances})
                      </p>
                    ))}
                  </div>
                )}
                <button
                  className="mt-2 rounded-full border border-paper/30 px-4 py-2 font-sans"
                  onClick={async () => {
                    const csv = await api.exportStatsCsv();
                    setConfigText(csv);
                    setStatus("Statistics CSV copied into the settings export box.");
                    setView("settings");
                  }}
                >
                  Export stats CSV
                </button>
              </div>
            )}
          </section>
        )}

        {view === "privacy" && privacy && (
          <section className="max-w-xl space-y-3">
            <h1 className="text-4xl">Privacy</h1>
            <p>Core pipeline is local. Cloud accounts are not required.</p>
            <ul className="list-disc pl-5 text-paper/80">
              <li>Audio → local: {String(privacy.audio_local)}</li>
              <li>STT → local: {String(privacy.stt_local)}</li>
              <li>LLM → local: {String(privacy.llm_local)}</li>
              <li>Data root: {privacy.data_root}</li>
            </ul>
            <p className="text-copper">Network is used only for:</p>
            {privacy.network_operations.map((item) => (
              <p key={item}>{item}</p>
            ))}
            <p className="text-sm text-paper/70">
              Audio cache uses a private 0700 folder. Logs rotate by size and never store tokens.
            </p>
            <div className="flex flex-wrap gap-3 pt-2">
              <button
                className="rounded-full border border-paper/30 px-4 py-2"
                onClick={async () => {
                  const srt = await api.exportHistoryTimecodes();
                  setConfigText(srt);
                  setStatus("History exported with timecodes.");
                  setView("settings");
                }}
              >
                Export history with timecodes
              </button>
              <button
                className="rounded-full border border-paper/30 px-4 py-2"
                onClick={async () => {
                  await api.resetStats();
                  setStatus("Statistics reset.");
                }}
              >
                Reset statistics
              </button>
              <button
                className="rounded-full border border-paper/30 px-4 py-2"
                onClick={async () => {
                  if (!window.confirm("Uninstall LocalFlow data? You can keep history.")) {
                    return;
                  }
                  const keep = window.confirm("Keep dictation history?");
                  const report = await api.uninstallLocalflow(keep);
                  setStatus(
                    `Removed:\n${report.removed.join("\n")}\nSkipped:\n${report.skipped.join("\n")}`,
                  );
                }}
              >
                Uninstall…
              </button>
            </div>
          </section>
        )}
      </main>
    </div>
  );
}
