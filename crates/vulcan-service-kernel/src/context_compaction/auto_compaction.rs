//! Context compaction auto compaction helpers.
//! 上下文压缩 auto compaction 辅助逻辑。

use super::*;

pub(crate) async fn maybe_auto_compact_webui_session_before_provider_request(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    request: &ExecutionPreparationRequest,
    preparation: &ExecutionPreparation,
) -> Result<Option<ExecutionPreparationRequest>, String> {
    if request.prompt_purpose != ExecutionPromptPurpose::UserTurn {
        return Ok(None);
    }
    let Some(context_budget) = preparation.context_budget.as_ref() else {
        return Ok(None);
    };
    if !context_budget.should_compact() {
        return Ok(None);
    }
    let session_id = request.session_id;
    let budget_note = context_budget.summary_note();
    webui_trace(
        "stream_auto_compaction_needed",
        format!(
            "session={session_id} severity={:?} usage={}",
            context_budget.severity,
            context_budget.usage_label()
        ),
    );

    let full_context_state = request.conversation_state.clone();
    let context_start_message_id = current_workspace_snapshot_from_shared_state(state)
        .ok()
        .flatten()
        .and_then(|workspace| {
            workspace
                .session_runtime_state
                .get(&session_id)
                .and_then(|runtime_state| runtime_state.context_start_message_id.clone())
        });
    let (previous_phase, status_message_id) = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let workspace = guard
            .workspace_snapshot
            .as_mut()
            .ok_or_else(|| String::from("workspace snapshot is not ready"))?;
        let previous_phase = workspace
            .session_run_phases
            .get(&session_id)
            .copied()
            .unwrap_or(SessionRunPhase::Preparing);
        let status_message_id = append_webui_context_compaction_status_message_to_workspace(
            workspace,
            session_id,
            &budget_note,
            AUTO_CONTEXT_COMPACTION_TRIGGER,
        )?;
        workspace
            .session_run_phases
            .insert(session_id, SessionRunPhase::Preparing);
        if let Some(session) = workspace
            .sessions
            .iter_mut()
            .find(|session| session.id == session_id)
        {
            session.status = SessionState::Running;
            session.last_updated_label = String::from("刚刚");
            session.updated_at = current_unix_millis_text();
        }
        (previous_phase, status_message_id)
    };
    emit_context_compaction_message_window_delta(app, state, session_id)?;
    apply_session_phase(app, state, session_id, SessionRunPhase::Preparing)?;

    if webui_auto_context_compaction_failure_count(state, session_id)
        >= MAX_CONSECUTIVE_AUTO_COMPACTION_FAILURES
    {
        let error = format!(
            "自动上下文压缩已连续失败 {MAX_CONSECUTIVE_AUTO_COMPACTION_FAILURES} 次，本次请求已停止，避免覆盖或反复改写当前会话状态。"
        );
        persist_failed_webui_context_compaction(
            app,
            state,
            controller_runtime,
            session_id,
            previous_phase,
            &status_message_id,
            &budget_note,
            AUTO_CONTEXT_COMPACTION_TRIGGER,
            &error,
        )?;
        return Err(error);
    }

    let plan = match webui_context_compaction_plan_from_conversation_state(
        session_id,
        &full_context_state,
        context_start_message_id.as_deref(),
        "自动上下文压缩未找到可压缩的历史上下文，已停止本次请求以避免预算溢出。",
    ) {
        Ok(plan) => plan,
        Err(error) => {
            persist_failed_webui_context_compaction(
                app,
                state,
                controller_runtime,
                session_id,
                previous_phase,
                &status_message_id,
                &budget_note,
                AUTO_CONTEXT_COMPACTION_TRIGGER,
                &error,
            )?;
            return Err(error);
        }
    };
    let job_request = match build_webui_context_compaction_job_request(
        state,
        agent_runtime,
        session_id,
        plan,
        full_context_state,
    ) {
        Ok(request) => request,
        Err(error) => {
            persist_failed_webui_context_compaction(
                app,
                state,
                controller_runtime,
                session_id,
                previous_phase,
                &status_message_id,
                &budget_note,
                AUTO_CONTEXT_COMPACTION_TRIGGER,
                &error,
            )?;
            return Err(error);
        }
    };
    let terminal_result = stream_webui_context_compaction_execution(
        app,
        state,
        job_request,
        session_id,
        &status_message_id,
        &budget_note,
        AUTO_CONTEXT_COMPACTION_TRIGGER,
    )
    .await;
    match terminal_result {
        Ok(execution) => {
            let compaction_usage_segment = context_compaction_usage_segment_from_execution(
                AUTO_CONTEXT_COMPACTION_TRIGGER,
                &execution,
            );
            let snapshot = finish_webui_context_compaction_execution(
                app,
                state,
                agent_runtime,
                session_id,
                previous_phase,
                &status_message_id,
                &budget_note,
                AUTO_CONTEXT_COMPACTION_TRIGGER,
                execution,
            )?;
            spawn_context_compaction_persist_task(
                state.clone(),
                controller_runtime.clone(),
                snapshot,
                session_id,
                status_message_id.clone(),
                Some(String::from("auto_context_compaction")),
            );
            let mut refreshed =
                refreshed_execution_request_after_context_compaction(state, request)?;
            refreshed
                .embedded_turn_usage_segments
                .push(compaction_usage_segment);
            Ok(Some(refreshed))
        }
        Err(error) => {
            persist_failed_webui_context_compaction(
                app,
                state,
                controller_runtime,
                session_id,
                previous_phase,
                &status_message_id,
                &budget_note,
                AUTO_CONTEXT_COMPACTION_TRIGGER,
                &error,
            )?;
            Err(error)
        }
    }
}

