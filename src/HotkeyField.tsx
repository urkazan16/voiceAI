import { useEffect, useRef, useState } from "react";
import { chordFromKeyboardEvent, isFnKey, normalizeChord } from "./hotkey";
import { isTauriRuntime, listenWhileMounted } from "./api";

type HotkeyFieldProps = {
  label: string;
  value: string;
  listeningLabel: string;
  onChange: (chord: string) => void;
  onListeningChange?: (listening: boolean) => void;
  presets?: string[];
  error?: string | null;
  hint?: string;
};

export function HotkeyField({
  label,
  value,
  listeningLabel,
  onChange,
  onListeningChange,
  presets = [],
  error,
  hint,
}: HotkeyFieldProps) {
  const [listening, setListening] = useState(false);
  const onChangeRef = useRef(onChange);
  const buttonRef = useRef<HTMLButtonElement | null>(null);

  useEffect(() => {
    onChangeRef.current = onChange;
  }, [onChange]);

  useEffect(() => {
    onListeningChange?.(listening);
  }, [listening, onListeningChange]);

  useEffect(() => {
    if (!listening) {
      return;
    }
    buttonRef.current?.focus();
    const commit = (event: KeyboardEvent) => {
      event.preventDefault();
      event.stopPropagation();
      if (event.code === "Escape" || event.key === "Escape") {
        setListening(false);
        return;
      }
      const chord = chordFromKeyboardEvent(event);
      if (!chord) {
        return;
      }
      const next = normalizeChord(chord);
      setListening(false);
      window.setTimeout(() => onChangeRef.current(next), 0);
    };
    const onKeyUp = (event: KeyboardEvent) => {
      if (isFnKey(event)) {
        commit(event);
      }
    };
    const unlistenNative = isTauriRuntime()
      ? listenWhileMounted<{ key: string; pressed: boolean }>("native-hotkey-event", (event) => {
          if (event.key === "Fn" && event.pressed) {
            setListening(false);
            window.setTimeout(() => onChangeRef.current("Fn"), 0);
          }
        })
      : () => undefined;
    const onPointerDown = (event: PointerEvent) => {
      const target = event.target as Node | null;
      if (target && buttonRef.current?.contains(target)) {
        return;
      }
      setListening(false);
    };
    window.addEventListener("keyup", onKeyUp, true);
    window.addEventListener("pointerdown", onPointerDown, true);
    return () => {
      window.removeEventListener("keyup", onKeyUp, true);
      window.removeEventListener("pointerdown", onPointerDown, true);
      unlistenNative();
    };
  }, [listening]);

  return (
    <div className="block text-sm text-paper/70">
      {label}
      <button
        ref={buttonRef}
        type="button"
        className={`mt-1 w-full rounded-lg p-2 text-left font-mono text-paper ${
          listening ? "bg-copper/20 ring-1 ring-copper" : "bg-paper/10"
        }`}
        onClick={() => setListening((on) => !on)}
        onKeyDownCapture={(event) => {
          if (!listening) {
            return;
          }
          event.preventDefault();
          event.stopPropagation();
          if (event.code === "Escape" || event.key === "Escape") {
            setListening(false);
            return;
          }
          const chord = chordFromKeyboardEvent(event.nativeEvent);
          if (!chord) {
            return;
          }
          const next = normalizeChord(chord);
          setListening(false);
          window.setTimeout(() => onChangeRef.current(next), 0);
        }}
      >
        {listening ? listeningLabel : value}
      </button>
      {presets.length > 0 && (
        <div className="mt-2 flex flex-wrap gap-2">
          {presets.map((chord) => (
            <button
              key={chord}
              type="button"
              className="rounded-full border border-paper/30 px-3 py-1 font-mono text-xs text-paper/80"
              onClick={() => {
                setListening(false);
                window.setTimeout(() => onChangeRef.current(normalizeChord(chord)), 0);
              }}
            >
              {chord}
            </button>
          ))}
        </div>
      )}
      {error ? <p className="mt-1 text-xs text-copper">{error}</p> : null}
      {hint && !error ? <p className="mt-1 text-xs text-paper/60">{hint}</p> : null}
    </div>
  );
}
