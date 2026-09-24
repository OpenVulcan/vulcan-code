import type { ConversationEventSummary } from "./conversation-event-window-store";

/** Binds incoming task cards to their local execution and places local results before terminal statistics.
 * 将收到的任务卡绑定到本地执行回合，并把本地任务结果放在终态统计之前。
 * Accepts canonical history and returns display-only copies; stored sequence and hook inputs remain untouched.
 * 接收规范历史并返回仅用于展示的副本；存储序列与钩子输入保持不变。 */
export function projectCollaborationTurnLayout(events: readonly ConversationEventSummary[]): ConversationEventSummary[] {
  // Only target-owned effects can bind incoming messages; outbound messages belong to their sender's turn.
  // 仅目标所有的效果可以绑定收到的消息；发出消息归属于发送方自身回合。
  const incomingTurns = new Map<string, number>();
  // Queued acceptance may predate the previous turn's completion; only the claim starts its new turn.
  // 排队接纳可能早于上一回合结束；只有认领才开启其新回合。
  const claimSequences = new Map<string, number>();
  for (const summary of events) {
    if (summary.event.kind !== "collaboration" || summary.event.after_state.kind !== "task_message_effect") continue;
    const effect = summary.event.after_state.effect;
    if (effect.outcome.kind !== "failed") incomingTurns.set(effect.delivery_id, effect.outcome.turn_id);
    if (effect.outcome.kind === "claimed") claimSequences.set(effect.delivery_id, summary.seq);
  }
  // Result relocation is keyed by the executor's terminal turn, never by the most recent statistics row.
  // 结果定位使用执行者终态回合，绝不按最近统计行猜测。
  const resultsByTurn = new Map<number, ConversationEventSummary[]>();
  const statsTurns = new Set(events.filter((summary) => summary.event.kind === "turn_stats").map((summary) => summary.turn_id));
  const remaining: ConversationEventSummary[] = [];
  const claimedMessages = new Map<number, ConversationEventSummary[]>();
  for (const summary of events) {
    if (summary.event.kind === "collaboration") {
      const state = summary.event.after_state;
      if (state.kind === "inbound_task_message") {
        const deliveryId = state.acceptance.message.delivery_id;
        const turnId = incomingTurns.get(deliveryId) ?? state.acceptance.message.target.turn_id ?? null;
        const message = { ...summary, turn_id: turnId };
        const claimSeq = claimSequences.get(deliveryId);
        if (claimSeq !== undefined) {
          const messages = claimedMessages.get(claimSeq) ?? [];
          messages.push(message);
          claimedMessages.set(claimSeq, messages);
        } else remaining.push(message);
        continue;
      }
      if (state.kind === "task_result" && state.result.terminal !== null) {
        const turnId = state.result.terminal.turn_id;
        const result = { ...summary, turn_id: turnId };
        if (statsTurns.has(turnId)) {
          const results = resultsByTurn.get(turnId) ?? [];
          results.push(result);
          resultsByTurn.set(turnId, results);
        } else remaining.push(result);
        continue;
      }
    }
    remaining.push(summary);
  }
  return remaining.flatMap((summary) => [
    ...(claimedMessages.get(summary.seq) ?? []),
    ...(summary.event.kind === "turn_stats" && summary.turn_id !== null ? resultsByTurn.get(summary.turn_id) ?? [] : []),
    summary,
  ]);
}
