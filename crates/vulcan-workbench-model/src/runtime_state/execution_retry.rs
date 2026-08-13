//! Durable execution-retry snapshot contracts shared by runtime, storage, and service layers.
//! 由运行时、存储层和服务层共享的可靠执行重试快照契约。

use super::*;
use vulcan_ai_contract::{AiExecutionError, AiExecutionRequest};
use vulcan_core::types::{ControllerConfig, ConversationSnapshot};

/// Strict schema version accepted for execution-retry snapshots.
/// 执行重试快照接受的严格 Schema 版本。
pub const EXECUTION_RETRY_SNAPSHOT_SCHEMA_VERSION: u32 = 1;

/// Lifecycle state of one persisted execution-retry snapshot.
/// 单个持久化执行重试快照的生命周期状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionRetrySnapshotState {
    /// The snapshot is complete and may be claimed exactly once.
    /// 快照完整且可以被精确领取一次。
    Frozen,
    /// One physical execution attempt currently owns the snapshot.
    /// 当前有一个物理执行尝试拥有该快照。
    Claimed,
    /// The restored execution committed its terminal state successfully.
    /// 恢复的执行已成功提交终态。
    Consumed,
    /// A changed or unsafe boundary permanently prevents replay.
    /// 已变化或不安全的边界永久阻止重放。
    Invalidated,
}

/// Exact next action represented by a retry checkpoint.
/// 重试 checkpoint 所表示的精确下一步动作。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ExecutionRetryPendingAction {
    /// Resume deterministic preparation from a frozen accepted submission.
    /// 从冻结且已接受的提交恢复确定性准备。
    ResumePreparation {
        /// Exact immutable preparation input captured after canonical user-message acceptance.
        /// 在规范用户消息接纳后捕获的精确不可变准备输入。
        input: Box<ExecutionRetryPreparationInput>,
    },
    /// Dispatch an exact provider-neutral request already persisted before transport execution.
    /// 分派在传输执行前已经持久化的精确供应商无关请求。
    DispatchProviderRequest {
        /// Exact request bytes represented as the public provider-neutral contract.
        /// 以公共供应商无关契约表示的精确请求内容。
        request: Box<AiExecutionRequest>,
    },
    /// Continue an already-materialized tool batch without repeating completed tools.
    /// 继续已物化的工具批次且不重复已完成工具。
    ExecuteToolBatch {
        /// Stable tool-call progress in original model order.
        /// 按模型原始顺序保存的稳定工具调用进度。
        tools: Vec<ExecutionRetryToolProgress>,
    },
    /// Build the next provider request from completed tool results in the frozen controller state.
    /// 从冻结控制器状态中的已完成工具结果构建下一份 Provider 请求。
    PrepareNextProviderRequest,
    /// Commit a provider-proven final assistant result without another provider request.
    /// 在不再次请求 Provider 的情况下提交已由 Provider 证明的最终助手结果。
    FinalizeAssistant,
}

