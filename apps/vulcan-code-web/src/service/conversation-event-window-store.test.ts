import { describe, expect, it, vi } from "vitest";

import {
  ConversationEventBody,
  ConversationEventLivePatch,
  ConversationEventReconnectPatch,
  ConversationEventSummary,
  ConversationEventWindowBaseline,
  ConversationEventWindowStore,
  clientActionId,
  formatWindowPatchError,
  type ClientActionId,
} from "./conversation-event-window-store";

/** Stable session id used by the primary event-window fixture.
 * 主要事件窗口夹具使用的稳定会话 ID。 */
const SESSION_ID = "session-primary";

/** Initial window generation used by the primary event-window fixture.
 * 主要事件窗口夹具使用的初始窗口代际。 */
const WINDOW_GENERATION = 1;

/** Stable service-process epoch used by the primary event-window fixture.
 * 主要事件窗口夹具使用的稳定服务进程代次。 */
const SERVICE_EPOCH = "service-epoch-primary";

/** Tool-call status accepted by the authoritative V1 event body.
 * 权威 V1 事件体允许的工具调用状态。 */
type ToolCallStatus = Extract<ConversationEventBody, { kind: "tool_call" }>["status"];

/** Exact mutable tool-call body used by lifecycle fixtures.
 * 生命周期夹具使用的精确可变工具调用事件体。 */
type ToolCallBody = Extract<ConversationEventBody, { kind: "tool_call" }>;

/** API-stat status accepted by the authoritative V1 event body.
 * 权威 V1 事件体允许的 API 统计状态。 */
type ApiStatStatus = Extract<ConversationEventBody, { kind: "api_stats" }>["status"];

/** Compile-time proof that user_input requires the shared branded ClientActionId.
 * user_input 必须使用共享强类型 ClientActionId 的编译期证明。 */
type UserInputHasRequiredStrongClientActionId = Extract<
  ConversationEventBody,
  { kind: "user_input" }
> extends { client_action_id: ClientActionId } ? true : false;

/** Runtime-visible witness that keeps the compile-time requirement inside the test suite.
 * 把编译期必填约束保留在测试套件中的运行时可见见证值。 */
const USER_INPUT_HAS_REQUIRED_STRONG_CLIENT_ACTION_ID: UserInputHasRequiredStrongClientActionId = true;

/** Builds one complete service event around an authoritative body.
 * 围绕权威事件体构建一条完整服务事件。 */
function summary(
  seq: number,
  event: ConversationEventBody,
  updatedAtMs = 1_700_000_000_000,
  eventRevision = 1,
): ConversationEventSummary {
  return {
    seq,
    turn_id: 1,
    created_at_ms: 1_700_000_000_000,
    updated_at_ms: updatedAtMs,
    event_revision: eventRevision,
    visibility: "provider",
    event,
  };
}

/** Builds one ownership-bound batch for an explicit window boundary.
 * 为显式窗口边界构建一批带归属事件。 */
function batch(
  events: readonly ConversationEventSummary[],
  sessionId = SESSION_ID,
  windowGeneration = WINDOW_GENERATION,
): ConversationEventWindowBaseline {
  const memoryRevision = events.reduce(
    (current, summaryValue) => Math.max(current, summaryValue.event_revision),
    0,
  );
  return {
    session_id: sessionId,
    window_generation: windowGeneration,
    service_epoch: SERVICE_EPOCH,
    memory_revision: memoryRevision,
    durable_revision: memoryRevision,
    events,
  };
}

/** Builds one revisioned sparse live patch for direct store tests.
 * 为直接 store 测试构建一份带修订的稀疏实时 patch。 */
function livePatch(
  events: readonly ConversationEventSummary[],
  options: Partial<Omit<ConversationEventLivePatch, "events">> = {},
): ConversationEventLivePatch {
  const eventRevision = events.reduce(
    (current, summaryValue) => Math.max(current, summaryValue.event_revision),
    0,
  );
  const revision = options.revision ?? eventRevision;
  const durability = options.durability ?? "volatile";
  return {
    session_id: SESSION_ID,
    window_generation: WINDOW_GENERATION,
    service_epoch: SERVICE_EPOCH,
    revision,
    durability,
    ...(durability === "volatile"
      ? { base_memory_revision: Math.max(0, revision - 1) }
      : { base_durable_revision: Math.max(0, revision - 1) }),
    events,
    ...options,
  };
}

/** Builds one user-input body with a stable local id.
 * 构建带稳定本地 ID 的用户输入事件体。 */
function userInput(inputId: number, text = `input-${inputId}`): ConversationEventBody {
  return {
    kind: "user_input",
    client_action_id: clientActionId(`store-input-${inputId}`),
    input_id: inputId,
    input_source: "root",
    text,
  };
}

/** Builds one assistant-text body for a physical API call.
 * 构建属于一次物理 API 调用的 assistant 正文事件体。 */
