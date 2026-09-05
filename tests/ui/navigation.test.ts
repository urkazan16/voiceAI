import { describe, expect, it } from "vitest";
import { formatBytes, NAV } from "../../src/ui";

describe("ui helpers", () => {
  it("formats gigabyte model sizes", () => {
    expect(formatBytes(2500000000)).toContain("GB");
  });

  it("lists product surfaces for completeness", () => {
    const labels = NAV.map((item) => item.label);
    expect(labels).toEqual(
      expect.arrayContaining([
        "Settings",
        "Models",
        "Dictionary",
        "Profiles",
        "Personalization",
        "History",
        "Diagnostics",
        "Privacy",
      ]),
    );
  });
});
