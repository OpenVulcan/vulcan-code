//! Canonical runtime-event log models shared by persistence and provider adapters.
//! 供持久化与 Provider 适配器共享的规范运行时事件日志模型。

use crate::protocol::{
    SessionConversationMessage, SessionConversationPayload, SessionConversationState,
    SessionConversationToolCall, normalize_replayed_protocol_messages,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use vulcan_core::types::{ConversationAttachment, ProviderUsageSnapshot};

/// Current schema version used by runtime JSONL event lines.
/// runtime JSONL 事件行当前使用的结构版本。
pub const RUNTIME_LOG_SCHEMA_VERSION: u32 = 1;

/// Append-only JSONL line stored in the session runtime log.
/// 存储在会话运行时日志中的追加式 JSONL 行。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeLogEventLine {
    /// Monotonic sequence number assigned inside one runtime log.
    /// 在单个运行时日志内分配的单调递增序号。
    pub seq: u64,
    /// Runtime log schema version used to decode this line.
    /// 用于解码该行的运行时日志结构版本。
    pub schema_version: u32,
    /// Unix-millis timestamp text captured when this line was written.
    /// 写入该行时捕获的 Unix 毫秒时间戳文本。
    pub created_at: String,
    /// Structured runtime event payload.
    /// 结构化运行时事件载荷。
    pub event: RuntimeLogEvent,
}

impl RuntimeLogEventLine {
    /// Builds one runtime log event line using the current schema version.
    /// 使用当前结构版本构建一条运行时日志事件行。
    pub fn new(seq: u64, created_at: impl Into<String>, event: RuntimeLogEvent) -> Self {
        Self {
            seq,
            schema_version: RUNTIME_LOG_SCHEMA_VERSION,
            created_at: created_at.into(),
            event,
        }
    }
}

/// Runtime event variants that describe provider-relevant conversation facts.
/// 描述 Provider 相关会话事实的运行时事件变体。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RuntimeLogEvent {
    /// User-authored message that belongs to the canonical runtime history.
    /// 属于规范运行时历史的用户原始消息。
    UserMessage(RuntimeUserMessageEvent),
    /// Assistant message text and provider metadata.
    /// 助手消息文本与 Provider 元数据。
    AssistantMessage(RuntimeAssistantMessageEvent),
    /// Start marker for one assistant tool-call batch.
    /// 单个助手工具调用批次的开始标记。
    ToolBatchStarted(RuntimeToolBatchStartedEvent),
    /// Single tool call emitted by an assistant batch.
    /// 助手批次发出的单个工具调用。
    ToolCall(RuntimeToolCallEvent),
    /// Single tool result returned by the host runtime.
    /// 宿主运行时返回的单个工具结果。
    ToolResult(RuntimeToolResultEvent),
    /// Completion marker for one assistant tool-call batch.
    /// 单个助手工具调用批次的完成标记。
    ToolBatchCompleted(RuntimeToolBatchCompletedEvent),
    /// Provider request checkpoint captured before or after conversion.
    /// 在转换前后捕获的 Provider 请求检查点。
    ProviderTurnCheckpoint(RuntimeProviderTurnCheckpointEvent),
    /// Context compaction event that changed the active runtime window.
    /// 改变活动运行时窗口的上下文压缩事件。
    ContextCompactionApplied(RuntimeContextCompactionAppliedEvent),
    /// Repair event emitted after log recovery or conservative downgrade.
    /// 日志恢复或保守降级后发出的修复事件。
    RuntimeRepair(RuntimeRepairEvent),
    /// Injection view applied to one provider-facing turn.
    /// 应用到单次 Provider 可见回合的注入视图。
    InjectionApplied(RuntimeInjectionAppliedEvent),
    /// Chronological system update that starts applying at a specific runtime position.
    /// 从特定运行时位置开始生效的时间顺序系统更新。
    SystemUpdate(RuntimeSystemUpdateEvent),
    /// Injection node or view was cancelled and must not appear in future provider requests.
    /// 注入节点或视图已取消，后续 Provider 请求不得继续携带。
    InjectionCancelled(RuntimeInjectionCancelledEvent),
    /// Persistent injection blocks were folded into a new stable baseline.
    /// 长期注入块已折叠进新的稳定基线。
    InjectionMerged(RuntimeInjectionMergedEvent),
    /// Provider request boundary captured before wire payload execution.
    /// 线协议载荷执行前捕获的 Provider 请求边界。
    ProviderRequestCheckpoint(RuntimeProviderRequestCheckpointEvent),
    /// Provider response boundary captured after stream or non-stream completion.
    /// 流式或非流式完成后捕获的 Provider 响应边界。
    ProviderResponseCheckpoint(RuntimeProviderResponseCheckpointEvent),
    /// Provider lowering downgrade, repair, or rejection captured for audit.
    /// 为审计捕获的 Provider 转换降级、修复或拒绝。
    ProviderLoweringRepair(RuntimeProviderLoweringRepairEvent),
}

/// Runtime record for one user-authored message.
/// 单条用户原始消息的运行时记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeUserMessageEvent {
    /// Optional logical turn identifier.
    /// 可选的逻辑回合标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    /// Stable local message identifier.
    /// 稳定的本地消息标识。
    pub message_id: String,
    /// User-visible text content.
    /// 用户可见文本内容。
    pub text: String,
    /// Structured attachments that belong to this user message.
    /// 归属于该用户消息的结构化附件。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<ConversationAttachment>,
}

/// Runtime record for one assistant message.
/// 单条助手消息的运行时记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeAssistantMessageEvent {
    /// Optional logical turn identifier.
    /// 可选的逻辑回合标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    /// Stable local message identifier.
    /// 稳定的本地消息标识。
    pub message_id: String,
    /// Visible assistant text content.
    /// 助手可见文本内容。
    pub text: String,
    /// Flattened reasoning text retained for local replay and audit.
    /// 为本地回放与审计保留的扁平化推理文本。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub reasoning: String,
    /// Optional provider identifier that produced this assistant message.
    /// 产出该助手消息的可选 Provider 标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    /// Optional model identifier that produced this assistant message.
    /// 产出该助手消息的可选模型标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    /// Optional provider-native assistant message identifier.
    /// 可选的 Provider 原生助手消息标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_message_id: Option<String>,
    /// Optional provider-native assistant state used only by matching provider families.
    /// 仅由匹配 Provider family 使用的可选 Provider 原生助手状态。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_message_state: Option<Value>,
    /// Optional provider usage snapshot.
    /// 可选的 Provider 用量快照。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<ProviderUsageSnapshot>,
}