function assistantText(
  apiCallId: number,
  status: "streaming" | "completed" | "interrupted" = "completed",
  text = "answer",
): Extract<ConversationEventBody, { kind: "assistant_text" }> {
  return {
    kind: "assistant_text",
    api_call_id: apiCallId,
    text,
    status,
  };
}

/** Builds one mutable tool-call body.
 * 构建一条可原位演进的工具调用事件体。 */
function toolCall(
  toolCallId: number,
  status: ToolCallStatus = "running",
  inputPath = "D:/project/README.md",
): ToolCallBody {
  return {
    kind: "tool_call",
    api_call_id: 5,
    tool_call_id: toolCallId,
    tool_name: "file",
    input: { path: inputPath },
    request_stream_id: `store-tool-${toolCallId}`,
    status,
  };
}

/** Builds one mutable physical API-stat body.
 * 构建一条可原位演进的物理 API 统计事件体。 */
function apiStats(apiCallId: number, status: ApiStatStatus): ConversationEventBody {
  return {
    kind: "api_stats",
    api_call_id: apiCallId,
    purpose: "main_generation",
    provider: "test-provider",
    model: "test-model",
    usage: {},
    status,
    continuation_eligibility: "forbidden",
    timing: {},
  };
}

/** Builds one immutable final subagent-return-stat body.
 * 构建一条不可变的终态子代理回流统计事件体。 */
function subagentStats(apiCallCount = 1): ConversationEventBody {
  return {
    kind: "subagent_return_stats",
    child_session_id: "child-session",
    child_task_id: "child-task",
    child_turn_stats_seq: 7,
    status: "final",
    totals: {
      api_call_count: apiCallCount,
      unknown_usage_api_call_count: 0,
      tool_call_count: 2,
      subagent_return_count: 1,
      usage: {},
    },
  };
}

