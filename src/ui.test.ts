import { describe, expect, it } from "vitest";
import { formatBytes } from "./ui";
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
});
