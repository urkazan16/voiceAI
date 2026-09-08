import { useEffect, useRef, useState } from "react";
import { chordFromKeyboardEvent, isFnKey } from "./hotkey";

type HotkeyFieldProps = {
  label: string;
  value: string;
  listeningLabel: string;
  onChange: (chord: string) => void;
};

export function HotkeyField({ label, value, listeningLabel, onChange }: HotkeyFieldProps) {
  const [listening, setListening] = useState(false);
  const onChangeRef = useRef(onChange);

  useEffect(() => {
    onChangeRef.current = onChange;
  }, [onChange]);

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
      onChangeRef.current(chord);
      setListening(false);
    };
    const onKeyDown = (event: KeyboardEvent) => commit(event);
    const onKeyUp = (event: KeyboardEvent) => {
      // Fn/Globe often only appears on keyup in WKWebView.
      if (isFnKey(event)) {
        commit(event);
      }
    };
    window.addEventListener("keydown", onKeyDown, true);
    window.addEventListener("keyup", onKeyUp, true);
    return () => {
      window.removeEventListener("keydown", onKeyDown, true);
      window.removeEventListener("keyup", onKeyUp, true);
    };
  }, [listening]);

  return (
    <div className="block text-sm text-paper/70">
      {label}
      <button
        type="button"
        className={`mt-1 w-full rounded-lg p-2 text-left font-mono text-paper ${
          listening ? "bg-copper/20 ring-1 ring-copper" : "bg-paper/10"
        }`}
        onClick={() => setListening((on) => !on)}
        onBlur={() => setListening(false)}
      >
        {listening ? listeningLabel : value}
      </button>
    </div>
  );
}
