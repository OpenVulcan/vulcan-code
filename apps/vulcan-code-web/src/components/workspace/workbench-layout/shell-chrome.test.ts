import { describe, expect, it } from "vitest";
import { shouldEnableWindowDragRegion } from "./shell-chrome";

// Shell chrome tests lock the native drag-region boundary to the authoritative maximize state.
// Shell chrome 测试将原生拖动区域边界锁定到权威的最大化状态。
describe("shouldEnableWindowDragRegion", () => {
  // Enables dragging only for a normal Tauri window.
  // 仅为普通状态的 Tauri 窗口启用拖动。
  it("enables dragging only when native controls exist and the window is not maximized", () => {
    expect(shouldEnableWindowDragRegion(true, false)).toBe(true);
    expect(shouldEnableWindowDragRegion(true, true)).toBe(false);
    expect(shouldEnableWindowDragRegion(false, false)).toBe(false);
    expect(shouldEnableWindowDragRegion(false, true)).toBe(false);
  });
});
