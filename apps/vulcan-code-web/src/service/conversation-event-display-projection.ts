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

/** User-visible category of one exact request-configuration transition.
 * 一次精确请求配置转换的用户可见分类。 */
export type RequestConfigChangeKind = "route" | "thinking" | "none";

/** Classifies one durable request configuration event by the fields that actually changed.
 * 按实际发生变化的字段分类一条可靠请求配置事件。
 *
 * @param event exact previous/current request configurations.
 *              精确的变更前后请求配置。
 * @returns route for Provider/account/model changes, thinking for thinking-only changes, otherwise none.
 *          Provider/账号/模型变化返回 route，仅思考强度变化返回 thinking，否则返回 none。 */
export function requestConfigChangeKind(event: RequestConfigChangedEvent): RequestConfigChangeKind {
  const routeChanged = event.previous.provider !== event.current.provider
    || (event.previous.connection_id ?? null) !== (event.current.connection_id ?? null)
    || event.previous.model !== event.current.model;
  if (routeChanged) {
    return "route";
  }
  if ((event.previous.thinking_level ?? null) !== (event.current.thinking_level ?? null)) {
    return "thinking";
  }
  return "none";
}

/** User input rendered through the established right-aligned conversation bubble.
 * 通过既有右对齐会话气泡渲染的用户输入。 */
export type UserInputEvent = Extract<ConversationEventBody, { kind: "user_input" }>;

/** Persisted terminal API facts attached to their assistant output instead of rendered as a row.
 * 附着到对应 assistant 输出而不是单独渲染成行的持久化 API 终态事实。 */
export type ApiStatsEvent = Extract<ConversationEventBody, { kind: "api_stats" }>;

/** User-visible status note emitted by the display-only event family.
 * 由纯显示事件族发出的用户可见状态提示。 */
export type DisplayNoteEvent = Extract<ConversationEventBody, { kind: "display_note" }>;

/** Persisted terminal statistics for one complete logical Turn.
 * 一个完整逻辑 Turn 的持久化终态统计。 */
export type TurnStatsEvent = Extract<ConversationEventBody, { kind: "turn_stats" }>;

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

/** Content item retained inside one collapsible process group in exact seq order.
 * 按精确 seq 顺序保留在一个可折叠处理过程组内的内容项。 */
export type ConversationProcessDisplayItem =
  | AssistantReasoningDisplayItem
  | AssistantMarkdownDisplayItem
  | ToolCallDisplayItem;

/** Transcript node base anchored by the first persisted seq it presents.
 * 由其展示的第一条持久化 seq 锚定的转录节点基类。 */
export interface ConversationTranscriptNodeBase {
  seq: number;
  turn_id: number | null;
}

/** One canonical user-input transcript node.
 * 一条 canonical 用户输入转录节点。 */
export interface ConversationUserInputTranscriptNode extends ConversationTranscriptNodeBase {
  kind: "user_input";
  item: AuxiliaryEventDisplayItem & { event: UserInputEvent };
}

/** Exact root-input identity used by actions rendered at the end of the same Turn.
 * 同一 Turn 结束位置动作使用的精确根输入身份。 */
export interface ConversationRootInputAnchor {
  seq: number;
  text: string;
}

/** One collapsible process group containing reasoning, tool preambles, and tool calls.
 * 一个包含思考、工具前说明与工具调用的可折叠处理过程组。 */
export interface ConversationProcessTranscriptNode extends ConversationTranscriptNodeBase {
  kind: "process_group";
  items: readonly ConversationProcessDisplayItem[];
  active: boolean;
}

/** One final assistant response with its matching physical API statistics.
 * 一条携带匹配物理 API 统计的最终 assistant 回复。 */
export interface ConversationAssistantTranscriptNode extends ConversationTranscriptNodeBase {
  kind: "assistant_final";
  item: AssistantMarkdownDisplayItem;
  api_stats: ApiStatsEvent | null;
  root_input: ConversationRootInputAnchor | null;
}

/** One framed terminal statistics row emitted at the durable end of a Turn.
 * 在 Turn 可靠结束位置发出的一条带边框终态统计行。 */
export interface ConversationTurnStatsTranscriptNode extends ConversationTranscriptNodeBase {
  kind: "turn_stats";
  item: AuxiliaryEventDisplayItem & { event: TurnStatsEvent };
}

/** One permanent request-configuration transition.
 * 一条永久请求配置切换。 */
export interface ConversationRequestConfigTranscriptNode extends ConversationTranscriptNodeBase {
  kind: "request_config_changed";
  item: RequestConfigChangedDisplayItem;
}

/** One user-visible status note without exposing internal ledger events.
 * 一条不暴露内部账本事件的用户可见状态提示。 */
export interface ConversationDisplayNoteTranscriptNode extends ConversationTranscriptNodeBase {
  kind: "display_note";
  item: AuxiliaryEventDisplayItem & { event: DisplayNoteEvent };
}

/** Visual transcript node derived only from typed V1 events.
 * 仅从强类型 V1 事件派生的视觉转录节点。 */
export type ConversationTranscriptDisplayNode =
  | ConversationUserInputTranscriptNode
  | ConversationProcessTranscriptNode
  | ConversationAssistantTranscriptNode
  | ConversationTurnStatsTranscriptNode
  | ConversationRequestConfigTranscriptNode
  | ConversationDisplayNoteTranscriptNode;

/** Builds the visual transcript without exposing structural persistence events as UI rows.
 * 构建视觉转录，同时不把结构性持久化事件暴露为 UI 行。
 *
 * @param items complete projected items in strict canonical seq order.
 *              按严格 canonical seq 顺序排列的完整投影项。
 * @returns visual nodes anchored to the same facts without sorting or parsing text.
 *          不排序也不解析文本、锚定到同一事实的视觉节点。 */