describe("conversation-event-window-store V1 ownership and boundaries", () => {
  it("requires one shared branded ClientActionId on every user_input body", () => {
    expect(USER_INPUT_HAS_REQUIRED_STRONG_CLIENT_ACTION_ID).toBe(true);
  });

  it("hydrates once, records ownership, and renders strictly by seq", () => {
    const store = new ConversationEventWindowStore();
    store.hydrate(batch([summary(4, userInput(1)), summary(5, assistantText(5))]));

    expect(store.getOwner()).toEqual({
      session_id: SESSION_ID,
      window_generation: WINDOW_GENERATION,
    });
    expect(store.getEarliestSeq()).toBe(4);
    expect(store.getLatestSeq()).toBe(5);
    expect(store.render().map((item) => item.seq)).toEqual([4, 5]);
  });

  it("rejects a hydration gap without publishing owner or partial events", () => {
    const store = new ConversationEventWindowStore();

    expect(() =>
      store.hydrate(batch([summary(1, userInput(1)), summary(3, assistantText(5))])),
    ).toThrow(/event window gap/);
    expect(store.getOwner()).toBeNull();
    expect(store.size()).toBe(0);
  });

  it("rejects a disordered hydration batch instead of repairing it during render", () => {
    const store = new ConversationEventWindowStore();

    expect(() =>
      store.hydrate(batch([summary(2, userInput(2)), summary(1, userInput(1))])),
    ).toThrow(/event window gap/);
    expect(store.getOwner()).toBeNull();
    expect(store.size()).toBe(0);
  });

  it("rejects repeated hydration and preserves the first complete window", () => {
    const store = new ConversationEventWindowStore();
    store.hydrate(batch([summary(1, userInput(1))]));

    expect(() => store.hydrate(batch([summary(1, userInput(2))]))).toThrow(
      /already initialized/,
    );
    expect(store.render()[0].event).toEqual(userInput(1));
  });

  it("prepends only an adjacent page owned by the current session generation", () => {
    const store = new ConversationEventWindowStore();
    store.hydrate(batch([summary(4, userInput(4)), summary(5, assistantText(5))]));
    store.prependPage(batch([summary(2, userInput(2)), summary(3, userInput(3))]));

    expect(store.render().map((item) => item.seq)).toEqual([2, 3, 4, 5]);
    expect(() => store.prependPage(batch([summary(1, userInput(1))], "other-session"))).toThrow(
      /session mismatch/,
    );
    expect(() => store.prependPage(batch([summary(1, userInput(1))], SESSION_ID, 2))).toThrow(
      /generation mismatch/,
    );
    expect(store.render().map((item) => item.seq)).toEqual([2, 3, 4, 5]);
  });

  it("keeps canonical Map iteration order across repeated prepend, evolution, and live append", () => {
    const store = new ConversationEventWindowStore();
    store.hydrate(batch([summary(5, toolCall(5, "running"), 10), summary(6, userInput(6))]));
    store.prependPage(batch([summary(3, userInput(3)), summary(4, userInput(4))]));
    store.prependPage(batch([summary(1, userInput(1)), summary(2, userInput(2))]));
    store.applyLive(livePatch([summary(5, toolCall(5, "completed"), 20, 2)]));
    store.applyLive(livePatch(
      [summary(7, userInput(7)), summary(8, userInput(8))],
      { revision: 3 },
    ));
    const sortSpy = vi.spyOn(Array.prototype, "sort");

    try {
      expect(store.toAscendingArray().map((item) => item.seq)).toEqual([1, 2, 3, 4, 5, 6, 7, 8]);
      expect(store.render().map((item) => item.seq)).toEqual([1, 2, 3, 4, 5, 6, 7, 8]);
      expect(sortSpy).not.toHaveBeenCalled();
    } finally {
      sortSpy.mockRestore();
    }
    expect(store.render()[4].event).toMatchObject({ status: "completed" });
  });

  it("rejects a non-adjacent page atomically", () => {
    const store = new ConversationEventWindowStore();
    store.hydrate(batch([summary(5, userInput(5))]));

    expect(() => store.prependPage(batch([summary(1, userInput(1))]))).toThrow(
      /event window gap/,
    );
    expect(store.render().map((item) => item.seq)).toEqual([5]);
  });

  it("allows live overlap updates before a consecutive append", () => {
    const store = new ConversationEventWindowStore();
    store.hydrate(
      batch([
        summary(1, toolCall(5), 10),
        summary(2, assistantText(5, "completed"), 10),
      ]),
    );
    store.applyLive(
      livePatch([
        summary(1, toolCall(5, "completed"), 20, 2),
        summary(3, userInput(3), 20),
      ]),
    );

    expect(store.render().map((item) => item.seq)).toEqual([1, 2, 3]);
    expect(store.render()[0].event).toMatchObject({ status: "completed" });
  });

  it("ignores sparse unknown old seq while updating loaded events and appending consecutively", () => {
    const store = new ConversationEventWindowStore();
    const hydrated = Array.from({ length: 30 }, (_, offset) => {
      const seq = 101 + offset;
      if (seq === 101 || seq === 110) {
        return summary(seq, toolCall(seq, "running"), 10, 1);
      }
      return summary(seq, userInput(seq), 10, 1);
    });
    store.hydrate(batch(hydrated));

    // sparsePatch proves unknown old seq do not make loaded revisions or a consecutive append fail.
    // sparsePatch 证明窗口外未知旧 seq 不会让已加载修订或连续追加失败。
    const sparsePatch = [
      summary(1, userInput(1), 20, 1),
      summary(50, userInput(50), 20, 1),
      summary(101, toolCall(101, "completed"), 20, 2),
      summary(110, toolCall(110, "failed"), 20, 2),
      summary(131, userInput(131), 20, 1),
    ];
    store.applyLive(livePatch(sparsePatch));

    expect(store.getEarliestSeq()).toBe(101);
    expect(store.getLatestSeq()).toBe(131);
    expect(store.render().map((item) => item.seq)).toEqual(
      Array.from({ length: 31 }, (_, offset) => 101 + offset),
    );
    expect(store.render()[0].event).toMatchObject({ status: "completed" });
    expect(store.render()[9].event).toMatchObject({ status: "failed" });
  });

  it("rejects an append gap after ignoring a sparse live prefix", () => {
    const store = new ConversationEventWindowStore();
    store.hydrate(batch([summary(101, userInput(101)), summary(102, userInput(102))]));

    expect(() =>
      store.applyLive(livePatch(
        [summary(1, userInput(1)), summary(104, userInput(104))],
        { revision: 2 },
      )),
    ).toThrow(/event window gap/);
    expect(store.render().map((item) => item.seq)).toEqual([101, 102]);
  });

  it("rejects a live gap without publishing earlier valid revisions from the batch", () => {
    const store = new ConversationEventWindowStore();
    store.hydrate(batch([summary(1, toolCall(5), 10)]));

    expect(() =>
      store.applyLive(
        livePatch([
          summary(1, toolCall(5, "completed"), 20, 2),
          summary(3, userInput(3), 20),
        ]),
      ),
    ).toThrow(/event window gap/);
    expect(store.getLatestSeq()).toBe(1);
    expect(store.render()[0].event).toMatchObject({ status: "running" });
  });

  it("rejects out-of-order live revisions without partially publishing them", () => {
    const store = new ConversationEventWindowStore();
    store.hydrate(
      batch([
        summary(1, toolCall(5, "running"), 10),
        summary(2, assistantText(5, "streaming", "partial"), 10),
      ]),
    );

    expect(() =>
      store.applyLive(
        livePatch([
          summary(2, assistantText(5, "completed", "answer"), 20, 2),
          summary(1, toolCall(5, "completed"), 20, 2),
        ]),
      ),
    ).toThrow(/event window gap/);
    expect(store.render()[0].event).toMatchObject({ status: "running" });
    expect(store.render()[1].event).toMatchObject({ status: "streaming", text: "partial" });
  });

  it("rejects an identity conflict without publishing earlier valid revisions", () => {
    const store = new ConversationEventWindowStore();
    store.hydrate(batch([summary(1, toolCall(5), 10), summary(2, userInput(1), 10)]));

    expect(() =>
      store.applyLive(
        livePatch([
          summary(1, toolCall(5, "completed"), 20, 2),
          summary(2, userInput(2), 20, 2),
        ]),
      ),
    ).toThrow(/identity conflict/);
    expect(store.render()[0].event).toMatchObject({ status: "running" });
    expect(store.render()[1].event).toEqual(userInput(1));
  });

  it("rejects live batches owned by another session or generation", () => {
    const store = new ConversationEventWindowStore();
    store.hydrate(batch([summary(1, userInput(1))]));

    expect(() => store.applyLive(livePatch(
      [summary(2, userInput(2))],
      { session_id: "other-session", revision: 2 },
    ))).toThrow(
      /session mismatch/,
    );
    expect(() => store.applyLive(livePatch(
      [summary(2, userInput(2))],
      { window_generation: 2, revision: 2 },
    ))).toThrow(
      /generation mismatch/,
    );
    expect(store.render().map((item) => item.seq)).toEqual([1]);
  });

  it("starts an empty hydrated session at seq one", () => {
    const store = new ConversationEventWindowStore();
    store.hydrate(batch([]));

    expect(() => store.applyLive(livePatch([summary(2, userInput(2))], { revision: 1 }))).toThrow(/expected seq 1/);
    store.applyLive(livePatch([summary(1, userInput(1))], { revision: 1 }));
    expect(store.render().map((item) => item.seq)).toEqual([1]);
  });

  it("rebases by swapping one complete validated map without calling clear", () => {
    const store = new ConversationEventWindowStore();
    store.hydrate(batch([summary(1, userInput(1)), summary(2, assistantText(5))]));
    const clearSpy = vi.spyOn(Map.prototype, "clear");

    try {
      store.rebase(batch([summary(8, userInput(8)), summary(9, assistantText(9))], SESSION_ID, 2));
      expect(clearSpy).not.toHaveBeenCalled();
    } finally {
      clearSpy.mockRestore();
    }
    expect(store.getOwner()).toEqual({ session_id: SESSION_ID, window_generation: 2 });
    expect(store.render().map((item) => item.seq)).toEqual([8, 9]);
  });

  it("keeps the old complete window when rebase validation fails", () => {
    const store = new ConversationEventWindowStore();
    store.hydrate(batch([summary(1, userInput(1)), summary(2, assistantText(5))]));

    expect(() =>
      store.rebase(batch([summary(8, userInput(8)), summary(10, assistantText(9))], SESSION_ID, 2)),
    ).toThrow(/event window gap/);
    expect(store.getOwner()).toEqual({
      session_id: SESSION_ID,
      window_generation: WINDOW_GENERATION,
    });
    expect(store.render().map((item) => item.seq)).toEqual([1, 2]);
  });

  it("requires same-session rebase to advance generation", () => {
    const store = new ConversationEventWindowStore();
    store.hydrate(batch([summary(1, userInput(1))]));

    expect(() => store.rebase(batch([summary(1, userInput(1))]))).toThrow(
      /generation must advance/,
    );
    expect(store.render().map((item) => item.seq)).toEqual([1]);
  });

  it("permits an explicit atomic rebase to a different session owner", () => {
    const store = new ConversationEventWindowStore();
    store.hydrate(batch([summary(1, userInput(1))]));
    store.rebase(batch([summary(1, userInput(9))], "session-secondary", 1));

    expect(store.getOwner()).toEqual({
      session_id: "session-secondary",
      window_generation: 1,
    });
    expect(store.render()[0].event).toEqual(userInput(9));
  });
});

