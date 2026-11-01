// The V1 service DTO mirrors the complete event envelope. Hydration and pagination
// carry consecutive windows, while live delivery carries sparse patches of the same event shape.
// V1 服务 DTO 镜像完整事件 envelope。hydration 与分页承载连续窗口，实时投递则承载
// 相同事件形态的稀疏 patch。
import { CLIENT_ACTION_ID_MAX_BYTES } from "@/generated/protocol-limits";
import type { AnswerModePreference } from "@/service/answer-mode";
import type { ConversationSessionAccessV1 } from "@/service/conversation-event-window-client";

/** JSON value allowed inside manifest-validated tool payloads.
 * 工具 Manifest 已校验载荷内允许的 JSON 值。 */
export type ToolJsonValue =
  | null
  | boolean
  | number
  | string
  | ToolJsonValue[]
  | { [key: string]: ToolJsonValue };

/** Canonical object-shaped tool input or output.
 * canonical 对象形态的工具输入或输出。 */
export type ToolPayload = { [key: string]: ToolJsonValue };

/** Provider projection visibility matching the Rust event contract.
 * 与 Rust 事件契约一致的 Provider 投影可见性。 */
export type ConversationEventVisibility = "provider" | "ui_only" | "host_only";

/** Largest integer that JavaScript can represent exactly.
 * JavaScript 可以精确表示的最大整数。 */
export const JS_SAFE_INTEGER_MAX = 9_007_199_254_740_991;

/** Nominal marker that prevents unrelated strings from becoming V1 client action identities.
 * 防止无关字符串成为 V1 客户端动作身份的名义类型标记。 */
declare const clientActionIdBrand: unique symbol;

/** Exact browser-generated idempotency key persisted by canonical V1 input events.
 * canonical V1 输入事件持久化的精确浏览器生成幂等键。 */
export type ClientActionId = string & {
  readonly [clientActionIdBrand]: "ClientActionId";
};

/** Validates and brands one browser-generated ClientActionId using the Core V1 bounds.
 * 使用 Core V1 边界校验并标记一个浏览器生成的 ClientActionId。 */
export function clientActionId(value: string): ClientActionId {
  // encodedLength mirrors Rust String::len UTF-8 byte semantics exactly.
  // encodedLength 精确对应 Rust String::len 的 UTF-8 字节语义。
  const encodedLength = new TextEncoder().encode(value).byteLength;
  if (value.trim().length === 0 || value.trim() !== value || encodedLength > CLIENT_ACTION_ID_MAX_BYTES) {
    throw new Error("ClientActionId must be non-empty, unpadded, and no longer than 256 UTF-8 bytes.");
  }
  return value as ClientActionId;
}

/** Nominal marker that prevents arbitrary numbers from becoming project resource identities.
 * 防止任意数字成为项目资源身份的名义类型标记。 */
declare const resourceIdBrand: unique symbol;

/** Exact project-scoped resource identity persisted by canonical V1 events.
 * canonical V1 事件持久化的精确项目级资源身份。 */
export type ResourceId = number & {
  readonly [resourceIdBrand]: "ResourceId";
};

/** Validates and brands one positive JavaScript-safe project ResourceId.
 * 校验并标记一个正数且 JavaScript 安全的项目 ResourceId。 */
export function resourceId(value: number): ResourceId {
  if (!Number.isSafeInteger(value) || value <= 0 || value > JS_SAFE_INTEGER_MAX) {
    throw new Error("ResourceId must be a positive JavaScript-safe integer.");
  }
  return value as ResourceId;
}

/** Physical Provider request purpose.
 * 物理 Provider 请求用途。 */
export type ApiPurposeKind =
  | "main_generation"
  | "retry_generation"
  | "compaction"
  | "continue"
  | "subagent"
  // Matches the canonical collaboration-question request purpose in Session Core.
  // 对齐 Session Core 中规范的协作答疑请求用途。
  | "question_execution";

/** Immutable resolved configuration of one Provider request.
 * 一次 Provider 请求的不可变已解析配置。 */
export interface ApiRequestSnapshot {
  provider: string;
  profile?: string;
  model: string;
  connection_id?: string;
  thinking_level?: string;
  temperature?: number;
}

/** Request configuration effective on one side of a safe-boundary change.
 * 安全边界配置变更一侧实际生效的请求配置。 */
export interface ActiveRequestConfig {
  provider?: string;
  connection_id?: string;
  model?: string;
  thinking_level?: string;
  temperature?: number;
}

/** Provider-native continuation checkpoint attached to one API group.
 * 附着在一个 API 组上的 Provider 原生续传检查点。 */
export interface ProviderSource {
  provider: string;
  profile?: string;
  model: string;
  connection_id?: string;
  message_id?: string;
  response_id?: string;
  opaque_replay_state?: string;
  covered_until_seq: number;
  fingerprint_sha256: string;
}

/** Stable reason one `ui-code` excerpt could not be captured when its reply was sealed.
 * 单个 `ui-code` 摘录在回答封口时无法抓取的稳定原因。 */
export type AnswerCodeUnavailableReason =
  | "not_found"
  | "no_workspace"
  | "beyond_end"
  | "not_text"
  | "too_large"
  | "read_failed";

/** File lines one `ui-code` location quoted, captured once when its reply was sealed.
 * 单个 `ui-code` 位置引用的文件行，在其回答封口时抓取一次。 */
export interface AnswerCodeSnapshot {
  /** Exact trimmed location cell such as `src/a.rs:40-58`.
   * 精确的去空白位置单元格，例如 `src/a.rs:40-58`。 */
  location: string;
  /** Lines joined by `\n` from the start line, or why they are unavailable.
   * 自首行起以 `\n` 连接的各行，或无法提供的原因。 */
  result:
    | { status: "captured"; text: string }
    | { status: "unavailable"; reason: AnswerCodeUnavailableReason };
}

/** Official token usage reported by one physical API request.
 * 一次物理 API 请求上报的官方 token 用量。 */
export interface UsageSnapshot {
  input_tokens?: number;
  uncached_input_tokens?: number;
  output_tokens?: number;
  reasoning_tokens?: number;
  total_tokens?: number;
  cache_read_tokens?: number;
  cache_write_tokens?: number;
  audio_input_tokens?: number;
  audio_output_tokens?: number;
  billing_tier?: string;
}

/** Aggregated token usage whose absent dimensions remain unknown.
 * 缺失维度保持未知的聚合 token 用量。 */
export interface AggregatedUsage {
  input_tokens?: number;
  uncached_input_tokens?: number;
  output_tokens?: number;
  reasoning_tokens?: number;
  total_tokens?: number;
  cache_read_tokens?: number;
  cache_write_tokens?: number;
  audio_input_tokens?: number;
  audio_output_tokens?: number;
}

/** One aggregate bucket used by terminal turn statistics.
 * 终态回合统计使用的一组聚合桶。 */
export interface StatBucket {
  /** Additive timing for measured requests; omitted in older history.
   * 实测请求的可累加计时；旧历史中省略。 */
  output_timing?: {
    total_request_count: number;
    total_request_duration_ms: number;
    request_count: number;
    request_duration_ms: number;
    first_token_latency_ms: number;
    output_duration_ms: number;
    output_tokens: number;
  };
  api_call_count: number;
  unknown_usage_api_call_count: number;
  tool_call_count: number;
  subagent_return_count: number;
  usage: AggregatedUsage;
}

/** Direct, subagent, and total buckets of one closed turn.
 * 一个闭合回合的 direct、subagent 与 total 三组统计。 */
export interface TurnStatBuckets {
  direct: StatBucket;
  subagent: StatBucket;
  total: StatBucket;
}

/** Canonical persisted change-set.
 * canonical 持久化 change-set。 */
export interface PersistentChangeSet {
  mode: "preview" | "applied";
  summary?: string;
  files: PersistentChangeSetFile[];
  diagnostics?: PersistentChangeSetDiagnostic[];
}

/** One structured producer diagnostic preserved with the change-set.
 * 与 change-set 一同保留的一条产出方结构化诊断。 */
export interface PersistentChangeSetDiagnostic {
  level: string;
  message: string;
}

/** Exact file text-format snapshot preserved before or after a mutation.
 * 文件变更前后保留的精确文本格式快照。 */
export interface PersistentTextFormat {
  encoding?: string;
  encoding_source?: string;
  encoding_certain?: boolean;
  bom?: boolean;
  line_ending?: string;
  preferred_line_ending?: string;
  mixed_line_endings?: boolean;
  final_newline?: boolean;
}

/** Exact lifecycle text retained for create, delete, or rename records.
 * 为创建、删除或重命名记录保留的精确生命周期正文。 */
export type PersistentLifecycleContent =
  | { mode: "full"; content: string; total_line_count: number }
  | { mode: "truncated"; head: string; tail: string; total_line_count: number }
  | { mode: "omitted"; total_line_count?: number };

/** One file record inside a persisted change-set.
 * 持久化 change-set 内的一条文件记录。 */
export interface PersistentChangeSetFile {
  resource_id?: string;
  change: "create" | "modify" | "delete" | "rename";
  path?: string;
  old_path?: string;
  new_path?: string;
  hunks: PersistentChangeSetHunk[];
  lifecycle_content?: PersistentLifecycleContent;
  before_text_format?: PersistentTextFormat;
  after_text_format?: PersistentTextFormat;
}

/** One exact modify hunk retaining only context and changed lines.
 * 一条仅保留上下文与变更行的精确修改 hunk。 */
export interface PersistentChangeSetHunk {
  before: string;
  after: string;
  deleted: PersistentChangeSetLine[];
  inserted: PersistentChangeSetLine[];
}

/** One line preserved by a change-set hunk.
 * change-set hunk 保留的一行。 */
export interface PersistentChangeSetLine {
  line: number;
  content: string;
}

/** Navigation-only external resource reference.
 * 仅用于导航的外部资源引用。 */
export interface ExternalResourceRef {
  kind: "terminal" | "subagent" | "lua_session";
  id: string;
}

/** Serialized kind of one typed collaboration mutation.
 * 一条强类型协作变更的序列化类型。 */
export type CollaborationEventKind =
  | "task_result_recorded"
  | "task_result_accepted"
  | "task_result_acknowledged"
  | "task_result_projected"
  | "task_message_sent"
  | "task_message_accepted"
  | "task_message_acknowledged"
  | "task_message_effect"
  | "question_created"
  | "delivery_recorded"
  | "delivery_accepted"
  | "delivery_projected"
  | "delivery_failed"
  | "delivery_settled"
  | "delivery_settlement_acknowledged"
  | "notification_sent"
  | "notification_progress"
  | "notification_accepted"
  | "notification_projected"
  | "notification_failed"
  | "question_answered"
  | "question_cancelled"
  | "answer_delivered"
  | "answer_delivery_failed"
  | "child_task_reserved"
  | "child_task_activated"
  | "child_task_terminal"
  | "child_task_non_success_recorded"
  | "child_task_non_success_projected"
  | "question_execution_started"
  | "question_execution_completed"
  | "question_successor_scheduled"
  | "grant_added"
  | "grant_removed"
  | "run_control_changed"
  | "generation_ownership_cleared";

