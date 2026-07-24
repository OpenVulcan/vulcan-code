import { describe, expect, it } from "vitest";

import {
  formatCompactGoalTokenCount,
  formatFullGoalTokenCount,
} from "./goal-token-format";

describe("formatCompactGoalTokenCount", () => {
  it("renders the required K, M, and B examples without a token suffix", () => {
    expect(formatCompactGoalTokenCount(100)).toBe("100");
    expect(formatCompactGoalTokenCount(1_000)).toBe("1K");
    expect(formatCompactGoalTokenCount(1_360)).toBe("1.36K");
    expect(formatCompactGoalTokenCount(239_000)).toBe("239K");
    expect(formatCompactGoalTokenCount(1_120_000)).toBe("1.12M");
    expect(formatCompactGoalTokenCount(999_110_000)).toBe("999.11M");
    expect(formatCompactGoalTokenCount(1_200_000_000)).toBe("1.2B");
  });

  it("promotes rounded unit boundaries and normalizes invalid values", () => {
    expect(formatCompactGoalTokenCount(999_999)).toBe("1M");
    expect(formatCompactGoalTokenCount(-1)).toBe("0");
    expect(formatCompactGoalTokenCount(Number.NaN)).toBe("0");
  });
});

describe("formatFullGoalTokenCount", () => {
  it("keeps complete locale-formatted values for Tooltip details", () => {
    expect(formatFullGoalTokenCount(113_230, "en-US")).toBe("113,230");
    expect(formatFullGoalTokenCount(113_230, "zh-CN")).toBe("113,230");
  });
});
