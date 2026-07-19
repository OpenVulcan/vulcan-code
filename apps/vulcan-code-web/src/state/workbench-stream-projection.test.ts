import { describe, expect, it } from "vitest";
import type { AppendMessageBodyDelta, WorkbenchDeltaEnvelope } from "@/types";
import { workbenchEnvelopeRequiresReactProjection } from "./workbench-stream-projection";

// appendEnvelope builds one streamed message event for projection classification tests.
// appendEnvelope 为投影分类测试构建一条流式消息事件。
//
// Parameters: generation orders the event and payload contains its exact append contract.
// 参数：generation 对事件排序，payload 包含精确追加契约。
// Returns: one valid workbench delta envelope.
// 返回值：一条有效工作台增量信封。
function appendEnvelope(
  generation: number,
  payload: AppendMessageBodyDelta,
): WorkbenchDeltaEnvelope {
  return {
    generation,
    delta: { kind: "appendMessageBody", payload },
  };
}

describe("workbench stream projection", () => {
  it("keeps pure active body continuations out of global React state", () => {
    expect(workbenchEnvelopeRequiresReactProjection(appendEnvelope(2, {
      conversationId: "conversation",
      messageId: "message",
      bodyDelta: "continuation",
      streamState: "active",
    }))).toBe(false);
  });

  it("retains initial, sealed, resync, and empty lifecycle events", () => {
    const base = {
      conversationId: "conversation",
      messageId: "message",
      bodyDelta: "",
    };
    expect(workbenchEnvelopeRequiresReactProjection(appendEnvelope(1, {
      ...base,
      bodyDelta: "first",
      streamState: "active",
      initialMessage: {
        id: "message",
        conversationId: "conversation",
        role: "assistant",
        body: "first",
        recordKind: "thinking-output",
      },
    }))).toBe(true);
    expect(workbenchEnvelopeRequiresReactProjection(appendEnvelope(2, {
      ...base,
      streamState: "sealed",
      currentBody: "final",
    }))).toBe(true);
    expect(workbenchEnvelopeRequiresReactProjection(appendEnvelope(3, {
      ...base,
      streamState: "active",
      currentBody: "resynced",
    }))).toBe(true);
    expect(workbenchEnvelopeRequiresReactProjection(appendEnvelope(4, {
      ...base,
      streamState: "active",
    }))).toBe(true);
  });
});