/** Current settlement state of one source-owned question.
 * 一条来源问题当前的结算状态。 */
export type CollaborationQuestionState = "pending" | "answered" | "cancelled";

/** Current progress of one collaboration delivery.
 * 一条协作投递当前的进度。 */
export type CollaborationDeliveryState = "recorded" | "accepted" | "projected" | "failed";

/** Independent lifecycle of one typed one-way notification.
 * 一条强类型单向通知的独立生命周期。 */
export type CollaborationNotificationState = "recorded" | "accepted" | "projected" | "failed";

/** Stable failure reason for one failed collaboration delivery.
 * 一条协作投递失败时使用的稳定原因。 */
export type CollaborationDeliveryFailureReason =
  | "target_stopped"
  | "target_missing"
  | "source_unavailable"
  | "authorization_revoked"
  | "generation_mismatch"
  | "invalid_binding"
  | "unknown";

/** Independent route used to deliver a settled answer.
 * 已结算答案使用的独立交付路径。 */
export type CollaborationAnswerDeliveryKind = "wait_tool" | "user_event";

/** Current state of the independent answer delivery record.
 * 独立答案交付记录的当前状态。 */
export type CollaborationAnswerDeliveryState = "undelivered" | "delivered" | "failed";

/** Lifecycle state of one child task reservation.
 * 一条子任务预留的生命周期状态。 */
export type CollaborationChildTaskState = "reserved" | "active" | "terminal";

/** Explicit terminal outcome of one child task.
 * 一条子任务的明确终态结果。 */
export type CollaborationChildTaskTerminalOutcome = "completed" | "failed" | "stopped" | "cancelled";

/** Explicit host run-control state captured by collaboration events.
 * 协作事件记录的明确宿主运行控制状态。 */
export type CollaborationRunControl = "running" | "naturally_idle" | "stopped" | "main_only" | "main_and_subagents";

/** Purpose of one collaboration-related execution.
 * 一次协作相关执行的用途。 */
export type CollaborationExecutionPurpose = "normal_task" | "in_turn_question" | "naturally_idle_question";

/** Lifecycle state of one dedicated question execution.
 * 一次专用问题执行的生命周期状态。 */
export type CollaborationQuestionExecutionState = "running" | "completed";

/** Outcome of one completed dedicated question execution.
 * 一次完成的专用问题执行结果。 */
export type CollaborationQuestionExecutionOutcome = "settled" | "failed" | "unanswered";

/** Terminal settlement selected by the source authority and acknowledged by the target.
 * 来源权威选定且目标确认的终态结算。 */
export type CollaborationTargetSettlementState = "answered" | "cancelled";

/** Authority that created one collaboration grant.
 * 创建一条协作授权的权威主体。 */
export type CollaborationGrantAuthority = "local_user";

/** Action admitted by one collaboration grant.
 * 一条协作授权允许的动作。 */
export type CollaborationGrantAction = "read" | "ask" | "steer" | "start" | "reply" | "notify";

/** Stable host-owned locator of one collaboration participant.
 * 一名协作参与者的宿主稳定定位信息。 */
export interface CollaborationParticipant {
  session_id: string;
  root_session_id: string;
  project_id: string;
  segment_generation: number;
  turn_id?: number;
  task_id?: string;
}

/** Independent delivery revision of one settled answer.
 * 一条已结算答案的独立交付修订记录。 */
export interface CollaborationAnswerDeliveryRecord {
  state: CollaborationAnswerDeliveryState;
  /** Stable reason when delivery failed before the source model received the answer.
   * 答案进入来源模型前交付失败时的稳定原因。 */
  failure?: CollaborationDeliveryFailureReason;
  delivery_revision: number;
  kind?: CollaborationAnswerDeliveryKind;
  projected_seq?: number;
  /** Stable UserInput identity when the answer uses a User event.
   * 答案使用 User 事件交付时的稳定 UserInput 身份。*/
  projected_input_id?: number;
  /** Stable tool-call identity when the answer uses a tool result.
   * 答案使用工具结果交付时的稳定工具调用身份。*/
  tool_call_id?: number;
  /** Physical ToolCall event sequence carrying the completed result.
   * 携带已完成结果的物理 ToolCall 事件序号。*/
  tool_result_seq?: number;
  last_operation_id: string;
  updated_at_ms: number;
}

/** Answer stored by the source question authority.
 * 来源问题权威持有的答案记录。 */
export interface CollaborationAnswerRecord {
  operation_id: string;
  responder: CollaborationParticipant;
  body: string;
  settled_at_ms: number;
  delivery: CollaborationAnswerDeliveryRecord;
}

/** Cancellation stored by the source question authority.
 * 来源问题权威持有的撤销记录。 */
export interface CollaborationCancellationRecord {
  operation_id: string;
  requester: CollaborationParticipant;
  reason: string;
  cancelled_at_ms: number;
}

/** Source-owned question record and its immutable identities.
 * 来源拥有的问题记录及其不可变身份。 */
export interface CollaborationQuestionRecord {
  operation_id: string;
  question_id: string;
  delivery_id: string;
  source: CollaborationParticipant;
  target: CollaborationParticipant;
  body: string;
  execution_purpose: CollaborationExecutionPurpose;
  state: CollaborationQuestionState;
  answer?: CollaborationAnswerRecord;
  cancellation?: CollaborationCancellationRecord;
  created_at_ms: number;
}

/** Source-side durable acknowledgement of a target terminal settlement.
 * 来源侧持久化记录的目标终态结算确认。 */
export interface CollaborationTargetSettlementAck {
  state: CollaborationTargetSettlementState;
  source_event_seq: number;
  target_event_seq: number;
  target_revision: number;
  acknowledged_at_ms: number;
}

/** Source-owned outbound delivery progress.
 * 来源拥有的出站投递进度。 */
export interface CollaborationDeliveryRecord {
  delivery_id: string;
  question_id: string;
  operation_id: string;
  source: CollaborationParticipant;
  target: CollaborationParticipant;
  state: CollaborationDeliveryState;
  failure?: CollaborationDeliveryFailureReason;
  target_revision?: number;
  target_event_seq?: number;
  settlement_ack?: CollaborationTargetSettlementAck;
  updated_at_ms: number;
}

/** Target-side terminal settlement acknowledgement.
 * 目标侧终态结算确认。 */
export interface CollaborationInboundSettlementRecord {
  state: CollaborationTargetSettlementState;
  source_event_seq: number;
  source_operation_id: string;
  target_operation_id: string;
  acknowledged_at_ms: number;
}

/** Target-owned accepted, projected, or failed delivery record.
 * 目标拥有的接纳、投影或失败投递记录。 */
export interface CollaborationInboundDeliveryRecord {
  delivery_id: string;
  question_id: string;
  operation_id: string;
  source: CollaborationParticipant;
  target: CollaborationParticipant;
  bound_turn_id?: number;
  execution_purpose: CollaborationExecutionPurpose;
  state: CollaborationDeliveryState;
  failure?: CollaborationDeliveryFailureReason;
  target_revision: number;
  projected_seq?: number;
  /** Stable UserInput identity proved by the projection event.
   * 投影事件证明的稳定 UserInput 身份。*/
  projected_input_id?: number;
  /** Logical turn carried by the projected UserInput event.
   * 投影 UserInput 事件携带的逻辑回合。*/
  projected_turn_id?: number;
  settlement?: CollaborationInboundSettlementRecord;
  updated_at_ms: number;
}

/** Source-owned outbox record for one one-way notification.
 * 一条单向通知的来源出站箱记录。 */
export interface CollaborationNotificationRecord {
  notification_id: string;
  delivery_id: string;
  operation_id: string;
  source: CollaborationParticipant;
  target: CollaborationParticipant;
  body: string;
  state: CollaborationNotificationState;
  failure?: CollaborationDeliveryFailureReason;
  target_revision?: number;
  target_event_seq?: number;
  created_at_ms: number;
  updated_at_ms: number;
}

/** Target-owned acceptance and projection record for one notification.
 * 一条通知的目标接纳与投影记录。 */
export interface CollaborationInboundNotificationRecord {
  notification_id: string;
  delivery_id: string;
  source_operation_id: string;
  operation_id: string;
  source: CollaborationParticipant;
  target: CollaborationParticipant;
  body: string;
  state: CollaborationNotificationState;
  failure?: CollaborationDeliveryFailureReason;
  target_revision: number;
  projected_seq?: number;
  projected_input_id?: number;
  projected_turn_id?: number;
  accepted_at_ms: number;
  updated_at_ms: number;
}

/** Parent-bound child task reservation and terminal state.
 * 按父回合绑定的子任务预留及终态。 */
export interface CollaborationChildTaskRecord {
  /** Authenticated original reservation operation.
   * 已认证的原始预留操作。 */
  reservation_operation_id: string;
  /** Original canonical ToolCall name.
   * 原始规范 ToolCall 名称。 */
  source_tool_name: string;
  /** Exact execution identity, independent of the reusable child session.
   * 精确执行身份，独立于可复用子会话。 */
  execution_id: string;
  task_id: string;
  /** Parent generation captured by the reservation.
   * 预留时捕获的父文档代际。 */
  parent_segment_generation: number;
  parent_turn_id: number;
  child: CollaborationParticipant;
  state: CollaborationChildTaskState;
  reserved_at_ms: number;
  active_at_ms?: number;
  terminal_at_ms?: number;
  terminal_outcome?: CollaborationChildTaskTerminalOutcome;
  /** Exact persisted non-success report owned by this reservation.
   * 此预留拥有的精确持久非成功报告。 */
  non_success_result: CollaborationChildTaskNonSuccessResult | null;
  /** Complete model input proof, independent of the original report receipt.
   * 完整模型输入凭据，独立于原报告回执。 */
  non_success_projection: CollaborationChildTaskResultProjection | null;
}

/** Durable complete User input batch for one owned child result.
 * 一份所属子结果的可靠完整 User 输入批次。 */
export interface CollaborationChildTaskResultProjection {
  /** Original child execution identity.
   * 原子执行身份。 */
  execution_id: string;
  /** Parent report event sequence.
   * 父报告事件序号。 */
  result_seq: number;
  /** Original parent business turn.
   * 原父业务回合。 */
  parent_turn_id: number;
  /** Complete ordered input identities.
   * 完整有序输入身份。 */
  inputs: Array<{ input_seq: number; input_id: number }>;
  /** Immutable projection operation.
   * 不可变投影操作。 */
  operation_id: string;
  /** Projection receipt event sequence.
   * 投影回执事件序号。 */
  event_seq: number;
  /** Atomic revision publishing all inputs.
   * 发布全部输入的原子修订。 */
  parent_revision: number;
  /** First projection timestamp.
   * 首次投影时间戳。 */
  projected_at_ms: number;
}

/** Durable failed or stopped child result, independent of assistant final text.
 * 可靠失败或停止子结果，独立于助手最终文本。 */
