import type {
  ConversationEventBody,
  ConversationEventVisibility,
} from "./conversation-event-window-store";

/** Tool-call event carrying its complete authoritative presentation payload.
 * 携带完整权威展示载荷的工具调用事件。 */
export type ToolCallEvent = Extract<ConversationEventBody, { kind: "tool_call" }>;

/** Exact safe-boundary request-configuration change rendered as one permanent UI fact.
 * 作为永久 UI 事实渲染的精确安全边界请求配置变更。 */
export type RequestConfigChangedEvent = Extract<ConversationEventBody, { kind: "request_config_changed" }>;

/** Typed collaboration event retained as an independent HostOnly timeline fact.
 * 作为独立 HostOnly 时间线事实保留的强类型协作事件。 */
export type CollaborationEvent = Extract<ConversationEventBody, { kind: "collaboration" }>;

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
  // An empty previous route is initial configuration, including notices persisted by older builds.
  // 空的上一条路线表示首次配置，也包括旧版本已持久化的首次配置提示。
  if (event.previous.provider == null && event.previous.model == null) return "none";
  // Credential rotation does not change the user-selected provider/model.
  // 凭据轮换不代表用户选择的供应商或模型发生变化。
  const routeChanged = event.previous.provider !== event.current.provider
    || event.previous.model !== event.current.model;
  if (routeChanged) {
    return "route";
  }
  if ((event.previous.thinking_level ?? null) !== (event.current.thinking_level ?? null)) {
    return "thinking";
  }
  return "none";
}

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

/** Display item for one complete tool-call event.
 * 一条完整工具调用事件的显示项。 */
export interface ToolCallDisplayItem extends ConversationDisplayItemBase {
  kind: "tool_call";
  tool_call: ToolCallEvent;
}

/** Returns whether a typed collaboration fact belongs in chat; execution bookkeeping has no message body.
 * 返回强类型协作事实是否属于聊天内容；执行记账没有消息正文。
 * Terminal bookkeeping stays in storage; subagent_return and non-success records own actual results.
 * 终态记账保留在存储中；真实结果由 subagent_return 与非成功报告负责展示。 */
export function collaborationEventIsUserVisible(event: CollaborationEvent): boolean {
  // Transport progress is attached by exact identity to its original tool/message card.
  // 传输进度按精确身份附加到原工具或消息卡片。
  return event.after_state.kind !== "outbound_delivery"
    && event.after_state.kind !== "inbound_delivery"
    && event.after_state.kind !== "answer_delivery"
    && event.after_state.kind !== "question_execution"
    && event.after_state.kind !== "task_result_acknowledgement"
    && event.after_state.kind !== "task_result_projection"
    && event.event_kind !== "run_control_changed"
    && event.event_kind !== "child_task_reserved"
    && event.event_kind !== "child_task_activated"
    && event.event_kind !== "child_task_terminal";
}

/** Returns the notification identity shared by one notification's lifecycle snapshots, on either side.
 * 返回一条通知在任一侧的生命周期快照共享的通知身份。
 * Other collaboration kinds return null. */
export function collaborationNotificationId(state: CollaborationEvent["after_state"]): string | null {
  return state.kind === "outbound_notification" || state.kind === "inbound_notification"
    ? state.notification.notification_id : null;
}

/** Returns the immutable delivery identity shared by task-message lifecycle snapshots.
 * 返回任务消息生命周期快照共享的不可变投递身份。
 * Other collaboration kinds return null and retain independent timeline semantics.
 * 其他协作类型返回空值，保留独立时间线语义。 */
export function collaborationTaskMessageId(state: CollaborationEvent["after_state"]): string | null {
  switch (state.kind) {
    case "outbound_task_message": return state.record.message.delivery_id;
    case "inbound_task_message": return state.acceptance.message.delivery_id;
    case "task_message_effect": return state.effect.delivery_id;
    default: return null;
  }
}
