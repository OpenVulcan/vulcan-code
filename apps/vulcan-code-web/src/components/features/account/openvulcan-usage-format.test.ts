import { describe, expect, it } from "vitest";
import { formatMicroPoints, formatPoints } from "./openvulcan-usage-format";

describe("formatMicroPoints", () => {
  it("always shows one decimal, flooring balances and rounding consumption", () => {
    expect(formatMicroPoints("4999973800", "en-US", "floor")).toBe("4,999.9");
    expect(formatMicroPoints("4999973800", "en-US")).toBe("5,000.0");
    expect(formatMicroPoints("956084", "en-US")).toBe("1.0");
    expect(formatMicroPoints("0", "en-US")).toBe("0.0");
  });

  it("marks a non-zero charge below the shown precision instead of showing zero", () => {
    expect(formatMicroPoints("26200", "en-US")).toBe("<0.1");
    expect(formatPoints("0.049999", "de-DE")).toBe("<0,1");
  });
});
