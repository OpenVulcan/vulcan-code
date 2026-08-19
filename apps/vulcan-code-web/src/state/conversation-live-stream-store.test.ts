import { describe, expect, it, vi } from "vitest";
import {
  ConversationLiveStreamStore,
  conversationLiveStreamKey,
  toolRequestProgressKey,
  transientReasoningStatusKey,
} from "./conversation-live-stream-store";
import type {
  AppendMessageBodyDelta,
  ConversationMessage,
  WorkbenchDeltaEnvelope,
  WorkbenchSnapshot,
} from "@/types";

// TEST_CONVERSATION_ID is the stable conversation identity shared by Store tests.
// TEST_CONVERSATION_ID 是 Store 测试共享的稳定会话身份。
const TEST_CONVERSATION_ID = "conversation-a";

// TEST_MESSAGE_ID is the stable streamed message identity shared by Store tests.
// TEST_MESSAGE_ID 是 Store 测试共享的稳定流式消息身份。
const TEST_MESSAGE_ID = "message-a";

// liveMessage builds one authoritative streamed conversation message.
// liveMessage 构建一条权威流式会话消息。
function liveMessage(body: string, state: "active" | "sealed" = "active"): ConversationMessage {
  return {
    id: TEST_MESSAGE_ID,
    conversationId: TEST_CONVERSATION_ID,
    role: "assistant",
    recordKind: "thinking-output",
    body,
    metadata: { stream_state: state, collapsed_by_default: true },
  };
}

// appendEnvelope builds one ordered streamed-body workbench event.
// appendEnvelope 构建一条有序流式正文工作台事件。
function appendEnvelope(
  generation: number,
  payload: Partial<AppendMessageBodyDelta> & Pick<AppendMessageBodyDelta, "bodyDelta">,
): WorkbenchDeltaEnvelope {
  return {
    generation,
    delta: {
      kind: "appendMessageBody",
      payload: {
        conversationId: TEST_CONVERSATION_ID,
        messageId: TEST_MESSAGE_ID,
        ...payload,
      },
    },
  };
}

// workbenchSnapshot builds one selected-conversation snapshot at an exact revision.
// workbenchSnapshot 构建一份精确 revision 的选中会话快照。
function workbenchSnapshot(
  revision: number,
  message: ConversationMessage,
): Pick<WorkbenchSnapshot, "revision" | "messages"> {
  return {
    revision,
    messages: [message],
  };
}

