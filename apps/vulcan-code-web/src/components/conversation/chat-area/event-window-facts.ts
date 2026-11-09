import React from "react";
import type { ConversationBlock } from "@/components/conversation/conversation-model";
import { activeThinkingOverlayBlock, type ThinkingDisplayMode } from "@/components/conversation/thinking-display";
import type { Translate } from "@/i18n";
import { selectConversationContinueCandidateV1 } from "@/service/conversation-continuation-selector-v1";
import type { ConversationEventWindowControllerSnapshot } from "@/service/conversation-event-window-controller";
import { isTerminalApiStatsStatus, type ConversationEventSummary } from "@/service/conversation-event-window-store";
import { selectConversationSessionTodolistV1 } from "@/service/conversation-session-tools-v1";
import { conversationSelectionEqual, useSelectedConversationEventWindowSelector } from "@/state/conversation-event-window-react";

/** Inputs that scope the facts ChatArea reads from the selected event window.
 * 限定 ChatArea 从选中事件窗口读取哪些事实的输入。 */
export interface ChatAreaEventWindowFactsOptions {
  /** Selected durable session, or undefined for a draft surface.
   * 选中的持久会话；草稿界面为 undefined。 */
  sessionId: string | undefined;
  /** Browser identity of the optimistic first prompt still awaiting its canonical record.
   * 仍在等待规范记录的乐观首条提示词的浏览器身份。 */
  pendingClientSubmissionId: string | null;
  /** Effective reasoning display mode; only the popup mode reads the live reasoning row.
   * 生效的思考展示模式；只有弹窗模式读取实时思考行。 */
  thinkingDisplayMode: ThinkingDisplayMode;
  /** Resolves the popup's display labels.
   * 解析弹窗展示文案。 */
  t: Translate;
}

/** Reads the selected event-window facts ChatArea renders, each through its own selector.
 * 通过各自的选择器读取 ChatArea 渲染所需的选中事件窗口事实。
 *
 * ChatArea never subscribes to the whole snapshot: a reply streams one publication per frame, and every fact below
 * keeps its value while text is only appended, so the chat surface, composer and overlays stay idle and only the
 * transcript re-renders. The popup reasoning block is the one fact that follows each reasoning delta.
 * ChatArea 绝不订阅整个快照：回复流式输出时每帧发布一次，而下列事实在只追加文本时保持不变，因此会话界面、输入区与浮层
 * 保持静止，只有转录重新渲染。弹窗思考块是唯一随每个思考增量变化的事实。
 *
 * @param options selected session, pending submission, display mode and translator.
 *                选中会话、待对账提交、展示模式与翻译函数。
 * @returns owner facts, transcript structure, live popup block and composer inputs.
 *          归属事实、转录结构、实时弹窗块与输入区输入。 */
