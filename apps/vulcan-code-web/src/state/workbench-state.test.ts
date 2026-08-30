import { describe, expect, it } from "vitest";

import { browserFallbackSnapshot } from "@/service/browser-fallback-workbench";
import {
  applySubagentPanelRouteResponse,
  applySessionAddonsChangedResponse,
  applyWorkbenchDelta,
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
