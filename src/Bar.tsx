import { useEffect, useState } from "react";
import { api, isTauriRuntime, type DictationState } from "./api";
import { listen } from "@tauri-apps/api/event";

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

  useEffect(() => {
    if (!isTauriRuntime()) {
      return;
    }
    let unlisten: (() => void) | undefined;
    void listen<DictationState>("dictation-state", (event) => {
      setState(event.payload);
    }).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, []);

  const preview = state.transcript ?? state.message;
  const recording = state.phase === "recording" || state.phase === "pressed";
  const busy = recording || state.phase === "processing";
  const rms = state.rms ?? 0;
  const failed = state.insert_ok === false && (state.transcript || state.phase === "error");

  return (
    <div
      className="flex h-screen flex-col justify-between rounded-2xl border border-paper/20 bg-ink/95 px-4 py-3 text-paper shadow-xl"
      data-tauri-drag-region
    >
      <div className="flex items-center justify-between gap-3">
        <p className="text-xs uppercase tracking-[0.2em] text-copper">
          {recording ? "Listening" : state.phase}
        </p>
        <span className="flex items-center gap-2">
          {state.wpm ? (
            <span className="text-[10px] tabular-nums text-paper/60">{state.wpm.toFixed(0)} wpm</span>
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
      <div className="flex justify-end gap-2">
        {failed && (
          <>
            <button
              className="rounded-full border border-paper/30 px-3 py-1 text-xs"
              onClick={() => void api.copyLastTranscript()}
            >
              Copy
            </button>
            <button
              className="rounded-full border border-paper/30 px-3 py-1 text-xs"
              onClick={() => void api.pasteLastTranscript()}
            >
              Paste last
            </button>
            <button
              className="rounded-full border border-paper/30 px-3 py-1 text-xs"
              onClick={() => {
                void api.clearLastTranscript();
                void api.dictationCancel();
              }}
            >
              Dismiss
            </button>
          </>
        )}
        <button
          className="rounded-full border border-paper/30 px-3 py-1 text-xs"
          onClick={() => void api.dictationCancel()}
        >
          Cancel
        </button>
        <button
          className="rounded-full bg-copper px-3 py-1 text-xs text-ink disabled:opacity-40"
          disabled={!busy}
          onClick={() => void api.dictationStop()}
        >
          Stop
        </button>
      </div>
    </div>
  );
}
