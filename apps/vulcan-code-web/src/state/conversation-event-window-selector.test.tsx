// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  ConversationEventWindowExternalStoreProvider,
  conversationSelectionEqual,
  createConversationEventWindowExternalStore,
  useSelectedConversationEventWindowSelector,
  type ConversationEventWindowOwnedStore,
} from "./conversation-event-window-react";
import type { ConversationEventWindowControllerSnapshot } from "@/service/conversation-event-window-controller";

// snapshotWithRevision builds one owner-bound snapshot whose revision advances with every streaming delta.
// snapshotWithRevision 构建一份归属绑定快照，其修订号随每个流式增量推进。
function snapshotWithRevision(revision: number, sessionId = "session-selector"): ConversationEventWindowControllerSnapshot {
  return {
    owner: { session_id: sessionId, window_generation: 1 },
    service_epoch: "service-epoch-selector",
    memory_revision: revision,
    durable_revision: revision,
    events: [],
    older_before_seq: null,
    has_more: false,
    open_turn_id: null,
    open_turn_start_seq: null,
    session_config: null,
  };
}

// selectOwner derives only the owner id, which streaming deltas never change.
// selectOwner 仅派生归属 id，流式增量不会改变它。
function selectOwner(snapshot: ConversationEventWindowControllerSnapshot): string | null {
  return snapshot.owner?.session_id ?? null;
}

describe("useSelectedConversationEventWindowSelector", () => {
  let host: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
  });

  afterEach(async () => {
    await act(async () => root.unmount());
    host.remove();
    vi.unstubAllGlobals();
  });

  // A consumer of the owner id must not re-render for deltas that leave the owner unchanged.
  // 归属 id 的消费者不得因归属未变的增量而重新渲染。
  it("re-renders only when the selected value changes", async () => {
    let current = snapshotWithRevision(1);
    const listeners = new Set<() => void>();
    const owned: ConversationEventWindowOwnedStore = {
      snapshot: () => current,
      subscribe: (listener) => {
        listeners.add(listener);
        return () => listeners.delete(listener);
      },
      commands: () => ({ rehydrate: async () => true, prependOlder: async () => true }),
      status: () => ({ kind: "ready", session_id: current.owner?.session_id ?? "" }),
    };
    // flushes collects scheduled frames so the test decides when a frame elapses.
    // flushes 收集已调度的帧，由测试决定帧何时结束。
    const flushes: Array<() => void> = [];
    const store = createConversationEventWindowExternalStore(owned, (flush) => {
      flushes.push(flush);
      return () => undefined;
    });
    const renders = vi.fn();
    function OwnerProbe() {
      const owner = useSelectedConversationEventWindowSelector(selectOwner);
      renders(owner);
      return <span data-owner={owner ?? "none"} />;
    }
    // publish advances the controller snapshot and lets the scheduled frame elapse.
    // publish 推进控制器快照并让已调度的帧结束。
    const publish = async (next: ConversationEventWindowControllerSnapshot) => {
      current = next;
      for (const listener of Array.from(listeners)) listener();
      await act(async () => {
        for (const flush of flushes.splice(0)) flush();
      });
    };

    await act(async () => root.render(
      <ConversationEventWindowExternalStoreProvider store={store}>
        <OwnerProbe />
      </ConversationEventWindowExternalStoreProvider>,
    ));
    const initialRenders = renders.mock.calls.length;

    await publish(snapshotWithRevision(2));
    await publish(snapshotWithRevision(3));
    expect(renders).toHaveBeenCalledTimes(initialRenders);

    await publish(snapshotWithRevision(4, "session-other"));
    expect(renders).toHaveBeenCalledTimes(initialRenders + 1);
    expect(host.querySelector("[data-owner]")?.getAttribute("data-owner")).toBe("session-other");
  });
});

describe("conversationSelectionEqual", () => {
  it("compares plain data by structure", () => {
    const shared = { step: "a", status: "completed" };
    expect(conversationSelectionEqual({ items: [shared], total: 1 }, { items: [{ ...shared }], total: 1 })).toBe(true);
    expect(conversationSelectionEqual({ items: [shared], total: 1 }, { items: [shared], total: 2 })).toBe(false);
    expect(conversationSelectionEqual({ a: undefined }, { b: undefined })).toBe(false);
    expect(conversationSelectionEqual([1], { 0: 1 })).toBe(false);
    expect(conversationSelectionEqual(null, null)).toBe(true);
    expect(conversationSelectionEqual(null, {})).toBe(false);
  });
});
