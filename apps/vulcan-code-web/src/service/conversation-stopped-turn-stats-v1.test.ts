import { describe, expect, it } from "vitest";

import type { ConversationEventWindowControllerSnapshot } from "./conversation-event-window-controller";
import type {
  ConversationEventBody,
  ConversationEventSummary,
  UsageSnapshot,
} from "./conversation-event-window-store";
import { withStoppedTurnStatsV1 } from "./conversation-stopped-turn-stats-v1";

/** Stable open Turn used by every fixture.
 * 每个夹具使用的稳定开放 Turn。 */
const OPEN_TURN_ID = 7;

/** Builds one canonical event summary of the open Turn.
 * 构建一条开放 Turn 的规范事件摘要。 */
function summary(seq: number, event: ConversationEventBody, eventRevision = 1): ConversationEventSummary {
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

/** Builds one physical API start boundary.
 * 构建一条物理 API 起始边界。 */
function apiStarted(apiCallId: number): ConversationEventBody {
  return {
    kind: "api_started",
    api_call_id: apiCallId,
    purpose: "main_generation",
    request: { provider: "provider", model: "model" },
  };
}

/** Builds one terminal API statistic that the user stopped.
 * 构建一条由用户停止的终态 API 统计。 */
function apiStats(apiCallId: number, usage: UsageSnapshot): ConversationEventBody {
  return {
    kind: "api_stats",
    api_call_id: apiCallId,
    purpose: "main_generation",
    provider: "provider",
    model: "model",
    usage,
    status: "interrupted",
    end_reason: "user_interrupted",
    continuation_eligibility: "allowed",
    timing: { completed_at_ms: 20 },
  };
}

/** Builds one finished tool call issued by the given request.
 * 构建一条由指定请求发出的已完成工具调用。 */
function toolCall(apiCallId: number, toolCallId: number): ConversationEventBody {
  return {
    kind: "tool_call",
    api_call_id: apiCallId,
    tool_call_id: toolCallId,
    tool_name: "read",
    input: {},
    status: "completed",
    request_stream_id: "stream",
  } as ConversationEventBody;
}

/** Builds one controller snapshot whose open Turn is the fixture Turn.
 * 构建一份开放 Turn 为夹具 Turn 的控制器快照。 */
function snapshot(events: readonly ConversationEventSummary[]): ConversationEventWindowControllerSnapshot {
  return {
    owner: { session_id: "session-stopped-v1", window_generation: 1 },
    service_epoch: "service-stopped-v1",
    memory_revision: 1,
    durable_revision: 1,
    events,
    older_before_seq: null,
    has_more: false,
    open_turn_id: OPEN_TURN_ID,
    open_turn_start_seq: 1,
    session_config: null,
  };
}

describe("stopped Turn statistics", () => {
  it("summarizes the requests a continuable stopped Turn already made", () => {
    const events = [
      summary(1, apiStarted(11)),
      summary(2, apiStats(11, { uncached_input_tokens: 100, cache_read_tokens: 900, output_tokens: 40 })),
      summary(3, toolCall(11, 21)),
      summary(4, apiStarted(12)),
      summary(5, apiStats(12, { uncached_input_tokens: 10, cache_read_tokens: 1000, output_tokens: 5 })),
    ];

    const presented = withStoppedTurnStatsV1(snapshot(events));

    expect(presented).toHaveLength(events.length + 1);
    const stopped = presented[presented.length - 1];
    expect(stopped.seq).toBeGreaterThan(5);
    expect(stopped.turn_id).toBe(OPEN_TURN_ID);
    expect(stopped.event).toMatchObject({
      kind: "turn_stats",
      start_seq: 1,
      end_seq: 5,
      outcome: "stopped",
      buckets: {
        total: {
          api_call_count: 2,
          tool_call_count: 1,
          usage: { uncached_input_tokens: 110, cache_read_tokens: 1900, output_tokens: 45 },
        },
      },
    });
    // A dimension one request left unreported is not summed from the others.
    // 某一请求未上报的维度不会由其他请求求和得出。
    expect(stopped.event.kind === "turn_stats" && stopped.event.buckets.total.usage.cache_write_tokens)
      .toBeUndefined();
  });

  it("leaves the window unchanged when nothing can be continued", () => {
    const events = [summary(1, apiStats(11, { output_tokens: 1 }))];
    const closed = { ...snapshot(events), open_turn_id: null, open_turn_start_seq: null };

    expect(withStoppedTurnStatsV1(closed)).toBe(closed.events);
  });
});
