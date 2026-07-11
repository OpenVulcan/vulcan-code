//! Runtime-event data models and schema metadata.
//! 运行时事件数据模型与结构版本元数据。

use serde::{Deserialize, Serialize};
use serde_json::Value;
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
#[serde(deny_unknown_fields)]
pub struct RuntimeToolResultEvent {
    /// Optional logical turn identifier.
    /// 可选的逻辑回合标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    /// Stable local protocol-message identifier that owns this result payload.
    /// 持有该结果载荷的稳定本地协议消息标识。
    pub message_id: String,
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
