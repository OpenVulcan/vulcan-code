//! Root conversation checkpoint event handling.
//! 根会话检查点事件处理。

use vulcan_agent_runtime::agent_runtime::ExecutionPreparation;
use vulcan_agent_runtime::conversation_protocol::SessionConversationState;
use vulcan_ai_contract::AiExecutionRequest;
use vulcan_core::types::ProviderUsageSnapshot;
use vulcan_core::types::{ControllerConfig, ConversationSnapshot};
use vulcan_workbench_model::{
    EXECUTION_RETRY_SNAPSHOT_SCHEMA_VERSION, ExecutionRetryPendingAction, ExecutionRetrySnapshot,
    ExecutionRetrySnapshotState,
};

use super::*;

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
pub(super) fn persist_prepared_execution_retry_checkpoint(
    task_state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    session_id: SessionId,
    active_stream_task_id: Uuid,
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
    let (snapshot_id, generation, logical_execution_id, provider_route) = {
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
        let logical_execution_id = reusable_reference
            .map(|reference| reference.logical_execution_id.clone())
            .unwrap_or_else(|| physical_attempt_id.clone());
        let provider_route = runtime_state.provider_route_lease.clone().ok_or_else(|| {
            String::from("Execution retry checkpoint has no exact frozen provider route.")
        })?;
        (
            snapshot_id,
            generation,
            logical_execution_id,
            provider_route,
        )
    };
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
        logical_turn_id: preparation.conversation_hook_turn_id.clone(),
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
    // Terminal projection may append canonical records after the provider request checkpoint was
    // frozen. Refresh both the controller snapshot and its digest atomically so Continue validates
    // the current canonical state while retaining the exact frozen provider request.
    // Provider 请求检查点冻结后，终态投影可能继续追加规范记录。这里原子刷新控制器快照及其
    // 摘要，使 Continue 校验当前规范状态，同时保留精确冻结的 Provider 请求。
    let previous_controller_snapshot = snapshot.controller_snapshot.clone();
    refresh_execution_retry_canonical_state(&mut snapshot, conversation_state)
        .map_err(ExecutionRetryRefreezeFailure::TransitionFailed)?;
    snapshot.state = ExecutionRetrySnapshotState::Frozen;
    snapshot.claimed_attempt_id = None;
    snapshot.updated_at = current_unix_millis_text();
    if has_new_committed_tool_result(&previous_controller_snapshot, &snapshot.controller_snapshot) {
        // A committed ToolResult proves the external tool outcome, so the next safe action is a
        // provider request built from the updated controller state rather than replaying the old call.
        // 已提交 ToolResult 证明外部工具结果，因此下一安全动作必须基于更新后的控制器状态构建
        // Provider 请求，不能重放旧的 Provider 调用。
        snapshot.pending_action = ExecutionRetryPendingAction::PrepareNextProviderRequest;
        snapshot.error = None;
    } else if should_replace_stop_interruption_error(
        snapshot.error.as_ref(),
        &snapshot.pending_action,
    ) {
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
        // canonicalTailMatches rejects stale cached labels and every assistant, missing, or synthetic tail.
        // canonicalTailMatches 拒绝过期缓存标签以及所有助手、缺失或合成尾部。
        let canonical_tail_matches = crate::canonical_conversation_tail_kind(runtime_state)
            .is_some_and(|tail_kind| {
                crate::is_prompt_free_continuation_tail_kind(tail_kind)
                    && continuation.canonical_tail_kind.as_deref() == Some(tail_kind)
            });
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
            | ExecutionRetryPendingAction::PrepareNextProviderRequest
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

/// Returns whether the canonical state advanced from the frozen request to a completed tool result.
/// 判断规范状态是否从冻结请求推进到了已完成工具结果。
fn has_new_committed_tool_result(
    previous: &ConversationSnapshot,
    current: &ConversationSnapshot,
) -> bool {
    let previous_message_count = previous.messages.len();
    current.messages.len() > previous_message_count
        && matches!(
            current.messages.last().map(|message| &message.payload),
            Some(vulcan_core::types::ConversationPayload::ToolResult { .. })
        )
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

/// Marks and durably publishes the current retry snapshot as non-replayable before a tool side effect may begin.
/// 在工具副作用允许开始前，将当前重试快照标记为不可重放并可靠发布。
///
/// # Parameters
/// 参数
/// `task_state` owns the authoritative live retry reference and mirrored continuation ledger.
/// `task_state` 持有权威实时重试引用及其镜像 Continue 账本。
/// `controller_runtime` owns immutable sidecar storage and durable main-session persistence.
/// `controller_runtime` 持有不可变 sidecar 存储及可靠主会话持久化能力。
/// `session_id` selects the exact conversation whose tool body is waiting at the safety gate.
/// `session_id` 选择工具主体正在安全门禁等待的精确会话。
///
/// # Returns
/// 返回值
/// Success only after the unsafe sidecar reference is represented by a durable main-session revision.
/// 仅当不安全 sidecar 引用已由可靠主会话修订表示后返回成功。
pub(crate) async fn mark_execution_retry_snapshot_tool_execution_unsafe(
    task_state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    session_id: SessionId,
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
    snapshot.error = Some(vulcan_ai_contract::AiExecutionError {
        category: vulcan_ai_contract::AiErrorCategory::Unclassified,
        subcategory: Some(String::from("tool_side_effect_in_flight")),
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
            "A tool started and its external side effects cannot be proven safe to replay.",
        ),
        dispatch_outcome: vulcan_ai_contract::DispatchOutcome::Dispatched,
        semantic_output_observed: true,
        side_effect_observed: true,
        terminal: false,
        retryable: false,
        retry_after: None,
    });
    snapshot.updated_at = current_unix_millis_text();
    let unsafe_reference =
        vulcan_workspace_store::workspace_store::persist_execution_retry_snapshot(
            &controller_runtime.data_root,
            session_id,
            &snapshot,
        )?;
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
        // currentReference is compared exactly so an older ToolStarted event cannot overwrite a newer safe boundary.
        // currentReference 通过精确比较，避免较旧的 ToolStarted 事件覆盖较新的安全边界。
        let current_reference = runtime_state.execution_retry_snapshot.clone();
        match current_reference.as_ref() {
            Some(current) if current == &reference => {
                runtime_state.execution_retry_snapshot = Some(unsafe_reference);
            }
            Some(current) if current.replay_safety_proven() => {
                return Err(String::from(
                    "Execution retry reference changed before the tool-safety boundary could be published.",
                ));
            }
            _ => {
                // A concurrent non-replayable or absent reference is already fail-closed and remains authoritative.
                // 并发产生的不可重放引用或缺失引用已经失败关闭，并继续保持权威。
            }
        }
        runtime_state.provider_call_ledger.continuation.eligible = false;
        runtime_state.updated_at = runtime_state
            .execution_retry_snapshot
            .as_ref()
            .map(|current| current.updated_at.clone())
            .unwrap_or_else(current_unix_millis_text);
    }
    // authoritativeSnapshot is read only after the shared write gate is held, preserving any newer same-session mutation that queued before this mandatory safety write.
    // authoritativeSnapshot 仅在持有共享写入闸门后读取，从而保留在本次强制安全写入前排队的任何较新同会话变更。
    let (_persist_guard, authoritative_snapshot) =
        lock_latest_retained_workspace_session_persistence_snapshot(task_state, session_id).await?;
    // runtimeStateUpdate is the newest retained revision and still contains the fail-closed side-effect reference published above.
    // runtimeStateUpdate 是最新保留修订，并仍包含上方发布的失败关闭副作用引用。
    let runtime_state_update = authoritative_snapshot
        .session_runtime_state
        .get(&session_id)
        .ok_or_else(|| {
            String::from("Execution retry runtime state disappeared before durable tool safety.")
        })?;
    vulcan_workspace_store::workspace_store::persist_single_session_runtime_state_update(
        controller_runtime.clone(),
        session_id,
        runtime_state_update,
    )
    .await
    .map_err(|error| {
        format!("Failed to durably publish the tool-execution retry safety boundary: {error}")
    })
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
    use vulcan_core::types::{ConversationMessage, ConversationPayload};

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

        assert!(has_new_committed_tool_result(&previous, &current));
    }

    /// Verifies an unchanged or non-tool tail never clears an unsafe boundary.
    /// 验证未变化或非工具尾部绝不会清除不安全边界。
    #[test]
    fn unchanged_boundary_is_not_marked_as_next_provider_ready() {
        let previous = assistant_tool_call_snapshot();
        assert!(!has_new_committed_tool_result(&previous, &previous));
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
