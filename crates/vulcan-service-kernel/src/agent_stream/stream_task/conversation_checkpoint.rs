//! Root conversation checkpoint event handling.
//! 根会话检查点事件处理。

use vulcan_agent_runtime::agent_runtime::ExecutionPreparation;
use vulcan_agent_runtime::agent_runtime::events::ToolExecutionRecoveryBoundary;
use vulcan_agent_runtime::conversation_protocol::SessionConversationState;
use vulcan_ai_contract::AiExecutionRequest;
use vulcan_core::controller::INTERJECTED_PROMPT_ADMISSION_ID_KEY;
use vulcan_core::types::{
    ControllerConfig, ConversationPayload, ConversationSnapshot, ProviderUsageSnapshot,
    TranscriptRecordKind, TranscriptRole,
};
use vulcan_workbench_model::{
    EXECUTION_RETRY_SNAPSHOT_SCHEMA_VERSION, ExecutionRetryPendingAction, ExecutionRetrySnapshot,
    ExecutionRetrySnapshotState, ExecutionRetryToolProgress, ExecutionRetryToolState,
    ExecutionRetryUserFollowup,
};

use super::*;

/// Stable blocker used when an exact per-tool recovery boundary is absent.
/// 精确逐工具恢复边界缺失时使用的稳定阻断原因。
pub(crate) const TOOL_RECOVERY_BOUNDARY_MISSING_REASON: &str =
    "ERR_CONVERSATION_CONTINUE_RETRY_TOOL_BOUNDARY_MISSING";

/// Stable blocker used when the current tool binding differs from the retained binding.
/// 当前工具绑定与保留绑定不一致时使用的稳定阻断原因。
pub(crate) const TOOL_RECOVERY_BINDING_MISMATCH_REASON: &str =
    "ERR_CONVERSATION_CONTINUE_RETRY_TOOL_BINDING_MISMATCH";

/// Stable blocker used when a retained tool result cannot be bound exactly.
/// 保留工具结果无法精确绑定时使用的稳定阻断原因。
pub(crate) const TOOL_RECOVERY_RESULT_MISMATCH_REASON: &str =
    "ERR_CONVERSATION_CONTINUE_RETRY_TOOL_RESULT_MISMATCH";

/// Validates that one prepared boundary is a complete duplicate-free tool batch.
/// 校验一个已准备边界是完整且无重复的工具批次。
///
/// # Parameters
/// 参数
/// `requests` is the exact Provider batch presented by the runtime bridge.
/// `requests` 是运行时桥接层提供的精确 Provider 批次。
/// `expected_slot_count` is the retained batch size when an earlier boundary already materialized slots.
/// `expected_slot_count` 是较早边界已经物化槽位时的保留批次大小。
///
/// # Returns
/// 返回值
/// Success only when every tool-call identity occurs once and the optional retained cardinality matches.
/// 仅当每个工具调用身份恰好出现一次且可选保留基数一致时返回成功。
fn validate_prepared_tool_batch_shape(
    requests: &[vulcan_agent_runtime::agent_runtime::events::ToolExecutionRecoveryRequest],
    expected_slot_count: Option<usize>,
) -> Result<(), String> {
    if requests.is_empty() {
        return Err(coded_tool_recovery_error(
            TOOL_RECOVERY_BOUNDARY_MISSING_REASON,
            "A durable tool recovery batch cannot be empty.",
        ));
    }
    // preparedToolCallIds proves the incoming boundary is duplicate-free before it can materialize or compare retained slots.
    // preparedToolCallIds 在传入边界能够物化或比较保留槽位前证明其不存在重复项。
    let prepared_tool_call_ids = requests
        .iter()
        .map(|prepared| prepared.request.tool_call_id.as_str())
        .collect::<std::collections::HashSet<_>>();
    if prepared_tool_call_ids.len() != requests.len() {
        return Err(coded_tool_recovery_error(
            TOOL_RECOVERY_BOUNDARY_MISSING_REASON,
            "The prepared tool batch contains duplicate tool-call identities.",
        ));
    }
    if expected_slot_count.is_some_and(|expected| expected != requests.len()) {
        return Err(coded_tool_recovery_error(
            TOOL_RECOVERY_BOUNDARY_MISSING_REASON,
            "The prepared tool batch does not contain every retained tool-call slot.",
        ));
    }
    Ok(())
}

/// Builds one coded internal error whose prefix can be projected to an i18n Continue explanation.
/// 构建带稳定前缀的内部错误，使其能够投影为 i18n Continue 说明。
///
/// # Parameters
/// 参数
/// `reason` is the stable localized blocker selector.
/// `reason` 是稳定的本地化阻断原因选择器。
/// `detail` is the controlled diagnostic retained only for logs and retry audit state.
/// `detail` 是仅保留在日志与重试审计状态中的受控诊断信息。
///
/// # Returns
/// 返回值
/// A coded diagnostic string with the stable reason as its exact prefix.
/// 返回以稳定原因作为精确前缀的编码诊断字符串。
fn coded_tool_recovery_error(reason: &str, detail: impl AsRef<str>) -> String {
    format!("{reason}: {}", detail.as_ref())
}

/// Selects the precise Continue blocker represented by a tool-recovery persistence error.
/// 选择工具恢复持久化错误所代表的精确 Continue 阻断原因。
///
/// # Parameters
/// 参数
/// `error` is the coded internal persistence error.
/// `error` 是已编码的内部持久化错误。
///
/// # Returns
/// 返回值
/// A stable i18n selector, or the generic transition blocker for non-structural failures.
/// 返回稳定的 i18n 选择器；非结构性失败返回通用状态转换阻断原因。
pub(crate) fn tool_recovery_boundary_blocked_reason(error: &str) -> &'static str {
    if error.starts_with(TOOL_RECOVERY_BOUNDARY_MISSING_REASON) {
        TOOL_RECOVERY_BOUNDARY_MISSING_REASON
    } else if error.starts_with(TOOL_RECOVERY_BINDING_MISMATCH_REASON) {
        TOOL_RECOVERY_BINDING_MISMATCH_REASON
    } else if error.starts_with(TOOL_RECOVERY_RESULT_MISMATCH_REASON) {
        TOOL_RECOVERY_RESULT_MISMATCH_REASON
    } else {
        "retry_snapshot_transition_failed"
    }
}

/// Returns the controlled audit summary for one exact tool-recovery blocker.
/// 返回一个精确工具恢复阻断原因对应的受控审计摘要。
///
/// # Parameters
/// 参数
/// `blocked_reason` is the stable reason selected from the persistence failure.
/// `blocked_reason` 是从持久化失败中选择出的稳定原因。
///
/// # Returns
/// 返回值
/// A non-sensitive English summary retained beside the blocked retry reference.
/// 返回与被阻断重试引用一同保留的非敏感英文摘要。
pub(crate) fn tool_recovery_boundary_error_summary(blocked_reason: &str) -> &'static str {
    match blocked_reason {
        TOOL_RECOVERY_BOUNDARY_MISSING_REASON => {
            "The exact per-tool recovery boundary is missing or incomplete."
        }
        TOOL_RECOVERY_BINDING_MISMATCH_REASON => {
            "The current tool binding differs from the binding retained by the interrupted call."
        }
        TOOL_RECOVERY_RESULT_MISMATCH_REASON => {
            "A retained tool result cannot be bound exactly to its interrupted tool call."
        }
        _ => "The exact per-tool recovery boundary could not be durably published.",
    }
}

/// Reads the exact immutable retry reference that owns an incoming tool boundary.
/// 读取拥有传入工具边界的精确不可变重试引用。
///
/// # Parameters
/// 参数
/// `task_state` owns the retained session runtime state.
/// `task_state` 持有保留的会话运行态。
/// `session_id` selects the exact conversation receiving the tool event.
/// `session_id` 选择接收工具事件的精确会话。
///
/// # Returns
/// 返回值
/// The current full reference, or a coded boundary-missing error when no owner exists.
/// 返回当前完整引用；不存在所有者时返回已编码的边界缺失错误。
pub(crate) fn execution_retry_reference_for_tool_boundary(
    task_state: &SharedServiceBridgeState,
    session_id: SessionId,
) -> Result<vulcan_workbench_model::ExecutionRetrySnapshotRef, String> {
    let guard = task_state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    guard
        .workspace_snapshot
        .as_ref()
        .and_then(|workspace| workspace.session_runtime_state.get(&session_id))
        .and_then(|runtime_state| runtime_state.execution_retry_snapshot.clone())
        .ok_or_else(|| {
            coded_tool_recovery_error(
                TOOL_RECOVERY_BOUNDARY_MISSING_REASON,
                "Execution retry reference is missing at a mandatory tool recovery boundary.",
            )
        })
}

/// Structured failure produced while a claimed execution retry is returned to frozen state.
/// 将已领取执行重试退回冻结状态时产生的结构化失败。
#[derive(Debug)]
pub(crate) enum ExecutionRetryRefreezeFailure {
    /// The immutable retry body could not be loaded or validated against its reference.
    /// 不可变重试正文无法加载，或无法通过引用一致性校验。
    BodyUnreadable(String),
    /// Canonical refresh, persistence, or shared-state publication failed.
    /// 规范状态刷新、持久化或共享状态发布失败。
    TransitionFailed(String),
}

impl ExecutionRetryRefreezeFailure {
    /// Returns the stable blocked reason projected to the localized Continue UI.
    /// 返回投影到本地化 Continue 界面的稳定阻断原因。
    ///
    /// # Returns
    /// 返回值
    /// A stable machine reason that selects one concrete localized explanation.
    /// 用于选择具体本地化说明的稳定机器原因。
    pub(crate) const fn blocked_reason(&self) -> &'static str {
        match self {
            Self::BodyUnreadable(_) => {
                crate::conversation_submission::EXECUTION_RETRY_SNAPSHOT_BODY_UNREADABLE_REASON
            }
            Self::TransitionFailed(_) => "retry_snapshot_transition_failed",
        }
    }

    /// Returns the controlled diagnostic summary retained beside the blocked retry reference.
    /// 返回与被阻断重试引用一同保留的受控诊断摘要。
    ///
    /// # Returns
    /// 返回值
    /// A non-sensitive summary that describes the failed safety boundary.
    /// 描述失败安全边界且不包含敏感信息的摘要。
    pub(crate) const fn error_summary(&self) -> &'static str {
        match self {
            Self::BodyUnreadable(_) => {
                crate::conversation_submission::EXECUTION_RETRY_SNAPSHOT_BODY_UNREADABLE_SUMMARY
            }
            Self::TransitionFailed(_) => {
                "The execution retry snapshot could not complete its safe state transition."
            }
        }
    }
}

impl std::fmt::Display for ExecutionRetryRefreezeFailure {
    /// Formats the internal diagnostic detail for logs without changing the stable UI reason.
    /// 格式化用于日志的内部诊断详情，同时不改变稳定界面原因。
    ///
    /// # Parameters
    /// 参数
    /// `formatter` receives the diagnostic text.
    /// `formatter` 接收诊断文本。
    ///
    /// # Returns
    /// 返回值
    /// The formatter result after writing the exact internal detail.
    /// 写入精确内部详情后的格式化结果。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BodyUnreadable(detail) | Self::TransitionFailed(detail) => {
                formatter.write_str(detail)
            }
        }
    }
}

