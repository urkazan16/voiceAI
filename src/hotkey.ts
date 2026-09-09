/** Map a keydown to the Tauri global-shortcut string LocalFlow stores. */

const MODIFIER_CODES = new Set([
  "ControlLeft",
  "ControlRight",
  "ShiftLeft",
  "ShiftRight",
  "AltLeft",
  "AltRight",
  "MetaLeft",
  "MetaRight",
  "OSLeft",
  "OSRight",
]);

export function metaModifierName(platform = navigator.platform): "Command" | "Super" {
  return /mac/i.test(platform) ? "Command" : "Super";
}

export function isFnKey(event: Pick<KeyboardEvent, "code" | "key">): boolean {
  const code = event.code ?? "";
  const key = event.key ?? "";
  return (
    code === "Fn" ||
    code === "FnLeft" ||
    code === "FnRight" ||
    key === "Fn" ||
    key === "Function" ||
    key === "Globe"
  );
}

export function keyFromCode(code: string): string | null {
  if (!code || MODIFIER_CODES.has(code)) {
    return null;
  }
  if (code === "Fn" || code === "FnLeft" || code === "FnRight") {
    return "Fn";
  }
  if (code === "Space") {
    return "Space";
  }
  if (code === "Enter" || code === "Tab" || code === "Backspace" || code === "Delete") {
    return code;
  }
  if (/^F([1-9]|1[0-9]|2[0-4])$/.test(code)) {
    return code;
  }
  if (/^Key[A-Z]$/.test(code)) {
    return code.slice(3);
  }
  if (/^Digit[0-9]$/.test(code)) {
    return code.slice(5);
  }
  if (code.startsWith("Numpad") && code.length > 6) {
    return code;
  }
  if (
    [
      "Minus",
      "Equal",
      "BracketLeft",
      "BracketRight",
      "Backslash",
      "IntlBackslash",
      "Semicolon",
      "Quote",
      "Backquote",
      "Comma",
      "Period",
      "Slash",
      "ArrowUp",
      "ArrowDown",
      "ArrowLeft",
      "ArrowRight",
      "Home",
      "End",
      "PageUp",
      "PageDown",
      "Insert",
    ].includes(code)
  ) {
    return code === "IntlBackslash" ? "Backslash" : code;
  }
  return null;
}

/** Null while only Control/Shift/Alt/Meta are down. Escape is handled by the field. */
export function chordFromKeyboardEvent(
  event: Pick<
    KeyboardEvent,
    "code" | "key" | "repeat" | "ctrlKey" | "altKey" | "shiftKey" | "metaKey"
  >,
  platform = navigator.platform,
): string | null {
  if (event.repeat) {
    return null;
  }
  if (event.code === "Escape" || event.key === "Escape") {
    return null;
  }
  if (isFnKey(event)) {
    return "Fn";
  }
  const key = keyFromCode(event.code);
  if (!key) {
    return null;
  }
  const parts: string[] = [];
  if (event.ctrlKey) {
    parts.push("Control");
  }
  if (event.altKey) {
    parts.push("Alt");
  }
  if (event.shiftKey) {
    parts.push("Shift");
  }
  if (event.metaKey) {
    parts.push(metaModifierName(platform));
  }
  parts.push(key);
  return normalizeChord(parts.join("+"));
}

const MOD_ORDER = ["Control", "Alt", "Shift", "Command", "Super"] as const;

function aliasPart(part: string): string {
  const p = part.trim();
  const lower = p.toLowerCase();
  if (lower === "ctrl" || lower === "control") {
    return "Control";
  }
  if (lower === "alt" || lower === "option") {
    return "Alt";
  }
  if (lower === "shift") {
    return "Shift";
  }
  if (lower === "command" || lower === "cmd") {
    return "Command";
  }
  if (lower === "super" || lower === "meta" || lower === "win") {
    return "Super";
  }
  if (lower === "esc" || lower === "escape") {
    return "Escape";
  }
  return p;
}

/** Stable Control+Alt+Shift+Command/Super+Key order for compare and persist. */
export function normalizeChord(chord: string): string {
  const parts = chord
    .split("+")
    .map(aliasPart)
    .filter((part) => part.length > 0);
  const mods = MOD_ORDER.filter((mod) => parts.some((part) => part === mod));
  const key = parts.find((part) => !(MOD_ORDER as readonly string[]).includes(part));
  return [...mods, ...(key ? [key] : [])].join("+");
}

function canon(chord: string): string {
  return normalizeChord(chord)
    .split("+")
    .map((part) => (part === "Command" || part === "Super" ? "super" : part.toLowerCase()))
    .join("+");
}

const RESERVED = new Set([
  "escape",
  "tab",
  "control+c",
  "control+v",
  "control+x",
  "control+a",
  "control+z",
  "super+c",
  "super+v",
  "super+x",
  "super+a",
  "super+z",
  "super+q",
  "super+w",
  "super+tab",
  "super+space",
  "control+space",
  "alt+space",
  "alt+tab",
  "control+alt+delete",
]);

export function reservedHotkeyReason(chord: string): string | null {
  const normalized = normalizeChord(chord);
  if (!normalized) {
    return "Hotkey cannot be empty.";
  }
  const id = canon(normalized);
  if (id === "escape" || normalized.split("+").includes("Escape")) {
    return "Escape cancels dictation and cannot be the talk shortcut.";
  }
  if (RESERVED.has(id)) {
    return `${normalized} is reserved by the OS or by copy/paste. Pick another combination.`;
  }
  const parts = normalized.split("+");
  const key = parts[parts.length - 1];
  const mods = parts.slice(0, -1);
  if (mods.length === 0 && key && /^[A-Z0-9]$/.test(key)) {
    return `${key} alone would fire while typing. Add Control/Shift or use F13 / Space.`;
  }
  return null;
}

export function collidingHotkeyReason(
  talk: string,
  copy: string,
  paste: string,
  edit: string,
): string | null {
  const named: [string, string][] = [
    ["Talk", talk],
    ["Copy last", copy],
    ["Paste last", paste],
    ["Edit", edit],
  ];
  const seen = new Map<string, string>();
  for (const [label, chord] of named) {
    const id = canon(chord);
    if (!id) {
      continue;
    }
    const previous = seen.get(id);
    if (previous) {
      return `${label} uses the same shortcut as ${previous} (${normalizeChord(chord)}).`;
    }
    seen.set(id, label);
  }
  return null;
}

export function validateTalkHotkey(
  talk: string,
  copy: string,
  paste: string,
  edit: string,
): string | null {
  for (const chord of [talk, copy, paste, edit]) {
    const reserved = reservedHotkeyReason(chord);
    if (reserved) {
      return reserved;
    }
  }
  return collidingHotkeyReason(talk, copy, paste, edit);
}

export function talkHotkeyPresets(host: "macos" | "windows" | "linux"): string[] {
  if (host === "macos") {
    return ["Control+Shift+Space", "F13", "Control+Shift+D"];
  }
  return ["Control+Shift+Space", "F13", "Control+Shift+D"];
}