describe("ConversationLiveStreamStore", () => {
  it("appends raw generations without rebuilding the historical body", () => {
    // store receives the exact uncoalesced event sequence.
    // store 接收精确且未合并的事件序列。
    const store = new ConversationLiveStreamStore();
    // key locates the single streamed thinking row.
    // key 定位唯一的流式思考行。
    const key = conversationLiveStreamKey(TEST_CONVERSATION_ID, TEST_MESSAGE_ID);

    expect(store.applyEnvelope(appendEnvelope(1, {
      bodyDelta: "first",
      initialMessage: liveMessage("first"),
      streamState: "active",
    }))).toBe(true);
    expect(store.applyEnvelope(appendEnvelope(2, {
      bodyDelta: " second",
      streamState: "active",
    }))).toBe(true);

    expect(store.readBody(key)).toMatchObject({
      body: "first second",
      baseGeneration: 1,
      lastGeneration: 2,
      lifecycle: "active",
    });
    expect(store.readDeltasAfter(key, 1)).toEqual([
      { generation: 2, text: " second" },
    ]);
  });

  it("rebases at a snapshot revision and retains only newer received fragments", () => {
    // store models a WebSocket event racing ahead of a selection RPC response.
    // store 模拟 WebSocket 事件领先会话选择 RPC 响应。
    const store = new ConversationLiveStreamStore();
    // key locates the stream after snapshot reconciliation.
    // key 定位快照对账后的流。
    const key = conversationLiveStreamKey(TEST_CONVERSATION_ID, TEST_MESSAGE_ID);
    store.applyEnvelope(appendEnvelope(10, {
      bodyDelta: "base",
      initialMessage: liveMessage("base"),
      streamState: "active",
    }));
    store.applyEnvelope(appendEnvelope(11, {
      bodyDelta: "+covered",
      streamState: "active",
    }));
    store.applyEnvelope(appendEnvelope(12, {
      bodyDelta: "+newer",
      streamState: "active",
    }));

    store.rebaseWorkbenchSnapshot(workbenchSnapshot(11, liveMessage("base+covered")));

    expect(store.readBody(key)).toMatchObject({
      body: "base+covered+newer",
      baseGeneration: 11,
      lastGeneration: 12,
    });
    expect(store.readDeltasAfter(key, 11)).toEqual([
      { generation: 12, text: "+newer" },
    ]);
  });

  it("does not let an older snapshot replace a newer full base", () => {
    // store owns a first event newer than the returned snapshot revision.
    // store 持有一条比返回快照版本更新的首事件。
    const store = new ConversationLiveStreamStore();
    // key locates the preserved newer base.
    // key 定位被保留的更新基线。
    const key = conversationLiveStreamKey(TEST_CONVERSATION_ID, TEST_MESSAGE_ID);
    store.applyEnvelope(appendEnvelope(20, {
      bodyDelta: "new body",
      initialMessage: liveMessage("new body"),
      streamState: "active",
    }));

    store.rebaseWorkbenchSnapshot(workbenchSnapshot(19, liveMessage("old body")));

    expect(store.readBody(key)?.body).toBe("new body");
    expect(store.readBody(key)?.baseGeneration).toBe(20);
  });

  it("does not let a newer structural snapshot regress an append-only active body", () => {
    const store = new ConversationLiveStreamStore();
    const key = conversationLiveStreamKey(TEST_CONVERSATION_ID, TEST_MESSAGE_ID);
    store.applyEnvelope(appendEnvelope(21, {
      bodyDelta: "base",
      initialMessage: liveMessage("base"),
      streamState: "active",
    }));
    store.applyEnvelope(appendEnvelope(22, {
      bodyDelta: "+hot",
      streamState: "active",
    }));

    store.rebaseWorkbenchSnapshot(workbenchSnapshot(23, liveMessage("base")));

    expect(store.readBody(key)).toMatchObject({
      body: "base+hot",
      lastGeneration: 22,
    });
  });

  it("seals with one authoritative final body and ignores stale continuations", () => {
    // store receives one active row before its authoritative sealed body.
    // store 在权威封口正文前接收一条活跃行。
    const store = new ConversationLiveStreamStore();
    // key locates the sealed record.
    // key 定位已封口记录。
    const key = conversationLiveStreamKey(TEST_CONVERSATION_ID, TEST_MESSAGE_ID);
    store.applyEnvelope(appendEnvelope(30, {
      bodyDelta: "partial",
      initialMessage: liveMessage("partial"),
      streamState: "active",
    }));
    store.applyEnvelope(appendEnvelope(31, {
      bodyDelta: "",
      currentBody: "authoritative final",
      streamState: "sealed",
    }));

    expect(store.applyEnvelope(appendEnvelope(30, {
      bodyDelta: " stale",
      streamState: "active",
    }))).toBe(false);
    expect(store.readBody(key)).toMatchObject({
      body: "authoritative final",
      lifecycle: "sealed",
      baseGeneration: 31,
      lastGeneration: 31,
    });
  });

  it("corrects a missing fragment from the sealed authoritative body", () => {
    const store = new ConversationLiveStreamStore();
    const key = conversationLiveStreamKey(TEST_CONVERSATION_ID, TEST_MESSAGE_ID);
    store.applyEnvelope(appendEnvelope(32, {
      bodyDelta: "first",
      initialMessage: liveMessage("first"),
      streamState: "active",
    }));
    store.applyEnvelope(appendEnvelope(34, {
      bodyDelta: "",
      currentBody: "first missing final",
      streamState: "sealed",
    }));

    expect(store.readBody(key)?.body).toBe("first missing final");
  });

  it("never reopens a sealed stream from a later active event or snapshot", () => {
    const store = new ConversationLiveStreamStore();
    const key = conversationLiveStreamKey(TEST_CONVERSATION_ID, TEST_MESSAGE_ID);
    store.applyEnvelope(appendEnvelope(35, {
      bodyDelta: "final",
      initialMessage: liveMessage("final"),
      streamState: "active",
    }));
    store.applyEnvelope(appendEnvelope(36, {
      bodyDelta: "",
      currentBody: "final",
      streamState: "sealed",
    }));

    expect(store.applyEnvelope(appendEnvelope(37, {
      bodyDelta: " invalid continuation",
      streamState: "active",
    }))).toBe(false);
    expect(store.applyEnvelope(appendEnvelope(38, {
      bodyDelta: " legacy continuation",
    }))).toBe(false);
    store.rebaseWorkbenchSnapshot(workbenchSnapshot(39, liveMessage("stale active")));

    expect(store.readBody(key)).toMatchObject({
      body: "final",
      lifecycle: "sealed",
      lastGeneration: 36,
    });
  });

  it("collects a sealed replica only after stable coverage and subscriber unmount", () => {
    const store = new ConversationLiveStreamStore();
    const key = conversationLiveStreamKey(TEST_CONVERSATION_ID, TEST_MESSAGE_ID);
    const unsubscribe = store.subscribe(key, vi.fn());
    store.applyEnvelope(appendEnvelope(50, {
      bodyDelta: "final",
      initialMessage: liveMessage("final"),
      streamState: "active",
    }));
    store.applyEnvelope(appendEnvelope(51, {
      bodyDelta: "",
      currentBody: "final",
      streamState: "sealed",
    }));

    expect(store.reconcileStableCoverage(workbenchSnapshot(50, liveMessage("final", "sealed")))).toBe(0);
    expect(store.readBody(key)).not.toBeNull();
    expect(store.reconcileStableCoverage(workbenchSnapshot(51, liveMessage("final", "sealed")))).toBe(0);
    expect(store.readBody(key)).not.toBeNull();

    unsubscribe();
    expect(store.readBody(key)).toBeNull();
  });

  it("retains a sealed replica when the subscriber leaves before stable React coverage", () => {
    const store = new ConversationLiveStreamStore();
    const key = conversationLiveStreamKey(TEST_CONVERSATION_ID, TEST_MESSAGE_ID);
    const unsubscribe = store.subscribe(key, vi.fn());
    store.applyEnvelope(appendEnvelope(55, {
      bodyDelta: "final",
      initialMessage: liveMessage("final"),
      streamState: "active",
    }));
    store.applyEnvelope(appendEnvelope(56, {
      bodyDelta: "",
      currentBody: "final",
      streamState: "sealed",
    }));

    unsubscribe();
    expect(store.readBody(key)).not.toBeNull();
    expect(store.reconcileStableCoverage(workbenchSnapshot(56, liveMessage("final", "sealed")))).toBe(1);
    expect(store.readBody(key)).toBeNull();
  });

  it("does not collect active streams or sealed bodies not covered by stable state", () => {
    const store = new ConversationLiveStreamStore();
    const key = conversationLiveStreamKey(TEST_CONVERSATION_ID, TEST_MESSAGE_ID);
    store.applyEnvelope(appendEnvelope(60, {
      bodyDelta: "active",
      initialMessage: liveMessage("active"),
      streamState: "active",
    }));
    expect(store.reconcileStableCoverage(workbenchSnapshot(60, liveMessage("active")))).toBe(0);
    expect(store.readBody(key)).not.toBeNull();

    store.applyEnvelope(appendEnvelope(61, {
      bodyDelta: "",
      currentBody: "sealed body",
      streamState: "sealed",
    }));
    expect(store.reconcileStableCoverage(workbenchSnapshot(61, liveMessage("different", "sealed")))).toBe(0);
    expect(store.readBody(key)?.body).toBe("sealed body");
  });

  it("batches subscriber notification while retaining every generation", () => {
    // store separates ingestion ordering from React notification cadence.
    // store 将写入顺序与 React 通知节奏分离。
    const store = new ConversationLiveStreamStore();
    // key locates the subscribed stream.
    // key 定位被订阅的流。
    const key = conversationLiveStreamKey(TEST_CONVERSATION_ID, TEST_MESSAGE_ID);
    // listener observes the single explicit flush.
    // listener 观察单次显式刷新。
    const listener = vi.fn();
    store.subscribe(key, listener);
    store.applyEnvelope(appendEnvelope(40, {
      bodyDelta: "a",
      initialMessage: liveMessage("a"),
      streamState: "active",
    }));
    store.applyEnvelope(appendEnvelope(41, {
      bodyDelta: "b",
      streamState: "active",
    }));

    expect(listener).not.toHaveBeenCalled();
    store.flushNotifications();
    expect(listener).toHaveBeenCalledTimes(1);
    expect(store.readDeltasAfter(key, 40)).toEqual([{ generation: 41, text: "b" }]);
  });

  it("replaces and clears transient reasoning status outside transcript streams", () => {
    // store owns one request-local status subscription without creating a transcript row.
    // store 持有一条请求局部状态订阅，且不会创建转录行。
    const store = new ConversationLiveStreamStore();
    // key isolates transient status notifications from persisted message streams.
    // key 将临时状态通知与持久化消息流隔离。
    const key = transientReasoningStatusKey(TEST_CONVERSATION_ID);
    // listener proves replace and clear events share the same subscription identity.
    // listener 证明替换与清除事件共享同一订阅身份。
    const listener = vi.fn();
    store.subscribe(key, listener);

    expect(store.applyEnvelope({
      generation: 70,
      delta: {
        kind: "transientReasoningStatusChanged",
        payload: {
          conversationId: TEST_CONVERSATION_ID,
          status: {
            kind: "summary",
            summaries: [{
              itemId: "summary-0",
              text: "正在检查协议",
            }],
            reasoningTokens: 321,
          },
        },
      },
    })).toBe(true);
    expect(store.getTransientReasoningStatus(TEST_CONVERSATION_ID)).toEqual({
      kind: "summary",
      summaries: [{
        itemId: "summary-0",
        text: "正在检查协议",
      }],
      reasoningTokens: 321,
    });
    expect(store.readBody(conversationLiveStreamKey(
      TEST_CONVERSATION_ID,
      TEST_MESSAGE_ID,
    ))).toBeNull();

    expect(store.applyEnvelope({
      generation: 71,
      delta: {
        kind: "transientReasoningStatusChanged",
        payload: {
          conversationId: TEST_CONVERSATION_ID,
          status: null,
        },
      },
    })).toBe(true);
    expect(store.getTransientReasoningStatus(TEST_CONVERSATION_ID)).toBeNull();
    store.flushNotifications();
    expect(listener).toHaveBeenCalledTimes(1);
  });

  it("replaces tool request progress by revision and clears it outside transcript streams", () => {
    // store owns one exact request identity without rebuilding any conversation message.
    // store 持有单个精确请求身份，且不会重建任何会话消息。
    const store = new ConversationLiveStreamStore();
    // requestStreamId is the closed correlation identity emitted by the backend coordinator.
    // requestStreamId 是后端协调器发出的封闭关联身份。
    const requestStreamId = "request-1";
    // key proves only the owning card receives the batched invalidation.
    // key 证明仅所属卡片接收批处理失效通知。
    const key = toolRequestProgressKey(TEST_CONVERSATION_ID, requestStreamId);
    const listener = vi.fn();
    store.subscribe(key, listener);

    expect(store.applyEnvelope({
      generation: 80,
      delta: {
        kind: "toolRequestProgressChanged",
        payload: {
          conversationId: TEST_CONVERSATION_ID,
          requestStreamId,
          progress: {
            requestStreamId,
            toolCallId: "call-1",
            toolName: "terminal_session",
            state: "receiving",
            receivedBytes: 512,
            receivedCharacters: 500,
            requestRevision: 3,
            preview: { kind: "command" },
          },
        },
      },
    })).toBe(true);
    expect(store.getToolRequestProgress(TEST_CONVERSATION_ID, requestStreamId)).toMatchObject({
      requestRevision: 3,
      receivedBytes: 512,
    });

    // staleEnvelope must not replace a newer request-local revision even with a later transport generation.
    // staleEnvelope 即使拥有更晚传输代际，也不得替换更新的请求局部修订。
    expect(store.applyEnvelope({
      generation: 81,
      delta: {
        kind: "toolRequestProgressChanged",
        payload: {
          conversationId: TEST_CONVERSATION_ID,
          requestStreamId,
          progress: {
            requestStreamId,
            toolCallId: "call-1",
            toolName: "terminal_session",
            state: "receiving",
            receivedBytes: 256,
            receivedCharacters: 250,
            requestRevision: 2,
          },
        },
      },
    })).toBe(false);
    expect(store.getToolRequestProgress(TEST_CONVERSATION_ID, requestStreamId)?.requestRevision).toBe(3);

    expect(store.applyEnvelope({
      generation: 82,
      delta: {
        kind: "toolRequestProgressChanged",
        payload: {
          conversationId: TEST_CONVERSATION_ID,
          requestStreamId,
          progress: null,
        },
      },
    })).toBe(true);
    expect(store.getToolRequestProgress(TEST_CONVERSATION_ID, requestStreamId)).toBeNull();
    store.flushNotifications();
    expect(listener).toHaveBeenCalledTimes(1);
  });

  it("clears transient reasoning and tool request progress on authoritative rebase", () => {
    // store starts with one transient reasoning value and one request-local progress snapshot.
    // store 初始持有一条瞬态思考值与一条请求局部进度快照。
    const store = new ConversationLiveStreamStore();
    // requestStreamId identifies the exact progress subscription invalidated by the snapshot boundary.
    // requestStreamId 标识由快照边界失效的精确进度订阅。
    const requestStreamId = "request-rebase";
    // reasoningListener and progressListener prove both leaf subscribers receive the batched clear.
    // reasoningListener 与 progressListener 证明两类叶子订阅者都会收到批量清除通知。
    const reasoningListener = vi.fn();
    const progressListener = vi.fn();
    store.subscribe(transientReasoningStatusKey(TEST_CONVERSATION_ID), reasoningListener);
    store.subscribe(
      toolRequestProgressKey(TEST_CONVERSATION_ID, requestStreamId),
      progressListener,
    );

    expect(store.applyEnvelope({
      generation: 90,
      delta: {
        kind: "transientReasoningStatusChanged",
        payload: {
          conversationId: TEST_CONVERSATION_ID,
          status: {
            kind: "encrypted",
            reasoningTokens: 128,
          },
        },
      },
    })).toBe(true);
    expect(store.applyEnvelope({
      generation: 91,
      delta: {
        kind: "toolRequestProgressChanged",
        payload: {
          conversationId: TEST_CONVERSATION_ID,
          requestStreamId,
          progress: {
            requestStreamId,
            toolCallId: "call-rebase",
            toolName: "terminal_session",
            state: "receiving",
            receivedBytes: 64,
            receivedCharacters: 64,
            requestRevision: 1,
          },
        },
      },
    })).toBe(true);

    store.rebaseWorkbenchSnapshot(workbenchSnapshot(92, liveMessage("authoritative")));
    expect(store.getTransientReasoningStatus(TEST_CONVERSATION_ID)).toBeNull();
    expect(store.getToolRequestProgress(TEST_CONVERSATION_ID, requestStreamId)).toBeNull();
    store.flushNotifications();
    expect(reasoningListener).toHaveBeenCalledTimes(1);
    expect(progressListener).toHaveBeenCalledTimes(1);
  });
});
