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

/// Emits the selected-session context injection side-panel projection as its own delta.
/// 将当前选中会话的上下文注入侧面板投影作为独立增量发出。
///
/// # Parameters
/// 参数
/// `app` emits the delta event to connected WebUI clients.
/// `app` 将增量事件发送给已连接的 WebUI 客户端。
/// `state` owns the retained workspace and workbench projections.
/// `state` 持有保留的工作区与工作台投影。
/// `session_id` selects the session whose memory-panel projection changed.
/// `session_id` 选择记忆面板投影发生变化的会话。
///
/// # Returns
/// 返回值
/// `Ok(())` when the selected-session projection was emitted or skipped for a background session.
/// 当前选中会话投影已发出，或后台会话被跳过时返回 `Ok(())`。
pub(crate) fn emit_context_injection_changed_delta(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
) -> Result<(), String> {
    let event = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let Some(workspace_snapshot) = guard.workspace_snapshot.as_ref() else {
            return Ok(());
        };
        let Some(delta) =
            context_injection_changed_delta_from_workspace_snapshot(workspace_snapshot, session_id)
        else {
            return Ok(());
        };
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        let generation = guard.workbench_generation;
        guard.workbench.revision = generation;
        guard.workbench.context_injection = delta.context_injection.clone();
        replace_conversation_scoped_projection_entry(
            &mut guard.workbench.context_injection_by_conversation,
            delta.conversation_id.as_str(),
            delta.context_injection.is_some().then(|| delta.clone()),
            |entry| entry.conversation_id.as_str(),
        );
        WorkbenchDeltaEnvelope {
            generation,
            delta: WorkbenchDelta::ContextInjectionChanged(delta),
        }
    };
    emit_service_event_with_trace(
        app,
        "workbench_context_injection_delta_emit",
        ServiceEvent::WorkbenchDelta(Box::new(event)),
    );
    Ok(())
}

/// Builds a memory-panel delta from the selected session runtime projection.
/// 从当前选中会话运行态投影构建记忆面板增量。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` provides the retained session runtime state.
/// `workspace_snapshot` 提供保留的会话运行态。
/// `session_id` selects the session whose projection may have changed.
/// `session_id` 选择可能发生投影变化的会话。
///
/// # Returns
/// 返回值
/// A context-injection delta when `session_id` is the selected session.
/// 当 `session_id` 是当前选中会话时返回上下文注入增量。
fn context_injection_changed_delta_from_workspace_snapshot(
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
) -> Option<ContextInjectionChangedDelta> {
    if workspace_snapshot.selected_session != session_id {
        return None;
    }
    Some(context_injection_changed_delta_from_runtime_state(
        session_id,
        workspace_snapshot.session_runtime_state.get(&session_id),
    ))
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
        run_phase: None,
    })
}

/// Builds a runtime-event message patch with the matching UI phase applied atomically.
/// 构建运行时事件消息补丁，并将匹配的界面阶段原子写入同一补丁。
fn build_runtime_event_conversation_messages_patched_delta(
    conversation_id: &str,
    before_messages: &[ConversationMessage],
    after_messages: &[ConversationMessage],
    event: &RuntimeEvent,
) -> Option<ConversationMessagesPatchedDelta> {
    let mut message_patch = build_conversation_messages_patched_delta(
        conversation_id,
        before_messages,
        after_messages,
    )?;
    message_patch.run_phase =
        runtime_event_projection_run_phase(event).map(|phase| phase.to_string());
    Some(message_patch)
}