/// Runtime record that opens one assistant tool-call batch.
/// 打开单个助手工具调用批次的运行时记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeToolBatchStartedEvent {
    /// Optional logical turn identifier.
    /// 可选的逻辑回合标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    /// Stable runtime batch identifier.
    /// 稳定的运行时批次标识。
    pub tool_batch_id: String,
    /// Assistant message that owns this batch.
    /// 拥有该批次的助手消息。
    pub assistant_message_id: String,
}

/// Runtime record for one assistant tool call.
/// 单个助手工具调用的运行时记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeToolCallEvent {
    /// Optional logical turn identifier.
    /// 可选的逻辑回合标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    /// Runtime batch identifier that groups parallel tool calls.
    /// 对并行工具调用分组的运行时批次标识。
    pub tool_batch_id: String,
    /// Assistant message that owns this tool call.
    /// 拥有该工具调用的助手消息。
    pub assistant_message_id: String,
    /// Stable runtime-owned tool call identifier.
    /// 由运行时持有的稳定工具调用标识。
    pub runtime_tool_call_id: String,
    /// Optional provider-visible tool call identifier.
    /// 可选的 Provider 可见工具调用标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_tool_call_id: Option<String>,
    /// Host tool name selected by the assistant.
    /// 助手选择的宿主工具名称。
    pub tool_name: String,
    /// Structured JSON arguments selected by the assistant.
    /// 助手选择的结构化 JSON 参数。
    pub arguments: Value,
}

/// Runtime record for one host tool result.
/// 单个宿主工具结果的运行时记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeToolResultEvent {
    /// Optional logical turn identifier.
    /// 可选的逻辑回合标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    /// Optional runtime batch identifier resolved during migration or live execution.
    /// 在迁移或实时执行期间解析出的可选运行时批次标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_batch_id: Option<String>,
    /// Optional runtime-owned tool call identifier.
    /// 可选的运行时工具调用标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_tool_call_id: Option<String>,
    /// Optional provider-visible tool call identifier.
    /// 可选的 Provider 可见工具调用标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_tool_call_id: Option<String>,
    /// Stable tool-result identifier.
    /// 稳定的工具结果标识。
    pub tool_result_id: String,
    /// Flattened text returned to the model.
    /// 回传给模型的扁平化文本。
    pub text: String,
    /// Result status used by repair and provider conversion.
    /// 供修复与 Provider 转换使用的结果状态。
    #[serde(default)]
    pub status: RuntimeToolResultStatus,
    /// Optional error summary when the tool failed.
    /// 工具失败时的可选错误摘要。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Runtime status for one tool result.
/// 单个工具结果的运行时状态。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeToolResultStatus {
    /// Tool execution completed successfully.
    /// 工具执行已成功完成。
    #[default]
    Completed,
    /// Tool execution failed but still produced model-visible text.
    /// 工具执行失败但仍产出了模型可见文本。
    Failed,
    /// Tool result could not be matched to a preceding runtime tool call.
    /// 工具结果无法匹配到前置运行时工具调用。
    Orphan,
}

/// Runtime record that closes one assistant tool-call batch.
/// 关闭单个助手工具调用批次的运行时记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeToolBatchCompletedEvent {
    /// Optional logical turn identifier.
    /// 可选的逻辑回合标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    /// Stable runtime batch identifier.
    /// 稳定的运行时批次标识。
    pub tool_batch_id: String,
    /// Number of tool calls recorded in this batch.
    /// 该批次中记录的工具调用数量。
    pub tool_call_count: usize,
    /// Number of tool results matched to this batch.
    /// 匹配到该批次的工具结果数量。
    pub tool_result_count: usize,
    /// Whether every call in this batch has a matched result.
    /// 该批次中的每个调用是否都有匹配结果。
    pub complete: bool,
}

/// Runtime record for one provider conversion checkpoint.
/// 单次 Provider 转换检查点的运行时记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeProviderTurnCheckpointEvent {
    /// Optional logical turn identifier.
    /// 可选的逻辑回合标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    /// Provider identifier targeted by the conversion.
    /// 转换目标 Provider 标识。
    pub provider_id: String,
    /// Model identifier targeted by the conversion.
    /// 转换目标模型标识。
    pub model_id: String,
    /// Provider API family used by the conversion.
    /// 转换使用的 Provider API family。
    pub provider_family: String,
    /// Stable prompt-layer fingerprint without storing the system prompt body.
    /// 不保存系统提示词正文的稳定提示词层指纹。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_fingerprint: Option<String>,
    /// Stable injection-view fingerprint.
    /// 稳定的注入视图指纹。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub injection_fingerprint: Option<String>,
    /// Converter version used to build the provider request.
    /// 构建 Provider 请求时使用的转换器版本。
    pub converter_version: u32,
}

/// Trust classification for chronological system updates.
/// 时间顺序系统更新的信任分级。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeSystemUpdateTrust {
    /// Host-generated update that may be lowered to privileged provider system fields.
    /// 宿主生成且可降级到特权 Provider system 字段的更新。
    TrustedHost,
    /// User-authored update that must remain user-scoped unless explicitly promoted by policy.
    /// 用户提供且除非策略明确提升否则必须保持用户作用域的更新。
    UserScoped,
    /// External or tool-derived update that must never become privileged system content.
    /// 外部或工具派生且绝不能成为特权 system 内容的更新。
    UntrustedExternal,
}

impl Default for RuntimeSystemUpdateTrust {
    /// Returns the conservative default trust level for historical or partially migrated events.
    /// 返回历史或部分迁移事件使用的保守默认信任级别。
    fn default() -> Self {
        Self::UserScoped
    }
}

