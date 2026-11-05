import { describe, expect, it } from "vitest";
import {
  contextWindowSelectionTokens,
  formatContextWindowTokens,
  formatCustomContextWindowInput,
  parseContextWindowTokens,
  sameContextWindowSelection,
  validateCustomContextWindowInput,
} from "@/lib/context-window-selection";

describe("parseContextWindowTokens", () => {
  it("accepts plain integers and integers followed by K, where 1K = 1000", () => {
    expect(parseContextWindowTokens("235000")).toBe(235_000);
    expect(parseContextWindowTokens("235K")).toBe(235_000);
    expect(parseContextWindowTokens(" 358k ")).toBe(358_000);
    expect(parseContextWindowTokens("100 K")).toBe(100_000);
  });

  it("rejects other shapes", () => {
    for (const text of ["", "K", "1.5K", "235M", "-5", "1e6", "12,000", "0"]) {
      expect(parseContextWindowTokens(text), text).toBeNull();
    }
    expect(parseContextWindowTokens("99999999999999999999")).toBeNull();
  });
});

describe("validateCustomContextWindowInput", () => {
  it("applies the minimum and lets a value equal the model window", () => {
    expect(validateCustomContextWindowInput("x", 100_000, 400_000)).toEqual({ error: "format" });
    expect(validateCustomContextWindowInput("99999", 100_000, 400_000)).toEqual({ error: "belowMinimum" });
    expect(validateCustomContextWindowInput("100K", 100_000, 400_000)).toEqual({ tokens: 100_000 });
    expect(validateCustomContextWindowInput("400K", 100_000, 400_000)).toEqual({ tokens: 400_000 });
    expect(validateCustomContextWindowInput("400001", 100_000, 400_000)).toEqual({ error: "aboveModel" });
  });
});

describe("context window selection values", () => {
  const nodes = [{ id: "compression-window-1" as const, tokens: 200_000 }];

  it("resolves tokens for nodes and custom values", () => {
    expect(contextWindowSelectionTokens(null, nodes)).toBeNull();
    expect(contextWindowSelectionTokens({ node: "compression-window-1" }, nodes)).toBe(200_000);
    expect(contextWindowSelectionTokens({ node: "compression-window-2" }, nodes)).toBeNull();
    expect(contextWindowSelectionTokens({ custom: 235_500 }, nodes)).toBe(235_500);
  });

  it("compares selections by value", () => {
    expect(sameContextWindowSelection(null, undefined)).toBe(true);
    expect(sameContextWindowSelection({ custom: 1 }, { custom: 1 })).toBe(true);
    expect(sameContextWindowSelection({ custom: 1 }, null)).toBe(false);
    expect(sameContextWindowSelection({ node: "compression-window-1" }, { custom: 200_000 })).toBe(false);
  });

  it("formats compact labels and exact custom input", () => {
    expect(formatContextWindowTokens(400_000)).toBe("400K");
    expect(formatContextWindowTokens(1_000_000)).toBe("1M");
    expect(formatContextWindowTokens(1_500_000)).toBe("1500K");
    expect(formatCustomContextWindowInput(235_000)).toBe("235K");
    expect(formatCustomContextWindowInput(235_500)).toBe("235500");
  });
});
