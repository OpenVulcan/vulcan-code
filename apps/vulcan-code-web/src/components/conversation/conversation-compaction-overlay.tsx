import { ScrollText } from "lucide-react";

import { MarkdownMessage } from "@/components/conversation/conversation-markdown";
import { useStreamingThinkingViewport } from "@/components/conversation/use-streaming-thinking-viewport";
import { useI18n } from "@/i18n";
import { isTerminalApiStatsStatus, type ConversationEventSummary } from "@/service/conversation-event-window-store";
import { useConversationContextCompactionPreview } from "@/state/conversation-live-stream-context";

/** The current physical compaction request and the text already present in its event window.
 * 当前实际发出的压缩请求，以及其事件窗口中已有的文本。 */
export interface ActiveConversationCompaction {
  /** Request identifier used to subscribe to its summary stream.
   * 用于订阅该请求摘要流的标识。 */
  apiCallId: number;
  /** Reasoning before the summary becomes available.
   * 摘要可用之前的思考内容。 */
  reasoning: string;
  /** Summary already projected into the event window.
   * 已投影到事件窗口的摘要。 */
  summary: string;
}

/** Reads the latest compaction request in the current event window.
 * 从当前事件窗口读取最新的压缩请求。
 * @param events chronological events of the current session. 当前会话按时间顺序排列的事件。
 * @returns the active compaction, or null after settlement or a subsequent request. 活动压缩，已结束或有后续请求时返回 null。 */
export function activeConversationCompaction(events: readonly ConversationEventSummary[]): ActiveConversationCompaction | null {
  // Stop at the latest request boundary so a retry or resumed answer cannot re-open an older popup.
  // 在最近一次请求的边界停止，防止重试或恢复回答重新打开较早的浮层。
  for (let index = events.length - 1; index >= 0; index -= 1) {
    // The start event is the authority for the request purpose and owning turn.
    // 开始事件是请求用途及所属回合的权威依据。
    const started = events[index];
    if (started.event.kind !== "api_started") continue;
    if (started.event.purpose !== "compaction") return null;

    // Match facts by request ID and turn closures by their own turn ID.
    // 请求事实按请求标识匹配，回合结束按其回合标识匹配。
    const apiCallId = started.event.api_call_id;
    let reasoning = "";
    let summary = "";
    for (let next = index + 1; next < events.length; next += 1) {
      // Text events contain cumulative content for their respective streams.
      // 文本事件包含各自流的累计内容。
      const { event, turn_id: turnId } = events[next];
      if (event.kind === "turn_boundary" && turnId !== null && turnId === started.turn_id) return null;
      if (!("api_call_id" in event) || event.api_call_id !== apiCallId) continue;
      if (event.kind === "api_stats" && isTerminalApiStatsStatus(event.status)) return null;
      if (event.kind === "assistant_reasoning") reasoning = event.text;
      if (event.kind === "assistant_text") summary = event.text;
    }
    return { apiCallId, reasoning, summary };
  }
  return null;
}

/** Shows the summary stream in a centered, non-modal surface with its own scroll viewport.
 * 在拥有独立滚动视口的居中非模态浮层内显示摘要流。
 * @param props the owning session and current compaction request. 所属会话及当前压缩请求。
 * @returns a floating status and its live content. 浮动状态及其实时内容。 */
export function ConversationCompactionOverlay(props: {
  /** Exact session owning the stream. 流所属的精确会话。 */
  conversationId: string;
  /** Active request selected from the session's canonical events. 从该会话规范事件选出的活动请求。 */
  compaction: ActiveConversationCompaction;
}) {
  // Reuse the reasoning surface's independent scroll intent, including manual reading above the tail.
  // 复用思考浮层的独立滚动意图，包括手动上滚阅读。
  const viewport = useStreamingThinkingViewport({ enabled: true });
  const { t } = useI18n();
  // The durable summary is authoritative once it reaches the window.
  // 持久化摘要到达事件窗口后以它为准。
  const preview = useConversationContextCompactionPreview(props.conversationId, props.compaction.apiCallId);
  const body = props.compaction.summary || preview || props.compaction.reasoning;

  return (
    <section className="vk-think-float vk-compaction-float" data-dock="center" aria-label={t("contextCompaction.title.running")}>
      <header className="vk-think-float-head">
        <ScrollText aria-hidden="true" />
        <b role="status">{t("contextCompaction.title.running")}</b>
      </header>
      <div
        className="vk-think-float-body"
        onPointerDown={viewport.handleBodyPointerDown}
        onScroll={viewport.handleBodyScroll}
        onWheel={viewport.handleBodyWheel}
        ref={viewport.bodyRef}
        tabIndex={0}
      >
        <div className="vk-think-float-content" ref={viewport.contentRef}>
          {body ? <MarkdownMessage content={body} eager streaming /> : t("contextCompaction.generating")}
        </div>
      </div>
    </section>
  );
}
