import { useEffect, useRef, useState } from "react";
import { chordFromKeyboardEvent, isFnKey, normalizeChord } from "./hotkey";

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
      onChangeRef.current(normalizeChord(chord));
      setListening(false);
    };
    const onKeyDown = (event: KeyboardEvent) => commit(event);
    const onKeyUp = (event: KeyboardEvent) => {
      if (isFnKey(event)) {
        commit(event);
      }
    };
    const onPointerDown = (event: PointerEvent) => {
      const target = event.target as Node | null;
      if (target && buttonRef.current?.contains(target)) {
        return;
      }
      setListening(false);
    };
    window.addEventListener("keydown", onKeyDown, true);
    window.addEventListener("keyup", onKeyUp, true);
    window.addEventListener("pointerdown", onPointerDown, true);
    return () => {
      window.removeEventListener("keydown", onKeyDown, true);
      window.removeEventListener("keyup", onKeyUp, true);
      window.removeEventListener("pointerdown", onPointerDown, true);
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
                onChange(normalizeChord(chord));
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
