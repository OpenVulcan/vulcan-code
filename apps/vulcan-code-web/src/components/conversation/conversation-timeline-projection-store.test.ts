import { describe, expect, it, vi } from "vitest";
import type { ConversationMessage } from "@/types";
import type { ConversationTurnProjection } from "./conversation-timeline-projection-store";
import { ConversationTimelineProjectionStore } from "./conversation-timeline-projection-store";

// createSourceMessage builds one immutable authoritative message reference for projection tests.
// createSourceMessage 为投影测试构建一个不可变权威消息引用。
// Parameters: id is the stable backend message identity.
// 参数：id 是稳定后端消息身份。
// Returns: a minimal typed transcript message.
// 返回值：一条最小类型化转录消息。
function createSourceMessage(id: string): ConversationMessage {
  return {
    body: `body-${id}`,
    conversationId: "conversation-1",
    createdAt: "1",
    id,
    role: "user",
  };
}

// createTurnProjection builds one source-backed user Turn.
// createTurnProjection 构建一个由来源消息支撑的用户 Turn。
// Parameters: index selects stable identity and source is the exact immutable transcript object.
// 参数：index 选择稳定身份；source 是精确不可变转录对象。
// Returns: one typed Turn projection.
// 返回值：一个类型化 Turn 投影。
function createTurnProjection(index: number, source: ConversationMessage): ConversationTurnProjection {
  return {
    key: `turn-${index}`,
    items: [{
      kind: "item",
      item: {
        blocks: [],
        id: source.id,
        renderKey: `item:${source.id}`,
        role: "user",
        sourceMessage: source,
        sourceMessages: [source],
        userMessage: source,
      },
    }],
  };
}

describe("ConversationTimelineProjectionStore", () => {
  it("uses an explicit first-mount fallback without mutating committed Store state", () => {
    // fallback is the parent projection visible before the layout commit reconciles a new Seat.
    // fallback 是布局提交协调新 Seat 前父级可见的投影。
    const fallback = createTurnProjection(0, createSourceMessage("first-mount"));
    const store = new ConversationTimelineProjectionStore();

    expect(store.getRowSnapshotOr(fallback.key, fallback)).toBe(fallback);
    expect(store.getOrderSnapshot()).toEqual([]);
    expect(() => store.getRowSnapshot(fallback.key)).toThrow("Conversation timeline row is not registered");
  });

  it("retains all three hundred historical Turn references across an unchanged parent projection", () => {
    // sources model immutable workbench message objects shared across parent projections.
    // sources 模拟在父级投影之间共享的不可变工作台消息对象。
    const sources = Array.from({ length: 300 }, (_, index) => createSourceMessage(`message-${index}`));
    // store owns stable row snapshots independently from the parent array identity.
    // store 独立于父数组身份拥有稳定行快照。
    const store = new ConversationTimelineProjectionStore();
    store.reconcile(sources.map((source, index) => createTurnProjection(index, source)));
    store.flushNotifications();
    // retainedSnapshots records exact objects before the parent reconstructs every projection wrapper.
    // retainedSnapshots 记录父级重建全部投影包装前的精确对象。
    const retainedSnapshots = store.getOrderSnapshot().map((rowKey) => store.getRowSnapshot(rowKey));
    // listener verifies an unchanged middle historical Turn receives no notification.
    // listener 验证未变化的中部历史 Turn 不会收到通知。
    const listener = vi.fn();
    store.subscribe("turn-150", listener);

    store.reconcile(sources.map((source, index) => createTurnProjection(index, source)));
    store.flushNotifications();

    expect(listener).not.toHaveBeenCalled();
    expect(store.getOrderSnapshot().map((rowKey) => store.getRowSnapshot(rowKey))).toEqual(retainedSnapshots);
    expect(store.getRowSnapshot("turn-150")).toBe(retainedSnapshots[150]);
  });

  it("notifies only the Turn whose exact contributing message reference changed", () => {
    // sources provide three distinct immutable row owners.
    // sources 提供三个不同的不可变行所有者。
    const sources = [createSourceMessage("one"), createSourceMessage("two"), createSourceMessage("three")];
    const store = new ConversationTimelineProjectionStore();
    store.reconcile(sources.map((source, index) => createTurnProjection(index, source)));
    store.flushNotifications();
    // listeners observe independent exact row subscriptions.
    // listeners 观察相互独立的精确行订阅。
    const listeners = [vi.fn(), vi.fn(), vi.fn()];
    listeners.forEach((listener, index) => store.subscribe(`turn-${index}`, listener));
    // changedSource is the authoritative immutable replacement for only the second Turn.
    // changedSource 仅是第二个 Turn 的权威不可变替换对象。
    const changedSource = { ...sources[1], body: "updated" };

    store.reconcile([
      createTurnProjection(0, sources[0]),
      createTurnProjection(1, changedSource),
      createTurnProjection(2, sources[2]),
    ]);
    store.flushNotifications();

    expect(listeners[0]).not.toHaveBeenCalled();
    expect(listeners[1]).toHaveBeenCalledTimes(1);
    expect(listeners[2]).not.toHaveBeenCalled();
  });

  it("invalidates only rows selected by an explicit external-runtime predicate", () => {
    // store contains two stable rows before a command-runtime dependency changes.
    // store 在命令运行时依赖变化前包含两条稳定行。
    const store = new ConversationTimelineProjectionStore();
    const sources = [createSourceMessage("plain"), createSourceMessage("command")];
    store.reconcile(sources.map((source, index) => createTurnProjection(index, source)));
    store.flushNotifications();
    // listeners confirm that explicit external invalidation stays row-scoped.
    // listeners 确认显式外部失效仍保持行级作用域。
    const plainListener = vi.fn();
    const commandListener = vi.fn();
    store.subscribe("turn-0", plainListener);
    store.subscribe("turn-1", commandListener);

    expect(store.invalidateRows((row) => row.key === "turn-1")).toBe(1);
    store.flushNotifications();

    expect(plainListener).not.toHaveBeenCalled();
    expect(commandListener).toHaveBeenCalledTimes(1);
  });
});
