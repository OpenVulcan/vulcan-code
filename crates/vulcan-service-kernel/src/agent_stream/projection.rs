//! Streaming transcript and runtime event projection helpers.
//! 流式转录与运行时事件投影辅助函数。

use super::*;

/// Appends or extends one streaming transcript record.
/// 追加或扩展单条流式转录记录。
pub(crate) fn append_stream_delta_record(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    record_id_slot: &mut Option<String>,
    record_kind: SessionMessageRecordKind,
    title: &str,
    delta: &str,
    collapsed_by_default: bool,
) -> Result<(), String> {
    if delta.is_empty() {
        return Ok(());
    }
    let conversation_id = session_id.to_string();
    let event = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let workspace = guard.workspace_snapshot.as_mut().ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
        let now = current_unix_millis_text();
        let is_new_message = record_id_slot.is_none();
        let run_phase = match record_kind {
            SessionMessageRecordKind::ThinkingOutput => SessionRunPhase::Thinking,
            _ => SessionRunPhase::Requesting,
        };
        let projected_entry = {
            let runtime_state = workspace
                .session_runtime_state
                .entry(session_id)
                .or_insert_with(|| SessionRuntimeStateEntry {
                    updated_at: now.clone(),
                    ..SessionRuntimeStateEntry::default()
                });
            append_stream_delta_record_to_state(
                runtime_state,
                session_id,
                &now,
                record_id_slot,
                record_kind,
                title,
                delta,
                collapsed_by_default,
            );
            let Some(message_id) = record_id_slot.as_deref() else {
                return Err(String::from("streaming record id was not materialized"));
            };
            let projected_entry = runtime_state
                .conversation_state
                .records
                .iter()
                .find(|record| record.id == message_id)
                .map(session_message_entry_from_conversation_record);
            runtime_state.updated_at = now.clone();
            projected_entry
        };
        if let Some(projected_entry) = projected_entry.as_ref()
            && projected_entry.is_visible_to_user()
            && !projected_entry.body.trim().is_empty()
        {
            let messages = workspace.session_messages.entry(session_id).or_default();
            if let Some(existing) = messages
                .iter_mut()
                .find(|message| message.id == projected_entry.id)
            {
                *existing = projected_entry.clone();
            } else {
                messages.push(projected_entry.clone());
            }
        }
        workspace.session_run_phases.insert(session_id, run_phase);
        sync_hydrated_session_dialogue_ready_marker(workspace, session_id);
        let Some(projected_entry) = projected_entry else {
            return Ok(());
        };
        let projected_message =
            conversation_message_from_session_entry(&conversation_id, &projected_entry);
        let initial_message = is_new_message.then(|| projected_message.clone());
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        let generation = guard.workbench_generation;
        if let Some(existing) = guard
            .workbench
            .messages
            .iter_mut()
            .find(|message| message.id == projected_message.id)
        {
            *existing = projected_message.clone();
        } else if workbench_surface_matches_conversation(&guard.workbench, &conversation_id) {
            guard.workbench.messages.push(projected_message.clone());
        }
        for conversation in &mut guard.workbench.conversations {
            if conversation.id == conversation_id {
                conversation.run_phase = run_phase.to_string();
                conversation.status = SessionState::Running.label().to_string();
                conversation.last_updated_label = String::from("刚刚");
                conversation.last_updated_at = now.clone();
                break;
            }
        }
        WorkbenchDeltaEnvelope {
            generation,
            delta: WorkbenchDelta::AppendMessageBody(AppendMessageBodyDelta {
                conversation_id,
                message_id: projected_entry.id,
                body_delta: delta.to_string(),
                current_body: Some(projected_message.body.clone()),
                initial_message,
                run_phase: Some(run_phase.to_string()),
            }),
        }
    };
    emit_service_event_with_trace(
        app,
        "workbench_delta_emit",
        ServiceEvent::WorkbenchDelta(Box::new(event)),
    );
    Ok(())
}

/// Returns whether the retained workbench currently displays one conversation surface.
/// 返回保留工作台当前是否正在展示指定会话界面。
pub(crate) fn workbench_surface_matches_conversation(
    workbench: &WorkbenchSnapshot,
    conversation_id: &str,
) -> bool {
    workbench.selected_conversation_id.as_deref() == Some(conversation_id)
        || workbench
            .subagent_navigation
            .as_ref()
            .is_some_and(|navigation| navigation.active_conversation_id == conversation_id)
}