export interface CollaborationChildTaskNonSuccessResult {
  /** Atomic parent revision publishing the original report.
   * 发布原报告的原子父修订。 */
  parent_revision: number;
  /** Parent operation freezing the original source proof.
   * 冻结原来源凭据的父操作。 */
  operation_id: string;
  /** Parent publication event sequence.
   * 父发布事件序号。 */
  event_seq: number;
  /** Exact child root, terminal and host diagnostic coordinates.
   * 精确子根输入、终态及宿主诊断坐标。 */
  proof: {
    /** Source physical session.
     * 来源物理会话。 */
    child_session_id: string;
    /** Source document generation.
     * 来源文档代次。 */
    child_segment_generation: number;
    /** Root-input execution identity.
     * 根输入执行身份。 */
    execution_id: string;
    /** Original source business turn.
     * 原来源业务回合。 */
    child_turn_id: number;
    /** Exact root input sequence.
     * 精确根输入序号。 */
    child_start_seq: number;
    /** Exact terminal statistic sequence.
     * 精确终态统计序号。 */
    child_terminal_seq: number;
    /** Persisted logical closure, distinct from failure classification.
     * 已持久逻辑闭合，区别于失败分类。 */
    child_turn_outcome: "stopped" | "abandoned";
    /** Authenticated host outcome.
     * 已认证宿主结果。 */
    outcome: "failed" | "stopped" | "cancelled";
    /** Original permanent diagnostics in event order.
     * 按事件顺序排列的原永久诊断。 */
    failure_notes: Array<{
      /** Child diagnostic event sequence.
       * 子诊断事件序号。 */
      event_seq: number;
      /** Stable host reason code.
       * 稳定宿主原因码。 */
      reason_code: string;
      /** Producer-redacted detail when present.
       * 存在时的生产端脱敏详情。 */
      redacted_detail: string | null;
    }>;
  };
}

/** Dedicated question execution state.
 * 专用问题执行状态。 */
export interface CollaborationQuestionExecutionRecord {
  execution_id: string;
  operation_id: string;
  question_id: string;
  participant: CollaborationParticipant;
  purpose: CollaborationExecutionPurpose;
  parent_turn_id?: number;
  execution_turn_id?: number;
  state: CollaborationQuestionExecutionState;
  started_at_ms: number;
  completed_at_ms?: number;
  outcome?: CollaborationQuestionExecutionOutcome;
  /** Exact queued business task selected after this auxiliary execution.
   * 此辅助执行之后选定的精确排队业务任务。 */
  queued_successor?: CollaborationQueuedUserSuccessor;
}

/** Durable queued successor identity, independent of the completed question attempt.
 * 可靠队列后继身份，独立于已完成问题尝试。 */
export interface CollaborationQueuedUserSuccessor {
  /** New business turn.
   * 新业务回合。 */
  turn_id: number;
  /** Preserved accepted input.
   * 保留的已接纳输入。 */
  input_id: number;
  /** Original user action.
   * 原用户动作。 */
  client_action_id: string;
  /** Exact scheduled input event.
   * 精确已调度输入事件。 */
  input_seq: number;
}

/** User-owned authorization grant record.
 * 用户拥有的协作授权记录。 */
export interface CollaborationGrantRecord {
  grant_id: string;
  authority: CollaborationGrantAuthority;
  source_session_id: string;
  target_session_id: string;
  actions: CollaborationGrantAction[];
  created_revision: number;
  created_at_ms: number;
  revoked_at_ms?: number;
  revoked_revision?: number;
}

/** One explicit run-control transition retained for audit and recovery.
 * 一条为审计和恢复保留的明确运行控制转换。 */
export interface CollaborationRunControlRecord {
  operation_id: string;
  state: CollaborationRunControl;
  changed_at_ms: number;
}

/** Frozen source intent for a new task or guidance to one exact existing turn.
 * 新任务或精确已有回合方向的冻结来源意图。 */
export interface CollaborationTaskMessage {
  // Stable source operation and delivery identities.
  // 稳定来源操作及投递身份。
  operation_id: string;
  delivery_id: string;
  // Host-verified participants; only steering binds an existing target turn.
  // 宿主验证的参与者；只有方向消息绑定目标已有回合。
  source: CollaborationParticipant;
  target: CollaborationParticipant;
  // Explicit business intent and literal body.
  // 明确业务意图及原样正文。
  kind: "task_start" | "task_steer";
  body: string;
}

/** Immutable acceptance receipt; it does not imply execution or task completion.
 * 不可变接纳回执；不表示执行或任务完成。 */
export interface CollaborationTaskMessageAcceptance {
  // Exact accepted intent and target operation.
  // 精确接纳意图及目标操作。
  message: CollaborationTaskMessage;
  operation_id: string;
  // Target publication coordinates and original acceptance time.
  // 目标发布坐标与原始接纳时间。
  target_revision: number;
  event_seq: number;
  accepted_at_ms: number;
}

/** Source outbox snapshot with its explicit nullable target acknowledgement.
 * 来源出站快照，携带明确可空的目标准入确认。 */
export interface CollaborationTaskMessageRecord {
  // Frozen original intent and its source creation time.
  // 冻结原始意图及来源创建时间。
  message: CollaborationTaskMessage;
  created_at_ms: number;
  // Null before source acknowledgement; target acceptance is never task completion.
  // 来源确认前为空；目标接纳绝不等于任务完成。
  acceptance: CollaborationTaskMessageAcceptance | null;
}

/** Exact target delivery effect, distinct from the business task result.
 * 精确目标投递效果，与业务任务结果区分。 */
export interface CollaborationTaskMessageEffect {
  // Delivery and effect operation identities.
  // 投递及效果操作身份。
  delivery_id: string;
  operation_id: string;
  // Explicit application coordinates or a classified delivery failure.
  // 明确应用坐标或分类投递失败。
  outcome:
    | { kind: "claimed"; execution_id: string; turn_id: number; input_id: number; input_seq: number }
    | { kind: "steered"; turn_id: number; input_id: number; input_seq: number }
    | { kind: "failed"; reason: CollaborationDeliveryFailureReason };
  // Target effect publication coordinates and timestamp.
  // 目标效果发布坐标及时间戳。
  target_revision: number;
  event_seq: number;
  occurred_at_ms: number;
}

/** Immutable completed assistant snapshot with its original target event coordinates.
 * 不可变的已完成助手快照及其原目标事件坐标。 */
export interface CollaborationTaskFinalAssistant {
  // Exact business API and its terminal statistic.
  // 精确业务 API 及其终态统计。
  api_call_id: number;
  api_stats_seq: number;
  // Unique complete original text snapshot; delivery chunking does not alter this source.
  // 唯一的完整原始正文快照；投递分块不改变此来源。
  source_text: { event_seq: number; text: string };
}

/** Frozen business closure, independent of the target's current active turn.
 * 冻结的业务闭合事实，独立于目标当前活动回合。 */
export interface CollaborationTaskTerminal {
  // Exact completed business turn and its statistic range.
  // 精确闭合业务回合及其统计范围。
  turn_id: number;
  turn_start_seq: number;
  turn_stats_seq: number;
  // Successful completion alone carries an actual final assistant response.
  // 仅成功完成携带实际最终助手回复。
  outcome: "completed" | "stopped" | "abandoned";
  final_assistant: CollaborationTaskFinalAssistant | null;
  // Actual target closure timestamp.
  // 实际目标闭合时间戳。
  completed_at_ms: number;
}

/** Target-owned result binding the accepted intent to business closure or pre-application failure.
 * 目标所有的结果，将已接纳意图绑定至业务闭合或应用前失败。 */
export interface CollaborationTaskResult {
  // Original participants, application effect and optional immutable business outcome.
  // 原始参与者、应用效果及可选的不可变业务结果。
  acceptance: CollaborationTaskMessageAcceptance;
  effect: CollaborationTaskMessageEffect;
  terminal: CollaborationTaskTerminal | null;
  // Target recording operation, publication coordinates and recording timestamp.
  // 目标记录操作、发布坐标及记录时间戳。
  operation_id: string;
  target_revision: number;
  event_seq: number;
  recorded_at_ms: number;
}

/** Original requester receipt for a full or redacted immutable result, separate from model consumption.
 * 原请求者对完整或脱敏不可变结果的回执，与模型消费区分。 */
export interface CollaborationTaskResultAcceptance {
  // Source-storable result and frozen disclosure decision.
  // 来源可存储结果及冻结的披露决策。
  result: CollaborationTaskResult;
  disclosure: "full" | "authorization_revoked";
  operation_id: string;
  // Receiver-local publication coordinates and original acceptance time.
  // 接收方本地发布坐标及原接纳时间。
  event_seq: number;
  receiver_revision: number;
  accepted_at_ms: number;
}

/** Exact authorized or redacted input batch committed to the original requesting turn.
 * 提交到原请求回合的精确已授权或脱敏输入批次。 */
export interface CollaborationTaskResultProjection {
  // Original delivery, receiver acceptance and business turn identities.
  // 原投递、接收方接纳及业务回合身份。
  delivery_id: string;
  acceptance_seq: number;
  turn_id: number;
  // Frozen disclosure decision for the model-visible input batch.
  // 对模型可见输入批次冻结的披露决策。
  disclosure: "full" | "authorization_revoked";
  // Ordered input proof for every result chunk.
  // 每份结果分块的有序输入凭据。
  inputs: Array<{ input_seq: number; input_id: number }>;
  // Immutable local operation and publication coordinates.
  // 不可变本地操作及发布坐标。
  operation_id: string;
  event_seq: number;
  receiver_revision: number;
  projected_at_ms: number;
}

/** Internal executor acknowledgement of the original requester's persistence, not a model reply.
 * 执行者对原请求者持久化的内部确认，不是模型回复。 */
export interface CollaborationTaskResultAcknowledgement {
  // Original immutable requester receipt and executor confirmation identity.
  // 原不可变请求者回执及执行者确认身份。
  acceptance: CollaborationTaskResultAcceptance;
  operation_id: string;
  // Executor-local publication coordinates and original acknowledgement time.
  // 执行者本地发布坐标及原确认时间。
  event_seq: number;
  target_revision: number;
  acknowledged_at_ms: number;
}

/** Exact entity after-state changed by one collaboration event.
 * 一条协作事件变更实体的精确后状态。 */
export type CollaborationAfterState =
  | { kind: "outbound_task_message"; record: CollaborationTaskMessageRecord }
  | { kind: "inbound_task_message"; acceptance: CollaborationTaskMessageAcceptance }
  | { kind: "task_message_effect"; effect: CollaborationTaskMessageEffect }
  | { kind: "task_result"; result: CollaborationTaskResult }
  | { kind: "received_task_result"; acceptance: CollaborationTaskResultAcceptance }
  | { kind: "task_result_acknowledgement"; acknowledgement: CollaborationTaskResultAcknowledgement }
  | { kind: "task_result_projection"; projection: CollaborationTaskResultProjection }
  | { kind: "question"; question: CollaborationQuestionRecord; delivery: CollaborationDeliveryRecord }
  | { kind: "outbound_delivery"; delivery: CollaborationDeliveryRecord }
  | { kind: "inbound_delivery"; delivery: CollaborationInboundDeliveryRecord }
  | { kind: "outbound_notification"; notification: CollaborationNotificationRecord }
  | { kind: "inbound_notification"; notification: CollaborationInboundNotificationRecord }
  | { kind: "answer_delivery"; question_id: string; delivery: CollaborationAnswerDeliveryRecord }
  | { kind: "child_task"; task: CollaborationChildTaskRecord }
  | { kind: "question_execution"; execution: CollaborationQuestionExecutionRecord }
  | { kind: "grant"; grant: CollaborationGrantRecord }
  | { kind: "run_control"; transition: CollaborationRunControlRecord }
  | { kind: "generation_ownership_cleared" };

