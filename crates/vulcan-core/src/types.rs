//! Shared protocol and diagnostic types owned by `vulcan-core`.
//! `vulcan-core` 自有的共享协议与诊断类型。

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::error::Error;
use std::fmt::{Display, Formatter};

/// Describes one controller-level failure that should never be hidden behind the provider runtime.
/// 描述一种不应被 Provider 运行时黑盒掩盖的控制器级失败。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreError {
    /// The controller was asked to act on an invalid or inconsistent state.
    /// 控制器被要求在无效或不一致的状态上继续执行。
    InvalidState {
        /// Human-readable failure detail.
        /// 人类可读的失败细节。
        message: String,
    },
    /// The provider adapter failed while building or sending a request.
    /// Provider 适配器在构建或发送请求时失败。
    ProviderFailure {
        /// Provider identifier attached to the failure.
        /// 与失败绑定的 Provider 标识。
        provider_id: String,
        /// Human-readable failure detail.
        /// 人类可读的失败细节。
        message: String,
    },
    /// The tool executor failed while running a host-side tool invocation.
    /// 工具执行器在运行宿主侧工具调用时失败。
    ToolFailure {
        /// Tool name attached to the failure.
        /// 与失败绑定的工具名称。
        tool_name: String,
        /// Human-readable failure detail.
        /// 人类可读的失败细节。
        message: String,
    },
}

impl Display for CoreError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            CoreError::InvalidState { message } => write!(f, "invalid state: {message}"),
            CoreError::ProviderFailure {
                provider_id,
                message,
            } => write!(f, "provider `{provider_id}` failure: {message}"),
            CoreError::ToolFailure { tool_name, message } => {
                write!(f, "tool `{tool_name}` failure: {message}")
            }
        }
    }
}

impl Error for CoreError {}

/// Represents the stable controller settings that shape every execution turn.
/// 表示塑造每一次执行回合的稳定控制器设置。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ControllerConfig {
    /// Provider identifier used by the active controller session.
    /// 当前控制器会话使用的 Provider 标识。
    pub provider_id: String,
    /// Model identifier passed to the provider adapter.
    /// 传递给 Provider 适配器的模型标识。
    pub model_id: String,
    /// System prompt or preamble applied to every request.
    /// 作用于每一次请求的系统提示词或前置语。
    pub system_prompt: String,
    /// Optional sampling temperature applied to the provider request.
    /// 作用于 Provider 请求的可选采样温度。
    pub temperature: Option<f64>,
    /// Maximum number of tool-bearing turns allowed for one execution.
    /// 单次执行允许的最大工具回合数。
    pub max_turns: usize,
    /// Optional provider-specific additional parameters.
    /// 可选的 Provider 专属附加参数。
    pub additional_params: Option<Value>,
}

/// Represents one provider continuation cursor snapshot kept beside the canonical history.
/// 表示与规范历史并存保存的一份 Provider continuation 游标快照。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProviderContinuation {
    /// Provider identifier that produced the latest cursor state.
    /// 产出最新游标状态的 Provider 标识。
    pub provider_id: Option<String>,
    /// Model identifier that produced the latest cursor state.
    /// 产出最新游标状态的模型标识。
    pub model_id: Option<String>,
    /// Latest provider-visible assistant message identifier.
    /// 最近一次 Provider 可见的助手消息标识。
    pub last_provider_message_id: Option<String>,
    /// Unix-millis timestamp string recorded when the cursor was refreshed.
    /// 游标刷新时记录的 Unix 毫秒时间戳文本。
    pub updated_at: Option<String>,
}

/// Represents the canonical conversation snapshot owned by the controller.
/// 表示由控制器持有的规范会话快照。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConversationSnapshot {
    /// Active canonical message window that will be sent to the next provider turn.
    /// 将发送到下一次 Provider 回合的活动规范消息窗口。
    pub messages: Vec<ConversationMessage>,
    /// Durable record stream retained for transcript projection and host metadata.
    /// 为转录投影与宿主元数据保留的持久化记录流。
    #[serde(default)]
    pub records: Vec<ConversationRecord>,
    /// Provider continuation snapshot used to chain later requests.
    /// 用于串联后续请求的 Provider continuation 快照。
    pub continuation: ProviderContinuation,
}

/// Represents one canonical protocol message stored in the active conversation window.
/// 表示存储在活动会话窗口中的一条规范协议消息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationMessage {
    /// Stable message identifier owned by the local workbench.
    /// 由本地工作台持有的稳定消息标识。
    pub id: String,
    /// Structured payload describing the message contents.
    /// 描述消息内容的结构化载荷。
    pub payload: ConversationPayload,
}

