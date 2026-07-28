//! Workbench synchronization helpers for stream updates.
//! 流更新使用的工作台同步辅助函数。

use super::*;

/// Builds the focused route response needed by subagent panel RPC calls.
/// 构建子代理面板 RPC 所需的聚焦路由响应。
pub(crate) fn build_subagent_panel_route_response_locked(
    guard: &mut ServiceBridgeState,
    workspace_snapshot: &WorkspaceSnapshot,
) -> Result<SubagentPanelRouteResponse, String> {
    let subagent_navigation =
        subagent_navigation_snapshot_from_cached_state(guard, workspace_snapshot)?;
    let active_session_id =
        preserved_active_surface_session_id(workspace_snapshot, subagent_navigation.as_ref());
    let mut surface_messages = surface_messages_for_session_without_subagent_navigation_refresh(
        guard,
        workspace_snapshot,
        active_session_id,
    );
    repair_task_mirror_statuses_for_surface_messages_locked(
        guard,
        workspace_snapshot,
        active_session_id,
        surface_messages.as_mut_slice(),
    );
    let selected_conversation_id =
        selected_workspace_session_id(workspace_snapshot).map(|session_id| session_id.to_string());
    let active_conversation_id = subagent_navigation
        .as_ref()
        .map(|navigation| navigation.active_conversation_id.clone())
        .or_else(|| selected_conversation_id.clone());
    let parent_conversation_id = subagent_navigation
        .as_ref()
        .map(|navigation| navigation.parent_conversation_id.clone())
        .or_else(|| selected_conversation_id.clone());
    let messages = active_conversation_id
        .as_deref()
        .map(|conversation_id| {
            conversation_messages_from_session(conversation_id, surface_messages.as_slice(), false)
        })
        .unwrap_or_default();
    guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
    let revision = guard.workbench_generation;
    guard.workbench.revision = revision;
    guard.workbench.messages = messages.clone();
    guard.workbench.subagent_navigation = subagent_navigation.clone();
    Ok(SubagentPanelRouteResponse {
        parent_conversation_id,
        active_conversation_id,
        subagent_navigation,
        messages,
        revision,
    })
}

/// Commits one execution-preparation mutation only for the exact active stream owner.
/// 仅为精确活动流所有者提交一次执行准备变更。
///
/// # Parameters
/// 参数
/// `state` owns the active task registry and retained preparation state.
/// `state` 持有活动任务注册表与保留准备状态。
/// `session_id` selects the session slot and `active_stream_task_id` identifies its exact expected owner.
/// `session_id` 选择会话槽位，`active_stream_task_id` 标识其精确预期所有者。
/// `commit` applies the caller's preparation mutation under the same guard after preflight.
/// `commit` 在预检后于同一 guard 内应用调用方的准备变更。
///
/// # Returns
/// 返回值
/// Returns the committed caller value, or an explicit ownership/workspace error before mutation.
/// 返回已提交的调用方值；所有权或工作区异常时在变更前返回显式错误。
pub(crate) fn commit_owned_execution_preparation_state<T, F>(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    active_stream_task_id: Uuid,
    commit: F,
) -> Result<T, String>
where
    F: FnOnce(&mut ServiceBridgeState) -> Result<T, String>,
{
    // guard binds exact task ownership, workspace preflight, and caller mutation into one transaction.
    // guard 将精确任务所有权、工作区预检与调用方变更绑定为一个事务。
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    if !active_stream_task_matches_id(&guard, session_id, active_stream_task_id) {
        return Err(format!(
            "execution preparation ownership was superseded: session={session_id} task={active_stream_task_id}"
        ));
    }
    if guard.workspace_snapshot.is_none() {
        return Err(String::from(
            "service is still starting; workspace snapshot is not ready",
        ));
    }
    commit(&mut guard)
}