export function useChatAreaEventWindowFacts(options: ChatAreaEventWindowFactsOptions) {
  const { sessionId, pendingClientSubmissionId, thinkingDisplayMode, t } = options;
  const owner = useSelectedConversationEventWindowSelector(selectOwner, conversationSelectionEqual);
  // ownerReady permits rendering only when the selected durable session owns the V1 snapshot.
  // ownerReady 仅在选中的持久会话拥有 V1 快照时允许渲染。
  const ownerReady = sessionId !== undefined && owner?.session_id === sessionId;
  const streamSignature = useSelectedConversationEventWindowSelector(buildConversationEventWindowStreamSignature);
  const hasVisibleTranscript = useSelectedConversationEventWindowSelector(selectHasVisibleTranscript);
  const submissionCorrelated = useSelectedConversationEventWindowSelector(React.useCallback(
    (snapshot: ConversationEventWindowControllerSnapshot) => pendingClientSubmissionId !== null
      && eventWindowHasAuthoritativeSubmissionCorrelation(snapshot.events, pendingClientSubmissionId),
    [pendingClientSubmissionId],
  ));
  const streamingReasoningBlock = useSelectedConversationEventWindowSelector(React.useCallback(
    (snapshot: ConversationEventWindowControllerSnapshot) => activeThinkingOverlayBlock(
      thinkingDisplayMode,
      latestStreamingReasoningBlock(snapshot.events, t),
    ),
    [t, thinkingDisplayMode],
  ), sameBlockRevision);
  // sessionTodolist is projected from the same durable V1 generation as the visible transcript.
  // sessionTodolist 从与可见转录相同的可靠 V1 代次投影。
  const sessionTodolist = useSelectedConversationEventWindowSelector(React.useCallback(
    (snapshot: ConversationEventWindowControllerSnapshot) => snapshot.owner?.session_id === sessionId
      ? selectConversationSessionTodolistV1(snapshot)
      : null,
    [sessionId],
  ), conversationSelectionEqual);
  // continuationCandidate identifies the latest eligible physical API in the selected V1 open Turn.
  // continuationCandidate 标识选中 V1 开放 Turn 中最近的可继续物理 API。
  const continuationCandidate = useSelectedConversationEventWindowSelector(
    selectConversationContinueCandidateV1,
    conversationSelectionEqual,
  );
  return {
    owner,
    ownerReady,
    streamSignature,
    hasVisibleTranscript: ownerReady && hasVisibleTranscript,
    submissionCorrelated: ownerReady && submissionCorrelated,
    streamingReasoningBlock: ownerReady ? streamingReasoningBlock : null,
    sessionTodolist,
    continuationCandidate,
  };
}

/** Returns the snapshot owner. 返回快照归属。 */
function selectOwner(snapshot: ConversationEventWindowControllerSnapshot) {
  return snapshot.owner;
}

/** Reports whether the snapshot holds visible transcript content. 判断快照是否包含可见转录内容。 */
function selectHasVisibleTranscript(snapshot: ConversationEventWindowControllerSnapshot): boolean {
  return eventWindowHasVisibleTranscript(snapshot.events);
}

/** Treats two popup blocks as the same while they show the same event revision.
 * 两个弹窗块展示同一事件修订时视为相同。 */
function sameBlockRevision(left: ConversationBlock | null, right: ConversationBlock | null): boolean {
  return left?.id === right?.id;
}

/** Reports whether one V1 UserInput already owns the browser submission identity.
 * 判断一条 V1 UserInput 是否已经拥有浏览器提交身份。
 *
 * @param events canonical selected-session events.
 *               当前选中会话的 canonical 事件。
 * @param clientActionId exact browser idempotency identity.
 *                       精确的浏览器幂等身份。
 * @returns true only for one exact persisted UserInput match.
 *          仅在存在精确持久化 UserInput 匹配时返回 true。 */
function eventWindowHasAuthoritativeSubmissionCorrelation(
  events: readonly ConversationEventSummary[],
  clientActionId: string,
): boolean {
  return events.some((event) => (
    event.event.kind === "user_input"
    && event.event.client_action_id === clientActionId
  ));
}

/** Reports whether a canonical V1 window contains visible transcript content.
 * 判断一个 canonical V1 窗口是否包含可见转录内容。
 *
 * @param events canonical selected-session events.
 *               当前选中会话的 canonical 事件。
 * @returns true when at least one user, assistant, tool, or durable display fact exists.
 *          存在至少一条用户、assistant、工具或可靠显示事实时返回 true。 */
function eventWindowHasVisibleTranscript(events: readonly ConversationEventSummary[]): boolean {
  return events.some((summary) => {
    if (summary.visibility === "host_only") {
      return false;
    }
    switch (summary.event.kind) {
      case "user_input":
      case "assistant_reasoning":
      case "assistant_text":
      case "tool_call":
      case "display_note":
      case "request_config_changed":
      case "subagent_return":
        return true;
      default:
        return false;
    }
  });
}

/** Projects the latest actively streaming V1 reasoning event into the established popup shell.
 * 将最新活动流式 V1 思考事件投影到既有浮层外壳。
 *
 * @param events canonical selected-session events.
 *               当前选中会话的 canonical 事件。
 * @param t Resolves the popup's display labels.
 *          解析弹窗展示文案。
 * @returns one presentation-only block, or null when no reasoning stream is active.
 *          一条仅用于展示的块；没有活动思考流时返回 null。 */
