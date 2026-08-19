import { describe, expect, it } from "vitest";
import type { ConversationMessage, ConversationSummary } from "@/types";
import type {
  ConversationBlock,
  ConversationRenderItem,
} from "@/components/conversation/conversation-model";
import { activeStreamingThinkingBlock } from "./conversation-runtime";

const thinkingBlock: ConversationBlock = {
  id: "thinking-block",
  sourceMessageId: "thinking-message",
  kind: "thinking",
  title: "Thinking",
  body: "Inspecting the tool result",
  summary: "Inspecting the tool result",
};

const activeThinkingMessage: ConversationMessage = {
  id: "thinking-message",
  conversationId: "conversation-1",
  role: "assistant",
  recordKind: "thinking-output",
  body: thinkingBlock.body,
  metadata: { stream_state: "active" },
};

const activeThinkingItem: ConversationRenderItem = {
  id: "thinking-item",
  role: "assistant",
  blocks: [thinkingBlock],
  sourceMessage: activeThinkingMessage,
  sourceMessages: [activeThinkingMessage],
};

describe("active streaming thinking block", () => {
  // An active reasoning record remains visible when a tool loop already advanced the session phase.
  // 当工具循环已经推进会话阶段时，活动推理记录仍保持可见。
  it("keeps post-tool reasoning visible across a requesting phase", () => {
    const conversation: ConversationSummary = {
      id: "conversation-1",
      title: "Post-tool reasoning",
      status: "running",
      runPhase: "requesting",
    };

    expect(activeStreamingThinkingBlock(conversation, [activeThinkingItem])).toBe(thinkingBlock);
  });

  // A sealed reasoning record cannot keep the transient popup alive.
  // 已封存的推理记录不能继续保留临时思考弹窗。
  it("hides a sealed reasoning record", () => {
    const conversation: ConversationSummary = {
      id: "conversation-1",
      title: "Sealed reasoning",
      status: "running",
      runPhase: "requesting",
    };
    const sealedMessage: ConversationMessage = {
      ...activeThinkingMessage,
      metadata: { stream_state: "sealed" },
    };
    const sealedItem: ConversationRenderItem = {
      ...activeThinkingItem,
      sourceMessage: sealedMessage,
      sourceMessages: [sealedMessage],
    };

    expect(activeStreamingThinkingBlock(conversation, [sealedItem])).toBeNull();
  });

  // A stopped conversation hides any stale active marker left in a delayed frontend projection.
  // 已停止的会话会隐藏前端延迟投影中残留的活动标记。
  it("hides stale active reasoning after the session stops", () => {
    const conversation: ConversationSummary = {
      id: "conversation-1",
      title: "Stopped reasoning",
      status: "stopped",
      runPhase: "stopped",
    };

    expect(activeStreamingThinkingBlock(conversation, [activeThinkingItem])).toBeNull();
  });
});