describe("conversation-event-window-store V1 same-seq evolution", () => {
  it("allows assistant streaming to complete", () => {
    const store = new ConversationEventWindowStore();
    store.hydrate(batch([summary(1, assistantText(5, "streaming", "par"), 10)]));
    store.applyLive(livePatch([summary(1, assistantText(5, "completed", "partial"), 20, 2)]));

    expect(store.render()[0].event).toMatchObject({ status: "completed", text: "partial" });
  });

  it("allows tool running through waiting-user to completed", () => {
    const store = new ConversationEventWindowStore();
    store.hydrate(batch([summary(1, toolCall(5, "running"), 10)]));
    store.applyLive(livePatch([summary(1, toolCall(5, "waiting_user"), 20, 2)]));
    store.applyLive(livePatch([summary(1, toolCall(5, "completed"), 30, 3)]));

    expect(store.render()[0].event).toMatchObject({ status: "completed" });
  });

  it("treats the unique terminal API stats event as immutable", () => {
    const store = new ConversationEventWindowStore();
    store.hydrate(batch([summary(1, apiStats(5, "completed"), 30)]));

    expect(() => store.applyLive(livePatch([summary(1, apiStats(5, "failed"), 40, 2)]))).toThrow(
      /illegal api_stats evolution/,
    );
    expect(store.render()[0].event).toMatchObject({ status: "completed" });
  });

  it("rejects any rewrite of immutable final subagent totals", () => {
    const store = new ConversationEventWindowStore();
    store.hydrate(batch([summary(1, subagentStats(), 10)]));

    expect(() =>
      store.applyLive(livePatch([summary(1, subagentStats(2), 20, 2)])),
    ).toThrow(/immutable.*changed/);
    expect(store.render()[0].event).toEqual(subagentStats());
  });

  it("rejects child result generation and execution changes without delimiter collisions", () => {
    // Source strings deliberately contain separators that would collide in a joined identity.
    // 来源字符串故意包含会在拼接身份中冲突的分隔符。
    const original: Extract<ConversationEventBody, { kind: "subagent_return" }> = {
      kind: "subagent_return", child_session_id: "child", child_segment_generation: 1,
      child_root_action_id: "run:part", child_task_id: "task", content: "result",
      source_assistant_text_seq: 3, source_from_seq: 1, source_to_seq: 4, child_turn_stats_seq: 4,
    };
    // Each mutation keeps parent event seq and payload text identical while changing source ownership.
    // 每个修改都保留父事件序号及正文相同，同时改变来源归属。
    const replacements = [
      { ...original, child_segment_generation: 2 },
      { ...original, child_root_action_id: "new-run" },
      { ...original, child_root_action_id: "run", child_task_id: "part:task" },
    ];
    for (const replacement of replacements) {
      // Rejection must leave the original event window untouched.
      // 拒绝必须保持原事件窗口不变。
      const store = new ConversationEventWindowStore();
      store.hydrate(batch([summary(1, original, 10)]));
      expect(() => store.applyLive(livePatch([summary(1, replacement, 20, 2)]))).toThrow(/identity conflict/);
      expect(store.render()[0].event).toEqual(original);
    }
  });

  it("ignores stale revisions before they can overwrite a newer terminal state", () => {
    const store = new ConversationEventWindowStore();
    store.hydrate(batch([summary(1, toolCall(5, "completed"), 30, 3)]));

    store.applyLive(livePatch([summary(1, toolCall(5, "running"), 20, 2)]));
    expect(store.render()[0].event).toMatchObject({ status: "completed" });
    expect(store.render()[0]).toMatchObject({ event_revision: 3, updated_at_ms: 30 });
  });

  it("rejects different bodies carrying the same event revision", () => {
    const store = new ConversationEventWindowStore();
    store.hydrate(batch([summary(1, toolCall(5, "running"), 10)]));

    expect(() => store.applyLive(livePatch([summary(1, toolCall(5, "completed"), 10)]))).toThrow(
      /illegal tool_call evolution/,
    );
    expect(store.render()[0].event).toMatchObject({ status: "running" });
  });

  it("accepts an identical revision when only the display timestamp differs", () => {
    const store = new ConversationEventWindowStore();
    store.hydrate(batch([summary(1, toolCall(5, "running"), 10, 1)]));
    store.applyLive(livePatch([summary(1, toolCall(5, "running"), 20, 1)]));

    expect(store.render()[0]).toMatchObject({ event_revision: 1, updated_at_ms: 10 });
    expect(store.render()[0].event).toMatchObject({ status: "running" });
  });

  it("rejects mutation of an immutable event body", () => {
    const store = new ConversationEventWindowStore();
    store.hydrate(batch([summary(1, userInput(1, "before"), 10)]));

    expect(() => store.applyLive(livePatch([summary(1, userInput(1, "after"), 20, 2)]))).toThrow(
      /illegal user_input evolution/,
    );
    expect(store.render()[0].event).toEqual(userInput(1, "before"));
  });

  it("rejects tool input mutation even when status evolution is valid", () => {
    const store = new ConversationEventWindowStore();
    store.hydrate(batch([summary(1, toolCall(5, "running", "D:/before"), 10)]));

    expect(() =>
      store.applyLive(livePatch([summary(1, toolCall(5, "completed", "D:/after"), 20, 2)])),
    ).toThrow(/illegal tool_call evolution/);
    expect(store.render()[0].event).toMatchObject({ input: { path: "D:/before" } });
  });

  it("accepts an exact duplicate revision idempotently", () => {
    const store = new ConversationEventWindowStore();
    const event = summary(1, userInput(1), 10);
    store.hydrate(batch([event]));
    store.applyLive(livePatch([{ ...event }]));

    expect(store.size()).toBe(1);
    expect(store.render()[0]).toEqual(event);
  });

  it("uses event revision when same-millisecond tool states advance", () => {
    const store = new ConversationEventWindowStore();
    store.hydrate(batch([summary(1, toolCall(5, "running"), 50, 1)]));
    store.applyLive(livePatch([summary(1, toolCall(5, "waiting_user"), 50, 2)]));
    store.applyLive(livePatch([summary(1, toolCall(5, "completed"), 50, 3)]));

    expect(store.render()[0]).toMatchObject({ event_revision: 3, updated_at_ms: 50 });
    expect(store.render()[0].event).toMatchObject({ status: "completed" });
  });

  it("accepts prepared, running, terminal, and durable acknowledgement without losing the memory frontier", () => {
    const store = new ConversationEventWindowStore();
    const prepared = {
      ...toolCall(5, "prepared"),
      request_stream_id: "request-stream-5",
    } satisfies ConversationEventBody;
    const running = {
      ...prepared,
      status: "running" as const,
      started_at_ms: 20,
    } satisfies ConversationEventBody;
    const completed = {
      ...running,
      status: "completed" as const,
      output: { result: "done" },
      finished_at_ms: 30,
    } satisfies ConversationEventBody;
    store.hydrate(batch([summary(1, prepared, 10, 1)]));

    store.applyLive(livePatch([summary(1, running, 20, 2)]));
    store.applyLive(livePatch([summary(1, completed, 30, 3)]));
    store.applyLive(livePatch([], {
      durability: "durable",
      base_durable_revision: 1,
      revision: 3,
    }));

    expect(store.getFrontier()).toEqual({
      service_epoch: SERVICE_EPOCH,
      memory_revision: 3,
      durable_revision: 3,
    });
    expect(store.render()[0].event).toMatchObject({
      status: "completed",
      started_at_ms: 20,
      finished_at_ms: 30,
    });
  });

  it("accepts a reconnect that coalesces prepared directly into a terminal tool snapshot", () => {
    const store = new ConversationEventWindowStore();
    const prepared = {
      ...toolCall(5, "prepared"),
      request_stream_id: "request-stream-5",
    } satisfies ConversationEventBody;
    const completed = {
      ...prepared,
      status: "completed" as const,
      started_at_ms: 20,
      finished_at_ms: 30,
      output: { result: "done" },
    } satisfies ConversationEventBody;
    store.hydrate(batch([summary(1, prepared, 10, 1)]));

    store.applyReconnect({
      session_id: SESSION_ID,
      window_generation: WINDOW_GENERATION,
      service_epoch: SERVICE_EPOCH,
      base_memory_revision: 1,
      memory_revision: 3,
      durable_revision: 3,
      events: [summary(1, completed, 30, 3)],
    });

    expect(store.getFrontier()).toEqual({
      service_epoch: SERVICE_EPOCH,
      memory_revision: 3,
      durable_revision: 3,
    });
    expect(store.render()[0].event).toMatchObject({ status: "completed" });
  });

  it("allows a terminal Provider-source attachment but rejects terminal output drift", () => {
    const store = new ConversationEventWindowStore();
    const completed = {
      ...toolCall(5, "completed"),
      started_at_ms: 20,
      finished_at_ms: 30,
      output: { result: "done" },
      request_stream_id: "request-stream-5",
    } satisfies ConversationEventBody;
    const withProviderSource = {
      ...completed,
      provider_source: {
        provider: "test-provider",
        model: "test-model",
        covered_until_seq: 1,
        fingerprint_sha256: "a".repeat(64),
      },
    } satisfies ConversationEventBody;
    store.hydrate(batch([summary(1, completed, 30, 1)]));
    store.applyLive(livePatch([summary(1, withProviderSource, 40, 2)]));

    expect(() => store.applyLive(livePatch([summary(1, {
      ...withProviderSource,
      output: { result: "drifted" },
    }, 50, 3)]))).toThrow(/illegal tool_call evolution/);
    expect(store.render()[0].event).toEqual(withProviderSource);
  });

  it("allows terminal assistant Provider-source attachment but rejects completed text drift", () => {
    const store = new ConversationEventWindowStore();
    const completed = assistantText(5, "completed", "done");
    const withProviderSource = {
      ...completed,
      provider_source: {
        provider: "test-provider",
        model: "test-model",
        covered_until_seq: 1,
        fingerprint_sha256: "b".repeat(64),
      },
    } satisfies ConversationEventBody;
    store.hydrate(batch([summary(1, completed, 30, 1)]));
    store.applyLive(livePatch([summary(1, withProviderSource, 40, 2)]));

    expect(() => store.applyLive(livePatch([summary(1, {
      ...withProviderSource,
      text: "drifted",
    }, 50, 3)]))).toThrow(/illegal assistant_text evolution/);
    expect(store.render()[0].event).toEqual(withProviderSource);
  });

  it("accepts a forward event revision when the display timestamp moves backward", () => {
    const store = new ConversationEventWindowStore();
    store.hydrate(batch([summary(1, assistantText(5, "streaming", "part"), 100, 1)]));
    store.applyLive(livePatch([summary(1, assistantText(5, "completed", "done"), 90, 2)]));

    expect(store.render()[0]).toMatchObject({ event_revision: 2, updated_at_ms: 90 });
    expect(store.render()[0].event).toMatchObject({ status: "completed", text: "done" });
  });

  it("accepts a reconnect revision jump directly to a valid terminal state", () => {
    const store = new ConversationEventWindowStore();
    store.hydrate(batch([summary(1, toolCall(5, "running"), 10, 1)]));
    const reconnect: ConversationEventReconnectPatch = {
      session_id: SESSION_ID,
      window_generation: WINDOW_GENERATION,
      service_epoch: SERVICE_EPOCH,
      base_memory_revision: 1,
      memory_revision: 4,
      durable_revision: 1,
      events: [summary(1, toolCall(5, "completed"), 40, 4)],
    };
    store.applyReconnect(reconnect);

    expect(store.render()[0]).toMatchObject({ event_revision: 4 });
    expect(store.render()[0].event).toMatchObject({ status: "completed" });
  });
});