/// Lowering preference attached to one chronological system update.
/// 附着在单条时间顺序系统更新上的转换偏好。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeSystemUpdateLoweringHint {
    /// Let the active provider capability choose native, fallback, or rejection.
    /// 由当前 Provider 能力选择原生、降级或拒绝路径。
    Auto,
    /// Prefer provider-native chronological system support when the provider allows it.
    /// 当 Provider 允许时优先使用原生时间顺序 system 支持。
    PreferNative,
    /// Force the update to remain a user-scoped wrapper.
    /// 强制该更新保持为用户作用域包装。
    UserWrapper,
    /// Reject the request if the update cannot be represented natively.
    /// 如果无法原生表达该更新则拒绝请求。
    RequireNative,
}

impl Default for RuntimeSystemUpdateLoweringHint {
    /// Returns the default provider-capability-driven lowering preference.
    /// 返回默认的 Provider 能力驱动转换偏好。
    fn default() -> Self {
        Self::Auto
    }
}

/// Runtime record for one provider-neutral chronological system update.
/// 单条 Provider 中立时间顺序系统更新的运行时记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeSystemUpdateEvent {
    /// Optional logical turn identifier that owns this update.
    /// 拥有该更新的可选逻辑回合标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    /// Stable local update identifier.
    /// 稳定的本地更新标识。
    pub update_id: String,
    /// Runtime position after which this update becomes effective.
    /// 该更新开始生效的运行时位置。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effective_after_event_id: Option<String>,
    /// Text content of the system update.
    /// 系统更新的文本内容。
    pub text: String,
    /// Trust level used by provider lowering.
    /// Provider 转换使用的信任级别。
    #[serde(default)]
    pub trust: RuntimeSystemUpdateTrust,
    /// Preferred lowering behavior for this update.
    /// 该更新的首选转换行为。
    #[serde(default)]
    pub lowering_hint: RuntimeSystemUpdateLoweringHint,
    /// Stable fingerprint of the text and source metadata.
    /// 文本与来源元数据的稳定指纹。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
    /// Human-readable source label for audit.
    /// 供审计使用的人类可读来源标签。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

/// Runtime record that marks one injection view or source node as cancelled.
/// 将单个注入视图或来源节点标记为已取消的运行时记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeInjectionCancelledEvent {
    /// Optional logical turn identifier that observed the cancellation.
    /// 观察到该取消动作的可选逻辑回合标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    /// Stable cancellation identifier.
    /// 稳定的取消动作标识。
    pub cancellation_id: String,
    /// Injection source node identifiers removed from future projections.
    /// 从后续投影中移除的注入来源节点标识。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_node_ids: Vec<String>,
    /// Optional persistent fingerprint before cancellation.
    /// 取消前的可选长期注入指纹。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_persistent_fingerprint: Option<String>,
    /// Optional transient fingerprint before cancellation.
    /// 取消前的可选动态注入指纹。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_transient_fingerprint: Option<String>,
    /// Human-readable reason for audit.
    /// 供审计使用的人类可读原因。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Runtime record that marks persistent injection content folded into a stable baseline.
/// 标记长期注入内容已折叠进稳定基线的运行时记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeInjectionMergedEvent {
    /// Optional logical turn identifier that triggered the merge.
    /// 触发该合并的可选逻辑回合标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    /// Stable merge identifier.
    /// 稳定的合并标识。
    pub merge_id: String,
    /// Optional compaction identifier associated with this merge.
    /// 与该合并关联的可选压缩标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compaction_id: Option<String>,
    /// Fingerprint of the previous persistent injection view.
    /// 旧长期注入视图的指纹。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_persistent_fingerprint: Option<String>,
    /// Fingerprint of the merged baseline after folding.
    /// 折叠后的合并基线指纹。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merged_baseline_fingerprint: Option<String>,
    /// Source node identifiers included in the merged baseline.
    /// 纳入合并基线的来源节点标识。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_node_ids: Vec<String>,
}

/// Runtime envelope for provider-native state stored inside canonical history.
/// 存储在规范历史中的 Provider 原生状态运行时信封。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeProviderNativeStateEnvelope {
    /// Provider protocol family that owns the payload.
    /// 拥有该载荷的 Provider 协议族。
    pub protocol_family: String,
    /// Provider identifier that produced the payload.
    /// 产出该载荷的 Provider 标识。
    pub provider_id: String,
    /// Model identifier that produced the payload.
    /// 产出该载荷的模型标识。
    pub model_id: String,
    /// Schema version of the envelope payload.
    /// 信封载荷的结构版本。
    pub state_schema_version: u32,
    /// Optional capability fingerprint that guarded creation.
    /// 创建该状态时使用的可选能力指纹。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability_fingerprint: Option<String>,
    /// Optional provider request identifier that created the state.
    /// 创建该状态的可选 Provider 请求标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_request_id: Option<String>,
    /// Optional logical source turn identifier.
    /// 可选的逻辑来源回合标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_turn_id: Option<String>,
    /// Provider-native payload owned by the matching adapter.
    /// 由匹配适配器拥有的 Provider 原生载荷。
    pub payload: Value,
}

impl RuntimeProviderNativeStateEnvelope {
    /// Builds a provider-native state envelope with the current envelope schema.
    /// 使用当前信封结构构建 Provider 原生状态信封。
    pub fn new(
        protocol_family: impl Into<String>,
        provider_id: impl Into<String>,
        model_id: impl Into<String>,
        payload: Value,
    ) -> Self {
        Self {
            protocol_family: protocol_family.into(),
            provider_id: provider_id.into(),
            model_id: model_id.into(),
            state_schema_version: 1,
            capability_fingerprint: None,
            created_request_id: None,
            source_turn_id: None,
            payload,
        }
    }

    /// Returns whether this envelope can be consumed by the active provider request.
    /// 返回该信封是否可被当前活动 Provider 请求消费。
    pub fn matches_scope(&self, protocol_family: &str, provider_id: &str, model_id: &str) -> bool {
        self.protocol_family == protocol_family
            && self.provider_id == provider_id
            && self.model_id == model_id
    }
}