/** Complete serialized collaboration event body inside the V1 event envelope.
 * V1 事件 envelope 内完整序列化协作事件体。 */
export interface CollaborationEvent {
  kind: "collaboration";
  version: number;
  operation_id: string;
  event_kind: CollaborationEventKind;
  after_state: CollaborationAfterState;
}

/** Persisted semantic kind of one project-owned resource generated by a tool.
 * 工具生成并归项目所有的一项持久化资源的语义类型。 */
export type GeneratedResourceKind = "audio" | "image" | "video" | "pdf" | "file";

/** Ordered project resource reference committed with one terminal tool event.
 * 随一条终态工具事件提交的有序项目资源引用。 */
export interface GeneratedResourceRef {
  resource_id: ResourceId;
  kind: GeneratedResourceKind;
  preview_resource_id?: ResourceId;
}

/** Timing checkpoints of one physical API request.
 * 一次物理 API 请求的时间点。 */
export interface ApiTimings {
  prepared_at_ms?: number;
  dispatched_at_ms?: number;
  first_event_at_ms?: number;
  first_token_at_ms?: number;
  completed_at_ms?: number;
}

/** Redacted API error persisted with terminal request statistics.
 * 与终态请求统计一同持久化的脱敏 API 错误。 */
export interface RedactedApiError {
  category: "provider" | "network" | "auth" | "balance" | "protocol" | "timeout" | "unknown";
  error_code?: string;
  message: string;
}

/** Host closure reasons shared by ordinary and auxiliary turns, matching Core's TurnClosureReason.
 * 普通与辅助回合共享的宿主闭合原因，与 Core 的 TurnClosureReason 一致。 */
export type TurnClosureReason = "completed" | "abandoned" | "stopped" | "failed";

/** Terminal maintenance facts serialized by Core; null on the execution means it remains open.
 * Core 序列化的维护终态事实；执行上的空值表示仍未闭合。 */
export interface ContextMaintenanceTerminal {
  /** Explicit closure classification, independent of business outcome.
   * 明确闭合分类，独立于业务结果。 */
  reason: TurnClosureReason;
  /** Committed summary identity, present only after successful maintenance.
   * 已提交摘要身份，仅成功维护后存在。 */
  compaction_id: number | null;
  /** Unique terminal statistics sequence for this auxiliary turn.
   * 此辅助回合唯一的终态统计序号。 */
  end_seq: number;
  /** Completion timestamp in Unix milliseconds.
   * 完成时间戳，单位为 Unix 毫秒。 */
  completed_at_ms: number;
}

/** Exact Core ContextMaintenanceExecution wire record retained by hydration, pagination and live updates.
 * 由首次加载、分页和实时更新保留的精确 Core ContextMaintenanceExecution 线协议记录。 */
export interface ContextMaintenanceExecution {
  /** Unique auxiliary execution identity, independent of business execution.
   * 唯一辅助执行身份，独立于业务执行。 */
  execution_id: string;
  /** Original child execution identity; manual root maintenance explicitly stores null.
   * 原子执行身份；手动主会话维护明确存储空值。 */
  business_execution_id: string | null;
  /** Completed business source turn, never reopened by maintenance.
   * 已完成的业务来源回合，绝不由维护重开。 */
  business_turn_id: number;
  /** Separate turn owning the maintenance API requests.
   * 拥有维护 API 请求的独立回合。 */
  turn_id: number;
  /** Exact history generation owning the execution.
   * 拥有执行的精确历史代次。 */
  segment_generation: number;
  /** Durable planning revision verified at admission.
   * 准入时核验的可靠规划修订。 */
  source_revision: number;
  /** Compaction checkpoint preceding this attempt, explicitly null before the first one.
   * 本次尝试之前的压缩检查点，首次压缩前明确为空。 */
  source_compaction_id: number | null;
  /** Immutable host-only start event sequence.
   * 不可变的仅宿主起始事件序号。 */
  started_seq: number;
  /** Admission timestamp in Unix milliseconds.
   * 准入时间戳，单位为 Unix 毫秒。 */
  started_at_ms: number;
  /** Published auxiliary closure, explicitly null while execution is open.
   * 已发布的辅助闭合；执行开放期间明确为空。 */
  terminal: ContextMaintenanceTerminal | null;
}

/** Full strongly typed persisted event body.
 * 完整强类型持久化事件体。 */
export type ConversationEventBody =
  | {
      kind: "system_context";
      context_key: string;
      operation: "upsert" | "remove";
      scope: "global" | "project" | "session" | "turn";
      ttl_turns?: number;
      provider_role_hint?: "developer" | "system" | "hidden_user_shell";
      content: string;
    }
  | {
      kind: "user_input";
      client_action_id: ClientActionId;
      input_id: number;
      input_source: "root" | "pending_in_turn" | "queued_scheduled" | "agent_message";
      text: string;
      resource_ids?: ResourceId[];
    }
  | {
      kind: "queued_input_resolution";
      client_action_id: ClientActionId;
      input_id: number;
resolution: "moved_to_draft" | "removed" | "promoted_to_pending_in_turn";
      text: string;
      resource_ids?: number[];
    }
  | {
      kind: "api_started";
      api_call_id: number;
      purpose: ApiPurposeKind;
      request: ApiRequestSnapshot;
    }
  | {
      kind: "assistant_reasoning";
      api_call_id: number;
      text: string;
      status: "streaming" | "completed" | "interrupted";
      provider_source?: ProviderSource;
      /** Provider summary text: displayed like thinking, never replayed. 供应商摘要：按思考展示，绝不重放。 */
      display_only?: boolean;
    }
  | {
      kind: "assistant_text";
      api_call_id: number;
      text: string;
      status: "streaming" | "completed" | "interrupted";
      provider_source?: ProviderSource;
      code_snapshots?: AnswerCodeSnapshot[];
    }
  | {
      kind: "tool_call";
      api_call_id: number;
      tool_call_id: number;
      tool_name: string;
      input: ToolPayload;
      output?: ToolPayload;
      changeset?: PersistentChangeSet;
      status: "prepared" | "running" | "waiting_user" | "completed" | "failed" | "cancelled" | "stopped" | "interrupted";
      started_at_ms?: number;
      finished_at_ms?: number;
      user_wait_started_at_ms?: number;
      user_wait_ms?: number;
      request_stream_id: string;
      provider_tool_call_id?: string;
      external_refs?: ExternalResourceRef[];
      generated_resources?: GeneratedResourceRef[];
      provider_source?: ProviderSource;
    }
  | {
      kind: "api_stats";
      api_call_id: number;
      purpose: ApiPurposeKind;
      provider: string;
      profile?: string;
      model: string;
      connection_id?: string;
      thinking_level?: string;
      temperature?: number;
      usage: UsageSnapshot;
      status: "prepared" | "dispatched" | "in_progress" | "completed" | "failed" | "cancelled" | "interrupted";
      end_reason?:
        | "completed_successfully"
        | "provider_error"
        | "network_error"
        | "balance_insufficient"
        | "user_interrupted"
        | "process_interrupted"
        | "retry_exhausted"
        | "cancelled"
        | "unknown";
      error?: RedactedApiError;
      /** Frozen policy decision for explicit continuation of this physical API.
       * 当前物理 API 是否允许显式继续的冻结策略判定。 */
      continuation_eligibility: "allowed" | "forbidden";
      timing: ApiTimings;
    }
  | {
      kind: "turn_boundary";
      /** Explicit host closure classification, independent of the aggregate stopped outcome.
       * 明确宿主闭合分类，独立于聚合停止结果。 */
      reason: TurnClosureReason;
      outcome: "completed" | "abandoned" | "stopped";
      /** Durable purpose distinguishes business completion from auxiliary questions and maintenance.
       * 持久化用途区分业务完成、辅助答疑和维护。 */
      purpose: "business" | "question_execution" | "context_maintenance";
    }
  | {
      kind: "turn_stats";
      start_seq: number;
      end_seq: number;
      outcome: "completed" | "abandoned" | "stopped";
      duration_ms?: number;
      buckets: TurnStatBuckets;
    }
  | {
      kind: "subagent_return";
      /** Exact generation owning the source sequence range.
       * 拥有来源序号范围的精确代次。 */
      child_segment_generation: number;
      /** Original root action of the returned execution.
       * 回流执行的原根动作。 */
      child_root_action_id: string;
      child_session_id: string;
      child_task_id: string;
      content: string;
      source_assistant_text_seq: number;
      source_from_seq: number;
      source_to_seq: number;
      child_turn_stats_seq: number;
    }
  | {
      kind: "display_note";
      notice_kind:
        | "provider_error"
        | "system_error"
        | "network_disconnected"
        | "balance_insufficient"
        | "retry_started"
        | "retry_exhausted"
        | "resumed"
        | "compaction_completed"
        | "goal_status"
        | "interrupted"
        | "stopped"
        | "memory_locked"
        | "memory_unlocked";
      severity: "info" | "warning" | "error";
      reason_code: string;
      redacted_detail?: string;
      api_call_id?: number;
      retry_count?: number;
      retry_limit?: number;
    }
  | {
      /** UI-only record of the long-term memory recall that ran for one user message.
       * 针对一条用户消息执行的长期记忆召回的仅 UI 记录。 */
      kind: "memory_recall";
      outcome: "matched" | "no_match" | "failed";
      queries?: string[];
      memories?: { memory_id: number; text: string }[];
    }
  | {
      kind: "request_config_changed";
      previous: ActiveRequestConfig;
      current: ActiveRequestConfig;
    }
  | {
      kind: "execution_control";
      control_kind:
        | "execution_interrupted"
        | "execution_retry_requested"
        | "execution_retry_exhausted"
        | "turn_continue_requested"
        | "turn_resumed"
        | "api_transport_observed"
        | "recovered";
      api_call_id?: number;
      client_action_id?: ClientActionId;
    }
  | CollaborationEvent
  | ({ kind: "context_maintenance" } & ContextMaintenanceExecution);

/** Stable event kind derived from the discriminated event body.
 * 从判别事件体派生的稳定事件种类。 */
export type ConversationEventKind = ConversationEventBody["kind"];

/** Immutable identity used to compare revisions of one persisted event.
 * 用于比较同一持久化事件修订的不可变身份。 */
export type EventIdentity = string;

/** Complete event shared by hydration, pagination, and live updates.
 * hydration、分页与实时更新共享的完整事件。 */
