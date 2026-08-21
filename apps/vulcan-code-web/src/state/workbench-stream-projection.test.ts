import { describe, expect, it } from "vitest";
import type { AppendMessageBodyDelta, WorkbenchDeltaEnvelope } from "@/types";
import {
  workbenchEnvelopeRequiresImmediateLiveFlush,
  workbenchEnvelopeRequiresReactProjection,
} from "./workbench-stream-projection";

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

  it("keeps transient reasoning status out of global React workbench state", () => {
    expect(workbenchEnvelopeRequiresReactProjection({
      generation: 5,
      delta: {
        kind: "transientReasoningStatusChanged",
        payload: {
          conversationId: "conversation",
          status: {
            kind: "encrypted",
            reasoningTokens: 144,
          },
        },
      },
    })).toBe(false);
  });

  it("keeps tool request progress out of global React workbench state", () => {
    expect(workbenchEnvelopeRequiresReactProjection({
      generation: 6,
      delta: {
        kind: "toolRequestProgressChanged",
        payload: {
          conversationId: "conversation",
          requestStreamId: "request-1",
          progress: {
            requestStreamId: "request-1",
            toolCallId: "call-1",
            toolName: "terminal_session",
            state: "receiving",
            receivedBytes: 512,
            receivedCharacters: 500,
            requestRevision: 3,
          },
        },
      },
    })).toBe(false);
  });

  it("flushes assembled and cleared tool request boundaries immediately", () => {
    const basePayload = {
      conversationId: "conversation",
      requestStreamId: "request-1",
    };
    const assembledProgress = {
      requestStreamId: "request-1",
      toolCallId: "call-1",
      toolName: "terminal_session",
      state: "arguments_assembled" as const,
      receivedBytes: 512,
      receivedCharacters: 500,
      requestRevision: 4,
    };
    const assembledEnvelope: WorkbenchDeltaEnvelope = {
      generation: 7,
      delta: {
        kind: "toolRequestProgressChanged",
        payload: {
          ...basePayload,
          progress: assembledProgress,
        },
      },
    };
    const receivingEnvelope: WorkbenchDeltaEnvelope = {
      ...assembledEnvelope,
      generation: 6,
      delta: {
        kind: "toolRequestProgressChanged",
        payload: {
          ...basePayload,
          progress: {
            ...assembledProgress,
            state: "receiving",
            requestRevision: 3,
          },
        },
      },
    };
    const clearedEnvelope: WorkbenchDeltaEnvelope = {
      generation: 8,
      delta: {
        kind: "toolRequestProgressChanged",
        payload: { ...basePayload, progress: null },
      },
    };

    expect(workbenchEnvelopeRequiresImmediateLiveFlush(receivingEnvelope)).toBe(false);
    expect(workbenchEnvelopeRequiresImmediateLiveFlush(assembledEnvelope)).toBe(true);
    expect(workbenchEnvelopeRequiresImmediateLiveFlush(clearedEnvelope)).toBe(true);
  });

  it("flushes structural tool lifecycle message patches immediately", () => {
    const lifecyclePhases = ["ToolCalling", "ToolRunning", "Requesting"];
    for (const [index, runPhase] of lifecyclePhases.entries()) {
      const envelope: WorkbenchDeltaEnvelope = {
        generation: index + 9,
        delta: {
          kind: "conversationMessagesPatched",
          payload: {
            conversationId: "conversation",
            upsertMessages: [],
            removeMessageIds: [],
            runPhase,
          },
        },
      };
      expect(workbenchEnvelopeRequiresImmediateLiveFlush(envelope)).toBe(true);
    }
  });

  it("keeps ordinary transcript patches in the batching path", () => {
    const envelope: WorkbenchDeltaEnvelope = {
      generation: 12,
      delta: {
        kind: "conversationMessagesPatched",
        payload: {
          conversationId: "conversation",
          upsertMessages: [],
          removeMessageIds: [],
        },
      },
    };

    expect(workbenchEnvelopeRequiresImmediateLiveFlush(envelope)).toBe(false);
  });
});