/// Runtime record captured before one provider request is executed.
/// 单次 Provider 请求执行前捕获的运行时记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeProviderRequestCheckpointEvent {
    /// Optional logical turn identifier.
    /// 可选的逻辑回合标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    /// Stable checkpoint identifier.
    /// 稳定的检查点标识。
    pub checkpoint_id: String,
    /// Provider identifier targeted by the request.
    /// 请求目标 Provider 标识。
    pub provider_id: String,
    /// Model identifier targeted by the request.
    /// 请求目标模型标识。
    pub model_id: String,
    /// Provider protocol family used by the request.
    /// 请求使用的 Provider 协议族。
    pub provider_family: String,
    /// Optional capability fingerprint used during lowering.
    /// 转换期间使用的可选能力指纹。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability_fingerprint: Option<String>,
    /// Optional prompt fingerprint without storing prompt body.
    /// 不存储提示词正文的可选提示词指纹。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_fingerprint: Option<String>,
    /// Optional injection fingerprint for this request.
    /// 该请求的可选注入指纹。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub injection_fingerprint: Option<String>,
    /// Optional request payload fingerprint.
    /// 可选的请求载荷指纹。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_fingerprint: Option<String>,
    /// Optional provider-neutral cache-policy fingerprint.
    /// 可选的 Provider 中立缓存策略指纹。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_policy_fingerprint: Option<String>,
    /// Optional cache-policy diagnostic emitted when hints are lowered away.
    /// 缓存提示被转换丢弃时发出的可选缓存策略诊断。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_policy_diagnostic: Option<String>,
    /// Converter version used to build the provider request.
    /// 构建 Provider 请求时使用的转换器版本。
    pub converter_version: u32,
}

/// Runtime record captured after one provider response completes.
/// 单次 Provider 响应完成后捕获的运行时记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeProviderResponseCheckpointEvent {
    /// Optional logical turn identifier.
    /// 可选的逻辑回合标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    /// Stable checkpoint identifier.
    /// 稳定的检查点标识。
    pub checkpoint_id: String,
    /// Provider identifier that produced the response.
    /// 产出响应的 Provider 标识。
    pub provider_id: String,
    /// Model identifier that produced the response.
    /// 产出响应的模型标识。
    pub model_id: String,
    /// Provider protocol family used by the response.
    /// 响应使用的 Provider 协议族。
    pub provider_family: String,
    /// Optional finish reason reported by the provider.
    /// Provider 报告的可选结束原因。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
    /// Optional provider-native state envelope retained for replay.
    /// 为回放保留的可选 Provider 原生状态信封。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_state: Option<RuntimeProviderNativeStateEnvelope>,
    /// Optional provider usage snapshot.
    /// 可选的 Provider 用量快照。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<ProviderUsageSnapshot>,
}

/// Runtime record for one provider lowering repair, fallback, or rejection.
/// 单次 Provider 转换修复、降级或拒绝的运行时记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeProviderLoweringRepairEvent {
    /// Optional logical turn identifier.
    /// 可选的逻辑回合标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    /// Stable repair identifier.
    /// 稳定的修复标识。
    pub repair_id: String,
    /// Provider identifier targeted by lowering.
    /// 转换目标 Provider 标识。
    pub provider_id: String,
    /// Model identifier targeted by lowering.
    /// 转换目标模型标识。
    pub model_id: String,
    /// Provider protocol family used by lowering.
    /// 转换使用的 Provider 协议族。
    pub provider_family: String,
    /// Machine-readable repair action.
    /// 机器可读的修复动作。
    pub action: String,
    /// Human-readable reason for the repair.
    /// 人类可读的修复原因。
    pub reason: String,
    /// Number of affected runtime or provider items.
    /// 受影响的运行时或 Provider 项数量。
    #[serde(default)]
    pub affected_items: usize,
}

/// Runtime record for one applied context compaction.
/// 单次已应用上下文压缩的运行时记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeContextCompactionAppliedEvent {
    /// Stable compaction identifier.
    /// 稳定的压缩标识。
    pub compaction_id: String,
    /// Optional summary message identifier produced by compaction.
    /// 压缩产出的可选摘要消息标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary_message_id: Option<String>,
    /// Runtime backup files created before compaction.
    /// 压缩前创建的运行时备份文件。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub backup_files: Vec<String>,
    /// Stable fingerprint for the compaction boundary and its runtime backup coverage.
    /// 压缩边界及其运行时备份覆盖范围的稳定指纹。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compaction_fingerprint: Option<String>,
    /// Optional merged persistent-injection fingerprint.
    /// 可选的已合并长期注入指纹。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merged_injection_fingerprint: Option<String>,
}

/// Runtime record for one recovery or downgrade action.
/// 单次恢复或降级动作的运行时记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeRepairEvent {
    /// Stable repair identifier.
    /// 稳定的修复标识。
    pub repair_id: String,
    /// Source that triggered the repair.
    /// 触发修复的来源。
    pub source: String,
    /// Human-readable repair detail.
    /// 人类可读的修复详情。
    pub detail: String,
    /// Number of tool segments downgraded to text.
    /// 被降级为文本的工具片段数量。
    #[serde(default)]
    pub downgraded_tool_segments: usize,
}

/// Runtime record for the injection view applied to one turn.
/// 单个回合已应用注入视图的运行时记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeInjectionAppliedEvent {
    /// Optional logical turn identifier.
    /// 可选的逻辑回合标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    /// Fingerprint for the complete provider-facing injection overlay.
    /// 完整 Provider 可见注入覆盖层的指纹。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub combined_fingerprint: Option<String>,
    /// Fingerprint for persistent system-layer injection blocks.
    /// 长期系统层注入块的指纹。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub persistent_fingerprint: Option<String>,
    /// Fingerprint for transient user-overlay injection blocks.
    /// 动态用户覆盖注入块的指纹。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transient_fingerprint: Option<String>,
    /// Whether transient user-overlay content was present.
    /// 是否存在动态用户覆盖内容。
    #[serde(default)]
    pub has_transient: bool,
    /// Optional merged-baseline fingerprint active when compaction has folded persistent injections.
    /// 压缩已折叠长期注入时生效的可选合并基线指纹。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merged_baseline_fingerprint: Option<String>,
    /// Stable non-content source identifiers included in this applied injection view.
    /// 本次已应用注入视图包含的稳定非正文来源标识。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_node_ids: Vec<String>,
}