/// Applies execution preparation identity fields to the target session runtime state.
/// 将执行准备得到的身份字段应用到目标会话运行时状态。
///
/// # Parameters
/// 参数
/// `app` emits committed preparation deltas and `state` owns retained service state.
/// `app` 发送已提交准备增量，`state` 持有保留服务状态。
/// `controller_runtime` persists the external session lifecycle.
/// `controller_runtime` 持久化外部会话生命周期。
/// `active_stream_task_id` proves the exact stream owner and `preparation` supplies immutable runtime facts.
/// `active_stream_task_id` 证明精确流所有者，`preparation` 提供不可变运行时事实。
///
/// # Returns
/// 返回值
/// Returns success after lifecycle start and delta emission, or an explicit preflight/lifecycle error.
/// 生命周期开始且增量发送后返回成功；预检或生命周期异常时返回显式错误。
pub(crate) fn apply_execution_preparation(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    active_stream_task_id: Uuid,
    preparation: &ExecutionPreparation,
) -> Result<(), String> {
    let apply_started_at = current_unix_millis_value();
    let (events, boundary_project_id) = commit_owned_execution_preparation_state(
        state,
        preparation.session_id,
        active_stream_task_id,
        |guard| {
            if let Some(context_budget) = preparation.context_budget.clone() {
                guard
                    .live_context_budgets
                    .insert(preparation.session_id, context_budget);
            }
            let live_context_budgets = guard.live_context_budgets.clone();
            let public_model_capabilities = active_public_ai_model_capabilities(
                guard
                    .agent_runtime
                    .as_ref()
                    .map(|runtime| runtime.public_ai_runtime.as_ref()),
            );
            let (boundary_project_id, context_usage) = {
                // workspace passed preflight before live budget and runtime identity mutation.
                // workspace 在 live budget 与运行时身份变更前已通过预检。
                let workspace = guard.workspace_snapshot.as_mut().ok_or_else(|| {
                    String::from("workspace snapshot is not ready after preflight")
                })?;
                let runtime_state = workspace
                    .session_runtime_state
                    .entry(preparation.session_id)
                    .or_insert_with(|| SessionRuntimeStateEntry {
                        updated_at: current_unix_millis_text(),
                        ..SessionRuntimeStateEntry::default()
                    });
                runtime_state.active_agent_id = Some(preparation.agent_id.clone());
                runtime_state.active_provider_id =
                    Some(preparation.model_blueprint.provider_id.clone());
                runtime_state.active_model_id = Some(preparation.model_blueprint.model_id.clone());
                runtime_state.active_reasoning_effort =
                    Some(preparation.model_blueprint.reasoning.requested_effort);
                runtime_state.active_temperature = preparation.temperature;
                runtime_state.live_context_budget_fingerprint = preparation
                    .context_budget
                    .as_ref()
                    .map(|budget| budget.fingerprint.clone());
                runtime_state.live_context_assembly_fingerprint = preparation
                    .context_baseline
                    .as_ref()
                    .map(|baseline| baseline.fingerprint.clone());
                runtime_state.updated_at = current_unix_millis_text();
                let boundary_project_id =
                    owning_project_id_for_session(workspace, preparation.session_id);
                let context_usage = context_usage_snapshot_from_runtime_state(
                    preparation.session_id,
                    workspace.session_runtime_state.get(&preparation.session_id),
                    &live_context_budgets,
                    public_model_capabilities.as_ref(),
                );
                (boundary_project_id, context_usage)
            };
            // phase_event reuses the shared workspace, Workbench, timestamp, and status projection.
            // phase_event 复用共享的工作区、Workbench、时间戳与状态投影。
            let phase_event = apply_session_phase_to_state(
                guard,
                preparation.session_id,
                SessionRunPhase::Preparing,
            )?;
            guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
            let context_generation = guard.workbench_generation;
            guard.workbench.revision = context_generation;
            guard.workbench.context_usage = context_usage.clone();
            Ok((
                vec![
                    phase_event,
                    WorkbenchDeltaEnvelope {
                        generation: context_generation,
                        delta: WorkbenchDelta::ContextUsageChanged(ContextUsageChangedDelta {
                            context_usage,
                        }),
                    },
                ],
                boundary_project_id,
            ))
        },
    )?;
    webui_after_precheck_trace(
        "after_precheck_25a_preparation_state_committed",
        format!(
            "session={} elapsed_ms={}",
            preparation.session_id,
            current_unix_millis_value().saturating_sub(apply_started_at)
        ),
    );
    if let Some(project_id) = boundary_project_id {
        let lifecycle_started_at = current_unix_millis_value();
        begin_session_external_turn_lifecycle(controller_runtime, &project_id, preparation)?;
        webui_after_precheck_trace(
            "after_precheck_25b_lifecycle_begin_finished",
            format!(
                "session={} elapsed_ms={} elapsed_ms_since_apply={}",
                preparation.session_id,
                current_unix_millis_value().saturating_sub(lifecycle_started_at),
                current_unix_millis_value().saturating_sub(apply_started_at)
            ),
        );
    } else {
        webui_after_precheck_trace(
            "after_precheck_25b_lifecycle_begin_skipped",
            format!(
                "session={} elapsed_ms_since_apply={}",
                preparation.session_id,
                current_unix_millis_value().saturating_sub(apply_started_at)
            ),
        );
    }
    for event in events {
        emit_service_event_with_trace(
            app,
            "workbench_preparation_delta_emit",
            ServiceEvent::WorkbenchDelta(Box::new(event)),
        );
    }
    webui_after_precheck_trace(
        "after_precheck_25c_preparation_delta_emitted",
        format!(
            "session={} elapsed_ms_since_apply={}",
            preparation.session_id,
            current_unix_millis_value().saturating_sub(apply_started_at)
        ),
    );
    Ok(())
}

