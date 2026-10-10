import type {
  DedicatedLockedSpanV1,
  DedicatedSessionProfileV1,
} from "@/service/conversation-event-window-client";
import type { ConversationEventSummary } from "@/service/conversation-event-window-store";
import type { ConversationEventWindowControllerSnapshot } from "@/service/conversation-event-window-controller";

/** Memory facts of the selected dedicated session, derived from its hydrated profile and live notes.
 * 选中专职会话的记忆事实，由已加载配置与实时通知共同派生。 */
export interface DedicatedMemorySelection {
  /** Exact session owning the facts.
   * 拥有这些事实的精确会话。 */
  sessionId: string;
  /** Exact history generation owning the facts.
   * 拥有这些事实的精确历史代次。 */
  generation: number;
  /** Tool access frozen at creation.
   * 创建时冻结的工具开放方式。 */
  toolAccess: DedicatedSessionProfileV1["tool_access"];
  /** Immutable base prompt, used to label the creation message.
   * 不可变基础提示词，用于标注创建消息。 */
  basePrompt: string;
  /** Ordered locked spans including lock/unlock notes that arrived after hydration.
   * 有序锁定区间，包含 hydration 之后到达的锁定/解锁通知。 */
  lockedSpans: DedicatedLockedSpanV1[];
  /** Turn currently open, which is always remembered while it runs.
   * 当前开放回合；运行期间始终保留记忆。 */
  openTurnId: number | null;
}

/** Extends the hydrated spans with lock/unlock notes newer than their last endpoint.
 * 用比最后端点更新的锁定/解锁通知扩展已加载区间。
 *
 * @param base spans captured at hydration; never mutated.
 *             hydration 时捕获的区间；不会被修改。
 * @param events loaded events in strict seq order.
 *               按严格 seq 顺序加载的事件。
 * @returns the effective ordered spans; only the last may stay open.
 *          生效的有序区间；只有最后一个可以保持开放。 */
export function dedicatedEffectiveLockedSpans(
  base: readonly DedicatedLockedSpanV1[],
  events: readonly ConversationEventSummary[],
): DedicatedLockedSpanV1[] {
  const spans = base.map((span) => ({ ...span }));
  const last = spans.at(-1);
  const lastEndpoint = last ? last.unlocked_at_seq ?? last.locked_at_seq : -1;
  for (const summary of events) {
    if (summary.seq <= lastEndpoint || summary.event.kind !== "display_note") continue;
    const open = spans.at(-1);
    if (summary.event.notice_kind === "memory_locked" && (!open || open.unlocked_at_seq !== null)) {
      spans.push({ locked_at_seq: summary.seq, unlocked_at_seq: null });
    } else if (summary.event.notice_kind === "memory_unlocked" && open && open.unlocked_at_seq === null) {
      open.unlocked_at_seq = summary.seq;
    }
  }
  return spans;
}

/** Reports whether memory is locked after the last effective span endpoint.
 * 判断最后一个生效区间端点之后记忆是否处于锁定状态。 */
export function dedicatedMemoryLocked(spans: readonly DedicatedLockedSpanV1[]): boolean {
  return spans.at(-1)?.unlocked_at_seq === null;
}

/** Reports whether a turn starting at `startSeq` is forgotten once it is no longer open.
 * 判断起点为 `startSeq` 的回合在不再开放后是否被遗忘。
 *
 * @param spans effective ordered locked spans.
 *              生效的有序锁定区间。
 * @param startSeq seq of the turn's first visible input.
 *                 回合首个可见输入的 seq。
 * @param open whether the turn is the currently open turn.
 *             该回合是否为当前开放回合。 */
export function dedicatedTurnForgotten(
  spans: readonly DedicatedLockedSpanV1[],
  startSeq: number,
  open: boolean,
): boolean {
  if (open) return false;
  return spans.some((span) => startSeq > span.locked_at_seq
    && (span.unlocked_at_seq === null || startSeq < span.unlocked_at_seq));
}

/** Selects dedicated memory facts; ordinary sessions and unhydrated windows select null.
 * 选择专职记忆事实；普通会话与尚未加载的窗口选择 null。
 * Compare results with `conversationSelectionEqual` so streaming deltas keep the previous value.
 * 使用 `conversationSelectionEqual` 比较结果，使流式增量沿用上一个值。 */
export function selectDedicatedMemory(
  snapshot: ConversationEventWindowControllerSnapshot,
): DedicatedMemorySelection | null {
  const profile = snapshot.session_config?.dedicated ?? null;
  if (snapshot.owner === null || profile === null) return null;
  return {
    sessionId: snapshot.owner.session_id,
    generation: snapshot.owner.window_generation,
    toolAccess: profile.tool_access,
    basePrompt: profile.base_prompt,
    lockedSpans: dedicatedEffectiveLockedSpans(profile.locked_spans, snapshot.events),
    openTurnId: snapshot.open_turn_id,
  };
}