/// Converts one canonical conversation state into runtime log lines.
/// 将一个规范会话状态转换为运行时日志行。
pub fn runtime_log_lines_from_conversation_state(
    state: &SessionConversationState,
    created_at: &str,
    starting_seq: u64,
) -> Vec<RuntimeLogEventLine> {
    let mut builder = RuntimeLogProjectionBuilder::new(created_at, starting_seq);
    let mut open_batches = BTreeMap::<String, RuntimeOpenToolBatch>::new();
    for message in &state.messages {
        builder.push_message(message, &mut open_batches);
    }
    builder.close_remaining_batches(open_batches);
    builder.lines
}

/// Rebuilds a canonical conversation state from runtime log lines.
/// 从运行时日志行重建规范会话状态。
pub fn conversation_state_from_runtime_log_lines(
    lines: &[RuntimeLogEventLine],
) -> SessionConversationState {
    let mut projection = RuntimeConversationProjection::default();
    for line in lines {
        projection.apply_event(&line.event);
    }
    projection.finish()
}

/// Internal builder that projects conversation messages into runtime events.
/// 将会话消息投影为运行时事件的内部构建器。
struct RuntimeLogProjectionBuilder<'a> {
    /// Timestamp attached to generated runtime lines.
    /// 附加到生成运行时行上的时间戳。
    created_at: &'a str,
    /// Next sequence number assigned to generated runtime lines.
    /// 分配给生成运行时行的下一个序号。
    next_seq: u64,
    /// Generated runtime event lines.
    /// 已生成的运行时事件行。
    lines: Vec<RuntimeLogEventLine>,
}

impl<'a> RuntimeLogProjectionBuilder<'a> {
    /// Builds one projection builder.
    /// 构建一个投影构建器。
    fn new(created_at: &'a str, starting_seq: u64) -> Self {
        Self {
            created_at,
            next_seq: starting_seq,
            lines: Vec::new(),
        }
    }

    /// Appends a runtime event and advances the local sequence.
    /// 追加一条运行时事件并推进本地序号。
    fn append(&mut self, event: RuntimeLogEvent) {
        self.lines.push(RuntimeLogEventLine::new(
            self.next_seq,
            self.created_at,
            event,
        ));
        self.next_seq = self.next_seq.saturating_add(1);
    }

    /// Projects one canonical message into runtime events.
    /// 将一条规范消息投影为运行时事件。
    fn push_message(
        &mut self,
        message: &SessionConversationMessage,
        open_batches: &mut BTreeMap<String, RuntimeOpenToolBatch>,
    ) {
        match &message.payload {
            SessionConversationPayload::UserText { text, attachments } => {
                self.append(RuntimeLogEvent::UserMessage(RuntimeUserMessageEvent {
                    turn_id: Some(message.id.clone()),
                    message_id: message.id.clone(),
                    text: text.clone(),
                    attachments: attachments.clone(),
                }));
            }
            SessionConversationPayload::Assistant {
                text,
                reasoning,
                provider_message_id,
                provider_message_state,
                usage,
                tool_calls,
            } => {
                self.append(RuntimeLogEvent::AssistantMessage(
                    RuntimeAssistantMessageEvent {
                        turn_id: Some(message.id.clone()),
                        message_id: message.id.clone(),
                        text: text.clone(),
                        reasoning: reasoning.clone(),
                        provider_id: None,
                        model_id: None,
                        provider_message_id: provider_message_id.clone(),
                        provider_message_state: provider_message_state.clone(),
                        usage: usage.clone(),
                    },
                ));
                if !tool_calls.is_empty() {
                    self.open_tool_batch(message, tool_calls, open_batches);
                }
            }
            SessionConversationPayload::ToolResult {
                tool_result_id,
                tool_call_id,
                text,
            } => {
                let runtime_tool_call_id = tool_call_id.clone();
                let tool_batch_id = runtime_tool_call_id
                    .as_deref()
                    .and_then(|id| open_batches.get(id))
                    .map(|batch| batch.tool_batch_id.clone());
                let status = if runtime_tool_call_id.is_some() && tool_batch_id.is_some() {
                    RuntimeToolResultStatus::Completed
                } else {
                    RuntimeToolResultStatus::Orphan
                };
                if let Some(runtime_tool_call_id) = runtime_tool_call_id.as_deref()
                    && let Some(batch) = open_batches.get_mut(runtime_tool_call_id)
                {
                    batch.tool_result_count = batch.tool_result_count.saturating_add(1);
                }
                self.append(RuntimeLogEvent::ToolResult(RuntimeToolResultEvent {
                    turn_id: Some(message.id.clone()),
                    tool_batch_id,
                    runtime_tool_call_id,
                    provider_tool_call_id: tool_call_id.clone(),
                    tool_result_id: tool_result_id.clone(),
                    text: text.clone(),
                    status,
                    error: None,
                }));
            }
        }
    }

    /// Opens and records one assistant tool-call batch.
    /// 打开并记录一个助手工具调用批次。
    fn open_tool_batch(
        &mut self,
        message: &SessionConversationMessage,
        tool_calls: &[SessionConversationToolCall],
        open_batches: &mut BTreeMap<String, RuntimeOpenToolBatch>,
    ) {
        let tool_batch_id = format!("tool-batch-{}", message.id);
        self.append(RuntimeLogEvent::ToolBatchStarted(
            RuntimeToolBatchStartedEvent {
                turn_id: Some(message.id.clone()),
                tool_batch_id: tool_batch_id.clone(),
                assistant_message_id: message.id.clone(),
            },
        ));
        for tool_call in tool_calls {
            let runtime_tool_call_id = runtime_tool_call_id(tool_call);
            open_batches.insert(
                runtime_tool_call_id.clone(),
                RuntimeOpenToolBatch {
                    tool_batch_id: tool_batch_id.clone(),
                    tool_call_count: tool_calls.len(),
                    tool_result_count: 0,
                },
            );
            self.append(RuntimeLogEvent::ToolCall(RuntimeToolCallEvent {
                turn_id: Some(message.id.clone()),
                tool_batch_id: tool_batch_id.clone(),
                assistant_message_id: message.id.clone(),
                runtime_tool_call_id,
                provider_tool_call_id: tool_call.tool_call_id.clone(),
                tool_name: tool_call.tool_name.clone(),
                arguments: tool_call.arguments.clone(),
            }));
        }
    }