/// Projects one canonical conversation checkpoint into optional live-usage views and retained session state.
/// 将单个规范会话检查点投影到可选实时用量视图与保留会话状态。
///
/// # Parameters
/// # 参数
/// `app` emits workbench updates required by the canonical checkpoint projection.
/// `app` 发送规范检查点投影所需的工作台更新。
/// `task_state` owns live context budgets, session goal summaries, and retained workspace state.
/// `task_state` 持有实时上下文预算、会话目标摘要与保留工作区状态。
/// `controller_runtime` persists canonical main-session checkpoint state.
/// `controller_runtime` 持久化规范主会话检查点状态。
/// `session_id` identifies the root provider-stream session.
/// `session_id` 标识根 Provider 流会话。
/// `conversation_state` is the canonical provider conversation state for this checkpoint.
/// `conversation_state` 是当前检查点对应的规范 Provider 会话状态。
/// `last_successful_provider_usage` is the latest successful provider usage snapshot, when available.
/// `last_successful_provider_usage` 是最近一次成功 Provider 调用的用量快照（如存在）。
/// # Returns
/// # 返回值
/// This function returns no value; each projection failure is logged independently so canonical checkpoint application is always attempted.
/// 此函数无返回值；每个投影失败都会独立记录，从而保证规范检查点始终会被尝试应用。
pub(super) fn handle_conversation_checkpoint_stream_event(
    app: &ServiceEventEmitter,
    task_state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    session_id: SessionId,
    conversation_state: SessionConversationState,
    last_successful_provider_usage: Option<ProviderUsageSnapshot>,
) {
    // Live context budget usage is an optional projection; its failure must not block goal accounting or the canonical checkpoint.
    // 实时上下文预算用量是可选投影；其失败不得阻塞目标计量或规范检查点。
    if let Some(usage) = last_successful_provider_usage.as_ref()
        && let Err(error) = cache_live_context_budget_observed_usage(
            task_state,
            session_id,
            usage,
            &conversation_state,
        )
    {
        eprintln!("[webui-service] live context budget usage update failed: {error}");
    }

    // Canonical checkpoint state is authoritative and must be attempted after all optional projections regardless of their outcomes.
    // 规范检查点状态具有权威性，无论可选投影结果如何，都必须在其后尝试应用。
    if let Err(error) = apply_conversation_checkpoint(
        app,
        task_state,
        controller_runtime,
        session_id,
        conversation_state,
    ) {
        eprintln!("[webui-service] apply checkpoint failed: {error}");
    }
}

/// Persists an exact pre-dispatch retry snapshot and publishes its lightweight runtime reference.
/// 持久化一份精确的分派前重试快照并发布其轻量运行态引用。
///
/// # Parameters
/// 参数
/// - `task_state`: Retained service state that owns the active stream and retry reference.
/// - `task_state`：拥有活动流与重试引用的保留服务状态。
/// - `controller_runtime`: Durable workspace store used for the immutable sidecar body.
/// - `controller_runtime`：用于不可变 sidecar 正文的可靠工作区存储。
/// - `session_id`: Exact session that owns the request.
/// - `session_id`：拥有该请求的精确会话。
/// - `active_stream_task_id`: Physical service-stream attempt that may publish the checkpoint.
/// - `active_stream_task_id`：获准发布检查点的物理服务流 attempt。
/// - `logical_execution_id`: Telemetry-owned logical Provider request identity.
/// - `logical_execution_id`：由遥测拥有的逻辑 Provider 请求身份。
/// - `logical_turn_id`: Stable logical conversation turn retained across Continue.
/// - `logical_turn_id`：跨 Continue 保留的稳定逻辑会话回合。
/// - `provider_attempt_index`: Exact zero-based physical Provider attempt being prepared.
/// - `provider_attempt_index`：正在准备的精确从零开始物理 Provider attempt。
/// - `preparation`: Frozen execution and route facts.
/// - `preparation`：冻结的执行与路由事实。
/// - `exact_request`: Provider-neutral request that may be replayed.
/// - `exact_request`：可以重放的 Provider 中立请求。
/// - `controller_config`: Exact Core controller configuration.
/// - `controller_config`：精确 Core 控制器配置。
/// - `controller_snapshot`: Canonical Core state immediately before dispatch.
/// - `controller_snapshot`：分派前一刻的规范 Core 状态。
///
/// # Returns
/// 返回值
/// Success only after the immutable sidecar is durable and its exact reference is retained.
/// 仅在不可变 sidecar 已可靠持久化且其精确引用已保留后返回成功。
pub(super) fn persist_prepared_execution_retry_checkpoint(
    task_state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    session_id: SessionId,
    active_stream_task_id: Uuid,
    logical_execution_id: String,
    logical_turn_id: Option<String>,
    provider_attempt_index: u32,
    preparation: &ExecutionPreparation,
    exact_request: AiExecutionRequest,
    controller_config: ControllerConfig,
    controller_snapshot: ConversationSnapshot,
) -> Result<(), String> {
    // physicalAttemptId is the exact service stream owner allowed to advance this checkpoint.
    // physicalAttemptId 是获准推进当前 checkpoint 的精确服务流所有者。
    let physical_attempt_id = active_stream_task_id.to_string();
    // retainedFacts are read under one lock so generation, logical identity, and route cannot split.
    // retainedFacts 在同一把锁下读取，避免代际、逻辑身份和路由发生分裂。
    let (snapshot_id, generation, provider_route) = {
        let guard = task_state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        if !active_stream_task_matches_id(&guard, session_id, active_stream_task_id) {
            return Err(String::from(
                "Execution retry checkpoint lost exact active-stream ownership before persistence.",
            ));
        }
        let runtime_state = guard
            .workspace_snapshot
            .as_ref()
            .and_then(|workspace| workspace.session_runtime_state.get(&session_id))
            .ok_or_else(|| String::from("Execution retry checkpoint runtime state is missing."))?;
        let reusable_reference =
            runtime_state
                .execution_retry_snapshot
                .as_ref()
                .filter(|reference| {
                    matches!(
                        reference.state,
                        ExecutionRetrySnapshotState::Claimed | ExecutionRetrySnapshotState::Frozen
                    )
                });
        let snapshot_id = reusable_reference
            .map(|reference| reference.snapshot_id.clone())
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let generation = runtime_state
            .execution_retry_snapshot
            .as_ref()
            .map(|reference| reference.generation)
            .unwrap_or_default()
            .checked_add(1)
            .ok_or_else(|| String::from("Execution retry generation overflowed."))?;
        if reusable_reference
            .is_some_and(|reference| reference.logical_execution_id != logical_execution_id)
        {
            return Err(String::from(
                "Execution retry checkpoint logical identity drifted from the active Provider request.",
            ));
        }
        let provider_route = runtime_state.provider_route_lease.clone().ok_or_else(|| {
            String::from("Execution retry checkpoint has no exact frozen provider route.")
        })?;
        (snapshot_id, generation, provider_route)
    };
    // nextProviderAttemptIndex advances only from the exact telemetry-owned current attempt.
    // nextProviderAttemptIndex 仅从遥测拥有的精确当前 attempt 推进。
    let next_provider_attempt_index = provider_attempt_index
        .checked_add(1)
        .ok_or_else(|| String::from("Provider attempt index overflowed."))?;
    // canonicalDigest covers the complete Core state rather than a transcript projection.
    // canonicalDigest 覆盖完整 Core 状态，而不是转录投影。
    let canonical_bytes = serde_json::to_vec(&controller_snapshot)
        .map_err(|error| format!("Failed to serialize retry checkpoint Core state: {error}"))?;
    let canonical_digest = sha256_hex(&canonical_bytes);
    let now = current_unix_millis_text();
    let snapshot = ExecutionRetrySnapshot {
        schema_version: EXECUTION_RETRY_SNAPSHOT_SCHEMA_VERSION,
        snapshot_id,
        session_id: session_id.to_string(),
        generation,
        state: ExecutionRetrySnapshotState::Frozen,
        logical_execution_id,
        logical_turn_id,
        next_provider_attempt_index,
        physical_attempt_id,
        claimed_attempt_id: None,
        provider_route: Some(provider_route),
        provider_service_entry_id: Some(preparation.model_blueprint.service_entry_id.clone()),
        credential_profile_id: preparation
            .model_blueprint
            .frozen_selection
            .as_ref()
            .map(|selection| selection.route.connection.credential_profile_id.clone()),
        controller_config: Some(controller_config),
        controller_snapshot,
        pending_action: ExecutionRetryPendingAction::DispatchProviderRequest {
            request: Box::new(exact_request),
        },
        user_followup: None,
        error: None,
        canonical_digest,
        context_generation_id: preparation.provider_usage_context_generation_id.clone(),
        created_at: now.clone(),
        updated_at: now.clone(),
    };
    let reference = vulcan_workspace_store::workspace_store::persist_execution_retry_snapshot(
        &controller_runtime.data_root,
        session_id,
        &snapshot,
    )?;
    let mut guard = task_state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    if !active_stream_task_matches_id(&guard, session_id, active_stream_task_id) {
        return Err(String::from(
            "Execution retry checkpoint lost exact active-stream ownership before reference publication.",
        ));
    }
    let runtime_state = guard
        .workspace_snapshot
        .as_mut()
        .and_then(|workspace| workspace.session_runtime_state.get_mut(&session_id))
        .ok_or_else(|| String::from("Execution retry checkpoint runtime state is missing."))?;
    runtime_state.execution_retry_snapshot = Some(reference);
    runtime_state.updated_at = now;
    Ok(())
}

/// Clears the live retry reference after the accepted success projection is committed in memory.
/// 在已接纳成功投影提交到内存后清除实时重试引用。
///
/// # Parameters
/// 参数
/// - `task_state`: Shared retained workspace state whose reference is cleared.
/// - `task_state`：清除引用的共享保留工作区状态。
/// - `session_id`: Session whose successful execution no longer needs recovery.
/// - `session_id`：成功执行后不再需要恢复的会话。
///
/// # Returns
/// 返回值
/// The detached snapshot id for post-persistence garbage collection, or `None` when no reference existed.
/// 返回供可靠持久化后垃圾清理使用的已分离快照标识；原本没有引用时返回 `None`。
pub(super) fn clear_execution_retry_snapshot_after_success(
    task_state: &SharedServiceBridgeState,
    session_id: SessionId,
) -> Result<Option<String>, String> {
    let reference = {
        let mut guard = task_state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let runtime_state = guard
            .workspace_snapshot
            .as_mut()
            .and_then(|workspace| workspace.session_runtime_state.get_mut(&session_id))
            .ok_or_else(|| {
                String::from("Execution retry runtime state disappeared while removing success.")
            })?;
        let reference = runtime_state.execution_retry_snapshot.take();
        runtime_state.updated_at = current_unix_millis_text();
        reference
    };
    Ok(reference.map(|reference| reference.snapshot_id))
}

/// Returns a claimed retry snapshot to the frozen state after stop or abnormal stream termination.
/// 在停止或异常流终止后将已认领重试快照退回冻结状态。
pub(crate) fn refreeze_claimed_execution_retry_snapshot(
    task_state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    session_id: SessionId,
    provider_call_binding: Option<(Uuid, String)>,
    message: &str,
) -> Result<(), ExecutionRetryRefreezeFailure> {
    let retry_state = {
        let guard = task_state.lock().map_err(|error| {
            ExecutionRetryRefreezeFailure::TransitionFailed(format!(
                "failed to lock service state: {error}"
            ))
        })?;
        guard
            .workspace_snapshot
            .as_ref()
            .and_then(|workspace| workspace.session_runtime_state.get(&session_id))
            .map(|runtime_state| {
                (
                    runtime_state.execution_retry_snapshot.clone(),
                    runtime_state.conversation_state.clone(),
                )
            })
    };
    let Some((reference, conversation_state)) = retry_state else {
        return Ok(());
    };
    refreeze_claimed_execution_retry_snapshot_from_canonical_state(
        task_state,
        controller_runtime,
        session_id,
        reference,
        &conversation_state,
        ExecutionRetryInterruptionEvidence::RuntimeBoundary(provider_call_binding),
        message,
    )
}

/// Describes the exact dispatch evidence available while refreezing an interrupted retry snapshot.
/// 描述重新冻结中断重试快照时可用的精确分派证据。
#[derive(Clone, Debug)]
enum ExecutionRetryInterruptionEvidence {
    /// The stream ran and may provide an exact stream-to-provider-call binding.
    /// 流已经运行，并且可能提供精确的流到 Provider 调用绑定。
    RuntimeBoundary(Option<(Uuid, String)>),
    /// The failure boundary precedes Provider dispatch, which proves that this attempt dispatched no request.
    /// 失败边界位于 Provider 分派之前，因此能够证明本次尝试没有分派请求。
    ProvenNotDispatchedBeforeProviderDispatch,
}

