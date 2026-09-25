// Verify the shared byte-size and date-time display formats.
// 验证共享的字节容量与日期时间显示格式。
import { describe, expect, it } from "vitest";
import { formatBytes, formatDateTime } from "./display-format";

describe("formatBytes", () => {
  it("keeps sizes below 1 KiB exact", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(1023)).toBe("1023 B");
  });

  it("uses one decimal with binary units and drops a zero decimal", () => {
    expect(formatBytes(2048)).toBe("2 KiB");
    expect(formatBytes(1_635_252)).toBe("1.6 MiB");
    expect(formatBytes(5 * 1024 ** 3)).toBe("5 GiB");
  });

  it("carries rounding into the next unit", () => {
    expect(formatBytes(1_048_575)).toBe("1 MiB");
  });

  it("treats invalid sizes as zero", () => {
    expect(formatBytes(Number.NaN)).toBe("0 B");
    expect(formatBytes(-5)).toBe("0 B");
  });
});

describe("formatDateTime", () => {
  // instant is a fixed moment; the expected text comes from the same Intl options, so the host time zone does not matter.
  // instant 是固定时间点；期望文本来自相同的 Intl 选项，因此与宿主时区无关。
  const instant = Date.UTC(2026, 8, 24, 15, 4);

  it("follows the requested locale", () => {
    for (const locale of ["zh-CN", "en-US"]) {
      expect(formatDateTime(instant, locale)).toBe(
        new Intl.DateTimeFormat(locale, { dateStyle: "short", timeStyle: "short" }).format(new Date(instant)),
      );
    }
    expect(formatDateTime(instant, "zh-CN")).not.toBe(formatDateTime(instant, "en-US"));
  });

  it("returns null for invalid instants", () => {
    expect(formatDateTime(Number.NaN, "en-US")).toBeNull();
    expect(formatDateTime(new Date("not a date"), "zh-CN")).toBeNull();
  });
});