function latestStreamingReasoningBlock(
  events: readonly ConversationEventSummary[],
  t: Translate,
): ConversationBlock | null {
  // A request or turn can close before the same-seq terminal reasoning update reaches this window.
  // 请求或回合可能已经结束，而同序号的思考终态更新尚未到达事件窗口。
  const terminalApiCallIds = new Set<number>();
  // Closed turn identities cover endings that do not include terminal stats for every request.
  // 已结束回合的身份覆盖未为每个请求提供终态统计的结束路径。
  const closedTurnIds = new Set<number>();
  for (let index = events.length - 1; index >= 0; index -= 1) {
    // summary is inspected in reverse only to select the latest fact; no display order is changed.
    // summary 仅为选择最新事实而逆向检查；不会改变任何显示顺序。
    const summary = events[index];
    // The newest physical request supersedes older requests, even before it starts reasoning.
    // 最新物理请求取代更早的请求，即使它还未开始输出思考。
    if (summary.event.kind === "api_started") return null;
    if (summary.event.kind === "api_stats" && isTerminalApiStatsStatus(summary.event.status)) {
      terminalApiCallIds.add(summary.event.api_call_id);
    }
    if (summary.event.kind === "turn_boundary" && summary.turn_id !== null) {
      closedTurnIds.add(summary.turn_id);
    }
    if (
      summary.event.kind !== "assistant_reasoning"
      || summary.event.status !== "streaming"
      || terminalApiCallIds.has(summary.event.api_call_id)
      || (summary.turn_id !== null && closedTurnIds.has(summary.turn_id))
    ) {
      continue;
    }
    return {
      id: `v1-reasoning:${summary.seq}:${summary.event_revision}`,
      kind: "thinking",
      title: t("conversation.block.thinking"),
      summary: t("conversation.block.thinkingActive"),
      body: summary.event.text,
      status: summary.event.status,
    };
  }
  return null;
}

/** Builds a structure-only V1 stream signature without reading cumulative event bodies.
 * 在不读取累计事件正文的情况下构建仅结构性的 V1 流签名。
 *
 * Text appended to a streaming reply or reasoning row is excluded: that row is the newest one, below any reader
 * anchor, and the transcript ResizeObserver already follows its growth while the tail is owned. Restoring on every
 * append forced a synchronous layout of the whole transcript once per frame. The frontier revisions are excluded for
 * the same reason; status changes and every other in-place revision still change the signature.
 * 流式回复或思考行追加的文本不计入签名：该行是最新一行，位于任何阅读锚点下方，且转录尺寸观察器已在持有尾部时跟随其增长。
 * 每次追加都恢复会让整个转录每帧强制同步布局一次。前沿修订号同理不计入；状态变化及其他原位修订仍会改变签名。
 *
 * @param snapshot atomic controller snapshot.
 *                 原子控制器快照。
 * @returns stable owner, window, and per-event structural identity.
 *          稳定的归属、窗口与逐事件结构身份。 */
function buildConversationEventWindowStreamSignature(
  snapshot: ConversationEventWindowControllerSnapshot,
): string {
  const eventRevisions = snapshot.events
    .map((event) => `${event.seq}:${streamingTextRow(event) ? "streaming" : event.event_revision}`)
    .join(",");
  return [
    snapshot.owner?.session_id ?? "none",
    snapshot.owner?.window_generation ?? "none",
    snapshot.has_more ? "more" : "complete",
    eventRevisions,
  ].join("|");
}

/** Reports whether a V1 row is reply or reasoning text that is still streaming.
 * 判断一条 V1 记录是否为仍在流式输出的回复或思考文本。
 *
 * @param summary canonical event row.
 *                规范事件记录。
 * @returns true only for assistant text or reasoning whose status is streaming.
 *          仅当回复文本或思考的状态为 streaming 时返回 true。 */
function streamingTextRow(summary: ConversationEventSummary): boolean {
  return (summary.event.kind === "assistant_text" || summary.event.kind === "assistant_reasoning")
    && summary.event.status === "streaming";
}
