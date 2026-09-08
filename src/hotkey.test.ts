import { describe, expect, it } from "vitest";
import { chordFromKeyboardEvent, keyFromCode, metaModifierName } from "./hotkey";

describe("chordFromKeyboardEvent", () => {
  it("builds Control+Shift+Space", () => {
    expect(
      chordFromKeyboardEvent({
        code: "Space",
        repeat: false,
        ctrlKey: true,
        altKey: false,
        shiftKey: true,
        metaKey: false,
      }),
    ).toBe("Control+Shift+Space");
  });

  it("uses Command on macOS for the meta key", () => {
    expect(
      chordFromKeyboardEvent(
        {
          code: "KeyC",
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

  it("ignores modifier-only and bare letters", () => {
    expect(
      chordFromKeyboardEvent({
        code: "ShiftLeft",
        repeat: false,
        ctrlKey: false,
        altKey: false,
        shiftKey: true,
        metaKey: false,
      }),
    ).toBeNull();
    expect(
      chordFromKeyboardEvent({
        code: "KeyA",
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
        repeat: false,
        ctrlKey: false,
        altKey: false,
        shiftKey: false,
        metaKey: false,
      }),
    ).toBe("F13");
  });
});
