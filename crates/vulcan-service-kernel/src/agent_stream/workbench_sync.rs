//! Workbench synchronization helpers for stream updates.
//! 流更新使用的工作台同步辅助函数。

use super::*;

/// Builds the focused route response needed by subagent panel RPC calls.
/// 构建子代理面板 RPC 所需的聚焦路由响应。
pub(crate) fn build_subagent_panel_route_response_locked(
    guard: &mut ServiceBridgeState,
    workspace_snapshot: &WorkspaceSnapshot,
) -> SubagentPanelRouteResponse {
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
    SubagentPanelRouteResponse {
        parent_conversation_id,
        active_conversation_id,
        subagent_navigation,
        messages,
        revision,
    }
}

/// Applies execution preparation identity fields to the target session runtime state.
/// 将执行准备得到的身份字段应用到目标会话运行时状态。
pub(crate) fn apply_execution_preparation(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    preparation: &ExecutionPreparation,
) -> Result<(), String> {
    let apply_started_at = current_unix_millis_value();
    let conversation_id = preparation.session_id.to_string();
    let (events, boundary_project_id) = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        if let Some(context_budget) = preparation.context_budget.clone() {
            guard
                .live_context_budgets
                .insert(preparation.session_id, context_budget);
        }
        let live_context_budgets = guard.live_context_budgets.clone();
        let provider_configuration = guard
            .agent_runtime
            .as_ref()
            .map(|runtime| runtime.provider_configuration.clone())
            .unwrap_or_default();
        let ai_tool_configuration = guard
            .agent_runtime
            .as_ref()
            .map(|runtime| runtime.ai_tool_configuration.clone())
            .unwrap_or_default();
        let (boundary_project_id, context_usage) = {
            let workspace = guard.workspace_snapshot.as_mut().ok_or_else(|| {
                String::from("service is still starting; workspace snapshot is not ready")
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
            let boundary_project_id =
                owning_project_id_for_session(workspace, preparation.session_id);
            workspace
                .session_run_phases
                .insert(preparation.session_id, SessionRunPhase::Preparing);
            let context_usage = context_usage_snapshot_from_runtime_state(
                preparation.session_id,
                workspace.session_runtime_state.get(&preparation.session_id),
                &live_context_budgets,
                &provider_configuration,
                &ai_tool_configuration,
            );
            (boundary_project_id, context_usage)
        };
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        let phase_generation = guard.workbench_generation;
        guard.workbench.revision = phase_generation;
        for conversation in &mut guard.workbench.conversations {
            if conversation.id == conversation_id {
                conversation.run_phase = SessionRunPhase::Preparing.to_string();
                conversation.status = SessionState::Running.label().to_string();
                conversation.last_updated_label = String::from("刚刚");
                conversation.last_updated_at = current_unix_millis_text();
                break;
            }
        }
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        let context_generation = guard.workbench_generation;
        guard.workbench.revision = context_generation;
        guard.workbench.context_usage = context_usage.clone();
        (
            vec![
                WorkbenchDeltaEnvelope {
                    generation: phase_generation,
                    delta: WorkbenchDelta::UpdateConversationRunPhase(
                        UpdateConversationRunPhaseDelta {
                            conversation_id,
                            run_phase: SessionRunPhase::Preparing.to_string(),
                            status: Some(SessionState::Running.label().to_string()),
                            last_updated_label: Some(String::from("now")),
                        },
                    ),
                },
                WorkbenchDeltaEnvelope {
                    generation: context_generation,
                    delta: WorkbenchDelta::ContextUsageChanged(ContextUsageChangedDelta {
                        context_usage,
                    }),
                },
            ],
            boundary_project_id,
        )
    };
    webui_after_precheck_trace(
        "after_precheck_25a_preparation_state_committed",
        format!(
            "session={} elapsed_ms={}",
            preparation.session_id,
            current_unix_millis_value().saturating_sub(apply_started_at)
        ),
    );
    for event in events {
        emit_service_event_with_trace(
            app,
            "workbench_preparation_delta_emit",
            ServiceEvent::WorkbenchDelta(Box::new(event)),
        );
    }
    webui_after_precheck_trace(
        "after_precheck_25b_preparation_delta_emitted",
        format!(
            "session={} elapsed_ms_since_apply={}",
            preparation.session_id,
            current_unix_millis_value().saturating_sub(apply_started_at)
        ),
    );
    if let Some(project_id) = boundary_project_id {
        let lifecycle_started_at = current_unix_millis_value();
        begin_session_external_turn_lifecycle(controller_runtime, &project_id, preparation)?;
        webui_after_precheck_trace(
            "after_precheck_25c_lifecycle_begin_finished",
            format!(
                "session={} elapsed_ms={} elapsed_ms_since_apply={}",
                preparation.session_id,
                current_unix_millis_value().saturating_sub(lifecycle_started_at),
                current_unix_millis_value().saturating_sub(apply_started_at)
            ),
        );
    } else {
        webui_after_precheck_trace(
            "after_precheck_25c_lifecycle_begin_skipped",
            format!(
                "session={} elapsed_ms_since_apply={}",
                preparation.session_id,
                current_unix_millis_value().saturating_sub(apply_started_at)
            ),
        );
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
    let conversation_id = session_id.to_string();
    let event = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let workspace = guard.workspace_snapshot.as_mut().ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
        workspace.session_run_phases.insert(session_id, phase);
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        let generation = guard.workbench_generation;
        guard.workbench.revision = generation;
        let status =
            is_active_session_run_phase(phase).then(|| SessionState::Running.label().to_string());
        for conversation in &mut guard.workbench.conversations {
            if conversation.id == conversation_id {
                conversation.run_phase = phase.to_string();
                if let Some(status) = status.as_ref() {
                    conversation.status = status.clone();
                }
                conversation.last_updated_label = String::from("刚刚");
                conversation.last_updated_at = current_unix_millis_text();
                break;
            }
        }
        WorkbenchDeltaEnvelope {
            generation,
            delta: WorkbenchDelta::UpdateConversationRunPhase(UpdateConversationRunPhaseDelta {
                conversation_id,
                run_phase: phase.to_string(),
                status,
                last_updated_label: Some(String::from("now")),
            }),
        }
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