/// Keeps visible streaming rows ahead of a newly projected runtime tool event.
/// 在新投影的运行时工具事件之前保留可见的流式记录。
///
/// `before_messages` is the exact visible order before the runtime event, while `projected_messages`
/// contains stable retained rows plus the newly projected tool row. The result replaces matching stable
/// rows in place, retains only transient thinking/assistant rows omitted from retained storage, and appends
/// genuinely new runtime rows after everything that was already visible.
///
/// `before_messages` 是运行时事件前的精确可见顺序，`projected_messages` 包含稳定保留记录与新投影工具记录。
/// 返回结果会原位替换匹配的稳定记录，仅保留未进入保留存储的临时思考/Assistant 记录，并将真正新增的运行时记录
/// 追加到此前已经可见的全部内容之后。
fn merge_visible_streaming_rows_with_runtime_projection(
    before_messages: &[ConversationMessage],
    projected_messages: &[ConversationMessage],
) -> Vec<ConversationMessage> {
    // projected_by_id provides authoritative stable replacements without changing prior visible positions.
    // projected_by_id 提供权威稳定替换项，同时不改变此前的可见位置。
    let projected_by_id = projected_messages
        .iter()
        .map(|message| (message.id.as_str(), message))
        .collect::<BTreeMap<_, _>>();
    // emitted_ids prevents stable rows already replaced in place from being appended again.
    // emitted_ids 防止已经原位替换的稳定记录被再次追加。
    let mut emitted_ids = BTreeSet::new();
    // merged_messages stores the exact visible order for the runtime-event patch.
    // merged_messages 保存运行时事件补丁使用的精确可见顺序。
    let mut merged_messages = Vec::with_capacity(
        before_messages
            .len()
            .saturating_add(projected_messages.len()),
    );
    for before_message in before_messages {
        if let Some(projected_message) = projected_by_id.get(before_message.id.as_str()) {
            merged_messages.push((*projected_message).clone());
            emitted_ids.insert(before_message.id.as_str());
            continue;
        }
        if runtime_projection_retains_visible_streaming_row(before_message) {
            merged_messages.push(before_message.clone());
            emitted_ids.insert(before_message.id.as_str());
        }
    }
    for projected_message in projected_messages {
        if emitted_ids.insert(projected_message.id.as_str()) {
            merged_messages.push(projected_message.clone());
        }
    }
    merged_messages
}