/// Returns a claimed retry snapshot to frozen state using an exact caller-owned canonical state.
/// 使用调用方持有的精确规范状态，将已认领重试快照退回冻结状态。
///
/// # Parameters
/// 参数
/// `task_state` owns the live retry reference, `controller_runtime` owns persistence, and `session_id` selects the session.
/// `task_state` 持有实时重试引用，`controller_runtime` 持有持久化能力，`session_id` 选择会话。
/// `conversation_state` is the authoritative post-mutation canonical state, `provider_call_binding` identifies the exact latest physical call when present, and `message` explains the interruption.
/// `conversation_state` 是变更后的权威规范状态，`provider_call_binding` 在存在时标识精确的最近物理调用，`message` 说明中断原因。
///
/// # Returns
/// 返回值
/// Success after the sidecar and shared reference are updated, or an explicit persistence/state error.
/// 在 sidecar 与共享引用更新后返回成功，否则返回明确的持久化或状态错误。
pub(crate) fn refreeze_claimed_execution_retry_snapshot_with_canonical_state(
    task_state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    session_id: SessionId,
    conversation_state: &SessionConversationState,
    provider_call_binding: Option<(Uuid, String)>,
    message: &str,
) -> Result<(), ExecutionRetryRefreezeFailure> {
    // reference is read from the shared claim owner while canonical content comes from the caller.
    // reference 从共享领取所有者读取，而规范内容来自调用方。
    let reference = {
        let guard = task_state.lock().map_err(|error| {
            ExecutionRetryRefreezeFailure::TransitionFailed(format!(
                "failed to lock service state: {error}"
            ))
        })?;
        guard
            .workspace_snapshot
            .as_ref()
            .and_then(|workspace| workspace.session_runtime_state.get(&session_id))
            .and_then(|runtime_state| runtime_state.execution_retry_snapshot.clone())
    };
    refreeze_claimed_execution_retry_snapshot_from_canonical_state(
        task_state,
        controller_runtime,
        session_id,
        reference,
        conversation_state,
        ExecutionRetryInterruptionEvidence::RuntimeBoundary(provider_call_binding),
        message,
    )
}

/// Returns a claimed retry snapshot to frozen state after a failure proven to precede Provider dispatch.
/// 在已证明发生于 Provider 分派前的失败之后，将已领取重试快照退回冻结状态。
///
/// # Parameters
/// 参数
/// `task_state` owns the exact live retry reference, `controller_runtime` owns sidecar storage, and `session_id` selects the failed execution.
/// `task_state` 持有精确实时重试引用，`controller_runtime` 持有 sidecar 存储，`session_id` 选择失败执行。
/// `message` explains the pre-dispatch failure retained with a newly created interruption error when needed.
/// `message` 说明分派前失败；需要创建新的中断错误时会随错误一同保留。
///
/// # Returns
/// 返回值
/// Success after publishing a frozen replay boundary, or an explicit sidecar/state failure.
/// 发布冻结重放边界后返回成功，否则返回明确的 sidecar 或状态失败。
pub(crate) fn refreeze_claimed_execution_retry_snapshot_before_provider_dispatch(
    task_state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    session_id: SessionId,
    message: &str,
) -> Result<(), ExecutionRetryRefreezeFailure> {
    // retryState captures the exact reference and canonical state before immutable sidecar publication.
    // retryState 在发布不可变 sidecar 前捕获精确引用与规范状态。
    let retry_state = {
        let guard = task_state.lock().map_err(|error| {
            ExecutionRetryRefreezeFailure::TransitionFailed(format!(
                "failed to lock service state: {error}"
            ))
        })?;
        guard
            .workspace_snapshot
            .as_ref()
            .and_then(|workspace| workspace.session_runtime_state.get(&session_id))
            .map(|runtime_state| {
                (
                    runtime_state.execution_retry_snapshot.clone(),
                    runtime_state.conversation_state.clone(),
                )
            })
    };
    let Some((reference, conversation_state)) = retry_state else {
        return Ok(());
    };
    refreeze_claimed_execution_retry_snapshot_from_canonical_state(
        task_state,
        controller_runtime,
        session_id,
        reference,
        &conversation_state,
        ExecutionRetryInterruptionEvidence::ProvenNotDispatchedBeforeProviderDispatch,
        message,
    )
}

/// Persists one refrozen sidecar from an exact retry reference and canonical conversation state.
/// 根据精确重试引用与规范会话状态持久化一份重新冻结的 sidecar。
///
/// # Parameters
/// 参数
/// All parameters are authoritative values captured by the public refreeze entrypoints.
/// 所有参数均为公共重冻入口捕获的权威值。
///
/// # Returns
/// 返回值
/// Success for an absent/ineligible reference or after atomically publishing the refreshed reference.
/// 引用缺失或不符合条件时返回成功；否则在原子发布刷新引用后返回成功。
fn refreeze_claimed_execution_retry_snapshot_from_canonical_state(
    task_state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    session_id: SessionId,
    reference: Option<vulcan_workbench_model::ExecutionRetrySnapshotRef>,
    conversation_state: &SessionConversationState,
    interruption_evidence: ExecutionRetryInterruptionEvidence,
    message: &str,
) -> Result<(), ExecutionRetryRefreezeFailure> {
    let Some(reference) = reference else {
        return Ok(());
    };
    if !matches!(
        reference.state,
        ExecutionRetrySnapshotState::Claimed | ExecutionRetrySnapshotState::Frozen
    ) {
        return Ok(());
    }
    let mut snapshot = vulcan_workspace_store::workspace_store::load_execution_retry_snapshot(
        &controller_runtime.data_root,
        session_id,
        &reference,
    )
    .map_err(ExecutionRetryRefreezeFailure::BodyUnreadable)?;
    // canonicalActionAdvanced reconciles every Provider-visible input committed after the frozen
    // checkpoint before terminal classification decides whether an interruption error is needed.
    // canonicalActionAdvanced 在终态分类判断是否需要中断错误前，协调冻结检查点之后提交的全部
    // Provider 可见输入。
    let canonical_action_advanced =
        reconcile_interrupted_execution_retry_canonical_state(&mut snapshot, conversation_state)
            .map_err(ExecutionRetryRefreezeFailure::TransitionFailed)?;
    snapshot.state = ExecutionRetrySnapshotState::Frozen;
    snapshot.claimed_attempt_id = None;
    snapshot.updated_at = current_unix_millis_text();
    if !canonical_action_advanced
        && should_replace_stop_interruption_error(snapshot.error.as_ref(), &snapshot.pending_action)
    {
        // providerInterruptionError uses only the call id atomically bound to this exact stream owner.
        // providerInterruptionError 仅使用原子绑定到当前精确流所有者的调用标识。
        // interruptionError preserves the strongest exact dispatch fact available at this boundary.
        // interruptionError 保留当前边界可用的最强精确分派事实。
        let interruption_error = match &interruption_evidence {
            ExecutionRetryInterruptionEvidence::RuntimeBoundary(provider_call_binding) => {
                provider_interruption_error_from_exact_binding(
                    task_state,
                    session_id,
                    &snapshot,
                    provider_call_binding.as_ref(),
                    message,
                )?
                .unwrap_or_else(|| interruption_error_without_provider_evidence(&snapshot, message))
            }
            ExecutionRetryInterruptionEvidence::ProvenNotDispatchedBeforeProviderDispatch => {
                interruption_error_with_dispatch_outcome(
                    &snapshot,
                    message,
                    vulcan_ai_contract::DispatchOutcome::NotDispatched,
                )
            }
        };
        snapshot.error = Some(interruption_error);
    }
    let updated_reference =
        vulcan_workspace_store::workspace_store::persist_execution_retry_snapshot(
            &controller_runtime.data_root,
            session_id,
            &snapshot,
        )
        .map_err(ExecutionRetryRefreezeFailure::TransitionFailed)?;
    // updatedReplaySafetyProven and updatedAt are copied before the reference moves into shared state.
    // updatedReplaySafetyProven 与 updatedAt 在引用移入共享状态前完成复制。
    let updated_replay_safety_proven = updated_reference.replay_safety_proven();
    let updated_at = updated_reference.updated_at.clone();
    let mut guard = task_state.lock().map_err(|error| {
        ExecutionRetryRefreezeFailure::TransitionFailed(format!(
            "failed to lock service state: {error}"
        ))
    })?;
    let runtime_state = guard
        .workspace_snapshot
        .as_mut()
        .and_then(|workspace| workspace.session_runtime_state.get_mut(&session_id))
        .ok_or_else(|| {
            ExecutionRetryRefreezeFailure::TransitionFailed(String::from(
                "Execution retry runtime state disappeared while refreezing.",
            ))
        })?;
    // A newer reference owns any concurrent transition; the completed immutable write becomes harmless garbage and must not overwrite it.
    // 如果并发迁移已经发布新引用，则由新引用拥有状态；本次已完成的不可变写入仅成为无害垃圾，绝不能覆盖它。
    if runtime_state.execution_retry_snapshot.as_ref() != Some(&reference) {
        return Ok(());
    }
    runtime_state.execution_retry_snapshot = Some(updated_reference);
    // continuationEligible refreshes the mirrored cache after a Claimed body becomes Frozen again.
    // continuationEligible 在 Claimed 正文重新变为 Frozen 后刷新镜像缓存。
    let continuation_eligible = {
        // ledger is the exact terminal Provider-call ledger paired with the refreshed reference.
        // ledger 是与刷新引用配对的精确终态 Provider 调用账本。
        let ledger = &runtime_state.provider_call_ledger;
        // continuation is the terminal generation whose cached eligibility is being reconciled.
        // continuation 是正在协调缓存资格的终态代际。
        let continuation = &ledger.continuation;
        // canonicalTailKind normalizes an absent presentation row to the exact ledger vocabulary label.
        // canonicalTailKind 将缺失的展示行规范化为精确的账本词汇标签。
        let canonical_tail_kind =
            crate::canonical_conversation_tail_kind(runtime_state).unwrap_or("empty");
        // canonicalTailMatches detects presentation drift without treating assistant or empty UI tails as protocol safety evidence.
        // canonicalTailMatches 检测展示漂移，但不把助手或空 UI 尾部当作协议安全证据。
        let canonical_tail_matches =
            crate::is_prompt_free_continuation_tail_kind(canonical_tail_kind)
                && continuation.canonical_tail_kind.as_deref() == Some(canonical_tail_kind);
        updated_replay_safety_proven
            && canonical_tail_matches
            && continuation.claimed_at_ms.is_none()
            && continuation.invalidated_at_ms.is_none()
            && continuation.terminal_at_ms.is_some()
            && continuation
                .terminal_reason
                .as_deref()
                .is_some_and(|reason| !reason.is_empty())
            && continuation.expected_ledger_revision == ledger.revision
            && ledger.in_flight_call_count() == 0
    };
    runtime_state.provider_call_ledger.continuation.eligible = continuation_eligible;
    runtime_state.updated_at = updated_at;
    Ok(())
}

