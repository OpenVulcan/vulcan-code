import { describe, expect, it } from "vitest";
import {
  DEFAULT_STREAMING_THINKING_LAYOUT_MODE,
  loadStreamingThinkingLayoutMode,
  nextStreamingThinkingLayoutMode,
  persistStreamingThinkingLayoutMode,
  STREAMING_THINKING_LAYOUT_STORAGE_KEY,
} from "@/components/conversation/streaming-thinking-layout";

describe("streaming thinking layout preference", () => {
  it("loads only supported globally persisted modes", () => {
    // values models browser storage without requiring a DOM environment.
    // values 模拟浏览器存储，无需 DOM 环境。
    const values = new Map<string, string>();
    // storage exposes the exact read contract used by the preference loader.
    // storage 暴露偏好读取器使用的精确读取契约。
    const storage = {
      getItem: (key: string) => values.get(key) ?? null,
    };

    expect(loadStreamingThinkingLayoutMode(storage)).toBe(DEFAULT_STREAMING_THINKING_LAYOUT_MODE);
    values.set(STREAMING_THINKING_LAYOUT_STORAGE_KEY, "right_docked");
    expect(loadStreamingThinkingLayoutMode(storage)).toBe("right_docked");
    values.set(STREAMING_THINKING_LAYOUT_STORAGE_KEY, "unsupported");
    expect(loadStreamingThinkingLayoutMode(storage)).toBe(DEFAULT_STREAMING_THINKING_LAYOUT_MODE);
  });

  it("persists and toggles the selected mode", () => {
    // values captures writes made through the browser-compatible storage contract.
    // values 捕获通过浏览器兼容存储契约执行的写入。
    const values = new Map<string, string>();
    // storage exposes the exact write contract used by the preference saver.
    // storage 暴露偏好保存器使用的精确写入契约。
    const storage = {
      setItem: (key: string, value: string) => values.set(key, value),
    };

    const nextMode = nextStreamingThinkingLayoutMode("centered");
    persistStreamingThinkingLayoutMode(nextMode, storage);

    expect(nextMode).toBe("right_docked");
    expect(values.get(STREAMING_THINKING_LAYOUT_STORAGE_KEY)).toBe("right_docked");
    expect(nextStreamingThinkingLayoutMode(nextMode)).toBe("centered");
  });
});