export interface ConversationEventSummary {
  seq: number;
  turn_id: number | null;
  created_at_ms: number;
  updated_at_ms: number;
  event_revision: number;
  visibility: ConversationEventVisibility;
  event: ConversationEventBody;
}

/** Ownership of one authoritative conversation event window.
 * 一个权威会话事件窗口的归属信息。 */
export interface ConversationEventWindowOwner {
  session_id: string;
  window_generation: number;
}

/** One ownership-bound event batch accepted by an explicit window boundary.
 * 由显式窗口边界接收的一批带归属事件。 */
export interface ConversationEventWindowBatch extends ConversationEventWindowOwner {
  events: readonly ConversationEventSummary[];
}

/** Owner-generation document frontiers captured by hydration or rebase.
 * hydration 或 rebase 捕获的归属代际文档前沿。 */
export interface ConversationEventWindowFrontier {
  /** Stable identity of the service process owning this frontier.
   * 拥有该前沿的服务进程稳定身份。 */
  service_epoch: string;
  /** Latest authoritative in-memory document revision.
   * 最新权威内存文档修订号。 */
  memory_revision: number;
  /** Latest whole-file revision confirmed durable.
   * 最新已确认整文件可靠落盘的修订号。 */
  durable_revision: number;
}

/** One complete ownership baseline used only by hydration and rebase.
 * 仅供 hydration 与 rebase 使用的一份完整归属基线。 */
export interface ConversationEventWindowBaseline
  extends ConversationEventWindowBatch,
    ConversationEventWindowFrontier {}

/** Durability represented by one revisioned live transport patch.
 * 一份带修订实时传输 patch 所表示的持久化状态。 */
export type ConversationEventPatchDurability = "volatile" | "durable";

/** One ownership-bound revisioned sparse live patch whose events remain strictly seq-ascending.
 * 一份带归属与修订的稀疏实时 patch，其中事件仍须按 seq 严格递增。 */
export interface ConversationEventLivePatch extends ConversationEventWindowBatch {
  /** Confirmed answer mode carried only by durable acknowledgements.
   * 仅可靠确认携带的已确认回答模式。 */
  answer_mode?: AnswerModePreference;
  /** Confirmed session access carried only by durable acknowledgements.
   * 仅可靠确认携带的已确认会话授权。 */
  access?: ConversationSessionAccessV1;
  /** Stable identity of the service process that emitted this edge.
   * 发出该边的服务进程稳定身份。 */
  service_epoch: string;
  /** Exact owner-generation document revision represented by this patch.
   * 此 patch 表示的精确归属代际文档修订号。 */
  revision: number;
  /** Exact predecessor of a volatile memory range; absent for durable acknowledgements.
   * A merged volatile patch spans `base_memory_revision..revision`.
   * volatile 内存区间的精确前驱；durable 确认中不存在。合并后的 volatile patch 跨越该区间。 */
  base_memory_revision?: number;
  /** Exact predecessor of a durable edge; absent for volatile memory patches.
   * durable 边的精确前驱；volatile 内存 patch 中不存在。 */
  base_durable_revision?: number;
  /** Whether this revision is memory-visible or durably published.
   * 此修订是仅内存可见还是已经可靠发布。 */
  durability: ConversationEventPatchDurability;
  /** Seq-ascending text appends to streaming events that are not repeated in `events`.
   * 对流式事件的正文追加，按 seq 递增，且不与 `events` 重复。 */
  text_appends?: ConversationEventTextAppend[];
}

/** One append to the text of a streaming `assistant_text` or `assistant_reasoning` event.
 * 对一条流式 `assistant_text` 或 `assistant_reasoning` 事件正文的一次追加。 */
export interface ConversationEventTextAppend {
  /** Event whose text grows.
   * 正文增长的事件。 */
  seq: number;
  /** Event revision after the append.
   * 追加后的事件修订号。 */
  event_revision: number;
  /** Update time after the append.
   * 追加后的更新时间。 */
  updated_at_ms: number;
  /** UTF-16 length of the text the append starts from.
   * 追加起点正文的 UTF-16 长度。 */
  base_text_utf16_len: number;
  /** Text appended at that offset.
   * 在该偏移处追加的正文。 */
  appended_text: string;
}

/** One authoritative reconnect range that may cover multiple memory revisions.
 * 一段可覆盖多个内存修订的权威重连范围。 */
export interface ConversationEventReconnectPatch extends ConversationEventWindowBatch {
  /** Persisted answer mode captured with reconnect storage state.
   * 与重连存储状态共同捕获的持久回答模式。 */
  answer_mode?: AnswerModePreference;
  /** Persisted session access captured with the same storage state.
   * 与同一存储状态共同捕获的持久会话授权。 */
  access?: ConversationSessionAccessV1;
  /** Stable identity of the service process that produced the recovery range.
   * 生成该恢复范围的服务进程稳定身份。 */
  service_epoch: string;
  /** Client memory frontier used to request this recovery range.
   * 请求该恢复范围时客户端使用的内存前沿。 */
  base_memory_revision: number;
  /** Authoritative memory frontier represented by the complete recovery range.
   * 完整恢复范围表示的权威内存前沿。 */
  memory_revision: number;
  /** Authoritative durable frontier captured with the same recovery range.
   * 与同一恢复范围一起捕获的权威可靠前沿。 */
  durable_revision: number;
}

/** Explicit revision-edge mismatch that requires reconnect or rebase.
 * 需要重连或重建的显式修订边不匹配。 */
export class ConversationEventRevisionGapError extends Error {
  /** Frontier whose predecessor did not match.
   * 前驱不匹配的前沿类型。 */
  readonly frontier: "memory" | "durable";
  /** Exact predecessor currently held by the store.
   * store 当前持有的精确前驱。 */
  readonly expected_base_revision: number;
  /** Predecessor declared by the rejected patch.
   * 被拒绝 patch 声明的前驱。 */
  readonly actual_base_revision: number;
  /** Target revision declared by the rejected patch.
   * 被拒绝 patch 声明的目标修订。 */
  readonly target_revision: number;

  /** Creates one structured revision-gap failure.
   * 创建一条结构化修订缺口失败。 */
  constructor(
    frontier: "memory" | "durable",
    expectedBaseRevision: number,
    actualBaseRevision: number,
    targetRevision: number,
  ) {
    super(
      `${frontier} revision gap: expected base ${expectedBaseRevision}, got ${actualBaseRevision} for target ${targetRevision}`,
    );
    this.name = "ConversationEventRevisionGapError";
    this.frontier = frontier;
    this.expected_base_revision = expectedBaseRevision;
    this.actual_base_revision = actualBaseRevision;
    this.target_revision = targetRevision;
  }
}

/** Explicit service-process identity mismatch that requires durable re-hydration.
 * 需要可靠重新 hydration 的显式服务进程身份不匹配。 */
export class ConversationEventServiceEpochMismatchError extends Error {
  /** Service epoch currently owned by the store.
   * store 当前拥有的服务代次。 */
  readonly expected_service_epoch: string;
  /** Service epoch carried by the rejected response or push.
   * 被拒绝响应或推送携带的服务代次。 */
  readonly actual_service_epoch: string;

  /** Creates one structured service-epoch mismatch.
   * 创建一条结构化服务代次不匹配。 */
  constructor(expectedServiceEpoch: string, actualServiceEpoch: string) {
    super(
      `conversation event service epoch mismatch: expected ${expectedServiceEpoch}, got ${actualServiceEpoch}`,
    );
    this.name = "ConversationEventServiceEpochMismatchError";
    this.expected_service_epoch = expectedServiceEpoch;
    this.actual_service_epoch = actualServiceEpoch;
  }
}

/** Event-window patch failure.
 * 事件窗口补丁失败。 */
export type WindowPatchError =
  | {
      code: "overlap_conflict";
      seq: number;
      expected_kind: ConversationEventKind | null;
      actual_kind: ConversationEventKind;
    }
  | { code: "gap"; expected: number; actual: number }
  | { code: "owner_uninitialized" }
  | { code: "owner_already_initialized"; session_id: string; window_generation: number }
  | { code: "session_mismatch"; expected: string; actual: string }
  | { code: "generation_mismatch"; expected: number; actual: number }
  | { code: "generation_not_advanced"; current: number; actual: number }
  | { code: "illegal_evolution"; seq: number; kind: ConversationEventKind; from: string; to: string };

/** Asserts that one persisted integer remains exact in JavaScript.
 * 断言一个持久化整数在 JavaScript 中仍可精确表示。 */
export function assertSafeSeq(seq: number): void {
  if (!Number.isSafeInteger(seq) || seq < 1) {
    throw new Error(`unsafe event seq: ${seq}`);
  }
}

/** Asserts that one event revision remains a positive exact JavaScript integer.
 * 断言一条事件修订号保持为 JavaScript 可精确表示的正整数。 */
export function assertSafeEventRevision(eventRevision: number): void {
  if (!Number.isSafeInteger(eventRevision) || eventRevision < 1) {
    throw new Error(`unsafe event revision: ${eventRevision}`);
  }
}

/** Asserts that one document frontier revision is a non-negative exact JavaScript integer.
 * 断言一个文档前沿修订号是 JavaScript 可精确表示的非负整数。 */
export function assertSafeDocumentRevision(revision: number): void {
  if (!Number.isSafeInteger(revision) || revision < 0) {
    throw new Error(`unsafe document revision: ${revision}`);
  }
}

/** Asserts that one window generation remains exact and positive in JavaScript.
 * 断言一个窗口代际在 JavaScript 中保持精确且为正数。 */
export function assertSafeWindowGeneration(windowGeneration: number): void {
  if (!Number.isSafeInteger(windowGeneration) || windowGeneration < 1) {
    throw new Error(`unsafe window generation: ${windowGeneration}`);
  }
}

/** Formats one event-window patch error for diagnostics.
 * 把一条事件窗口补丁错误格式化为诊断文本。 */
export function formatWindowPatchError(error: WindowPatchError): string {
  switch (error.code) {
    case "gap":
      return `event window gap: expected seq ${error.expected}, got ${error.actual}`;
    case "overlap_conflict":
      return `identity conflict at seq ${error.seq}: ${error.actual_kind} overlaps ${error.expected_kind ?? "null"}`;
    case "owner_uninitialized":
      return "event window owner is not initialized";
    case "owner_already_initialized":
      return `event window owner is already initialized for session ${error.session_id} generation ${error.window_generation}`;
    case "session_mismatch":
      return `event window session mismatch: expected ${error.expected}, got ${error.actual}`;
    case "generation_mismatch":
      return `event window generation mismatch: expected ${error.expected}, got ${error.actual}`;
    case "generation_not_advanced":
      return `event window rebase generation must advance beyond ${error.current}, got ${error.actual}`;
    case "illegal_evolution":
      return `illegal ${error.kind} evolution at seq ${error.seq}: ${error.from} -> ${error.to}`;
  }
}

/** V1 window keyed and rendered only by the canonical event seq.
 * 仅按 canonical 事件 seq 建键与渲染的 V1 窗口。 */
export class ConversationEventWindowStore {
  /** Loaded event window indexed by seq.
   * 按 seq 索引的已加载事件窗口。 */
  private events = new Map<number, ConversationEventSummary>();