/// Returns the current consecutive automatic compaction failure count for one session.
/// 返回单个会话当前连续自动压缩失败次数。
pub(crate) fn webui_auto_context_compaction_failure_count(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
) -> usize {
    current_workspace_snapshot_from_shared_state(state)
        .ok()
        .flatten()
        .and_then(|workspace| {
            workspace
                .session_runtime_state
                .get(&session_id)
                .map(|runtime_state| runtime_state.auto_compaction_consecutive_failures)
        })
        .unwrap_or_default()
}

/// Builds a compaction plan from a specific pre-status conversation snapshot.
/// 从指定的状态节点插入前会话快照构建压缩计划。
pub(crate) fn webui_context_compaction_plan_from_conversation_state(
    session_id: SessionId,
    conversation_state: &SessionConversationState,
    context_start_message_id: Option<&str>,
    empty_error: &str,
) -> Result<CompactionPlan, String> {
    let full_session_messages =
        conversation_state_to_compaction_source_messages(conversation_state);
    if full_session_messages.is_empty() {
        return Err(empty_error.to_string());
    }
    plan_context_compaction(CompactionPlanInput {
        session_label: &session_id.to_string(),
        messages: &full_session_messages,
        all_messages: &full_session_messages,
        context_start_message_id,
        source_until_message_id: None,
        policy: Default::default(),
    })
    .ok_or_else(|| empty_error.to_string())
}

/// Rebuilds an execution request after a successful compaction commit.
/// 在压缩提交成功后重建执行请求。
pub(crate) fn refreshed_execution_request_after_context_compaction(
    state: &SharedServiceBridgeState,
    request: &ExecutionPreparationRequest,
) -> Result<ExecutionPreparationRequest, String> {
    let workspace_snapshot =
        current_workspace_snapshot_from_shared_state(state)?.ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
    let runtime_state = workspace_snapshot
        .session_runtime_state
        .get(&request.session_id)
        .ok_or_else(|| String::from("自动上下文压缩完成后未找到会话运行时状态。"))?;
    let mut refreshed = request.clone();
    refreshed.conversation_state = runtime_state.conversation_state.clone();
    let turn_marker = refreshed
        .boundary_events
        .first()
        .and_then(|event| event.turn_id.clone());
    refreshed.context_injection = core_provider_context_injection_from_state_for_turn(
        &runtime_state.context_injection_state,
        turn_marker.as_deref(),
    );
    Ok(refreshed)
}

