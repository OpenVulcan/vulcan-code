import type {
  ConversationEventBody,
  ConversationEventSummary,
} from "./conversation-event-window-store";
import type { ConversationEventWindowControllerSnapshot } from "./conversation-event-window-controller";

/** One terminal ApiStats event that may authorize an explicit V1 Continue action.
 * 一条可能授权显式 V1 Continue 操作的终态 ApiStats 事件。 */
type ApiStatsSummary = ConversationEventSummary & {
  event: Extract<ConversationEventBody, { kind: "api_stats" }>;
};

/** UI-only identity of one exact continuation candidate in the current event generation.
 * 当前事件代际中一个精确 Continue 候选的纯 UI 身份。 */
export interface ConversationContinueCandidateV1 {
  /** Unambiguous local identity; it is never included in the service request.
   * 无歧义的本地身份；绝不包含在服务请求中。 */
  ui_identity: string;
  /** Session that owns the open Turn.
   * 持有开放 Turn 的会话。 */
  session_id: string;
  /** Edit generation that owns the loaded event window.
   * 持有已加载事件窗口的编辑代际。 */
  window_generation: number;
  /** Exact open logical Turn selected by the controller boundary.
   * 控制器边界选中的精确开放逻辑 Turn。 */
  open_turn_id: number;
  /** Physical API call whose terminal policy allows continuation.
   * 终态策略允许继续的物理 API 调用。 */
  api_call_id: number;
  /** Seq of the authoritative terminal ApiStats event.
   * 权威终态 ApiStats 事件的 seq。 */
  api_stats_seq: number;
  /** In-place revision of the authoritative terminal ApiStats event.
   * 权威终态 ApiStats 事件的原位修订号。 */
  api_stats_event_revision: number;
}

/** Selects the only continuation candidate authorized by the latest physical API boundary.
 * 选择最后一个物理 API 边界授权的唯一 Continue 候选。
 *
 * @param snapshot complete controller window including HostOnly events.
 *                 包含 HostOnly 事件的完整控制器窗口。
 * @returns one UI-only candidate, or null when the open Turn is not claimable.
 *          一条纯 UI 候选；开放 Turn 不可领取时返回 null。 */
export function selectConversationContinueCandidateV1(
  snapshot: ConversationEventWindowControllerSnapshot,
): ConversationContinueCandidateV1 | null {
  // owner and openTurnId are the only authoritative session/Turn boundaries.
  // owner 与 openTurnId 是唯一权威的会话/Turn 边界。
  const owner = snapshot.owner;
  const openTurnId = snapshot.open_turn_id;
  if (owner === null || openTurnId === null) return null;

  // latestApiCallId tracks only the last physical ApiStarted boundary in the open Turn.
  // latestApiCallId 只跟踪开放 Turn 中最后一个物理 ApiStarted 边界。
  let latestApiCallId: number | null = null;
  // terminalStats belongs only to latestApiCallId and is reset by every newer ApiStarted.
  // terminalStats 只属于 latestApiCallId，并在每个更新 ApiStarted 出现时重置。
  let terminalStats: ApiStatsSummary | null = null;
  for (const summary of snapshot.events) {
    if (summary.turn_id !== openTurnId) continue;
    if (summary.event.kind === "api_started") {
      latestApiCallId = summary.event.api_call_id;
      terminalStats = null;
      continue;
    }
    if (
      summary.event.kind === "api_stats"
      && latestApiCallId !== null
      && summary.event.api_call_id === latestApiCallId
    ) {
      terminalStats = summary as ApiStatsSummary;
    }
  }

  if (
    terminalStats === null
    || terminalStats.event.continuation_eligibility !== "allowed"
    || !isTerminalApiStatsStatus(terminalStats.event.status)
  ) {
    return null;
  }

  // A later continuation control consumes the visible candidate before the next API starts.
  // 后续 Continue 控制事件会在下一次 API 启动前消费当前可见候选。
  for (const summary of snapshot.events) {
    if (summary.seq <= terminalStats.seq || summary.turn_id !== openTurnId) continue;
    if (summary.event.kind === "api_started") return null;
    if (
      summary.event.kind === "execution_control"
      && (
        summary.event.control_kind === "turn_continue_requested"
        || summary.event.control_kind === "turn_resumed"
      )
    ) {
      return null;
    }
  }

  // uiIdentity is an unambiguous JSON tuple and never becomes transport authorization.
  // uiIdentity 是无歧义 JSON 元组，绝不会成为传输授权信息。
  const uiIdentity = JSON.stringify([
    owner.session_id,
    owner.window_generation,
    openTurnId,
    terminalStats.seq,
    terminalStats.event_revision,
  ]);
  return {
    ui_identity: uiIdentity,
    session_id: owner.session_id,
    window_generation: owner.window_generation,
    open_turn_id: openTurnId,
    api_call_id: terminalStats.event.api_call_id,
    api_stats_seq: terminalStats.seq,
    api_stats_event_revision: terminalStats.event_revision,
  };
}

/** Returns whether one ApiStats status is terminal rather than an in-flight snapshot.
 * 判断一条 ApiStats 状态是否为终态而不是进行中快照。 */
function isTerminalApiStatsStatus(
  status: Extract<ConversationEventBody, { kind: "api_stats" }>["status"],
): boolean {
  return status === "completed"
    || status === "failed"
    || status === "cancelled"
    || status === "interrupted";
}
