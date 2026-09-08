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

export function keyFromCode(code: string): string | null {
  if (!code || MODIFIER_CODES.has(code)) {
    return null;
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
    ].includes(code)
  ) {
    return code;
  }
  return null;
}

/** Null while only modifiers are down, or for a bare letter/space that would steal typing. */
export function chordFromKeyboardEvent(
  event: Pick<KeyboardEvent, "code" | "repeat" | "ctrlKey" | "altKey" | "shiftKey" | "metaKey">,
  platform = navigator.platform,
): string | null {
  if (event.repeat) {
    return null;
  }
  const key = keyFromCode(event.code);
  if (!key) {
    return null;
  }
  const hasModifier = event.ctrlKey || event.altKey || event.shiftKey || event.metaKey;
  if (!hasModifier && !key.startsWith("F")) {
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
  return parts.join("+");
}
