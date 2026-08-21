//! Durable execution-retry snapshot contracts shared by runtime, storage, and service layers.
//! 由运行时、存储层和服务层共享的可靠执行重试快照契约。

use super::*;
use vulcan_ai_contract::{AiExecutionError, AiExecutionRequest};
use vulcan_core::types::{
    ControllerConfig, ConversationSnapshot, ToolInvocationRequest, ToolInvocationResult,
};

/// Strict schema version accepted for execution-retry snapshots.
/// 执行重试快照接受的严格 Schema 版本。
pub const EXECUTION_RETRY_SNAPSHOT_SCHEMA_VERSION: u32 = 8;

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
    /// Build the next Provider request from stable seeded input in the frozen controller state.
    /// 从冻结控制器状态中的稳定播种输入构建下一份 Provider 请求。
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

/// Lightweight route selection frozen for a retry that resumes pre-provider preparation.
/// 为恢复 Provider 调用前准备的重试冻结的轻量路由选择。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionRetryPreparationRouteSelection {
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
}

/// Replay-safety state of one tool call retained in a retry snapshot.
/// 重试快照中保留的单个工具调用之重放安全状态。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ExecutionRetryToolState {
    /// The tool has not started and may be executed after exact binding validation.
    /// 工具尚未开始，可在精确绑定校验后执行。
    NotStarted,
    /// The tool body started but no durable terminal result has been retained yet.
    /// 工具主体已经启动，但尚未保留可靠的终态结果。
    Running,
    /// The tool completed successfully and its exact result is retained in this sidecar.
    /// 工具已成功完成，且精确结果保留在当前 sidecar 中。
    Completed,
    /// The tool completed with an error and its exact result is retained in this sidecar.
    /// 工具以错误结束，且精确结果保留在当前 sidecar 中。
    Failed,
    /// The tool may have produced an external side effect, so automatic replay is forbidden.
    /// 工具可能已产生外部副作用，因此禁止自动重放。
    OutcomeUnknown,
}

/// Stable identity and replay state of one tool call.
/// 单个工具调用的稳定身份与重放状态。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutionRetryToolProgress {
    /// Exact original invocation request emitted by the Provider.
    /// Provider 发出的精确原始调用请求。
    pub request: ToolInvocationRequest,
    /// Exact durable result for a completed or failed invocation.
    /// 已完成或失败调用对应的精确可靠结果。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<ToolInvocationResult>,
    /// Stable fingerprint of the frozen tool binding and schema, or `None` only before the mandatory batch-prepared boundary.
    /// 冻结工具绑定与 Schema 的稳定指纹；仅在强制批次准备边界之前允许为 `None`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binding_fingerprint: Option<String>,
    /// Replay-safety state at the persisted boundary.
    /// 持久化边界处的重放安全状态。
    pub state: ExecutionRetryToolState,
}

/// One user-authored follow-up durably attached to an interrupted execution before recovery starts.
/// 在恢复开始前可靠附着到被中断执行的一条用户补充消息。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionRetryUserFollowup {
    /// User-visible prompt text appended only after prior tool-call reconciliation.
    /// 仅在旧工具调用完成对账后追加的用户可见提示词文本。
    pub visible_prompt: String,
    /// Provider-visible prompt text, including optional host-only guidance.
    /// Provider 可见提示词文本，包含可选的仅宿主可见引导。
    pub provider_prompt: String,
    /// Stable client submission identity used by transcript admission metadata.
    /// 转录准入元数据使用的稳定客户端提交标识。
    pub client_submission_id: String,
}