    /// Closes every open batch that survived projection.
    /// 关闭投影后仍保持打开的每个批次。
    fn close_remaining_batches(&mut self, open_batches: BTreeMap<String, RuntimeOpenToolBatch>) {
        let mut closed_batches = BTreeMap::<String, RuntimeOpenToolBatch>::new();
        for batch in open_batches.into_values() {
            closed_batches
                .entry(batch.tool_batch_id.clone())
                .and_modify(|existing| {
                    existing.tool_result_count = existing
                        .tool_result_count
                        .saturating_add(batch.tool_result_count);
                    existing.tool_call_count = existing.tool_call_count.max(batch.tool_call_count);
                })
                .or_insert(batch);
        }
        for batch in closed_batches.into_values() {
            self.append(RuntimeLogEvent::ToolBatchCompleted(
                RuntimeToolBatchCompletedEvent {
                    turn_id: None,
                    tool_batch_id: batch.tool_batch_id,
                    tool_call_count: batch.tool_call_count,
                    tool_result_count: batch.tool_result_count,
                    complete: batch.tool_call_count == batch.tool_result_count,
                },
            ));
        }
    }
}

/// Tracks one open tool batch during conversation-to-runtime projection.
/// 在会话到运行时投影期间跟踪一个打开的工具批次。
#[derive(Debug, Clone)]
struct RuntimeOpenToolBatch {
    /// Runtime batch identifier.
    /// 运行时批次标识。
    tool_batch_id: String,
    /// Number of tool calls inside the batch.
    /// 批次中的工具调用数量。
    tool_call_count: usize,
    /// Number of matched tool results seen so far.
    /// 当前已经看到的匹配工具结果数量。
    tool_result_count: usize,
}

/// Internal projection state used to rebuild conversation messages from runtime events.
/// 用于从运行时事件重建会话消息的内部投影状态。
#[derive(Debug, Default)]
struct RuntimeConversationProjection {
    /// Rebuilt canonical messages.
    /// 已重建的规范消息。
    messages: Vec<SessionConversationMessage>,
    /// Assistant messages keyed by local message id.
    /// 按本地消息标识索引的助手消息。
    assistant_indexes: BTreeMap<String, usize>,
    /// Tool calls keyed by runtime-owned id.
    /// 按运行时持有标识索引的工具调用。
    tool_calls: BTreeMap<String, RuntimeToolCallEvent>,
}

impl RuntimeConversationProjection {
    /// Applies one runtime event to the projection.
    /// 将一条运行时事件应用到投影。
    fn apply_event(&mut self, event: &RuntimeLogEvent) {
        match event {
            RuntimeLogEvent::UserMessage(event) => self.push_user_message(event),
            RuntimeLogEvent::AssistantMessage(event) => self.push_assistant_message(event),
            RuntimeLogEvent::ToolCall(event) => self.attach_tool_call(event),
            RuntimeLogEvent::ToolResult(event) => self.push_tool_result(event),
            RuntimeLogEvent::ToolBatchStarted(_)
            | RuntimeLogEvent::ToolBatchCompleted(_)
            | RuntimeLogEvent::ProviderTurnCheckpoint(_)
            | RuntimeLogEvent::ContextCompactionApplied(_)
            | RuntimeLogEvent::RuntimeRepair(_)
            | RuntimeLogEvent::InjectionApplied(_)
            | RuntimeLogEvent::SystemUpdate(_)
            | RuntimeLogEvent::InjectionCancelled(_)
            | RuntimeLogEvent::InjectionMerged(_)
            | RuntimeLogEvent::ProviderRequestCheckpoint(_)
            | RuntimeLogEvent::ProviderResponseCheckpoint(_)
            | RuntimeLogEvent::ProviderLoweringRepair(_) => {}
        }
    }

    /// Finalizes the rebuilt conversation state.
    /// 完成已重建会话状态。
    fn finish(self) -> SessionConversationState {
        SessionConversationState {
            messages: normalize_replayed_protocol_messages(self.messages),
            records: Vec::new(),
            continuation: Default::default(),
        }
    }

    /// Adds one user message to the projection.
    /// 向投影添加一条用户消息。
    fn push_user_message(&mut self, event: &RuntimeUserMessageEvent) {
        self.messages.push(SessionConversationMessage {
            id: event.message_id.clone(),
            payload: SessionConversationPayload::UserText {
                text: event.text.clone(),
                attachments: event.attachments.clone(),
            },
        });
    }

    /// Adds one assistant message to the projection.
    /// 向投影添加一条助手消息。
    fn push_assistant_message(&mut self, event: &RuntimeAssistantMessageEvent) {
        let index = self.messages.len();
        self.messages.push(SessionConversationMessage {
            id: event.message_id.clone(),
            payload: SessionConversationPayload::Assistant {
                text: event.text.clone(),
                reasoning: event.reasoning.clone(),
                provider_message_id: event.provider_message_id.clone(),
                provider_message_state: event.provider_message_state.clone(),
                usage: event.usage.clone(),
                tool_calls: Vec::new(),
            },
        });
        self.assistant_indexes
            .insert(event.message_id.clone(), index);
    }

    /// Attaches one tool call to its owning assistant message.
    /// 将一条工具调用附加到其所属助手消息。
    fn attach_tool_call(&mut self, event: &RuntimeToolCallEvent) {
        self.tool_calls
            .insert(event.runtime_tool_call_id.clone(), event.clone());
        let Some(index) = self
            .assistant_indexes
            .get(&event.assistant_message_id)
            .copied()
        else {
            return;
        };
        let Some(message) = self.messages.get_mut(index) else {
            return;
        };
        let SessionConversationPayload::Assistant { tool_calls, .. } = &mut message.payload else {
            return;
        };
        tool_calls.push(SessionConversationToolCall {
            id: event.runtime_tool_call_id.clone(),
            tool_call_id: event
                .provider_tool_call_id
                .clone()
                .or_else(|| Some(event.runtime_tool_call_id.clone())),
            tool_name: event.tool_name.clone(),
            arguments: event.arguments.clone(),
        });
    }

