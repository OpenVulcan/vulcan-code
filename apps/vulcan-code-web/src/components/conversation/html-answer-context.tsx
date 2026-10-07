import * as React from "react";
import type { ConversationEventWindowControllerSnapshot } from "@/service/conversation-event-window-controller";
import type { HtmlAnswerEventProjection } from "@/service/html-answer-projections";
import type { HtmlAnswerSourceIdentity } from "@/service/html-answers-service";
import type { ConversationEventSummary } from "@/service/conversation-event-window-store";

/** Display context for one exact provider-visible assistant body, never a tool or user sample.
 * 一条精确供应商可见助手正文的展示上下文，绝不用于工具或用户示例。 */
export interface HtmlAnswerMessageContext {
  /** Owner coordinates shared by every result in this event.
   * 当前事件中全部结果共用的归属坐标。 */
  source: Omit<HtmlAnswerSourceIdentity, "artifactId">;
  /** Durable Core annotations; absent means no execution authority yet.
   * Core 可靠标注；缺失表示尚无执行权威。 */
  projection: HtmlAnswerEventProjection | undefined;
}

/** Exact message lookup scoped to the mounted branch; missing entries preserve ordinary Markdown.
 * 限于已挂载分支的精确消息查表；没有条目时保留普通 Markdown。 */
const HtmlAnswerWindowContext = React.createContext<ReadonlyMap<string, HtmlAnswerMessageContext>>(new Map());

/** Publish eligible canonical events and annotations without modifying the persisted message model.
 * 发布符合条件的权威事件及标注，不修改持久消息模型。
 * Inputs are the atomic window and explicit parent; returns a branch-scoped context provider.
 * 输入为原子窗口及显式父会话；返回分支范围上下文提供器。 */
export function HtmlAnswerWindowProvider(props: {
  /** Current atomic window belonging to sessionId.
   * 属于 sessionId 的当前原子窗口。 */
  snapshot: ConversationEventWindowControllerSnapshot;
  sessionId: string;
  /** Embedded parent supplied by the same owner used to hydrate the window.
   * 与加载窗口相同的所有者提供的嵌入父会话。 */
  parentSessionId: string | null;
  /** Existing conversation surface.
   * 既有会话界面。 */
  children: React.ReactNode;
}) {
  const { snapshot, sessionId, parentSessionId } = props;
  // Canonical events are immutable. Reuse unchanged entries so unrelated deltas cannot remount viewers.
  // 权威事件不可变。复用未变条目，防止无关增量重新挂载查看器。
  const cached = React.useMemo(() => new WeakMap<ConversationEventSummary, HtmlAnswerMessageContext | null>(),
    [sessionId, parentSessionId, snapshot.owner?.window_generation]);
  const value = React.useMemo(() => {
    const entries = new Map<string, HtmlAnswerMessageContext>();
    const owner = snapshot.owner;
    if (owner?.session_id !== sessionId) return entries;
    const projections = new Map(snapshot.html_answer_projections?.map((entry) => [entry.seq, entry]));
    for (const summary of snapshot.events) {
      const previous = cached.get(summary);
      if (previous === null) continue;
      const candidate = projections.get(summary.seq);
      const projection = candidate?.event_revision === summary.event_revision ? candidate : undefined;
      if (previous && previous.projection === projection) {
        entries.set(`v1:${summary.seq}`, previous);
        continue;
      }
      if (summary.visibility !== "provider" || summary.event.kind !== "assistant_text"
        || !summary.event.text.includes("vulcan-html")) { cached.set(summary, null); continue; }
      const entry = {
        source: { sessionId, parentSessionId, windowGeneration: owner.window_generation,
          eventSeq: summary.seq, eventRevision: summary.event_revision },
        projection,
      };
      cached.set(summary, entry);
      entries.set(`v1:${summary.seq}`, entry);
    }
    return entries;
  }, [snapshot.events, snapshot.html_answer_projections, snapshot.owner, sessionId, parentSessionId, cached]);
  return <HtmlAnswerWindowContext.Provider value={value}>{props.children}</HtmlAnswerWindowContext.Provider>;
}

/** Resolve only the exact canonical message ID; missing IDs cannot create result cards.
 * 仅解析精确权威消息标识；没有标识时不能创建结果卡。 */
export function useHtmlAnswerMessage(messageId: string | undefined): HtmlAnswerMessageContext | undefined {
  return React.useContext(HtmlAnswerWindowContext).get(messageId ?? "");
}
