import type {
  AggregatedUsage,
  ConversationEventBody,
  ConversationEventSummary,
  StatBucket,
} from "./conversation-event-window-store";
import type { ConversationEventWindowControllerSnapshot } from "./conversation-event-window-controller";
import { selectConversationContinueCandidateV1 } from "./conversation-continuation-selector-v1";
import { USAGE_DIMENSIONS } from "./conversation-usage-coverage";

/** ApiStats body narrowed from the canonical event union.
 * 从规范事件联合类型收窄的 ApiStats 正文。 */
type ApiStatsBody = Extract<ConversationEventBody, { kind: "api_stats" }>;

/** Appends display-only statistics for a stopped Turn that can still be continued.
 * 为仍可继续的已停止 Turn 追加仅用于展示的统计。
 * A stopped Turn stays open so Continue can resume it, which means its persisted statistics only
 * arrive when it finally closes. Until then the Turn shows the requests it already made; a Continue
 * request consumes the candidate and the provisional row disappears with it.
 * 已停止的 Turn 保持开放以便继续执行，因此其持久化统计要到最终闭合时才出现。在此之前展示该 Turn
 * 已发出的请求；继续请求会消费候选，临时统计行随之消失。
 * Returns `snapshot.events` unchanged when there is no candidate, otherwise a new array whose last
 * element is a synthetic `turn_stats` summary that never reaches storage or RPC. */
export function withStoppedTurnStatsV1(
  snapshot: ConversationEventWindowControllerSnapshot,
): readonly ConversationEventSummary[] {
  const candidate = selectConversationContinueCandidateV1(snapshot);
  if (candidate === null) return snapshot.events;
  // apis keeps the newest revision of each physical request, as replayed windows may repeat it.
  // apis 保留每个物理请求的最新修订，因为重放窗口可能重复出现它。
  const apis = new Map<number, ConversationEventSummary & { event: ApiStatsBody }>();
  let toolCallCount = 0;
  let startSeq: number | null = null;
  let lastSeq = 0;
  let lastUpdatedMs = 0;
  for (const summary of snapshot.events) {
    lastSeq = Math.max(lastSeq, summary.seq);
    if (summary.turn_id !== candidate.open_turn_id) continue;
    // A Turn that already closed carries its own persisted statistics.
    // 已闭合的 Turn 自带持久化统计。
    if (summary.event.kind === "turn_stats") return snapshot.events;
    startSeq ??= summary.seq;
    lastUpdatedMs = Math.max(lastUpdatedMs, summary.updated_at_ms);
    if (summary.event.kind === "tool_call") toolCallCount += 1;
    if (summary.event.kind !== "api_stats") continue;
    const previous = apis.get(summary.event.api_call_id);
    if (!previous || summary.seq > previous.seq
      || (summary.seq === previous.seq && summary.event_revision >= previous.event_revision)) {
      apis.set(summary.event.api_call_id, summary as ConversationEventSummary & { event: ApiStatsBody });
    }
  }
  if (startSeq === null) return snapshot.events;
  const direct: StatBucket = {
    api_call_count: apis.size,
    unknown_usage_api_call_count: 0,
    tool_call_count: toolCallCount,
    subagent_return_count: 0,
    usage: {},
  };
  const usage: AggregatedUsage = {};
  for (const field of USAGE_DIMENSIONS) {
    let sum = 0;
    let known = apis.size > 0;
    for (const api of apis.values()) {
      const value = api.event.usage[field];
      if (value === undefined) {
        known = false;
        break;
      }
      sum += value;
    }
    // A dimension any request left unreported stays unknown, matching persisted buckets.
    // 任一请求未上报的维度保持未知，与持久化统计桶一致。
    if (known) usage[field] = sum;
  }
  for (const api of apis.values()) {
    if (USAGE_DIMENSIONS.every((field) => api.event.usage[field] === undefined)) {
      direct.unknown_usage_api_call_count += 1;
    }
  }
  direct.usage = usage;
  const empty: StatBucket = {
    api_call_count: 0,
    unknown_usage_api_call_count: 0,
    tool_call_count: 0,
    subagent_return_count: 0,
    usage: {},
  };
  const stopped: ConversationEventSummary = {
    // The next unused seq keeps the row after the Turn without colliding with a loaded event.
    // 下一个未使用的 seq 让该行位于 Turn 之后，且不与已加载事件冲突。
    seq: lastSeq + 1,
    turn_id: candidate.open_turn_id,
    created_at_ms: lastUpdatedMs,
    updated_at_ms: lastUpdatedMs,
    event_revision: 0,
    visibility: "ui_only",
    event: {
      kind: "turn_stats",
      start_seq: startSeq,
      end_seq: lastSeq,
      outcome: "stopped",
      buckets: { direct, subagent: empty, total: direct },
    },
  };
  return [...snapshot.events, stopped];
}