    /// Adds one matched or downgraded tool result to the projection.
    /// 向投影添加一条已匹配或已降级的工具结果。
    fn push_tool_result(&mut self, event: &RuntimeToolResultEvent) {
        self.messages.push(SessionConversationMessage {
            id: event.tool_result_id.clone(),
            payload: SessionConversationPayload::ToolResult {
                tool_result_id: event.tool_result_id.clone(),
                tool_call_id: event
                    .provider_tool_call_id
                    .clone()
                    .or_else(|| event.runtime_tool_call_id.clone()),
                text: event.text.clone(),
            },
        });
    }
}

/// Resolves the stable runtime id for one tool call.
/// 解析单条工具调用的稳定运行时标识。
fn runtime_tool_call_id(tool_call: &SessionConversationToolCall) -> String {
    tool_call
        .tool_call_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| tool_call.id.clone())
}

#[cfg(test)]
mod tests {
    //! Runtime log projection regression tests.
    //! 运行时日志投影回归测试。

    use super::*;
    use serde_json::json;

    /// Builds one assistant tool call for projection tests.
    /// 为投影测试构建一条助手工具调用。
    fn tool_call(id: &str) -> SessionConversationToolCall {
        SessionConversationToolCall {
            id: id.to_string(),
            tool_call_id: Some(id.to_string()),
            tool_name: String::from("demo"),
            arguments: json!({ "value": 1 }),
        }
    }

    #[test]
    fn runtime_log_projection_preserves_parallel_tool_batch() {
        let state = SessionConversationState {
            messages: vec![
                SessionConversationMessage {
                    id: String::from("user-1"),
                    payload: SessionConversationPayload::UserText {
                        text: String::from("run"),
                        attachments: Vec::new(),
                    },
                },
                SessionConversationMessage {
                    id: String::from("assistant-1"),
                    payload: SessionConversationPayload::Assistant {
                        text: String::new(),
                        reasoning: String::new(),
                        provider_message_id: None,
                        provider_message_state: None,
                        usage: None,
                        tool_calls: vec![tool_call("call-a"), tool_call("call-b")],
                    },
                },
                SessionConversationMessage {
                    id: String::from("result-a"),
                    payload: SessionConversationPayload::ToolResult {
                        tool_result_id: String::from("result-a"),
                        tool_call_id: Some(String::from("call-a")),
                        text: String::from("ok"),
                    },
                },
                SessionConversationMessage {
                    id: String::from("result-b"),
                    payload: SessionConversationPayload::ToolResult {
                        tool_result_id: String::from("result-b"),
                        tool_call_id: Some(String::from("call-b")),
                        text: String::from("ok"),
                    },
                },
            ],
            records: Vec::new(),
            continuation: Default::default(),
        };

        let lines = runtime_log_lines_from_conversation_state(&state, "1", 1);
        let batch_started = lines
            .iter()
            .filter(|line| matches!(line.event, RuntimeLogEvent::ToolBatchStarted(_)))
            .count();
        let batch_completed = lines
            .iter()
            .find_map(|line| match &line.event {
                RuntimeLogEvent::ToolBatchCompleted(event) => Some(event),
                _ => None,
            })
            .expect("batch completed");

        assert_eq!(batch_started, 1);
        assert_eq!(batch_completed.tool_call_count, 2);
        assert_eq!(batch_completed.tool_result_count, 2);
        assert!(batch_completed.complete);
    }

    #[test]
    fn runtime_log_projection_marks_orphan_tool_result() {
        let state = SessionConversationState {
            messages: vec![SessionConversationMessage {
                id: String::from("result-x"),
                payload: SessionConversationPayload::ToolResult {
                    tool_result_id: String::from("result-x"),
                    tool_call_id: Some(String::from("missing")),
                    text: String::from("orphan"),
                },
            }],
            records: Vec::new(),
            continuation: Default::default(),
        };

        let lines = runtime_log_lines_from_conversation_state(&state, "1", 1);
        let status = lines
            .iter()
            .find_map(|line| match &line.event {
                RuntimeLogEvent::ToolResult(event) => Some(&event.status),
                _ => None,
            })
            .expect("tool result");

        assert_eq!(status, &RuntimeToolResultStatus::Orphan);
    }

