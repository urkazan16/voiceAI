import { useEffect, useState } from "react";
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
  type ViewId,
} from "./api";
import { formatBytes, NAV } from "./ui";
import { listen } from "@tauri-apps/api/event";

const fallbackSettings = (): AppSettings => ({
  hotkey: "Control+Shift+Space",
  mode: "normal",
  microphone_name: null,
  active_stt_model: "whisper-small",
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
  log_max_bytes: 2097152,
  autostart: false,
  history_enabled: true,
});

function isToday(iso: string): boolean {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) {
    return false;
  }
  const now = new Date();
  return date.toDateString() === now.toDateString();
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
        setPipelineOut(null);
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
    if (view !== "models" || !isTauriRuntime()) {
      return;
    }
    let cancelled = false;
    async function pullStatus() {
      try {
        const next = await api.listModelStatus();
        if (!cancelled) {
          setModelStatus(next);
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

  async function save(next: AppSettings) {
    setSettings(next);
    try {
      await api.saveSettings(next);
    } catch {
      setStatus("Settings saved locally in this preview session.");
    }
  }

  if (view === "onboarding") {
    const sttReady = modelStatus.some((item) => {
      const record = models.find((model) => model.model_id === item.model_id);
      return record?.kind === "stt" && item.active && item.verified;
    });
    return (
      <div className="min-h-screen bg-ink px-10 py-12 text-paper">
        <p className="text-copper tracking-[0.3em] text-xs uppercase">LocalFlow</p>
        <h1 className="mt-4 max-w-2xl text-5xl leading-tight">
          Speak. Release. Insert — entirely on this Mac.
        </h1>
        <ol className="mt-8 max-w-xl space-y-3 text-lg text-paper/80">
          <li>1. Allow Microphone and Accessibility (paste into other apps).</li>
          <li>2. Download Whisper in Models, then Set as active.</li>
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
            ? "Whisper is installed and active."
            : "No verified Whisper model yet. Continue, then open Models — dictation will prompt you."}
          {permissions
            ? ` Accessibility: ${permissions.accessibility_trusted ? "trusted" : "not trusted yet"}.`
            : ""}
        </p>
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
          {NAV.map((item) => (
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
              const sttReady = modelStatus.some((item) => {
                const record = models.find((model) => model.model_id === item.model_id);
                return record?.kind === "stt" && item.active && item.verified;
              });
              if (sttReady) {
                return null;
              }
              return (
                <p className="mt-3 rounded-xl border border-copper/40 bg-copper/10 px-4 py-3 text-sm">
                  Whisper is not ready. Open Models, download Medium or Small, then Set as active.
                  Until then, dictation may fall back to macOS Speech Recognition.
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
              </dl>
            )}
          </section>
        )}

        {view === "settings" && (
          <section className="max-w-xl space-y-4">
            <h1 className="text-4xl">Settings</h1>
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
              Russian is more accurate for Russian speech. Auto-detect can slip into Ukrainian or
              English and mangle similar-sounding words.
            </p>
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
              For Russian dictation download Whisper Medium, then Set as active. Small is faster but
              weaker. Large v3 Turbo is a similar download to Medium and usually quicker, with a bit
              less precision. Qwen models are only for text formatting, not speech.
            </p>
            <p className="mt-2 text-copper">{modelMessage}</p>
            <div className="mt-6 grid gap-4">
              {models.map((model) => {
                const status = modelStatus.find((item) => item.model_id === model.model_id);
                const progress = downloadProgress[model.model_id];
                const state = status?.state ?? "missing";
                const ready = state === "verified" || state === "installed";
                const busy =
                  state === "downloading" ||
                  progress?.phase === "downloading" ||
                  progress?.phase === "verifying" ||
                  progress?.phase === "installing";
                const bytes = Math.max(status?.bytes_on_disk ?? 0, progress?.bytes_downloaded ?? 0);
                const total = status?.expected_bytes || model.size || progress?.total_bytes || 0;
                const percent = total > 0 ? Math.min(100, Math.round((bytes / total) * 100)) : 0;
                const badge = ready
                  ? {
                      label: status?.active ? "Installed · Active" : "Installed",
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
                  <article key={model.model_id} className="rounded-2xl border border-paper/10 p-5">
                    <div className="flex items-baseline justify-between gap-4">
                      <h2 className="text-2xl">{model.display_name}</h2>
                      <span
                        className={`rounded-full px-3 py-1 text-xs font-semibold uppercase tracking-wide ${badge.className}`}
                      >
                        {badge.label}
                      </span>
                    </div>
                    <p className="mt-2 text-sm text-paper/70">
                      {model.format} {model.quantization} · {formatBytes(model.size)}
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
                        {status?.active ? "Active model. " : ""}
                        Ready at {status?.local_path}
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
                      {ready && !status?.active && (
                        <button
                          className="text-paper/80 underline"
                          onClick={async () => {
                            try {
                              await api.setActiveModel(model.model_id);
                              setModelMessage(`${model.display_name} is now the active model.`);
                              await refresh();
                            } catch (error) {
                              setModelMessage(
                                error instanceof Error ? error.message : String(error),
                              );
                            }
                          }}
                        >
                          Set as active
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
            {["today", "earlier"].map((bucket) => {
              const items = history.filter((item) =>
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