/// Represents one durable transcript record stored outside the active message window.
/// 表示存储在活动消息窗口之外的一条持久化转录记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationRecord {
    /// Stable record identifier shared by projection caches and checkpoints.
    /// 被投影缓存与检查点共享的稳定记录标识。
    pub id: String,
    /// Transcript role attached to the record.
    /// 绑定到该记录的转录角色。
    pub role: TranscriptRole,
    /// Durable record kind that describes whether the record is visible or internal.
    /// 描述该记录是可见还是内部节点的持久化记录类型。
    pub record_kind: TranscriptRecordKind,
    /// Whether the record should appear in the visible transcript projection.
    /// 该记录是否应出现在可见转录投影中。
    pub visible: bool,
    /// Optional display title for the transcript card header.
    /// 转录卡片头部使用的可选显示标题。
    pub title: String,
    /// Primary body text shown in the transcript and modal surfaces.
    /// 在转录与弹层中展示的主要正文文本。
    pub body: String,
    /// Optional structured metadata for host-side audit and transcript-only enrichment.
    /// 供宿主审计与仅转录增强使用的可选结构化元数据。
    pub metadata: Option<Value>,
    /// Unix-millis timestamp string captured when the record was created.
    /// 记录创建时捕获的 Unix 毫秒时间戳文本。
    pub created_at: String,
}

/// Represents one canonical assistant tool-call entry.
/// 表示一条规范的助手工具调用项。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationToolCall {
    /// Stable tool-call item identifier emitted by the provider.
    /// 由 Provider 发出的稳定工具调用项标识。
    pub id: String,
    /// Optional provider-visible call identifier used by tool-result messages.
    /// 工具结果消息使用的可选 Provider 可见调用标识。
    pub tool_call_id: Option<String>,
    /// Tool function name selected by the assistant.
    /// 助手选择的工具函数名称。
    pub tool_name: String,
    /// Structured JSON arguments selected by the assistant.
    /// 助手选择的结构化 JSON 参数。
    pub arguments: Value,
}

/// Represents one canonical user attachment retained for future request replays.
/// 表示一条为后续请求回放保留的规范用户附件。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConversationAttachmentKind {
    /// Audio attachment backed by one local file path.
    /// 由本地文件路径支撑的音频附件。
    Audio,
    /// Image attachment backed by one local file path.
    /// 由本地文件路径支撑的图片附件。
    Image,
    /// Video attachment backed by one local file path.
    /// 由本地文件路径支撑的视频附件。
    Video,
    /// PDF attachment backed by one local file path.
    /// 由本地文件路径支撑的 PDF 附件。
    Pdf,
}

impl ConversationAttachmentKind {
    /// Returns the stable lowercase modality name attached to the current attachment kind.
    /// 返回绑定到当前附件类型上的稳定小写模态名称。
    pub fn modality_name(&self) -> &'static str {
        match self {
            Self::Audio => "audio",
            Self::Image => "image",
            Self::Video => "video",
            Self::Pdf => "pdf",
        }
    }

    /// Returns the compact Chinese label shown in host-side attachment previews and chips.
    /// 返回展示在宿主侧附件预览与标签中的紧凑中文名称。
    pub fn display_label(&self) -> &'static str {
        match self {
            Self::Audio => "音频",
            Self::Image => "图片",
            Self::Video => "视频",
            Self::Pdf => "PDF",
        }
    }
}

/// Represents one structured user attachment kept beside canonical user text.
/// 表示一条保存在规范用户文本旁边的结构化用户附件。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationAttachment {
    /// Stable attachment kind used by capability gates and provider mappers.
    /// 供能力门控与 Provider 映射使用的稳定附件类型。
    pub kind: ConversationAttachmentKind,
    /// Human-readable file name shown in transcript and queue previews.
    /// 展示在转录与队列预览中的人类可读文件名。
    pub file_name: String,
    /// Canonical managed local file path used to re-read the attachment during future provider replays.
    /// 在未来 Provider 回放期间用于重新读取附件的规范托管本地文件路径。
    pub local_path: String,
    /// Optional original source path retained for diagnostics and replay fallback when the managed copy is missing.
    /// 当托管副本缺失时，为诊断与回放回退保留的可选原始来源路径。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
    /// Optional stable managed-asset identifier retained by the host-managed attachment store.
    /// 宿主管理附件存储保留的可选稳定托管资产标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset_id: Option<String>,
    /// Optional source byte length retained for host-side budget estimation and diagnostics.
    /// 为宿主侧预算估算与诊断保留的可选源字节长度。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub byte_len: Option<u64>,
    /// Optional local preview image path derived from the attachment, used by host-side media cards when the primary file is not directly displayable.
    /// 当主文件本身不可直接展示时，供宿主侧媒体卡片使用的、从附件派生出的可选本地预览图片路径。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview_local_path: Option<String>,
    /// Resolved MIME type attached to the local file.
    /// 绑定到本地文件上的已解析 MIME 类型。
    pub mime_type: String,
}