describe("conversation-event-window-store V1 owner-generation frontiers", () => {
  it("rejects a skipped sparse revision until reconnect restores its missing append", () => {
    const store = new ConversationEventWindowStore();
    const initial = Array.from({ length: 11 }, (_, offset) => {
      const seq = 90 + offset;
      return seq === 90
        ? summary(seq, toolCall(90, "running"), 10, 1)
        : summary(seq, userInput(seq), 10, 1);
    });
    store.hydrate({
      ...batch(initial),
      memory_revision: 3,
      durable_revision: 3,
    });

    expect(() => store.applyLive(livePatch(
      [summary(90, toolCall(90, "completed"), 50, 2)],
      { base_memory_revision: 4, revision: 5 },
    ))).toThrow(/memory revision gap/);
    expect(store.getFrontier()).toEqual({ service_epoch: SERVICE_EPOCH, memory_revision: 3, durable_revision: 3 });
    expect(store.getLatestSeq()).toBe(100);

    expect(() => store.applyLive(livePatch([], {
      durability: "durable",
      base_durable_revision: 3,
      revision: 4,
    }))).toThrow(/durable revision 4 exceeds memory revision 3/);
    expect(store.getLatestSeq()).toBe(100);

    store.applyReconnect({
      session_id: SESSION_ID,
      window_generation: WINDOW_GENERATION,
      service_epoch: SERVICE_EPOCH,
      base_memory_revision: 3,
      memory_revision: 5,
      durable_revision: 4,
      events: [
        summary(90, toolCall(90, "completed"), 50, 2),
        summary(101, userInput(101), 40, 1),
      ],
    });
    expect(store.getFrontier()).toEqual({ service_epoch: SERVICE_EPOCH, memory_revision: 5, durable_revision: 4 });
    expect(store.getLatestSeq()).toBe(101);
    expect(store.render()[0].event).toMatchObject({ status: "completed" });
  });

  it("advances an empty metadata revision between two event mutations", () => {
    const store = new ConversationEventWindowStore();
    store.hydrate({
      ...batch([summary(1, toolCall(5, "running"), 10, 1)]),
      memory_revision: 1,
      durable_revision: 1,
    });

    store.applyLive(livePatch(
      [summary(1, toolCall(5, "waiting_user"), 20, 2)],
      { base_memory_revision: 1, revision: 2 },
    ));
    store.applyLive(livePatch([], {
      base_memory_revision: 2,
      revision: 3,
      durability: "volatile",
    }));
    store.applyLive(livePatch([], {
      base_durable_revision: 1,
      revision: 3,
      durability: "durable",
    }));
    store.applyLive(livePatch(
      [summary(1, toolCall(5, "completed"), 30, 3), summary(2, userInput(2), 30, 1)],
      { base_memory_revision: 3, revision: 4 },
    ));

    expect(store.getFrontier()).toEqual({ service_epoch: SERVICE_EPOCH, memory_revision: 4, durable_revision: 3 });
    expect(store.render().map((event) => event.seq)).toEqual([1, 2]);
    expect(store.render()[0].event).toMatchObject({ status: "completed" });
  });

  it("rejects an invalid hydration frontier before publishing owner or events", () => {
    const store = new ConversationEventWindowStore();

    expect(() => store.hydrate({
      ...batch([summary(1, userInput(1))]),
      memory_revision: 1,
      durable_revision: 2,
    })).toThrow(/durable revision 2 exceeds memory revision 1/);
    expect(store.getOwner()).toBeNull();
    expect(store.getFrontier()).toBeNull();
    expect(store.render()).toEqual([]);
  });

  it("advances memory on volatile mutation and durable only on the later empty acknowledgement", () => {
    const store = new ConversationEventWindowStore();
    store.hydrate({
      ...batch([summary(1, toolCall(5, "running"), 10, 1)]),
      memory_revision: 1,
      durable_revision: 0,
    });

    store.applyLive(livePatch(
      [summary(1, toolCall(5, "completed"), 20, 2)],
      { revision: 2, durability: "volatile" },
    ));
    expect(store.getFrontier()).toEqual({ service_epoch: SERVICE_EPOCH, memory_revision: 2, durable_revision: 0 });

    store.applyLive(livePatch([], {
      revision: 2,
      durability: "durable",
      base_durable_revision: 0,
    }));
    store.applyLive(livePatch([], {
      revision: 2,
      durability: "durable",
      base_durable_revision: 0,
    }));
    expect(store.getFrontier()).toEqual({ service_epoch: SERVICE_EPOCH, memory_revision: 2, durable_revision: 2 });
  });

  it("keeps stale patches harmless and rejects an empty durable acknowledgement beyond memory", () => {
    const store = new ConversationEventWindowStore();
    store.hydrate({
      ...batch([summary(1, toolCall(5, "completed"), 30, 3)]),
      memory_revision: 3,
      durable_revision: 1,
    });

    store.applyLive(livePatch(
      [summary(1, toolCall(5, "running"), 20, 2)],
      { revision: 2, durability: "volatile" },
    ));
    store.applyLive(livePatch([], { revision: 1, durability: "durable" }));
    expect(store.getFrontier()).toEqual({ service_epoch: SERVICE_EPOCH, memory_revision: 3, durable_revision: 1 });
    expect(store.render()[0].event).toMatchObject({ status: "completed" });

    expect(() => store.applyLive(
      livePatch([], { revision: 4, durability: "durable", base_durable_revision: 1 }),
    )).toThrow(/durable revision 4 exceeds memory revision 3/);
    expect(store.getFrontier()).toEqual({ service_epoch: SERVICE_EPOCH, memory_revision: 3, durable_revision: 1 });
  });

  it("rejects a different event snapshot at the same document revision", () => {
    const store = new ConversationEventWindowStore();
    store.hydrate({
      ...batch([summary(1, toolCall(5, "running"), 10, 1)]),
      memory_revision: 4,
      durable_revision: 3,
    });

    expect(() => store.applyLive(livePatch(
      [summary(1, toolCall(5, "completed"), 20, 2)],
      { revision: 4, durability: "volatile" },
    ))).toThrow(/document revision 4 conflicts/);
    expect(store.getFrontier()).toEqual({ service_epoch: SERVICE_EPOCH, memory_revision: 4, durable_revision: 3 });
    expect(store.render()[0].event).toMatchObject({ status: "running" });
  });

  it("resets both frontiers on generation rebase and leaves them unchanged on prepend", () => {
    const store = new ConversationEventWindowStore();
    store.hydrate({
      ...batch([summary(3, userInput(3)), summary(4, userInput(4))]),
      memory_revision: 8,
      durable_revision: 7,
    });
    store.prependPage(batch([summary(1, userInput(1)), summary(2, userInput(2))]));
    expect(store.getFrontier()).toEqual({ service_epoch: SERVICE_EPOCH, memory_revision: 8, durable_revision: 7 });

    store.rebase({
      ...batch([summary(1, userInput(1))], SESSION_ID, 2),
      memory_revision: 2,
      durable_revision: 1,
    });
    expect(store.getFrontier()).toEqual({ service_epoch: SERVICE_EPOCH, memory_revision: 2, durable_revision: 1 });
  });
});

describe("conversation-event-window-store V1 diagnostics", () => {
  it("rejects unsafe seq and generation values", () => {
    const store = new ConversationEventWindowStore();

    expect(() => store.hydrate(batch([summary(0, userInput(1))]))).toThrow(/unsafe event seq/);
    expect(() => store.hydrate(batch([], SESSION_ID, Number.MAX_SAFE_INTEGER + 1))).toThrow(
      /unsafe window generation/,
    );
    expect(() =>
      store.hydrate(
        batch([{ ...summary(1, userInput(1)), event_revision: Number.MAX_SAFE_INTEGER + 1 }]),
      ),
    ).toThrow(/unsafe event revision/);
  });

  it("formats gap, ownership, and evolution diagnostics", () => {
    expect(formatWindowPatchError({ code: "gap", expected: 2, actual: 4 })).toContain("gap");
    expect(
      formatWindowPatchError({ code: "session_mismatch", expected: "a", actual: "b" }),
    ).toContain("session mismatch");
    expect(
      formatWindowPatchError({
        code: "illegal_evolution",
        seq: 1,
        kind: "tool_call",
        from: "completed",
        to: "running",
      }),
    ).toContain("illegal tool_call evolution");
  });
});
