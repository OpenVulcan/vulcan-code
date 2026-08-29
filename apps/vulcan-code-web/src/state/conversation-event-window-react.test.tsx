import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";

import {
  ConversationEventWindowExternalStoreProvider,
  createConversationEventWindowExternalStore,
  useSelectedConversationEventWindowCommands,
  useSelectedConversationEventWindowSnapshot,
  useSelectedConversationEventWindowStatus,
  useConversationEventWindowSnapshot,
  type ConversationEventWindowOwnedStore,
  type ConversationEventWindowReadableStore,
} from "./conversation-event-window-react";
import type { ConversationEventWindowControllerSnapshot } from "@/service/conversation-event-window-controller";
import type {
  SelectedConversationEventWindowCommands,
  SelectedConversationEventWindowStatus,
} from "@/service/selected-conversation-event-window-registry";

/** Builds one stable empty controller snapshot for external-store tests.
 * 为外部 store 测试构建一份稳定的空控制器快照。 */
function emptySnapshot(memoryRevision: number): ConversationEventWindowControllerSnapshot {
  return {
    owner: { session_id: "session-react-store", window_generation: 1 },
    service_epoch: "service-epoch-react-store",
    memory_revision: memoryRevision,
    durable_revision: memoryRevision,
    events: [],
    older_before_seq: null,
    has_more: false,
    open_turn_id: null,
    open_turn_start_seq: null,
    session_config: null,
  };
}

/** Builds one stable ready lifecycle for the selected-session React bridge.
 * 为选中会话 React 桥构建一个稳定的已就绪生命周期。 */
function readyStatus(): SelectedConversationEventWindowStatus {
  return { kind: "ready", session_id: "session-react-store" };
}

/** Renders the revision selected through React useSyncExternalStore.
 * 渲染通过 React useSyncExternalStore 选中的修订号。 */
function SnapshotProbe(props: { store: ConversationEventWindowReadableStore }) {
  // snapshot is obtained exclusively through the external-store hook.
  // snapshot 仅通过外部 store hook 获取。
  const snapshot = useConversationEventWindowSnapshot(props.store);
  return <span data-memory-revision={snapshot.memory_revision}>{snapshot.memory_revision}</span>;
}

/** Renders the WorkbenchApp-owned snapshot without receiving a controller prop.
 * 在不接收控制器 prop 的情况下渲染 WorkbenchApp 持有的快照。 */
function OwnedSnapshotProbe() {
  // snapshot comes only from the stable selected-session owner context.
  // snapshot 仅来自稳定的选中会话归属上下文。
  const snapshot = useSelectedConversationEventWindowSnapshot();
  return <span data-owner-session={snapshot.owner?.session_id ?? "none"} />;
}

/** Renders the selected-session lifecycle published independently from canonical events.
 * 渲染独立于 canonical 事件发布的选中会话生命周期。 */
function OwnedStatusProbe() {
  const status = useSelectedConversationEventWindowStatus();
  return <span data-window-status={status.kind} />;
}

/** Captures commands exposed by the WorkbenchApp-owned external-store boundary.
 * 捕获 WorkbenchApp 持有的外部 store 边界所暴露的命令。 */
function OwnedCommandsProbe(props: {
  /** Receives the exact owner-bound command object selected during rendering.
   * 接收渲染期间选中的精确归属绑定命令对象。 */
  onCommands(commands: SelectedConversationEventWindowCommands): void;
}) {
  // commands comes from the same selected owner publication as the snapshot hook.
  // commands 来自与快照 hook 相同的选中归属发布。
  const commands = useSelectedConversationEventWindowCommands();
  props.onCommands(commands);
  return <span data-command-surface="selected" />;
}

