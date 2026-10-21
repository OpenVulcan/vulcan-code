import { describe, expect, it } from "vitest";

import { browserFallbackSnapshot } from "@/service/browser-fallback-workbench";
import type { SessionProviderUsageSummary } from "@/types/conversation";
import type { SessionUsageBucket } from "@/types/session-lifetime-usage";
import {
  applySubagentPanelRouteResponse,
  applySessionAddonsChangedResponse,
  applyWorkbenchDelta,
  mergeWorkbenchSnapshot,
} from "@/state/workbench-state";

// Verifies a lower revision remains stale inside one service instance.
// 验证同一服务实例内较低版本仍会被视为过期。
describe("mergeWorkbenchSnapshot service instance ordering", () => {
  // A cleared authoritative selection must not fall back to the local route or an arbitrary indexed row.
  // 权威空选择不能回退到本地路由或任意索引行。
  it("honors an explicitly cleared incoming selection with visible conversations", () => {
    // Both rows remain visible, modeling a metadata read failure that must not authorize deletion.
    // 两行仍可见，模拟不允许删除数据的元数据读取失败。
    const current = {
      ...browserFallbackSnapshot(), revision: 1, selectedConversationId: "unavailable",
      conversations: [{ id: "unavailable", title: "Unavailable", status: "ready" }, { id: "peer", title: "Peer", status: "ready" }],
    };
    const incoming = { ...current, revision: 2, selectedConversationId: null };
    expect(mergeWorkbenchSnapshot(current, incoming, { selectedConversationSource: "incoming" }).selectedConversationId).toBeNull();
    // Ordinary refresh still preserves a legitimate local choice when navigation owns it.
    // 普通刷新在导航拥有合法本地选择时仍保留它。
    expect(mergeWorkbenchSnapshot(current, incoming).selectedConversationId).toBe("unavailable");
  });

  // A removed target cannot reappear merely because another indexed row remains in the list.
  // 不能仅因列表中仍有其他索引行就复活已移除目标。
  it("keeps the cleared selection after missing-index cleanup and accepts a valid replacement", () => {
    const current = {
      ...browserFallbackSnapshot(), revision: 1, selectedConversationId: "missing",
      conversations: [{ id: "missing", title: "Missing", status: "ready" }],
    };
    const incoming = {
      ...current, revision: 2, selectedConversationId: null,
      conversations: [{ id: "peer", title: "Peer", status: "ready" }],
    };
    const merged = mergeWorkbenchSnapshot(current, incoming);
    expect(merged.selectedConversationId).toBeNull();
    expect(merged.conversations.map((conversation) => conversation.id)).toEqual(["peer"]);
    expect(mergeWorkbenchSnapshot(current, { ...incoming, selectedConversationId: "peer" }, { selectedConversationSource: "incoming" }).selectedConversationId).toBe("peer");
  });

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
  // A newer transport envelope must not replace newer usage with an older aggregate snapshot.
  // 较新的传输包不能用旧累计快照覆盖较新的用量。
  it("retains monotonic usage while applying other addon fields", () => {
    const empty: SessionUsageBucket = {
      requestCount: 0, unknownUsageCount: 0, toolCallCount: 0,
      inputTokens: { sum: 0, samples: 0 }, uncachedInputTokens: { sum: 0, samples: 0 },
      outputTokens: { sum: 0, samples: 0 }, reasoningTokens: { sum: 0, samples: 0 },
      cacheReadTokens: { sum: 0, samples: 0 }, cacheWriteTokens: { sum: 0, samples: 0 },
      totalTokens: { sum: 0, samples: 0 }, budgetTokens: { sum: 0, samples: 0 },
      firstTokenMs: { sum: 0, samples: 0 }, requestDurationMs: { sum: 0, samples: 0 },
      outputDurationMs: { sum: 0, samples: 0 }, measuredOutputTokens: 0,
      cacheInputTokens: { sum: 0, samples: 0 }, measuredCacheReadTokens: 0,
    };
    const usage: SessionProviderUsageSummary = {
      providerCallCount: 0, inFlightCallCount: 0, unknownUsageCallCount: 0, usageRevision: 8,
      lifetime: { version: 1, rootSessionId: "root", revision: 8, updatedAtMs: 1,
        main: empty, childrenTotal: empty, childrenBySessionId: {} },
    };
    const current = applySessionAddonsChangedResponse(browserFallbackSnapshot(), {
      conversationId: "root", revision: 100, sessionProviderUsage: usage,
    });
    const next = applySessionAddonsChangedResponse(current, {
      conversationId: "root", revision: 101, sessionContextCompactionWindowNodeId: "compression-window-2",
      sessionProviderUsage: { ...usage, usageRevision: 7, lifetime: { ...usage.lifetime!, revision: 7 } },
    });
    expect(next.sessionAddonsByConversation?.[0].sessionProviderUsage?.lifetime?.revision).toBe(8);
    expect(next.sessionAddonsByConversation?.[0].sessionContextCompactionWindowNodeId).toBe("compression-window-2");
    const staleSnapshot = { ...current, revision: 102, sessionAddonsByConversation: [{
      conversationId: "root", sessionProviderUsage: { ...usage, usageRevision: 6, lifetime: { ...usage.lifetime!, revision: 6 } },
    }] };
    expect(mergeWorkbenchSnapshot(next, staleSnapshot).sessionAddonsByConversation?.[0].sessionProviderUsage?.lifetime?.revision).toBe(8);
    expect(mergeWorkbenchSnapshot(next, staleSnapshot, { allowServiceRevisionReset: true })
      .sessionAddonsByConversation?.[0].sessionProviderUsage?.lifetime?.revision).toBe(6);
  });

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

// Builds a parent navigation fixture with one child in the requested lifecycle state.
// 构建包含一个指定生命周期状态子任务的父会话导航夹具。
// Parameters: runState is the authoritative child lifecycle state; returns its workbench snapshot.
// 参数：runState 是权威子任务生命周期状态；返回对应工作台快照。
function subagentLifecycleWorkbench(runState: string): ReturnType<typeof browserFallbackSnapshot> {
  return {
    ...browserFallbackSnapshot(),
    revision: 7,
    selectedConversationId: "parent-conversation",
    conversations: [{ id: "parent-conversation", title: "Parent", status: "ready" }],
    subagentNavigationByConversation: [{
      parentConversationId: "parent-conversation",
      activeConversationId: "parent-conversation",
      activeTaskId: null,
      tasks: [{
        taskId: "child",
        publicTaskId: "child",
        label: "Child",
        runState,
        isRunning: false,
        isActive: false,
      }],
    }],
  };
}

// Verifies execution updates cannot undo archive lifecycle or revive dismissed navigation membership.
// 验证执行更新不能撤销归档生命周期，也不能恢复已移除的导航成员。
describe("subagent navigation lifecycle authority", () => {
  // Verifies a live closure push updates standby immediately without any child-page request.
  // 验证实时关闭推送立即更新待命状态，不需要请求子任务页面。
  // Parameters: none. Returns: none; asserts both the parent-owned list and selected list change.
  // 参数：无。返回值：无；断言父会话列表与当前选中列表同时变化。
  it("closes a standby child from the navigation delta without reopening its page", () => {
    // closed contains the same complete navigation payload emitted by the backend close boundary.
    // closed 包含与后端关闭边界发出的相同完整导航负载。
    const closed = subagentLifecycleWorkbench("closed").subagentNavigationByConversation?.[0];
    if (!closed) throw new Error("The lifecycle fixture must contain its parent navigation.");
    // next receives only the pushed delta; no panel-route load participates in this transition.
    // next 只接收推送增量；此次转换不涉及面板路由加载。
    const next = applyWorkbenchDelta(subagentLifecycleWorkbench("completed"), {
      generation: 8,
      delta: {
        kind: "subagentNavigationChanged",
        payload: { parentConversationId: "parent-conversation", subagentNavigation: closed },
      },
    });
    expect(next.subagentNavigationByConversation).toEqual([closed]);
    expect(next.subagentNavigation?.tasks?.[0]).toMatchObject({ runState: "closed", isRunning: false });
  });

  it.each(["completed", "ready", "idle", "stopped", "running"])(
    "preserves closed state after a late %s execution phase",
    (runPhase) => {
      // current retains the archived child for inspection in the closing turn.
      // current 为关闭当轮的查看保留已归档子任务。
      const current = subagentLifecycleWorkbench("closed");
      // next accepts a newer execution delta without reopening its navigation row.
      // next 接收更新的执行增量，但不得重新打开导航行。
      const next = applyWorkbenchDelta(current, {
        generation: 8,
        delta: { kind: "updateConversationRunPhase", payload: { conversationId: "child", runPhase } },
      });
      expect(next.subagentNavigationByConversation).toEqual(current.subagentNavigationByConversation);
      expect(next.subagentNavigation?.tasks?.[0]).toMatchObject({ runState: "closed", isRunning: false });
    },
  );

  it("moves a row's update time forward only when the phase delta carries an update label", () => {
    const current = subagentLifecycleWorkbench("completed");
    const before = current.conversations.find((conversation) => conversation.id === "parent-conversation")?.lastUpdatedAt;
    const phaseOnly = applyWorkbenchDelta(current, {
      generation: 8,
      delta: { kind: "updateConversationRunPhase", payload: { conversationId: "parent-conversation", runPhase: "running" } },
    });
    expect(phaseOnly.conversations.find((conversation) => conversation.id === "parent-conversation")?.lastUpdatedAt).toBe(before);
    const startedAt = Date.now();
    const labelled = applyWorkbenchDelta(current, {
      generation: 8,
      delta: { kind: "updateConversationRunPhase", payload: { conversationId: "parent-conversation", runPhase: "running", lastUpdatedLabel: "now" } },
    });
    expect(Number(labelled.conversations.find((conversation) => conversation.id === "parent-conversation")?.lastUpdatedAt)).toBeGreaterThanOrEqual(startedAt);
  });

  it("still updates the execution phase of an unclosed child", () => {
    // next proves the archive guard does not suppress legitimate new work on an open child.
    // next 证明归档保护不会抑制未关闭子任务的合法新执行。
    const next = applyWorkbenchDelta(subagentLifecycleWorkbench("completed"), {
      generation: 8,
      delta: { kind: "updateConversationRunPhase", payload: { conversationId: "child", runPhase: "running" } },
    });
    expect(next.subagentNavigation?.tasks?.[0]).toMatchObject({ runState: "running", isRunning: true });
  });

  it("does not revive a dismissed closed child after a late phase or a historical click", () => {
    // cleared is the authoritative next-turn navigation removal.
    // cleared 是权威的下一回合导航移除结果。
    const cleared = applyWorkbenchDelta(subagentLifecycleWorkbench("closed"), {
      generation: 8,
      delta: {
        kind: "subagentNavigationChanged",
        payload: { parentConversationId: "parent-conversation", subagentNavigation: null },
      },
    });
    // updated receives a late child execution update after removal.
    // updated 在移除后收到迟到的子任务执行更新。
    const updated = applyWorkbenchDelta(cleared, {
      generation: 9,
      delta: { kind: "updateConversationRunPhase", payload: { conversationId: "child", runPhase: "completed" } },
    });
    // opened requests the persisted transcript without admitting the child to current navigation.
    // opened 请求持久化转录，但不将子任务准入当前导航。
    const opened = applySubagentPanelRouteResponse(updated, {
      parentConversationId: "parent-conversation",
      activeConversationId: "child",
      subagentNavigation: null,
      revision: 9,
    });
    expect(opened.subagentNavigationByConversation).toEqual([]);
    expect(opened.subagentNavigation).toBeNull();
  });
});

// Verifies a route-only persisted child load cannot erase the current parent navigation list.
// 验证仅路由的持久化子会话加载不会清除当前父会话导航列表。
describe("applySubagentPanelRouteResponse route-only hydration", () => {
  it("preserves retained closed rows when the route response has no navigation snapshot", () => {
    const current = {
      ...browserFallbackSnapshot(),
      revision: 7,
      selectedConversationId: "parent-conversation",
      conversations: [{
        id: "parent-conversation",
        title: "Parent conversation",
        status: "ready",
      }],
      subagentNavigationByConversation: [{
        parentConversationId: "parent-conversation",
        activeConversationId: "parent-conversation",
        activeTaskId: null,
        tasks: [{
          taskId: "closed-child",
          publicTaskId: "closed-child",
          label: "Closed child",
          runState: "closed",
          isRunning: false,
          isActive: false,
        }],
      }],
    };

    const next = applySubagentPanelRouteResponse(current, {
      parentConversationId: "parent-conversation",
      activeConversationId: "closed-child",
      subagentNavigation: null,
      revision: 7,
    });

    expect(next.subagentNavigationByConversation).toEqual(
      current.subagentNavigationByConversation,
    );
  });
});

describe("LuaSkills operation progress delta", () => {
  // operationProgressDelta builds one focused install-progress replacement.
  // operationProgressDelta 构建一条聚焦的安装进度替换增量。
  const operationProgressDelta = (generation: number) => ({
    generation,
    delta: {
      kind: "luaSkillsOperationProgressChanged" as const,
      payload: { progress: {
        correlationId: "luaskills-op-0", action: "install", inFlight: true,
        overflowed: false, sawUpstreamTerminal: false, phases: 1,
        latestPhase: "downloading_archive", percent: 10,
      } },
    },
  });

  it("replaces only operation progress and leaves the rest of the catalog intact", () => {
    const current = browserFallbackSnapshot();
    const next = applyWorkbenchDelta(current, operationProgressDelta(1));
    expect(next.luaSkills.operationProgress?.correlationId).toBe("luaskills-op-0");
    expect(next.luaSkills.operationProgress?.latestPhase).toBe("downloading_archive");
    // The focused envelope owns one field, so the skill rows must be untouched.
    // 聚焦 envelope 只拥有一个字段，因此技能行必须保持不变。
    expect(next.luaSkills.skills).toEqual(current.luaSkills.skills);
  });

  it("applies at an equal generation because it owns its own lifecycle domain", () => {
    // This is what makes a reconnect work: progress owns an independent lifecycle
    // domain, so the generation gate must not drop it. If the delta kind is missing
    // from that list the update is silently discarded instead.
    // 这正是重连得以工作的原因：进度拥有独立生命周期域，因此 generation 门禁不得丢弃它。
    // 若该增量类型不在该列表中，更新会被静默丢弃。
    const current = { ...browserFallbackSnapshot(), revision: 4 };
    const next = applyWorkbenchDelta(current, operationProgressDelta(4));
    expect(next.luaSkills.operationProgress?.correlationId).toBe("luaskills-op-0");
  });
});

describe("conversation archive selection", () => {
  // archiveWorkbench holds one selected ordinary session, a floating assistant, and optionally another ordinary session.
  // archiveWorkbench 包含一个选中的普通会话、一个浮动助手，以及可选的另一个普通会话。
  function archiveWorkbench(withOrdinarySibling: boolean): ReturnType<typeof browserFallbackSnapshot> {
    const base = browserFallbackSnapshot();
    return {
      ...base,
      selectedConversationId: "ordinary-1",
      conversations: [
        { id: "assistant-1", title: "Assistant", status: "idle", projectId: "project-1", dedicated: true, assistantName: "Helper" },
        { id: "ordinary-1", title: "Ordinary 1", status: "idle", projectId: "project-1" },
        ...(withOrdinarySibling ? [{ id: "ordinary-2", title: "Ordinary 2", status: "idle", projectId: "project-1" }] : []),
      ],
    };
  }

  it("never falls back to an assistant session after archiving the selected session", () => {
    const next = applyWorkbenchDelta(archiveWorkbench(true), {
      generation: (archiveWorkbench(true).revision ?? 0) + 1,
      delta: { kind: "conversationArchived", payload: { conversationId: "ordinary-1", selectedConversationId: "assistant-1" } },
    });
    expect(next.selectedConversationId).toBe("ordinary-2");
  });

  it("clears the selection when only assistant sessions remain", () => {
    const next = applyWorkbenchDelta(archiveWorkbench(false), {
      generation: (archiveWorkbench(false).revision ?? 0) + 1,
      delta: { kind: "conversationArchived", payload: { conversationId: "ordinary-1", selectedConversationId: null } },
    });
    expect(next.selectedConversationId).toBeNull();
  });
});

// Verifies a child agent's blocked request reaches the parent conversation, which is the only place it can be seen.
// 验证子代理的阻塞请求进入父会话，那是唯一能看到它的地方。
describe("child agent pending requests", () => {
  // question builds one approval request owned by the given session.
  // question 构建一条由指定会话持有的审批请求。
  function question(id: string, sessionId: string, sequence: number) {
    return sessionId === "child"
      ? { id, sessionId, sequence, questions: [], ownerConversationId: "parent-conversation", requesterLabel: "Child" }
      : { id, sessionId, sequence, questions: [] };
  }

  it("shows the child's request in the parent and names the child", () => {
    const next = applyWorkbenchDelta(subagentLifecycleWorkbench("running"), {
      generation: 8,
      delta: {
        kind: "pendingQuestionChanged",
        payload: { conversationId: "child", pendingQuestion: question("q-child", "child", 1), pendingQuestions: [question("q-child", "child", 1)] },
      },
    });
    expect(next.pendingQuestion?.id).toBe("q-child");
    expect(next.pendingQuestionRequester).toBe("Child");
    expect(next.conversations.find((conversation) => conversation.id === "parent-conversation")?.attention?.kind).toBe("ask");
  });

  it("keeps the parent's own request ahead of the child's", () => {
    const pending = [question("q-child", "child", 1), question("q-parent", "parent-conversation", 2)];
    const next = applyWorkbenchDelta(subagentLifecycleWorkbench("running"), {
      generation: 8,
      delta: {
        kind: "pendingQuestionChanged",
        payload: { conversationId: "child", pendingQuestion: pending[0], pendingQuestions: pending },
      },
    });
    expect(next.pendingQuestion?.id).toBe("q-parent");
    expect(next.pendingQuestionRequester).toBeNull();
  });
});