impl ExecutionRetryUserFollowup {
    /// Returns whether every required follow-up identity and prompt field is non-empty.
    /// 返回补充消息的全部必需身份与提示词字段是否均非空。
    ///
    /// # Returns
    /// 返回值
    /// `true` only for a structurally usable durable recovery follow-up.
    /// 仅对结构上可用的可靠恢复补充消息返回 `true`。
    pub fn is_structurally_valid(&self) -> bool {
        !self.visible_prompt.trim().is_empty()
            && !self.provider_prompt.trim().is_empty()
            && !self.client_submission_id.trim().is_empty()
    }
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
    /// Zero-based physical Provider attempt index reserved for the next request.
    /// 为下一次请求保留的从零开始的物理 Provider attempt 索引。
    pub next_provider_attempt_index: u32,
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
    /// Optional user follow-up appended at the first protocol-safe recovery boundary.
    /// 在首个协议安全恢复边界追加的可选用户补充消息。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_followup: Option<ExecutionRetryUserFollowup>,
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
    /// Frozen provider route identity used to reject route drift before claim.
    /// 用于在领取前拒绝路由漂移的冻结供应商路由身份。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_route: Option<SessionProviderRouteLeaseIdentity>,
    /// Frozen pre-resolution selection when the pending action resumes preparation.
    /// 待执行动作为恢复准备时冻结的解析前选择。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preparation_route_selection: Option<ExecutionRetryPreparationRouteSelection>,
    /// Context-generation identity mirrored for display-time drift rejection.
    /// 为展示阶段拒绝代际漂移而镜像的上下文代际身份。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_generation_id: Option<String>,
    /// SHA-256 digest of provider-visible messages and continuation cursor.
    /// Provider 可见消息与 continuation 游标的 SHA-256 摘要。
    #[serde(default)]
    pub canonical_dispatch_sha256: String,
    /// Whether the complete sidecar proves that automatic replay is safe.
    /// 完整 sidecar 是否证明可以安全自动重放。
    #[serde(default)]
    pub retryable: bool,
    /// Whether an operator must verify an external outcome before continuing.
    /// 继续前是否必须由人工确认外部结果。
    #[serde(default)]
    pub requires_manual_confirmation: bool,
    /// Stable machine-readable reason when replay is blocked.
    /// 阻止重放时的稳定机器可读原因。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocked_reason: Option<String>,
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

impl ExecutionRetrySnapshotRef {
    /// Returns whether this pending action has an executor registered by the current runtime.
    /// 返回当前运行时是否为该待执行动作注册了 Continue 执行器。
    pub fn pending_action_executor_supported(&self) -> bool {
        matches!(
            self.pending_action_label.as_str(),
            "resume_preparation"
                | "dispatch_provider_request"
                | "execute_tool_batch"
                | "prepare_next_provider_request"
                | "finalize_assistant"
        )
    }

    /// Returns whether the reference proves replay safety independent of lifecycle ownership.
    /// 返回引用是否证明重放安全；该判断不包含生命周期所有权。
    pub fn replay_safety_proven(&self) -> bool {
        // Content identity must be structurally readable before the UI may advertise an executable body.
        // 在 UI 宣称正文可执行前，内容身份必须在结构上可读取。
        let content_identity_valid = uuid::Uuid::parse_str(self.snapshot_id.as_str()).is_ok()
            && self.content_bytes > 0
            && self.content_sha256.len() == 64
            && self
                .content_sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
        let action_boundary_complete = match self.pending_action_label.as_str() {
            "resume_preparation" => self.preparation_route_selection.is_some(),
            "dispatch_provider_request"
            | "execute_tool_batch"
            | "prepare_next_provider_request"
            | "finalize_assistant" => {
                self.preparation_route_selection.is_none() && self.provider_route.is_some()
            }
            _ => false,
        };
        self.schema_version == EXECUTION_RETRY_SNAPSHOT_SCHEMA_VERSION
            && content_identity_valid
            && self.retryable
            && !self.requires_manual_confirmation
            && self.blocked_reason.is_none()
            && self.pending_action_executor_supported()
            && action_boundary_complete
            && self.canonical_dispatch_sha256.len() == 64
            && self
                .canonical_dispatch_sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    }

    /// Permanently invalidates this lightweight reference after its replay-safety proof is lost.
    /// 在重放安全证明丢失后永久失效当前轻量引用。
    ///
    /// # Parameters
    /// 参数
    /// `blocked_reason` is the stable machine-readable reason projected through WebUI i18n.
    /// `blocked_reason` 是通过 WebUI i18n 投影的稳定机器可读原因。
    /// `error_summary` is the controlled non-secret diagnostic retained for support inspection.
    /// `error_summary` 是为支持排查保留的受控非敏感诊断。
    /// `updated_at` is the authoritative Unix-millisecond timestamp text for this transition.
    /// `updated_at` 是当前状态迁移使用的权威 Unix 毫秒时间戳文本。
    pub fn invalidate_for_safety_reason(
        &mut self,
        blocked_reason: String,
        error_summary: String,
        updated_at: String,
    ) {
        self.state = ExecutionRetrySnapshotState::Invalidated;
        self.retryable = false;
        self.requires_manual_confirmation = true;
        self.blocked_reason = Some(blocked_reason);
        self.claimed_attempt_id = None;
        self.error_summary = Some(error_summary);
        self.updated_at = updated_at;
    }
}

#[cfg(test)]
mod tests {
    //! Tests for the explicit replay-safety proof carried by the lightweight reference.
    //! 轻量引用携带的显式重放安全证明测试。

    use super::*;

    /// Builds a minimal frozen reference for replay-safety assertions.
    /// 构建用于重放安全断言的最小冻结引用。
    fn safe_reference() -> ExecutionRetrySnapshotRef {
        ExecutionRetrySnapshotRef {
            schema_version: EXECUTION_RETRY_SNAPSHOT_SCHEMA_VERSION,
            snapshot_id: String::from("00000000-0000-0000-0000-000000000001"),
            generation: 1,
            state: ExecutionRetrySnapshotState::Frozen,
            content_sha256: "1".repeat(64),
            content_bytes: 1,
            logical_execution_id: String::from("logical-execution"),
            pending_action_label: String::from("resume_preparation"),
            provider_route: None,
            preparation_route_selection: Some(ExecutionRetryPreparationRouteSelection {
                active_provider_id: None,
                active_connection_id: None,
                active_model_id: None,
            }),
            context_generation_id: None,
            canonical_dispatch_sha256: "0".repeat(64),
            retryable: true,
            requires_manual_confirmation: false,
            blocked_reason: None,
            claimed_attempt_id: None,
            error_summary: None,
            updated_at: String::from("1"),
        }
    }

