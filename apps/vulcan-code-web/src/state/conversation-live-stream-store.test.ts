import { describe, expect, it, vi } from "vitest";

import {
  ConversationLiveStreamStore,
  toolRequestProgressKey,
  transientReasoningStatusKey,
} from "./conversation-live-stream-store";
import type { WorkbenchDeltaEnvelope } from "@/types";

// envelope wraps one exact Workbench delta for Store tests.
// envelope 为存储测试封装一条精确 Workbench 增量。
// Parameters: generation is the transport revision and delta is the discriminated payload.
// 参数：generation 是传输修订号，delta 是判别式载荷。
// Returns: one typed Workbench envelope.
// 返回值：一条带类型的 Workbench 信封。
function envelope(
  generation: number,
  delta: WorkbenchDeltaEnvelope["delta"],
): WorkbenchDeltaEnvelope {
  return { generation, delta };
}

describe("ConversationLiveStreamStore", () => {
  it("replaces transient reasoning and clears it at the explicit boundary", () => {
    const store = new ConversationLiveStreamStore();
    const listener = vi.fn();
    store.subscribe(transientReasoningStatusKey("session-1"), listener);

    expect(store.applyEnvelope(envelope(1, {
      kind: "transientReasoningStatusChanged",
      payload: {
        conversationId: "session-1",
        status: {
          kind: "summary",
          summaries: [{ itemId: "summary-1", text: "first" }],
          reasoningTokens: 12,
        },
      },
    }))).toBe(true);
    expect(store.getTransientReasoningStatus("session-1")?.summaries?.[0]?.text).toBe("first");
    expect(store.flushNotifications()).toBe(1);
    expect(listener).toHaveBeenCalledTimes(1);

    store.resetForWorkbenchSnapshot();
    expect(store.getTransientReasoningStatus("session-1")).toBeNull();
    expect(store.flushNotifications()).toBe(1);
    expect(listener).toHaveBeenCalledTimes(2);
  });

  it("accepts monotonic tool progress and prevents terminal resurrection", () => {
    const store = new ConversationLiveStreamStore();
    const listener = vi.fn();
    store.subscribe(toolRequestProgressKey("session-1", "request-1"), listener);
    const progress = {
      requestStreamId: "request-1",
      toolCallId: "tool-1",
      toolName: "file",
      state: "receiving" as const,
      receivedBytes: 4,
      receivedCharacters: 4,
      requestRevision: 1,
    };

    expect(store.applyEnvelope(envelope(1, {
      kind: "toolRequestProgressChanged",
      payload: {
        conversationId: "session-1",
        requestStreamId: "request-1",
        progress,
      },
    }))).toBe(true);
    expect(store.getToolRequestProgress("session-1", "request-1")).toEqual(progress);
    expect(store.flushNotifications()).toBe(1);

    expect(store.applyEnvelope(envelope(2, {
      kind: "toolRequestProgressChanged",
      payload: {
        conversationId: "session-1",
        requestStreamId: "request-1",
        progress: null,
      },
    }))).toBe(true);
    expect(store.getToolRequestProgress("session-1", "request-1")).toBeNull();
    expect(store.flushNotifications()).toBe(1);
    expect(listener).toHaveBeenCalledTimes(2);

    expect(store.applyEnvelope(envelope(3, {
      kind: "toolRequestProgressChanged",
      payload: {
        conversationId: "session-1",
        requestStreamId: "request-1",
        progress: { ...progress, requestRevision: 2 },
      },
    }))).toBe(false);
    expect(store.flushNotifications()).toBe(0);
  });
});