/// Begins the session lifecycle for one WebUI provider-stream turn.
/// 开始单个 WebUI provider 流式轮次的 会话生命周期。
pub(crate) fn begin_session_external_turn_lifecycle(
    controller_runtime: &ControllerRuntimeHandle,
    project_id: &str,
    preparation: &ExecutionPreparation,
) -> Result<(), String> {
    let Some(turn_id) = preparation
        .boundary_events
        .first()
        .and_then(|event| event.turn_id.clone())
    else {
        return Ok(());
    };
    let store = WorkspaceSegmentedSessionBoundaryStore::new(
        &controller_runtime.data_root,
        project_id,
        preparation.session_id,
        &preparation.session_title,
    );
    let runner = SessionLifecycleRunner::new(store, WebuiSessionLifecycleHook);
    runner.begin_turn(SessionLifecycleBeginRequest {
        thread_id: SessionNodeId::from(preparation.session_id),
        turn_id,
        accepted_events: preparation.boundary_events.clone(),
    })
}

/// Finishes the session lifecycle for one WebUI provider-stream turn.
/// 结束单个 WebUI provider 流式轮次的 会话生命周期。
pub(crate) fn finish_session_external_turn_lifecycle(
    controller_runtime: &ControllerRuntimeHandle,
    project_id: &str,
    session_id: SessionId,
    session_title: &str,
    final_events: Vec<SessionBoundaryEvent>,
    terminal_event: SessionBoundaryEvent,
) -> Result<(), String> {
    let Some(turn_id) = terminal_event.turn_id.clone() else {
        return Ok(());
    };
    let terminal_status = session_lifecycle_terminal_status_from_event(&terminal_event)?;
    let store = WorkspaceSegmentedSessionBoundaryStore::new(
        &controller_runtime.data_root,
        project_id,
        session_id,
        session_title,
    );
    let runner = SessionLifecycleRunner::new(store, WebuiSessionLifecycleHook);
    runner.finish_turn(SessionLifecycleFinishRequest {
        thread_id: SessionNodeId::from(session_id),
        turn_id,
        terminal_status,
        final_events,
        terminal_event: Some(terminal_event),
    })
}

/// Parses the explicit terminal status carried by a session-boundary terminal event.
/// 解析会话边界终态事件携带的显式终态状态。
///
/// # Parameters
/// 参数
/// `terminal_event` is the hidden turn-completed boundary event emitted for one provider turn.
/// `terminal_event` 是单个 Provider 回合发出的隐藏回合完成边界事件。
///
/// # Returns
/// 返回值
/// A lifecycle terminal status when the event carries a known status string.
/// 当事件携带已知状态字符串时返回生命周期终态状态。
pub(crate) fn session_lifecycle_terminal_status_from_event(
    terminal_event: &SessionBoundaryEvent,
) -> Result<SessionTurnTerminalStatus, String> {
    let Some(status) = terminal_event.payload.get("status").and_then(Value::as_str) else {
        return Err(format!(
            "terminal boundary event `{}` is missing payload.status",
            terminal_event.event_id
        ));
    };
    match status {
        "completed" => Ok(SessionTurnTerminalStatus::Completed),
        "failed" => Ok(SessionTurnTerminalStatus::Failed),
        other => Err(format!(
            "terminal boundary event `{}` has unknown payload.status `{other}`",
            terminal_event.event_id
        )),
    }
}

