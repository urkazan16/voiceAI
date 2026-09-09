import { describe, expect, it } from "vitest";
import {
  chordFromKeyboardEvent,
  collidingHotkeyReason,
  isFnKey,
  keyFromCode,
  metaModifierName,
  reservedHotkeyReason,
  validateTalkHotkey,
} from "./hotkey";

describe("chordFromKeyboardEvent", () => {
  it("builds Control+Shift+Space", () => {
    expect(
      chordFromKeyboardEvent({
        code: "Space",
        key: " ",
        repeat: false,
        ctrlKey: true,
        altKey: false,
        shiftKey: true,
        metaKey: false,
      }),
    ).toBe("Control+Shift+Space");
  });

  it("builds Control+Space", () => {
    expect(
      chordFromKeyboardEvent({
        code: "Space",
        key: " ",
        repeat: false,
        ctrlKey: true,
        altKey: false,
        shiftKey: false,
        metaKey: false,
      }),
    ).toBe("Control+Space");
  });

  it("accepts a single Space or letter", () => {
    expect(
      chordFromKeyboardEvent({
        code: "Space",
        key: " ",
        repeat: false,
        ctrlKey: false,
        altKey: false,
        shiftKey: false,
        metaKey: false,
      }),
    ).toBe("Space");
    expect(
      chordFromKeyboardEvent({
        code: "KeyA",
        key: "a",
        repeat: false,
        ctrlKey: false,
        altKey: false,
        shiftKey: false,
        metaKey: false,
      }),
    ).toBe("A");
  });

  it("records Fn by itself", () => {
    expect(isFnKey({ code: "Fn", key: "Fn" })).toBe(true);
    expect(
      chordFromKeyboardEvent({
        code: "Fn",
        key: "Fn",
        repeat: false,
        ctrlKey: false,
        altKey: false,
        shiftKey: false,
        metaKey: false,
      }),
    ).toBe("Fn");
    expect(
      chordFromKeyboardEvent({
        code: "",
        key: "Globe",
        repeat: false,
        ctrlKey: false,
        altKey: false,
        shiftKey: false,
        metaKey: false,
      }),
    ).toBe("Fn");
  });

  it("uses Command on macOS for the meta key", () => {
    expect(
      chordFromKeyboardEvent(
        {
          code: "KeyC",
          key: "c",
          repeat: false,
          ctrlKey: true,
          altKey: false,
          shiftKey: false,
          metaKey: true,
        },
        "MacIntel",
      ),
    ).toBe("Control+Command+C");
  });

  it("uses Super on Windows for the meta key", () => {
    expect(metaModifierName("Win32")).toBe("Super");
    expect(
      chordFromKeyboardEvent(
        {
          code: "KeyV",
          key: "v",
          repeat: false,
          ctrlKey: true,
          altKey: true,
          shiftKey: false,
          metaKey: false,
        },
        "Win32",
      ),
    ).toBe("Control+Alt+V");
  });

  it("ignores modifier-only chords until a real key arrives", () => {
    expect(
      chordFromKeyboardEvent({
        code: "ShiftLeft",
        key: "Shift",
        repeat: false,
        ctrlKey: false,
        altKey: false,
        shiftKey: true,
        metaKey: false,
      }),
    ).toBeNull();
    expect(
      chordFromKeyboardEvent({
        code: "Escape",
        key: "Escape",
        repeat: false,
        ctrlKey: false,
        altKey: false,
        shiftKey: false,
        metaKey: false,
      }),
    ).toBeNull();
  });

  it("allows a function key without modifiers", () => {
    expect(keyFromCode("F13")).toBe("F13");
    expect(
      chordFromKeyboardEvent({
        code: "F13",
        key: "F13",
        repeat: false,
        ctrlKey: false,
        altKey: false,
        shiftKey: false,
        metaKey: false,
      }),
    ).toBe("F13");
  });
});

describe("hotkey validation", () => {
  it("rejects Control+C and Command+Space as talk keys", () => {
    expect(reservedHotkeyReason("Control+C")).toMatch(/reserved/i);
    expect(reservedHotkeyReason("Command+Space")).toMatch(/reserved/i);
    expect(reservedHotkeyReason("A")).toMatch(/typing/);
    expect(reservedHotkeyReason("Control+Shift+Space")).toBeNull();
  });

  it("rejects two LocalFlow actions on the same chord", () => {
    expect(
      collidingHotkeyReason(
        "Control+Shift+Space",
        "Control+Shift+Space",
        "Control+Alt+V",
        "Control+Alt+E",
      ),
    ).toMatch(/same shortcut/);
    expect(
      validateTalkHotkey(
        "Control+Shift+Space",
        "Command+Control+C",
        "Command+Control+V",
        "Command+Control+E",
      ),
    ).toBeNull();
  });
});
