import { describe, expect, it, vi } from "vitest";

import {
  ConversationLiveStreamStore,
  toolRequestProgressKey,
  toolRequestMembershipKey,
  transientReasoningStatusKey,
} from "./conversation-live-stream-store";
import type { ToolRequestProgressSnapshot, WorkbenchDeltaEnvelope } from "@/types";

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
  // Exercise the common protocol rather than assuming any particular tool renderer.
  // 验证公共协议，不依赖任何特定工具渲染器。
  it("retains semantic previews across progress-only updates without crossing request identities", () => {
    // Store lifetime bounds all previews to active requests in the current workbench.
    // 存储生命周期将全部预览限定在当前工作台的活动请求内。
    const store = new ConversationLiveStreamStore();
    // This minimal sanitized projection is opaque to the common stream store.
    // 该最小净化投影对公共流存储而言是不透明数据。
    const preview = { kind: "file_write", revision: 1,
      title_fields: { path: "first.ts" }, progress_fields: { new_line_count: 150 } };
    // The source identity is checked before retaining any previous preview.
    // 源身份会在保留任何历史预览之前接受校验。
    const seed: ToolRequestProgressSnapshot = { apiCallId: 1, requestStreamId: "request",
      toolCallId: "call", toolName: "file", state: "receiving",
      receivedBytes: 100, receivedCharacters: 100, requestRevision: 1, preview };
    // Publish a typed request update into an explicit conversation; return whether it was accepted.
    // 将带类型的请求更新发布到明确的会话；返回是否接受该更新。
    const publish = (progress: ToolRequestProgressSnapshot, conversationId = "session") => store.applyEnvelope(
      envelope(progress.requestRevision, { kind: "toolRequestProgressChanged", payload: {
        conversationId, requestStreamId: progress.requestStreamId, progress,
      } }),
    );
    expect(publish(seed)).toBe(true);
    // Keep object identity stable on counter-only traffic, avoiding unnecessary semantic projection work.
    // 纯计数流量保持对象身份稳定，避免不必要的语义投影工作。
    const retained = store.getToolRequestProgress("session", "request")?.preview;
    expect(retained).toEqual(preview);
    for (const [index, state] of (["receiving", "arguments_assembled", "ready"] as const).entries()) {
      expect(publish({ ...seed, state, requestRevision: index + 2,
        receivedBytes: 200 + index, preview: index === 0 ? undefined : null })).toBe(true);
      expect(store.getToolRequestProgress("session", "request")).toMatchObject({ state, receivedBytes: 200 + index });
      expect(store.getToolRequestProgress("session", "request")?.preview).toBe(retained);
    }
    // A later preview may correct counts downward; authoritative replacements must not be max-merged.
    // 后续预览可向下校正行数；权威替换不能使用最大值合并。
    const corrected = { ...preview, revision: 2, progress_fields: { new_line_count: 149 } };
    expect(publish({ ...seed, requestRevision: 5, preview: corrected })).toBe(true);
    expect(store.getToolRequestProgress("session", "request")?.preview).toEqual(corrected);
    expect(publish(seed)).toBe(false);
    expect(publish({ ...seed, toolCallId: "other", requestRevision: 6 })).toBe(false);
    expect(publish({ ...seed, apiCallId: 2, requestRevision: 6 })).toBe(false);
    expect(store.getToolRequestProgress("session", "request")?.preview).toEqual(corrected);
    expect(publish({ ...seed, requestStreamId: "other-request", preview: null })).toBe(true);
    expect(publish({ ...seed, preview: null }, "other-session")).toBe(true);
    expect(store.getToolRequestProgress("session", "other-request")?.preview).toBeNull();
    expect(store.getToolRequestProgress("other-session", "request")?.preview).toBeNull();
    expect(store.settleToolRequestProgress("session", "request")).toBe(true);
    expect(publish({ ...seed, requestRevision: 6 })).toBe(false);
    expect(store.getToolRequestProgress("session", "request")).toBeNull();
    expect(publish({ ...seed, requestStreamId: "reset-request" })).toBe(true);
    store.resetForWorkbenchSnapshot();
    expect(store.getToolRequestProgress("session", "reset-request")).toBeNull();
    expect(publish({ ...seed, requestStreamId: "reset-request", preview: null })).toBe(true);
    expect(store.getToolRequestProgress("session", "reset-request")?.preview).toBeNull();
  });

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