/// Resolves a structured interruption error from one exact stream-to-provider-call binding.
/// 从一个精确的流到 Provider 调用绑定解析结构化中断错误。
///
/// # Parameters
/// 参数
/// `task_state` owns the authoritative Provider ledger, `session_id` selects its session, `snapshot` owns the frozen route and stream identity, `provider_call_binding` names the exact physical call, and `message` explains the interruption.
/// `task_state` 持有权威 Provider 账本，`session_id` 选择其会话，`snapshot` 持有冻结路由与流身份，`provider_call_binding` 命名精确物理调用，`message` 说明中断原因。
///
/// # Returns
/// 返回值
/// An exact error only when ownership, route, terminal status, dispatch, semantic-output, and replay facts all agree; otherwise `None` fails closed through the generic unknown boundary.
/// 仅当所有权、路由、终态、分派、语义输出与重放事实全部一致时返回精确错误；否则返回 `None`，并由通用未知边界失败关闭。
fn provider_interruption_error_from_exact_binding(
    task_state: &SharedServiceBridgeState,
    session_id: SessionId,
    snapshot: &ExecutionRetrySnapshot,
    provider_call_binding: Option<&(Uuid, String)>,
    message: &str,
) -> Result<Option<vulcan_ai_contract::AiExecutionError>, ExecutionRetryRefreezeFailure> {
    if !matches!(
        &snapshot.pending_action,
        ExecutionRetryPendingAction::DispatchProviderRequest { .. }
    ) {
        return Ok(None);
    }
    // binding is absent before the first durable physical-call Started boundary.
    // binding 在首个可靠物理调用 Started 边界前不存在。
    let Some((stream_task_id, provider_call_id)) = provider_call_binding else {
        return Ok(None);
    };
    if snapshot.physical_attempt_id != stream_task_id.to_string() {
        return Ok(None);
    }
    // providerCallRecord is selected by the exact bound key rather than recency or turn heuristics.
    // providerCallRecord 按精确绑定键选择，而不是依赖最近记录或回合启发式判断。
    let provider_call_record = {
        let guard = task_state.lock().map_err(|error| {
            ExecutionRetryRefreezeFailure::TransitionFailed(format!(
                "failed to lock service state: {error}"
            ))
        })?;
        guard
            .workspace_snapshot
            .as_ref()
            .and_then(|workspace| workspace.session_runtime_state.get(&session_id))
            .and_then(|runtime_state| {
                runtime_state
                    .provider_call_ledger
                    .calls
                    .get(provider_call_id.as_str())
            })
            .cloned()
    };
    let Some(provider_call_record) = provider_call_record else {
        return Ok(None);
    };
    let Some(provider_route) = snapshot.provider_route.as_ref() else {
        return Ok(None);
    };
    // identityMatches proves the bound ledger record belongs to the exact frozen execution route.
    // identityMatches 证明已绑定账本记录属于精确冻结执行路由。
    let identity_matches = provider_call_record.active_branch_member
        && provider_call_record.superseded_by_edit_generation.is_none()
        && provider_call_record.identity.owner_kind == "main_agent"
        && provider_call_record.identity.operation == "conversation"
        && provider_call_record.identity.logical_request_id == snapshot.logical_execution_id
        && provider_call_record.identity.turn_id == snapshot.logical_turn_id
        && provider_call_record.context_generation_id == snapshot.context_generation_id
        && provider_call_record.identity.provider_id == provider_route.provider_id
        && provider_call_record.identity.connection_id == provider_route.connection_id
        && provider_call_record.identity.model_id == provider_route.model_id
        && provider_call_record.identity.protocol_profile_id == provider_route.protocol_profile_id
        && provider_call_record.identity.manifest_revision == provider_route.manifest_revision
        && provider_call_record.identity.manifest_generation == provider_route.manifest_generation
        && provider_call_record.identity.route_revision == provider_route.route_revision
        && snapshot.provider_service_entry_id.as_deref()
            == Some(provider_call_record.identity.service_entry_id.as_str());
    if !identity_matches {
        return Ok(None);
    }
    // dispatchOutcome is accepted only with the ledger's explicit known bit and registered value.
    // dispatchOutcome 仅在账本明确 known 且值已注册时才被接纳。
    let dispatch_outcome = match (
        provider_call_record.dispatch_outcome_known,
        provider_call_record.dispatch_outcome.as_deref(),
    ) {
        (Some(true), Some("dispatched")) => vulcan_ai_contract::DispatchOutcome::Dispatched,
        (Some(true), Some("not_dispatched")) => vulcan_ai_contract::DispatchOutcome::NotDispatched,
        _ => return Ok(None),
    };
    // semanticOutputObserved requires both an explicit value and its matching proof bit.
    // semanticOutputObserved 同时要求显式值及其匹配的证明位。
    let semantic_output_observed = match (
        provider_call_record.semantic_output_observed_known,
        provider_call_record.semantic_output_observed,
    ) {
        (Some(true), Some(value)) => value,
        _ => return Ok(None),
    };
    // retryable comes from the terminal Provider replay-safety fact and is never inferred from status text.
    // retryable 来自终态 Provider 重放安全事实，绝不根据状态文本推断。
    let Some(retryable) = provider_call_record.replay_safe else {
        return Ok(None);
    };
    // subcategory preserves the exact registered terminal class without exposing Provider text.
    // subcategory 保留精确注册终态类别，且不暴露 Provider 文本。
    let subcategory = match provider_call_record.status {
        vulcan_workbench_model::SessionProviderCallStatus::Cancelled => "execution_interrupted",
        vulcan_workbench_model::SessionProviderCallStatus::Failed => "provider_request_failed",
        vulcan_workbench_model::SessionProviderCallStatus::TimedOut => "provider_request_timed_out",
        _ => return Ok(None),
    };
    Ok(Some(vulcan_ai_contract::AiExecutionError {
        category: vulcan_ai_contract::AiErrorCategory::Cancelled,
        subcategory: Some(String::from(subcategory)),
        stage: String::from("provider_request"),
        http_status: None,
        transport: None,
        websocket_close_code: None,
        upstream_code: None,
        upstream_type: None,
        upstream_parameter: None,
        upstream_message: None,
        request_id: None,
        provider_id: Some(provider_call_record.identity.provider_id),
        entry_id: Some(provider_call_record.identity.service_entry_id),
        model_id: Some(provider_call_record.identity.model_id),
        message: message.to_string(),
        dispatch_outcome,
        semantic_output_observed,
        side_effect_observed: false,
        terminal: true,
        retryable,
        retry_after: None,
    }))
}

/// Builds the fail-closed interruption error used when no exact Provider call evidence is available.
/// 构建在缺少精确 Provider 调用证据时使用的失败关闭中断错误。
///
/// # Parameters
/// 参数
/// `snapshot` owns the pending action and frozen route, while `message` explains the interruption.
/// `snapshot` 持有待执行动作与冻结路由，`message` 说明中断原因。
///
/// # Returns
/// 返回值
/// A retryable pre-dispatch error only when the pending action itself proves no dispatch; Provider dispatch otherwise remains unknown and blocked.
/// 仅当待执行动作本身证明尚未分派时返回可重试错误；否则 Provider 分派保持未知并被阻断。
fn interruption_error_without_provider_evidence(
    snapshot: &ExecutionRetrySnapshot,
    message: &str,
) -> vulcan_ai_contract::AiExecutionError {
    // dispatchOutcome is derived only from the pending action's explicit execution boundary.
    // dispatchOutcome 仅从待执行动作的显式执行边界推导。
    let dispatch_outcome = stop_interruption_dispatch_outcome(&snapshot.pending_action);
    interruption_error_with_dispatch_outcome(snapshot, message, dispatch_outcome)
}

/// Builds a retryable interruption error from one exact dispatch outcome.
/// 根据一个精确分派结果构建可重试的中断错误。
///
/// # Parameters
/// 参数
/// `snapshot` supplies the frozen route identity, `message` explains the interruption, and `dispatch_outcome` records the proven dispatch fact.
/// `snapshot` 提供冻结路由身份，`message` 说明中断原因，`dispatch_outcome` 记录已证明的分派事实。
///
/// # Returns
/// 返回值
/// A terminal, side-effect-free, retryable execution error carrying the supplied dispatch evidence.
/// 返回携带给定分派证据的终态、无副作用、可重试执行错误。
fn interruption_error_with_dispatch_outcome(
    snapshot: &ExecutionRetrySnapshot,
    message: &str,
    dispatch_outcome: vulcan_ai_contract::DispatchOutcome,
) -> vulcan_ai_contract::AiExecutionError {
    vulcan_ai_contract::AiExecutionError {
        category: vulcan_ai_contract::AiErrorCategory::Cancelled,
        subcategory: Some(String::from("execution_interrupted")),
        stage: String::from("service_execution"),
        http_status: None,
        transport: None,
        websocket_close_code: None,
        upstream_code: None,
        upstream_type: None,
        upstream_parameter: None,
        upstream_message: None,
        request_id: None,
        provider_id: snapshot
            .provider_route
            .as_ref()
            .map(|route| route.provider_id.clone()),
        entry_id: snapshot.provider_service_entry_id.clone(),
        model_id: snapshot
            .provider_route
            .as_ref()
            .map(|route| route.model_id.clone()),
        message: message.to_string(),
        dispatch_outcome,
        semantic_output_observed: false,
        side_effect_observed: false,
        terminal: true,
        retryable: true,
        retry_after: None,
    }
}

/// Returns whether a retry action proves that no Provider request can have been dispatched yet.
/// 返回某个重试动作是否证明当前尚未分派任何 Provider 请求。
fn action_can_prove_no_provider_dispatch(pending_action: &ExecutionRetryPendingAction) -> bool {
    matches!(
        pending_action,
        ExecutionRetryPendingAction::ResumePreparation { .. }
            | ExecutionRetryPendingAction::ExecuteToolBatch { .. }
            | ExecutionRetryPendingAction::PrepareNextProviderRequest
            | ExecutionRetryPendingAction::FinalizeAssistant
    )
}

/// Selects the structured dispatch fact emitted for a stop interruption.
/// 选择停止中断要写入的结构化分派事实。
fn stop_interruption_dispatch_outcome(
    pending_action: &ExecutionRetryPendingAction,
) -> vulcan_ai_contract::DispatchOutcome {
    if action_can_prove_no_provider_dispatch(pending_action) {
        vulcan_ai_contract::DispatchOutcome::NotDispatched
    } else {
        vulcan_ai_contract::DispatchOutcome::Unknown
    }
}

/// Returns whether stop may replace an existing interruption marker without hiding unsafe facts.
/// 返回停止是否可以替换现有中断标记且不掩盖不安全事实。
///
/// # Parameters
/// 参数
/// `error` is the structured error already retained by the retry sidecar.
/// `error` 是重试 sidecar 已保留的结构化错误。
/// `pending_action` is the exact action represented by the sidecar.
/// `pending_action` 是 sidecar 表示的精确待执行动作。
///
/// # Returns
/// 返回值
/// `true` only when the sidecar has no error or an unknown dispatch is proven absent by the action.
/// 仅当 sidecar 没有错误，或动作证明未知分派实际不存在时返回 `true`。
fn should_replace_stop_interruption_error(
    error: Option<&vulcan_ai_contract::AiExecutionError>,
    pending_action: &ExecutionRetryPendingAction,
) -> bool {
    match error {
        None => true,
        Some(error) => {
            action_can_prove_no_provider_dispatch(pending_action)
                && error.retryable
                && !error.semantic_output_observed
                && !error.side_effect_observed
                && error.subcategory.as_deref() != Some("provider_dispatch_outcome_unknown")
                && error.dispatch_outcome == vulcan_ai_contract::DispatchOutcome::Unknown
        }
    }
}

/// Determines whether one retained recovery supplement already entered canonical history.
/// 判断一条保留的恢复补充消息是否已经进入规范历史。
///
/// # Parameters
/// 参数
/// `previous` is the controller snapshot frozen before the recovery claim.
/// `previous` 是恢复领取前冻结的控制器快照。
/// `current` is the latest canonical controller snapshot observed while refreezing.
/// `current` 是重新冻结时观测到的最新规范控制器快照。
/// `followup` is the exact durable supplement whose admission identity must occur once.
/// `followup` 是其准入身份必须恰好出现一次的精确可靠补充消息。
///
/// # Returns
/// 返回值
/// `true` only when one exact visible admission record and its Provider-visible user message are retained.
/// 仅当一条精确可见准入记录及其 Provider 可见用户消息均已保留时返回 `true`。
fn execution_retry_user_followup_was_committed(
    previous: &ConversationSnapshot,
    current: &ConversationSnapshot,
    followup: &ExecutionRetryUserFollowup,
) -> Result<bool, String> {
    let admission_id = followup.client_submission_id.trim();
    let visible_prompt = followup.visible_prompt.trim();
    let provider_prompt = followup.provider_prompt.trim();
    // matchingRecords is selected only by the stable admission identity created by the recovery claim.
    // matchingRecords 仅按恢复领取创建的稳定准入身份进行选择。
    let matching_records = current
        .records
        .iter()
        .filter(|record| {
            record
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get(INTERJECTED_PROMPT_ADMISSION_ID_KEY))
                .and_then(serde_json::Value::as_str)
                == Some(admission_id)
        })
        .collect::<Vec<_>>();
    if matching_records.is_empty() {
        return Ok(false);
    }
    if matching_records.len() != 1 {
        return Err(String::from(
            "The execution retry user follow-up admission identity is duplicated in canonical history.",
        ));
    }
    let record = matching_records[0];
    let metadata = record.metadata.as_ref().ok_or_else(|| {
        String::from("The committed execution retry user follow-up record has no metadata.")
    })?;
    if record.role != TranscriptRole::User
        || record.record_kind != TranscriptRecordKind::Visible
        || !record.visible
        || record.body.trim() != visible_prompt
        || metadata
            .get("canonical_text")
            .and_then(serde_json::Value::as_str)
            != Some(visible_prompt)
        || metadata
            .get("canonical_user_text")
            .and_then(serde_json::Value::as_str)
            != Some(visible_prompt)
        || metadata.get("source").and_then(serde_json::Value::as_str) != Some("user_interrupt")
    {
        return Err(String::from(
            "The committed execution retry user follow-up record does not match its retained body.",
        ));
    }
    // previousAdmissionCount distinguishes a newly appended supplement from an already committed
    // supplement recovered after a crash in the same pre-dispatch window.
    // previousAdmissionCount 区分本次新追加的补充消息与同一分派前窗口崩溃后恢复的已提交补充消息。
    let previous_admission_count = previous
        .records
        .iter()
        .filter(|candidate| {
            candidate
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get(INTERJECTED_PROMPT_ADMISSION_ID_KEY))
                .and_then(serde_json::Value::as_str)
                == Some(admission_id)
        })
        .count();
    if previous_admission_count > 1 {
        return Err(String::from(
            "The frozen execution retry snapshot already contains duplicate user follow-up admissions.",
        ));
    }
    // messageScope is restricted to the newly appended suffix for a first commit. A crash-restored
    // snapshot may already contain the admission, so its complete canonical window is examined.
    // messageScope 在首次提交时仅检查新追加后缀。崩溃恢复快照可能已包含准入记录，因此检查其完整规范窗口。
    let message_scope = if previous_admission_count == 1 {
        current.messages.as_slice()
    } else {
        current
            .messages
            .strip_prefix(previous.messages.as_slice())
            .ok_or_else(|| {
                String::from(
                    "Canonical messages no longer preserve the frozen prefix while committing a recovery follow-up.",
                )
            })?
    };
    let matching_message_count = message_scope
        .iter()
        .filter(|message| {
            matches!(
                &message.payload,
                ConversationPayload::UserText { text, attachments }
                    if text.trim() == provider_prompt && attachments.is_empty()
            )
        })
        .count();
    if matching_message_count != 1 {
        return Err(String::from(
            "The committed execution retry user follow-up does not have one exact canonical user message.",
        ));
    }
    Ok(true)
}

