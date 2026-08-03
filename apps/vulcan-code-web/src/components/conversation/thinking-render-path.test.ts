import { describe, expect, it } from "vitest";
import {
  shouldUseStreamingThinkingMarkdown,
  streamingThinkingIdentity,
} from "./thinking-render-path";

describe("thinking segmented render path", () => {
  // An identified active block must use the same segmented renderer as the popup.
  // 具备完整身份的活动思考块必须使用与弹窗相同的分段渲染器。
  it("enables segmentation for an identified active block", () => {
    const identity = streamingThinkingIdentity("conversation-1", "message-1", "block-1");
    expect(identity).not.toBeNull();
    expect(shouldUseStreamingThinkingMarkdown(identity, true, null)).toBe(true);
  });

  // The same mounted block must keep segmentation while its live Store record seals.
  // 同一挂载块在实时 Store 记录 sealed 时必须继续保持分段渲染。
  it("keeps segmentation through the active-to-sealed handoff", () => {
    const identity = streamingThinkingIdentity("conversation-1", "message-1", "block-1");
    expect(shouldUseStreamingThinkingMarkdown(identity, false, identity)).toBe(true);
  });

  // A historical block that never entered the live path must remain on static Markdown.
  // 从未进入实时路径的历史块必须继续使用静态 Markdown。
  it("keeps an initially sealed block static", () => {
    const identity = streamingThinkingIdentity("conversation-1", "message-1", "block-1");
    expect(shouldUseStreamingThinkingMarkdown(identity, false, null)).toBe(false);
  });

  // Missing live identity must explicitly fall back instead of guessing another field.
  // 缺少实时身份时必须显式回退，不得猜测其他字段。
  it("requires the complete live identity", () => {
    expect(streamingThinkingIdentity(null, "message-1", "block-1")).toBeNull();
    expect(streamingThinkingIdentity("conversation-1", undefined, "block-1")).toBeNull();
    expect(shouldUseStreamingThinkingMarkdown(null, true, null)).toBe(false);
  });

  // A new stream identity cannot inherit the previous block's sealed renderer latch.
  // 新实时流身份不能继承前一个块的 sealed 渲染锁存状态。
  it("isolates a changed stream identity", () => {
    const previousIdentity = streamingThinkingIdentity("conversation-1", "message-1", "block-1");
    const nextIdentity = streamingThinkingIdentity("conversation-1", "message-2", "block-2");
    expect(shouldUseStreamingThinkingMarkdown(nextIdentity, false, previousIdentity)).toBe(false);
  });
});
