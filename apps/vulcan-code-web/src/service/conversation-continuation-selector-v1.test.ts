import { describe, expect, it } from "vitest";

import { selectConversationContinueCandidateV1 } from "./conversation-continuation-selector-v1";
import type { ConversationEventWindowControllerSnapshot } from "./conversation-event-window-controller";
import type {
  ConversationEventBody,
  ConversationEventSummary,
} from "./conversation-event-window-store";

/** Stable open Turn used by every selector fixture.
 * 每个 selector 夹具使用的稳定开放 Turn。 */
const OPEN_TURN_ID = 7;

/** Strongly typed ApiStats body used by selector-only fixtures.
 * selector 专用夹具使用的强类型 ApiStats 事件体。 */
type ApiStatsEventBody = Extract<ConversationEventBody, { kind: "api_stats" }>;

/** Builds one canonical event summary in exact seq order.
 * 按精确 seq 顺序构建一条 canonical 事件摘要。 */
function summary(
  seq: number,
  event: ConversationEventBody,
  eventRevision = 1,
): ConversationEventSummary {
  return {
    seq,
    turn_id: OPEN_TURN_ID,
    created_at_ms: seq,
    updated_at_ms: seq,
    event_revision: eventRevision,
    visibility: event.kind === "api_stats" ? "ui_only" : "host_only",
    event,
  };
}

/** Builds one HostOnly physical API start boundary.
 * 构建一条 HostOnly 物理 API 起始边界。 */
function apiStarted(apiCallId: number): ConversationEventBody {
  return {
    kind: "api_started",
    api_call_id: apiCallId,
    purpose: "main_generation",
    request: { provider: "provider", model: "model" },
  };
}

/** Builds one typed API statistic with an explicit continuation policy.
 * 构建一条携带显式 Continue 策略的强类型 API 统计。 */
function apiStats(
  apiCallId: number,
  eligibility: "allowed" | "forbidden",
  status: ApiStatsEventBody["status"] = "interrupted",
  endReason: NonNullable<ApiStatsEventBody["end_reason"]> = status === "interrupted"
    ? "user_interrupted"
    : "completed_successfully",
): ConversationEventBody {
  return {
    kind: "api_stats",
    api_call_id: apiCallId,
    purpose: "main_generation",
    provider: "provider",
    model: "model",
    usage: {},
    status,
    end_reason: endReason,
    continuation_eligibility: eligibility,
    timing: { completed_at_ms: 20 },
  };
}

/** Builds one complete controller snapshot without consulting legacy summaries.
 * 在不查询旧摘要的情况下构建一份完整控制器快照。 */
function snapshot(
  events: readonly ConversationEventSummary[],
  options: {
    openTurnId?: number | null;
    windowGeneration?: number;
  } = {},
): ConversationEventWindowControllerSnapshot {
  return {
    owner: {
      session_id: "session-continue-v1",
      window_generation: options.windowGeneration ?? 1,
    },
    service_epoch: "service-continue-v1",
    memory_revision: 1,
    durable_revision: 1,
    events,
    older_before_seq: null,
    has_more: false,
    open_turn_id: options.openTurnId === undefined ? OPEN_TURN_ID : options.openTurnId,
    open_turn_start_seq: options.openTurnId === null ? null : 1,
    session_config: null,
  };
}

