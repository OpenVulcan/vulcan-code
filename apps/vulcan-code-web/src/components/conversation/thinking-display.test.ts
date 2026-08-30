import { describe, expect, it } from "vitest";
import type { ConversationBlock } from "./conversation-model";
import { activeThinkingOverlayBlock, type ThinkingDisplayMode } from "./thinking-display";

// modes defines the floating-surface contract independently of inline history filtering.
// modes 定义独立于内联历史过滤的浮层契约。
const modes: readonly [ThinkingDisplayMode, boolean][] = [
  ["popup_only", true],
  ["inline_output", false],
  ["first_inline_with_popup", true],
  ["first_inline_only", false],
];

describe("activeThinkingOverlayBlock", () => {
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
