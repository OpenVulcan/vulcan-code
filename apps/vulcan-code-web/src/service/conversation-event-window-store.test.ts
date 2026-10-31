import { describe, expect, it, vi } from "vitest";

import {
  ConversationEventBody,
  ConversationEventLivePatch,
  ConversationEventReconnectPatch,
  ConversationEventRevisionGapError,
  ConversationEventSummary,
  ConversationEventTextAppend,
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

describe("conversation-event-window-store V1 ownership and boundaries", () => {
  /** Maintenance records remain immutable host facts across live append, pagination and hydration.
   * 维护记录在实时追加、分页和首次加载期间保持为不可变宿主事实。
   * Parameters: root or child business identity. Returns: none; verifies preservation and ownership rejection.
   * 参数：主会话或子会话业务身份。返回值：无；验证记录保留及归属拒绝。 */
  it.each([null, "child-business"])("preserves context maintenance ownership for %s", (businessExecutionId) => {
    // The original start anchor uses exactly the Core wire fields and nullability.
    // 原起始锚点使用精确 Core 线协议字段及可空性。
    const body: Extract<ConversationEventBody, { kind: "context_maintenance" }> = {
      kind: "context_maintenance", execution_id: "maintenance-a", business_execution_id: businessExecutionId,
      business_turn_id: 1, turn_id: 2, segment_generation: WINDOW_GENERATION, source_revision: 1,
      source_compaction_id: null, started_seq: 1, started_at_ms: 10, terminal: null,
    };
    // Start and closure are separate immutable records even when they share execution identity.
    // 即使共享执行身份，起始与闭合仍是两条独立不可变记录。
    const started: ConversationEventSummary = { ...summary(body.started_seq, body, 10), turn_id: body.turn_id, visibility: "host_only" };
    // A later page can contain closure without the start, so pagination must preserve both records.
    // 后续页面可以仅包含闭合而不包含起始，因此分页必须保留两条记录。
    const terminal: ConversationEventSummary = {
      ...started, seq: started.seq + 1, event_revision: 2, updated_at_ms: 20,
      event: { ...body, terminal: { reason: "failed", compaction_id: null, end_seq: started.seq + 3, completed_at_ms: 20 } },
    };
    // The store accepts canonical live facts without replacing earlier history at the same execution ID.
    // 存储接纳规范实时事实，不用同执行 ID 替换较早历史。
    const live = new ConversationEventWindowStore();
    live.hydrate(batch([]));
    live.applyLive(livePatch([started]));
    live.applyLive(livePatch([terminal]));
    expect(live.render()).toEqual([started, terminal]);
    // Cold loading and pagination must retain the same typed window as live updates.
    // 冷加载与分页必须保留与实时更新相同的强类型窗口。
    const restored = new ConversationEventWindowStore();
    restored.hydrate(batch([terminal]));
    restored.prependPage(batch([started]));
    expect(restored.render()).toEqual(live.render());
    // A same-sequence replacement may never switch to another maintenance execution.
    // 同序号替换绝不能切换到另一个维护执行。
    const replaced = { ...started, event_revision: 3, updated_at_ms: 30, event: { ...body, execution_id: "maintenance-b" } };
    expect(() => restored.applyLive(livePatch([replaced]))).toThrow(/identity conflict/);
    expect(restored.render()).toEqual([started, terminal]);
    // A higher revision cannot turn the immutable start fact into its terminal record either.
    // 较高修订也不能将不可变起始事实改成其终态记录。
    const rewritten = { ...terminal, seq: started.seq, event_revision: 3 };
    expect(() => restored.applyLive(livePatch([rewritten]))).toThrow(/illegal context_maintenance evolution/);
    expect(restored.render()).toEqual([started, terminal]);
  });

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

  /** Distinct collaboration kinds must never be read as revisions of one another.
   * 不同的协作类型绝不能被读作彼此的一次修订。 */
  it("refuses to read a different collaboration kind as a revision of another", () => {
    const collaborationish = (
      eventKind: "question_created" | "delivery_settled",
      operationId: string,
      seq: number,
    ): ConversationEventSummary =>
      summary(seq, {
        kind: "collaboration",
        version: 1,
        operation_id: operationId,
        event_kind: eventKind,
        after_state: { kind: "generation_ownership_cleared" },
      } as ConversationEventBody);

    // Two operation identities never collide: each keeps its own kind at the same seq.
    // 两个不同的操作标识绝不冲突：各自在同一 seq 上保持自己的类型。
    const distinct = new ConversationEventWindowStore();
    distinct.hydrate(batch([collaborationish("question_created", "operation-a", 1)]));
    distinct.applyLive(
      livePatch([collaborationish("delivery_settled", "operation-b", 2)], { revision: 2 }),
    );
    expect(
      distinct.render().map((event) => (event.event as { event_kind?: string }).event_kind),
    ).toEqual(["question_created", "delivery_settled"]);

    // The identity key must include the operation id even when the kind is unchanged: two different
    // operations of the same kind at one seq are two entities, never one entity's revision.
    // 即使类型相同，身份键也必须包含操作标识：同一 seq 上同类型的两个不同操作是两个实体，
    // 绝不是同一实体的两次修订。
    const sameKind = new ConversationEventWindowStore();
    sameKind.hydrate(batch([collaborationish("question_created", "operation-a", 1)]));
    expect(() =>
      sameKind.applyLive(
        livePatch(
          [
            {
              ...collaborationish("question_created", "operation-b", 1),
              event_revision: 2,
            },
          ],
          { revision: 2 },
        ),
      ),
    ).toThrow(/identity conflict/);
    expect(
      sameKind.render().map((event) => (event.event as { operation_id?: string }).operation_id),
    ).toEqual(["operation-a"]);

    // The same operation identity may not silently change kind or after-state: an answer must not be
    // accepted as a receipt of itself, and the store must refuse rather than merge the two readings.
    // 同一操作标识不得静默改变类型或后状态：答案不得被当作自身的回执接纳，存储必须拒绝而不是把两种
    // 读法合并。
    const sameIdentity = new ConversationEventWindowStore();
    sameIdentity.hydrate(batch([collaborationish("question_created", "operation-shared", 1)]));
    expect(() =>
      sameIdentity.applyLive(
        livePatch(
          [
            {
              ...collaborationish("delivery_settled", "operation-shared", 1),
              event_revision: 2,
            },
          ],
          { revision: 2 },
        ),
      ),
    ).toThrow(/illegal collaboration evolution/);
    // Refusal must be atomic: the original reading survives untouched.
    // 拒绝必须是原子的：原有读法保持不变。
    expect(
      sameIdentity.render().map((event) => (event.event as { event_kind?: string }).event_kind),
    ).toEqual(["question_created"]);
    expect(
      (
        sameIdentity.render()[0].event as {
          after_state: { kind: string };
        }
      ).after_state.kind,
    ).toBe("generation_ownership_cleared");
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

  /** Two windows over one session must not consume or restate each other's single message.
   * 同一会话上的两个窗口不得消费或改写彼此的那一条消息。 */
  it("keeps two windows over one session independent for the same single event", () => {
    // The baseline is the one authoritative snapshot both windows hydrate from.
    // 该基线是两个窗口共同水合的同一份权威快照。
    const baseline = batch([summary(1, userInput(1)), summary(2, assistantText(5))]);
    const first = new ConversationEventWindowStore();
    const second = new ConversationEventWindowStore();
    first.hydrate(baseline);
    second.hydrate(baseline);

    // A live append of one message reaches both windows with the same identity and revision.
    // 一条消息的实时追加以相同身份与修订到达两个窗口。
    const appended = livePatch([summary(3, userInput(3), 1_700_000_000_500, 2)], { revision: 2 });
    first.applyLive(appended);
    second.applyLive(appended);

    // Both windows observe the message exactly once: neither consumed it from the other.
    // 两个窗口各观察到该消息恰好一次：没有任何一方从另一方消费掉它。
    expect(first.render().map((event) => event.seq)).toEqual([1, 2, 3]);
    expect(second.render().map((event) => event.seq)).toEqual([1, 2, 3]);
    expect(first.render().filter((event) => event.seq === 3)).toHaveLength(1);
    expect(second.render().filter((event) => event.seq === 3)).toHaveLength(1);
    // The same single event must not be restated with a different body in either window.
    // 同一条消息不得在任一窗口中以不同正文被复述。
    expect(first.render()[2]).toEqual(second.render()[2]);

    // One window advancing further is a rendering change that must not reach the other window.
    // 一个窗口继续前进属于渲染变化，不得传递到另一个窗口。
    first.applyLive(livePatch([summary(4, assistantText(6), 1_700_000_000_900, 3)], { revision: 3 }));
    expect(first.render().map((event) => event.seq)).toEqual([1, 2, 3, 4]);
    expect(second.render().map((event) => event.seq)).toEqual([1, 2, 3]);
    expect(second.getOwner()).toEqual(first.getOwner());

    // A window may re-apply the identical revision idempotently without touching the other window.
    // 一个窗口可以幂等地重复应用同一修订，而不影响另一个窗口。
    second.applyLive(appended);
    expect(second.render().map((event) => event.seq)).toEqual([1, 2, 3]);
    expect(first.render().map((event) => event.seq)).toEqual([1, 2, 3, 4]);
  });
});

describe("conversation-event-window-store V1 text appends and merged ranges", () => {
  /** Hydrates one streaming assistant text at an explicit memory revision.
   * 在显式内存修订处 hydration 一条流式 assistant 正文。 */
  function streamingStore(text: string, memoryRevision: number): ConversationEventWindowStore {
    const store = new ConversationEventWindowStore();
    store.hydrate({
      ...batch([summary(1, assistantText(5, "streaming", text), 100, memoryRevision)]),
      memory_revision: memoryRevision,
      durable_revision: memoryRevision,
    });
    return store;
  }

  /** Builds one text append to seq 1.
   * 构建一次针对 seq 1 的正文追加。 */
  function append(baseLength: number, text: string, eventRevision: number): ConversationEventTextAppend {
    return {
      seq: 1,
      event_revision: eventRevision,
      updated_at_ms: 100 + eventRevision,
      base_text_utf16_len: baseLength,
      appended_text: text,
    };
  }

  /** Builds one volatile patch spanning `base..revision` with only text appends.
   * 构建一份跨越 `base..revision`、仅含正文追加的 volatile patch。 */
  function appendPatch(
    base: number,
    revision: number,
    appends: ConversationEventTextAppend[],
  ): ConversationEventLivePatch {
    return livePatch([], { base_memory_revision: base, revision, text_appends: appends });
  }

  it("applies a UTF-16 measured append exactly like the complete event", () => {
    const appended = streamingStore("你好😀", 1);
    appended.applyLive(appendPatch(1, 2, [append("你好😀".length, "，世界", 2)]));
    const complete = streamingStore("你好😀", 1);
    complete.applyLive(livePatch([summary(1, assistantText(5, "streaming", "你好😀，世界"), 102, 2)]));

    expect("你好😀".length).toBe(4);
    expect(appended.render()).toEqual(complete.render());
    expect(appended.getFrontier()).toEqual(complete.getFrontier());
  });

  it("replays the same revision without changing the window", () => {
    const store = streamingStore("ab", 1);
    const patch = appendPatch(1, 2, [append(2, "cd", 2)]);
    store.applyLive(patch);
    const before = store.render();
    store.applyLive(patch);

    expect(store.render()).toEqual(before);
    expect(store.render()[0].event).toMatchObject({ text: "abcd" });
  });

  it("rejects an append whose base disagrees with the loaded text", () => {
    const store = streamingStore("ab", 1);

    expect(() => store.applyLive(appendPatch(1, 2, [append(1, "cd", 2)])))
      .toThrow(/expects base length 1/);
    expect(store.getFrontier()?.memory_revision).toBe(1);
    expect(store.render()[0].event).toMatchObject({ text: "ab" });
  });

  it("rejects appends to finished, foreign, unknown, or duplicated events", () => {
    const completed = new ConversationEventWindowStore();
    completed.hydrate(batch([summary(1, assistantText(5, "completed", "ab"), 100, 1)]));
    expect(() => completed.applyLive(appendPatch(1, 2, [append(2, "cd", 2)])))
      .toThrow(/loaded completed text/);

    const foreign = new ConversationEventWindowStore();
    foreign.hydrate(batch([summary(1, userInput(1), 100, 1)]));
    expect(() => foreign.applyLive(appendPatch(1, 2, [append(0, "cd", 2)])))
      .toThrow(/targets user_input/);

    const store = streamingStore("ab", 1);
    expect(() => store.applyLive(appendPatch(1, 2, [{ ...append(2, "cd", 2), seq: 2 }])))
      .toThrow(/unknown seq 2/);
    expect(() => store.applyLive(livePatch(
      [summary(1, assistantText(5, "streaming", "abcd"), 102, 2)],
      { base_memory_revision: 1, revision: 2, text_appends: [append(2, "cd", 2)] },
    ))).toThrow(/repeats a complete event/);
    expect(() => store.applyLive(appendPatch(1, 2, [append(2, "", 2)])))
      .toThrow(/must append text/);
    expect(() => store.applyLive(livePatch([], {
      durability: "durable",
      base_durable_revision: 1,
      revision: 2,
      text_appends: [append(2, "cd", 2)],
    }))).toThrow(/must not carry text appends/);
    expect(store.render()[0].event).toMatchObject({ text: "ab" });
  });

  it("applies a merged range that starts before the loaded frontier", () => {
    // Loaded at revision 3, when seq 1 already read "abc" and seq 2 existed.
    // 已加载到修订 3，此时 seq 1 为 "abc"，seq 2 已存在。
    const store = new ConversationEventWindowStore();
    store.hydrate({
      ...batch([
        summary(1, assistantText(5, "streaming", "abc"), 100, 3),
        summary(2, userInput(2), 100, 1),
      ]),
      memory_revision: 3,
      durable_revision: 3,
    });
    // Merged range 1..5: seq 1 grew from "a", seq 2 is unchanged since 1, seq 3 is new.
    // 合并区间 1..5：seq 1 从 "a" 增长，seq 2 自修订 1 起未变，seq 3 为新事件。
    store.applyLive(livePatch(
      [summary(2, userInput(2), 100, 1), summary(3, userInput(3), 105, 1)],
      { base_memory_revision: 1, revision: 5, text_appends: [append(1, "bcde", 5)] },
    ));

    expect(store.getFrontier()?.memory_revision).toBe(5);
    expect(store.render().map((event) => event.seq)).toEqual([1, 2, 3]);
    expect(store.render()[0]).toMatchObject({ event_revision: 5, event: { text: "abcde" } });

    // A range whose loaded text disagrees with its target is rejected.
    // 已加载正文与目标不一致的区间会被拒绝。
    const drifted = streamingStore("abX", 3);
    expect(() => drifted.applyLive(appendPatch(1, 5, [append(1, "bcde", 5)])))
      .toThrow(/expects base length 1/);
  });

  it("still reports a gap for a range that starts after the frontier", () => {
    const store = streamingStore("ab", 1);

    expect(() => store.applyLive(appendPatch(2, 4, [append(3, "cd", 4)])))
      .toThrow(ConversationEventRevisionGapError);
    expect(() => store.applyLive(appendPatch(2, 2, [])))
      .toThrow(/must advance its base/);
    expect(store.getFrontier()?.memory_revision).toBe(1);
  });
});
