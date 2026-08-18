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

/// Deletes the retry sidecar after the accepted success projection is committed.
/// 在已接纳成功投影提交后删除重试 sidecar。
///
/// # Parameters
/// 参数
/// - `task_state`: Shared retained workspace state whose reference is cleared first.
/// - `task_state`：首先清除引用的共享保留工作区状态。
/// - `controller_runtime`: Workspace data-plane owner used to delete the sidecar.
/// - `controller_runtime`：用于删除 sidecar 的工作区数据面所有者。
/// - `session_id`: Session whose successful execution no longer needs recovery.
/// - `session_id`：成功执行后不再需要恢复的会话。
///
/// # Returns
/// 返回值
/// Success after no retry reference or sidecar remains; deletion failure restores the reference.
/// 不再保留重试引用或 sidecar 后返回成功；删除失败时恢复引用。
pub(super) fn remove_execution_retry_snapshot_after_success(
    task_state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    session_id: SessionId,
) -> Result<(), String> {
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
    let Some(reference) = reference else {
        return Ok(());
    };
    if let Err(error) = vulcan_workspace_store::workspace_store::delete_execution_retry_snapshot(
        &controller_runtime.data_root,
        session_id,
        &reference.snapshot_id,
    ) {
        let mut guard = task_state.lock().map_err(|lock_error| {
            format!("{error}; failed to restore retry reference: {lock_error}")
        })?;
        let runtime_state = guard
            .workspace_snapshot
            .as_mut()
            .and_then(|workspace| workspace.session_runtime_state.get_mut(&session_id))
            .ok_or_else(|| format!("{error}; retry runtime state disappeared during rollback"))?;
        if runtime_state.execution_retry_snapshot.is_none() {
            runtime_state.execution_retry_snapshot = Some(reference);
        }
        return Err(error);
    }
    Ok(())
}

/// Returns a claimed retry snapshot to the frozen state after stop or abnormal stream termination.
/// 在停止或异常流终止后将已认领重试快照退回冻结状态。
pub(crate) fn refreeze_claimed_execution_retry_snapshot(
    task_state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    session_id: SessionId,
    message: &str,
) -> Result<(), String> {
    let retry_state = {
        let guard = task_state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
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
    )?;
    // Terminal projection may append canonical records after the provider request checkpoint was
    // frozen. Refresh both the controller snapshot and its digest atomically so Continue validates
    // the current canonical state while retaining the exact frozen provider request.
    // Provider 请求检查点冻结后，终态投影可能继续追加规范记录。这里原子刷新控制器快照及其
    // 摘要，使 Continue 校验当前规范状态，同时保留精确冻结的 Provider 请求。
    refresh_execution_retry_canonical_state(&mut snapshot, &conversation_state)?;
    snapshot.state = ExecutionRetrySnapshotState::Frozen;
    snapshot.claimed_attempt_id = None;
    snapshot.updated_at = current_unix_millis_text();
    if snapshot.error.as_ref().is_none_or(|error| error.retryable) {
        snapshot.error = Some(vulcan_ai_contract::AiExecutionError {
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
    }
    let updated_reference =
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
            String::from("Execution retry runtime state disappeared while refreezing.")
        })?;
    runtime_state.execution_retry_snapshot = Some(updated_reference);
    Ok(())
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

/// Marks the current safe request snapshot non-replayable while a tool side effect is in flight.
/// 在工具副作用执行期间将当前安全请求快照标记为不可重放。
pub(crate) fn mark_execution_retry_snapshot_tool_execution_unsafe(
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
    if reference.state != ExecutionRetrySnapshotState::Frozen {
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
    let mut guard = task_state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let runtime_state = guard
        .workspace_snapshot
        .as_mut()
        .and_then(|workspace| workspace.session_runtime_state.get_mut(&session_id))
        .ok_or_else(|| String::from("Execution retry runtime state disappeared at tool start."))?;
    runtime_state.execution_retry_snapshot = Some(unsafe_reference);
    Ok(())
}

/// Invalidates only the retained retry reference when its authoritative sidecar cannot transition.
/// 当权威 sidecar 无法完成状态迁移时，仅使保留的重试引用失效。
pub(super) fn invalidate_execution_retry_reference(
    task_state: &SharedServiceBridgeState,
    session_id: SessionId,
    message: &str,
) -> Result<(), String> {
    let mut guard = task_state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let reference = guard
        .workspace_snapshot
        .as_mut()
        .and_then(|workspace| workspace.session_runtime_state.get_mut(&session_id))
        .and_then(|runtime_state| runtime_state.execution_retry_snapshot.as_mut());
    if let Some(reference) = reference {
        reference.state = ExecutionRetrySnapshotState::Invalidated;
        reference.claimed_attempt_id = None;
        reference.error_summary = Some(message.to_string());
        reference.updated_at = current_unix_millis_text();
    }
    Ok(())
}

/// Computes a lowercase SHA-256 digest for exact serialized checkpoint bytes.
/// 为精确序列化的 checkpoint 字节计算小写 SHA-256 摘要。
fn sha256_hex(encoded: &[u8]) -> String {
    use sha2::{Digest, Sha256};

    let digest = Sha256::digest(encoded);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}