/// Persists and projects one failed automatic or manual compaction attempt.
/// 持久化并投影一次失败的自动或手动压缩尝试。
pub(crate) fn persist_failed_webui_context_compaction(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    session_id: SessionId,
    previous_phase: SessionRunPhase,
    section_id: &str,
    budget_note: &str,
    trigger: &str,
    error: &str,
) -> Result<(), String> {
    let snapshot = fail_webui_context_compaction_execution(
        app,
        state,
        session_id,
        previous_phase,
        section_id,
        budget_note,
        trigger,
        error,
    )?;
    spawn_workspace_session_persist_task(
        state.clone(),
        controller_runtime.clone(),
        snapshot,
        session_id,
        Some(String::from("context_compaction_failed")),
    );
    Ok(())
}

pub(crate) fn build_webui_context_compaction_job_request(
    state: &SharedServiceBridgeState,
    agent_runtime: &AgentRuntimeBridge,
    session_id: SessionId,
    plan: CompactionPlan,
    full_context_state: SessionConversationState,
) -> Result<PromptCompactionJobRequest, String> {
    let mcp_runtime_access = agent_runtime
        .mcp_runtime
        .clone()
        .ok_or_else(|| String::from("上下文压缩运行时未就绪，无法发起模型总结请求。"))?;
    let workspace_snapshot = current_workspace_snapshot_from_shared_state(state)
        .ok()
        .flatten();
    let workspace_path = workspace_snapshot
        .as_ref()
        .map(|workspace| workspace_path_for_session(workspace, session_id))
        .unwrap_or_else(|| normalized_workspace_path_text_from_path(&workspace_root()));
    let ai_tool_configuration = workspace_snapshot
        .as_ref()
        .map(|workspace| {
            session_ai_tool_configuration_or_default(
                workspace,
                session_id,
                &agent_runtime.ai_tool_configuration,
            )
            .clone()
        })
        .unwrap_or_else(|| agent_runtime.ai_tool_configuration.clone());
    let budget_snapshot = context_compaction_budget_snapshot_from_state(
        state,
        session_id,
        &agent_runtime.provider_configuration,
        &agent_runtime.ai_tool_configuration,
    );
    Ok(PromptCompactionJobRequest {
        session_id,
        session_title: current_session_title_from_state(state, session_id),
        workspace_path,
        plan,
        full_context_state,
        input_budget: context_compaction_input_budget_from_snapshot(budget_snapshot.as_ref()),
        observed_input_tokens: context_compaction_observed_input_tokens_from_snapshot(
            budget_snapshot.as_ref(),
        ),
        estimated_input_tokens: context_compaction_estimated_input_tokens_from_snapshot(
            budget_snapshot.as_ref(),
        ),
        provider_configuration: agent_runtime.provider_configuration.clone(),
        model_label: agent_runtime.provider_configuration.model.clone(),
        reasoning_effort: active_reasoning_selection_effort(&agent_runtime.provider_configuration)
            .unwrap_or(ReasoningEffortLevel::Medium),
        ai_tool_configuration,
        agent_profile_catalog: agent_runtime.agent_profile_catalog.clone(),
        luaskills_runtime: agent_runtime.luaskills_runtime.clone(),
        mcp_runtime_access,
        task_runtime: Arc::clone(&agent_runtime.task_runtime),
        question_runtime: Arc::clone(&agent_runtime.question_runtime),
        command_session_runtime: Arc::clone(&agent_runtime.command_session_runtime),
        token_budget_runtime: Arc::clone(&agent_runtime.token_budget_runtime),
    })
}