/// Immutable accepted-submission input required to resume pre-provider preparation.
/// 恢复 Provider 调用前准备所需的不可变已接纳提交输入。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutionRetryPreparationInput {
    /// Stable logical turn identity created by the original accepted submission.
    /// 原始已接纳提交创建的稳定逻辑回合身份。
    pub preparation_turn_id: String,
    /// Human-readable session title frozen for diagnostics and prompt assembly.
    /// 为诊断和提示词装配冻结的会话标题。
    pub session_title: String,
    /// Exact workspace path frozen for the accepted submission.
    /// 为已接纳提交冻结的精确工作区路径。
    pub workspace_path: String,
    /// Session-selected provider identity frozen before route resolution.
    /// 在路由解析前冻结的会话级 Provider 选择身份。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_provider_id: Option<String>,
    /// Session-selected connection identity frozen before route resolution.
    /// 在路由解析前冻结的会话级连接选择身份。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_connection_id: Option<String>,
    /// Session-selected model identity frozen before route resolution.
    /// 在路由解析前冻结的会话级模型选择身份。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_model_id: Option<String>,
    /// Original logical source whose completed-turn semantics must be preserved.
    /// 必须保留完成回合语义的原始逻辑来源。
    pub original_source: PendingSessionPromptSource,
    /// Sanitized original prompt retained independently from the WebUI composer.
    /// 独立于 WebUI 输入框保留的已清理原始提示词。
    pub prompt: String,
    /// Optional model-only guidance frozen with the original submission.
    /// 与原始提交一并冻结的可选仅模型可见引导。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hidden_model_guidance: Option<String>,
    /// Provider-visible agent messages serialized through their authoritative serde contract.
    /// 通过权威 serde 契约序列化的 Provider 可见智能体消息。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provider_visible_agent_messages: Vec<serde_json::Value>,
    /// Structured attachments bound to the original accepted user message.
    /// 绑定到原始已接纳用户消息的结构化附件。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<ConversationAttachment>,
    /// Exact execution mode selected for the original submission.
    /// 原始提交选择的精确执行模式。
    pub execution_mode: String,
    /// Exact optional reasoning effort selected for the original submission.
    /// 原始提交选择的精确可选推理强度。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<ReasoningEffortLevel>,
    /// Session-boundary events serialized through their authoritative serde contract.
    /// 通过权威 serde 契约序列化的会话边界事件。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub boundary_events: Vec<serde_json::Value>,
    /// Exact canonical user-message identity already appended before interruption.
    /// 中断前已经追加的精确规范用户消息身份。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canonical_user_message_id: Option<String>,
    /// Exact precheck Hook payload prepared by the original logical turn.
    /// 原始逻辑回合准备的精确 precheck Hook 载荷。
    pub precheck_payload: serde_json::Value,
    /// Exact profile Hook payload prepared by the original logical turn.
    /// 原始逻辑回合准备的精确画像 Hook 载荷。
    pub profile_payload: serde_json::Value,
}

/// Replay-safety state of one tool call retained in a retry snapshot.
/// 重试快照中保留的单个工具调用之重放安全状态。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ExecutionRetryToolState {
    /// The tool has not started and may be executed after exact binding validation.
    /// 工具尚未开始，可在精确绑定校验后执行。
    NotStarted,
    /// The tool completed and its exact result is retained in the canonical controller snapshot.
    /// 工具已完成，且精确结果已保留在规范控制器快照中。
    Completed,
    /// The tool may have produced an external side effect, so automatic replay is forbidden.
    /// 工具可能已产生外部副作用，因此禁止自动重放。
    OutcomeUnknown,
}

/// Stable identity and replay state of one tool call.
/// 单个工具调用的稳定身份与重放状态。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutionRetryToolProgress {
    /// Provider-issued stable tool-call identity.
    /// Provider 签发的稳定工具调用身份。
    pub tool_call_id: String,
    /// Exact registered public tool name.
    /// 精确注册的公共工具名称。
    pub tool_name: String,
    /// Stable fingerprint of the frozen tool binding and schema.
    /// 冻结工具绑定与 Schema 的稳定指纹。
    pub binding_fingerprint: String,
    /// Replay-safety state at the persisted boundary.
    /// 持久化边界处的重放安全状态。
    pub state: ExecutionRetryToolState,
}

