import { describe, expect, it } from "vitest";
import { copy, formatBytes, NAV, navItems } from "./ui";
import { formatInvokeError } from "./api";

describe("formatBytes", () => {
  it("uses MB for whisper-sized artifacts", () => {
    expect(formatBytes(147951465)).toContain("MB");
  });
});

describe("formatInvokeError", () => {
  it("renders Tauri command errors instead of [object Object]", () => {
    expect(
      formatInvokeError({
        code: "MODEL_FORMAT_INVALID",
        message: "ggml-small.bin is not a ggml/whisper artifact",
      }),
    ).toBe("MODEL_FORMAT_INVALID: ggml-small.bin is not a ggml/whisper artifact");
  });

  it("falls back to String for plain values", () => {
    expect(formatInvokeError("boom")).toBe("boom");
    expect(formatInvokeError(null)).toBe("null");
  });
});

describe("navItems", () => {
  it("switches labels for Russian UI", () => {
    expect(navItems("ru").map((item) => item.id)).toEqual(NAV.map((item) => item.id));
    expect(navItems("ru").find((item) => item.id === "history")?.label).toBe("История");
    expect(navItems("en").find((item) => item.id === "history")?.label).toBe("History");
  });
});

describe("copy", () => {
  it("translates settings descriptions when the interface language is Russian", () => {
    expect(copy("en").hotkeyHelp).toMatch(/Spotlight/);
    expect(copy("ru").hotkeyHelp).toMatch(/Spotlight|раскладка/);
    expect(copy("ru").speechLangHelp).not.toBe(copy("en").speechLangHelp);
    expect(copy("ru").settingsTitle).toBe("Настройки");
    expect(copy("en").speechModel).toMatch(/downloaded/i);
    expect(copy("ru").speechModel).toMatch(/скачан/i);
    expect(copy("ru").noDownloadedSpeech).not.toBe(copy("en").noDownloadedSpeech);
    expect(copy("ru").onboardingTitle).toMatch(/Mac/);
    expect(copy("ru").interfaceLanguage).toBe("Язык интерфейса");
  });
});

describe("formatBytes edges", () => {
  it("uses B and KB for small artifacts", () => {
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(2048)).toContain("KB");
  });
});
