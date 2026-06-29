//! subagent event ingest helpers.
//! subagent event ingest 辅助逻辑。

use super::*;

/// Consumes one delegate runtime event into the WebUI subagent cache and child transcript state.
/// 将一条委派运行时事件吸收到 WebUI 子代理缓存与子会话转录状态中。
pub(crate) fn ingest_webui_subagent_runtime_event(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    _agent_runtime: &AgentRuntimeBridge,
    parent_session_id: SessionId,
    event: &RuntimeEvent,
) -> Result<bool, String> {
    if !matches!(
        event.kind,
        RuntimeEventKind::DelegateStarted | RuntimeEventKind::DelegateFinished
    ) {
        return Ok(false);
    }
    let is_finished = event.kind == RuntimeEventKind::DelegateFinished;
    let Some(metadata) = event.metadata.as_ref() else {
        return Ok(true);
    };
    let prompt = parse_subagent_task_prompt_metadata(metadata);
    let Some(panel_state) = parse_subagent_task_panel_metadata(parent_session_id, metadata, None)
    else {
        return Ok(true);
    };
    let usage_segments = if is_finished {
        match subagent_usage_segments_from_runtime_metadata(metadata) {
            Ok(usage_segments) => usage_segments,
            Err(error) => {
                eprintln!("[webui-service] subagent usage segment parse failed: {error}");
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };
    webui_trace(
        "subagent_runtime_event_ingest",
        format!(
            "parent={parent_session_id} task={} kind={:?} is_finished={is_finished} public_task_id={} label={}",
            panel_state.task_id, event.kind, panel_state.public_task_id, panel_state.subagent_label
        ),
    );
    let normalized_state = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let workspace_snapshot = guard.workspace_snapshot.as_ref();
        let normalized = if is_finished {
            normalize_webui_subagent_task_state(workspace_snapshot, &panel_state, None, false)
        } else {
            panel_state.clone()
        };
        guard
            .session_subagent_navigation_rounds
            .insert(parent_session_id);
        if is_finished {
            guard
                .detached_subagent_task_listeners
                .remove(&normalized.task_id);
            if !usage_segments.is_empty() {
                guard
                    .pending_subagent_usage_segments
                    .entry(parent_session_id)
                    .or_default()
                    .extend(usage_segments);
            }
        }
        guard
            .session_subagent_tasks
            .entry(parent_session_id)
            .or_default()
            .insert(normalized.task_id, normalized.clone());
        normalized
    };
    webui_trace(
        "subagent_runtime_event_normalized",
        format!(
            "parent={parent_session_id} task={} run_state={} has_conversation_state={} result_chars={}",
            normalized_state.task_id,
            subagent_run_state_key(normalized_state.run_state),
            normalized_state.conversation_state.is_some(),
            normalized_state.result_text.chars().count()
        ),
    );

    with_workspace_mutation(app, state, |workspace| {
        attach_webui_task_session_metadata_to_parent_tool_message(
            workspace,
            parent_session_id,
            metadata,
        );
        if let Some(prompt) = prompt.as_deref() {
            seed_webui_subagent_task_prompt_if_needed(workspace, normalized_state.task_id, prompt);
        }
        if let Some(conversation_state) = normalized_state.conversation_state.clone() {
            replace_webui_subagent_task_conversation_state(
                workspace,
                normalized_state.task_id,
                conversation_state,
            );
        }
    })?;
    if is_finished {
        webui_trace(
            "subagent_runtime_event_projection_finished",
            format!(
                "parent={parent_session_id} task={}",
                normalized_state.task_id
            ),
        );
    }
    Ok(true)
}

/// Drains pending child-agent usage segments for one parent session exactly once.
/// 对单个父会话的一组待聚合子代理用量分段执行一次性取走。
pub(crate) fn take_pending_webui_subagent_usage_segments(
    state: &SharedServiceBridgeState,
    parent_session_id: SessionId,
) -> Result<Vec<SessionTurnUsageSegment>, String> {
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    Ok(guard
        .pending_subagent_usage_segments
        .remove(&parent_session_id)
        .unwrap_or_default())
}

/// Parses parent-turn usage segments from one task-session delegate metadata payload.
/// 从一份 task-session 委派元数据载荷解析父会话回合用量分段。
fn subagent_usage_segments_from_runtime_metadata(
    metadata: &Value,
) -> Result<Vec<SessionTurnUsageSegment>, String> {
    let Some(payload) = task_session_payload_from_tool_metadata(metadata) else {
        return Ok(Vec::new());
    };
    let Some(usage_segments) = payload.get("usage_segments") else {
        return Ok(Vec::new());
    };
    serde_json::from_value::<Vec<SessionTurnUsageSegment>>(usage_segments.clone())
        .map_err(|error| format!("invalid task-session usage_segments payload: {error}"))
}

/// Seeds one child task transcript with its initial prompt when no child transcript exists yet.
/// 当子任务尚无转录时，使用初始提示词播种子任务转录。
pub(crate) fn seed_webui_subagent_task_prompt_if_needed(
    workspace_snapshot: &mut WorkspaceSnapshot,
    task_id: SessionId,
    prompt: &str,
) {
    if prompt.trim().is_empty()
        || workspace_snapshot
            .session_messages
            .get(&task_id)
            .is_some_and(|messages| !messages.is_empty())
    {
        return;
    }
    let now = current_unix_millis_text();
    workspace_snapshot.session_messages.insert(
        task_id,
        vec![
            SessionMessageEntry::new(SessionMessageKind::User, "用户指令", prompt)
                .with_runtime_metadata(format!("session-{task_id}-subagent-prompt-{now}"), now),
        ],
    );
    sync_hydrated_session_dialogue_ready_marker(workspace_snapshot, task_id);
}

/// Appends the canonical stopped notice to a child task transcript when it is missing.
/// 当子任务转录缺少停止提示时追加规范停止提示。
pub(crate) fn append_webui_subagent_stop_notice_if_needed(
    workspace_snapshot: &mut WorkspaceSnapshot,
    task_id: SessionId,
) {
    let messages = workspace_snapshot
        .session_messages
        .entry(task_id)
        .or_default();
    if messages.iter().any(|message| {
        message.title == "执行已停止" && message.body.trim() == SUBAGENT_TASK_STOPPED_SUMMARY_TEXT
    }) {
        return;
    }
    let now = current_unix_millis_text();
    messages.push(
        SessionMessageEntry::new(
            SessionMessageKind::Assistant,
            "执行已停止",
            SUBAGENT_TASK_STOPPED_SUMMARY_TEXT,
        )
        .with_runtime_metadata(format!("session-{task_id}-subagent-stopped-{now}"), now),
    );
    sync_hydrated_session_dialogue_ready_marker(workspace_snapshot, task_id);
}

/// Links one delegate task-session payload back onto the visible WebUI parent task card.
/// 将一份委派 task-session 载荷反挂回 WebUI 可见父级 task 卡片。
pub(crate) fn attach_webui_task_session_metadata_to_parent_tool_message(
    workspace_snapshot: &mut WorkspaceSnapshot,
    parent_session_id: SessionId,
    metadata: &Value,
) {
    let Some(payload) = task_session_payload_from_tool_metadata(metadata) else {
        return;
    };
    let message_ids = find_webui_task_session_tool_message_ids_for_payload(
        workspace_snapshot,
        parent_session_id,
        payload,
    );
    if message_ids.is_empty() {
        return;
    }
    let execution_status =
        task_session_execution_status_from_tool_metadata(metadata).unwrap_or("running");
    let metadata_update =
        attach_tool_display_state_to_metadata(Some(metadata.clone()), execution_status)
            .unwrap_or_else(|| metadata.clone());
    for message_id in message_ids {
        let _ = merge_webui_task_session_tool_message_metadata(
            workspace_snapshot,
            parent_session_id,
            &message_id,
            metadata_update.clone(),
            execution_status,
        );
    }
}

/// Shallow-merges task-session metadata into one WebUI parent tool-card record and projection.
/// 将 task-session 元数据浅合并进一条 WebUI 父级工具卡片记录与投影。
pub(crate) fn merge_webui_task_session_tool_message_metadata(
    workspace_snapshot: &mut WorkspaceSnapshot,
    parent_session_id: SessionId,
    message_id: &str,
    metadata_update: Value,
    execution_status: &str,
) -> bool {
    let current_metadata =
        webui_task_session_tool_message_metadata(workspace_snapshot, parent_session_id, message_id);
    let Some(updated_metadata) = merge_task_mirror_tool_display_metadata(
        current_metadata,
        task_session_display_metadata_update(metadata_update, execution_status),
        execution_status == "running",
    ) else {
        return false;
    };
    let mut updated = false;
    let updated_at = current_unix_millis_text();
    if let Some(runtime_state) = workspace_snapshot
        .session_runtime_state
        .get_mut(&parent_session_id)
    {
        for record in &mut runtime_state.conversation_state.records {
            if record.id == message_id {
                record.metadata = Some(updated_metadata.clone());
                runtime_state.updated_at = updated_at.clone();
                updated = true;
                break;
            }
        }
    }
    if let Some(messages) = workspace_snapshot
        .session_messages
        .get_mut(&parent_session_id)
    {
        for message in messages {
            if message.id == message_id {
                message.metadata = Some(updated_metadata.clone());
                updated = true;
                break;
            }
        }
    }
    updated
}
