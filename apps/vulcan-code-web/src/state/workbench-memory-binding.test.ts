import { describe, expect, it } from "vitest";
import { browserFallbackSnapshot } from "@/service/browser-fallback-workbench";
import { applyWorkbenchDelta } from "@/state/workbench-state";
import type { ContextInjectionSnapshot } from "@/types/runtime-context";

// Builds the same per-session injection projection carried by native binding management events.
// 构建原生绑定管理事件携带的相同会话级注入投影。
// Parameters: body is the previous recalled text. Returns: one nonempty dynamic memory card.
// 参数：body 为此前召回正文。返回值：一张非空动态记忆卡片。
function memory(body: string): ContextInjectionSnapshot {
  return { dynamicCards: [{ tag: "vmm", description: "Memory", items: [
    { content: body, detail: body, ttlLabel: "TTL 3", memoryId: 11 },
  ] }], staticCards: [], staticProfile: null };
}

// Verifies focused memory invalidation affects only the named session and cannot be undone by a stale delta.
// 验证聚焦记忆失效仅影响指定会话，且不能被旧增量撤销。
describe("native memory binding context invalidation", () => {
  // Parameters: selected determines which panel is currently visible. Returns: nothing; asserts exact state projection.
  // 参数：selected 决定当前可见面板。返回值：无；断言准确状态投影。
  it.each(["affected", "peer"])("clears an evicted card while %s is selected", (selected) => {
    // Each owner has distinct text so an accidental cross-session replacement is observable.
    // 每个归属持有不同正文，使误跨会话替换可观察。
    const oldMemory = memory("obsolete native binding memory");
    // The peer's card remains independent of the affected binding.
    // 其他会话卡片独立于受影响绑定。
    const peerMemory = memory("keep peer memory");
    // The baseline contains the retained card even though no runtime hydration is represented in this client state.
    // 基线保留旧卡片，而客户端状态不表示任何运行态水合。
    const current = {
      ...browserFallbackSnapshot(), revision: 10, selectedConversationId: selected,
      conversations: [
        { id: "affected", title: "Affected", status: "ready" },
        { id: "peer", title: "Peer", status: "ready" },
      ],
      contextInjection: selected === "affected" ? oldMemory : peerMemory,
      contextInjectionByConversation: [
        { conversationId: "affected", contextInjection: oldMemory },
        { conversationId: "peer", contextInjection: peerMemory },
      ],
    };
    // This is the explicit null projection emitted for an evicted native session after rebinding.
    // 这是原生会话重绑后针对已驱逐会话发出的明确空投影。
    const cleared = applyWorkbenchDelta(current, { generation: 11, delta: {
      kind: "contextInjectionChanged", payload: { conversationId: "affected", contextInjection: null },
    } });
    expect(cleared.contextInjectionByConversation).toEqual([
      { conversationId: "peer", contextInjection: peerMemory },
    ]);
    expect(cleared.contextInjection).toEqual(selected === "affected" ? null : peerMemory);
    expect(cleared.conversations).toBe(current.conversations);
    // Older events must not resurrect either the removed projection or its provider-visible preview.
    // 旧事件不得复活已移除投影或其面向供应商的预览。
    const stale = applyWorkbenchDelta(cleared, { generation: 10, delta: {
      kind: "contextInjectionChanged", payload: { conversationId: "affected", contextInjection: oldMemory },
    } });
    expect(stale).toBe(cleared);
  });
});
