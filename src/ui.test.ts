import { describe, expect, it } from "vitest";
import { formatBytes, NAV, navItems } from "./ui";
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

describe("formatBytes edges", () => {
  it("uses B and KB for small artifacts", () => {
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(2048)).toContain("KB");
  });
});
