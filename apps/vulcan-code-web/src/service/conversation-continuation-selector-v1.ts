import type {
  ApiPurposeKind,
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

/** Selects the continuation candidate authorized by the latest business API after all physical work stops.
 * 全部物理请求停止后，选择最近业务 API 授权的 Continue 候选。
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

  // latestApiCallId includes auxiliaries because every active physical request remains exclusive.
  // latestApiCallId 包含辅助请求，因为每个活动物理请求都仍保持排他。
  let latestApiCallId: number | null = null;
  // latestApiIsTerminal prevents a business candidate from bypassing an active auxiliary request.
  // latestApiIsTerminal 防止业务候选绕过仍活动的辅助请求。
  let latestApiIsTerminal = false;
  // latestBusinessApiCallId mirrors Core: auxiliary requests cannot grant or revoke business Continue.
  // latestBusinessApiCallId 对齐 Core：辅助请求不能授予或撤销业务 Continue。
  let latestBusinessApiCallId: number | null = null;
  // terminalStats retains only the exact latest business terminal, never an auxiliary terminal.
  // terminalStats 只保留精确的最近业务终态，不采用辅助终态。
  let terminalStats: ApiStatsSummary | null = null;
  for (const summary of snapshot.events) {
    if (summary.turn_id !== openTurnId) continue;
    if (summary.event.kind === "api_started") {
      latestApiCallId = summary.event.api_call_id;
      latestApiIsTerminal = false;
      if (isBusinessApiPurpose(summary.event.purpose)) {
        latestBusinessApiCallId = summary.event.api_call_id;
        terminalStats = null;
      }
      continue;
    }
    if (summary.event.kind === "api_stats") {
      if (summary.event.api_call_id === latestApiCallId) {
        latestApiIsTerminal = isTerminalApiStatsStatus(summary.event.status);
      }
      if (summary.event.api_call_id === latestBusinessApiCallId) {
        terminalStats = summary as ApiStatsSummary;
      }
    }
  }

  if (
    !latestApiIsTerminal
    || terminalStats === null
    || terminalStats.event.continuation_eligibility !== "allowed"
    || !isTerminalApiStatsStatus(terminalStats.event.status)
  ) {
    return null;
  }

  // Auxiliary work does not supersede the business boundary or undo an admitted Continue action.
  // 辅助工作不会替代业务边界，也不能撤销已准入的 Continue 动作。
  for (const summary of snapshot.events) {
    if (summary.seq <= terminalStats.seq || summary.turn_id !== openTurnId) continue;
    if (summary.event.kind === "api_started" && isBusinessApiPurpose(summary.event.purpose)) return null;
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

/** Classifies the canonical purpose using the same business boundary as Session Core.
 * 使用与 Session Core 相同的业务边界划分规范请求用途。
 * @param purpose the declared purpose from ApiStarted, never inferred from status or model.
 *                ApiStarted 声明的用途，绝不从状态或模型推断。
 * @returns false for compaction and question execution, which cannot authorize business Continue.
 *          压缩与答疑返回 false，因为它们不能授权业务 Continue。 */
function isBusinessApiPurpose(purpose: ApiPurposeKind): boolean {
  return purpose !== "compaction" && purpose !== "question_execution";
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
