import React from "react";
import { useConversationVisualTransitions } from "@/components/conversation/chat-area/visual-state";
import { StreamingThinkingOverlay } from "@/components/conversation/conversation-live-reasoning";
import type { ConversationBlock } from "@/components/conversation/conversation-model";
import { activeThinkingOverlayBlock, type ThinkingDisplayMode } from "@/components/conversation/thinking-display";
import { useI18n, type Translate } from "@/i18n";
import type { ConversationEventWindowControllerSnapshot } from "@/service/conversation-event-window-controller";
import { isTerminalApiStatsStatus } from "@/service/conversation-event-window-store";
import { useSelectedConversationEventWindowSelector } from "@/state/conversation-event-window-react";

// SelectedConversationThinkingOverlayProps binds the floating surface to the selected session and its display mode.
// SelectedConversationThinkingOverlayProps 将浮层绑定到选中会话及其展示模式。
interface SelectedConversationThinkingOverlayProps {
  // Exact session whose reasoning may appear in the overlay.
  // 允许在浮层中显示思考的精确会话。
  conversationId: string;
  // Effective session display preference.
  // 生效的会话展示偏好。
  mode: ThinkingDisplayMode;
  // Reports the visible session or null so the composer cue can hand off before paint.
  // 报告可见浮层的会话或 null，以便输入区提示在绘制前完成交接。
  onVisibilityChange: (conversationId: string | null) => void;
}

/** Subscribes to live reasoning and retains the last committed text for the overlay exit.
 * 订阅实时思考，并在浮层退场时保留最后一次提交的正文。
 * @param props the selected session, display mode, and callback for open or close transitions.
 *              选中会话、展示模式以及开关变化回调。
 * @returns the live or outgoing overlay, or null when it is closed.
 *          实时或正在退场的浮层，关闭时返回 null。 */
export const SelectedConversationThinkingOverlay = React.memo(function SelectedConversationThinkingOverlay({
  conversationId,
  mode,
  onVisibilityChange,
}: SelectedConversationThinkingOverlayProps) {
  // Resolve labels and subscribe to the text in the same leaf that renders them.
  // 由渲染正文的叶子组件解析文案并订阅正文。
  const { t } = useI18n();
  const displayedThinkingBlock = useSelectedConversationEventWindowSelector(React.useCallback(
    (snapshot: ConversationEventWindowControllerSnapshot) => activeThinkingOverlayBlock(
      mode,
      latestStreamingReasoningBlock(snapshot, conversationId, t),
    ),
    [conversationId, mode, t],
  ), sameBlockRevision);
  // Keep the last block and fade timer in this leaf so streaming and fading do not render ChatArea.
  // 将最后一个思考块和淡出计时留在此处，避免流式更新和淡出触发 ChatArea 渲染。
  const { visibleThinkingBlock, thinkingOverlayLeaving } = useConversationVisualTransitions({
    surfaceKey: conversationId,
    displayedThinkingBlock,
  });
  // A stable visibility flag keeps the composer cue suppressed until the overlay has finished leaving.
  // 通过稳定的可见标记使输入区提示在浮层完全退场前继续隐藏。
  const visible = visibleThinkingBlock !== null;
  React.useLayoutEffect(() => {
    if (!visible) return;
    onVisibilityChange(conversationId);
    return () => onVisibilityChange(null);
  }, [conversationId, onVisibilityChange, visible]);

  return visibleThinkingBlock ? (
    <StreamingThinkingOverlay block={visibleThinkingBlock} conversationId={conversationId} leaving={thinkingOverlayLeaving} />
  ) : null;
});

/** Checks whether both selections describe the same event revision of the same owner window.
 * 检查两次选择是否对应同一归属窗口的同一事件修订。
 * @param left previous popup block, or null when closed. 先前的浮层块，关闭时为 null。
 * @param right next popup block, or null when closed. 下一次浮层块，关闭时为 null。
 * @returns true when both are absent or their scoped revision is identical.
 *          两者均不存在或作用域内的修订相同时返回 true。 */
function sameBlockRevision(left: ConversationBlock | null, right: ConversationBlock | null): boolean {
  return left?.id === right?.id;
}

/** Projects the latest active V1 reasoning event belonging to the selected conversation.
 * 投影属于选中会话的最新活动 V1 思考事件。
 * @param snapshot the current event window. 当前事件窗口。
 * @param conversationId the session that may use this window. 可以使用此窗口的会话。
 * @param t resolves the popup's display labels. 解析浮层展示文案。
 * @returns one display block, or null when no matching reasoning stream is active.
 *          一条展示块；没有匹配的活动思考流时返回 null。 */
function latestStreamingReasoningBlock(
  snapshot: ConversationEventWindowControllerSnapshot,
  conversationId: string,
  t: Translate,
): ConversationBlock | null {
  // Verify the snapshot's owner before reading any reasoning from it.
  // 读取思考前先核对快照归属。
  const { events, owner } = snapshot;
  if (owner?.session_id !== conversationId) return null;
  // A request or turn can close before the same-seq terminal reasoning update reaches this window.
  // 请求或回合可能已经结束，而同序号的思考终态更新尚未到达事件窗口。
  const terminalApiCallIds = new Set<number>();
  // Closed turn identities cover endings that do not include terminal stats for every request.
  // 已结束回合的身份覆盖未为每个请求提供终态统计的结束路径。
  const closedTurnIds = new Set<number>();
  // Scan backwards so the latest physical request owns the overlay, even before it starts reasoning.
  // 从后往前查找，让最新的物理请求拥有浮层，即使它还未开始思考。
  for (let index = events.length - 1; index >= 0; index -= 1) {
    // Read the canonical event without changing its position in the transcript.
    // 读取规范事件，不改变它在转录中的位置。
    const summary = events[index];
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
    // Compaction reasoning is displayed by the compaction surface and must not enter this popup's exit timer.
    // 压缩思考由压缩浮层展示，不能进入普通思考浮层的退场计时。
    const apiCallId = summary.event.api_call_id;
    if (events.some(({ event }) => (event.kind === "api_started" || event.kind === "api_stats")
      && event.api_call_id === apiCallId && event.purpose === "compaction")) {
      return null;
    }
    // Sequence and revision can repeat across sessions and regenerated windows, so include both scopes.
    // 序号与修订号可在不同会话和重生成窗口中重复，因此包含两层归属。
    return {
      id: `v1-reasoning:${owner.session_id}:${owner.window_generation}:${summary.seq}:${summary.event_revision}`,
      kind: "thinking",
      title: t("conversation.block.thinking"),
      summary: t("conversation.block.thinkingActive"),
      body: summary.event.text,
      status: summary.event.status,
    };
  }
  return null;
}