  /** Current authoritative window owner.
   * 当前权威窗口归属。 */
  private owner: ConversationEventWindowOwner | null = null;

  /** Earliest loaded seq.
   * 已加载的最早 seq。 */
  private earliest: number | null = null;

  /** Latest loaded seq.
   * 已加载的最晚 seq。 */
  private latest: number | null = null;

  /** Stable identity of the service process owning the current frontier.
   * 拥有当前前沿的服务进程稳定身份。 */
  private serviceEpoch: string | null = null;

  /** Current authoritative in-memory revision for the owner generation.
   * 当前归属代际的权威内存修订号。 */
  private memoryRevision: number | null = null;

  /** Current confirmed durable revision for the owner generation.
   * 当前归属代际已确认的可靠修订号。 */
  private durableRevision: number | null = null;

  /** Returns a defensive copy of the current owner.
   * 返回当前归属信息的防御性副本。 */
  getOwner(): ConversationEventWindowOwner | null {
    return this.owner === null ? null : { ...this.owner };
  }

  /** Returns the earliest loaded seq.
   * 返回已加载的最早 seq。 */
  getEarliestSeq(): number | null {
    return this.earliest;
  }

  /** Returns the latest loaded seq.
   * 返回已加载的最晚 seq。 */
  getLatestSeq(): number | null {
    return this.latest;
  }

  /** Returns a defensive copy of the current owner-generation revision frontiers.
   * 返回当前归属代际修订前沿的防御性副本。 */
  getFrontier(): ConversationEventWindowFrontier | null {
    if (
      this.serviceEpoch === null ||
      this.memoryRevision === null ||
      this.durableRevision === null
    ) return null;
    return {
      service_epoch: this.serviceEpoch,
      memory_revision: this.memoryRevision,
      durable_revision: this.durableRevision,
    };
  }

  /** Returns the loaded event count.
   * 返回已加载事件数量。 */
  size(): number {
    return this.events.size;
  }

  /** Returns a seq-ascending event snapshot.
   * 返回按 seq 升序排列的事件快照。 */
  toAscendingArray(): ConversationEventSummary[] {
    return Array.from(this.events.values());
  }

  /** Establishes the first ownership-bound hydration snapshot atomically.
   * 原子建立首个带归属的 hydration 快照。 */
  hydrate(batch: ConversationEventWindowBaseline): void {
    if (this.owner !== null) {
      throw new Error(
        formatWindowPatchError({
          code: "owner_already_initialized",
          session_id: this.owner.session_id,
          window_generation: this.owner.window_generation,
        }),
      );
    }
    const replacement = buildConsecutiveWindow(batch);
    this.commitReplacement(batch, replacement, batch);
  }

  /** Prepends one adjacent historical page for the current owner atomically.
   * 为当前归属原子前插一页相邻历史。 */
  prependPage(batch: ConversationEventWindowBatch): void {
    this.assertCurrentOwner(batch);
    const page = buildConsecutiveWindow(batch);
    if (page.size === 0) {
      return;
    }
    if (this.earliest === null) {
      throw new Error(formatWindowPatchError({ code: "gap", expected: 1, actual: batch.events[0].seq }));
    }
    const pageLatest = batch.events[batch.events.length - 1].seq;
    assertConsecutiveAfter(pageLatest, this.earliest);
    const replacement = new Map<number, ConversationEventSummary>();
    for (const [seq, summary] of page) {
      replacement.set(seq, summary);
    }
    for (const [seq, summary] of this.events) {
      replacement.set(seq, summary);
    }
    this.commitEvents(replacement);
  }

  /** Applies one sparse live patch atomically after validating identity and loaded-window gaps.
   * 在校验身份与已加载窗口缺口后原子应用一份稀疏实时 patch。 */
  applyLive(patch: ConversationEventLivePatch): void {
    this.assertCurrentOwner(patch);
    const frontier = this.requireFrontier();
    if (patch.service_epoch !== frontier.service_epoch) {
      throw new ConversationEventServiceEpochMismatchError(
        frontier.service_epoch,
        patch.service_epoch,
      );
    }
    assertSafeDocumentRevision(patch.revision);

    if (patch.durability === "volatile") {
      if (patch.base_memory_revision === undefined) {
        throw new Error("volatile event patch is missing base_memory_revision");
      }
      if (patch.base_durable_revision !== undefined) {
        throw new Error("volatile event patch must not declare base_durable_revision");
      }
      assertSafeDocumentRevision(patch.base_memory_revision);
      if (patch.revision <= patch.base_memory_revision) {
        throw new Error(
          `volatile event patch must advance its base: base ${patch.base_memory_revision}, target ${patch.revision}`,
        );
      }
      if (patch.revision < frontier.memory_revision) return;
      const sameRevision = patch.revision === frontier.memory_revision;
      // A merged range may start before the frontier; its events are the state at `revision`.
      // 合并区间可能起始于前沿之前；其事件即 `revision` 处的状态。
      if (!sameRevision && patch.base_memory_revision > frontier.memory_revision) {
        throw new ConversationEventRevisionGapError(
          "memory",
          frontier.memory_revision,
          patch.base_memory_revision,
          patch.revision,
        );
      }
      const replacement = this.buildLiveReplacement(
        patch,
        sameRevision,
        patch.revision,
        patch.text_appends ?? [],
        patch.base_memory_revision === frontier.memory_revision,
      );
      this.commitEventsWithFrontier(
        replacement,
        sameRevision ? frontier.memory_revision : patch.revision,
        frontier.durable_revision,
      );
      return;
    }

    if (patch.base_durable_revision === undefined) {
      throw new Error("durable acknowledgement is missing base_durable_revision");
    }
    if (patch.base_memory_revision !== undefined) {
      throw new Error("durable acknowledgement must not declare base_memory_revision");
    }
    assertSafeDocumentRevision(patch.base_durable_revision);
    if (patch.events.length !== 0) {
      throw new Error("durable acknowledgement must not carry events");
    }
    if (patch.revision <= patch.base_durable_revision) {
      throw new Error(
        `durable acknowledgement must advance its base: base ${patch.base_durable_revision}, target ${patch.revision}`,
      );
    }
    if (patch.text_appends !== undefined && patch.text_appends.length !== 0) {
      throw new Error("durable acknowledgement must not carry text appends");
    }
    if (patch.revision <= frontier.durable_revision) return;
    if (patch.base_durable_revision !== frontier.durable_revision) {
      throw new ConversationEventRevisionGapError(
        "durable",
        frontier.durable_revision,
        patch.base_durable_revision,
        patch.revision,
      );
    }
    if (patch.revision > frontier.memory_revision) {
      throw new Error(
        `durable revision ${patch.revision} exceeds memory revision ${frontier.memory_revision}`,
      );
    }
    this.commitEventsWithFrontier(
      new Map(this.events),
      frontier.memory_revision,
      patch.revision,
    );
  }

  /** Applies one authoritative reconnect range whose explicit base may span multiple revisions.
   * 应用一段带显式前驱、可跨越多个修订的权威重连范围。 */
  applyReconnect(patch: ConversationEventReconnectPatch): void {
    this.assertCurrentOwner(patch);
    assertSafeDocumentRevision(patch.base_memory_revision);
    assertWindowFrontier(patch);
    const frontier = this.requireFrontier();
    if (patch.service_epoch !== frontier.service_epoch) {
      throw new ConversationEventServiceEpochMismatchError(
        frontier.service_epoch,
        patch.service_epoch,
      );
    }
    if (patch.memory_revision < patch.base_memory_revision) {
      throw new Error(
        `reconnect memory revision ${patch.memory_revision} precedes base ${patch.base_memory_revision}`,
      );
    }
    if (patch.memory_revision < frontier.memory_revision) return;
    if (patch.base_memory_revision !== frontier.memory_revision) {
      throw new ConversationEventRevisionGapError(
        "memory",
        frontier.memory_revision,
        patch.base_memory_revision,
        patch.memory_revision,
      );
    }
    if (patch.durable_revision < frontier.durable_revision) {
      throw new Error(
        `reconnect durable revision rollback: current ${frontier.durable_revision}, actual ${patch.durable_revision}`,
      );
    }
    const sameRevision = patch.memory_revision === frontier.memory_revision;
    const replacement = this.buildLiveReplacement(
      patch,
      sameRevision,
      patch.memory_revision,
    );
    this.commitEventsWithFrontier(
      replacement,
      patch.memory_revision,
      patch.durable_revision,
    );
  }

  /** Builds one validated live replacement without publishing events or frontiers.
   * `exactTextBase` states that loaded events are exactly at the base of `textAppends`.
   * 构建一份已校验的实时替换，但不发布事件或前沿。`exactTextBase` 表示已加载事件恰好处于
   * `textAppends` 的基线。 */
  private buildLiveReplacement(
    patch: ConversationEventWindowBatch,
    requireIdempotent: boolean,
    documentRevision: number,
    textAppends: readonly ConversationEventTextAppend[] = [],
    exactTextBase = false,
  ): Map<number, ConversationEventSummary> {
    let validatedLatest = this.latest;
    const pending = new Map<number, ConversationEventSummary>();
    let appended = false;
    let previousBatchSeq: number | null = null;
    for (const summary of patch.events) {
      assertSafeSeq(summary.seq);
      assertSafeEventRevision(summary.event_revision);
      if (previousBatchSeq !== null && summary.seq <= previousBatchSeq) {
        throw new Error(
          formatWindowPatchError({
            code: "gap",
            expected: previousBatchSeq + 1,
            actual: summary.seq,
          }),
        );
      }
      previousBatchSeq = summary.seq;

      // A sparse patch may mention changed events older than the bounded hydration window.
      // 稀疏 patch 可能携带早于有界 hydration 窗口的已变更事件。
      if (this.earliest !== null && summary.seq < this.earliest && !this.events.has(summary.seq)) {
        continue;
      }

      const existing = pending.get(summary.seq) ?? this.events.get(summary.seq);
      if (existing !== undefined) {
        if (appended) {
          throw new Error(
            formatWindowPatchError({
              code: "gap",
              expected: (validatedLatest ?? 0) + 1,
              actual: summary.seq,
            }),
          );
        }
        if (requireIdempotent && summary.event_revision > existing.event_revision) {
          throw new Error(
            `document revision ${documentRevision} conflicts with newer event revision ${summary.event_revision} at seq ${summary.seq}`,
          );
        }
        pending.set(summary.seq, validateSameSeqEvolution(existing, summary));
        continue;
      }
      if (requireIdempotent) {
        throw new Error(
          `document revision ${documentRevision} cannot append unknown seq ${summary.seq}`,
        );
      }
      const expectedSeq = validatedLatest === null ? 1 : validatedLatest + 1;
      assertExactSeq(expectedSeq, summary.seq);
      validatedLatest = summary.seq;
      appended = true;
      pending.set(summary.seq, summary);
    }
    let previousAppendSeq: number | null = null;
    for (const append of textAppends) {
      assertSafeSeq(append.seq);
      if (previousAppendSeq !== null && append.seq <= previousAppendSeq) {
        throw new Error(`text appends must be seq-ascending: ${previousAppendSeq} then ${append.seq}`);
      }
      previousAppendSeq = append.seq;
      if (pending.has(append.seq)) {
        throw new Error(`text append at seq ${append.seq} repeats a complete event`);
      }
      const existing = this.events.get(append.seq);
      if (existing === undefined) {
        // Like a complete event, an append may target history older than the loaded window.
        // 与完整事件相同，追加可能指向早于已加载窗口的历史。
        if (this.earliest !== null && append.seq < this.earliest) continue;
        throw new Error(`text append targets unknown seq ${append.seq}`);
      }
      if (requireIdempotent && append.event_revision > existing.event_revision) {
        throw new Error(
          `document revision ${documentRevision} conflicts with newer event revision ${append.event_revision} at seq ${append.seq}`,
        );
      }
      pending.set(
        append.seq,
        validateSameSeqEvolution(existing, resolveTextAppend(existing, append, exactTextBase)),
      );
    }
    const replacement = new Map(this.events);
    for (const [seq, summary] of pending) replacement.set(seq, summary);
    return replacement;
  }