/// Returns whether one visible row is a transient stream that stable tool projection intentionally omits.
/// 返回单条可见记录是否为稳定工具投影有意省略的临时流记录。
fn runtime_projection_retains_visible_streaming_row(message: &ConversationMessage) -> bool {
    matches!(
        message.record_kind.as_deref(),
        Some(kind)
            if kind == SessionMessageRecordKind::ThinkingOutput.as_persisted_key()
                || kind == SessionMessageRecordKind::AssistantOutput.as_persisted_key()
    )
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
    if !inserted_conversation_messages
        && let Some(next_messages) = pending_conversation_messages.take()
    {
        messages.extend(next_messages);
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
    let mut subagent_navigation_changed = false;
    let mut transcript_changed = false;
    let (workspace_snapshot, message_window_deltas) = {
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
        let mut child_message_projection: Option<(
            SessionId,
            Vec<SessionMessageEntry>,
            Vec<ConversationMessage>,
        )> = None;
        let mut child_completion_gate_ignored_task_id: Option<SessionId> = None;
        if let Some((child_task_id, execution_id)) =
            terminal_subagent_send_message_resume_from_event(event)?
        {
            consume_webui_subagent_mailbox_input_for_execution(
                &mut guard,
                session_id,
                child_task_id,
                execution_id.as_str(),
            );
        }
        for (child_task_id, execution_id) in
            terminal_subagent_wait_consumed_executions_from_event(event)?
        {
            consume_webui_subagent_mailbox_input_for_execution(
                &mut guard,
                session_id,
                child_task_id,
                execution_id.as_str(),
            );
        }
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
                let before_projection_records = runtime_state.conversation_state.records.clone();
                if append_runtime_event_projection_to_state(runtime_state, session_id, &now, event)
                {
                    let projection_entries = runtime_event_projection_entries_from_changed_records(
                        before_projection_records.as_slice(),
                        runtime_state.conversation_state.records.as_slice(),
                    );
                    let merged_messages = merge_session_messages_with_runtime_event_entries(
                        workspace
                            .session_messages
                            .get(&session_id)
                            .map(Vec::as_slice)
                            .unwrap_or(&[]),
                        projection_entries.as_slice(),
                    );
                    if !projection_entries.is_empty() {
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
            if let Some(child_task_id) =
                append_webui_subagent_send_message_from_event_if_needed(workspace, event)?
            {
                child_completion_gate_ignored_task_id = Some(child_task_id);
                let child_conversation_id = child_task_id.to_string();
                let child_messages = workspace
                    .session_messages
                    .get(&child_task_id)
                    .cloned()
                    .unwrap_or_default();
                let child_projected_messages = conversation_messages_from_session(
                    &child_conversation_id,
                    child_messages.as_slice(),
                    false,
                );
                child_message_projection =
                    Some((child_task_id, child_messages, child_projected_messages));
            }
            if marker_sync_required {
                sync_hydrated_session_dialogue_ready_marker(workspace, session_id);
            }
            workspace.clone()
        };
        if let Some(child_task_id) = child_completion_gate_ignored_task_id {
            mark_webui_subagent_completion_gate_ignored(&mut guard, child_task_id);
        }
        let closed_subagent_task_ids = close_agent_closed_subagent_task_ids_from_event(event)?;
        if remove_webui_closed_subagent_tasks_in_state(&mut guard, &closed_subagent_task_ids) {
            subagent_navigation_changed = true;
        }
        let surface_matches =
            workbench_should_emit_runtime_event_message_patch(&guard, &conversation_id, session_id);
        let mut message_window_deltas = Vec::new();
        if surface_matches && transcript_changed {
            projected_messages = merge_visible_streaming_rows_with_runtime_projection(
                before_visible_messages.as_slice(),
                projected_messages.as_slice(),
            );
            let message_patch = build_runtime_event_conversation_messages_patched_delta(
                &conversation_id,
                before_visible_messages.as_slice(),
                projected_messages.as_slice(),
                event,
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
                message_window_deltas.push(WorkbenchDeltaEnvelope {
                    generation,
                    delta: WorkbenchDelta::ConversationMessagesPatched(message_patch),
                });
            }
        }
        if let Some((child_task_id, child_display_messages, child_projected_messages)) =
            child_message_projection
        {
            if let Some(cache_entry) = guard.subagent_surface_detail_cache.get_mut(&child_task_id) {
                cache_entry.messages = child_display_messages;
                cache_entry.last_access_ms = current_unix_millis_value();
            }
            let child_conversation_id = child_task_id.to_string();
            let before_child_visible_messages = guard
                .workbench
                .messages
                .iter()
                .filter(|message| message.conversation_id == child_conversation_id)
                .cloned()
                .collect::<Vec<_>>();
            if let Some(message_patch) = build_conversation_messages_patched_delta(
                &child_conversation_id,
                before_child_visible_messages.as_slice(),
                child_projected_messages.as_slice(),
            ) {
                guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
                let generation = guard.workbench_generation;
                guard.workbench.revision = generation;
                replace_conversation_messages_in_workbench_window(
                    &mut guard.workbench.messages,
                    &child_conversation_id,
                    child_projected_messages,
                );
                message_window_deltas.push(WorkbenchDeltaEnvelope {
                    generation,
                    delta: WorkbenchDelta::ConversationMessagesPatched(message_patch),
                });
            }
        }
        (workspace_snapshot, message_window_deltas)
    };
    for event in message_window_deltas {
        emit_service_event_with_trace(
            app,
            "workbench_runtime_event_messages_delta_emit",
            ServiceEvent::WorkbenchDelta(Box::new(event)),
        );
    }
    if subagent_navigation_changed {
        emit_webui_subagent_navigation_changed_delta_for_parent(app, state, session_id)?;
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

/// Returns whether one runtime-event transcript patch should be emitted to the WebUI message window.
/// 返回单个运行时事件转录补丁是否应该发送到 WebUI 消息窗口。
pub(crate) fn workbench_should_emit_runtime_event_message_patch(
    guard: &ServiceBridgeState,
    conversation_id: &str,
    session_id: SessionId,
) -> bool {
    workbench_surface_matches_conversation(&guard.workbench, conversation_id)
        || guard
            .subagent_surface_detail_cache
            .contains_key(&session_id)
        || guard.detached_subagent_task_listeners.contains(&session_id)
}

/// Returns stable transcript entries created or updated by one runtime tool projection.
/// 返回单次运行时工具投影创建或更新的稳定转录项。
fn runtime_event_projection_entries_from_changed_records(
    before_records: &[SessionConversationRecord],
    after_records: &[SessionConversationRecord],
) -> Vec<SessionMessageEntry> {
    let before_by_id = before_records
        .iter()
        .map(|record| (record.id.as_str(), record))
        .collect::<BTreeMap<_, _>>();
    after_records
        .iter()
        .filter(|record| runtime_event_projection_record_is_stable(record))
        .filter(|record| {
            before_by_id
                .get(record.id.as_str())
                .is_none_or(|before| !session_conversation_records_equal(before, record))
        })
        .map(session_message_entry_from_conversation_record)
        .collect()
}

/// Returns whether one runtime record is safe to retain before the turn has completed.
/// 返回单条运行时记录是否可以在当前轮次完成前稳定保留。
fn runtime_event_projection_record_is_stable(record: &SessionConversationRecord) -> bool {
    matches!(
        record.record_kind,
        SessionMessageRecordKind::ToolCallOutput
            | SessionMessageRecordKind::ToolResultOutput
            | SessionMessageRecordKind::ToolDetailOutput
    )
}

/// Compares two runtime conversation records without requiring protocol-level PartialEq.
/// 在不要求协议类型实现 PartialEq 的情况下比较两条运行时会话记录。
fn session_conversation_records_equal(
    left: &SessionConversationRecord,
    right: &SessionConversationRecord,
) -> bool {
    left.id == right.id
        && left.kind == right.kind
        && left.record_kind == right.record_kind
        && left.visible == right.visible
        && left.title == right.title
        && left.body == right.body
        && left.metadata == right.metadata
        && left.created_at == right.created_at
}

/// Merges changed runtime tool entries into retained display transcript rows.
/// 将变化的运行时工具项合并到已保留的显示转录行中。
fn merge_session_messages_with_runtime_event_entries(
    existing_messages: &[SessionMessageEntry],
    projection_entries: &[SessionMessageEntry],
) -> Vec<SessionMessageEntry> {
    let mut merged_messages = existing_messages.to_vec();
    for entry in projection_entries {
        if let Some(existing_entry) = merged_messages
            .iter_mut()
            .find(|message| message.id == entry.id)
        {
            *existing_entry = entry.clone();
        } else {
            merged_messages.push(entry.clone());
        }
    }
    merged_messages
}

/// Extracts closed subagent task ids from a structured `close_agent` host result.
/// 从结构化 `close_agent` 宿主结果中提取已关闭的子代理任务标识。
fn close_agent_closed_subagent_task_ids_from_event(
    event: &RuntimeEvent,
) -> Result<Vec<SessionId>, String> {
    if event.kind != RuntimeEventKind::ToolFinished {
        return Ok(Vec::new());
    }
    let Some(metadata) = event.metadata.as_ref() else {
        return Ok(Vec::new());
    };
    Ok(
        subagent_close_agent_projection_from_tool_metadata(metadata)?
            .map(|projection| projection.affected_task_ids)
            .unwrap_or_default(),
    )
}

/// Returns the UI run phase that must advance together with one runtime-event message patch.
/// 返回必须与单条运行时事件消息补丁一起推进的界面运行阶段。
fn runtime_event_projection_run_phase(event: &RuntimeEvent) -> Option<SessionRunPhase> {
    match event.kind {
        RuntimeEventKind::ToolStarted => Some(SessionRunPhase::ToolRunning),
        RuntimeEventKind::ToolFinished => Some(SessionRunPhase::Requesting),
        _ => None,
    }
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
    use std::collections::{BTreeMap, BTreeSet};
    use vulcan_subagent_core::STOPPED_SUBAGENT_TASK_IDS_KEY;

    /// Builds one minimal visible message for runtime-projection ordering tests.
    /// 为运行时投影顺序测试构建一条最小可见消息。
    fn test_runtime_projection_message(
        id: &str,
        record_kind: SessionMessageRecordKind,
    ) -> ConversationMessage {
        ConversationMessage {
            id: id.to_string(),
            conversation_id: String::from("conversation"),
            created_at: id.to_string(),
            role: MessageRole::Assistant,
            title: None,
            record_kind: Some(record_kind.as_persisted_key().to_string()),
            body: id.to_string(),
            metadata: None,
            attachments: Vec::new(),
            editable_user_prompt: false,
        }
    }

    /// Verifies a new tool projection remains after already visible thinking and assistant stream rows.
    /// 验证新工具投影保持在已经可见的思考与 Assistant 流记录之后。
    #[test]
    fn runtime_tool_projection_preserves_visible_stream_order() {
        // before_messages mirrors thinking and Assistant rows already shown before the tool event.
        // before_messages 模拟工具事件前已经显示的思考与 Assistant 记录。
        let before_messages = vec![
            test_runtime_projection_message("1000", SessionMessageRecordKind::ThinkingOutput),
            test_runtime_projection_message("1001", SessionMessageRecordKind::AssistantOutput),
        ];
        // projected_messages contains the newly stable tool row and intentionally omits active stream drafts.
        // projected_messages 包含新稳定工具记录，并有意省略活跃流式草稿。
        let projected_messages = vec![test_runtime_projection_message(
            "1002",
            SessionMessageRecordKind::ToolCallOutput,
        )];

        // merged_messages must retain the visible stream rows before appending the new tool.
        // merged_messages 必须先保留可见流记录，再追加新工具。
        let merged_messages = merge_visible_streaming_rows_with_runtime_projection(
            before_messages.as_slice(),
            projected_messages.as_slice(),
        );

        assert_eq!(
            merged_messages
                .iter()
                .map(|message| message.id.as_str())
                .collect::<Vec<_>>(),
            vec!["1000", "1001", "1002"]
        );
    }

    /// Verifies close-agent UI cleanup reads only the structured host result task ids.
    /// 验证关闭代理的界面清理只读取结构化宿主结果中的任务标识。
    #[test]
    fn close_agent_closed_task_ids_are_extracted_from_matched_agents() {
        let task_id = SessionId::new_v4();
        let event = RuntimeEvent::new(
            RuntimeEventKind::ToolFinished,
            "工具返回 · close_agent",
            "closed",
        )
        .with_metadata(Some(json!({
            "host_result": {
                "kind": "subagent_control",
                "payload": {
                    "tool": "close_agent",
                    "matched_agents": [{
                        (SUBAGENT_TASK_ID_KEY): task_id.to_string(),
                        "agent_id": "code-review"
                    }],
                    (STOPPED_SUBAGENT_TASK_IDS_KEY): [],
                }
            }
        })));

        assert_eq!(
            close_agent_closed_subagent_task_ids_from_event(&event).expect("close ids"),
            vec![task_id]
        );
    }

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

    /// Verifies runtime tool projection does not promote active streaming drafts into retained transcript rows.
    /// 验证运行时工具投影不会把活跃流式草稿提升为保留转录行。
    #[test]
    fn runtime_tool_projection_keeps_streaming_drafts_out_of_retained_rows() {
        let session_id = SessionId::new_v4();
        let mut runtime_state = SessionRuntimeStateEntry::default();
        let mut thinking_record_id = None;
        append_stream_delta_record_to_state(
            &mut runtime_state,
            session_id,
            "1000",
            &mut thinking_record_id,
            SessionMessageRecordKind::ThinkingOutput,
            "思考过程",
            "正在整理工具调用。",
            true,
        );
        let before_projection_records = runtime_state.conversation_state.records.clone();
        let started = RuntimeEvent::new(
            RuntimeEventKind::ToolStarted,
            "工具调用 · call-read · vulcan_file_read",
            "## Tool Call",
        )
        .with_metadata(Some(json!({
            "tool_call_id": "call-read",
            "tool_name": "vulcan_file_read",
            "arguments": {
                "path": "README.md"
            }
        })));

        assert!(append_runtime_event_projection_to_state(
            &mut runtime_state,
            session_id,
            "1001",
            &started,
        ));
        let projection_entries = runtime_event_projection_entries_from_changed_records(
            before_projection_records.as_slice(),
            runtime_state.conversation_state.records.as_slice(),
        );
        let merged_messages =
            merge_session_messages_with_runtime_event_entries(&[], projection_entries.as_slice());

        assert_eq!(merged_messages.len(), 1);
        assert_eq!(
            merged_messages[0].record_kind,
            SessionMessageRecordKind::ToolCallOutput
        );
        assert!(
            !merged_messages
                .iter()
                .any(|message| message.record_kind == SessionMessageRecordKind::ThinkingOutput)
        );
    }

    /// Verifies selected-session memory injections are projected through a dedicated workbench delta.
    /// 验证当前选中会话的记忆注入会通过专用工作台增量投影。
    #[test]
    fn context_injection_delta_projects_selected_session_memory_panel() {
        let session_id = SessionId::new_v4();
        let background_session_id = SessionId::new_v4();
        let mut runtime_state = SessionRuntimeStateEntry::default();
        runtime_state.context_injection_state.apply_insert(
            InjectionInsert::single(
                InjectionKind::Transient,
                "vmm_inject",
                "Dynamic memory",
                "动态记忆内容",
                Some(1),
            ),
            "turn-1",
        );
        runtime_state.context_injection_state.apply_insert(
            InjectionInsert::single(
                InjectionKind::Persistent,
                "profile",
                "Static profile",
                "持久画像内容",
                None,
            ),
            "turn-1",
        );
        let workspace_snapshot = WorkspaceSnapshot {
            task_projects: Vec::new(),
            archived_task_projects: Vec::new(),
            sessions: Vec::new(),
            archived_sessions: Vec::new(),
            session_messages: BTreeMap::new(),
            archived_session_messages: BTreeMap::new(),
            session_compactions: BTreeMap::new(),
            archived_session_compactions: BTreeMap::new(),
            session_runtime_state: BTreeMap::from([(session_id, runtime_state)]),
            archived_session_runtime_state: BTreeMap::new(),
            session_run_phases: BTreeMap::new(),
            expanded_task_projects: BTreeSet::new(),
            selected_session: session_id,
            hydrated_session_ids: BTreeSet::new(),
        };

        let delta = context_injection_changed_delta_from_workspace_snapshot(
            &workspace_snapshot,
            session_id,
        )
        .expect("selected session should produce context injection delta");

        assert_eq!(delta.conversation_id, session_id.to_string());
        let snapshot = delta
            .context_injection
            .expect("runtime state should produce a memory-panel snapshot");
        assert_eq!(snapshot.dynamic_cards.len(), 1);
        assert_eq!(snapshot.static_cards.len(), 1);
        assert!(
            snapshot.dynamic_cards[0].items[0]
                .content
                .contains("动态记忆内容")
        );
        assert!(
            snapshot.static_cards[0].items[0]
                .content
                .contains("持久画像内容")
        );
        assert!(
            context_injection_changed_delta_from_workspace_snapshot(
                &workspace_snapshot,
                background_session_id,
            )
            .is_none()
        );
    }

    /// Verifies selected-session deltas can clear the memory panel while retained lists omit empties.
    /// 验证选中会话增量可清空记忆面板，而保留列表会省略空条目。
    #[test]
    fn context_injection_delta_clears_selected_session_but_omits_empty_retained_entries() {
        // Selected session whose runtime state is absent after memory cleanup.
        // 记忆清理后运行态缺失的选中会话。
        let selected_session_id = SessionId::new_v4();
        // Hydrated background session with no runtime state.
        // 没有运行态的已水合后台会话。
        let hydrated_session_id = SessionId::new_v4();
        // Workspace snapshot used by both stream delta and retained projection paths.
        // 流式增量与保留投影路径共同使用的工作区快照。
        let workspace_snapshot = WorkspaceSnapshot {
            task_projects: Vec::new(),
            archived_task_projects: Vec::new(),
            sessions: Vec::new(),
            archived_sessions: Vec::new(),
            session_messages: BTreeMap::new(),
            archived_session_messages: BTreeMap::new(),
            session_compactions: BTreeMap::new(),
            archived_session_compactions: BTreeMap::new(),
            session_runtime_state: BTreeMap::new(),
            archived_session_runtime_state: BTreeMap::new(),
            session_run_phases: BTreeMap::new(),
            expanded_task_projects: BTreeSet::new(),
            selected_session: selected_session_id,
            hydrated_session_ids: BTreeSet::from([hydrated_session_id]),
        };

        // Selected session still produces a delta with a None payload so the UI can clear it.
        // 选中会话仍会产生 None 载荷增量，以便 UI 清空面板。
        let selected_delta = context_injection_changed_delta_from_workspace_snapshot(
            &workspace_snapshot,
            selected_session_id,
        )
        .expect("selected session should emit a clearing delta");
        // Retained by-conversation projection omits empty entries.
        // 按会话保留的投影会省略空条目。
        let retained_deltas = context_injection_by_conversation_from_workspace(&workspace_snapshot);

        assert_eq!(
            selected_delta.conversation_id,
            selected_session_id.to_string()
        );
        assert!(selected_delta.context_injection.is_none());
        assert!(retained_deltas.is_empty());
    }

    /// Verifies a tool-start message patch carries the phase that closes the thinking overlay.
    /// 验证工具开始消息补丁携带用于关闭思考浮层的运行阶段。
    #[test]
    fn runtime_tool_started_message_patch_carries_tool_running_phase() {
        let conversation_id = SessionId::new_v4().to_string();
        let started = RuntimeEvent::new(
            RuntimeEventKind::ToolStarted,
            "工具调用 · call-read · vulcan_file_read",
            "## Tool Call",
        );
        let tool_message = ConversationMessage {
            id: String::from("tool-call-call-read"),
            conversation_id: conversation_id.clone(),
            created_at: String::from("1000"),
            role: MessageRole::Assistant,
            title: Some(String::from("工具调用 · vulcan_file_read")),
            record_kind: Some(String::from("tool-call-output")),
            body: String::new(),
            metadata: Some(json!({
                "tool_call_id": "call-read",
                "tool_name": "vulcan_file_read",
            })),
            attachments: Vec::new(),
            editable_user_prompt: false,
        };

        let patch = build_runtime_event_conversation_messages_patched_delta(
            &conversation_id,
            &[],
            &[tool_message],
            &started,
        )
        .expect("tool start should insert a visible tool-call message");

        assert_eq!(
            patch.run_phase,
            Some(SessionRunPhase::ToolRunning.to_string())
        );
        assert_eq!(patch.upsert_messages.len(), 1);
        assert_eq!(
            patch.upsert_messages[0].record_kind.as_deref(),
            Some("tool-call-output")
        );
    }
}