describe("V1 conversation continuation selector", () => {
  it("returns the latest terminal allowed ApiStats as one UI-only identity", () => {
    // controllerSnapshot contains one complete claimable physical API boundary.
    // controllerSnapshot 包含一个完整可领取的物理 API 边界。
    const controllerSnapshot = snapshot([
      summary(1, apiStarted(11)),
      summary(2, apiStats(11, "allowed"), 3),
    ]);

    expect(selectConversationContinueCandidateV1(controllerSnapshot)).toEqual({
      ui_identity: JSON.stringify(["session-continue-v1", 1, OPEN_TURN_ID, 2, 3]),
      session_id: "session-continue-v1",
      window_generation: 1,
      open_turn_id: OPEN_TURN_ID,
      api_call_id: 11,
      api_stats_seq: 2,
      api_stats_event_revision: 3,
    });
  });

  it("rejects forbidden, non-terminal, and missing-open-Turn snapshots", () => {
    // forbidden carries a terminal API policy that explicitly disallows Continue.
    // forbidden 携带明确禁止 Continue 的终态 API 策略。
    const forbidden = snapshot([
      summary(1, apiStarted(11)),
      summary(2, apiStats(11, "forbidden")),
    ]);
    // inFlight proves an allowed label cannot bypass the terminal-status requirement.
    // inFlight 证明 allowed 标签不能绕过终态状态要求。
    const inFlight = snapshot([
      summary(1, apiStarted(11)),
      summary(2, apiStats(11, "allowed", "in_progress")),
    ]);
    // closed has no authoritative open Turn even though old events remain loaded.
    // closed 虽然仍加载旧事件，但没有权威开放 Turn。
    const closed = snapshot(forbidden.events, { openTurnId: null });

    expect(selectConversationContinueCandidateV1(forbidden)).toBeNull();
    expect(selectConversationContinueCandidateV1(inFlight)).toBeNull();
    expect(selectConversationContinueCandidateV1(closed)).toBeNull();
  });

  it.each([
    ["allowed", true],
    ["forbidden", false],
  ] as const)(
    "uses persisted %s eligibility for a process-interrupted API instead of recomputing from reason",
    (eligibility, expectedCandidate) => {
      // crashed closes with the exact process-interrupted reason and an independently persisted policy.
      // crashed 使用精确的进程中断原因与独立持久化策略闭合。
      const crashed = snapshot([
        summary(1, apiStarted(11)),
        summary(2, apiStats(11, eligibility, "interrupted", "process_interrupted")),
      ]);

      expect(selectConversationContinueCandidateV1(crashed) !== null).toBe(expectedCandidate);
    },
  );

  it("uses only the last physical API boundary in the open Turn", () => {
    // awaitingLatest starts a newer physical API after an older allowed terminal boundary.
    // awaitingLatest 在旧 allowed 终态边界后启动更新的物理 API。
    const awaitingLatest = snapshot([
      summary(1, apiStarted(11)),
      summary(2, apiStats(11, "allowed")),
      summary(3, apiStarted(12)),
    ]);
    // forbiddenLatest closes the newer boundary with a forbidden policy.
    // forbiddenLatest 使用 forbidden 策略闭合更新边界。
    const forbiddenLatest = snapshot([
      ...awaitingLatest.events,
      summary(4, apiStats(12, "forbidden")),
    ]);
    // allowedLatest closes the newer boundary with the only eligible policy.
    // allowedLatest 使用唯一可领取策略闭合更新边界。
    const allowedLatest = snapshot([
      ...awaitingLatest.events,
      summary(4, apiStats(12, "allowed"), 2),
    ]);

    expect(selectConversationContinueCandidateV1(awaitingLatest)).toBeNull();
    expect(selectConversationContinueCandidateV1(forbiddenLatest)).toBeNull();
    expect(selectConversationContinueCandidateV1(allowedLatest)).toMatchObject({
      api_call_id: 12,
      api_stats_seq: 4,
      api_stats_event_revision: 2,
    });
  });

  it.each(["turn_continue_requested", "turn_resumed"] as const)(
    "consumes an allowed candidate after %s",
    (controlKind) => {
      // consumed appends the exact same-Turn execution control after terminal ApiStats.
      // consumed 在终态 ApiStats 后追加精确的同 Turn 执行控制。
      const consumed = snapshot([
        summary(1, apiStarted(11)),
        summary(2, apiStats(11, "allowed")),
        summary(3, { kind: "execution_control", control_kind: controlKind, api_call_id: 11 }),
      ]);

      expect(selectConversationContinueCandidateV1(consumed)).toBeNull();
    },
  );

  it("changes identity after edit generation and preserves it across reconnect reload", () => {
    // events represents the same durable open-Turn facts before and after reconnect.
    // events 表示重连前后相同的可靠开放 Turn 事实。
    const events = [
      summary(1, apiStarted(11)),
      summary(2, apiStats(11, "allowed"), 4),
    ];
    // original is the first hydrated owner generation.
    // original 是首次 hydration 的 owner 代际。
    const original = selectConversationContinueCandidateV1(snapshot(events));
    // reloaded is a byte-equivalent durable hydration in the same generation.
    // reloaded 是同一代际中按字节等价的可靠 hydration。
    const reloaded = selectConversationContinueCandidateV1(snapshot(structuredClone(events)));
    // edited belongs to a newer segment generation after explicit rebase.
    // edited 属于显式 rebase 后更新的 segment 代际。
    const edited = selectConversationContinueCandidateV1(snapshot(events, { windowGeneration: 2 }));

    expect(reloaded).toEqual(original);
    expect(edited?.ui_identity).not.toBe(original?.ui_identity);
    expect(edited?.window_generation).toBe(2);
  });
});
