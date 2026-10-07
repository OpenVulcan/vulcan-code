import type { ConversationEventSummary } from "./conversation-event-window-store";

/** Exact Core source offsets; JavaScript uses the UTF-16 coordinates without reparsing HTML.
 * Core 精确源码偏移；JavaScript 使用 UTF-16 坐标，不重新解析 HTML。 */
export interface HtmlAnswerRange {
  /** UTF-8 start/end of the original interval.
   * 原始区间的 UTF-8 起止位置。 */
  byte_start: number;
  byte_end: number;
  /** UTF-16 start/end consumed by the existing Markdown AST.
   * 现有 Markdown AST 使用的 UTF-16 起止位置。 */
  utf16_start: number;
  utf16_end: number;
}

/** Machine-readable Core rejection codes, localized by the shared catalog.
 * Core 机器可读拒绝原因，由共享目录翻译。 */
export type HtmlAnswerUnavailableReason = "invalid_metadata" | "unsupported_version" | "invalid_title"
  | "unclosed_fence" | "interrupted" | "not_persisted" | "page_too_large" | "too_many_pages"
  | "total_too_large" | "empty_source" | "invalid_draft";

/** One Core-derived fence descriptor; generated HTML remains in its original event.
 * 一条 Core 派生围栏描述；生成的 HTML 仍保留在原事件中。 */
export interface HtmlAnswerDescriptor {
  /** Zero-based recognized fence ordinal, including rejected fences.
   * 已识别围栏的零基序号，包含被拒绝的围栏。 */
  ordinal: number;
  /** Validated title, or null when its metadata is invalid.
   * 校验后的标题；元信息非法时为空。 */
  title: string | null;
  /** Exact entire fenced Markdown node.
   * 完整 Markdown 围栏节点的精确范围。 */
  fence: HtmlAnswerRange;
  /** Execution authority comes only from durable Core projection.
   * 执行权威仅来自可靠 Core 投影。 */
  availability: { status: "ready"; artifact_id: string; source: HtmlAnswerRange }
    | { status: "unavailable"; reason: HtmlAnswerUnavailableReason };
}

/** Annotations for one exact event revision within the enclosing owner and generation.
 * 外层归属和代次内一个精确事件修订的标注。 */
export interface HtmlAnswerEventProjection {
  /** Canonical assistant event sequence.
   * 权威助手事件序号。 */
  seq: number;
  /** Exact source revision used by Core.
   * Core 使用的精确源码修订。 */
  event_revision: number;
  /** Source-free descriptors.
   * 不含源码的描述集合。 */
  answers: readonly HtmlAnswerDescriptor[];
}

/** Find the first completed candidate missing Core annotations; return null for ordinary messages.
 * 查找首条缺少 Core 标注的终态候选；普通消息返回空。
 * The marker is only a recovery hint, never parsing, validation or execution authority.
 * 标记仅作为恢复提示，不承担解析、校验或执行授权。
 */
export function htmlAnswerRecoveryStart(
  projections: readonly HtmlAnswerEventProjection[],
  events: readonly ConversationEventSummary[],
): number | null {
  const annotated = new Set(projections.map((projection) => projection.seq));
  const missing = events.find((event) => event.visibility === "provider"
    && event.event.kind === "assistant_text" && event.event.status !== "streaming"
    && event.event.text.includes("vulcan-html") && !annotated.has(event.seq));
  return missing?.seq ?? null;
}

/** Merge accepted Core annotations against the canonical window; preserve identity on replay.
 * 对照权威窗口合并已接纳的 Core 标注；重放时保持对象身份。
 * `previous` is reset on rebase; `incoming` is absent on ordinary traffic. Only loaded exact
 * revisions survive. Publication evidence comes from the accepted Core boundary, not event counters.
 * 重建时清空 `previous`；普通流量不含 `incoming`。仅保留已加载精确修订；持久证据来自接纳的 Core 边界，不取自事件计数器。 */
export function mergeHtmlAnswerProjections(
  previous: readonly HtmlAnswerEventProjection[],
  incoming: readonly HtmlAnswerEventProjection[] | undefined,
  events: readonly ConversationEventSummary[],
): readonly HtmlAnswerEventProjection[] {
  if (previous.length === 0 && !incoming?.length) return previous;
  /** Find an exact seq by binary search over the canonical, already sorted window.
   * 在已排序权威窗口中二分查找精确序号。 */
  const matches = (projection: HtmlAnswerEventProjection): boolean => {
    let lower = 0;
    let upper = events.length;
    while (lower < upper) {
      const middle = Math.floor((lower + upper) / 2);
      if (events[middle].seq < projection.seq) lower = middle + 1;
      else upper = middle;
    }
    const event = events[lower];
    return event !== undefined && event.seq === projection.seq
      && event.event_revision === projection.event_revision
      && event.visibility === "provider" && event.event.kind === "assistant_text"
      && event.event.status !== "streaming";
  };
  const merged = new Map(previous.filter(matches).map((projection) => [projection.seq, projection]));
  for (const projection of incoming ?? []) {
    if (!matches(projection)) continue;
    const existing = merged.get(projection.seq);
    // Only small derived metadata is compared on acknowledgement replay; source text is never serialized.
    // 仅在确认重放时比较少量派生元数据；绝不序列化源码正文。
    if (existing !== undefined && JSON.stringify(existing) === JSON.stringify(projection)) continue;
    merged.set(projection.seq, projection);
  }
  const next = [...merged.values()].sort((left, right) => left.seq - right.seq);
  return next.length === previous.length && next.every((entry, index) => entry === previous[index])
    ? previous : next;
}
