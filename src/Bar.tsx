import { useEffect, useState } from "react";
import { api, isTauriRuntime, type DictationState } from "./api";
import { listen } from "@tauri-apps/api/event";
import { copy, hostKindFromUa, showMacOnlyControls } from "./ui";

export function Bar() {
  const [state, setState] = useState<DictationState>({
    phase: "idle",
    message: "Listening…",
    transcript: null,
    raw_transcript: null,
    duration_ms: 0,
    rms: 0,
    wpm: null,
  });
  const [lang, setLang] = useState("en");

  const [needsAccess, setNeedsAccess] = useState(false);
  const host = hostKindFromUa();
  const t = copy(lang, host);
  const preview = state.transcript ?? state.message;
  const recording = state.phase === "recording" || state.phase === "pressed";
  const busy = recording || state.phase === "processing";
  const rms = state.rms ?? 0;
  const failed = state.insert_ok === false && Boolean(state.transcript || state.phase === "error");

  useEffect(() => {
    if (!isTauriRuntime()) {
      return;
    }
    void api
      .getSettings()
      .then((settings) => setLang(settings.ui_language))
      .catch(() => undefined);
    let unlisten: (() => void) | undefined;
    void listen<DictationState>("dictation-state", (event) => {
      setState(event.payload);
    }).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, []);

  useEffect(() => {
    if (!failed || !isTauriRuntime()) {
      return;
    }
    let cancelled = false;
    void api
      .permissionStatus()
      .then((status) => {
        if (!cancelled) {
          setNeedsAccess(showMacOnlyControls(host) && !status.accessibility_trusted);
        }
      })
      .catch(() => {
        if (!cancelled) {
          setNeedsAccess(false);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [failed, host]);

  return (
    <div
      className="flex h-screen flex-col justify-between rounded-2xl border border-paper/20 bg-ink/95 px-4 py-3 text-paper shadow-xl"
      data-tauri-drag-region
    >
      <div className="flex items-center justify-between gap-3">
        <p className="text-xs uppercase tracking-[0.2em] text-copper">
          {recording
            ? t.barListening
            : failed
              ? t.barNotPasted
              : state.phase === "error"
                ? t.barFailed
                : state.phase}
        </p>
        <span className="flex items-center gap-2">
          {state.wpm ? (
            <span className="text-[10px] tabular-nums text-paper/60">
              {state.wpm.toFixed(0)} wpm
            </span>
          ) : null}
          <span className={`h-3 w-3 rounded-full bg-copper ${recording ? "animate-pulse" : ""}`} />
        </span>
      </div>
      {recording && (
        <div className="flex h-8 items-end gap-1">
          {[0, 1, 2, 3, 4, 5, 6].map((bar) => {
            const height = Math.max(4, Math.min(28, rms * (90 + bar * 18)));
            return (
              <span
                key={bar}
                className="w-1.5 rounded-full bg-copper/90"
                style={{ height: `${height}px` }}
              />
            );
          })}
        </div>
      )}
      <p className="line-clamp-3 text-sm text-paper/85">{preview}</p>
      {failed && (
        <p className="text-[11px] text-paper/60">
          {needsAccess ? t.barAccessHint : t.barPasteHint}
        </p>
      )}
      <div className="flex justify-end gap-2">
        {failed && (
          <>
            {needsAccess && (
              <button
                className="rounded-full border border-paper/30 px-3 py-1 text-xs"
                onClick={() => void api.openPrivacyPane("accessibility")}
              >
                {t.accessPermission}
              </button>
            )}
            {needsAccess && (
              <button
                className="rounded-full border border-paper/30 px-3 py-1 text-xs"
                onClick={() => void api.relaunchApp()}
              >
                {t.quitRelaunchAccess}
              </button>
            )}
            <button
              className="rounded-full border border-paper/30 px-3 py-1 text-xs"
              onClick={() => void api.copyLastTranscript()}
            >
              {t.copyLast}
            </button>
            <button
              className="rounded-full border border-paper/30 px-3 py-1 text-xs"
              onClick={() => void api.pasteLastTranscript()}
            >
              {t.pasteLast}
            </button>
            <button
              className="rounded-full border border-paper/30 px-3 py-1 text-xs"
              onClick={() => {
                void api.clearLastTranscript();
                void api.dictationCancel();
              }}
            >
              {t.barDismiss}
            </button>
          </>
        )}
        <button
          className="rounded-full border border-paper/30 px-3 py-1 text-xs"
          onClick={() => void api.dictationCancel()}
        >
          {t.barCancel}
        </button>
        <button
          className="rounded-full bg-copper px-3 py-1 text-xs text-ink disabled:opacity-40"
          disabled={!busy}
          onClick={() => void api.dictationStop()}
        >
          {t.barStop}
        </button>
      </div>
    </div>
  );
}
