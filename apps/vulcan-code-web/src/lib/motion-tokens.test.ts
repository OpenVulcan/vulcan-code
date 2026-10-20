// @vitest-environment jsdom
import { afterEach, expect, it } from "vitest";
import { motionTokenMs } from "./motion-tokens";

afterEach(() => document.documentElement.style.removeProperty("--vk-t-fast"));

it("reads millisecond and second durations from the live token", () => {
  document.documentElement.style.setProperty("--vk-t-fast", "120ms");
  expect(motionTokenMs("--vk-t-fast")).toBe(120);
  document.documentElement.style.setProperty("--vk-t-fast", "0.25s");
  expect(motionTokenMs("--vk-t-fast")).toBe(250);
});

it("waits zero when the stylesheet declares no usable duration", () => {
  expect(motionTokenMs("--vk-t-fast")).toBe(0);
  document.documentElement.style.setProperty("--vk-t-fast", "fast");
  expect(motionTokenMs("--vk-t-fast")).toBe(0);
});