  /** Atomically replaces the complete loaded window without exposing an empty intermediate state.
   * 原子替换完整已加载窗口，且不暴露空白中间状态。 */
  rebase(batch: ConversationEventWindowBaseline): void {
    if (this.owner === null) {
      throw new Error(formatWindowPatchError({ code: "owner_uninitialized" }));
    }
    assertWindowOwner(batch);
    if (
      batch.session_id === this.owner.session_id &&
      batch.window_generation <= this.owner.window_generation
    ) {
      throw new Error(
        formatWindowPatchError({
          code: "generation_not_advanced",
          current: this.owner.window_generation,
          actual: batch.window_generation,
        }),
      );
    }
    const replacement = buildConsecutiveWindow(batch);
    this.commitReplacement(batch, replacement, batch);
  }

  /** Replaces the current owner from a durable hydration after the service instance changes.
   * 在服务实例变化后，以可靠 hydration 原子替换当前归属。 */
  restoreDurableBaseline(batch: ConversationEventWindowBaseline): void {
    if (this.owner === null) {
      throw new Error(formatWindowPatchError({ code: "owner_uninitialized" }));
    }
    assertWindowOwner(batch);
    if (batch.session_id !== this.owner.session_id) {
      throw new Error(
        formatWindowPatchError({
          code: "session_mismatch",
          expected: this.owner.session_id,
          actual: batch.session_id,
        }),
      );
    }
    const replacement = buildConsecutiveWindow(batch);
    this.commitReplacement(batch, replacement, batch);
  }

  /** Returns the sole render order: ascending canonical seq.
   * 返回唯一渲染顺序：canonical seq 升序。 */
  render(): readonly ConversationEventSummary[] {
    return this.toAscendingArray();
  }

  /** Verifies that a page or live batch belongs to the active window.
   * 校验分页或实时批次属于当前活动窗口。 */
  private assertCurrentOwner(owner: ConversationEventWindowOwner): void {
    if (this.owner === null) {
      throw new Error(formatWindowPatchError({ code: "owner_uninitialized" }));
    }
    assertWindowOwner(owner);
    if (owner.session_id !== this.owner.session_id) {
      throw new Error(
        formatWindowPatchError({
          code: "session_mismatch",
          expected: this.owner.session_id,
          actual: owner.session_id,
        }),
      );
    }
    if (owner.window_generation !== this.owner.window_generation) {
      throw new Error(
        formatWindowPatchError({
          code: "generation_mismatch",
          expected: this.owner.window_generation,
          actual: owner.window_generation,
        }),
      );
    }
  }

  /** Publishes one already validated replacement in a single synchronous state swap.
   * 通过一次同步状态交换发布已校验的替换窗口。 */
  private commitReplacement(
    owner: ConversationEventWindowOwner,
    replacement: Map<number, ConversationEventSummary>,
    frontier: ConversationEventWindowFrontier,
  ): void {
    const bounds = validatedIterationBounds(replacement);
    assertWindowFrontier(frontier);
    this.owner = { session_id: owner.session_id, window_generation: owner.window_generation };
    this.events = replacement;
    this.earliest = bounds.earliest;
    this.latest = bounds.latest;
    this.serviceEpoch = frontier.service_epoch;
    this.memoryRevision = frontier.memory_revision;
    this.durableRevision = frontier.durable_revision;
  }

  /** Publishes one same-owner replacement after validating its iteration invariant.
   * 校验迭代顺序不变量后发布同一归属下的替换。*/
  private commitEvents(replacement: Map<number, ConversationEventSummary>): void {
    const bounds = validatedIterationBounds(replacement);
    this.events = replacement;
    this.earliest = bounds.earliest;
    this.latest = bounds.latest;
  }

  /** Publishes one same-owner event replacement and its validated frontiers together.
   * 同步发布同一归属的事件替换及其已校验前沿。 */
  private commitEventsWithFrontier(
    replacement: Map<number, ConversationEventSummary>,
    memoryRevision: number,
    durableRevision: number,
  ): void {
    const bounds = validatedIterationBounds(replacement);
    assertWindowFrontier({
      service_epoch: this.requireFrontier().service_epoch,
      memory_revision: memoryRevision,
      durable_revision: durableRevision,
    });
    this.events = replacement;
    this.earliest = bounds.earliest;
    this.latest = bounds.latest;
    this.memoryRevision = memoryRevision;
    this.durableRevision = durableRevision;
  }

  /** Returns initialized revision frontiers or reports an explicit lifecycle misuse.
   * 返回已初始化的修订前沿，或显式报告生命周期误用。 */
  private requireFrontier(): ConversationEventWindowFrontier {
    const frontier = this.getFrontier();
    if (frontier === null) throw new Error("conversation event window frontier is not initialized");
    return frontier;
  }
}

/** Validates exact owner-generation revision frontiers before atomic publication.
 * 在原子发布前校验精确的归属代际修订前沿。 */
function assertWindowFrontier(frontier: ConversationEventWindowFrontier): void {
  if (frontier.service_epoch.trim().length === 0) {
    throw new Error("conversation event service epoch must not be empty");
  }
  assertSafeDocumentRevision(frontier.memory_revision);
  assertSafeDocumentRevision(frontier.durable_revision);
  if (frontier.durable_revision > frontier.memory_revision) {
    throw new Error(
      `durable revision ${frontier.durable_revision} exceeds memory revision ${frontier.memory_revision}`,
    );
  }
}

/** Returns the first and last seq after proving Map iteration is strictly consecutive.
 * 在证明 Map 迭代顺序严格连续后返回首尾 seq。*/
function validatedIterationBounds(
  events: ReadonlyMap<number, ConversationEventSummary>,
): { earliest: number | null; latest: number | null } {
  let earliest: number | null = null;
  let latest: number | null = null;
  for (const [seq, summary] of events) {
    assertSafeSeq(seq);
    assertSafeEventRevision(summary.event_revision);
    if (summary.seq !== seq) {
      throw new Error(
        formatWindowPatchError({
          code: "overlap_conflict",
          seq,
          expected_kind: summary.event.kind,
          actual_kind: summary.event.kind,
        }),
      );
    }
    if (latest !== null) assertConsecutiveAfter(latest, seq);
    earliest ??= seq;
    latest = seq;
  }
  return { earliest, latest };
}

/** Rejects same-seq updates that target a different semantic entity.
 * 拒绝同一 seq 指向不同语义实体的更新。 */
function rejectIdentityConflict(
  existing: ConversationEventSummary,
  incoming: ConversationEventSummary,
): void {
  const identityMismatch = eventIdentity(existing) !== eventIdentity(incoming);
  const immutableMismatch =
    existing.turn_id !== incoming.turn_id ||
    existing.created_at_ms !== incoming.created_at_ms ||
    existing.visibility !== incoming.visibility;
  if (identityMismatch || immutableMismatch) {
    throw new Error(
      formatWindowPatchError({
        code: "overlap_conflict",
        seq: incoming.seq,
        expected_kind: existing.event.kind,
        actual_kind: incoming.event.kind,
      }),
    );
  }
}

/** Validates one same-seq event evolution exclusively through its event revision and body.
 * 仅通过事件修订号与事件正文校验同 seq 事件演进。 */
function validateSameSeqEvolution(
  existing: ConversationEventSummary,
  incoming: ConversationEventSummary,
): ConversationEventSummary {
  rejectIdentityConflict(existing, incoming);
  assertSafeEventRevision(existing.event_revision);
  assertSafeEventRevision(incoming.event_revision);
  if (incoming.event_revision < existing.event_revision) {
    return existing;
  }
  const bodyUnchanged = canonicalJson(existing.event) === canonicalJson(incoming.event);
  if (incoming.event_revision === existing.event_revision) {
    if (!bodyUnchanged) {
      throwIllegalEvolution(existing, incoming, "same_revision", "conflicting_revision");
    }
    return existing;
  }
  if (bodyUnchanged) {
    return incoming;
  }
  assertMutableEventEvolution(existing, incoming);
  return incoming;
}

/** Resolves one text append against the loaded event into the complete event it describes.
 * With `exactBase`, the loaded text must be exactly the append's base; otherwise it may already
 * include part or all of the append, as when a merged range starts before the loaded frontier.
 * 将一次正文追加对照已加载事件解析为其描述的完整事件。`exactBase` 时已加载正文必须恰为追加
 * 基线；否则它可能已包含部分或全部追加，例如合并区间起始于已加载前沿之前。 */
function resolveTextAppend(
  existing: ConversationEventSummary,
  append: ConversationEventTextAppend,
  exactBase: boolean,
): ConversationEventSummary {
  assertSafeEventRevision(append.event_revision);
  const base = append.base_text_utf16_len;
  if (!Number.isSafeInteger(base) || base < 0) {
    throw new Error(`text append at seq ${append.seq} has invalid base length ${String(base)}`);
  }
  if (typeof append.appended_text !== "string" || append.appended_text.length === 0) {
    throw new Error(`text append at seq ${append.seq} must append text`);
  }
  const body = existing.event;
  if (body.kind !== "assistant_text" && body.kind !== "assistant_reasoning") {
    throw new Error(`text append at seq ${append.seq} targets ${body.kind}`);
  }
  // JavaScript string lengths and offsets are UTF-16 code units, the unit of the base length.
  // JavaScript 字符串长度与偏移以 UTF-16 码元计量，与基线长度单位一致。
  const text = body.text.slice(0, base) + append.appended_text;
  if (existing.event_revision >= append.event_revision) {
    if (!body.text.startsWith(text)) {
      throw new Error(`text append at seq ${append.seq} disagrees with newer loaded text`);
    }
    return existing;
  }
  const lengthMatches = exactBase
    ? body.text.length === base
    : body.text.length >= base && text.startsWith(body.text);
  if (body.status !== "streaming" || !lengthMatches) {
    throw new Error(
      `text append at seq ${append.seq} expects base length ${base}, loaded ${body.status} text has ${body.text.length}`,
    );
  }
  return {
    ...existing,
    event_revision: append.event_revision,
    updated_at_ms: append.updated_at_ms,
    event: { ...body, text },
  };
}