/// Represents one structured canonical payload inside the controller message window.
/// 表示控制器消息窗口内部的一条结构化规范载荷。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConversationPayload {
    /// Plain user-authored text input.
    /// 纯文本用户输入。
    UserText {
        /// Visible user text content.
        /// 用户可见文本内容。
        text: String,
        /// Structured attachments that should travel with the user-authored text.
        /// 应随用户文本一起传递的结构化附件集合。
        #[serde(default)]
        attachments: Vec<ConversationAttachment>,
    },
    /// Assistant turn that may contain visible text, reasoning, and tool calls.
    /// 可能同时包含可见文本、思考内容与工具调用的助手回合。
    Assistant {
        /// Visible assistant text content.
        /// 助手可见文本内容。
        text: String,
        /// Flattened reasoning text kept for local audit only.
        /// 保留给本地审计使用的扁平化思考文本。
        reasoning: String,
        /// Optional provider-native assistant message identifier.
        /// 可选的 Provider 原生助手消息标识。
        provider_message_id: Option<String>,
        /// Optional provider-native assistant state that must survive future history replays.
        /// 必须在后续历史回放中保留下来的可选 Provider 原生助手状态。
        provider_message_state: Option<Value>,
        /// Optional provider usage snapshot retained for host-side budget calibration.
        /// 为宿主侧预算校准保留的可选 Provider 用量快照。
        #[serde(default)]
        usage: Option<ProviderUsageSnapshot>,
        /// Structured tool-call list emitted by the assistant turn.
        /// 由该助手回合发出的结构化工具调用列表。
        tool_calls: Vec<ConversationToolCall>,
    },
    /// Tool-result payload returned to the model after one assistant tool call.
    /// 在一次助手工具调用后回传给模型的工具结果载荷。
    ToolResult {
        /// Stable tool-result identifier emitted by the provider or synthesized locally.
        /// 由 Provider 发出或在本地合成的稳定工具结果标识。
        tool_result_id: String,
        /// Optional provider-visible tool-call identifier linked to the result.
        /// 与该结果关联的可选 Provider 可见工具调用标识。
        tool_call_id: Option<String>,
        /// Flattened human-readable tool-result text.
        /// 已扁平化的人类可读工具结果文本。
        text: String,
    },
}

/// Represents the role that a transcript record should surface in the projection layer.
/// 表示转录记录在投影层应呈现的角色。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TranscriptRole {
    /// User-originated content.
    /// 用户发起的内容。
    User,
    /// Assistant-originated content.
    /// 助手发起的内容。
    Assistant,
    /// Tool-originated content.
    /// 工具发起的内容。
    Tool,
    /// System-originated content.
    /// 系统发起的内容。
    System,
    /// Thinking-only content that should stay local.
    /// 仅供本地保留的思考内容。
    Thinking,
    /// Summary or compaction content.
    /// 摘要或压缩内容。
    Summary,
    /// Hidden explore or inspection content.
    /// 隐藏的探索或检查内容。
    Explore,
    /// Any other role that has not been normalized yet.
    /// 任何尚未完成规范化的其他角色。
    Other,
}

/// Represents the durable record category used by the projection and checkpoint layers.
/// 表示投影层与检查点层使用的持久化记录类别。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TranscriptRecordKind {
    /// The record should appear in the visible transcript.
    /// 该记录应出现在可见转录中。
    Visible,
    /// The record should remain hidden from normal transcript surfaces.
    /// 该记录应对普通转录界面保持隐藏。
    Hidden,
    /// The record is a checkpoint or compaction anchor.
    /// 该记录是检查点或压缩锚点。
    Checkpoint,
    /// The record exists only as a projection helper.
    /// 该记录仅作为投影辅助节点存在。
    ProjectionOnly,
}

/// Represents one tool invocation request that can be executed by the host tool runtime.
/// 表示可由宿主工具运行时执行的一次工具调用请求。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInvocationRequest {
    /// Stable tool-call identifier associated with the request.
    /// 与请求关联的稳定工具调用标识。
    pub tool_call_id: String,
    /// Tool name requested by the assistant.
    /// 助手请求的工具名称。
    pub tool_name: String,
    /// Structured JSON arguments passed to the tool.
    /// 传递给工具的结构化 JSON 参数。
    pub arguments: Value,
}

