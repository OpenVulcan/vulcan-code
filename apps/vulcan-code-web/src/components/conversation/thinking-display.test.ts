import { describe, expect, it } from "vitest";
import type { ConversationBlock } from "./conversation-model";
import { activeThinkingOverlayBlock, normalizeThinkingDisplayMode, hiddenThinkingBlockIdsForDisplayMode, type ThinkingDisplayMode } from "./thinking-display";

// modes defines the floating-surface contract independently of inline history filtering.
// modes 定义独立于内联历史过滤的浮层契约。
const modes: readonly [ThinkingDisplayMode, boolean][] = [
  ["popup_only", true],
  ["inline_output", false],
  ["first_inline_with_popup", true],
  ["first_inline_only", false],
];

describe("activeThinkingOverlayBlock", () => {
  // Existing stored values preserve popup intent while no longer hiding later reasoning.
  // 已存值保留弹窗意图，不再隐藏后续思考。
  it("normalizes legacy preferences into two states without hiding content", () => {
    expect(normalizeThinkingDisplayMode("first_inline_with_popup")).toBe("popup_only");
    expect(normalizeThinkingDisplayMode("first_inline_only")).toBe("inline_output");
    for (const [mode] of modes) expect(hiddenThinkingBlockIdsForDisplayMode(mode, []).size).toBe(0);
  });
  // A first-inline-only choice must never open a popup while reasoning is streaming.
  // 仅首段输出模式绝不能在思考流式输出时打开弹窗。
  it.each(modes)("resolves the live overlay for %s", (mode, visible) => {
    // block is the exact active reasoning record; presentation selection must not copy or mutate it.
    // block 是精确的活动思考记录；展示选择不得复制或修改该记录。
    const block: ConversationBlock = {
      id: "reasoning-live",
      kind: "thinking",
      title: "思考过程",
      summary: "正在思考",
      body: "实时思考正文",
      status: "streaming",
    };
    expect(activeThinkingOverlayBlock(mode, block)).toBe(visible ? block : null);
    expect(activeThinkingOverlayBlock(mode, null)).toBeNull();
  });
});