/// Updates one session phase in the retained workspace snapshot.
/// 更新保留工作区快照中的单个会话阶段。
///
/// # Parameters
/// # 参数
/// `state` owns the retained workspace and workbench state mutated under the caller's lock.
/// `state` 持有在调用方锁内修改的保留工作区与工作台状态。
/// `session_id` identifies the session whose phase is changing.
/// `session_id` 标识阶段正在变化的会话。
/// `phase` is the exact retained and visible run phase to apply.
/// `phase` 是需要应用的精确保留与可见运行阶段。
///
/// # Returns
/// # 返回值
/// The workbench delta envelope aligned with the completed retained-state mutation.
/// 返回与已完成保留状态变更对齐的工作台增量封装。
pub(crate) fn apply_session_phase_to_state(
    state: &mut ServiceBridgeState,
    session_id: SessionId,
    phase: SessionRunPhase,
) -> Result<WorkbenchDeltaEnvelope, String> {
    // conversation_id is the stable surface key corresponding to the session identity.
    // conversation_id 是与会话身份对应的稳定界面键。
    let conversation_id = session_id.to_string();
    // session_state is the shared authoritative coarse status for this exact phase.
    // session_state 是当前精确阶段对应的共享权威粗粒度状态。
    let session_state = session_state_for_run_phase(phase);
    // updated_at keeps retained workspace and Workbench timestamps aligned to the same mutation.
    // updated_at 使保留工作区与 Workbench 时间戳对齐到同一次变更。
    let updated_at = current_unix_millis_text();
    // workspace owns the authoritative retained run phase.
    // workspace 持有权威保留运行阶段。
    let workspace = state.workspace_snapshot.as_mut().ok_or_else(|| {
        String::from("service is still starting; workspace snapshot is not ready")
    })?;
    workspace.session_run_phases.insert(session_id, phase);
    if let Some(session) = workspace
        .sessions
        .iter_mut()
        .find(|session| session.id == session_id)
    {
        session.status = session_state;
        session.last_updated_label = String::from("刚刚");
        session.updated_at = updated_at.clone();
    }
    state.workbench_generation = state.workbench_generation.wrapping_add(1);
    // generation binds the emitted delta to this exact retained mutation revision.
    // generation 将发送增量绑定到当前精确保留变更版本。
    let generation = state.workbench_generation;
    state.workbench.revision = generation;
    // status_label is shared by retained Workbench state and the emitted delta.
    // status_label 由保留 Workbench 状态与发送增量共享。
    let status_label = session_state.label().to_string();
    for conversation in &mut state.workbench.conversations {
        if conversation.id == conversation_id {
            conversation.run_phase = phase.to_string();
            conversation.status = status_label.clone();
            conversation.last_updated_label = String::from("刚刚");
            conversation.last_updated_at = updated_at;
            break;
        }
    }
    Ok(WorkbenchDeltaEnvelope {
        generation,
        delta: WorkbenchDelta::UpdateConversationRunPhase(UpdateConversationRunPhaseDelta {
            conversation_id,
            run_phase: phase.to_string(),
            status: Some(status_label),
            last_updated_label: Some(String::from("now")),
        }),
    })
}

/// Ownership outcome from applying one active-stream phase transition.
/// 应用一次活动流阶段转换后的所有权结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ActiveStreamPhaseCommit {
    /// The exact task owner committed retained phase state and its aligned delta.
    /// 精确任务所有者已提交保留阶段状态及其对齐增量。
    Committed,
    /// A replacement task owns the session, so no phase state or delta changed.
    /// 替代任务拥有会话，因此阶段状态与增量均未改变。
    Superseded,
}

