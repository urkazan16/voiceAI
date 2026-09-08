import { useEffect, useRef, useState } from "react";
import { chordFromKeyboardEvent } from "./hotkey";

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
    const onKeyDown = (event: KeyboardEvent) => {
      event.preventDefault();
      event.stopPropagation();
      if (event.code === "Escape") {
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
    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
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