/// Complete sidecar payload required to resume the same logical execution.
/// 恢复同一逻辑执行所需的完整 sidecar 载荷。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionRetrySnapshot {
    /// Strict snapshot schema version.
    /// 严格快照 Schema 版本。
    pub schema_version: u32,
    /// Content-independent snapshot identity.
    /// 与内容无关的快照身份。
    pub snapshot_id: String,
    /// Owning session identity.
    /// 所属会话身份。
    pub session_id: String,
    /// Monotonic retry generation within the session.
    /// 会话内单调递增的重试代际。
    pub generation: u64,
    /// Authoritative sidecar lifecycle state mirrored by the lightweight reference.
    /// 由轻量引用镜像的权威 sidecar 生命周期状态。
    pub state: ExecutionRetrySnapshotState,
    /// Stable logical execution identity preserved across physical attempts.
    /// 跨物理尝试保留的稳定逻辑执行身份。
    pub logical_execution_id: String,
    /// Stable logical turn identity when one exists.
    /// 存在时的稳定逻辑回合身份。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logical_turn_id: Option<String>,
    /// Physical attempt that published this boundary.
    /// 发布当前边界的物理尝试身份。
    pub physical_attempt_id: String,
    /// Physical attempt that currently claims the snapshot.
    /// 当前领取快照的物理尝试身份。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claimed_attempt_id: Option<String>,
    /// Frozen non-secret provider route identity when route resolution already completed.
    /// 路由解析已经完成时冻结的非秘密 Provider 路由身份。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_route: Option<SessionProviderRouteLeaseIdentity>,
    /// Exact provider service entry selected for the frozen request.
    /// 冻结请求选择的精确 Provider 服务入口。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_service_entry_id: Option<String>,
    /// Exact credential profile identity selected for the frozen request.
    /// 冻结请求选择的精确凭据 Profile 身份。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_profile_id: Option<String>,
    /// Exact controller configuration used by the logical execution.
    /// 逻辑执行使用的精确控制器配置。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub controller_config: Option<ControllerConfig>,
    /// Exact canonical controller snapshot at the replay-safe boundary.
    /// 可回放安全边界处的精确规范控制器快照。
    pub controller_snapshot: ConversationSnapshot,
    /// Exact next action to execute after a successful claim.
    /// 成功领取后要执行的精确下一步动作。
    pub pending_action: ExecutionRetryPendingAction,
    /// Structured terminal or interruption error that froze this snapshot.
    /// 冻结当前快照的结构化终态或中断错误。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<AiExecutionError>,
    /// Canonical conversation digest used to reject edits, forks, and changed context generations.
    /// 用于拒绝编辑、分叉和上下文代际变化的规范会话摘要。
    pub canonical_digest: String,
    /// Context-generation identity frozen with the request.
    /// 与请求一起冻结的上下文代际身份。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_generation_id: Option<String>,
    /// Creation timestamp as Unix-millis text.
    /// Unix 毫秒文本形式的创建时间。
    pub created_at: String,
    /// Last mutation timestamp as Unix-millis text.
    /// Unix 毫秒文本形式的最近修改时间。
    pub updated_at: String,
}

/// Lightweight main-state reference and eligibility summary for one sidecar snapshot.
/// 单个 sidecar 快照在主状态中的轻量引用与资格摘要。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionRetrySnapshotRef {
    /// Strict reference schema version.
    /// 严格引用 Schema 版本。
    pub schema_version: u32,
    /// Referenced snapshot identity.
    /// 被引用的快照身份。
    pub snapshot_id: String,
    /// Monotonic retry generation.
    /// 单调重试代际。
    pub generation: u64,
    /// Current claim lifecycle state.
    /// 当前领取生命周期状态。
    pub state: ExecutionRetrySnapshotState,
    /// SHA-256 digest of the exact sidecar bytes.
    /// 精确 sidecar 字节的 SHA-256 摘要。
    pub content_sha256: String,
    /// Exact serialized sidecar size in bytes.
    /// sidecar 序列化后的精确字节数。
    pub content_bytes: u64,
    /// Logical execution identity mirrored for cheap eligibility checks.
    /// 为低成本资格检查镜像的逻辑执行身份。
    pub logical_execution_id: String,
    /// Stable pending-action label mirrored without the large payload.
    /// 在不携带大载荷的情况下镜像的稳定待执行动作标签。
    pub pending_action_label: String,
    /// Physical attempt that currently claims the snapshot.
    /// 当前领取快照的物理尝试身份。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claimed_attempt_id: Option<String>,
    /// Controlled last-error summary for user-visible retry diagnostics.
    /// 用于用户可见重试诊断的受控最近错误摘要。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_summary: Option<String>,
    /// Last reference mutation timestamp as Unix-millis text.
    /// Unix 毫秒文本形式的最近引用修改时间。
    pub updated_at: String,
}