/// Applies one session phase only while the exact active stream task owns the session slot.
/// 仅在精确活动流任务拥有会话槽位时应用单个会话阶段。
///
/// # Parameters
/// 参数
/// `app` emits the committed phase delta and `state` owns retained phase and task state.
/// `app` 发送已提交阶段增量，`state` 持有保留阶段与任务状态。
/// `session_id` selects the session, `active_stream_task_id` proves its exact owner, and `phase` is the requested transition.
/// `session_id` 选择会话，`active_stream_task_id` 证明其精确所有者，`phase` 是请求的转换。
///
/// # Returns
/// 返回值
/// Returns a committed or superseded ownership result, or an explicit retained-state error.
/// 返回已提交或已被替代的所有权结果，或显式保留状态错误。
pub(crate) fn apply_active_stream_session_phase(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    active_stream_task_id: Uuid,
    phase: SessionRunPhase,
) -> Result<ActiveStreamPhaseCommit, String> {
    // phase_event exists only when exact ownership and retained mutation commit under one guard.
    // phase_event 仅在精确所有权与保留变更于同一 guard 内提交时存在。
    let phase_event = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        if !active_stream_task_matches_id(&guard, session_id, active_stream_task_id) {
            return Ok(ActiveStreamPhaseCommit::Superseded);
        }
        apply_session_phase_to_state(&mut guard, session_id, phase)?
    };
    emit_service_event_with_trace(
        app,
        "workbench_active_stream_phase_delta_emit",
        ServiceEvent::WorkbenchDelta(Box::new(phase_event)),
    );
    Ok(ActiveStreamPhaseCommit::Committed)
}

/// Updates one session phase in retained state and emits its aligned workbench delta.
/// 更新保留状态中的单个会话阶段并发送与之对齐的工作台增量。
///
/// # Parameters
/// # 参数
/// `app` emits the resulting workbench phase delta.
/// `app` 发送生成的工作台阶段增量。
/// `state` owns the retained workspace and workbench state.
/// `state` 持有保留工作区与工作台状态。
/// `session_id` identifies the session whose phase is changing.
/// `session_id` 标识阶段正在变化的会话。
/// `phase` is the exact retained and visible run phase to apply.
/// `phase` 是需要应用的精确保留与可见运行阶段。
///
/// # Returns
/// # 返回值
/// Success after retained mutation and delta emission, or an explicit state error.
/// 在完成保留状态变更与增量发送后返回成功，否则返回显式状态错误。
pub(crate) fn apply_session_phase(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    phase: SessionRunPhase,
) -> Result<(), String> {
    // event is built while the retained state lock is held and emitted only after lock release.
    // event 在持有保留状态锁时构建，并仅在释放锁后发送。
    let event = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        apply_session_phase_to_state(&mut guard, session_id, phase)?
    };
    emit_service_event_with_trace(
        app,
        "workbench_phase_delta_emit",
        ServiceEvent::WorkbenchDelta(Box::new(event)),
    );
    Ok(())
}

/// Marks the in-flight prompt snapshot as preparing before slower outer steps run.
/// 在较慢的外层步骤执行前，将当前提示词快照标记为准备中。
pub(crate) fn mark_webui_prompt_snapshot_preparing(
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
) {
    let now = current_unix_millis_text();
    let phase = workspace_snapshot
        .session_run_phases
        .get(&session_id)
        .copied()
        .unwrap_or(SessionRunPhase::Preparing);
    // Preserve the explicit memory-recall phase until the precheck continuation promotes the turn to provider preparation.
    // 保留显式记忆召回阶段，直到 precheck continuation 将本轮推进到 Provider 准备阶段。
    if phase != SessionRunPhase::MemoryRecall {
        workspace_snapshot
            .session_run_phases
            .insert(session_id, SessionRunPhase::Preparing);
    }
    if let Some(runtime_state) = workspace_snapshot
        .session_runtime_state
        .get_mut(&session_id)
    {
        runtime_state.updated_at = now.clone();
    }
    if let Some(session) = workspace_session_summary_for_session_mut(workspace_snapshot, session_id)
    {
        session.status = SessionState::Running;
        session.focus = String::from("正在准备请求");
        session.last_updated_label = String::from("刚刚");
    }
}
