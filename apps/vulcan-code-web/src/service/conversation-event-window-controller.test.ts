import { describe, expect, it, vi } from "vitest";

import goldenFixture from "../../../../crates/vulcan-service-api/tests/fixtures/conversation_event_window_v1.golden.json";
import { ConversationLiveStreamStore } from "@/state/conversation-live-stream-store";
import {
  SESSION_EVENT_HYDRATION_V1_METHOD,
  SESSION_EVENT_PAGE_V1_METHOD,
  SESSION_EVENT_RECONNECT_V1_METHOD,
  SESSION_EVENT_DELTA_V1_TOPIC,
  ServiceConversationEventWindowClient,
  type ConversationEventHydrationV1Response,
  type ConversationEventLiveDeltaV1,
  type ConversationEventReconnectV1Response,
  type ConversationEventPageV1Response,
  type ConversationEventWindowRpc,
  type ConversationSessionConfigV1,
} from "./conversation-event-window-client";
import { ConversationEventWindowController } from "./conversation-event-window-controller";
import {
  ConversationEventWindowStore,
  clientActionId,
  type ConversationEventBody,
  type ConversationEventSummary,
  type ConversationEventWindowOwner,
} from "./conversation-event-window-store";

/** Stable session identity used by controller integration fixtures.
 * 控制器集成夹具使用的稳定会话身份。 */
const SESSION_ID = "session-primary";

/** Stable service-process epoch used by controller integration fixtures.
 * 控制器集成夹具使用的稳定服务进程代次。 */
const SERVICE_EPOCH = "service-epoch-primary";

/** Replacement service-process epoch used by restart recovery fixtures.
 * 服务重启恢复夹具使用的替换服务进程代次。 */
const SERVICE_EPOCH_NEXT = "service-epoch-next";

/** Returns an asynchronously assigned test callback or fails with the owning label.
 * 返回异步赋值的测试回调；若尚未赋值，则携带归属标签明确失败。
 * @param callback Callback assigned by the mocked subscription or promise executor.
 *                 由模拟订阅或 Promise 执行器赋值的回调。
 * @param label Human-readable callback ownership used by the assertion failure.
 *              断言失败时使用的可读回调归属标签。
 * @returns The assigned callback.
 *          已赋值的回调。
 */
function requireAssignedCallback<TValue>(
  callback: ((value: TValue) => void) | null,
  label: string,
): (value: TValue) => void {
  if (callback === null) {
    throw new Error(`${label} was not assigned`);
  }
  return callback;
}

/** Builds one complete event summary without flattening the typed body.
 * 在不展平强类型事件体的情况下构建一条完整事件摘要。 */
function summary(
  seq: number,
  event: ConversationEventBody,
  updatedAtMs = 10,
  eventRevision = 1,
  turnId: number | null = 1,
): ConversationEventSummary {
  return {
    seq,
    turn_id: turnId,
    created_at_ms: 1,
    updated_at_ms: updatedAtMs,
    event_revision: eventRevision,
    visibility: event.kind === "display_note" ? "ui_only" : "provider",
    event,
  };
}

/** Builds one user-input body with an exact local identity.
 * 构建一条具有精确本地身份的用户输入事件体。 */
function userInput(inputId: number): ConversationEventBody {
  return {
    kind: "user_input",
    client_action_id: clientActionId(`controller-input-${inputId}`),
    input_id: inputId,
    input_source: "root",
    text: `input-${inputId}`,
  };
}

/** Builds one mutable tool-call body for reconnect and stale-update tests.
 * 为重连与过期更新测试构建一条可变工具调用事件体。 */
function toolCall(
  status: Extract<ConversationEventBody, { kind: "tool_call" }>["status"],
  toolCallId = 3,
  requestStreamId = `controller-tool-${toolCallId}`,
): Extract<ConversationEventBody, { kind: "tool_call" }> {
  return {
    kind: "tool_call",
    api_call_id: 2,
    tool_call_id: toolCallId,
    tool_name: "file",
    input: { path: "D:/project/README.md" },
    request_stream_id: requestStreamId,
    status,
  };
}

/** Builds one mutable assistant-text body for open-Turn reconnect tests.
 * 为开放 Turn 重连测试构建一条可变 assistant 正文事件体。 */
function assistantText(
  status: Extract<ConversationEventBody, { kind: "assistant_text" }>["status"],
  text: string,
): Extract<ConversationEventBody, { kind: "assistant_text" }> {
  return {
    kind: "assistant_text",
    api_call_id: 2,
    text,
    status,
  };
}

/** Builds one V1 metadata configuration for selected-session projection tests.
 * 为选中会话投影测试构建一份 V1 metadata 配置。 */
function sessionConfig(
  overrides: Partial<ConversationSessionConfigV1> = {},
): ConversationSessionConfigV1 {
  return {
    current_request: {
      provider: null,
      connection_id: null,
      model: null,
      thinking_level: null,
      temperature: null,
      set_at_ms: null,
    },
    next_request: null,
    context_compaction_window_node_id: null,
    ...overrides,
  };
}

/** Builds one full hydration/page response around canonical summaries.
 * 围绕 canonical 摘要构建一份完整 hydration/分页响应。 */
function response(
  events: readonly ConversationEventSummary[],
  options: Partial<ConversationEventHydrationV1Response> = {},
): ConversationEventHydrationV1Response {
  return {
    session_id: SESSION_ID,
    window_generation: 1,
    service_epoch: SERVICE_EPOCH,
    memory_revision: 1,
    durable_revision: 1,
    events,
    older_before_seq: null,
    has_more: false,
    open_turn_id: null,
    open_turn_start_seq: null,
    session_config: sessionConfig(),
    ...options,
  };
}

/** Builds one historical page that cannot carry owner-generation revision frontiers.
 * 构建一份不能携带归属代际修订前沿的历史分页。 */
function pageResponse(
  events: readonly ConversationEventSummary[],
  options: Partial<ConversationEventPageV1Response> = {},
): ConversationEventPageV1Response {
  return {
    session_id: SESSION_ID,
    window_generation: 1,
    events,
    older_before_seq: null,
    has_more: false,
    open_turn_id: null,
    open_turn_start_seq: null,
    session_config: sessionConfig(),
    ...options,
  };
}

/** Builds one revisioned sparse live patch using the Rust transport contract.
 * 按 Rust 传输契约构建一份带修订号的稀疏实时 patch。
 * @param events Exact changed events or reconnect recovery range.
 *               精确变更事件或重连恢复范围。
 * @param options Optional owner, revision, or durability overrides.
 *                可选的归属、修订号或持久化状态覆盖。
 * @returns One strongly typed live delta.
 *          一份强类型实时增量。
 */
