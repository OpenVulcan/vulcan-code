//! Workbench synchronization helpers for stream updates.
//! 流更新使用的工作台同步辅助函数。

use super::*;

/// Rebuilds and emits the retained workbench snapshot after a service-side queue mutation.
/// 在 service 侧队列变更后重建并发出保留的工作台快照。
pub(crate) fn store_current_workbench_snapshot(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    workspace_snapshot: WorkspaceSnapshot,
) -> Result<WorkbenchSnapshot, String> {
    // Measures this retained-workbench rebuild entry used by precheck and mailbox turns.
    // 衡量 precheck 与 mailbox 回合共用的保留工作台重建入口。
    let trace_started_at = current_unix_millis_value();
    // Measures runtime catalog extraction before surface projection.
    // 衡量界面投影前运行时目录提取耗时。
    let catalog_started_at = current_unix_millis_value();
    let (luaskills_catalog, token_budget_runtime, mcp_catalog, logs) = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let luaskills_catalog = cached_luaskills_catalog_from_state(&guard);
        let token_budget_runtime = guard.token_budget_runtime.clone().ok_or_else(|| {
            String::from("service is still starting; token budget runtime is not ready")
        })?;
        let mcp_catalog = cached_mcp_catalog_from_state(&guard);
        (
            luaskills_catalog,
            token_budget_runtime,
            mcp_catalog,
            guard.startup.recent_logs.clone(),
        )
    };
    webui_trace_if_slow(
        "store_current_workbench_catalog_slow",
        catalog_started_at,
        format!("selected_session={}", workspace_snapshot.selected_session),
    );
    // Measures active surface and subagent navigation projection.
    // 衡量活动界面与子代理导航投影耗时。
    let surface_started_at = current_unix_millis_value();
    let (
        live_context_budgets,
        surface_messages,
        subagent_navigation,
        session_goal_summaries,
        hydrated_window_summaries,
    ) = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        retain_hydrated_conversation_windows_for_workspace(
            &mut guard.hydrated_conversation_windows,
            &workspace_snapshot,
        );
        (
            guard.live_context_budgets.clone(),
            active_surface_messages_from_workspace_and_state(&mut guard, &workspace_snapshot),
            subagent_navigation_snapshot_from_state(&mut guard, &workspace_snapshot),
            guard.session_goal_summaries.clone(),
            hydrated_conversation_window_summaries(&guard.hydrated_conversation_windows),
        )
    };
    webui_trace_if_slow(
        "store_current_workbench_surface_slow",
        surface_started_at,
        format!(
            "selected_session={} surface_messages={} has_subagent_navigation={}",
            workspace_snapshot.selected_session,
            surface_messages.len(),
            subagent_navigation.is_some()
        ),
    );
    // Measures final DTO construction after runtime projection inputs are ready.
    // 衡量运行时投影输入就绪后的最终 DTO 构建耗时。
    let workbench_started_at = current_unix_millis_value();
    let workbench = workbench_from_runtime(
        controller_runtime,
        Some(agent_runtime.command_session_runtime.as_ref()),
        &luaskills_catalog,
        &token_budget_runtime,
        &mcp_catalog,
        &agent_runtime.provider_configuration,
        &agent_runtime.ai_tool_configuration,
        &workspace_snapshot,
        surface_messages.as_slice(),
        subagent_navigation,
        pending_question_snapshot_from_agent_runtime(Some(agent_runtime)),
        &logs,
        &live_context_budgets,
        &session_goal_summaries,
        &hydrated_window_summaries,
    );
    webui_trace_if_slow(
        "store_current_workbench_build_slow",
        workbench_started_at,
        format!("selected_session={}", workspace_snapshot.selected_session),
    );
    // Measures retained state commit for rebuilt workbench snapshots.
    // 衡量重建工作台快照写回保留状态的耗时。
    let commit_started_at = current_unix_millis_value();
    {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        guard.workspace_snapshot = Some(workspace_snapshot);
        guard.workbench = workbench.clone();
    }
    webui_trace_if_slow(
        "store_current_workbench_commit_slow",
        commit_started_at,
        format!(
            "selected_conversation={}",
            workbench
                .selected_conversation_id
                .as_deref()
                .unwrap_or("<none>")
        ),
    );
    // Measures frontend event emission cost for rebuilt workbench snapshots.
    // 衡量重建工作台快照前端事件发出的耗时。
    let emit_started_at = current_unix_millis_value();
    emit_workbench_event(app, &workbench);
    webui_trace_if_slow(
        "store_current_workbench_emit_slow",
        emit_started_at,
        format!(
            "selected_conversation={}",
            workbench
                .selected_conversation_id
                .as_deref()
                .unwrap_or("<none>")
        ),
    );
    webui_trace_if_slow(
        "store_current_workbench_total_slow",
        trace_started_at,
        format!(
            "selected_conversation={}",
            workbench
                .selected_conversation_id
                .as_deref()
                .unwrap_or("<none>")
        ),
    );
    Ok(workbench)
}

