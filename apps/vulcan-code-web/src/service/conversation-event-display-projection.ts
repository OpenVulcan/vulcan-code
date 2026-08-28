import {
  assertSafeSeq,
  ConversationEventBody,
  ConversationEventSummary,
  ConversationEventVisibility,
} from "./conversation-event-window-store";

/** Assistant reasoning event rendered directly from the V1 event stream.
 * 直接从 V1 事件流渲染的 assistant 思考事件。 */
export type AssistantReasoningEvent = Extract<ConversationEventBody, { kind: "assistant_reasoning" }>;

/** Assistant text event whose text remains the sole Markdown source.
 * 文本作为唯一 Markdown 来源的 assistant 正文事件。 */
export type AssistantTextEvent = Extract<ConversationEventBody, { kind: "assistant_text" }>;

/** Tool-call event carrying its complete authoritative presentation payload.
 * 携带完整权威展示载荷的工具调用事件。 */
export type ToolCallEvent = Extract<ConversationEventBody, { kind: "tool_call" }>;

/** Exact safe-boundary request-configuration change rendered as one permanent UI fact.
 * 作为永久 UI 事实渲染的精确安全边界请求配置变更。 */
export type RequestConfigChangedEvent = Extract<ConversationEventBody, { kind: "request_config_changed" }>;

/** Every persisted event that is not assistant reasoning, assistant text, or a tool call.
 * 除 assistant 思考、正文和工具调用之外的全部持久化事件。 */
export type AuxiliaryConversationEvent = Exclude<
  ConversationEventBody,
  AssistantReasoningEvent | AssistantTextEvent | ToolCallEvent | RequestConfigChangedEvent
>;

/** Stable API grouping coordinates derived only from typed event fields.
 * 仅从强类型事件字段派生的稳定 API 分组坐标。 */
export interface ConversationDisplayGroup {
  turn_id: number | null;
  api_call_id: number | null;
}

/** Shared immutable envelope copied from one authoritative event summary.
 * 从一条权威事件摘要复制的共享不可变 envelope。 */
export interface ConversationDisplayItemBase {
  seq: number;
  turn_id: number | null;
  created_at_ms: number;
  updated_at_ms: number;
  event_revision: number;
  visibility: ConversationEventVisibility;
  group: ConversationDisplayGroup;
}

/** Display item for one assistant reasoning event.
 * 一条 assistant 思考事件的显示项。 */
export interface AssistantReasoningDisplayItem extends ConversationDisplayItemBase {
  kind: "assistant_reasoning";
  reasoning: AssistantReasoningEvent;
}

/** Display item for one assistant text event.
 * 一条 assistant 正文事件的显示项。 */
export interface AssistantMarkdownDisplayItem extends ConversationDisplayItemBase {
  kind: "assistant_text";
  markdown: string;
  assistant: AssistantTextEvent;
}

/** Display item for one complete tool-call event.
 * 一条完整工具调用事件的显示项。 */
export interface ToolCallDisplayItem extends ConversationDisplayItemBase {
  kind: "tool_call";
  tool_call: ToolCallEvent;
}

/** Display item containing exact previous and current request configurations.
 * 包含精确变更前后请求配置的显示项。 */
export interface RequestConfigChangedDisplayItem extends ConversationDisplayItemBase {
  kind: "request_config_changed";
  request_config_changed: RequestConfigChangedEvent;
}

/** Display item for one non-content event retained in canonical seq order.
 * 一条按 canonical seq 顺序保留的非正文事件显示项。 */
export interface AuxiliaryEventDisplayItem extends ConversationDisplayItemBase {
  kind: "event";
  event: AuxiliaryConversationEvent;
}

/** Strongly typed display projection of one authoritative V1 event.
 * 一条权威 V1 事件的强类型显示投影。 */
export type ConversationEventDisplayItem =
  | AssistantReasoningDisplayItem
  | AssistantMarkdownDisplayItem
  | ToolCallDisplayItem
  | RequestConfigChangedDisplayItem
  | AuxiliaryEventDisplayItem;

/** Filters renderer-visible items only after the complete seq sequence has been projected.
 * 仅在完整 seq 序列完成投影后过滤 renderer 可见项。 */
export function visibleConversationEventDisplayItems(
  items: readonly ConversationEventDisplayItem[],
): ConversationEventDisplayItem[] {
  return items.filter(
    (item) => item.visibility !== "host_only" && !isExplicitlyNonRenderedUiEvent(item),
  );
}

/** Identifies persisted UI receipts that affect composer state but have no timeline card.
 * 识别会影响输入区状态、但不生成时间线卡片的持久化 UI receipt。 */
function isExplicitlyNonRenderedUiEvent(item: ConversationEventDisplayItem): boolean {
  return item.kind === "event" && item.event.kind === "queued_input_resolution";
}

/** Validates and projects the already-windowed V1 event sequence without reordering it.
 * 在不重新排序的情况下校验并投影窗口层已经组织好的 V1 事件序列。 */
export function projectConversationEventDisplay(
  events: readonly ConversationEventSummary[],
): ConversationEventDisplayItem[] {
  let previousSeq: number | null = null;
  return events.map((summary) => {
    assertSafeSeq(summary.seq);
    if (previousSeq !== null && summary.seq !== previousSeq + 1) {
      throw new Error(
        `conversation display projection gap: expected seq ${previousSeq + 1}, got ${summary.seq}`,
      );
    }
    previousSeq = summary.seq;
    return projectConversationEvent(summary);
  });
}

/** Projects one event without text parsing, tool merging, detail loading, or filesystem reads.
 * 在不解析工具文本、不合并工具、不加载详情且不读取文件系统的情况下投影一条事件。 */
function projectConversationEvent(summary: ConversationEventSummary): ConversationEventDisplayItem {
  const base = displayItemBase(summary);
  switch (summary.event.kind) {
    case "assistant_reasoning":
      return { ...base, kind: "assistant_reasoning", reasoning: summary.event };
    case "assistant_text":
      return {
        ...base,
        kind: "assistant_text",
        markdown: summary.event.text,
        assistant: summary.event,
      };
    case "tool_call":
      return { ...base, kind: "tool_call", tool_call: summary.event };
    case "request_config_changed":
      return {
        ...base,
        kind: "request_config_changed",
        request_config_changed: summary.event,
      };
    default:
      return { ...base, kind: "event", event: summary.event };
  }
}

/** Builds display envelope and grouping coordinates from one summary.
 * 从一条摘要构建显示 envelope 与分组坐标。 */
function displayItemBase(summary: ConversationEventSummary): ConversationDisplayItemBase {
  return {
    seq: summary.seq,
    turn_id: summary.turn_id,
    created_at_ms: summary.created_at_ms,
    updated_at_ms: summary.updated_at_ms,
    event_revision: summary.event_revision,
    visibility: summary.visibility,
    group: {
      turn_id: summary.turn_id,
      api_call_id: eventApiCallId(summary.event),
    },
  };
}

/** Returns the typed physical API id when the event belongs to one API group.
 * 当事件属于一个 API 分组时返回其强类型物理 API ID。 */
function eventApiCallId(event: ConversationEventBody): number | null {
  switch (event.kind) {
    case "api_started":
    case "assistant_reasoning":
    case "assistant_text":
    case "tool_call":
    case "api_stats":
      return event.api_call_id;
    case "display_note":
    case "execution_control":
      return event.api_call_id ?? null;
    default:
      return null;
  }
}