/// Represents one tool execution result returned by the host runtime.
/// 表示由宿主运行时返回的一次工具执行结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInvocationResult {
    /// Stable tool-call identifier associated with the result.
    /// 与结果关联的稳定工具调用标识。
    pub tool_call_id: String,
    /// Tool name that produced the result.
    /// 产出该结果的工具名称。
    pub tool_name: String,
    /// Text that should be fed back to the model.
    /// 应当回传给模型的文本内容。
    pub model_output: String,
    /// Optional host-side structured metadata reserved for UI and audit consumers.
    /// 供 UI 与审计消费者保留的可选宿主侧结构化元数据。
    pub host_result: Option<Value>,
    /// Whether the tool invocation should be treated as an error by the controller.
    /// 控制器是否应将该工具调用视为错误。
    pub is_error: bool,
}

/// Represents one normalized provider usage snapshot retained for budget calibration and diagnostics.
/// 表示一份被保留用于预算校准与诊断的规范化 Provider 用量快照。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderUsageSnapshot {
    /// Optional provider-reported input token count.
    /// Provider 报告的可选输入 Token 数量。
    #[serde(default)]
    pub input_tokens: Option<usize>,
    /// Optional provider-reported output token count.
    /// Provider 报告的可选输出 Token 数量。
    #[serde(default)]
    pub output_tokens: Option<usize>,
    /// Optional provider-reported reasoning token count.
    /// Provider 报告的可选推理 Token 数量。
    #[serde(default)]
    pub reasoning_tokens: Option<usize>,
    /// Optional provider-reported total token count.
    /// Provider 报告的可选总 Token 数量。
    #[serde(default)]
    pub total_tokens: Option<usize>,
    /// Optional provider-reported cache-hit input token count.
    /// Provider 报告的可选缓存命中输入 Token 数量。
    #[serde(default)]
    pub cached_input_tokens: Option<usize>,
    /// Optional provider-reported cache-creation input token count.
    /// Provider 报告的可选缓存写入输入 Token 数量。
    #[serde(default)]
    pub cache_creation_input_tokens: Option<usize>,
}

impl ProviderUsageSnapshot {
    /// Returns whether the snapshot contains at least one concrete token count.
    /// 返回该快照是否至少包含一个具体 Token 数量。
    pub const fn has_any_tokens(&self) -> bool {
        self.input_tokens.is_some()
            || self.output_tokens.is_some()
            || self.reasoning_tokens.is_some()
            || self.total_tokens.is_some()
            || self.cached_input_tokens.is_some()
            || self.cache_creation_input_tokens.is_some()
    }
}

/// Represents one normalized provider turn response consumed by the host-owned controller loop.
/// 表示一份由宿主自有控制器循环消费的规范化 Provider 单回合响应。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderTurnResponse {
    /// Visible assistant text returned by the provider.
    /// Provider 返回的助手可见文本。
    pub assistant_text: String,
    /// Flattened assistant reasoning retained for local audit only.
    /// 仅保留给本地审计使用的扁平化助手思考内容。
    pub assistant_reasoning: String,
    /// Optional provider-native assistant message identifier.
    /// 可选的 Provider 原生助手消息标识。
    pub provider_message_id: Option<String>,
    /// Optional provider-native assistant state that must survive canonical history replay.
    /// 必须在规范历史回放中保留下来的可选 Provider 原生助手状态。
    pub provider_message_state: Option<Value>,
    /// Optional provider usage snapshot retained for host-side budget calibration.
    /// 为宿主侧预算校准保留的可选 Provider 用量快照。
    #[serde(default)]
    pub usage: Option<ProviderUsageSnapshot>,
    /// Structured tool calls returned by the provider for the current turn.
    /// Provider 在当前回合返回的结构化工具调用集合。
    #[serde(default)]
    pub tool_calls: Vec<ConversationToolCall>,
    /// Optional continuation cursor snapshot refreshed by the provider.
    /// 由 Provider 刷新的可选 continuation 游标快照。
    pub continuation: Option<ProviderContinuation>,
}

/// Represents one streaming delta emitted while the provider is still building the normalized turn response.
/// 表示 Provider 在构建规范化回合响应期间持续发出的单条流式增量。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ProviderTurnDelta {
    /// One reasoning delta emitted by the provider stream.
    /// Provider 流发出的一条思考增量。
    Thinking {
        /// Raw visible reasoning delta text.
        /// 原始可见思考增量文本。
        delta: String,
    },
    /// One assistant output delta emitted by the provider stream.
    /// Provider 流发出的一条助手输出增量。
    Output {
        /// Raw visible assistant output delta text.
        /// 原始可见助手输出增量文本。
        delta: String,
    },
}