/// Converts one canonical conversation record into the transcript entry projection.
/// 将单条规范会话记录转换为转录项投影。
pub(crate) fn session_message_entry_from_conversation_record(
    record: &SessionConversationRecord,
) -> SessionMessageEntry {
    SessionMessageEntry {
        id: record.id.clone(),
        kind: record.kind,
        record_kind: record.record_kind,
        visible: record.visible,
        title: record.title.clone(),
        body: record.body.clone(),
        metadata: record.metadata.clone(),
        created_at: record.created_at.clone(),
    }
}

/// Converts one session transcript entry into a WebUI conversation message.
/// 将单条会话转录项转换为 WebUI 会话消息。
pub(crate) fn conversation_message_from_session_entry(
    conversation_id: &str,
    message: &SessionMessageEntry,
) -> ConversationMessage {
    ConversationMessage {
        id: if message.id.is_empty() {
            format!("{}-{}", conversation_id, message.created_at)
        } else {
            message.id.clone()
        },
        conversation_id: conversation_id.to_string(),
        created_at: message.created_at.clone(),
        role: match message.kind {
            SessionMessageKind::User => MessageRole::User,
            SessionMessageKind::Assistant => MessageRole::Assistant,
            SessionMessageKind::Error => MessageRole::System,
        },
        title: non_empty_string(&message.title),
        record_kind: Some(message.record_kind.as_persisted_key().to_string()),
        body: message.body.clone(),
        metadata: message.metadata.clone(),
        attachments: composer_message_attachments_from_metadata(message.metadata.as_ref()),
        editable_user_prompt: false,
    }
}

/// Keeps the thinking row collapsed after its streaming phase completes.
/// 在思考流阶段完成后保持思考行折叠。
pub(crate) fn collapse_streaming_thinking(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    record_ids: &SessionStreamingRecordIds,
) -> Result<(), String> {
    with_workspace_mutation_preserving_subagent_navigation(app, state, |workspace| {
        let now = current_unix_millis_text();
        let runtime_state = workspace
            .session_runtime_state
            .entry(session_id)
            .or_insert_with(|| SessionRuntimeStateEntry {
                updated_at: now.clone(),
                ..SessionRuntimeStateEntry::default()
            });
        collapse_streaming_thinking_in_state(runtime_state, &now, record_ids);
    })
    .map(|_| ())
}