/// Emits a display-only workbench snapshot after a subagent route change.
/// 在子代理显示路由变更后发出仅显示用途的工作台快照。
pub(crate) fn emit_subagent_display_route_workbench_snapshot(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    workspace_snapshot: WorkspaceSnapshot,
) -> Result<WorkbenchSnapshot, String> {
    let workbench = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let workbench =
            build_subagent_display_route_workbench_snapshot_locked(&mut guard, &workspace_snapshot);
        guard.workspace_snapshot = Some(workspace_snapshot);
        workbench
    };
    emit_workbench_event(app, &workbench);
    Ok(workbench)
}

/// Builds one display-only workbench snapshot for a subagent route without cloning the full workspace snapshot.
/// 构建一份子代理路由专用的显示快照，避免克隆完整工作区快照。
pub(crate) fn build_subagent_display_route_workbench_snapshot_locked(
    guard: &mut ServiceBridgeState,
    workspace_snapshot: &WorkspaceSnapshot,
) -> WorkbenchSnapshot {
    let subagent_navigation =
        subagent_navigation_snapshot_from_cached_state(guard, workspace_snapshot);
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
    let mut workbench = guard.workbench.clone();
    let active_conversation_id = subagent_navigation
        .as_ref()
        .map(|navigation| navigation.active_conversation_id.clone())
        .or_else(|| {
            (workspace_snapshot.selected_session != SessionId::nil())
                .then(|| workspace_snapshot.selected_session.to_string())
        });
    workbench.messages = active_conversation_id
        .as_deref()
        .map(|conversation_id| {
            conversation_messages_from_session(conversation_id, surface_messages.as_slice(), false)
        })
        .unwrap_or_default();
    workbench.subagent_navigation = subagent_navigation;
    guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
    guard.workbench = workbench.clone();
    workbench
}

/// Applies execution preparation identity fields to the target session runtime state.
/// 将执行准备得到的身份字段应用到目标会话运行时状态。
pub(crate) fn apply_execution_preparation(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    preparation: &ExecutionPreparation,
) -> Result<(), String> {
    if let Some(context_budget) = preparation.context_budget.clone() {
        state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?
            .live_context_budgets
            .insert(preparation.session_id, context_budget);
    }
    let mut boundary_project_id: Option<String> = None;
    with_workspace_mutation(app, state, |workspace| {
        let runtime_state = workspace
            .session_runtime_state
            .entry(preparation.session_id)
            .or_insert_with(|| SessionRuntimeStateEntry {
                updated_at: current_unix_millis_text(),
                ..SessionRuntimeStateEntry::default()
            });
        runtime_state.active_agent_id = Some(preparation.agent_id.clone());
        runtime_state.active_provider_id = Some(preparation.model_blueprint.provider_id.clone());
        runtime_state.active_model_id = Some(preparation.model_blueprint.model_id.clone());
        runtime_state.active_openrouter_endpoint_route = preparation
            .model_blueprint
            .openrouter_endpoint_route
            .clone();
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
        boundary_project_id = owning_project_id_for_session(workspace, preparation.session_id);
        workspace
            .session_run_phases
            .insert(preparation.session_id, SessionRunPhase::Preparing);
    })
    .map(|_| ())?;
    if let Some(project_id) = boundary_project_id {
        begin_session_external_turn_lifecycle(controller_runtime, &project_id, preparation)?;
    }
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
    let terminal_status = terminal_event
        .payload
        .get("status")
        .and_then(Value::as_str)
        .map(|status| {
            if status == "completed" {
                SessionTurnTerminalStatus::Completed
            } else {
                SessionTurnTerminalStatus::Failed
            }
        })
        .unwrap_or(SessionTurnTerminalStatus::Completed);
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

/// Updates one session phase in the retained workspace snapshot.
/// 更新保留工作区快照中的单个会话阶段。
pub(crate) fn apply_session_phase(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    phase: SessionRunPhase,
) -> Result<(), String> {
    with_workspace_mutation(app, state, |workspace| {
        workspace.session_run_phases.insert(session_id, phase);
    })
    .map(|_| ())
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
    if let Some(session) = workspace_snapshot
        .sessions
        .iter_mut()
        .find(|session| session.id == session_id)
    {
        session.status = SessionState::Running;
        session.focus = String::from("正在准备请求");
        session.last_updated_label = String::from("刚刚");
    }
}