export function projectConversationTranscriptDisplay(
  items: readonly ConversationEventDisplayItem[],
): ConversationTranscriptDisplayNode[] {
  const finalAssistantSeqByTurn = finalAssistantSeqs(items);
  const apiStatsById = new Map<number, ApiStatsEvent>();
  const rootInputByTurn = new Map<string, ConversationRootInputAnchor>();
  for (const item of items) {
    if (item.kind === "event" && item.event.kind === "api_stats") {
      apiStatsById.set(item.event.api_call_id, item.event);
    }
    if (
      item.kind === "event"
      && item.event.kind === "user_input"
      && item.event.input_source === "root"
    ) {
      rootInputByTurn.set(turnKey(item.turn_id), { seq: item.seq, text: item.event.text });
    }
  }

  const nodes: ConversationTranscriptDisplayNode[] = [];
  let processItems: ConversationProcessDisplayItem[] = [];

  /** Flushes the current process run before the next independently visible transcript node.
   * 在下一个独立可见转录节点之前提交当前处理过程片段。 */
  const flushProcess = () => {
    if (processItems.length === 0) return;
    nodes.push({
      kind: "process_group",
      seq: processItems[0].seq,
      turn_id: processItems[0].turn_id,
      active: processItems.some(isActiveProcessItem),
      items: processItems,
    });
    processItems = [];
  };

  /** Appends one process item without ever merging two logical Turns.
   * 追加一条处理过程项，同时绝不合并两个逻辑 Turn。 */
  const appendProcess = (item: ConversationProcessDisplayItem) => {
    if (processItems.length > 0 && processItems[0].turn_id !== item.turn_id) flushProcess();
    processItems.push(item);
  };

  for (const item of items) {
    if (item.visibility === "host_only") continue;
    if (item.kind === "assistant_reasoning" || item.kind === "tool_call") {
      appendProcess(item);
      continue;
    }
    if (item.kind === "assistant_text") {
      if (finalAssistantSeqByTurn.get(turnKey(item.turn_id)) === item.seq) {
        flushProcess();
        nodes.push({
          kind: "assistant_final",
          seq: item.seq,
          turn_id: item.turn_id,
          item,
          api_stats: apiStatsById.get(item.assistant.api_call_id) ?? null,
          root_input: rootInputByTurn.get(turnKey(item.turn_id)) ?? null,
        });
      } else {
        appendProcess(item);
      }
      continue;
    }
    if (item.kind === "request_config_changed") {
      flushProcess();
      nodes.push({
        kind: "request_config_changed",
        seq: item.seq,
        turn_id: item.turn_id,
        item,
      });
      continue;
    }
    if (item.event.kind === "user_input") {
      flushProcess();
      nodes.push({
        kind: "user_input",
        seq: item.seq,
        turn_id: item.turn_id,
        item: { ...item, event: item.event },
      });
      continue;
    }
    if (item.event.kind === "display_note") {
      flushProcess();
      nodes.push({
        kind: "display_note",
        seq: item.seq,
        turn_id: item.turn_id,
        item: { ...item, event: item.event },
      });
      continue;
    }
    if (item.event.kind === "turn_stats") {
      flushProcess();
      nodes.push({
        kind: "turn_stats",
        seq: item.seq,
        turn_id: item.turn_id,
        item: { ...item, event: item.event },
      });
    }
  }
  flushProcess();
  return nodes;
}

/** Selects at most one final assistant text for each logical Turn.
 * 为每个逻辑 Turn 最多选择一条最终 assistant 正文。 */
function finalAssistantSeqs(
  items: readonly ConversationEventDisplayItem[],
): ReadonlyMap<string, number> {
  const apiLastSeq = new Map<number, number>();
  const apiTurn = new Map<number, number | null>();
  const toolApiIds = new Set<number>();
  for (const item of items) {
    const apiCallId = item.group.api_call_id;
    if (apiCallId !== null) {
      apiLastSeq.set(apiCallId, item.seq);
      apiTurn.set(apiCallId, item.turn_id);
    }
    if (item.kind === "tool_call") toolApiIds.add(item.tool_call.api_call_id);
  }

  const latestApiByTurn = new Map<string, number>();
  for (const [apiCallId, seq] of apiLastSeq) {
    const key = turnKey(apiTurn.get(apiCallId) ?? null);
    const previousApiCallId = latestApiByTurn.get(key);
    if (previousApiCallId === undefined || seq > (apiLastSeq.get(previousApiCallId) ?? -1)) {
      latestApiByTurn.set(key, apiCallId);
    }
  }

  const result = new Map<string, number>();
  for (const item of items) {
    if (item.kind !== "assistant_text" || item.visibility === "host_only") continue;
    const key = turnKey(item.turn_id);
    if (latestApiByTurn.get(key) !== item.assistant.api_call_id) continue;
    if (toolApiIds.has(item.assistant.api_call_id)) continue;
    result.set(key, item.seq);
  }
  return result;
}

/** Reports whether one process item still belongs to an active physical operation.
 * 判断一条处理过程项是否仍属于活动中的物理操作。 */
function isActiveProcessItem(item: ConversationProcessDisplayItem): boolean {
  if (item.kind === "assistant_reasoning") return item.reasoning.status === "streaming";
  if (item.kind === "assistant_text") return item.assistant.status === "streaming";
  return item.tool_call.status === "running" || item.tool_call.status === "waiting_user";
}

/** Produces one stable map key without conflating a missing Turn with Turn zero.
 * 生成稳定映射键，同时避免把缺失 Turn 与 Turn 零混为一谈。 */
function turnKey(turnId: number | null): string {
  return turnId === null ? "turn:none" : `turn:${turnId}`;
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