/// Projects runtime events that have user-facing transcript cards into the retained session state.
/// 将具备用户可见转录卡片的运行时事件投影到保留的会话状态中。
pub(crate) fn append_runtime_event_projection(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    event: &RuntimeEvent,
) -> Result<(), String> {
    if !matches!(
        event.kind,
        RuntimeEventKind::ToolStarted | RuntimeEventKind::ToolFinished
    ) {
        return Ok(());
    }
    if let Some(goal_update) = goal_summary_from_tool_finished_event(event) {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        match goal_update {
            Some(summary) => {
                if let Ok(summary_session_id) = summary.session_id.parse::<SessionId>() {
                    guard
                        .session_goal_summaries
                        .insert(summary_session_id, summary);
                }
            }
            None => {
                guard.session_goal_summaries.remove(&session_id);
            }
        }
    }
    let mut changed_todolist: Option<SessionTodolistState> = None;
    let mut changed_plan_mode: Option<SessionPlanModeState> = None;
    let mut transcript_changed = false;
    let workspace_snapshot =
        with_workspace_mutation_preserving_subagent_navigation(app, state, |workspace| {
            let now = current_unix_millis_text();
            // marker_sync_required defers workspace-level marker updates until the runtime-state borrow ends.
            // marker_sync_required 将工作区级标记更新延后到运行态借用结束之后。
            let mut marker_sync_required = false;
            {
                let runtime_state = workspace
                    .session_runtime_state
                    .entry(session_id)
                    .or_insert_with(|| SessionRuntimeStateEntry {
                        updated_at: now.clone(),
                        ..SessionRuntimeStateEntry::default()
                    });
                if append_runtime_event_projection_to_state(runtime_state, session_id, &now, event)
                {
                    workspace.session_messages.insert(
                        session_id,
                        merge_session_messages_with_conversation_state_records(
                            workspace
                                .session_messages
                                .get(&session_id)
                                .map(Vec::as_slice)
                                .unwrap_or(&[]),
                            &runtime_state.conversation_state,
                        ),
                    );
                    marker_sync_required = true;
                    transcript_changed = true;
                }
                if let Some(todolist) = todolist_state_from_tool_finished_event(event, &now)
                    && runtime_state.todolist != todolist
                {
                    runtime_state.todolist = todolist.clone();
                    runtime_state.updated_at = now.clone();
                    changed_todolist = Some(todolist);
                }
                if let Some(artifact) = plan_artifact_state_from_tool_finished_event(event) {
                    let mut plan_mode = runtime_state.plan_mode.clone();
                    plan_mode.artifact = Some(artifact);
                    if runtime_state.plan_mode != plan_mode {
                        runtime_state.plan_mode = plan_mode.clone();
                        runtime_state.updated_at = now.clone();
                        changed_plan_mode = Some(plan_mode);
                    }
                }
            }
            if marker_sync_required {
                sync_hydrated_session_dialogue_ready_marker(workspace, session_id);
            }
        })?;
    if transcript_changed {
        let controller_runtime = controller_runtime_from_state(state)?;
        spawn_workspace_session_transcript_persist_task(
            state.clone(),
            controller_runtime,
            workspace_snapshot.clone(),
            session_id,
            Some(String::from("runtime_event_projection")),
        );
    }
    if let Some(todolist) = changed_todolist {
        let controller_runtime = controller_runtime_from_state(state)?;
        let updated_at = todolist
            .updated_at
            .clone()
            .unwrap_or_else(current_unix_millis_text);
        let mut addon_state = SessionAddonState::new(session_id);
        let _ = addon_state.set_todolist(&todolist, updated_at);
        spawn_workspace_session_addon_state_persist_task(
            app.clone(),
            state.clone(),
            controller_runtime,
            workspace_snapshot.clone(),
            session_id,
            addon_state,
            vec![SessionAddonNamespace::Todolist],
            Some(String::from("todolist_tool")),
        );
    }
    if let Some(plan_mode) = changed_plan_mode {
        let controller_runtime = controller_runtime_from_state(state)?;
        let updated_at = plan_mode
            .artifact
            .as_ref()
            .map(|artifact| artifact.updated_at.clone())
            .unwrap_or_else(current_unix_millis_text);
        let mut addon_state = SessionAddonState::new(session_id);
        let _ = addon_state.set_work_mode(
            &SessionWorkMode {
                execution_mode: String::from("plan"),
                plan_mode,
            },
            updated_at,
        );
        spawn_workspace_session_addon_state_persist_task(
            app.clone(),
            state.clone(),
            controller_runtime,
            workspace_snapshot,
            session_id,
            addon_state,
            vec![SessionAddonNamespace::WorkMode],
            Some(String::from("submit_plan_tool")),
        );
    }
    Ok(())
}

/// Caches the latest provider usage into the live context-budget snapshot.
/// 将最近一次 Provider 用量缓存进实时上下文预算快照。
pub(crate) fn cache_live_context_budget_observed_usage(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    usage: &ProviderUsageSnapshot,
) -> Result<(), String> {
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let Some(context_budget) = guard.live_context_budgets.get_mut(&session_id) else {
        return Ok(());
    };
    apply_observed_usage_to_context_budget(context_budget, usage);
    Ok(())
}

/// Applies one observed provider usage sample to an existing budget snapshot.
/// 将一份 Provider 实测用量应用到既有预算快照。
pub(crate) fn apply_observed_usage_to_context_budget(
    context_budget: &mut ContextBudgetSnapshot,
    usage: &ProviderUsageSnapshot,
) {
    let Some(input_tokens) =
        vulcan_context::observed_input_tokens_from_usage(Some(usage)).or(usage.total_tokens)
    else {
        context_budget.observed_usage = Some(usage.clone());
        return;
    };
    context_budget.estimated_input_tokens = input_tokens;
    context_budget.estimated_total_tokens = usage.total_tokens.unwrap_or_else(|| {
        input_tokens
            .saturating_add(usage.output_tokens.unwrap_or_default())
            .saturating_add(usage.reasoning_tokens.unwrap_or_default())
    });
    context_budget.usage_ratio = context_budget
        .effective_input_limit
        .map(|limit| input_tokens as f32 / limit.max(1) as f32)
        .unwrap_or_else(|| if input_tokens > 0 { 0.01 } else { 0.0 });
    context_budget.severity = historical_context_budget_severity(context_budget.usage_ratio);
    context_budget.observed_usage = Some(usage.clone());
}
