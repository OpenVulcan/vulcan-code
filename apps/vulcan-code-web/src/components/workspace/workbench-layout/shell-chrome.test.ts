import { describe, expect, it } from "vitest";
import { shouldEnableWindowDragRegion } from "./shell-chrome";

// Shell chrome tests lock the native drag-region boundary to the authoritative maximize and full-screen state.
// Shell chrome 测试将原生拖动区域边界锁定到权威的最大化与全屏状态。
describe("shouldEnableWindowDragRegion", () => {
  // Enables dragging only for a normal desktop window.
  // 仅为普通状态的桌面窗口启用拖动。
  it("enables dragging only when native controls exist and the window is neither maximized nor full screen", () => {
    expect(shouldEnableWindowDragRegion(true, false, false)).toBe(true);
    expect(shouldEnableWindowDragRegion(true, true, false)).toBe(false);
    expect(shouldEnableWindowDragRegion(true, false, true)).toBe(false);
    expect(shouldEnableWindowDragRegion(false, false, false)).toBe(false);
    expect(shouldEnableWindowDragRegion(false, true, false)).toBe(false);
  });
});