    /// Verifies repaired runtime replay groups adjacent display-derived tool-call messages.
    /// 验证修复后的 runtime 重放会合并相邻的展示派生工具调用消息。
    #[test]
    fn runtime_log_replay_groups_adjacent_tool_call_messages() {
        let lines = vec![
            RuntimeLogEventLine::new(
                1,
                "1",
                RuntimeLogEvent::AssistantMessage(RuntimeAssistantMessageEvent {
                    turn_id: Some(String::from("assistant-1")),
                    message_id: String::from("assistant-1"),
                    text: String::new(),
                    reasoning: String::new(),
                    provider_id: None,
                    model_id: None,
                    provider_message_id: None,
                    provider_message_state: None,
                    usage: None,
                }),
            ),
            RuntimeLogEventLine::new(
                2,
                "1",
                RuntimeLogEvent::ToolCall(RuntimeToolCallEvent {
                    turn_id: Some(String::from("assistant-1")),
                    tool_batch_id: String::from("batch-1"),
                    assistant_message_id: String::from("assistant-1"),
                    runtime_tool_call_id: String::from("call-a"),
                    provider_tool_call_id: Some(String::from("call-a")),
                    tool_name: String::from("first_tool"),
                    arguments: json!({ "value": "a" }),
                }),
            ),
            RuntimeLogEventLine::new(
                3,
                "1",
                RuntimeLogEvent::AssistantMessage(RuntimeAssistantMessageEvent {
                    turn_id: Some(String::from("assistant-2")),
                    message_id: String::from("assistant-2"),
                    text: String::new(),
                    reasoning: String::new(),
                    provider_id: None,
                    model_id: None,
                    provider_message_id: None,
                    provider_message_state: None,
                    usage: None,
                }),
            ),
            RuntimeLogEventLine::new(
                4,
                "1",
                RuntimeLogEvent::ToolCall(RuntimeToolCallEvent {
                    turn_id: Some(String::from("assistant-2")),
                    tool_batch_id: String::from("batch-2"),
                    assistant_message_id: String::from("assistant-2"),
                    runtime_tool_call_id: String::from("call-b"),
                    provider_tool_call_id: Some(String::from("call-b")),
                    tool_name: String::from("second_tool"),
                    arguments: json!({ "value": "b" }),
                }),
            ),
            RuntimeLogEventLine::new(
                5,
                "1",
                RuntimeLogEvent::ToolResult(RuntimeToolResultEvent {
                    turn_id: Some(String::from("result-a")),
                    tool_batch_id: Some(String::from("batch-1")),
                    runtime_tool_call_id: Some(String::from("call-a")),
                    provider_tool_call_id: Some(String::from("call-a")),
                    tool_result_id: String::from("result-a"),
                    text: String::from("first result"),
                    status: RuntimeToolResultStatus::Completed,
                    error: None,
                }),
            ),
            RuntimeLogEventLine::new(
                6,
                "1",
                RuntimeLogEvent::ToolResult(RuntimeToolResultEvent {
                    turn_id: Some(String::from("result-b")),
                    tool_batch_id: Some(String::from("batch-2")),
                    runtime_tool_call_id: Some(String::from("call-b")),
                    provider_tool_call_id: Some(String::from("call-b")),
                    tool_result_id: String::from("result-b"),
                    text: String::from("second result"),
                    status: RuntimeToolResultStatus::Completed,
                    error: None,
                }),
            ),
        ];

        let state = conversation_state_from_runtime_log_lines(&lines);

        assert_eq!(state.messages.len(), 3);
        let SessionConversationPayload::Assistant { tool_calls, .. } = &state.messages[0].payload
        else {
            panic!("first replayed message should be a grouped assistant tool batch");
        };
        assert_eq!(tool_calls.len(), 2);
        assert_eq!(tool_calls[0].tool_call_id.as_deref(), Some("call-a"));
        assert_eq!(tool_calls[1].tool_call_id.as_deref(), Some("call-b"));
        assert!(matches!(
            state.messages[1].payload,
            SessionConversationPayload::ToolResult { .. }
        ));
        assert!(matches!(
            state.messages[2].payload,
            SessionConversationPayload::ToolResult { .. }
        ));
    }

    /// Verifies audit-only runtime events never become visible conversation messages.
    /// 验证仅审计运行时事件绝不会变成可见会话消息。
    #[test]
    fn runtime_log_projection_ignores_control_and_audit_events() {
        let lines = vec![
            RuntimeLogEventLine::new(
                1,
                "1",
                RuntimeLogEvent::SystemUpdate(RuntimeSystemUpdateEvent {
                    turn_id: Some(String::from("turn-1")),
                    update_id: String::from("system-update-1"),
                    effective_after_event_id: None,
                    text: String::from("Use terse answers."),
                    trust: RuntimeSystemUpdateTrust::TrustedHost,
                    lowering_hint: RuntimeSystemUpdateLoweringHint::Auto,
                    fingerprint: Some(String::from("sys-fp")),
                    source: Some(String::from("test")),
                }),
            ),
            RuntimeLogEventLine::new(
                2,
                "1",
                RuntimeLogEvent::InjectionCancelled(RuntimeInjectionCancelledEvent {
                    turn_id: Some(String::from("turn-1")),
                    cancellation_id: String::from("cancel-1"),
                    source_node_ids: vec![String::from("node-1")],
                    previous_persistent_fingerprint: Some(String::from("old-persistent")),
                    previous_transient_fingerprint: None,
                    reason: Some(String::from("removed")),
                }),
            ),
            RuntimeLogEventLine::new(
                3,
                "1",
                RuntimeLogEvent::InjectionMerged(RuntimeInjectionMergedEvent {
                    turn_id: Some(String::from("turn-1")),
                    merge_id: String::from("merge-1"),
                    compaction_id: Some(String::from("compact-1")),
                    previous_persistent_fingerprint: Some(String::from("old")),
                    merged_baseline_fingerprint: Some(String::from("new")),
                    source_node_ids: vec![String::from("node-2")],
                }),
            ),
            RuntimeLogEventLine::new(
                4,
                "1",
                RuntimeLogEvent::ProviderRequestCheckpoint(RuntimeProviderRequestCheckpointEvent {
                    turn_id: Some(String::from("turn-1")),
                    checkpoint_id: String::from("request-1"),
                    provider_id: String::from("anthropic"),
                    model_id: String::from("claude"),
                    provider_family: String::from("anthropic"),
                    capability_fingerprint: Some(String::from("cap")),
                    prompt_fingerprint: Some(String::from("prompt")),
                    injection_fingerprint: Some(String::from("inject")),
                    request_fingerprint: Some(String::from("request")),
                    cache_policy_fingerprint: Some(String::from("cache")),
                    cache_policy_diagnostic: Some(String::from("cache_hints_unsupported")),
                    converter_version: 1,
                }),
            ),
            RuntimeLogEventLine::new(
                5,
                "1",
                RuntimeLogEvent::ProviderResponseCheckpoint(
                    RuntimeProviderResponseCheckpointEvent {
                        turn_id: Some(String::from("turn-1")),
                        checkpoint_id: String::from("response-1"),
                        provider_id: String::from("anthropic"),
                        model_id: String::from("claude"),
                        provider_family: String::from("anthropic"),
                        finish_reason: Some(String::from("stop")),
                        native_state: Some(RuntimeProviderNativeStateEnvelope::new(
                            "anthropic",
                            "anthropic",
                            "claude",
                            json!({ "content": [] }),
                        )),
                        usage: None,
                    },
                ),
            ),
            RuntimeLogEventLine::new(
                6,
                "1",
                RuntimeLogEvent::ProviderLoweringRepair(RuntimeProviderLoweringRepairEvent {
                    turn_id: Some(String::from("turn-1")),
                    repair_id: String::from("repair-1"),
                    provider_id: String::from("anthropic"),
                    model_id: String::from("claude"),
                    provider_family: String::from("anthropic"),
                    action: String::from("fallback"),
                    reason: String::from("unsupported system update"),
                    affected_items: 1,
                }),
            ),
        ];

        let state = conversation_state_from_runtime_log_lines(&lines);

        assert!(state.messages.is_empty());
    }
}
