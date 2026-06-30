//! Streaming transcript and runtime event projection helpers.
//! 流式转录与运行时事件投影辅助函数。

use super::*;
use vulcan_service_api::ConversationMessagesPatchedDelta;

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
        guard.workbench.revision = generation;
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

/// Emits one already-materialized visible session message through the lightweight workbench delta channel.
/// 通过轻量工作台增量通道发送一条已经物化的可见会话消息。
///
/// # Parameters
/// 参数
/// `app` emits the delta event to connected WebUI clients.
/// `app` 将增量事件发送给已连接的 WebUI 客户端。
/// `state` owns the retained workbench projection that must stay aligned with emitted deltas.
/// `state` 持有需要与增量事件保持一致的保留工作台投影。
/// `session_id` selects the conversation that owns the message.
/// `session_id` 选择拥有该消息的会话。
/// `message` is the already-projected session transcript entry to expose immediately.
/// `message` 是需要立即展示的已投影会话转录项。
/// `run_phase` is the current session run phase after the message was accepted.
/// `run_phase` 是消息被接受后的当前会话运行阶段。
///
/// # Returns
/// 返回值
/// `Ok(())` when the delta was emitted or skipped because the target surface is not visible.
/// 当增量已发送，或因目标界面不可见而跳过时返回 `Ok(())`。
pub(crate) fn emit_materialized_session_message_delta(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    message: &SessionMessageEntry,
    run_phase: SessionRunPhase,
) -> Result<(), String> {
    if !message.is_visible_to_user() {
        return Ok(());
    }
    let conversation_id = session_id.to_string();
    let projected_message = conversation_message_from_session_entry(&conversation_id, message);
    let event = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        if !workbench_surface_matches_conversation(&guard.workbench, &conversation_id) {
            return Ok(());
        }
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        let generation = guard.workbench_generation;
        guard.workbench.revision = generation;
        if let Some(existing) = guard
            .workbench
            .messages
            .iter_mut()
            .find(|candidate| candidate.id == projected_message.id)
        {
            *existing = projected_message.clone();
        } else {
            guard.workbench.messages.push(projected_message.clone());
        }
        for conversation in &mut guard.workbench.conversations {
            if conversation.id == conversation_id {
                conversation.run_phase = run_phase.to_string();
                conversation.status = session_state_for_run_phase(run_phase).label().to_string();
                conversation.last_updated_label = String::from("刚刚");
                conversation.last_updated_at = message.created_at.clone();
                break;
            }
        }
        WorkbenchDeltaEnvelope {
            generation,
            delta: WorkbenchDelta::AppendMessageBody(AppendMessageBodyDelta {
                conversation_id,
                message_id: projected_message.id.clone(),
                body_delta: String::new(),
                current_body: Some(projected_message.body.clone()),
                initial_message: Some(projected_message),
                run_phase: Some(run_phase.to_string()),
            }),
        }
    };
    webui_submit_trace(
        "submit_prompt_user_message_delta_emitted",
        format!("session={session_id} message_id={}", message.id),
    );
    emit_service_event_with_trace(
        app,
        "workbench_user_message_delta_emit",
        ServiceEvent::WorkbenchDelta(Box::new(event)),
    );
    Ok(())
}