/// Returns whether canonical history appended exact input that can seed the next Provider turn.
/// 返回规范历史是否追加了可启动下一次 Provider 回合的精确输入。
fn has_new_committed_provider_seed(
    previous: &ConversationSnapshot,
    current: &ConversationSnapshot,
) -> bool {
    current
        .messages
        .strip_prefix(previous.messages.as_slice())
        .is_some_and(|suffix| {
            !suffix.is_empty()
                && vulcan_core::controller::conversation_snapshot_has_seeded_provider_input(current)
        })
}

/// Builds exact unprepared recovery slots from one retained assistant tool-call batch.
/// 从一批已保留的助手工具调用构建精确的未准备恢复槽位。
///
/// # Parameters
/// 参数
/// `assistant_calls` is the canonical Provider-authored tool-call batch in original order.
/// `assistant_calls` 是按原始顺序保存的规范 Provider 工具调用批次。
///
/// # Returns
/// 返回值
/// One `NotStarted` slot per canonical call, with binding resolution deferred to the mandatory batch-prepared barrier.
/// 为每个规范调用返回一个 `NotStarted` 槽位，并把绑定解析延迟到强制批次准备屏障。
fn build_unprepared_tool_recovery_slots(
    assistant_calls: &[vulcan_core::types::ConversationToolCall],
) -> Vec<ExecutionRetryToolProgress> {
    assistant_calls
        .iter()
        .map(|assistant_call| ExecutionRetryToolProgress {
            request: vulcan_core::types::ToolInvocationRequest {
                tool_call_id: vulcan_core::controller::resolved_tool_call_id(assistant_call),
                tool_name: assistant_call.tool_name.clone(),
                arguments: assistant_call.arguments.clone(),
            },
            result: None,
            binding_fingerprint: None,
            state: ExecutionRetryToolState::NotStarted,
        })
        .collect()
}

/// Reconciles canonical progress committed after a retry checkpoint but before terminal interruption.
/// 协调重试检查点之后、终态中断之前提交的规范进度。
///
/// # Parameters
/// 参数
/// `snapshot` is the claimed or frozen sidecar whose pending action must match current Core state.
/// `snapshot` 是其待执行动作必须与当前 Core 状态一致的已领取或冻结 sidecar。
/// `conversation_state` is the latest retained canonical conversation projected by the service.
/// `conversation_state` 是服务投影的最新保留规范会话。
///
/// # Returns
/// 返回值
/// `true` when committed canonical progress selected a new safe action and no interruption error is required.
/// 当已提交规范进度选择了新的安全动作且不需要中断错误时返回 `true`。
pub(crate) fn reconcile_interrupted_execution_retry_canonical_state(
    snapshot: &mut ExecutionRetrySnapshot,
    conversation_state: &SessionConversationState,
) -> Result<bool, String> {
    // previousControllerSnapshot is the immutable boundary used to identify newly committed input.
    // previousControllerSnapshot 是用于识别新提交输入的不可变边界。
    let previous_controller_snapshot = snapshot.controller_snapshot.clone();
    refresh_execution_retry_canonical_state(snapshot, conversation_state)?;
    // committedUserFollowup proves the claimed supplement already entered canonical history. The
    // sidecar must consume it before becoming Frozen again so a later Continue cannot append it twice.
    // committedUserFollowup 证明已领取补充消息已经进入规范历史。sidecar 在重新变为 Frozen 前
    // 必须消费它，避免后续 Continue 重复追加。
    let committed_user_followup = snapshot
        .user_followup
        .as_ref()
        .map(|followup| {
            execution_retry_user_followup_was_committed(
                &previous_controller_snapshot,
                &snapshot.controller_snapshot,
                followup,
            )
        })
        .transpose()?
        .unwrap_or(false);
    if committed_user_followup {
        snapshot.user_followup = None;
    }
    // Running slots become outcome-unknown after interruption and can only return a synthetic result.
    // 运行中槽位在中断后转换为结果未知，并且只能返回合成结果。
    if let ExecutionRetryPendingAction::ExecuteToolBatch { tools } = &mut snapshot.pending_action {
        for tool in tools {
            if tool.state == ExecutionRetryToolState::Running {
                tool.state = ExecutionRetryToolState::OutcomeUnknown;
            }
        }
    }
    if committed_user_followup
        || has_new_committed_provider_seed(
            &previous_controller_snapshot,
            &snapshot.controller_snapshot,
        )
    {
        // Any newly committed Provider seed makes the old frozen request stale. The next request
        // must be rebuilt from the updated controller state.
        // 任一新提交的 Provider 播种输入都会使旧冻结请求过期。下一请求必须从更新后的控制器状态重建。
        snapshot.pending_action = ExecutionRetryPendingAction::PrepareNextProviderRequest;
        snapshot.error = None;
        return Ok(true);
    }
    if !matches!(
        snapshot.pending_action,
        ExecutionRetryPendingAction::ExecuteToolBatch { .. }
    ) && let Some(ConversationPayload::Assistant { tool_calls, .. }) = snapshot
        .controller_snapshot
        .messages
        .last()
        .map(|message| &message.payload)
        && !tool_calls.is_empty()
    {
        // An assistant tool-call tail proves the Provider completed. A crash before BatchPrepared
        // must resume local batch preparation instead of replaying the already-completed Provider.
        // 助手工具调用尾部证明 Provider 已完成。BatchPrepared 前的崩溃必须恢复本地批次准备，
        // 不能重放已经完成的 Provider。
        snapshot.pending_action = ExecutionRetryPendingAction::ExecuteToolBatch {
            tools: build_unprepared_tool_recovery_slots(tool_calls),
        };
        snapshot.error = None;
        return Ok(true);
    }
    if matches!(
        snapshot
            .controller_snapshot
            .messages
            .last()
            .map(|message| &message.payload),
        Some(ConversationPayload::Assistant { tool_calls, .. }) if tool_calls.is_empty()
    ) {
        // A retained tool-free assistant tail proves Provider completion and needs only local finalization.
        // 已保留的无工具助手尾部证明 Provider 已完成，只需执行本地终结。
        snapshot.pending_action = ExecutionRetryPendingAction::FinalizeAssistant;
        snapshot.error = None;
        return Ok(true);
    }
    Ok(false)
}

/// Refreshes the retry sidecar's controller state and digest from the retained canonical state.
/// 从保留的规范状态刷新重试 sidecar 的控制器状态与摘要。
///
/// # Parameters
/// # 参数
/// - `snapshot`: Frozen or claimed retry snapshot that retains the exact pending action.
/// - `snapshot`：保留精确待执行动作的冻结或已领取重试快照。
/// - `conversation_state`: Latest retained canonical state after terminal projection.
/// - `conversation_state`：终态投影完成后的最新保留规范状态。
///
/// # Returns
/// # 返回值
/// Returns success after both fields are refreshed, or a bridge/serialization error.
/// 在两个字段均刷新后返回成功；桥接或序列化失败时返回错误。
pub(crate) fn refresh_execution_retry_canonical_state(
    snapshot: &mut ExecutionRetrySnapshot,
    conversation_state: &SessionConversationState,
) -> Result<(), String> {
    let controller_snapshot = vulcan_agent_runtime::agent_runtime::provider_request_bridge_conversation::build_core_conversation_snapshot(
        conversation_state,
    )?;
    let canonical_bytes = serde_json::to_vec(&controller_snapshot)
        .map_err(|error| format!("Failed to serialize refrozen retry Core state: {error}"))?;
    snapshot.controller_snapshot = controller_snapshot;
    snapshot.canonical_digest = sha256_hex(&canonical_bytes);
    Ok(())
}

