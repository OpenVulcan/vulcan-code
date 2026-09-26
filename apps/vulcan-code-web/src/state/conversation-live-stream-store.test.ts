import { describe, expect, it, vi } from "vitest";

import {
  ConversationLiveStreamStore,
  toolRequestProgressKey,
  toolRequestMembershipKey,
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
      apiCallId: 1,
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
    expect(store.flushNotifications()).toBe(2);

    expect(store.applyEnvelope(envelope(2, {
      kind: "toolRequestProgressChanged",
      payload: {
        conversationId: "session-1",
        requestStreamId: "request-1",
        progress: null,
      },
    }))).toBe(true);
    expect(store.getToolRequestProgress("session-1", "request-1")).toBeNull();
    expect(store.flushNotifications()).toBe(2);
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

  it("notifies branch membership only on insertion/removal and retires failed API drafts", () => {
    // Independent listeners verify that tokens never fan out to branch subscribers.
    // 独立监听器验证 Token 绝不扩散到分支订阅者。
    const store = new ConversationLiveStreamStore();
    const membership = vi.fn();
    const leaf = vi.fn();
    store.subscribe(toolRequestMembershipKey("session"), membership);
    store.subscribe(toolRequestProgressKey("session", "request"), leaf);
    // Safe progress has no raw arguments and remains bound to one API.
    // 安全进度不含原始参数，且保持绑定单个 API。
    const progress = { apiCallId: 7, requestStreamId: "request", toolCallId: "call",
      toolName: "apply_patch", state: "receiving" as const,
      receivedBytes: 0, receivedCharacters: 0, requestRevision: 1 };
    store.applyEnvelope(envelope(1, { kind: "toolRequestProgressChanged",
      payload: { conversationId: "session", requestStreamId: "request", progress } }));
    store.flushNotifications();
    const seed = store.getToolRequestMembership("session");
    for (let revision = 2; revision <= 10; revision++) {
      store.applyEnvelope(envelope(revision, { kind: "toolRequestProgressChanged",
        payload: { conversationId: "session", requestStreamId: "request",
          progress: { ...progress, requestRevision: revision, receivedCharacters: revision } } }));
      store.flushNotifications();
    }
    expect(store.getToolRequestMembership("session")).toBe(seed);
    expect(membership).toHaveBeenCalledTimes(1);
    expect(leaf).toHaveBeenCalledTimes(10);
    expect(store.settleToolRequestApi("session", 8)).toBe(false);
    expect(store.settleToolRequestApi("session", 7)).toBe(true);
    store.flushNotifications();
    expect(store.getToolRequestMembership("session")).toEqual([]);
    expect(membership).toHaveBeenCalledTimes(2);
    expect(store.applyEnvelope(envelope(11, { kind: "toolRequestProgressChanged",
      payload: { conversationId: "session", requestStreamId: "request",
        progress: { ...progress, requestRevision: 11 } } }))).toBe(false);
  });
});
