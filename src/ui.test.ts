import { describe, expect, it } from "vitest";
import {
  copy,
  formatBytes,
  formatJournalLine,
  NAV,
  navItems,
  canDeleteDownloadedModel,
  hostKindFrom,
  showMacOnlyControls,
} from "./ui";
import { formatInvokeError } from "./api";

describe("formatBytes", () => {
  it("uses MB for whisper-sized artifacts", () => {
    expect(formatBytes(147951465)).toContain("MB");
  });
});

describe("formatJournalLine", () => {
  it("turns unix seconds into a UTC stamp and keeps the event", () => {
    const formatted = formatJournalLine(
      "1788881474 insert_failed PERMISSION_DENIED: Accessibility permission required for insertion",
    );
    expect(formatted).toContain("UTC");
    expect(formatted).toContain("insert_failed");
    expect(formatted).not.toMatch(/^1788881474 /);
  });

  it("leaves non-journal lines unchanged", () => {
    expect(formatJournalLine("not a log")).toBe("not a log");
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
    expect(navItems("en").find((item) => item.id === "logs")?.label).toBe("Logs");
    expect(navItems("ru").find((item) => item.id === "logs")?.label).toBe("Логи");
  });
});

describe("copy", () => {
  it("translates settings descriptions when the interface language is Russian", () => {
    expect(copy("en").hotkeyListening).toMatch(/key or combination/);
    expect(copy("ru").hotkeyListening).toMatch(/клавишу или сочетание/);
    expect(copy("en").hotkeyHelp).toMatch(/Fn/);
    expect(copy("en").computeGpu).toMatch(/Metal/);
    expect(copy("en", "windows").computeGpu).toBe("GPU");
    expect(copy("en", "linux").computeGpu).toBe("GPU");
    expect(copy("ru").barPasteHint).toMatch(/Вставить последнее/);
    expect(copy("ru").hotkeyHelp).toMatch(/Spotlight|раскладка/);
    expect(copy("ru").speechLangHelp).not.toBe(copy("en").speechLangHelp);
    expect(copy("ru").settingsTitle).toBe("Настройки");
    expect(copy("en").speechModel).toMatch(/downloaded/i);
    expect(copy("ru").speechModel).toMatch(/скачан/i);
    expect(copy("ru").noDownloadedSpeech).not.toBe(copy("en").noDownloadedSpeech);
    expect(copy("en").unusedModels).toMatch(/Unused/i);
    expect(copy("ru").deleteUnusedAll).toMatch(/неиспользуем/i);
    expect(copy("ru").onboardingTitle).toMatch(/Mac/);
    expect(copy("ru").interfaceLanguage).toBe("Язык интерфейса");
    expect(copy("en").uninstallButton).toMatch(/completely/i);
    expect(copy("ru").uninstallButton).toMatch(/полностью/i);
    expect(copy("en").uninstallConfirm).toMatch(/models/i);
    expect(copy("ru").uninstallConfirm).toMatch(/модели/i);
  });

  it("keeps macOS wording when host is omitted", () => {
    expect(copy("en").clipboardHelp).toMatch(/Cmd\+V/);
    expect(copy("en").barAccessHint).toMatch(/Quit from the menu bar/);
    expect(copy("ru").barAccessHint).toMatch(/строку меню/);
    expect(copy("en").quitRelaunchAccess).toMatch(/relaunch/i);
  });

  it("uses Windows wording without macOS Accessibility copy", () => {
    expect(copy("en", "windows").onboardingTitle).toMatch(/this PC/);
    expect(copy("en", "windows").hotkeyHelp).toMatch(/Win\+Space/);
    expect(copy("en", "windows").hotkeyHelp).not.toMatch(/Spotlight/);
    expect(copy("en", "windows").clipboardHelp).toMatch(/Ctrl\+V/);
    expect(copy("en", "windows").onboarding1).not.toMatch(/Accessibility/);
    expect(copy("ru", "windows").onboardingTitle).toMatch(/ПК/);
    expect(copy("ru", "windows").onboardingTitle).not.toMatch(/Mac/);
  });

  it("uses Linux wording and a Wayland paste note", () => {
    expect(copy("en", "linux").onboardingTitle).toMatch(/this computer/);
    expect(copy("en", "linux").hotkeyHelp).toMatch(/Super\+Space/);
    expect(copy("en", "linux").clipboardHelp).toMatch(/Wayland/);
    expect(copy("ru", "linux").onboarding1).toMatch(/Wayland/);
    expect(copy("ru", "linux").onboardingTitle).not.toMatch(/Mac/);
  });
});

describe("hostKindFrom", () => {
  it("maps build.platform strings from get_build_info", () => {
    expect(hostKindFrom("macOS Intel")).toBe("macos");
    expect(hostKindFrom("macOS Apple Silicon")).toBe("macos");
    expect(hostKindFrom("windows")).toBe("windows");
    expect(hostKindFrom("linux")).toBe("linux");
    expect(showMacOnlyControls("macos")).toBe(true);
    expect(showMacOnlyControls("windows")).toBe(false);
    expect(showMacOnlyControls("linux")).toBe(false);
  });
});

describe("canDeleteDownloadedModel", () => {
  it("keeps the ready model in use and allows leftover files", () => {
    expect(canDeleteDownloadedModel(undefined)).toBe(false);
    expect(
      canDeleteDownloadedModel({
        bytes_on_disk: 100,
        local_path: "/tmp/m.bin",
        active: true,
        installed: true,
        verified: true,
      }),
    ).toBe(false);
    expect(
      canDeleteDownloadedModel({
        bytes_on_disk: 100,
        local_path: "/tmp/m.bin",
        active: false,
        installed: true,
        verified: true,
      }),
    ).toBe(true);
    expect(
      canDeleteDownloadedModel({
        bytes_on_disk: 40,
        local_path: "/tmp/m.bin.partial",
        active: true,
        installed: false,
        verified: false,
      }),
    ).toBe(true);
  });
});

describe("formatBytes edges", () => {
  it("uses B and KB for small artifacts", () => {
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(2048)).toContain("KB");
  });
});

describe("onboarding continue", () => {
  it("marks setup complete in client state so Setup leaves the nav", async () => {
    const src = (await import("./App.tsx?raw")).default;
    const click = src.split("completeOnboarding()")[1] ?? "";
    expect(click).toContain("onboarding_complete: true");
    expect(click).toContain('setView("home")');
  });
});