/// Commits one workspace snapshot and emits a phase-only conversation delta.
/// 提交单个工作区快照，并发出纯会话阶段增量。
///
/// # Parameters
/// 参数
/// `app` emits the delta event to connected WebUI clients.
/// `app` 将增量事件发送给已连接的 WebUI 客户端。
/// `state` owns the retained workbench and workspace snapshots.
/// `state` 持有保留的工作台与工作区快照。
/// `workspace_snapshot` is the authoritative workspace state after the phase change.
/// `workspace_snapshot` 是阶段变化后的权威工作区状态。
/// `session_id` selects the conversation that owns the phase.
/// `session_id` 选择拥有该阶段的会话。
/// `run_phase` is the backend lifecycle phase to expose.
/// `run_phase` 是需要暴露的后端生命周期阶段。
///
/// # Returns
/// 返回值
/// `Ok(())` when state was committed and the delta was emitted.
/// 当状态已提交且增量已发送时返回 `Ok(())`。
pub(crate) fn commit_workspace_snapshot_and_emit_session_phase_delta(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    workspace_snapshot: WorkspaceSnapshot,
    session_id: SessionId,
    run_phase: SessionRunPhase,
) -> Result<(), String> {
    let conversation_id = session_id.to_string();
    let session_state = session_state_for_run_phase(run_phase);
    let event = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        let generation = guard.workbench_generation;
        guard.workbench.revision = generation;
        guard.workspace_snapshot = Some(workspace_snapshot);
        for conversation in &mut guard.workbench.conversations {
            if conversation.id == conversation_id {
                conversation.run_phase = run_phase.to_string();
                conversation.status = session_state.label().to_string();
                conversation.last_updated_label = String::from("刚刚");
                conversation.last_updated_at = current_unix_millis_text();
                break;
            }
        }
        WorkbenchDeltaEnvelope {
            generation,
            delta: WorkbenchDelta::UpdateConversationRunPhase(UpdateConversationRunPhaseDelta {
                conversation_id,
                run_phase: run_phase.to_string(),
                status: Some(session_state.label().to_string()),
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

/// Maps one execution phase to the compact sidebar session status.
/// 将单个执行阶段映射为侧栏使用的紧凑会话状态。
fn session_state_for_run_phase(run_phase: SessionRunPhase) -> SessionState {
    match run_phase {
        SessionRunPhase::Failed | SessionRunPhase::Stopped => SessionState::Blocked,
        SessionRunPhase::Idle | SessionRunPhase::Ready => SessionState::Completed,
        SessionRunPhase::Preparing
        | SessionRunPhase::MemoryRecall
        | SessionRunPhase::Requesting
        | SessionRunPhase::Thinking
        | SessionRunPhase::ToolRunning => SessionState::Running,
    }
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

/// Builds a node-level message patch by comparing visible messages before and after projection.
/// 通过比较投影前后的可见消息构建节点级消息补丁。
pub(crate) fn build_conversation_messages_patched_delta(
    conversation_id: &str,
    before_messages: &[ConversationMessage],
    after_messages: &[ConversationMessage],
) -> Option<ConversationMessagesPatchedDelta> {
    let mut remove_message_ids = Vec::new();
    for before_message in before_messages {
        if !after_messages
            .iter()
            .any(|after_message| after_message.id == before_message.id)
        {
            remove_message_ids.push(before_message.id.clone());
        }
    }

    let mut upsert_messages = Vec::new();
    for after_message in after_messages {
        match before_messages
            .iter()
            .find(|before_message| before_message.id == after_message.id)
        {
            Some(before_message) if before_message == after_message => {}
            _ => upsert_messages.push(after_message.clone()),
        }
    }

    let before_order: Vec<&str> = before_messages
        .iter()
        .map(|message| message.id.as_str())
        .collect();
    let after_order: Vec<&str> = after_messages
        .iter()
        .map(|message| message.id.as_str())
        .collect();
    let message_order = if before_order == after_order {
        None
    } else {
        Some(
            after_messages
                .iter()
                .map(|message| message.id.clone())
                .collect(),
        )
    };

    if upsert_messages.is_empty() && remove_message_ids.is_empty() && message_order.is_none() {
        return None;
    }

    Some(ConversationMessagesPatchedDelta {
        conversation_id: conversation_id.to_string(),
        upsert_messages,
        remove_message_ids,
        message_order,
    })
}

/// Replaces one conversation's visible message block while preserving unrelated message positions.
/// 替换单个会话的可见消息块，同时保留无关消息的位置。
pub(crate) fn replace_conversation_messages_in_workbench_window(
    messages: &mut Vec<ConversationMessage>,
    conversation_id: &str,
    next_conversation_messages: Vec<ConversationMessage>,
) {
    let previous_messages = std::mem::take(messages);
    let mut pending_conversation_messages = Some(next_conversation_messages);
    let mut inserted_conversation_messages = false;
    for message in previous_messages {
        if message.conversation_id != conversation_id {
            messages.push(message);
            continue;
        }
        if !inserted_conversation_messages {
            if let Some(next_messages) = pending_conversation_messages.take() {
                messages.extend(next_messages);
            }
            inserted_conversation_messages = true;
        }
    }
    if !inserted_conversation_messages {
        if let Some(next_messages) = pending_conversation_messages.take() {
            messages.extend(next_messages);
        }
    }
}

/// Keeps the thinking row collapsed after its streaming phase completes.
/// 在思考流阶段完成后保持思考行折叠。
pub(crate) fn collapse_streaming_thinking(
    _app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    record_ids: &SessionStreamingRecordIds,
) -> Result<(), String> {
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let workspace = guard.workspace_snapshot.as_mut().ok_or_else(|| {
        String::from("service is still starting; workspace snapshot is not ready")
    })?;
    let now = current_unix_millis_text();
    let runtime_state = workspace
        .session_runtime_state
        .entry(session_id)
        .or_insert_with(|| SessionRuntimeStateEntry {
            updated_at: now.clone(),
            ..SessionRuntimeStateEntry::default()
        });
    collapse_streaming_thinking_in_state(runtime_state, &now, record_ids);
    Ok(())
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
    let mut changed_todolist: Option<SessionTodolistState> = None;
    let mut changed_plan_mode: Option<SessionPlanModeState> = None;
    let mut changed_addon_session_id: Option<SessionId> = None;
    let mut transcript_changed = false;
    let (workspace_snapshot, message_window_delta) = {
        let conversation_id = session_id.to_string();
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        if let Some(goal_update) = goal_summary_from_tool_finished_event(event) {
            match goal_update {
                Some(summary) => {
                    if let Ok(summary_session_id) = summary.session_id.parse::<SessionId>() {
                        guard
                            .session_goal_summaries
                            .insert(summary_session_id, summary);
                        changed_addon_session_id = Some(summary_session_id);
                    }
                }
                None => {
                    guard.session_goal_summaries.remove(&session_id);
                    changed_addon_session_id = Some(session_id);
                }
            }
        }
        let before_visible_messages: Vec<ConversationMessage> = guard
            .workbench
            .messages
            .iter()
            .filter(|message| message.conversation_id == conversation_id)
            .cloned()
            .collect();
        let mut projected_messages = Vec::new();
        let workspace_snapshot = {
            let workspace = guard.workspace_snapshot.as_mut().ok_or_else(|| {
                String::from("service is still starting; workspace snapshot is not ready")
            })?;
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
                    let merged_messages = merge_session_messages_with_conversation_state_records(
                        workspace
                            .session_messages
                            .get(&session_id)
                            .map(Vec::as_slice)
                            .unwrap_or(&[]),
                        &runtime_state.conversation_state,
                    );
                    // Runtime tool projection must use the same visible-message filter as final hydration.
                    // 运行时工具投影必须使用与最终水合一致的可见消息过滤规则。
                    projected_messages = conversation_messages_from_session(
                        &conversation_id,
                        &merged_messages,
                        false,
                    );
                    workspace
                        .session_messages
                        .insert(session_id, merged_messages);
                    marker_sync_required = true;
                    transcript_changed = true;
                }
                if let Some(todolist) = todolist_state_from_tool_finished_event(event, &now)
                    && runtime_state.todolist != todolist
                {
                    runtime_state.todolist = todolist.clone();
                    runtime_state.updated_at = now.clone();
                    changed_todolist = Some(todolist);
                    changed_addon_session_id = Some(session_id);
                }
                if let Some(artifact) = plan_artifact_state_from_tool_finished_event(event) {
                    let mut plan_mode = runtime_state.plan_mode.clone();
                    plan_mode.artifact = Some(artifact);
                    if runtime_state.plan_mode != plan_mode {
                        runtime_state.plan_mode = plan_mode.clone();
                        runtime_state.updated_at = now.clone();
                        changed_plan_mode = Some(plan_mode);
                        changed_addon_session_id = Some(session_id);
                    }
                }
            }
            if marker_sync_required {
                sync_hydrated_session_dialogue_ready_marker(workspace, session_id);
            }
            workspace.clone()
        };
        let surface_matches =
            workbench_surface_matches_conversation(&guard.workbench, &conversation_id);
        let message_window_delta = if surface_matches && transcript_changed {
            let message_patch = build_conversation_messages_patched_delta(
                &conversation_id,
                before_visible_messages.as_slice(),
                projected_messages.as_slice(),
            );
            if let Some(message_patch) = message_patch {
                guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
                let generation = guard.workbench_generation;
                guard.workbench.revision = generation;
                replace_conversation_messages_in_workbench_window(
                    &mut guard.workbench.messages,
                    &conversation_id,
                    projected_messages.clone(),
                );
                Some(WorkbenchDeltaEnvelope {
                    generation,
                    delta: WorkbenchDelta::ConversationMessagesPatched(message_patch),
                })
            } else {
                None
            }
        } else {
            None
        };
        (workspace_snapshot, message_window_delta)
    };
    if let Some(event) = message_window_delta {
        emit_service_event_with_trace(
            app,
            "workbench_runtime_event_messages_delta_emit",
            ServiceEvent::WorkbenchDelta(Box::new(event)),
        );
    }
    if let Some(addon_session_id) = changed_addon_session_id {
        emit_session_addons_changed_delta(app, state, addon_session_id)?;
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies runtime tool patches use the same visible transcript projection as final hydration.
    /// 验证运行时工具补丁使用与最终水合一致的可见转录投影。
    #[test]
    fn runtime_tool_projection_filters_hidden_tool_detail_records() {
        let session_id = SessionId::new_v4();
        let conversation_id = session_id.to_string();
        let mut runtime_state = SessionRuntimeStateEntry::default();
        let started = RuntimeEvent::new(
            RuntimeEventKind::ToolStarted,
            "工具调用 · call-shell · shell_command",
            "## Tool Call",
        )
        .with_metadata(Some(json!({
            "tool_call_id": "call-shell",
            "tool_name": "shell_command",
            "arguments": {
                "command": "git diff --stat"
            }
        })));
        assert!(append_runtime_event_projection_to_state(
            &mut runtime_state,
            session_id,
            "1000",
            &started,
        ));
        let finished = RuntimeEvent::new(
            RuntimeEventKind::ToolFinished,
            "工具返回 · call-shell",
            "## Result ID\n\n`tool-result-call-shell`\n\n## Tool Result\n\nCommand: rtk git diff --stat\n\n[stdout]\nfiles changed",
        )
        .with_metadata(Some(json!({
            "tool_call_id": "call-shell",
            "tool_result_id": "tool-result-call-shell",
            "tool_name": "shell_command",
            "display_state": "completed",
            "host_result": {
                "kind": "command_run",
                "payload": {
                    "command": "rtk git diff --stat",
                    "status": "completed"
                }
            }
        })));
        assert!(append_runtime_event_projection_to_state(
            &mut runtime_state,
            session_id,
            "1001",
            &finished,
        ));

        let merged_messages = merge_session_messages_with_conversation_state_records(
            &[],
            &runtime_state.conversation_state,
        );
        assert!(
            merged_messages
                .iter()
                .any(|message| message.record_kind == SessionMessageRecordKind::ToolDetailOutput)
        );

        let projected_messages =
            conversation_messages_from_session(&conversation_id, &merged_messages, false);
        assert_eq!(projected_messages.len(), 1);
        let tool_message = projected_messages
            .first()
            .expect("tool call output should remain visible");
        assert_eq!(
            tool_message.record_kind.as_deref(),
            Some("tool-call-output")
        );
        assert!(tool_message.body.is_empty());
        assert!(
            !projected_messages
                .iter()
                .any(|message| message.record_kind.as_deref() == Some("tool-detail-output"))
        );
        assert_eq!(
            tool_message
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("result"))
                .and_then(|result| result.get("detail_ref"))
                .and_then(|detail_ref| detail_ref.get("message_index"))
                .and_then(serde_json::Value::as_u64),
            Some(1)
        );
    }
}
