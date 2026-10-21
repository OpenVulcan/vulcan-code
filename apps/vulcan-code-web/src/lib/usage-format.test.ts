// 验证 Token 统计的紧凑单位格式与耗时的一位小数格式。
import { describe, expect, it } from "vitest";
import { formatDurationSeconds, formatTokenCount, formatTokenSpeed, tokenUnitLabel } from "./usage-format";

describe("formatTokenCount", () => {
  it("keeps counts below 1000 exact", () => {
    expect(formatTokenCount(0)).toBe("0");
    expect(formatTokenCount(715)).toBe("715");
    expect(formatTokenCount(999)).toBe("999");
  });

  it("uses one decimal with K, M and B from 1000 upward", () => {
    expect(formatTokenCount(1_000)).toBe("1K");
    expect(formatTokenCount(24_494)).toBe("24.5K");
    expect(formatTokenCount(25_209)).toBe("25.2K");
    expect(formatTokenCount(950_000)).toBe("950K");
    expect(formatTokenCount(1_234_567)).toBe("1.2M");
    expect(formatTokenCount(3_450_000_000)).toBe("3.5B");
    expect(formatTokenCount(1_500_000_000_000)).toBe("1500B");
  });

  it("carries rounding into the next unit and clamps invalid input", () => {
    expect(formatTokenCount(999_950)).toBe("1M");
    expect(formatTokenCount(999_949)).toBe("999.9K");
    expect(formatTokenCount(-5)).toBe("0");
    expect(formatTokenCount(Number.NaN)).toBe("0");
  });
});

describe("formatDurationSeconds", () => {
  it("keeps one decimal of seconds", () => {
    expect(formatDurationSeconds(3_009)).toBe("3.0 s");
    expect(formatDurationSeconds(5_089)).toBe("5.1 s");
    expect(formatDurationSeconds(2_080)).toBe("2.1 s");
    expect(formatDurationSeconds(-1)).toBe("0.0 s");
  });
});

describe("formatTokenSpeed", () => {
  it("uses one decimal and the locale token unit", () => {
    expect(formatTokenSpeed(343.84)).toBe("343.8 t/s");
    expect(formatTokenSpeed(0.04)).toBe("<0.1 t/s");
    expect(formatTokenSpeed(0)).toBe("0.0 t/s");
    expect(tokenUnitLabel("zh-CN")).toBe("Token");
  });
});
