import { describe, expect, it } from "vitest";
import { formatBytes } from "./ui";

describe("formatBytes", () => {
  it("uses MB for whisper-sized artifacts", () => {
    expect(formatBytes(147951465)).toContain("MB");
  });
});
