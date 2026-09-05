import { useEffect, useState } from "react";
import {
  api,
  type AppSettings,
  type BuildInfo,
  type DictionaryEntry,
  type HistoryItem,
  type ModelRecord,
  type PrivacySummary,
  type ViewId,
} from "./api";
import { formatBytes, NAV } from "./ui";

const fallbackSettings = (): AppSettings => ({
  hotkey: "Alt+Space",
  mode: "normal",
  microphone_name: null,
  active_stt_model: "whisper-small",
  active_llm_model: "Qwen3-4B-Instruct-2507",
  restore_clipboard: true,
  onboarding_complete: false,
});

export function App() {
  const [view, setView] = useState<ViewId>("onboarding");
  const [settings, setSettings] = useState<AppSettings>(fallbackSettings);
  const [models, setModels] = useState<ModelRecord[]>([]);
  const [dictionary, setDictionary] = useState<DictionaryEntry[]>([]);
  const [history, setHistory] = useState<HistoryItem[]>([]);
  const [build, setBuild] = useState<BuildInfo | null>(null);
  const [privacy, setPrivacy] = useState<PrivacySummary | null>(null);
  const [status, setStatus] = useState("Hold Option+Space, speak, release.");
  const [draft, setDraft] = useState("");
  const [term, setTerm] = useState("");
  const [replacement, setReplacement] = useState("");
  const [configText, setConfigText] = useState("");
  const [modelMessage, setModelMessage] = useState("");

  async function refresh() {
    try {
      const [nextSettings, nextModels, nextDict, nextHistory, nextBuild, nextPrivacy] =
        await Promise.all([
          api.getSettings(),
          api.listModels(),
          api.listDictionary(),
          api.listHistory(),
          api.getBuildInfo(),
          api.privacySummary(),
        ]);
      setSettings(nextSettings);
      setModels(nextModels);
      setDictionary(nextDict);
      setHistory(nextHistory);
      setBuild(nextBuild);
      setPrivacy(nextPrivacy);
      setView(nextSettings.onboarding_complete ? "home" : "onboarding");
    } catch {
      setView("home");
    }
  }

  useEffect(() => {
    void refresh();
  }, []);

  async function save(next: AppSettings) {
    setSettings(next);
    try {
      await api.saveSettings(next);
    } catch {
      setStatus("Settings saved locally in this preview session.");
    }
  }

  if (view === "onboarding") {
    return (
      <div className="min-h-screen bg-ink px-10 py-12 text-paper">
        <p className="text-copper tracking-[0.3em] text-xs uppercase">LocalFlow</p>
        <h1 className="mt-4 max-w-2xl text-5xl leading-tight">
          Speak. Release. Insert — entirely on this Mac.
        </h1>
        <ol className="mt-8 max-w-xl space-y-3 text-lg text-paper/80">
          <li>1. Grant Microphone and Accessibility permissions.</li>
          <li>2. Download Whisper and Qwen models in Model Manager (network, user-initiated).</li>
          <li>3. Hold Option+Space, talk, release. Text is processed locally and pasted.</li>
        </ol>
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
            <p className="mt-2 text-paper/70">{status}</p>
            <textarea
              className="mt-6 h-32 w-full rounded-2xl border border-paper/15 bg-paper/5 p-4"
              placeholder="Preview a transcript without the microphone"
              value={draft}
              onChange={(e) => setDraft(e.target.value)}
            />
            <button
              className="mt-4 rounded-full bg-copper px-5 py-2 text-ink"
              onClick={async () => {
                try {
                  await api.processTranscript(draft);
                  setStatus("Processed locally and recorded in history.");
                  setHistory(await api.listHistory());
                } catch (error) {
                  setStatus(String(error));
                }
              }}
            >
              Process locally
            </button>
          </section>
        )}

        {view === "settings" && (
          <section className="max-w-xl space-y-4">
            <h1 className="text-4xl">Settings</h1>
            <label className="block text-sm text-paper/70">
              Hotkey
              <input
                className="mt-1 w-full rounded-lg bg-paper/10 p-2"
                value={settings.hotkey}
                onChange={(e) => void save({ ...settings, hotkey: e.target.value })}
              />
            </label>
            <label className="block text-sm text-paper/70">
              Mode
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
            <label className="flex items-center gap-2 text-sm">
              <input
                type="checkbox"
                checked={settings.restore_clipboard}
                onChange={(e) => void save({ ...settings, restore_clipboard: e.target.checked })}
              />
              Restore clipboard after insert
            </label>
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
            <p className="mt-2 text-paper/70">
              Models are not bundled. Download is a visible network action. Activation requires
              SHA-256.
            </p>
            <p className="mt-2 text-copper">{modelMessage}</p>
            <div className="mt-6 grid gap-4">
              {models.map((model) => (
                <article key={model.model_id} className="rounded-2xl border border-paper/10 p-5">
                  <div className="flex items-baseline justify-between gap-4">
                    <h2 className="text-2xl">{model.display_name}</h2>
                    <span className="text-copper text-sm">{model.kind.toUpperCase()}</span>
                  </div>
                  <p className="mt-2 text-sm text-paper/70">
                    {model.format} {model.quantization} · {formatBytes(model.size)}
                  </p>
                  <p className="mt-1 text-sm">License: {model.license}</p>
                  <p className="mt-1 text-sm">Source: {model.source}</p>
                  <p className="mt-1 break-all font-mono text-xs text-paper/50">
                    SHA-256: {model.sha256}
                  </p>
                  <div className="mt-3 flex gap-3">
                    <a
                      className="text-copper underline"
                      href={model.license_url}
                      target="_blank"
                      rel="noreferrer"
                    >
                      View License
                    </a>
                    <button
                      className="text-paper/80 underline"
                      onClick={async () => {
                        try {
                          const path = await api.verifyModel(model.model_id);
                          setModelMessage(`Verified at ${path}`);
                        } catch (error) {
                          setModelMessage(String(error));
                        }
                      }}
                    >
                      Verify local file
                    </button>
                  </div>
                  {model.network_required_to_obtain && (
                    <p className="mt-3 text-xs uppercase tracking-wide text-copper">
                      Network required to download
                    </p>
                  )}
                </article>
              ))}
            </div>
          </section>
        )}

        {view === "dictionary" && (
          <section className="max-w-xl">
            <h1 className="text-4xl">Dictionary</h1>
            <div className="mt-4 flex gap-2">
              <input
                className="flex-1 rounded-lg bg-paper/10 p-2"
                placeholder="spoken term"
                value={term}
                onChange={(e) => setTerm(e.target.value)}
              />
              <input
                className="flex-1 rounded-lg bg-paper/10 p-2"
                placeholder="replacement"
                value={replacement}
                onChange={(e) => setReplacement(e.target.value)}
              />
              <button
                className="rounded-lg bg-moss px-3 text-ink"
                onClick={async () => {
                  await api.upsertDictionary({
                    id: crypto.randomUUID(),
                    source: term,
                    replacement,
                    case_sensitive: false,
                  });
                  setDictionary(await api.listDictionary());
                  setTerm("");
                  setReplacement("");
                }}
              >
                Add
              </button>
            </div>
            <ul className="mt-6 space-y-2">
              {dictionary.map((entry) => (
                <li key={entry.id} className="flex justify-between rounded-lg bg-paper/5 px-3 py-2">
                  <span>
                    {entry.source} → {entry.replacement}
                  </span>
                  <button
                    onClick={async () => {
                      await api.removeDictionary(entry.id);
                      setDictionary(await api.listDictionary());
                    }}
                  >
                    Remove
                  </button>
                </li>
              ))}
            </ul>
          </section>
        )}

        {view === "profiles" && (
          <section>
            <h1 className="text-4xl">Profiles</h1>
            <p className="mt-2 text-paper/70">
              Default profile follows the mode in Settings and the shared dictionary.
            </p>
          </section>
        )}

        {view === "personalization" && (
          <section>
            <h1 className="text-4xl">Personalization</h1>
            <p className="mt-2 max-w-xl text-paper/70">
              Accepted corrections become learned candidates. Reset removes correction events,
              learned candidates, and accepted inferred preferences.
            </p>
            <button
              className="mt-6 rounded-full border border-copper px-4 py-2 text-copper"
              onClick={async () => {
                await api.resetPersonalization();
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
            <ul className="mt-6 space-y-3">
              {history.map((item) => (
                <li key={item.id} className="rounded-xl bg-paper/5 p-4">
                  <p className="text-xs text-paper/50">
                    {item.created_at} · {item.mode}
                  </p>
                  <p className="mt-2">{item.output}</p>
                </li>
              ))}
            </ul>
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
          </section>
        )}
      </main>
    </div>
  );
}
