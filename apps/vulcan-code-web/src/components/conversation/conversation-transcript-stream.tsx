import React from "react";
import type { ConversationEventWindowControllerSnapshot } from "@/service/conversation-event-window-controller";
import type { ConversationEventSummary } from "@/service/conversation-event-window-store";
import type { ConversationEventWindowSnapshotReader } from "@/state/conversation-event-window-react";
import { isFullTranscriptMarkdown, normalizeTranscriptMarkdown } from "./conversation-transcript-parser";

/** Exact event-window boundary shared only within one transcript surface.
 * 仅在一个转录界面内共享的精确事件窗口边界。 */
interface TranscriptStream {
  /** Coalesced canonical snapshot reader. 合并发布的规范快照读取器。 */
  reader: ConversationEventWindowSnapshotReader;
  /** Owner captured by the structural render. 结构渲染捕获的归属。 */
  owner: ConversationEventWindowControllerSnapshot["owner"];
  /** Service process epoch captured by the structural render. 结构渲染捕获的服务进程代次。 */
  serviceEpoch: string | null;
}

/** Provides the active Markdown leaves with the same reader used by their transcript.
 * 为活动 Markdown 叶子提供与所属转录相同的读取器。 */
export const ConversationTranscriptStreamContext = React.createContext<TranscriptStream | null>(null);

/** Keeps a transcript publication stable across body-only changes to an existing Markdown reply.
 * 既有 Markdown 回复仅变化正文时保留稳定的转录发布。
 * @param reader the canonical frame-coalesced window reader. 规范按帧合并窗口读取器。
 * @param running whether the surface renders active reply blocks. 界面是否渲染活动回复块。
 * @returns the latest snapshot needed to render transcript structure. 渲染转录结构所需的最新快照。 */
export function useConversationTranscriptSnapshot(
  reader: ConversationEventWindowSnapshotReader,
  running: boolean,
): ConversationEventWindowControllerSnapshot {
  // Compare consecutive publications so a long reply does not repeatedly compare against its initial text.
  // 比较连续发布，避免长回复反复对照最初的正文。
  const read = React.useMemo(() => {
    let observed = reader.getSnapshot();
    let visible = observed;
    return () => {
      const next = reader.getSnapshot();
      if (next !== observed) {
        if (!running || !sameTranscriptStructure(observed, next)) visible = next;
        observed = next;
      }
      return visible;
    };
  }, [reader, running]);
  return React.useSyncExternalStore(reader.subscribe, read, read);
}

/** Compares the visible boundaries and verifies every changed event is an isolated reply append.
 * 比较可见边界，并验证变化事件都只是可隔离的回复正文追加。
 * @param previous preceding canonical publication. 上一次规范发布。
 * @param next current canonical publication. 当前规范发布。
 * @returns true if the existing transcript projection still owns the same structure.
 *          当前转录投影仍表示相同结构时返回 true。 */
function sameTranscriptStructure(
  previous: ConversationEventWindowControllerSnapshot,
  next: ConversationEventWindowControllerSnapshot,
): boolean {
  if (previous.owner?.session_id !== next.owner?.session_id
    || previous.owner?.window_generation !== next.owner?.window_generation
    || previous.service_epoch !== next.service_epoch
    || previous.older_before_seq !== next.older_before_seq
    || previous.has_more !== next.has_more
    || previous.open_turn_id !== next.open_turn_id
    || previous.open_turn_start_seq !== next.open_turn_start_seq
    || previous.session_config !== next.session_config
    || previous.events.length !== next.events.length) return false;

  return next.events.every((event, index) => {
    const before = previous.events[index];
    if (event === before) return true;
    if (!isStreamingAnswerAppend(before, event) || event.event.kind !== "assistant_text") return false;
    // A compaction API owns a shared status card whose summary must follow the original projection.
    // 压缩 API 持有共享状态卡，其摘要继续沿原投影更新。
    const apiCallId = event.event.api_call_id;
    return !next.events.some(({event: body}) => (body.kind === "api_started" || body.kind === "api_stats")
      && body.purpose === "compaction" && body.api_call_id === apiCallId);
  });
}

/** Identifies a same-event append that preserves a complete ordinary Markdown block.
 * 识别保留完整普通 Markdown 块的同事件追加。
 * @param before previous canonical event. 前一份规范事件。
 * @param after current canonical event. 当前规范事件。
 * @returns true when the event's only display change is appended Markdown. 仅追加 Markdown 时返回 true。 */
function isStreamingAnswerAppend(before: ConversationEventSummary, after: ConversationEventSummary): boolean {
  const oldBody = before.event;
  const body = after.event;
  return oldBody.kind === "assistant_text" && body.kind === "assistant_text"
    && oldBody.status === "streaming" && body.status === "streaming"
    && before.seq === after.seq && before.turn_id === after.turn_id
    && before.visibility === after.visibility && before.created_at_ms === after.created_at_ms
    && oldBody.api_call_id === body.api_call_id && oldBody.provider_source === body.provider_source
    && oldBody.code_snapshots === body.code_snapshots
    && body.text.startsWith(oldBody.text)
    && isFullTranscriptMarkdown(normalizeTranscriptMarkdown(oldBody.text))
    && isFullTranscriptMarkdown(normalizeTranscriptMarkdown(body.text));
}

/** Reads the current text of one exact Markdown event without subscribing its ancestors.
 * 读取单个精确 Markdown 事件的当前正文，不让祖先组件参与订阅。
 * @param conversationId transcript's owning session. 转录所属会话。
 * @param seq canonical event sequence when the block owns the full reply. 整条回复对应的规范事件序号。
 * @param projectedBody text from the structural render. 结构渲染传入的正文。
 * @returns current normalized Markdown, or the projected body until the matching event is available.
 *          当前规范化 Markdown，匹配事件不可用时沿用投影正文。 */
export function useConversationStreamingAnswerText(
  conversationId: string,
  seq: number | undefined,
  projectedBody: string,
): string {
  const stream = React.useContext(ConversationTranscriptStreamContext);
  const reader = seq === undefined ? null : stream?.reader ?? null;
  const read = React.useCallback(() => {
    if (stream === null || seq === undefined) return projectedBody;
    const snapshot = stream.reader.getSnapshot();
    if (snapshot.owner?.session_id !== conversationId || stream.owner?.session_id !== conversationId
      || snapshot.owner.window_generation !== stream.owner.window_generation
      || snapshot.service_epoch !== stream.serviceEpoch) return projectedBody;
    // A contiguous canonical window gives a constant-time lookup without scanning historical events.
    // 连续的规范窗口允许按序号常数时间定位，无需扫描历史事件。
    const first = snapshot.events[0];
    const summary = first && snapshot.events[seq - first.seq];
    const event = summary?.event;
    if (summary?.seq !== seq || event?.kind !== "assistant_text" || event.status !== "streaming") return projectedBody;
    // Hold the last projected block until a newly structured reply is reconciled by its parent.
    // 正文变为结构化内容时保留原投影块，直到上层完成重新协调。
    const text = normalizeTranscriptMarkdown(event.text);
    return isFullTranscriptMarkdown(text) ? text : projectedBody;
  }, [conversationId, projectedBody, seq, stream]);
  return React.useSyncExternalStore(reader?.subscribe ?? subscribeInactive, read, read);
}

/** Provides the external-store unsubscribe contract for static Markdown.
 * 为静态 Markdown 提供外部 store 的取消订阅契约。
 * @returns an inert cleanup callback. 无操作的清理回调。 */
function subscribeInactive(): () => void {
  return () => undefined;
}