    /// Verifies that an explicit blocked reason invalidates an otherwise safe reference.
    /// 验证显式阻塞原因会使原本安全的引用失效。
    #[test]
    fn blocked_reason_invalidates_replay_safety_proof() {
        let safe = safe_reference();
        assert!(safe.replay_safety_proven());

        let mut blocked = safe;
        blocked.blocked_reason = Some(String::from("provider_dispatch_outcome_unknown"));
        assert!(!blocked.replay_safety_proven());
    }

    /// Verifies malformed or empty sidecar identities never produce a clickable replay proof.
    /// 验证格式错误或空的 sidecar 身份绝不会产生可点击的重放证明。
    #[test]
    fn malformed_sidecar_identity_invalidates_replay_safety_proof() {
        // malformedSnapshotId proves the reference must name an exact UUID-owned body.
        // malformedSnapshotId 证明引用必须指向由精确 UUID 拥有的正文。
        let mut malformed_snapshot_id = safe_reference();
        malformed_snapshot_id.snapshot_id = String::from("not-a-uuid");
        assert!(!malformed_snapshot_id.replay_safety_proven());

        // malformedDigest proves a syntactically invalid content address cannot reach the button.
        // malformedDigest 证明语法无效的内容地址不能到达按钮。
        let mut malformed_digest = safe_reference();
        malformed_digest.content_sha256 = String::from("digest");
        assert!(!malformed_digest.replay_safety_proven());

        // emptyBody proves a zero-byte sidecar cannot be advertised as executable.
        // emptyBody 证明零字节 sidecar 不能被宣称为可执行。
        let mut empty_body = safe_reference();
        empty_body.content_bytes = 0;
        assert!(!empty_body.replay_safety_proven());
    }

    /// Verifies tool-only recovery and final-assistant recovery both require and accept an exact route lease.
    /// 验证纯工具恢复与最终助手恢复都要求并接受精确路由租约。
    #[test]
    fn route_bound_recovery_actions_have_registered_replay_executors() {
        // route is the frozen Provider identity required by post-preparation recovery actions.
        // route 是准备完成后的恢复动作所要求的冻结 Provider 身份。
        let route = SessionProviderRouteLeaseIdentity {
            schema_version: 1,
            provider_id: String::from("provider-a"),
            connection_id: String::from("connection-a"),
            model_id: String::from("model-a"),
            protocol_profile_id: String::from("openai-responses.v1"),
            manifest_revision: 1,
            manifest_generation: 1,
            catalog_revision: String::from("catalog-a"),
            route_revision: String::from("route-a"),
        };
        for action in ["execute_tool_batch", "finalize_assistant"] {
            // reference isolates one supported action while preserving the same exact sidecar proof.
            // reference 隔离一个受支持动作，同时保留相同的精确 sidecar 证明。
            let mut reference = safe_reference();
            reference.pending_action_label = String::from(action);
            reference.preparation_route_selection = None;
            reference.provider_route = Some(route.clone());

            assert!(reference.pending_action_executor_supported());
            assert!(reference.replay_safety_proven());
        }
    }

    /// Verifies safety invalidation closes lifecycle ownership and retains a concrete diagnostic.
    /// 验证安全失效会关闭生命周期所有权并保留具体诊断。
    #[test]
    fn safety_invalidation_closes_replay_reference() {
        // reference starts as a claimable snapshot owned by one interrupted physical attempt.
        // reference 初始为由一个已中断物理尝试领取的可重放快照。
        let mut reference = safe_reference();
        reference.state = ExecutionRetrySnapshotState::Claimed;
        reference.claimed_attempt_id = Some(String::from("attempt-1"));

        reference.invalidate_for_safety_reason(
            String::from("provider_dispatch_outcome_unknown"),
            String::from("Provider dispatch outcome requires manual verification."),
            String::from("2"),
        );

        assert_eq!(reference.state, ExecutionRetrySnapshotState::Invalidated);
        assert!(!reference.retryable);
        assert!(reference.requires_manual_confirmation);
        assert_eq!(
            reference.blocked_reason.as_deref(),
            Some("provider_dispatch_outcome_unknown")
        );
        assert!(reference.claimed_attempt_id.is_none());
        assert_eq!(
            reference.error_summary.as_deref(),
            Some("Provider dispatch outcome requires manual verification.")
        );
        assert_eq!(reference.updated_at, "2");
        assert!(!reference.replay_safety_proven());
    }
}