/// Durably publishes one exact per-tool recovery transition before or after the corresponding tool boundary.
/// 在对应工具边界之前或之后，可靠发布一次精确的逐工具恢复迁移。
///
/// # Parameters
/// 参数
/// `task_state` owns the authoritative live retry reference and mirrored continuation ledger.
/// `task_state` 持有权威实时重试引用及其镜像 Continue 账本。
/// `controller_runtime` owns immutable sidecar storage and durable main-session persistence.
/// `controller_runtime` 持有不可变 sidecar 存储及可靠主会话持久化能力。
/// `session_id` selects the exact conversation whose tool recovery boundary is being persisted.
/// `session_id` 选择正在持久化工具恢复边界的精确会话。
/// `expected_reference` is the immutable owner captured before asynchronous persistence starts.
/// `expected_reference` 是异步持久化开始前捕获的不可变所有者。
/// `boundary` is the exact prepared, started, or finished tool transition.
/// `boundary` 是精确的已准备、已启动或已完成工具迁移。
///
/// # Returns
/// 返回值
/// Success only after the exact sidecar reference is represented by a durable main-session revision.
/// 仅当精确 sidecar 引用已由可靠主会话修订表示后返回成功。
pub(crate) async fn persist_execution_retry_tool_recovery_boundary(
    task_state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    session_id: SessionId,
    expected_reference: &vulcan_workbench_model::ExecutionRetrySnapshotRef,
    boundary: &ToolExecutionRecoveryBoundary,
) -> Result<(), String> {
    let (reference, conversation_state) = {
        let guard = task_state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let runtime_state = guard
            .workspace_snapshot
            .as_ref()
            .and_then(|workspace| workspace.session_runtime_state.get(&session_id))
            .ok_or_else(|| {
                String::from("Execution retry runtime state disappeared at a tool boundary.")
            })?;
        (
            runtime_state.execution_retry_snapshot.clone(),
            runtime_state.conversation_state.clone(),
        )
    };
    let Some(reference) = reference else {
        return Err(coded_tool_recovery_error(
            TOOL_RECOVERY_BOUNDARY_MISSING_REASON,
            "Execution retry reference is missing at a mandatory tool recovery boundary.",
        ));
    };
    if &reference != expected_reference {
        return Err(coded_tool_recovery_error(
            TOOL_RECOVERY_BOUNDARY_MISSING_REASON,
            "Execution retry reference changed before tool-boundary validation started.",
        ));
    }
    if !matches!(
        reference.state,
        ExecutionRetrySnapshotState::Frozen | ExecutionRetrySnapshotState::Claimed
    ) {
        return Err(coded_tool_recovery_error(
            TOOL_RECOVERY_BOUNDARY_MISSING_REASON,
            "The retained recovery owner is no longer frozen or claimed.",
        ));
    }
    let mut snapshot = vulcan_workspace_store::workspace_store::load_execution_retry_snapshot(
        &controller_runtime.data_root,
        session_id,
        &reference,
    )?;
    match boundary {
        ToolExecutionRecoveryBoundary::BatchPrepared(requests) => {
            validate_prepared_tool_batch_shape(requests, None)?;
            refresh_execution_retry_canonical_state(&mut snapshot, &conversation_state)?;
            match &mut snapshot.pending_action {
                ExecutionRetryPendingAction::ExecuteToolBatch { tools } => {
                    validate_prepared_tool_batch_shape(requests, Some(tools.len()))?;
                    for prepared in requests {
                        let retained = tools
                            .iter_mut()
                            .find(|tool| {
                                tool.request.tool_call_id == prepared.request.tool_call_id
                            })
                            .ok_or_else(|| {
                                coded_tool_recovery_error(
                                    TOOL_RECOVERY_BOUNDARY_MISSING_REASON,
                                    format!(
                                        "Prepared tool `{}` is absent from the retained recovery batch.",
                                        prepared.request.tool_call_id
                                    ),
                                )
                            })?;
                        if retained.request != prepared.request
                            || !matches!(retained.state, ExecutionRetryToolState::NotStarted)
                        {
                            return Err(coded_tool_recovery_error(
                                TOOL_RECOVERY_BINDING_MISMATCH_REASON,
                                format!(
                                    "Prepared tool `{}` does not match its retained recovery slot.",
                                    prepared.request.tool_call_id
                                ),
                            ));
                        }
                        match retained.binding_fingerprint.as_ref() {
                            Some(binding_fingerprint)
                                if binding_fingerprint == &prepared.binding_fingerprint => {}
                            None => {
                                // The durable batch-prepared barrier freezes the first exact binding
                                // before Core is allowed to emit any tool-started boundary.
                                // 可靠批次准备屏障会在 Core 获准发出任何工具启动边界前，
                                // 冻结首个精确绑定。
                                retained.binding_fingerprint =
                                    Some(prepared.binding_fingerprint.clone());
                            }
                            Some(_) => {
                                return Err(coded_tool_recovery_error(
                                    TOOL_RECOVERY_BINDING_MISMATCH_REASON,
                                    format!(
                                        "Prepared tool `{}` changed its retained recovery binding.",
                                        prepared.request.tool_call_id
                                    ),
                                ));
                            }
                        }
                    }
                }
                _ => {
                    snapshot.pending_action = ExecutionRetryPendingAction::ExecuteToolBatch {
                        tools: requests
                            .iter()
                            .map(|prepared| ExecutionRetryToolProgress {
                                request: prepared.request.clone(),
                                result: None,
                                binding_fingerprint: Some(prepared.binding_fingerprint.clone()),
                                state: ExecutionRetryToolState::NotStarted,
                            })
                            .collect(),
                    };
                }
            }
            snapshot.error = None;
        }
        ToolExecutionRecoveryBoundary::Started {
            request,
            binding_fingerprint,
        } => {
            let ExecutionRetryPendingAction::ExecuteToolBatch { tools } =
                &mut snapshot.pending_action
            else {
                return Err(coded_tool_recovery_error(
                    TOOL_RECOVERY_BOUNDARY_MISSING_REASON,
                    "A tool started before its complete recovery batch became durable.",
                ));
            };
            let progress = tools
                .iter_mut()
                .find(|tool| tool.request.tool_call_id == request.tool_call_id)
                .ok_or_else(|| {
                    coded_tool_recovery_error(
                        TOOL_RECOVERY_BOUNDARY_MISSING_REASON,
                        format!(
                            "Started tool `{}` is absent from the retained recovery batch.",
                            request.tool_call_id
                        ),
                    )
                })?;
            if progress.request != *request
                || progress.binding_fingerprint.as_deref() != Some(binding_fingerprint.as_str())
                || !matches!(
                    progress.state,
                    ExecutionRetryToolState::NotStarted | ExecutionRetryToolState::Running
                )
            {
                return Err(coded_tool_recovery_error(
                    TOOL_RECOVERY_BINDING_MISMATCH_REASON,
                    format!(
                        "Started tool `{}` does not match a replayable retained slot.",
                        request.tool_call_id
                    ),
                ));
            }
            progress.state = ExecutionRetryToolState::Running;
            snapshot.error = Some(tool_execution_in_flight_error(&snapshot));
        }
        ToolExecutionRecoveryBoundary::Finished(result) => {
            let ExecutionRetryPendingAction::ExecuteToolBatch { tools } =
                &mut snapshot.pending_action
            else {
                return Err(coded_tool_recovery_error(
                    TOOL_RECOVERY_BOUNDARY_MISSING_REASON,
                    "A tool finished without a retained recovery batch.",
                ));
            };
            let progress = tools
                .iter_mut()
                .find(|tool| tool.request.tool_call_id == result.tool_call_id)
                .ok_or_else(|| {
                    coded_tool_recovery_error(
                        TOOL_RECOVERY_BOUNDARY_MISSING_REASON,
                        format!(
                            "Finished tool `{}` is absent from the retained recovery batch.",
                            result.tool_call_id
                        ),
                    )
                })?;
            if progress.request.tool_name != result.tool_name {
                return Err(coded_tool_recovery_error(
                    TOOL_RECOVERY_RESULT_MISMATCH_REASON,
                    format!(
                        "Finished tool `{}` changed its retained tool name.",
                        result.tool_call_id
                    ),
                ));
            }
            match (&progress.state, &progress.result) {
                (ExecutionRetryToolState::Running, None) => {
                    progress.state = if result.is_error {
                        ExecutionRetryToolState::Failed
                    } else {
                        ExecutionRetryToolState::Completed
                    };
                    progress.result = Some(result.clone());
                }
                (
                    ExecutionRetryToolState::Completed | ExecutionRetryToolState::Failed,
                    Some(old),
                ) if old == result => {}
                _ => {
                    return Err(coded_tool_recovery_error(
                        TOOL_RECOVERY_RESULT_MISMATCH_REASON,
                        format!(
                            "Finished tool `{}` does not follow its retained running boundary.",
                            result.tool_call_id
                        ),
                    ));
                }
            }
            if tools.iter().all(|tool| {
                matches!(
                    tool.state,
                    ExecutionRetryToolState::Completed | ExecutionRetryToolState::Failed
                )
            }) {
                snapshot.error = None;
            }
        }
    }
    snapshot.updated_at = current_unix_millis_text();
    let updated_reference =
        vulcan_workspace_store::workspace_store::persist_execution_retry_snapshot(
            &controller_runtime.data_root,
            session_id,
            &snapshot,
        )?;
    if !updated_reference.replay_safety_proven() {
        return Err(format!(
            "The persisted tool recovery boundary is not continuable: {}",
            updated_reference
                .blocked_reason
                .as_deref()
                .unwrap_or("retry_safety_proof_missing")
        ));
    }
    {
        let mut guard = task_state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let workspace = guard
            .workspace_snapshot
            .as_mut()
            .ok_or_else(|| String::from("Execution retry workspace disappeared at tool start."))?;
        let runtime_state = workspace
            .session_runtime_state
            .get_mut(&session_id)
            .ok_or_else(|| {
                String::from("Execution retry runtime state disappeared at tool start.")
            })?;
        // currentReference is compared exactly so an older tool event cannot overwrite a newer recovery boundary.
        // currentReference 通过精确比较，避免较旧的工具事件覆盖较新的恢复边界。
        let current_reference = runtime_state.execution_retry_snapshot.clone();
        match current_reference.as_ref() {
            Some(current) if current == &reference => {
                runtime_state.execution_retry_snapshot = Some(updated_reference);
            }
            _ => {
                return Err(String::from(
                    "Execution retry reference changed before the exact tool recovery boundary could be published.",
                ));
            }
        }
        runtime_state.provider_call_ledger.continuation.eligible = false;
        runtime_state.updated_at = runtime_state
            .execution_retry_snapshot
            .as_ref()
            .map(|current| current.updated_at.clone())
            .unwrap_or_else(current_unix_millis_text);
    }
    // authoritativeSnapshot is read only after the shared write gate is held, preserving newer same-session recovery mutations.
    // authoritativeSnapshot 仅在持有共享写入闸门后读取，从而保留较新的同会话恢复变更。
    let (_persist_guard, authoritative_snapshot) =
        lock_latest_retained_workspace_session_persistence_snapshot(task_state, session_id).await?;
    // runtimeStateUpdate is the newest retained revision containing the exact per-tool boundary published above.
    // runtimeStateUpdate 是包含上方精确逐工具边界的最新保留修订。
    let runtime_state_update = authoritative_snapshot
        .session_runtime_state
        .get(&session_id)
        .ok_or_else(|| {
            String::from("Execution retry runtime state disappeared before durable tool recovery.")
        })?;
    vulcan_workspace_store::workspace_store::persist_single_session_runtime_state_update(
        controller_runtime.clone(),
        session_id,
        runtime_state_update,
    )
    .await
    .map_err(|error| format!("Failed to durably publish the exact tool recovery boundary: {error}"))
}

/// Builds structured interruption evidence for a tool whose body may already have external effects.
/// 为主体可能已经产生外部影响的工具构建结构化中断证据。
fn tool_execution_in_flight_error(
    snapshot: &ExecutionRetrySnapshot,
) -> vulcan_ai_contract::AiExecutionError {
    vulcan_ai_contract::AiExecutionError {
        category: vulcan_ai_contract::AiErrorCategory::Unclassified,
        subcategory: Some(String::from("tool_outcome_requires_reconciliation")),
        stage: String::from("tool_execution"),
        http_status: None,
        transport: None,
        websocket_close_code: None,
        upstream_code: None,
        upstream_type: None,
        upstream_parameter: None,
        upstream_message: None,
        request_id: None,
        provider_id: snapshot
            .provider_route
            .as_ref()
            .map(|route| route.provider_id.clone()),
        entry_id: None,
        model_id: snapshot
            .provider_route
            .as_ref()
            .map(|route| route.model_id.clone()),
        message: String::from(
            "A tool invocation may have started and must be reconciled without automatic replay.",
        ),
        dispatch_outcome: vulcan_ai_contract::DispatchOutcome::Dispatched,
        semantic_output_observed: true,
        side_effect_observed: true,
        terminal: false,
        retryable: true,
        retry_after: None,
    }
}

/// Marks a provider-boundary retry as unsafe when command commit and dispatch outcome diverge.
/// 当 Provider 边界命令已提交但分派结果分裂时，将重试边界标记为不安全。
///
/// # Parameters
/// 参数
/// `task_state` owns the live retry reference that must receive the blocked projection.
/// `task_state` 持有必须接收阻塞投影的实时重试引用。
/// `controller_runtime` owns the authoritative retry sidecar storage.
/// `controller_runtime` 持有权威重试 sidecar 存储。
/// `session_id` selects the exact conversation whose provider boundary is uncertain.
/// `session_id` 选择 Provider 边界结果不确定的精确会话。
/// `message` retains the controlled diagnostic for audit and support logs.
/// `message` 保留用于审计和支持日志的受控诊断。
///
/// # Returns
/// 返回值
/// Success after the sidecar and in-memory reference both expose the explicit blocked reason.
/// sidecar 与内存引用均暴露明确阻塞原因后返回成功。
pub(crate) fn mark_execution_retry_snapshot_provider_dispatch_unknown(
    task_state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    session_id: SessionId,
    message: &str,
) -> Result<(), String> {
    let reference = {
        let guard = task_state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        guard
            .workspace_snapshot
            .as_ref()
            .and_then(|workspace| workspace.session_runtime_state.get(&session_id))
            .and_then(|runtime_state| runtime_state.execution_retry_snapshot.clone())
    };
    let Some(reference) = reference else {
        return Ok(());
    };
    if !matches!(
        reference.state,
        ExecutionRetrySnapshotState::Frozen | ExecutionRetrySnapshotState::Claimed
    ) {
        return Ok(());
    }
    let mut snapshot = vulcan_workspace_store::workspace_store::load_execution_retry_snapshot(
        &controller_runtime.data_root,
        session_id,
        &reference,
    )?;
    snapshot.state = ExecutionRetrySnapshotState::Frozen;
    snapshot.claimed_attempt_id = None;
    snapshot.error = Some(vulcan_ai_contract::AiExecutionError {
        category: vulcan_ai_contract::AiErrorCategory::Unclassified,
        subcategory: Some(String::from("provider_dispatch_outcome_unknown")),
        stage: String::from("provider_boundary"),
        http_status: None,
        transport: None,
        websocket_close_code: None,
        upstream_code: None,
        upstream_type: None,
        upstream_parameter: None,
        upstream_message: None,
        request_id: None,
        provider_id: snapshot
            .provider_route
            .as_ref()
            .map(|route| route.provider_id.clone()),
        entry_id: None,
        model_id: snapshot
            .provider_route
            .as_ref()
            .map(|route| route.model_id.clone()),
        message: message.to_string(),
        dispatch_outcome: vulcan_ai_contract::DispatchOutcome::Unknown,
        semantic_output_observed: false,
        side_effect_observed: false,
        terminal: true,
        retryable: true,
        retry_after: None,
    });
    snapshot.updated_at = current_unix_millis_text();
    let blocked_reference =
        vulcan_workspace_store::workspace_store::persist_execution_retry_snapshot(
            &controller_runtime.data_root,
            session_id,
            &snapshot,
        )?;
    let mut guard = task_state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let runtime_state = guard
        .workspace_snapshot
        .as_mut()
        .and_then(|workspace| workspace.session_runtime_state.get_mut(&session_id))
        .ok_or_else(|| {
            String::from("Execution retry runtime state disappeared at provider-boundary failure.")
        })?;
    runtime_state.execution_retry_snapshot = Some(blocked_reference);
    runtime_state.updated_at = snapshot.updated_at;
    Ok(())
}