/** Enforces the explicit state matrix for event kinds that may evolve in place.
 * 对允许原位演进的事件类型执行显式状态矩阵。 */
function assertMutableEventEvolution(
  existing: ConversationEventSummary,
  incoming: ConversationEventSummary,
): void {
  const current = existing.event;
  const next = incoming.event;
  if (current.kind !== next.kind) {
    rejectIdentityConflict(existing, incoming);
    return;
  }
  switch (current.kind) {
    case "assistant_reasoning":
    case "assistant_text":
      if (next.kind === current.kind) {
        // allowed preserves streaming progress and permits one terminal Provider-source attachment revision.
        // allowed 保留流式进度，并允许终态追加一次 Provider 来源证明修订。
        const allowed = current.status === "streaming"
          ? ["streaming", "completed", "interrupted"]
          : [current.status];
        assertAllowedTransition(
          incoming,
          current.status,
          next.status,
          allowed,
        );
        assertAssistantEvolution(existing, incoming);
        return;
      }
      break;
    case "tool_call":
      if (next.kind === "tool_call") {
        assertToolImmutableFields(existing, incoming);
        const allowed =
          current.status === "prepared"
            ? ["prepared", "running", "waiting_user", "completed", "failed", "cancelled", "stopped", "interrupted"]
            : current.status === "running"
            ? ["running", "waiting_user", "completed", "failed", "cancelled", "stopped", "interrupted"]
            : current.status === "waiting_user"
              ? ["waiting_user", "running", "completed", "failed", "cancelled", "stopped", "interrupted"]
              : [current.status];
        assertAllowedTransition(incoming, current.status, next.status, allowed);
        assertTerminalToolEvolution(existing, incoming);
        return;
      }
      break;
  }
  throwIllegalEvolution(existing, incoming, "immutable", "changed");
}

/** Restricts same-seq assistant evolution to streaming text progress or terminal Provider-source attachment.
 * 将同 seq assistant 演进限制为流式正文进度或终态 Provider 来源证明追加。 */
function assertAssistantEvolution(
  existing: ConversationEventSummary,
  incoming: ConversationEventSummary,
): void {
  const current = existing.event;
  const next = incoming.event;
  const assistantKind = current.kind === "assistant_reasoning" || current.kind === "assistant_text";
  if (!assistantKind || next.kind !== current.kind) return;
  if (current.api_call_id !== next.api_call_id) {
    throwIllegalEvolution(existing, incoming, current.status, next.status);
  }
  if (current.status === "streaming") return;
  const providerSourceUnchanged = canonicalJson(current.provider_source) === canonicalJson(next.provider_source);
  const providerSourceAttached = current.provider_source === undefined && next.provider_source !== undefined;
  if (
    current.status !== next.status
    || current.text !== next.text
    || (!providerSourceUnchanged && !providerSourceAttached)
  ) {
    throwIllegalEvolution(existing, incoming, current.status, next.status);
  }
}

/** Rejects changes to immutable tool-call identity and request fields.
 * 拒绝工具调用身份与请求字段发生变化。 */
function assertToolImmutableFields(
  existing: ConversationEventSummary,
  incoming: ConversationEventSummary,
): void {
  if (existing.event.kind !== "tool_call" || incoming.event.kind !== "tool_call") return;
  const current = existing.event;
  const next = incoming.event;
  if (
    current.api_call_id !== next.api_call_id ||
    current.tool_name !== next.tool_name ||
    canonicalJson(current.input) !== canonicalJson(next.input) ||
    current.request_stream_id !== next.request_stream_id ||
    current.provider_tool_call_id !== next.provider_tool_call_id
  ) {
    throwIllegalEvolution(existing, incoming, current.status, next.status);
  }
}

/** Restricts a terminal same-status update to the one Core-owned Provider-source attachment.
 * 将终态同状态更新限制为 Core 持有的唯一 Provider 来源附着。 */
function assertTerminalToolEvolution(
  existing: ConversationEventSummary,
  incoming: ConversationEventSummary,
): void {
  if (existing.event.kind !== "tool_call" || incoming.event.kind !== "tool_call") return;
  // current is the already accepted same-seq tool snapshot.
  // current 是已经接受的同 seq 工具快照。
  const current = existing.event;
  // next is the higher event-revision candidate delivered by live or reconnect.
  // next 是由实时推送或重连送达的更高事件修订候选。
  const next = incoming.event;
  if (!isTerminalToolStatus(current.status) || current.status !== next.status) return;
  // Provider source is the only terminal field that Core may attach after tool completion.
  // Provider 来源是 Core 可以在工具完成后继续附着的唯一终态字段。
  const { provider_source: currentProviderSource, ...currentWithoutProviderSource } = current;
  const { provider_source: nextProviderSource, ...nextWithoutProviderSource } = next;
  // immutableTerminalBodyChanged detects every terminal fact drift except the allowed source attachment.
  // immutableTerminalBodyChanged 检测除允许的来源附着外的所有终态事实漂移。
  const immutableTerminalBodyChanged = canonicalJson(currentWithoutProviderSource) !== canonicalJson(nextWithoutProviderSource);
  // existingProviderSourceChanged prevents an attached source from being replaced or removed later.
  // existingProviderSourceChanged 防止已经附着的来源随后被替换或移除。
  const existingProviderSourceChanged = currentProviderSource !== undefined
    && canonicalJson(currentProviderSource) !== canonicalJson(nextProviderSource);
  if (immutableTerminalBodyChanged || existingProviderSourceChanged) {
    throwIllegalEvolution(existing, incoming, current.status, next.status);
  }
}

/** Returns whether one V1 tool status is terminal and cannot transition to another lifecycle state.
 * 返回一个 V1 工具状态是否为不可再迁移到其他生命周期状态的终态。 */
function isTerminalToolStatus(status: Extract<ConversationEventBody, { kind: "tool_call" }>["status"]): boolean {
  return status === "completed"
    || status === "failed"
    || status === "cancelled"
    || status === "stopped"
    || status === "interrupted";
}

/** Verifies one explicit status transition.
 * 校验一条显式状态迁移。 */
function assertAllowedTransition(
  summary: ConversationEventSummary,
  from: string,
  to: string,
  allowed: readonly string[],
): void {
  if (allowed.includes(to)) return;
  throw new Error(
    formatWindowPatchError({ code: "illegal_evolution", seq: summary.seq, kind: summary.event.kind, from, to }),
  );
}

/** Throws one normalized illegal-evolution diagnostic.
 * 抛出一条规范化的非法演进诊断。 */
function throwIllegalEvolution(
  existing: ConversationEventSummary,
  incoming: ConversationEventSummary,
  from: string,
  to: string,
): never {
  throw new Error(
    formatWindowPatchError({
      code: "illegal_evolution",
      seq: incoming.seq,
      kind: existing.event.kind,
      from,
      to,
    }),
  );
}

/** Builds and validates one standalone consecutive event window.
 * 构建并校验一个独立且连续的事件窗口。 */
function buildConsecutiveWindow(
  batch: ConversationEventWindowBatch,
): Map<number, ConversationEventSummary> {
  assertWindowOwner(batch);
  const replacement = new Map<number, ConversationEventSummary>();
  let expectedSeq = batch.events[0]?.seq ?? null;
  for (const summary of batch.events) {
    assertSafeSeq(summary.seq);
    assertSafeEventRevision(summary.event_revision);
    if (expectedSeq !== null) assertExactSeq(expectedSeq, summary.seq);
    replacement.set(summary.seq, summary);
    expectedSeq = summary.seq + 1;
  }
  return replacement;
}

/** Validates a non-empty session id and an exact positive generation.
 * 校验非空会话 ID 与精确的正代际编号。 */
function assertWindowOwner(owner: ConversationEventWindowOwner): void {
  if (owner.session_id.trim().length === 0) {
    throw new Error("event window session_id must not be empty");
  }
  assertSafeWindowGeneration(owner.window_generation);
}

/** Produces a deterministic JSON representation for structural comparisons.
 * 为结构比较生成确定性的 JSON 表示。 */
function canonicalJson(value: unknown): string {
  if (value === null || typeof value !== "object") return JSON.stringify(value);
  if (Array.isArray(value)) return `[${value.map((item) => canonicalJson(item)).join(",")}]`;
  const objectValue = value as Record<string, unknown>;
  return `{${Object.keys(objectValue)
    .sort()
    .map((key) => `${JSON.stringify(key)}:${canonicalJson(objectValue[key])}`)
    .join(",")}}`;
}

/** Derives the immutable same-seq identity from the authoritative event body.
 * 从权威事件体推导不可变的同 seq 身份。 */
function eventIdentity(summary: ConversationEventSummary): EventIdentity {
  const event = summary.event;
  switch (event.kind) {
    case "system_context":
      return `${event.kind}:${event.context_key}`;
    case "user_input":
      return `${event.kind}:${event.input_id}`;
    case "queued_input_resolution":
      return `${event.kind}:${event.input_id}:${event.client_action_id}`;
    case "api_started":
    case "assistant_reasoning":
    case "assistant_text":
    case "api_stats":
      return `${event.kind}:${event.api_call_id}`;
    case "tool_call":
      return `${event.kind}:${event.tool_call_id}`;
    case "turn_boundary":
    case "turn_stats":
      return `${event.kind}:${summary.turn_id ?? "none"}`;
    case "subagent_return":
      // Root action and task strings may contain separators; preserve tuple boundaries exactly.
      // 根动作及任务字符串可包含分隔符；精确保留元组边界。
      return JSON.stringify([event.kind, event.child_session_id, event.child_segment_generation, event.child_root_action_id, event.child_task_id, event.source_assistant_text_seq, event.child_turn_stats_seq]);
    case "display_note":
      return `${event.kind}:${event.notice_kind}:${event.reason_code}:${event.api_call_id ?? "none"}`;
    case "memory_recall":
      // The host records at most one recall per turn.
      // 宿主每个回合最多记录一次召回。
      return `${event.kind}:${summary.turn_id ?? "none"}`;
    case "request_config_changed":
      return event.kind;
    case "execution_control":
      return `${event.kind}:${event.control_kind}:${event.api_call_id ?? "none"}:${event.client_action_id ?? "none"}`;
    case "collaboration":
      return `${event.kind}:${event.operation_id}`;
    case "context_maintenance":
      // Start and terminal records retain their execution owner; sequence identity keeps both facts separate.
      // 起始与终态记录保留执行所有者；序号身份让两条事实彼此独立。
      return `${event.kind}:${event.execution_id}`;
  }
}

/** Asserts that candidate is exactly one seq after anchor.
 * 断言 candidate 恰好位于 anchor 后一个 seq。 */
function assertConsecutiveAfter(anchor: number, candidate: number): void {
  assertExactSeq(anchor + 1, candidate);
}

/** Asserts one exact expected seq value.
 * 断言一个精确的预期 seq 值。 */
function assertExactSeq(expected: number, actual: number): void {
  if (actual === expected) return;
  throw new Error(formatWindowPatchError({ code: "gap", expected, actual }));
}
