import { describe, expect, it } from "vitest";

import { browserFallbackSnapshot } from "@/service/browser-fallback-workbench";
import {
  applySessionAddonsChangedResponse,
  mergeWorkbenchSnapshot,
} from "@/state/workbench-state";

// Verifies a lower revision remains stale inside one service instance.
// 验证同一服务实例内较低版本仍会被视为过期。
describe("mergeWorkbenchSnapshot service instance ordering", () => {
  it("rejects a lower revision without an instance reset", () => {
    const current = {
      ...browserFallbackSnapshot(),
      appVersion: "old-instance",
      revision: 900,
    };
    const incoming = {
      ...browserFallbackSnapshot(),
      appVersion: "new-instance",
      revision: 3,
    };

    expect(mergeWorkbenchSnapshot(current, incoming)).toBe(current);
  });

  // Verifies one confirmed service-instance transition can establish a new revision baseline.
  // 验证一次已确认的服务实例切换可以建立新的版本基线。
  it("accepts a lower revision after a confirmed instance reset", () => {
    const current = {
      ...browserFallbackSnapshot(),
      appVersion: "old-instance",
      revision: 900,
    };
    const incoming = {
      ...browserFallbackSnapshot(),
      appVersion: "new-instance",
      revision: 3,
    };

    const merged = mergeWorkbenchSnapshot(current, incoming, {
      allowServiceRevisionReset: true,
    });

    expect(merged).not.toBe(current);
    expect(merged.revision).toBe(3);
    expect(merged.appVersion).toBe("new-instance");
  });
});

// Verifies a context-window node is sufficient to retain one session addon projection.
// 验证上下文窗口节点足以保留一条会话附属投影。
describe("applySessionAddonsChangedResponse context window", () => {
  it("retains a node-id-only session addon response", () => {
    const current = browserFallbackSnapshot();
    const next = applySessionAddonsChangedResponse(current, {
      conversationId: "conversation-a",
      revision: (current.revision ?? 0) + 1,
      sessionContextCompactionWindowNodeId: "compression-window-2",
    });

    expect(next.sessionAddonsByConversation).toEqual([{
      conversationId: "conversation-a",
      revision: (current.revision ?? 0) + 1,
      sessionContextCompactionWindowNodeId: "compression-window-2",
    }]);
  });
});
