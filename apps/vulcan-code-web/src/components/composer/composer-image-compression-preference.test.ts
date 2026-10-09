// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import {
  composerAttachmentOverflowCount,
  readComposerImageCompressionPreference,
  saveComposerImageCompressionPreference,
} from "./composer-image-compression-preference";

afterEach(() => {
  vi.restoreAllMocks();
});

it("defaults to compression and remembers an explicit choice", () => {
  // Synthetic storage keeps the test independent of the runner's real browser store.
  // 合成存储使测试独立于运行器的真实浏览器存储。
  const values = new Map<string, string>();
  vi.spyOn(window, "localStorage", "get").mockReturnValue({
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => void values.set(key, value),
  } as Storage);
  expect(readComposerImageCompressionPreference()).toBe(true);
  saveComposerImageCompressionPreference(false);
  expect(readComposerImageCompressionPreference()).toBe(false);
});

it("keeps the default when the browser store is unavailable", () => {
  vi.spyOn(window, "localStorage", "get").mockImplementation(() => {
    throw new Error("blocked");
  });
  expect(readComposerImageCompressionPreference()).toBe(true);
  expect(() => saveComposerImageCompressionPreference(false)).not.toThrow();
});

it("counts only attachments rejected by the request-wide encoded budget", () => {
  expect(composerAttachmentOverflowCount({
    a: { reason: "encoded_budget" },
    b: { reason: "inline_threshold" },
    c: { reason: "encoded_budget" },
  })).toBe(2);
  expect(composerAttachmentOverflowCount({})).toBe(0);
});