/// Invalidates the retained retry reference and its ledger eligibility when an authoritative terminal transition fails.
/// 当权威终态迁移失败时，使保留的重试引用及其账本资格一并失效。
///
/// # Parameters
/// 参数
/// `task_state` owns the retained runtime state whose exact retry reference must be closed.
/// `task_state` 持有必须关闭精确重试引用的保留运行态。
/// `session_id` identifies the owning session whose lightweight reference is invalidated.
/// `session_id` 标识轻量引用需要失效的所属会话。
/// `blocked_reason` is the stable machine reason projected through WebUI i18n.
/// `blocked_reason` 是通过 WebUI i18n 投影的稳定机器原因。
/// `message` is the controlled non-secret diagnostic retained with the reference.
/// `message` 是随引用保留的受控非敏感诊断。
///
/// # Returns
/// 返回值
/// Success after the exact reference and ledger cache are closed, or when no reference remains.
/// 精确引用与账本缓存关闭后返回成功；引用已经不存在时也返回成功。
pub(crate) fn invalidate_execution_retry_reference(
    task_state: &SharedServiceBridgeState,
    session_id: SessionId,
    blocked_reason: &str,
    message: &str,
) -> Result<(), String> {
    let mut guard = task_state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let runtime_state = guard
        .workspace_snapshot
        .as_mut()
        .and_then(|workspace| workspace.session_runtime_state.get_mut(&session_id))
        .ok_or_else(|| {
            String::from("Execution retry runtime state disappeared during invalidation.")
        })?;
    let _ = invalidate_execution_retry_runtime_state(
        runtime_state,
        blocked_reason,
        message,
        current_unix_millis_text(),
    );
    Ok(())
}

/// Invalidates a retry reference only while the exact expected immutable reference still owns the session.
/// 仅在精确预期的不可变引用仍持有会话时使重试引用失效。
///
/// # Parameters
/// 参数
/// `task_state` owns the retained runtime state and `session_id` selects the exact conversation.
/// `task_state` 持有保留运行态，`session_id` 选择精确会话。
/// `expected_reference` is the full immutable reference that must still match before mutation.
/// `expected_reference` 是变更前必须仍然完全匹配的不可变引用。
/// `blocked_reason` and `message` retain the stable machine reason and controlled diagnostic.
/// `blocked_reason` 与 `message` 保留稳定机器原因和受控诊断信息。
///
/// # Returns
/// 返回值
/// `true` when the expected owner was invalidated, `false` after a concurrent replacement, or an explicit state error.
/// 预期所有者被失效时返回 `true`，并发替换后返回 `false`，状态异常时返回明确错误。
pub(crate) fn invalidate_execution_retry_reference_if_matches(
    task_state: &SharedServiceBridgeState,
    session_id: SessionId,
    expected_reference: &vulcan_workbench_model::ExecutionRetrySnapshotRef,
    blocked_reason: &str,
    message: &str,
) -> Result<bool, String> {
    let mut guard = task_state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let runtime_state = guard
        .workspace_snapshot
        .as_mut()
        .and_then(|workspace| workspace.session_runtime_state.get_mut(&session_id))
        .ok_or_else(|| {
            String::from("Execution retry runtime state disappeared during exact invalidation.")
        })?;
    if runtime_state.execution_retry_snapshot.as_ref() != Some(expected_reference) {
        return Ok(false);
    }
    Ok(invalidate_execution_retry_runtime_state(
        runtime_state,
        blocked_reason,
        message,
        current_unix_millis_text(),
    ))
}

/// Applies one fail-closed retry invalidation to an exact detached session runtime state.
/// 对精确的分离会话运行态应用一次失败关闭重试失效。
///
/// # Parameters
/// 参数
/// `runtime_state` owns the lightweight reference and mirrored continuation eligibility.
/// `runtime_state` 持有轻量引用与镜像的 Continue 资格。
/// `blocked_reason` is the stable fallback reason used when no earlier concrete reason exists.
/// `blocked_reason` 是不存在更早具体原因时使用的稳定回退原因。
/// `message` is the controlled support diagnostic retained with the invalidated reference.
/// `message` 是随失效引用保留的受控支持诊断。
/// `updated_at` is the authoritative Unix-millisecond timestamp text for the transition.
/// `updated_at` 是当前迁移使用的权威 Unix 毫秒时间戳文本。
///
/// # Returns
/// 返回值
/// `true` when a retained reference was invalidated; `false` when no reference existed, while mirrored ledger eligibility is still closed.
/// 保留引用被失效时返回 `true`；引用不存在时返回 `false`，但镜像账本资格仍会关闭。
pub(crate) fn invalidate_execution_retry_runtime_state(
    runtime_state: &mut SessionRuntimeStateEntry,
    blocked_reason: &str,
    message: &str,
    updated_at: String,
) -> bool {
    // Ledger eligibility closes even when the lightweight reference is already absent.
    // 即使轻量引用已经不存在，账本资格也必须关闭。
    runtime_state.provider_call_ledger.continuation.eligible = false;
    runtime_state.updated_at = updated_at.clone();
    let Some(reference) = runtime_state.execution_retry_snapshot.as_mut() else {
        return false;
    };
    // resolvedBlockedReason preserves an earlier concrete safety fact instead of replacing it with a later storage failure.
    // resolvedBlockedReason 保留更早的具体安全事实，避免被后续存储失败覆盖。
    let resolved_blocked_reason = reference
        .blocked_reason
        .as_deref()
        .filter(|reason| !reason.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| blocked_reason.to_string());
    reference.invalidate_for_safety_reason(
        resolved_blocked_reason,
        message.to_string(),
        updated_at.clone(),
    );
    runtime_state.updated_at = updated_at;
    true
}