function liveDelta(
  events: readonly ConversationEventSummary[],
  options: Partial<Omit<ConversationEventLiveDeltaV1, "events">> = {},
): ConversationEventLiveDeltaV1 {
  const eventRevision = events.reduce(
    (current, summaryValue) => Math.max(current, summaryValue.event_revision),
    0,
  );
  const revision = options.revision ?? eventRevision;
  const durability = options.durability ?? "volatile";
  return {
    session_id: SESSION_ID,
    window_generation: 1,
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

/** Builds one authoritative reconnect response with explicit source and target frontiers.
 * 构建一份带显式来源与目标前沿的权威重连响应。 */
function reconnectResponse(
  events: readonly ConversationEventSummary[],
  options: Partial<Omit<ConversationEventReconnectV1Response, "events">> = {},
): ConversationEventReconnectV1Response {
  const memoryRevision = events.reduce(
    (current, summaryValue) => Math.max(current, summaryValue.event_revision),
    0,
  );
  return {
    session_id: SESSION_ID,
    window_generation: 1,
    service_epoch: SERVICE_EPOCH,
    base_memory_revision: 1,
    memory_revision: memoryRevision,
    durable_revision: memoryRevision,
    disposition: { kind: "recovery" },
    events,
    ...options,
  };
}

/** Creates an RPC mock whose methods fail unless a test overrides them.
 * 创建一个 RPC mock；除非测试覆盖方法，否则调用即失败。 */
function rpcMock(overrides: Partial<ConversationEventWindowRpc>): ConversationEventWindowRpc {
  return {
    hydrate: async () => {
      throw new Error("unexpected hydrate");
    },
    page: async () => {
      throw new Error("unexpected page");
    },
    reconnect: async () => {
      throw new Error("unexpected reconnect");
    },
    subscribeLive: async () => () => undefined,
    ...overrides,
  };
}

describe("V1 conversation event service client", () => {
  it("sends exact camelCase RPC parameters for live, embedded-snapshot, page, and reconnect reads", async () => {
    const request = vi.fn();
    const subscribe = vi.fn(async () => () => undefined);
    const service = {
      /** Records one generic service request while returning an empty typed test payload.
       * 记录一次通用服务请求，并返回空的强类型测试载荷。 */
      async request<T>(...args: Parameters<ConversationEventWindowRpcTestRequest>): Promise<T> {
        request(...args);
        return {} as T;
      },
      /** Records one typed production event subscription.
       * 记录一次类型化生产事件订阅。*/
      subscribe,
    };
    const client = new ServiceConversationEventWindowClient(service);
    const owner: ConversationEventWindowOwner = { session_id: SESSION_ID, window_generation: 7 };

    await client.hydrate(SESSION_ID);
    await client.hydrateEmbeddedTaskSnapshot("session-parent", SESSION_ID);
    await client.page(owner, 40);
    await client.reconnect(owner, 88, 12, SERVICE_EPOCH);
    const unsubscribe = await client.subscribeLive(() => undefined);
    unsubscribe();

    expect(request).toHaveBeenNthCalledWith(
      1,
      SESSION_EVENT_HYDRATION_V1_METHOD,
      { sessionId: SESSION_ID },
      undefined,
    );
    expect(request).toHaveBeenNthCalledWith(
      2,
      SESSION_EVENT_HYDRATION_V1_METHOD,
      { sessionId: SESSION_ID, parentSessionId: "session-parent" },
      undefined,
    );
    expect(request).toHaveBeenNthCalledWith(
      3,
      SESSION_EVENT_PAGE_V1_METHOD,
      { sessionId: SESSION_ID, windowGeneration: 7, beforeSeq: 40 },
      undefined,
    );
    expect(request).toHaveBeenNthCalledWith(
      4,
      SESSION_EVENT_RECONNECT_V1_METHOD,
      {
        sessionId: SESSION_ID,
        windowGeneration: 7,
        lastConfirmedSeq: 88,
        lastConfirmedMemoryRevision: 12,
        serviceEpoch: SERVICE_EPOCH,
      },
      undefined,
    );
    expect(subscribe).toHaveBeenCalledWith(SESSION_EVENT_DELTA_V1_TOPIC, expect.any(Function));
  });
});

/** Generic request signature mirrored by the narrow client test double.
 * 窄客户端测试替身镜像的通用请求签名。 */
type ConversationEventWindowRpcTestRequest = <T>(
  method: string,
  params?: Record<string, unknown>,
  options?: unknown,
) => Promise<T>;

describe("V1 conversation event window controller", () => {
  it("restores paged history before reconnecting and retains it while missing events arrive", async () => {
    // The first visit has loaded older pages; these events are required by saved viewport anchors.
    // 首次访问已加载更早分页；保存的视口锚点依赖这些事件。
    const first = new ConversationEventWindowController(rpcMock({
      hydrate: async () => response([summary(3, userInput(3)), summary(4, userInput(4))], { older_before_seq: 3, has_more: true }),
      page: async () => pageResponse([summary(1, userInput(1)), summary(2, userInput(2))]),
    }));
    await first.hydrate(SESSION_ID);
    await first.prependOlder();
    const cached = first.snapshot();
    first.dispose();
    // The new active controller must render immediately and recover only the missing revision range.
    // 新活动控制器必须立即渲染，并仅恢复缺失修订范围。
    const reconnect = vi.fn(async () => reconnectResponse([summary(5, userInput(5), 20, 2)], { memory_revision: 2, durable_revision: 2 }));
    const subscribeLive = vi.fn(async () => () => undefined);
    const restored = new ConversationEventWindowController(rpcMock({ reconnect, subscribeLive }));
    restored.restoreCachedSnapshot(cached);
    expect(restored.snapshot()).toEqual(cached);
    expect(restored.snapshot().events.map((event) => event.seq)).toEqual([1, 2, 3, 4]);
    expect(subscribeLive).not.toHaveBeenCalled();
    await restored.resumeCachedWindow();
    expect(subscribeLive).toHaveBeenCalledOnce();
    expect(reconnect).toHaveBeenCalledWith(cached.owner, 4, 1, SERVICE_EPOCH, undefined);
    expect(restored.snapshot().events.map((event) => event.seq)).toEqual([1, 2, 3, 4, 5]);
    restored.dispose();
  });

  it("hydrates an immutable snapshot without opening the live subscription", () => {
    const subscribeLive = vi.fn(async () => () => undefined);
    const controller = new ConversationEventWindowController(rpcMock({ subscribeLive }));

    expect(controller.hydrateSnapshot(response([summary(1, userInput(1))]))).toBe(true);

    expect(subscribeLive).not.toHaveBeenCalled();
    expect(controller.snapshot().events).toEqual([summary(1, userInput(1))]);
  });

  it("hands a settled request draft to the admitted V1 ToolCall without allowing replay revival", async () => {
    // requestStreamId is the exact identity shared by the live-only draft and canonical ToolCall.
    // requestStreamId 是仅实时草稿与 canonical ToolCall 共享的精确身份。
    const requestStreamId = "request-v1-handoff";
    // liveStore models the independent high-frequency request-progress side channel.
    // liveStore 模拟独立的高频请求进度旁路。
    const liveStore = new ConversationLiveStreamStore();
    expect(liveStore.applyEnvelope({
      generation: 1,
      delta: {
        kind: "toolRequestProgressChanged",
        payload: {
          conversationId: SESSION_ID,
          requestStreamId,
          progress: {
            requestStreamId,
            toolCallId: "provider-call-3",
            toolName: "file",
            state: "ready",
            receivedBytes: 128,
            receivedCharacters: 128,
            requestRevision: 4,
          },
        },
      },
    })).toBe(true);
    // controller settles the draft only after the consecutive hydration window commits.
    // controller 仅在连续 hydration 窗口提交后封口草稿。
    const controller = new ConversationEventWindowController(
      rpcMock({
        hydrate: async () => response([
          summary(1, toolCall("running", 3, requestStreamId)),
        ]),
      }),
      new ConversationEventWindowStore(),
      liveStore,
    );

    await controller.hydrate(SESSION_ID);

    expect(liveStore.getToolRequestProgress(SESSION_ID, requestStreamId)).toBeNull();
    expect(liveStore.applyEnvelope({
      generation: 2,
      delta: {
        kind: "toolRequestProgressChanged",
        payload: {
          conversationId: SESSION_ID,
          requestStreamId,
          progress: {
            requestStreamId,
            toolCallId: "provider-call-3",
            toolName: "file",
            state: "ready",
            receivedBytes: 128,
            receivedCharacters: 128,
            requestRevision: 5,
          },
        },
      },
    })).toBe(false);
    expect(liveStore.getToolRequestProgress(SESSION_ID, requestStreamId)).toBeNull();
  });

  it("hydrates the Rust JSON golden through the same TypeScript DTO and store", async () => {
    const golden = goldenFixture as unknown as ConversationEventHydrationV1Response;
    const controller = new ConversationEventWindowController(
      rpcMock({ hydrate: async () => golden }),
    );

    await controller.hydrate("session-golden");

    expect(controller.snapshot()).toMatchObject({
      owner: { session_id: "session-golden", window_generation: 7 },
      memory_revision: golden.memory_revision,
      durable_revision: golden.durable_revision,
      older_before_seq: null,
      has_more: false,
      open_turn_id: 2,
      open_turn_start_seq: 1,
    });
    expect(controller.snapshot().events).toEqual(golden.events);
    // userInput proves the Rust JSON golden retains the mandatory browser idempotency key.
    // userInput 证明 Rust JSON golden 保留必填的浏览器幂等键。
    const userInputSummary = controller
      .snapshot()
      .events.find((summary) => summary.event.kind === "user_input");
    if (userInputSummary === undefined || userInputSummary.event.kind !== "user_input") {
      throw new Error("Rust V1 golden is missing the required UserInput event");
    }
    expect(userInputSummary.event.client_action_id).toBe("golden-root-input");
    // queuedResolution proves queue disposal remains an ordered durable UI fact.
    // queuedResolution 证明队列处置仍是按序持久化的 UI 事实。
    const queuedResolution = controller
      .snapshot()
      .events.find((summary) => summary.event.kind === "queued_input_resolution");
    if (
      queuedResolution === undefined
      || queuedResolution.event.kind !== "queued_input_resolution"
    ) {
      throw new Error("Rust V1 golden is missing the queued input resolution event");
    }
    expect(queuedResolution.turn_id).toBeNull();
    expect(queuedResolution.event).toMatchObject({
      client_action_id: "golden-queued-input",
      input_id: 3,
      resolution: "removed",
      text: "queued then removed",
    });
    // transportObserved proves wire dispatch acknowledgement is a host-only ledger fact.
    // transportObserved 证明网络派发确认是仅宿主可见的账本事实。
    const transportObserved = controller
      .snapshot()
      .events.find(
        (summary) => summary.event.kind === "execution_control"
          && summary.event.control_kind === "api_transport_observed",
      );
    if (
      transportObserved === undefined
      || transportObserved.event.kind !== "execution_control"
    ) {
      throw new Error("Rust V1 golden is missing the API transport observation event");
    }
    expect(transportObserved.visibility).toBe("host_only");
    expect(transportObserved.event.api_call_id).toBe(4);
    expect(transportObserved.event.client_action_id).toBeUndefined();
    // apiStats proves the shared Rust fixture exposes the frozen continuation policy verbatim.
    // apiStats 证明共享 Rust 夹具逐字暴露冻结的 Continue 策略。
    const apiStats = controller
      .snapshot()
      .events.find((summary) => summary.event.kind === "api_stats");
    if (apiStats === undefined || apiStats.event.kind !== "api_stats") {
      throw new Error("Rust V1 golden is missing the required ApiStats event");
    }
    expect(apiStats.event.continuation_eligibility).toBe("allowed");
    expect(apiStats.event.end_reason).toBe("process_interrupted");
  });

  it("projects current, next, and compaction UI state only from the V1 response", async () => {
    const projectedConfig = sessionConfig({
      current_request: {
        provider: "openai",
        connection_id: "current-account",
        model: "gpt-current",
        thinking_level: "high",
        temperature: 0.2,
        set_at_ms: null,
      },
      next_request: {
        provider: "openai",
        connection_id: "next-account",
        model: "gpt-next",
        thinking_level: "xhigh",
        temperature: 0.4,
        set_at_ms: 99,
      },
      context_compaction_window_node_id: "compression-window-4",
    });
    const controller = new ConversationEventWindowController(
      rpcMock({
        hydrate: async () => response([summary(1, userInput(1))], { session_config: projectedConfig }),
      }),
    );

    await controller.hydrate(SESSION_ID);

    expect(controller.snapshot().session_config).toEqual(projectedConfig);
  });

  it("prepends only an adjacent complete-Turn page and advances its cursor", async () => {
    const page = vi.fn(async () =>
      pageResponse([summary(1, userInput(1)), summary(2, userInput(2))], {
        older_before_seq: null,
        has_more: false,
      }),
    );
    const controller = new ConversationEventWindowController(
      rpcMock({
        hydrate: async () =>
          response([summary(3, userInput(3)), summary(4, userInput(4))], {
            memory_revision: 5,
            durable_revision: 4,
            older_before_seq: 3,
            has_more: true,
          }),
        page,
      }),
    );
    await controller.hydrate(SESSION_ID);

    expect(await controller.prependOlder()).toBe(true);

    expect(page).toHaveBeenCalledWith(
      { session_id: SESSION_ID, window_generation: 1 },
      3,
      undefined,
    );
    expect(controller.snapshot().events.map((event) => event.seq)).toEqual([1, 2, 3, 4]);
    expect(controller.snapshot().has_more).toBe(false);
    expect(controller.snapshot()).toMatchObject({ memory_revision: 5, durable_revision: 4 });
  });

  it("shares one in-flight historical page across concurrent prepend requests", async () => {
    // resolvePage releases the exact RPC response after duplicate commands join one task.
    // resolvePage 在重复命令加入同一任务后释放精确 RPC 响应。
    let resolvePage: ((value: ConversationEventPageV1Response) => void) | null = null;
    // page records the single transport call owned by the controller single-flight.
    // page 记录控制器单飞持有的唯一传输调用。
    const page = vi.fn(() => new Promise<ConversationEventPageV1Response>((resolve) => {
      resolvePage = resolve;
    }));
    const controller = new ConversationEventWindowController(
      rpcMock({
        hydrate: async () => response([summary(3, userInput(3))], {
          older_before_seq: 3,
          has_more: true,
        }),
        page,
      }),
    );
    await controller.hydrate(SESSION_ID);

    const first = controller.prependOlder();
    const second = controller.prependOlder();

    expect(page).toHaveBeenCalledTimes(1);
    requireAssignedCallback(resolvePage, "historical page resolver")(
      pageResponse([summary(1, userInput(1)), summary(2, userInput(2))]),
    );
    await expect(Promise.all([first, second])).resolves.toEqual([true, true]);
    expect(controller.snapshot().events.map((event) => event.seq)).toEqual([1, 2, 3]);
  });

  it("keeps the snapshot unchanged after pagination failure and permits an exact retry", async () => {
    // page fails once before returning the adjacent page on the next command.
    // page 首次失败，下一条命令再返回相邻分页。
    const page = vi.fn()
      .mockRejectedValueOnce(new Error("page failed"))
      .mockResolvedValueOnce(pageResponse([summary(1, userInput(1)), summary(2, userInput(2))]));
    const controller = new ConversationEventWindowController(
      rpcMock({
        hydrate: async () => response([summary(3, userInput(3))], {
          older_before_seq: 3,
          has_more: true,
        }),
        page,
      }),
    );
    await controller.hydrate(SESSION_ID);
    const snapshotBeforeFailure = controller.snapshot();

    await expect(controller.prependOlder()).rejects.toThrow("page failed");

    expect(controller.snapshot()).toBe(snapshotBeforeFailure);
    await expect(controller.prependOlder()).resolves.toBe(true);
    expect(page).toHaveBeenCalledTimes(2);
    expect(controller.snapshot().events.map((event) => event.seq)).toEqual([1, 2, 3]);
  });

  it("does not commit a historical page after controller disposal", async () => {
    // resolvePage releases an RPC response after the controller loses ownership.
    // resolvePage 在控制器失去归属后释放 RPC 响应。
    let resolvePage: ((value: ConversationEventPageV1Response) => void) | null = null;
    // page remains pending until disposal has invalidated the controller.
    // page 保持等待，直到释放操作使控制器失效。
    const page = vi.fn(() => new Promise<ConversationEventPageV1Response>((resolve) => {
      resolvePage = resolve;
    }));
    const controller = new ConversationEventWindowController(
      rpcMock({
        hydrate: async () => response([summary(3, userInput(3))], {
          older_before_seq: 3,
          has_more: true,
        }),
        page,
      }),
    );
    await controller.hydrate(SESSION_ID);
    const snapshotBeforeDispose = controller.snapshot();
    const pending = controller.prependOlder();

    controller.dispose();
    requireAssignedCallback(resolvePage, "disposed page resolver")(
      pageResponse([summary(1, userInput(1)), summary(2, userInput(2))]),
    );

    await expect(pending).rejects.toThrow(/controller is disposed/);
    expect(controller.snapshot()).toBe(snapshotBeforeDispose);
  });

  it("applies a duplicate reconnect revision before consecutive missing events", async () => {
    const reconnect = vi.fn(async (): Promise<ConversationEventReconnectV1Response> => reconnectResponse([
        summary(1, toolCall("completed"), 20, 2),
        summary(2, userInput(2), 20),
      ], { base_memory_revision: 1, memory_revision: 2, durable_revision: 2 }));
    const controller = new ConversationEventWindowController(
      rpcMock({
        hydrate: async () => response([summary(1, toolCall("running"), 10)]),
        reconnect,
      }),
    );
    await controller.hydrate(SESSION_ID);

    await controller.reconnect();

    expect(reconnect).toHaveBeenCalledWith(
      { session_id: SESSION_ID, window_generation: 1 },
      1,
      1,
      SERVICE_EPOCH,
      undefined,
    );
    expect(controller.snapshot().events.map((event) => event.seq)).toEqual([1, 2]);
    expect(controller.snapshot().events[0].event).toMatchObject({ status: "completed" });
    expect(controller.snapshot()).toMatchObject({ memory_revision: 2, durable_revision: 2 });
  });

  it("keeps volatile state visible across flush failure and advances durable on retry acknowledgement", async () => {
    const controller = new ConversationEventWindowController(
      rpcMock({
        hydrate: async () => response(
          [summary(1, toolCall("running"), 10, 1)],
          { memory_revision: 1, durable_revision: 0 },
        ),
      }),
    );
    await controller.hydrate(SESSION_ID);

    controller.applyLive(liveDelta(
      [summary(1, toolCall("completed"), 20, 2)],
      { revision: 2, durability: "volatile" },
    ));
    expect(controller.snapshot()).toMatchObject({
      memory_revision: 2,
      durable_revision: 0,
    });

    controller.applyLive(liveDelta([], {
      revision: 2,
      durability: "durable",
      base_durable_revision: 0,
    }));
    controller.applyLive(liveDelta([], {
      revision: 2,
      durability: "durable",
      base_durable_revision: 0,
    }));
    expect(controller.snapshot()).toMatchObject({
      memory_revision: 2,
      durable_revision: 2,
    });
  });

  it("keeps live delivery synchronized when a completed assistant gains its durable Provider source", async () => {
    const completed = assistantText("completed", "answer");
    const withProviderSource = {
      ...completed,
      provider_source: {
        provider: "test-provider",
        model: "test-model",
        covered_until_seq: 1,
        fingerprint_sha256: "a".repeat(64),
      },
    } satisfies Extract<ConversationEventBody, { kind: "assistant_text" }>;
    const controller = new ConversationEventWindowController(
      rpcMock({
        hydrate: async () => response(
          [summary(1, completed, 10, 1)],
          { memory_revision: 1, durable_revision: 1 },
        ),
      }),
    );
    await controller.hydrate(SESSION_ID);

    controller.applyLive(liveDelta(
      [summary(1, withProviderSource, 20, 2)],
      { revision: 2, durability: "volatile" },
    ));
    controller.applyLive(liveDelta([], {
      revision: 2,
      durability: "durable",
      base_durable_revision: 1,
    }));

    expect(controller.snapshot()).toMatchObject({
      memory_revision: 2,
      durable_revision: 2,
    });
    expect(controller.snapshot().events[0].event).toEqual(withProviderSource);
  });

  it("rejects frontier overflow and durable-ahead acknowledgements without regressing on stale delivery", async () => {
    const controller = new ConversationEventWindowController(
      rpcMock({
        hydrate: async () => response(
          [summary(1, toolCall("completed"), 30, 3)],
          { memory_revision: 3, durable_revision: 1 },
        ),
      }),
    );
    await controller.hydrate(SESSION_ID);

    controller.applyLive(liveDelta([], { revision: 2, durability: "durable" }));
    controller.applyLive(liveDelta(
      [summary(1, toolCall("running"), 20, 2)],
      { revision: 2, durability: "volatile" },
    ));
    controller.applyLive(liveDelta([], { revision: 1, durability: "durable" }));
    expect(controller.snapshot()).toMatchObject({ memory_revision: 3, durable_revision: 2 });
    expect(controller.snapshot().events[0].event).toMatchObject({ status: "completed" });

    expect(() => controller.applyLive(
      liveDelta([], { revision: 4, durability: "durable", base_durable_revision: 2 }),
    )).toThrow(/durable revision 4 exceeds memory revision 3/);
    expect(() => controller.applyLive(
      liveDelta([], { revision: Number.MAX_SAFE_INTEGER + 1, durability: "volatile" }),
    )).toThrow(/unsafe document revision/);
    expect(controller.snapshot()).toMatchObject({ memory_revision: 3, durable_revision: 2 });
  });

  it("replays the full open Turn so earlier parallel states reach their terminal revisions", async () => {
    const reconnect = vi.fn(async (): Promise<ConversationEventReconnectV1Response> => reconnectResponse([
        summary(10, toolCall("completed", 3), 50, 2),
        summary(11, toolCall("completed", 4), 50, 4),
        summary(12, assistantText("completed", "answer"), 40, 3),
      ], { base_memory_revision: 1, memory_revision: 4, durable_revision: 4 }));
    const controller = new ConversationEventWindowController(
      rpcMock({
        hydrate: async () =>
          response(
            [
              summary(10, toolCall("running", 3), 50, 1),
              summary(11, toolCall("waiting_user", 4), 50, 2),
              summary(12, assistantText("streaming", "part"), 50, 1),
            ],
            { open_turn_id: 1, open_turn_start_seq: 10 },
          ),
        reconnect,
      }),
    );
    await controller.hydrate(SESSION_ID);

    await controller.reconnect();

    expect(reconnect).toHaveBeenCalledWith(
      { session_id: SESSION_ID, window_generation: 1 },
      12,
      1,
      SERVICE_EPOCH,
      undefined,
    );
    expect(controller.snapshot().events.map((event) => event.event_revision)).toEqual([2, 4, 3]);
    expect(controller.snapshot().events.map((event) =>
      "status" in event.event ? event.event.status : null,
    )).toEqual(["completed", "completed", "completed"]);
  });

  it("subscribes before hydration and atomically applies buffered sparse live patches", async () => {
    let push: ((delta: ConversationEventLiveDeltaV1) => void) | null = null;
    let resolveHydration: ((value: ConversationEventHydrationV1Response) => void) | null = null;
    const hydration = new Promise<ConversationEventHydrationV1Response>((resolve) => {
      resolveHydration = resolve;
    });
    const controller = new ConversationEventWindowController(
      rpcMock({
        hydrate: async () => hydration,
        subscribeLive: async (handler) => {
          push = handler;
          return () => undefined;
        },
      }),
    );

    const hydrationTask = controller.hydrate(SESSION_ID);
    await Promise.resolve();
    expect(push).not.toBeNull();
    requireAssignedCallback(push, "live patch subscriber")(
      liveDelta(
        [summary(1, toolCall("completed"), 20, 2), summary(2, userInput(2), 20)],
        { revision: 2 },
      ),
    );
    requireAssignedCallback(resolveHydration, "hydration resolver")(
      response([summary(1, toolCall("running"), 10, 1)]),
    );
    await hydrationTask;

    expect(controller.snapshot().events.map((event) => event.seq)).toEqual([1, 2]);
    expect(controller.snapshot().events[0].event).toMatchObject({ status: "completed" });
    expect(controller.snapshot()).toMatchObject({ memory_revision: 2, durable_revision: 1 });
  });

  it("commits HTTP hydration even when physical live subscription readiness never settles", async () => {
    // subscribeLive models a WebSocket handshake that remains pending indefinitely.
    // subscribeLive 模拟一个无限期保持等待的 WebSocket 握手。
    const subscribeLive = vi.fn(
      () => new Promise<() => void>(() => undefined),
    );
    // hydrate proves durable session data remains independently available over HTTP.
    // hydrate 证明可靠会话数据仍可通过 HTTP 独立获得。
    const hydrate = vi.fn(async () => response([summary(1, userInput(1))]));
    // controller must publish the durable snapshot without waiting for physical socket readiness.
    // controller 必须在不等待物理套接字就绪的情况下发布可靠快照。
    const controller = new ConversationEventWindowController(rpcMock({
      hydrate,
      subscribeLive,
    }));

    await expect(controller.hydrate(SESSION_ID)).resolves.toBe(true);

    expect(subscribeLive).toHaveBeenCalledTimes(1);
    expect(hydrate).toHaveBeenCalledTimes(1);
    expect(controller.snapshot()).toMatchObject({
      owner: { session_id: SESSION_ID, window_generation: 1 },
      memory_revision: 1,
      durable_revision: 1,
    });
    expect(controller.snapshot().events.map((event) => event.seq)).toEqual([1]);
  });

  it("reconnects once after delayed live readiness to close the post-hydration gap", async () => {
    // resolveSubscription releases the exact delayed physical readiness barrier.
    // resolveSubscription 释放精确的延迟物理就绪屏障。
    let resolveSubscription: ((unsubscribe: () => void) => void) | null = null;
    // subscription remains pending until after the durable hydration has committed.
    // subscription 保持等待，直到可靠 hydration 已经提交。
    const subscription = new Promise<() => void>((resolve) => {
      resolveSubscription = resolve;
    });
    // reconnect returns the same frontier because this fixture has no missed live events.
    // reconnect 返回相同前沿，因为此夹具不存在遗漏的实时事件。
    const reconnect = vi.fn(async () => reconnectResponse([], {
      base_memory_revision: 1,
      memory_revision: 1,
      durable_revision: 1,
    }));
    // controller owns both the durable hydration and its delayed transport reconciliation.
    // controller 同时持有可靠 hydration 与其延迟传输协调。
    const controller = new ConversationEventWindowController(rpcMock({
      hydrate: async () => response([summary(1, userInput(1))]),
      reconnect,
      subscribeLive: async () => subscription,
    }));

    await expect(controller.hydrate(SESSION_ID)).resolves.toBe(true);
    expect(reconnect).not.toHaveBeenCalled();

    requireAssignedCallback(resolveSubscription, "delayed subscription resolver")(
      () => undefined,
    );
    await vi.waitFor(() => expect(reconnect).toHaveBeenCalledTimes(1));

    expect(reconnect).toHaveBeenCalledWith(
      { session_id: SESSION_ID, window_generation: 1 },
      1,
      1,
      SERVICE_EPOCH,
      undefined,
    );
    expect(controller.snapshot()).toMatchObject({
      memory_revision: 1,
      durable_revision: 1,
    });
  });

  it("repairs a skipped revision before a later durable acknowledgement can hide its append", async () => {
    let push: ((delta: ConversationEventLiveDeltaV1) => void) | null = null;
    let resolveReconnect: ((value: ConversationEventReconnectV1Response) => void) | null = null;
    const reconnectResult = new Promise<ConversationEventReconnectV1Response>((resolve) => {
      resolveReconnect = resolve;
    });
    const initial = Array.from({ length: 11 }, (_, offset) => {
      const seq = 90 + offset;
      return seq === 90
        ? summary(seq, toolCall("running", 90), 10, 1)
        : summary(seq, userInput(seq), 10, 1);
    });
    const reconnect = vi.fn(async () => reconnectResult);
    const controller = new ConversationEventWindowController(
      rpcMock({
        hydrate: async () => response(initial, { memory_revision: 3, durable_revision: 3 }),
        reconnect,
        subscribeLive: async (handler) => {
          push = handler;
          return () => undefined;
        },
      }),
    );
    await controller.hydrate(SESSION_ID);

    requireAssignedCallback(push, "live patch subscriber")(liveDelta(
      [summary(90, toolCall("completed", 90), 50, 2)],
      { base_memory_revision: 4, revision: 5 },
    ));
    requireAssignedCallback(push, "live patch subscriber")(liveDelta([], {
      durability: "durable",
      base_durable_revision: 3,
      revision: 4,
    }));
    expect(controller.snapshot()).toMatchObject({ memory_revision: 3, durable_revision: 3 });
    expect(controller.snapshot().events.at(-1)?.seq).toBe(100);

    requireAssignedCallback(resolveReconnect, "revision reconnect resolver")({
      session_id: SESSION_ID,
      window_generation: 1,
      service_epoch: SERVICE_EPOCH,
      base_memory_revision: 3,
      memory_revision: 5,
      durable_revision: 4,
      disposition: { kind: "recovery" },
      events: [
        summary(90, toolCall("completed", 90), 50, 2),
        summary(101, userInput(101), 40, 1),
      ],
    });
    await vi.waitFor(() => {
      expect(controller.snapshot()).toMatchObject({ memory_revision: 5, durable_revision: 4 });
      expect(controller.snapshot().events.at(-1)?.seq).toBe(101);
    });
    expect(reconnect).toHaveBeenCalledWith(
      { session_id: SESSION_ID, window_generation: 1 },
      100,
      3,
      SERVICE_EPOCH,
    );
  });

  it("rehydrates when reconnect reports that the service process changed", async () => {
    const hydrate = vi
      .fn<ConversationEventWindowRpc["hydrate"]>()
      .mockResolvedValueOnce(response(
        [summary(1, toolCall("completed"), 20, 2)],
        { memory_revision: 2, durable_revision: 1 },
      ))
      .mockResolvedValueOnce(response(
        [summary(1, toolCall("running"), 10, 1)],
        {
          service_epoch: SERVICE_EPOCH_NEXT,
          memory_revision: 1,
          durable_revision: 1,
        },
      ));
    const reconnect = vi.fn(async (): Promise<ConversationEventReconnectV1Response> =>
      reconnectResponse([], {
        service_epoch: SERVICE_EPOCH_NEXT,
        base_memory_revision: 2,
        memory_revision: 1,
        durable_revision: 1,
        disposition: { kind: "rehydrate_required", reason: "service_epoch_changed" },
      }));
    const controller = new ConversationEventWindowController(rpcMock({ hydrate, reconnect }));
    await controller.hydrate(SESSION_ID);

    await controller.recoverConnection(false);

    expect(controller.snapshot()).toMatchObject({
      service_epoch: SERVICE_EPOCH_NEXT,
      memory_revision: 1,
      durable_revision: 1,
    });
    expect(controller.snapshot().events[0].event).toMatchObject({ status: "running" });
  });

  it("buffers replacement-process pushes until durable hydration drops old volatile state", async () => {
    let push: ((delta: ConversationEventLiveDeltaV1) => void) | null = null;
    let resolveRestartHydration:
      | ((value: ConversationEventHydrationV1Response) => void)
      | null = null;
    const restartHydration = new Promise<ConversationEventHydrationV1Response>((resolve) => {
      resolveRestartHydration = resolve;
    });
    const hydrate = vi
      .fn<ConversationEventWindowRpc["hydrate"]>()
      .mockResolvedValueOnce(response(
        [summary(1, toolCall("running"), 10, 1)],
        { memory_revision: 1, durable_revision: 1 },
      ))
      .mockImplementationOnce(async () => restartHydration);
    const controller = new ConversationEventWindowController(rpcMock({
      hydrate,
      subscribeLive: async (handler) => {
        push = handler;
        return () => undefined;
      },
    }));
    await controller.hydrate(SESSION_ID);

    requireAssignedCallback(push, "live patch subscriber")(liveDelta(
      [summary(1, toolCall("completed"), 20, 2)],
      {
        service_epoch: SERVICE_EPOCH_NEXT,
        base_memory_revision: 1,
        revision: 2,
      },
    ));
    requireAssignedCallback(push, "live patch subscriber")(liveDelta(
      [summary(2, userInput(2), 30, 1)],
      {
        service_epoch: SERVICE_EPOCH_NEXT,
        base_memory_revision: 2,
        revision: 3,
      },
    ));
    requireAssignedCallback(
      resolveRestartHydration,
      "restart hydration resolver",
    )(response(
      [summary(1, toolCall("running"), 10, 1)],
      {
        service_epoch: SERVICE_EPOCH_NEXT,
        memory_revision: 1,
        durable_revision: 1,
      },
    ));

    await vi.waitFor(() => {
      expect(controller.snapshot()).toMatchObject({
        service_epoch: SERVICE_EPOCH_NEXT,
        memory_revision: 3,
        durable_revision: 1,
      });
    });
    expect(controller.snapshot().events.map((event) => event.seq)).toEqual([1, 2]);
    expect(controller.snapshot().events[0].event).toMatchObject({ status: "completed" });
  });

  it("uses incremental reconnect for a same-instance transport interruption", async () => {
    const reconnect = vi.fn(async (): Promise<ConversationEventReconnectV1Response> =>
      reconnectResponse(
        [summary(1, toolCall("completed"), 20, 2), summary(2, userInput(2), 20, 1)],
        { base_memory_revision: 1, memory_revision: 2, durable_revision: 1 },
      ));
    const controller = new ConversationEventWindowController(rpcMock({
      hydrate: async () => response(
        [summary(1, toolCall("running"), 10, 1)],
        { memory_revision: 1, durable_revision: 1 },
      ),
      reconnect,
    }));
    await controller.hydrate(SESSION_ID);

    await controller.recoverConnection(false);

    expect(reconnect).toHaveBeenCalledWith(
      { session_id: SESSION_ID, window_generation: 1 },
      1,
      1,
      SERVICE_EPOCH,
      undefined,
    );
    expect(controller.snapshot()).toMatchObject({ memory_revision: 2, durable_revision: 1 });
    expect(controller.snapshot().events.map((event) => event.seq)).toEqual([1, 2]);
  });

  it("ignores unrelated and stale-owner pushed deltas, then deduplicates reconnect overlap", async () => {
    let push: ((delta: ConversationEventLiveDeltaV1) => void) | null = null;
    const reconnect = vi.fn(async (): Promise<ConversationEventReconnectV1Response> => reconnectResponse(
      [summary(1, toolCall("completed"), 20, 2), summary(2, userInput(2), 20)],
      {
        window_generation: 2,
        base_memory_revision: 2,
        memory_revision: 3,
        durable_revision: 3,
      },
    ));
    const controller = new ConversationEventWindowController(
      rpcMock({
        hydrate: async () =>
          response([summary(1, toolCall("running"), 10, 1)], { window_generation: 2 }),
        reconnect,
        subscribeLive: async (handler) => {
          push = handler;
          return () => undefined;
        },
      }),
    );
    await controller.hydrate(SESSION_ID);

    requireAssignedCallback(push, "live patch subscriber")(
      liveDelta([summary(1, toolCall("completed"), 20, 2)], {
        session_id: "other-session",
        window_generation: 2,
        revision: 2,
      }),
    );
    requireAssignedCallback(push, "live patch subscriber")(
      liveDelta([summary(1, toolCall("completed"), 20, 2)], {
        window_generation: 1,
        revision: 2,
      }),
    );
    requireAssignedCallback(push, "live patch subscriber")(
      liveDelta([summary(1, toolCall("completed"), 20, 2)], {
        window_generation: 2,
        revision: 2,
      }),
    );
    await controller.reconnect();

    expect(controller.snapshot().events.map((event) => event.seq)).toEqual([1, 2]);
    expect(controller.snapshot().events[0]).toMatchObject({ event_revision: 2 });
    expect(controller.snapshot().events[0].event).toMatchObject({ status: "completed" });
    expect(controller.snapshot()).toMatchObject({ memory_revision: 3, durable_revision: 3 });
  });

  it("keeps the pushed frontier live across prepared, running, terminal, and durable tool updates", async () => {
    let push: ((delta: ConversationEventLiveDeltaV1) => void) | null = null;
    const prepared = toolCall("prepared", 3, "request-stream-3");
    const running = {
      ...prepared,
      status: "running" as const,
      started_at_ms: 20,
    };
    const completed = {
      ...running,
      status: "completed" as const,
      output: { result: "done" },
      finished_at_ms: 30,
    };
    const controller = new ConversationEventWindowController(
      rpcMock({
        hydrate: async () => response(
          [summary(1, prepared, 10, 1)],
          { memory_revision: 1, durable_revision: 1 },
        ),
        subscribeLive: async (handler) => {
          push = handler;
          return () => undefined;
        },
      }),
    );
    await controller.hydrate(SESSION_ID);

    requireAssignedCallback(push, "tool lifecycle subscriber")(
      liveDelta([summary(1, running, 20, 2)], { revision: 2 }),
    );
    requireAssignedCallback(push, "tool lifecycle subscriber")(
      liveDelta([summary(1, completed, 30, 3)], { revision: 3 }),
    );
    requireAssignedCallback(push, "tool lifecycle subscriber")
      (liveDelta([], { durability: "durable", base_durable_revision: 1, revision: 3 }));

    expect(controller.snapshot()).toMatchObject({ memory_revision: 3, durable_revision: 3 });
    expect(controller.snapshot().events[0].event).toMatchObject({
      status: "completed",
      started_at_ms: 20,
      finished_at_ms: 30,
    });
  });

  it("rehydrates instead of crashing when a pushed same-seq evolution cannot be reconciled", async () => {
    let push: ((delta: ConversationEventLiveDeltaV1) => void) | null = null;
    const warn = vi.spyOn(console, "warn").mockImplementation(() => undefined);
    const hydrate = vi
      .fn()
      .mockResolvedValueOnce(response(
        [summary(1, assistantText("completed", "initial"), 10, 1)],
        { memory_revision: 1, durable_revision: 1 },
      ))
      .mockResolvedValueOnce(response(
        [summary(1, assistantText("completed", "authoritative"), 30, 2)],
        { memory_revision: 2, durable_revision: 2 },
      ));
    const controller = new ConversationEventWindowController(
      rpcMock({
        hydrate,
        reconnect: async () => reconnectResponse(
          [summary(1, assistantText("completed", "reconnect"), 20, 2)],
          { base_memory_revision: 1, memory_revision: 2, durable_revision: 2 },
        ),
        subscribeLive: async (handler) => {
          push = handler;
          return () => undefined;
        },
      }),
    );
    await controller.hydrate(SESSION_ID);

    expect(() => requireAssignedCallback(push, "invalid evolution subscriber")(
      liveDelta(
        [summary(1, assistantText("completed", "invalid-live"), 20, 2)],
        { revision: 2 },
      ),
    )).not.toThrow();
    await vi.waitFor(() => expect(controller.snapshot().events[0].event).toMatchObject({
      text: "authoritative",
    }));

    expect(hydrate).toHaveBeenCalledTimes(2);
    expect(controller.snapshot()).toMatchObject({ memory_revision: 2, durable_revision: 2 });
    warn.mockRestore();
  });

  it("keeps an open edited Turn visible until a higher generation atomically truncates the tail", async () => {
    let push: ((delta: ConversationEventLiveDeltaV1) => void) | null = null;
    let resolveRebase: ((value: ConversationEventHydrationV1Response) => void) | null = null;
    const rebaseHydration = new Promise<ConversationEventHydrationV1Response>((resolve) => {
      resolveRebase = resolve;
    });
    const hydrate = vi
      .fn()
      .mockResolvedValueOnce(
        response(
          [
            summary(1, userInput(1)),
            summary(2, toolCall("running"), 10, 1),
            summary(3, userInput(3)),
            summary(4, userInput(4)),
          ],
          {
            memory_revision: 5,
            durable_revision: 4,
            open_turn_id: 1,
            open_turn_start_seq: 1,
          },
        ),
      )
      .mockImplementationOnce(async () => rebaseHydration);
    const controller = new ConversationEventWindowController(
      rpcMock({
        hydrate,
        subscribeLive: async (handler) => {
          push = handler;
          return () => undefined;
        },
      }),
    );
    await controller.hydrate(SESSION_ID);

    requireAssignedCallback(push, "live patch subscriber")(
      liveDelta([], { window_generation: 2, revision: 2 }),
    );
    expect(controller.snapshot().owner?.window_generation).toBe(1);
    expect(controller.snapshot().events.map((event) => event.seq)).toEqual([1, 2, 3, 4]);

    requireAssignedCallback(resolveRebase, "generation-two rebase resolver")(
      response([summary(1, userInput(1)), summary(2, toolCall("waiting_user"), 20, 2)], {
        window_generation: 2,
        memory_revision: 2,
        durable_revision: 2,
        open_turn_id: 1,
        open_turn_start_seq: 1,
      }),
    );
    await vi.waitFor(() => expect(controller.snapshot().owner?.window_generation).toBe(2));

    expect(controller.snapshot().events.map((event) => event.seq)).toEqual([1, 2]);
    expect(controller.snapshot().events[1].event).toMatchObject({ status: "waiting_user" });
    expect(controller.snapshot()).toMatchObject({ memory_revision: 2, durable_revision: 2 });
    requireAssignedCallback(push, "live patch subscriber")(
      liveDelta([summary(2, toolCall("completed"), 30, 3)], { revision: 3 }),
    );
    expect(controller.snapshot().events[1].event).toMatchObject({ status: "waiting_user" });
  });

  it("rehydrates an owned session by atomically rebasing to a newer generation", async () => {
    const hydrate = vi
      .fn()
      .mockResolvedValueOnce(response([summary(1, userInput(1))]))
      .mockResolvedValueOnce(
        response([summary(1, userInput(2))], {
          window_generation: 2,
          memory_revision: 2,
          durable_revision: 2,
        }),
      );
    const controller = new ConversationEventWindowController(rpcMock({ hydrate }));
    await controller.hydrate(SESSION_ID);

    await expect(controller.rehydrate(SESSION_ID)).resolves.toBe(true);

    expect(controller.snapshot().owner).toEqual({
      session_id: SESSION_ID,
      window_generation: 2,
    });
    expect(controller.snapshot().events[0].event).toMatchObject({ input_id: 2 });
  });

  it("accepts manual rehydration after live generation rebase without reinitializing the owner", async () => {
    let push: ((delta: ConversationEventLiveDeltaV1) => void) | null = null;
    const generationTwo = response([summary(1, userInput(2))], {
      window_generation: 2,
      memory_revision: 2,
      durable_revision: 2,
    });
    const hydrate = vi
      .fn()
      .mockResolvedValueOnce(response([summary(1, userInput(1))]))
      .mockResolvedValueOnce(generationTwo)
      .mockResolvedValueOnce(generationTwo);
    const controller = new ConversationEventWindowController(
      rpcMock({
        hydrate,
        subscribeLive: async (handler) => {
          push = handler;
          return () => undefined;
        },
      }),
    );
    await controller.hydrate(SESSION_ID);
    requireAssignedCallback(push, "generation rebase subscriber")(
      liveDelta([], { window_generation: 2, revision: 2 }),
    );
    await vi.waitFor(() => expect(controller.snapshot().owner?.window_generation).toBe(2));

    await expect(controller.rehydrate(SESSION_ID)).resolves.toBe(true);

    expect(controller.snapshot().owner?.window_generation).toBe(2);
    expect(hydrate).toHaveBeenCalledTimes(3);
  });

  it("ignores a delayed manual rehydration after live state reaches a newer generation", async () => {
    let push: ((delta: ConversationEventLiveDeltaV1) => void) | null = null;
    let resolveManual: ((value: ConversationEventHydrationV1Response) => void) | null = null;
    const delayedManual = new Promise<ConversationEventHydrationV1Response>((resolve) => {
      resolveManual = resolve;
    });
    const hydrate = vi
      .fn()
      .mockResolvedValueOnce(response([summary(1, userInput(1))]))
      .mockImplementationOnce(async () => delayedManual)
      .mockResolvedValueOnce(
        response([summary(1, userInput(3))], {
          window_generation: 3,
          memory_revision: 3,
          durable_revision: 3,
        }),
      );
    const controller = new ConversationEventWindowController(
      rpcMock({
        hydrate,
        subscribeLive: async (handler) => {
          push = handler;
          return () => undefined;
        },
      }),
    );
    await controller.hydrate(SESSION_ID);
    const manualRehydration = controller.rehydrate(SESSION_ID);
    requireAssignedCallback(push, "newer generation subscriber")(
      liveDelta([], { window_generation: 3, revision: 3 }),
    );
    await vi.waitFor(() => expect(controller.snapshot().owner?.window_generation).toBe(3));
    requireAssignedCallback(resolveManual, "delayed manual rehydration resolver")(
      response([summary(1, userInput(2))], {
        window_generation: 2,
        memory_revision: 2,
        durable_revision: 2,
      }),
    );

    await expect(manualRehydration).resolves.toBe(true);
    expect(controller.snapshot().owner?.window_generation).toBe(3);
    expect(controller.snapshot().events[0].event).toMatchObject({ input_id: 3 });
  });

  it("hydrates through rapid higher-generation pushes without committing an older response", async () => {
    let push: ((delta: ConversationEventLiveDeltaV1) => void) | null = null;
    let resolveGenerationTwo: ((value: ConversationEventHydrationV1Response) => void) | null = null;
    const generationTwoHydration = new Promise<ConversationEventHydrationV1Response>((resolve) => {
      resolveGenerationTwo = resolve;
    });
    const hydrate = vi
      .fn()
      .mockResolvedValueOnce(response([summary(1, userInput(1)), summary(2, userInput(2))]))
      .mockImplementationOnce(async () => generationTwoHydration)
      .mockResolvedValueOnce(
        response([summary(1, userInput(1))], {
          window_generation: 3,
          memory_revision: 3,
          durable_revision: 2,
          open_turn_id: 1,
          open_turn_start_seq: 1,
        }),
      );
    const controller = new ConversationEventWindowController(
      rpcMock({
        hydrate,
        subscribeLive: async (handler) => {
          push = handler;
          return () => undefined;
        },
      }),
    );
    await controller.hydrate(SESSION_ID);

    requireAssignedCallback(push, "live patch subscriber")(
      liveDelta([], { window_generation: 2, revision: 2 }),
    );
    requireAssignedCallback(push, "live patch subscriber")(
      liveDelta([], { window_generation: 3, revision: 3 }),
    );
    requireAssignedCallback(resolveGenerationTwo, "generation-two hydration resolver")(
      response([summary(1, userInput(1)), summary(2, userInput(2))], {
        window_generation: 2,
        memory_revision: 2,
        durable_revision: 2,
      }),
    );

    await vi.waitFor(() => expect(controller.snapshot().owner?.window_generation).toBe(3));
    expect(hydrate).toHaveBeenCalledTimes(3);
    expect(controller.snapshot().events.map((event) => event.seq)).toEqual([1]);
    expect(controller.snapshot()).toMatchObject({ memory_revision: 3, durable_revision: 2 });
  });

  it.each([
    {
      name: "gap",
      delta: liveDelta([summary(3, userInput(3))], { revision: 2 }),
      message: /gap/,
    },
    {
      name: "out-of-order",
      delta: liveDelta([
        summary(2, userInput(2)),
        summary(1, toolCall("completed"), 20, 2),
      ]),
      message: /gap/,
    },
    {
      name: "identity conflict",
      delta: liveDelta([summary(1, userInput(99), 20, 2)]),
      message: /identity conflict/,
    },
  ])("rejects a $name live patch atomically", async ({ delta, message }) => {
    const controller = new ConversationEventWindowController(
      rpcMock({ hydrate: async () => response([summary(1, toolCall("running"), 10)]) }),
    );
    await controller.hydrate(SESSION_ID);

    expect(() => controller.applyLive(delta)).toThrow(message);
    expect(controller.snapshot().events).toEqual([summary(1, toolCall("running"), 10)]);
  });

  it("keeps the complete old window visible during an atomic compaction rebase", async () => {
    const controller = new ConversationEventWindowController(
      rpcMock({ hydrate: async () => response([summary(1, userInput(1))]) }),
    );
    await controller.hydrate(SESSION_ID);
    const clearSpy = vi.spyOn(Map.prototype, "clear");
    const compaction = summary(
      2,
      {
        kind: "display_note",
        notice_kind: "compaction_completed",
        severity: "info",
        reason_code: "context_compacted",
      },
      20,
    );

    try {
      controller.rebase(
        response([summary(1, userInput(1)), compaction], { window_generation: 2 }),
      );
      expect(clearSpy).not.toHaveBeenCalled();
    } finally {
      clearSpy.mockRestore();
    }
    expect(controller.snapshot().events.map((event) => event.seq)).toEqual([1, 2]);
  });

  it("rejects delayed old-generation live data and ignores older same-seq state", async () => {
    const controller = new ConversationEventWindowController(
      rpcMock({
        hydrate: async () => response(
          [summary(1, toolCall("completed"), 30, 3)],
          { memory_revision: 3, durable_revision: 3 },
        ),
      }),
    );
    await controller.hydrate(SESSION_ID);
    controller.rebase(
      response([summary(1, toolCall("completed"), 30, 3)], {
        window_generation: 2,
        memory_revision: 3,
        durable_revision: 3,
      }),
    );

    expect(() =>
      controller.applyLive({
        session_id: SESSION_ID,
        window_generation: 1,
        service_epoch: SERVICE_EPOCH,
        base_memory_revision: 3,
        revision: 4,
        durability: "volatile",
        events: [summary(1, toolCall("running"), 40, 4)],
      }),
    ).toThrow(/generation mismatch/);
    controller.applyLive({
      session_id: SESSION_ID,
      window_generation: 2,
      service_epoch: SERVICE_EPOCH,
      base_memory_revision: 1,
      revision: 2,
      durability: "volatile",
      events: [summary(1, toolCall("running"), 20, 2)],
    });
    expect(controller.snapshot().events[0].event).toMatchObject({ status: "completed" });
    expect(controller.snapshot().events[0]).toMatchObject({ event_revision: 3, updated_at_ms: 30 });
  });

  it("rejects an incomplete open-Turn boundary before mutating the store", async () => {
    const invalid = response([summary(1, userInput(1))], {
      open_turn_id: 1,
      open_turn_start_seq: null,
    });
    const controller = new ConversationEventWindowController(
      rpcMock({ hydrate: async () => invalid }),
    );

    await expect(controller.hydrate(SESSION_ID)).rejects.toThrow(/open Turn boundary is incomplete/);
    expect(controller.snapshot().owner).toBeNull();
    expect(controller.snapshot().events).toEqual([]);
  });

  it("publishes stable external-store snapshots and stops after unsubscribe", async () => {
    // controller owns one running tool whose same-seq terminal evolution is observable.
    // controller 持有一条运行中工具，其同 seq 终态演进应当可观察。
    const controller = new ConversationEventWindowController(rpcMock({
      hydrate: async () => response(
        [summary(1, toolCall("running"), 10, 1)],
        { memory_revision: 1, durable_revision: 1 },
      ),
    }));
    // notifications records only complete published snapshot boundaries.
    // notifications 只记录完整发布的快照边界。
    const notifications: number[] = [];
    // initialSnapshot proves getSnapshot identity is stable before the first mutation.
    // initialSnapshot 证明首次变更前 getSnapshot 身份保持稳定。
    const initialSnapshot = controller.snapshot();
    const unsubscribe = controller.subscribe(() => notifications.push(controller.snapshot().memory_revision ?? -1));

    expect(controller.snapshot()).toBe(initialSnapshot);
    await controller.hydrate(SESSION_ID);
    const hydratedSnapshot = controller.snapshot();
    expect(hydratedSnapshot).not.toBe(initialSnapshot);
    expect(controller.snapshot()).toBe(hydratedSnapshot);
    expect(notifications).toEqual([1]);

    // terminalPatch advances one document revision and one event revision together.
    // terminalPatch 同时推进一次文档修订与事件修订。
    const terminalPatch = liveDelta(
      [summary(1, toolCall("completed"), 20, 2)],
      { base_memory_revision: 1, revision: 2 },
    );
    controller.applyLive(terminalPatch);
    const terminalSnapshot = controller.snapshot();
    expect(notifications).toEqual([1, 2]);
    expect(terminalSnapshot.events[0].event).toMatchObject({ status: "completed" });

    controller.applyLive(terminalPatch);
    expect(controller.snapshot()).toBe(terminalSnapshot);
    expect(notifications).toEqual([1, 2]);

    unsubscribe();
    controller.applyLive(liveDelta([], {
      base_durable_revision: 1,
      durability: "durable",
      revision: 2,
    }));
    expect(controller.snapshot()).not.toBe(terminalSnapshot);
    expect(controller.snapshot().durable_revision).toBe(2);
    expect(notifications).toEqual([1, 2]);
  });

  it("publishes hydration and buffered live patches as one external-store boundary", async () => {
    // push receives live data after subscription but before durable hydration resolves.
    // push 在订阅完成但可靠 hydration 尚未解析时接收实时数据。
    let push: ((delta: ConversationEventLiveDeltaV1) => void) | null = null;
    // resolveHydration publishes the delayed durable baseline selected by the test.
    // resolveHydration 发布由测试选择的延迟可靠基线。
    let resolveHydration: ((value: ConversationEventHydrationV1Response) => void) | null = null;
    // hydration blocks the request while the simulated live patch is buffered.
    // hydration 在模拟实时 patch 被缓冲期间阻塞请求。
    const hydration = new Promise<ConversationEventHydrationV1Response>((resolve) => {
      resolveHydration = resolve;
    });
    const controller = new ConversationEventWindowController(rpcMock({
      hydrate: async () => hydration,
      subscribeLive: async (handler) => {
        push = handler;
        return () => undefined;
      },
    }));
    // observed stores the only snapshot visible to an external React subscriber.
    // observed 保存外部 React 订阅者可见的唯一快照。
    const observed: Array<{ revision: number | null; status: string | undefined }> = [];
    controller.subscribe(() => {
      const snapshot = controller.snapshot();
      const event = snapshot.events[0]?.event;
      observed.push({
        revision: snapshot.memory_revision,
        status: event?.kind === "tool_call" ? event.status : undefined,
      });
    });

    const hydrationTask = controller.hydrate(SESSION_ID);
    await vi.waitFor(() => expect(push).not.toBeNull());
    requireAssignedCallback(push, "live patch subscriber")(liveDelta(
      [summary(1, toolCall("completed"), 20, 2)],
      { base_memory_revision: 1, revision: 2 },
    ));
    requireAssignedCallback(resolveHydration, "hydration resolver")(response(
      [summary(1, toolCall("running"), 10, 1)],
      { memory_revision: 1, durable_revision: 1 },
    ));

    await hydrationTask;
    expect(observed).toEqual([{ revision: 2, status: "completed" }]);
  });

  it("releases a live subscription that resolves after controller disposal", async () => {
    // resolveSubscription controls the exact race between owner disposal and subscription setup.
    // resolveSubscription 控制归属释放与订阅建立之间的精确竞态。
    let resolveSubscription: ((unsubscribe: () => void) => void) | null = null;
    // subscription remains pending until the controller has already been disposed.
    // subscription 保持等待，直到控制器已经被释放。
    const subscription = new Promise<() => void>((resolve) => {
      resolveSubscription = resolve;
    });
    // release proves the late subscription cannot survive its production owner.
    // release 证明迟到订阅不能存活于其生产归属之后。
    const release = vi.fn();
    // controller starts hydration through the pending production live subscription.
    // controller 通过等待中的生产实时订阅启动 hydration。
    const controller = new ConversationEventWindowController(rpcMock({
      hydrate: async () => response([summary(1, userInput(1))]),
      subscribeLive: async () => subscription,
    }));

    const hydrationTask = controller.hydrate(SESSION_ID);
    controller.dispose();
    requireAssignedCallback(resolveSubscription, "subscription resolver")(release);

    await expect(hydrationTask).resolves.toBe(false);
    expect(release).toHaveBeenCalledTimes(1);
  });
});