describe("conversation event window React external store", () => {
  it("binds stable snapshot and idempotent unsubscribe callbacks", () => {
    // snapshot is the exact immutable object returned by every read before publication.
    // snapshot 是发布前每次读取返回的同一不可变对象。
    const snapshot = emptySnapshot(7);
    // listenerSet models controller-owned subscriptions without a React runtime dependency.
    // listenerSet 模拟控制器持有的订阅，无需依赖 React 运行时。
    const listenerSet = new Set<() => void>();
    // readable exposes only the production controller surface required by the adapter.
    // readable 只暴露适配器需要的生产控制器表面。
    // commands is the exact owner-bound surface returned by the external store.
    // commands 是外部 store 返回的精确归属绑定表面。
    const commands: SelectedConversationEventWindowCommands = {
      rehydrate: vi.fn(async () => true),
      prependOlder: vi.fn(async () => true),
    };
    // readable exposes the full selected-owner surface used by WorkbenchApp.
    // readable 暴露 WorkbenchApp 使用的完整选中归属表面。
    const readable: ConversationEventWindowOwnedStore = {
      snapshot: () => snapshot,
      subscribe: (listener) => {
        listenerSet.add(listener);
        return () => listenerSet.delete(listener);
      },
      commands: () => commands,
      status: readyStatus,
    };
    // externalStore freezes the callback identities consumed by useSyncExternalStore.
    // externalStore 固定 useSyncExternalStore 消费的回调身份。
    const externalStore = createConversationEventWindowExternalStore(readable);
    // listener records notifications while the subscription remains active.
    // listener 在订阅有效期间记录通知。
    const listener = vi.fn();
    // unsubscribe removes exactly the registered listener.
    // unsubscribe 精确移除已注册的监听器。
    const unsubscribe = externalStore.subscribe(listener);

    expect(externalStore.getSnapshot()).toBe(snapshot);
    expect(externalStore.getServerSnapshot()).toBe(snapshot);
    expect(externalStore.getCommands()).toBe(commands);
    expect(externalStore.getStatus()).toEqual(readyStatus());
    for (const activeListener of listenerSet) activeListener();
    expect(listener).toHaveBeenCalledTimes(1);

    unsubscribe();
    unsubscribe();
    for (const activeListener of listenerSet) activeListener();
    expect(listener).toHaveBeenCalledTimes(1);
  });

  it("provides the authoritative server snapshot to the React bridge", () => {
    // snapshot is shared by the client and server snapshot functions.
    // snapshot 由客户端与服务端快照函数共享。
    const snapshot = emptySnapshot(11);
    // readable proves the hook depends on no legacy transcript or message store.
    // readable 证明 hook 不依赖旧 transcript 或 message store。
    // commands keeps the provider contract complete without invoking pagination in this test.
    // commands 在本测试不调用分页的情况下保持 provider 契约完整。
    const commands: SelectedConversationEventWindowCommands = {
      rehydrate: vi.fn(async () => true),
      prependOlder: vi.fn(async () => true),
    };
    // readable includes the selected-owner command surface required by the provider.
    // readable 包含 provider 所需的选中归属命令面。
    const readable: ConversationEventWindowOwnedStore = {
      snapshot: () => snapshot,
      subscribe: () => () => undefined,
      commands: () => commands,
      status: readyStatus,
    };
    // markup is produced through React's server-side useSyncExternalStore path.
    // markup 通过 React 服务端 useSyncExternalStore 路径生成。
    const markup = renderToStaticMarkup(<SnapshotProbe store={readable} />);

    expect(markup).toContain('data-memory-revision="11"');
    expect(markup).toContain(">11</span>");
  });

  it("exposes stable snapshot callbacks through the WorkbenchApp owner boundary", () => {
    // snapshot is the exact selected-session value exposed by the future ChatArea cutover.
    // snapshot 是后续 ChatArea 切换将消费的精确选中会话值。
    const snapshot = emptySnapshot(13);
    // readable remains independent from every legacy message or transcript store.
    // readable 保持独立于全部旧消息或 transcript store。
    // commands completes the WorkbenchApp owner contract without invoking pagination.
    // commands 在不调用分页的情况下补全 WorkbenchApp 归属契约。
    const commands: SelectedConversationEventWindowCommands = {
      rehydrate: vi.fn(async () => true),
      prependOlder: vi.fn(async () => true),
    };
    const readable: ConversationEventWindowOwnedStore = {
      snapshot: () => snapshot,
      subscribe: () => () => undefined,
      commands: () => commands,
      status: readyStatus,
    };
    // externalStore is created once by the WorkbenchApp owner.
    // externalStore 由 WorkbenchApp 归属方创建一次。
    const externalStore = createConversationEventWindowExternalStore(readable);
    // markup proves descendants can read the selected owner without a parallel renderer.
    // markup 证明后代可在不建立平行 renderer 的情况下读取选中归属。
    const markup = renderToStaticMarkup(
      <ConversationEventWindowExternalStoreProvider store={externalStore}>
        <OwnedSnapshotProbe />
        <OwnedStatusProbe />
      </ConversationEventWindowExternalStoreProvider>,
    );

    expect(markup).toContain('data-owner-session="session-react-store"');
    expect(markup).toContain('data-window-status="ready"');
  });

  it("exposes the exact owner-bound commands through the React provider", () => {
    // snapshot and commands are published by one selected-owner source.
    // snapshot 与 commands 由同一个选中归属来源发布。
    const snapshot = emptySnapshot(17);
    // commands records pagination without accessing any legacy history callback.
    // commands 在不访问任何旧历史回调的情况下记录分页。
    const commands: SelectedConversationEventWindowCommands = {
      rehydrate: vi.fn(async () => true),
      prependOlder: vi.fn(async () => true),
    };
    // ownedStore mirrors the registry surface installed by WorkbenchApp.
    // ownedStore 镜像 WorkbenchApp 安装的 registry 表面。
    const ownedStore: ConversationEventWindowOwnedStore = {
      snapshot: () => snapshot,
      subscribe: () => () => undefined,
      commands: () => commands,
      status: readyStatus,
    };
    // capturedCommands receives the exact command object observed by the hook.
    // capturedCommands 接收 hook 观察到的精确命令对象。
    let capturedCommands: SelectedConversationEventWindowCommands | null = null;
    // externalStore is the stable provider value installed above the workbench subtree.
    // externalStore 是安装在工作台子树上方的稳定 provider 值。
    const externalStore = createConversationEventWindowExternalStore(ownedStore);

    const markup = renderToStaticMarkup(
      <ConversationEventWindowExternalStoreProvider store={externalStore}>
        <OwnedCommandsProbe onCommands={(value) => {
          capturedCommands = value;
        }} />
      </ConversationEventWindowExternalStoreProvider>,
    );

    expect(markup).toContain('data-command-surface="selected"');
    expect(capturedCommands).toBe(commands);
  });
});