/// Computes a lowercase SHA-256 digest for exact serialized checkpoint bytes.
/// 为精确序列化的 checkpoint 字节计算小写 SHA-256 摘要。
fn sha256_hex(encoded: &[u8]) -> String {
    use sha2::{Digest, Sha256};

    let digest = Sha256::digest(encoded);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    //! Tests for retry-boundary classification after canonical tool progress.
    //! 规范工具进度后的重试边界分类测试。

    use super::*;
    use vulcan_ai_contract::{AiErrorCategory, AiExecutionError, DispatchOutcome};
    use vulcan_core::types::{
        ConversationMessage, ConversationPayload, ConversationRecord, ToolInvocationRequest,
    };

    /// Verifies a durable prepared boundary rejects duplicate identities and retained-batch subsets.
    /// 验证可靠的已准备边界会拒绝重复身份与保留批次子集。
    #[test]
    fn prepared_tool_batch_shape_requires_exact_duplicate_free_cardinality() {
        // request is the exact Provider invocation reused to construct malformed batch shapes.
        // request 是用于构造错误批次形态的精确 Provider 调用。
        let request = ToolInvocationRequest {
            tool_call_id: String::from("tool-call-a"),
            tool_name: String::from("read_file"),
            arguments: serde_json::json!({"path": "a.txt"}),
        };
        // recoveryRequest freezes the request and its resolved binding fingerprint.
        // recoveryRequest 冻结请求及其已解析绑定指纹。
        let recovery_request =
            vulcan_agent_runtime::agent_runtime::events::ToolExecutionRecoveryRequest {
                request,
                binding_fingerprint: "a".repeat(64),
            };
        // duplicateError proves duplicate tool-call identities fail before slot materialization.
        // duplicateError 证明重复工具调用身份会在槽位物化前失败。
        let duplicate_error = validate_prepared_tool_batch_shape(
            &[recovery_request.clone(), recovery_request.clone()],
            None,
        )
        .expect_err("duplicate tool-call identities must be rejected");
        assert!(duplicate_error.starts_with(TOOL_RECOVERY_BOUNDARY_MISSING_REASON));

        // incompleteError proves a subset cannot validate against a larger retained batch.
        // incompleteError 证明子集无法通过更大保留批次的校验。
        let incomplete_error = validate_prepared_tool_batch_shape(&[recovery_request], Some(2))
            .expect_err("prepared subset must be rejected");
        assert!(incomplete_error.starts_with(TOOL_RECOVERY_BOUNDARY_MISSING_REASON));
    }

    /// Verifies each refreeze failure category exposes its exact stable localized reason.
    /// 验证每种重冻失败分类都会暴露精确且稳定的本地化原因。
    #[test]
    fn refreeze_failure_exposes_exact_blocked_reason_category() {
        // bodyFailure represents an immutable sidecar that failed load or integrity validation.
        // bodyFailure 表示加载或完整性校验失败的不可变 sidecar。
        let body_failure = ExecutionRetryRefreezeFailure::BodyUnreadable(String::from("body"));
        // transitionFailure represents a canonical refresh or publication failure after body load.
        // transitionFailure 表示正文加载后的规范刷新或发布失败。
        let transition_failure =
            ExecutionRetryRefreezeFailure::TransitionFailed(String::from("transition"));

        assert_eq!(
            body_failure.blocked_reason(),
            crate::conversation_submission::EXECUTION_RETRY_SNAPSHOT_BODY_UNREADABLE_REASON
        );
        assert_eq!(
            transition_failure.blocked_reason(),
            "retry_snapshot_transition_failed"
        );
        assert_ne!(
            body_failure.error_summary(),
            transition_failure.error_summary()
        );
    }

    /// Builds one lightweight retry reference for terminal-invalidation regression tests.
    /// 为终态失效回归测试构建一条轻量重试引用。
    ///
    /// # Returns
    /// 返回值
    /// A frozen, explicitly replay-safe reference with no prior blocked reason.
    /// 返回冻结、明确可重放且没有既有阻断原因的引用。
    fn retry_reference_for_terminal_invalidation()
    -> vulcan_workbench_model::ExecutionRetrySnapshotRef {
        vulcan_workbench_model::ExecutionRetrySnapshotRef {
            schema_version: EXECUTION_RETRY_SNAPSHOT_SCHEMA_VERSION,
            snapshot_id: String::from("00000000-0000-0000-0000-000000000001"),
            generation: 1,
            state: ExecutionRetrySnapshotState::Frozen,
            content_sha256: "0".repeat(64),
            content_bytes: 1,
            logical_execution_id: String::from("execution-1"),
            pending_action_label: String::from("resume_preparation"),
            provider_route: None,
            preparation_route_selection: Some(
                vulcan_workbench_model::ExecutionRetryPreparationRouteSelection {
                    active_provider_id: None,
                    active_connection_id: None,
                    active_model_id: None,
                },
            ),
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

    /// Verifies successful consumption detaches the pointer without touching its immutable body.
    /// 验证成功消费只分离指针，不触碰其不可变正文。
    #[test]
    fn successful_consumption_returns_snapshot_id_for_post_persistence_cleanup() {
        // sessionId identifies the exact retained regular conversation under test.
        // sessionId 标识被测的精确保留常规会话。
        let session_id = SessionId::new_v4();
        // reference is the immutable body pointer that must remain available until main-state persistence succeeds.
        // reference 是在主状态持久化成功前必须保持可用的不可变正文指针。
        let reference = retry_reference_for_terminal_invalidation();
        // serviceState supplies the minimum retained workspace shell required by the production clear helper.
        // serviceState 提供生产清除辅助函数所需的最小保留工作区外壳。
        let mut service_state =
            crate::tests::test_support::test_service_state_with_parent_session(session_id);
        service_state
            .workspace_snapshot
            .as_mut()
            .expect("test workspace should exist")
            .session_runtime_state
            .insert(
                session_id,
                SessionRuntimeStateEntry {
                    execution_retry_snapshot: Some(reference.clone()),
                    ..SessionRuntimeStateEntry::default()
                },
            );
        // taskState exposes the same shared mutation boundary used by terminal orchestration.
        // taskState 暴露终态编排使用的同一共享变更边界。
        let task_state: SharedServiceBridgeState =
            std::sync::Arc::new(std::sync::Mutex::new(service_state));

        let cleared_snapshot_id =
            clear_execution_retry_snapshot_after_success(&task_state, session_id)
                .expect("successful consumption should clear the live reference");

        assert_eq!(
            cleared_snapshot_id.as_deref(),
            Some(reference.snapshot_id.as_str())
        );
        assert!(
            task_state
                .lock()
                .expect("test service state should remain lockable")
                .workspace_snapshot
                .as_ref()
                .expect("test workspace should remain present")
                .session_runtime_state[&session_id]
                .execution_retry_snapshot
                .is_none()
        );
    }

    /// Verifies terminal-transition failure closes the complete safety summary and ledger cache.
    /// 验证终态迁移失败会关闭完整安全摘要与账本缓存。
    #[test]
    fn terminal_transition_failure_invalidates_retry_safety_summary() {
        // runtimeState begins with a safe reference and a stale eligible ledger projection.
        // runtimeState 初始包含安全引用与过期的可领取账本投影。
        let mut runtime_state = SessionRuntimeStateEntry::default();
        runtime_state.execution_retry_snapshot = Some(retry_reference_for_terminal_invalidation());
        runtime_state.provider_call_ledger.continuation.eligible = true;

        assert!(invalidate_execution_retry_runtime_state(
            &mut runtime_state,
            "retry_snapshot_transition_failed",
            "The retry sidecar terminal transition failed.",
            String::from("2"),
        ));

        // reference is retained only as a fail-closed diagnostic token.
        // reference 仅作为失败关闭诊断令牌保留。
        let reference = runtime_state
            .execution_retry_snapshot
            .as_ref()
            .expect("invalidated retry reference should remain visible");
        assert_eq!(reference.state, ExecutionRetrySnapshotState::Invalidated);
        assert!(!reference.retryable);
        assert!(reference.requires_manual_confirmation);
        assert_eq!(
            reference.blocked_reason.as_deref(),
            Some("retry_snapshot_transition_failed")
        );
        assert!(reference.claimed_attempt_id.is_none());
        assert_eq!(
            reference.error_summary.as_deref(),
            Some("The retry sidecar terminal transition failed.")
        );
        assert_eq!(reference.updated_at, "2");
        assert!(!runtime_state.provider_call_ledger.continuation.eligible);
        assert_eq!(runtime_state.updated_at, "2");
    }

    /// Verifies missing retry references still close mirrored continuation eligibility.
    /// 验证重试引用缺失时仍会关闭镜像 Continue 资格。
    #[test]
    fn terminal_transition_failure_without_reference_still_closes_ledger_eligibility() {
        // runtime_state models a damaged state whose lightweight pointer disappeared while eligibility stayed stale.
        // runtime_state 模拟轻量指针已消失但资格仍过期保留的损坏状态。
        let mut runtime_state = SessionRuntimeStateEntry::default();
        runtime_state.provider_call_ledger.continuation.eligible = true;

        assert!(!invalidate_execution_retry_runtime_state(
            &mut runtime_state,
            "retry_snapshot_transition_failed",
            "The retry reference disappeared before terminal settlement.",
            String::from("3"),
        ));

        assert!(!runtime_state.provider_call_ledger.continuation.eligible);
        assert_eq!(runtime_state.updated_at, "3");
    }

    /// Verifies a later terminal-storage failure does not erase an earlier concrete safety reason.
    /// 验证后续终态存储失败不会抹除更早的具体安全原因。
    #[test]
    fn terminal_transition_failure_preserves_prior_blocked_reason() {
        // runtimeState retains the tool-side-effect fact that must remain authoritative.
        // runtimeState 保留必须继续保持权威的工具副作用事实。
        let mut runtime_state = SessionRuntimeStateEntry::default();
        let mut reference = retry_reference_for_terminal_invalidation();
        reference.retryable = false;
        reference.requires_manual_confirmation = true;
        reference.blocked_reason = Some(String::from("tool_side_effect_in_flight"));
        runtime_state.execution_retry_snapshot = Some(reference);

        assert!(invalidate_execution_retry_runtime_state(
            &mut runtime_state,
            "retry_snapshot_transition_failed",
            "The retry sidecar terminal transition failed.",
            String::from("2"),
        ));
        assert_eq!(
            runtime_state
                .execution_retry_snapshot
                .as_ref()
                .and_then(|reference| reference.blocked_reason.as_deref()),
            Some("tool_side_effect_in_flight")
        );
    }

    /// Builds one assistant message that has no committed tool result yet.
    /// 构建一条尚未提交工具结果的助手消息。
    fn assistant_tool_call_snapshot() -> ConversationSnapshot {
        ConversationSnapshot {
            messages: vec![ConversationMessage {
                id: String::from("assistant-1"),
                payload: ConversationPayload::Assistant {
                    text: String::new(),
                    reasoning: String::new(),
                    provider_message_id: None,
                    provider_message_state: None,
                    usage: None,
                    tool_calls: Vec::new(),
                },
            }],
            ..ConversationSnapshot::default()
        }
    }

    /// Verifies a newly appended ToolResult advances the retry boundary safely.
    /// 验证新追加的 ToolResult 会安全推进重试边界。
    #[test]
    fn committed_tool_result_selects_next_provider_boundary() {
        let previous = assistant_tool_call_snapshot();
        let mut current = previous.clone();
        current.messages.push(ConversationMessage {
            id: String::from("tool-result-1"),
            payload: ConversationPayload::ToolResult {
                tool_call_id: Some(String::from("call-1")),
                is_error: false,
                text: String::from("done"),
            },
        });
        // followup models the supplement appended after every restored tool result and before the
        // next Provider request is prepared.
        // followup 模拟在全部恢复工具结果之后、下一次 Provider 请求准备之前追加的补充消息。
        current.messages.push(ConversationMessage {
            id: String::from("user-followup-1"),
            payload: ConversationPayload::UserText {
                text: String::from("Please continue with this additional constraint."),
                attachments: Vec::new(),
            },
        });

        assert!(has_new_committed_provider_seed(&previous, &current));
    }

    /// Verifies one committed recovery supplement is consumed across both first refreeze and crash recovery.
    /// 验证一条已提交恢复补充消息在首次重新冻结与崩溃恢复中都会被识别为已消费。
    #[test]
    fn committed_retry_user_followup_is_not_reappended_after_refreeze() {
        let previous = assistant_tool_call_snapshot();
        let mut current = previous.clone();
        let followup = ExecutionRetryUserFollowup {
            visible_prompt: String::from("补充一个限制"),
            provider_prompt: String::from(
                "补充一个限制\n\nThe user supplied this after interrupting the prior attempt.",
            ),
            client_submission_id: String::from("submission-followup-1"),
        };
        current.messages.push(ConversationMessage {
            id: String::from("user-followup-message-1"),
            payload: ConversationPayload::UserText {
                text: followup.provider_prompt.clone(),
                attachments: Vec::new(),
            },
        });
        current.records.push(ConversationRecord {
            id: String::from("user-followup-record-1"),
            role: TranscriptRole::User,
            record_kind: TranscriptRecordKind::Visible,
            visible: true,
            title: String::from("用户指令"),
            body: followup.visible_prompt.clone(),
            metadata: Some(serde_json::json!({
                "canonical_text": followup.visible_prompt.clone(),
                "canonical_user_text": "补充一个限制",
                "source": "user_interrupt",
                "prompt_admission_id": followup.client_submission_id.clone(),
            })),
            created_at: String::from("1"),
        });

        assert!(
            execution_retry_user_followup_was_committed(&previous, &current, &followup)
                .expect("the newly appended supplement should be recognized")
        );
        assert!(
            execution_retry_user_followup_was_committed(&current, &current, &followup)
                .expect("the crash-restored supplement should remain recognized")
        );
    }

    /// Verifies an unchanged or non-tool tail never clears an unsafe boundary.
    /// 验证未变化或非工具尾部绝不会清除不安全边界。
    #[test]
    fn unchanged_boundary_is_not_marked_as_next_provider_ready() {
        let previous = assistant_tool_call_snapshot();
        assert!(!has_new_committed_provider_seed(&previous, &previous));
    }

    /// Builds one structured interruption error for stop-classification tests.
    /// 为停止分类测试构建一条结构化中断错误。
    fn interruption_error(
        dispatch_outcome: DispatchOutcome,
        semantic_output_observed: bool,
        side_effect_observed: bool,
        retryable: bool,
    ) -> AiExecutionError {
        AiExecutionError {
            category: AiErrorCategory::ProviderUnavailable,
            subcategory: None,
            stage: String::from("provider_request"),
            http_status: None,
            transport: None,
            websocket_close_code: None,
            upstream_code: None,
            upstream_type: None,
            upstream_parameter: None,
            upstream_message: None,
            request_id: None,
            provider_id: None,
            entry_id: None,
            model_id: None,
            message: String::from("test"),
            dispatch_outcome,
            semantic_output_observed,
            side_effect_observed,
            terminal: true,
            retryable,
            retry_after: None,
        }
    }

    /// Verifies safe stop actions are marked not-dispatched while provider dispatch remains unknown.
    /// 验证安全停止动作标记为未分派，而 Provider 分派动作保留未知事实。
    #[test]
    fn stop_interruption_classifies_dispatch_boundary() {
        let next_provider_action = ExecutionRetryPendingAction::PrepareNextProviderRequest;
        let dispatch_action = ExecutionRetryPendingAction::DispatchProviderRequest {
            request: Box::new(
                serde_json::from_value(serde_json::json!({
                    "operation": "conversation_respond",
                    "model_id": "model-a"
                }))
                .expect("minimal provider request should decode"),
            ),
        };
        assert!(should_replace_stop_interruption_error(
            None,
            &next_provider_action
        ));
        assert_eq!(
            stop_interruption_dispatch_outcome(&next_provider_action),
            DispatchOutcome::NotDispatched
        );
        assert!(should_replace_stop_interruption_error(
            None,
            &dispatch_action
        ));
        assert_eq!(
            stop_interruption_dispatch_outcome(&dispatch_action),
            DispatchOutcome::Unknown
        );
    }

    /// Verifies stop preserves semantic, side-effect, and non-retryable facts.
    /// 验证停止会保留语义输出、副作用和不可重试事实。
    #[test]
    fn stop_interruption_preserves_unsafe_facts() {
        let next_provider_action = ExecutionRetryPendingAction::PrepareNextProviderRequest;
        assert!(!should_replace_stop_interruption_error(
            Some(&interruption_error(
                DispatchOutcome::Unknown,
                true,
                false,
                true,
            )),
            &next_provider_action
        ));
        assert!(!should_replace_stop_interruption_error(
            Some(&interruption_error(
                DispatchOutcome::Unknown,
                false,
                true,
                true,
            )),
            &next_provider_action
        ));
        assert!(!should_replace_stop_interruption_error(
            Some(&interruption_error(
                DispatchOutcome::NotDispatched,
                false,
                false,
                false,
            )),
            &next_provider_action
        ));
        assert!(should_replace_stop_interruption_error(
            Some(&interruption_error(
                DispatchOutcome::Unknown,
                false,
                false,
                true,
            )),
            &next_provider_action
        ));

        let mut explicit_unknown = interruption_error(DispatchOutcome::Unknown, false, false, true);
        explicit_unknown.subcategory = Some(String::from("provider_dispatch_outcome_unknown"));
        assert!(!should_replace_stop_interruption_error(
            Some(&explicit_unknown),
            &next_provider_action
        ));
    }
}