/// Represents one summary of a completed host-owned controller loop.
/// 表示一份已完成的宿主自有控制器循环摘要。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControllerRunResult {
    /// Total provider turns executed during the loop.
    /// 循环期间实际执行的 Provider 回合总数。
    pub provider_turns_executed: usize,
    /// Total tool calls processed during the loop, including skipped calls.
    /// 循环期间处理过的工具调用总数，包含被策略跳过的调用。
    pub tool_calls_processed: usize,
    /// Final assistant message identifier produced by the loop when one exists.
    /// 循环结束时产出的最终助手消息标识（如果存在）。
    pub final_assistant_message_id: Option<String>,
}

/// Represents one request snapshot captured before a provider call is sent.
/// 表示在发送 Provider 调用前捕获的一份请求快照。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderRequestSnapshot {
    /// Provider identifier used for the request.
    /// 用于该请求的 Provider 标识。
    pub provider_id: String,
    /// Model identifier used for the request.
    /// 用于该请求的模型标识。
    pub model_id: String,
    /// System prompt attached to the request.
    /// 附加到请求中的系统提示词。
    pub system_prompt: String,
    /// Latest outgoing prompt message body attached to the request.
    /// 附加到请求中的最新出站提示消息正文。
    pub prompt: String,
    /// Canonical history forwarded into the request.
    /// 传入请求的规范历史。
    pub history: Vec<ConversationMessage>,
    /// Temperature attached to the request.
    /// 附加到请求中的温度参数。
    pub temperature: Option<f64>,
    /// Maximum number of tool turns allowed for this request.
    /// 该请求允许的最大工具回合数。
    pub max_turns: usize,
    /// Tool schemas exposed to the provider for the current turn.
    /// 当前回合对 Provider 暴露的工具 schema 集合。
    pub tool_schemas: Vec<ProviderRequestToolSchema>,
    /// Optional provider-specific additional parameters.
    /// 可选的 Provider 专属附加参数。
    pub additional_params: Option<Value>,
    /// Optional serialized request headers that will be sent to the provider.
    /// 将要发送给 Provider 的可选序列化请求头。
    pub request_headers: Option<Value>,
    /// Continuation cursor snapshot that will participate in request chaining.
    /// 将参与请求串联的 continuation 游标快照。
    pub continuation: ProviderContinuation,
}

/// Represents one tool schema exposed to the provider inside a request snapshot.
/// 表示请求快照中暴露给 Provider 的单个工具 schema。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderRequestToolSchema {
    /// Stable tool identifier exposed to the provider.
    /// 暴露给 Provider 的稳定工具标识。
    pub id: String,
    /// Human-readable tool name exposed to the provider.
    /// 暴露给 Provider 的人类可读工具名称。
    pub name: String,
    /// Human-readable tool description exposed to the provider.
    /// 暴露给 Provider 的人类可读工具描述。
    pub description: String,
    /// Stable tool source label used for diagnostics and UI inspection.
    /// 用于诊断与界面检查的稳定工具来源标签。
    pub source_label: String,
    /// JSON Schema describing the tool arguments sent to the provider.
    /// 描述发送给 Provider 的工具参数 JSON Schema。
    pub parameters_schema: Value,
}

/// Represents one controller event emitted for the GUI, logs, and persistence adapters.
/// 表示供 GUI、日志与持久化适配器消费的一条控制器事件。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreEvent {
    /// Event kind describing the semantic phase.
    /// 描述语义阶段的事件类型。
    pub kind: CoreEventKind,
    /// Human-readable event title.
    /// 人类可读的事件标题。
    pub title: String,
    /// Human-readable event body.
    /// 人类可读的事件正文。
    pub body: String,
    /// Optional structured metadata reserved for host-side consumers.
    /// 供宿主侧消费者保留的可选结构化元数据。
    pub metadata: Option<Value>,
}

/// Represents the semantic kind of a controller event.
/// 表示控制器事件的语义类型。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CoreEventKind {
    /// Assistant text emitted by the provider stream.
    /// Provider 流式输出的助手文本。
    Output,
    /// Assistant reasoning emitted by the provider stream.
    /// Provider 流式输出的助手思考内容。
    Thinking,
    /// A tool call has started.
    /// 一次工具调用已开始。
    ToolStarted,
    /// A tool call has finished.
    /// 一次工具调用已完成。
    ToolFinished,
    /// A final response or final history snapshot has been produced.
    /// 已产生最终回复或最终历史快照。
    Final,
    /// The controller encountered an error.
    /// 控制器遇到错误。
    Error,
}
