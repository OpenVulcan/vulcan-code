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
        let normalized = if is_finished {
            terminal_webui_subagent_task_state(panel_state.clone())
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
        normalized
    };
    webui_trace(
        "subagent_runtime_event_normalized",
        format!(
            "parent={parent_session_id} task={} run_state={} has_conversation_state={} result_chars={} terminal={is_finished}",
            normalized_state.task_id,
            subagent_run_state_key(normalized_state.run_state),
            normalized_state.conversation_state.is_some(),
            normalized_state.result_text.chars().count()
        ),
    );

    let message_window_event = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let conversation_id = normalized_state.task_id.to_string();
        let before_visible_messages = guard
            .workbench
            .messages
            .iter()
            .filter(|message| message.conversation_id == conversation_id)
            .cloned()
            .collect::<Vec<_>>();
        let (projected_messages, cached_messages) = {
            let workspace = guard
                .workspace_snapshot
                .as_mut()
                .ok_or_else(|| String::from("workspace snapshot is not ready"))?;
            attach_webui_task_session_metadata_to_parent_tool_message(
                workspace,
                parent_session_id,
                metadata,
            );
            if let Some(prompt) = prompt.as_deref() {
                seed_webui_subagent_task_prompt_if_needed(
                    workspace,
                    normalized_state.task_id,
                    prompt,
                );
            }
            if let Some(conversation_state) = normalized_state.conversation_state.clone() {
                replace_webui_subagent_task_conversation_state(
                    workspace,
                    normalized_state.task_id,
                    conversation_state,
                );
            }
            let messages = workspace
                .session_messages
                .get(&normalized_state.task_id)
                .cloned()
                .unwrap_or_default();
            (
                conversation_messages_from_session(&conversation_id, messages.as_slice(), false),
                messages,
            )
        };
        let cache_access_ms = current_unix_millis_value();
        guard.subagent_surface_detail_cache.insert(
            normalized_state.task_id,
            WebuiSubagentSurfaceDetailCacheEntry {
                task_state: normalized_state.clone(),
                messages: cached_messages,
                last_access_ms: cache_access_ms,
            },
        );
        prune_webui_subagent_surface_detail_cache(&mut guard, cache_access_ms);
        if let Some(message_patch) = build_conversation_messages_patched_delta(
            &conversation_id,
            before_visible_messages.as_slice(),
            projected_messages.as_slice(),
        ) {
            guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
            let generation = guard.workbench_generation;
            guard.workbench.revision = generation;
            replace_conversation_messages_in_workbench_window(
                &mut guard.workbench.messages,
                &conversation_id,
                projected_messages,
            );
            Some(WorkbenchDeltaEnvelope {
                generation,
                delta: WorkbenchDelta::ConversationMessagesPatched(message_patch),
            })
        } else {
            None
        }
    };
    if let Some(event) = message_window_event {
        emit_service_event_with_trace(
            app,
            "subagent_messages_patch_emit",
            ServiceEvent::WorkbenchDelta(Box::new(event)),
        );
    }
    emit_webui_subagent_navigation_changed_delta(app, state)?;
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

/// Converts one delegate-finished panel payload into an authoritative terminal state.
/// 将单条 delegate-finished 面板载荷转换为权威终态状态。
fn terminal_webui_subagent_task_state(mut state: SubagentTaskPanelState) -> SubagentTaskPanelState {
    if state.run_state.is_running() {
        state.run_state = SubagentTaskRunState::Completed;
    }
    state
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
    append_webui_subagent_session_message(workspace_snapshot, task_id, || {
        build_webui_subagent_task_prompt_message(task_id, prompt)
    });
}

/// Appends one child-task display message and synchronizes its dialogue-ready marker.
/// 追加一条子任务显示消息，并同步其对话层就绪标记。
fn append_webui_subagent_session_message<Build>(
    workspace_snapshot: &mut WorkspaceSnapshot,
    task_id: SessionId,
    build_message: Build,
) where
    Build: FnOnce() -> SessionMessageEntry,
{
    workspace_snapshot
        .session_messages
        .entry(task_id)
        .or_default()
        .push(build_message());
    sync_hydrated_session_dialogue_ready_marker(workspace_snapshot, task_id);
}

/// Appends one child-task display message only when no existing row matches the duplicate predicate, then synchronizes its dialogue-ready marker.
/// 仅当没有现有行命中重复判定时追加一条子任务显示消息，然后同步其对话层就绪标记。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` owns the child transcript cache being mutated.
/// `workspace_snapshot` 持有正在变更的子任务转录缓存。
/// `task_id` selects the child transcript to update.
/// `task_id` 选择要更新的子任务转录。
/// `is_duplicate` identifies an already-projected row that should suppress the new message.
/// `is_duplicate` 识别会抑制新消息的已投影行。
/// `build_message` creates the new display row only after duplicate checks pass.
/// `build_message` 仅在重复检查通过后创建新的显示行。
///
/// # Returns
/// 返回值
/// Returns whether the message was appended.
/// 返回是否实际追加了消息。
fn append_webui_subagent_session_message_if_missing<IsDuplicate, Build>(
    workspace_snapshot: &mut WorkspaceSnapshot,
    task_id: SessionId,
    is_duplicate: IsDuplicate,
    build_message: Build,
) -> bool
where
    IsDuplicate: Fn(&SessionMessageEntry) -> bool,
    Build: FnOnce() -> SessionMessageEntry,
{
    let messages = workspace_snapshot
        .session_messages
        .entry(task_id)
        .or_default();
    if messages.iter().any(is_duplicate) {
        return false;
    }
    messages.push(build_message());
    sync_hydrated_session_dialogue_ready_marker(workspace_snapshot, task_id);
    true
}

/// Builds the initial child task prompt display row.
/// 构建初始子任务提示词显示行。
fn build_webui_subagent_task_prompt_message(
    task_id: SessionId,
    prompt: &str,
) -> SessionMessageEntry {
    let now = current_unix_millis_text();
    SessionMessageEntry::new(SessionMessageKind::User, "用户指令", prompt)
        .with_runtime_metadata(format!("session-{task_id}-subagent-prompt-{now}"), now)
}

/// Appends a parent-to-child send_message prompt to the child transcript from one tool event.
/// 根据单条工具事件把父级发给子智能体的 send_message 提示词追加到子会话转录。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` is the mutable display snapshot that owns child transcript rows.
/// `workspace_snapshot` 是持有子会话转录行的可变显示快照。
/// `event` is the runtime tool-finished event being projected.
/// `event` 是正在投影的运行时工具完成事件。
///
/// # Returns
/// 返回值
/// The child task id when one visible message was appended.
/// 当成功追加一条可见消息时返回子任务标识。
pub(crate) fn append_webui_subagent_send_message_from_event_if_needed(
    workspace_snapshot: &mut WorkspaceSnapshot,
    event: &RuntimeEvent,
) -> Option<SessionId> {
    if event.kind != RuntimeEventKind::ToolFinished {
        return None;
    }
    let metadata = event.metadata.as_ref()?;
    let payload = metadata
        .get("host_result")
        .filter(|host_result| {
            host_result.get("kind").and_then(Value::as_str) == Some("subagent_message")
        })
        .and_then(|host_result| host_result.get("payload"))?;
    if payload.get("tool").and_then(Value::as_str) != Some("send_message") {
        return None;
    }
    if payload.get("target").and_then(Value::as_str) == Some("root") {
        return None;
    }
    let task_id = payload
        .get(SUBAGENT_TASK_ID_KEY)
        .and_then(Value::as_str)
        .and_then(|value| value.parse::<SessionId>().ok())?;
    let message = payload
        .get("message")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())?;
    let tool_call_id = subagent_send_message_tool_call_id(event, metadata)?;
    let message_id = format!("session-{task_id}-subagent-send-message-{tool_call_id}");
    let duplicate_message_id = message_id.clone();
    let appended = append_webui_subagent_session_message_if_missing(
        workspace_snapshot,
        task_id,
        |entry| entry.id == duplicate_message_id,
        || build_webui_subagent_send_message(payload, task_id, message, message_id),
    );
    if !appended {
        return None;
    }
    Some(task_id)
}

/// Builds one display-only child transcript row from a parent-to-child send_message payload.
/// 根据父级发往子任务的 send_message 载荷构建一条仅显示的子转录行。
fn build_webui_subagent_send_message(
    payload: &Value,
    task_id: SessionId,
    message: &str,
    message_id: String,
) -> SessionMessageEntry {
    let now = current_unix_millis_text();
    let mut entry = SessionMessageEntry::new(SessionMessageKind::User, "智能体消息", message)
        .with_runtime_metadata(message_id, now);
    entry.record_kind = SessionMessageRecordKind::DisplayNote;
    entry.metadata = Some(json!({
        "display_note": true,
        "kind": "subagent_message",
        "tool": "send_message",
        "parent_session_id": payload.get("parent_session_id").cloned(),
        "target": payload.get("target").cloned(),
        (SUBAGENT_TASK_ID_KEY): task_id.to_string(),
        "delivery_mode": payload.get("delivery_mode").cloned(),
        "queued": payload.get("queued").cloned(),
        "interrupted": payload.get("interrupted").cloned(),
    }));
    entry
}

/// Resolves the stable tool-call id used to de-duplicate a projected send_message prompt.
/// 解析用于去重投影 send_message 提示词的稳定工具调用标识。
fn subagent_send_message_tool_call_id(event: &RuntimeEvent, metadata: &Value) -> Option<String> {
    metadata
        .get("tool_call_id")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .or_else(|| parse_tool_finished_event_title_call_id(&event.title))
}

/// Parses the standard runtime tool-finished title into a tool-call id.
/// 将标准运行时工具完成标题解析为工具调用标识。
fn parse_tool_finished_event_title_call_id(title: &str) -> Option<String> {
    let mut segments = title.split(" · ");
    match (segments.next(), segments.next()) {
        (Some("工具返回"), Some(tool_call_id)) if !tool_call_id.trim().is_empty() => {
            Some(tool_call_id.trim().to_string())
        }
        _ => None,
    }
}

/// Appends the canonical stopped notice to a child task transcript when it is missing.
/// 当子任务转录缺少停止提示时追加规范停止提示。
pub(crate) fn append_webui_subagent_stop_notice_if_needed(
    workspace_snapshot: &mut WorkspaceSnapshot,
    task_id: SessionId,
) {
    append_webui_subagent_session_message_if_missing(
        workspace_snapshot,
        task_id,
        |message| {
            message.title == "执行已停止"
                && message.body.trim() == SUBAGENT_TASK_STOPPED_SUMMARY_TEXT
        },
        || build_webui_subagent_stop_notice(task_id),
    );
}

/// Builds the canonical stopped notice appended to one child task transcript.
/// 构建追加到单个子任务转录中的规范停止提示。
fn build_webui_subagent_stop_notice(task_id: SessionId) -> SessionMessageEntry {
    let now = current_unix_millis_text();
    SessionMessageEntry::new(
        SessionMessageKind::Assistant,
        "执行已停止",
        SUBAGENT_TASK_STOPPED_SUMMARY_TEXT,
    )
    .with_runtime_metadata(format!("session-{task_id}-subagent-stopped-{now}"), now)
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
    let execution_status =
        task_session_execution_status_from_tool_metadata(metadata).unwrap_or("running");
    if message_ids.is_empty() {
        let task_id = payload
            .get(SUBAGENT_TASK_ID_KEY)
            .or_else(|| payload.get("task_id"))
            .and_then(Value::as_str)
            .unwrap_or("<unknown>");
        webui_trace(
            "subagent_parent_tool_metadata_target_missing",
            format!("parent={parent_session_id} task={task_id} status={execution_status}"),
        );
        return;
    }
    let metadata_update =
        attach_tool_display_state_to_metadata(Some(metadata.clone()), execution_status)
            .unwrap_or_else(|| metadata.clone());
    for message_id in message_ids {
        if let Err(error) = merge_webui_task_session_tool_message_metadata(
            workspace_snapshot,
            parent_session_id,
            &message_id,
            metadata_update.clone(),
            execution_status,
        ) {
            webui_trace(
                "subagent_parent_tool_metadata_merge_failed",
                format!(
                    "parent={parent_session_id} message_id={message_id} status={execution_status} error={error}"
                ),
            );
        }
    }
}

/// Shallow-merges task-session metadata into one WebUI parent tool-card record and projection.
/// 将 task-session 元数据浅合并进一条 WebUI 父级工具卡片记录与投影。
///
/// # Returns
/// 返回值
/// `Ok(())` when a canonical record or visible message is updated.
/// 规范记录或可见消息被更新时返回 `Ok(())`。
pub(crate) fn merge_webui_task_session_tool_message_metadata(
    workspace_snapshot: &mut WorkspaceSnapshot,
    parent_session_id: SessionId,
    message_id: &str,
    metadata_update: Value,
    execution_status: &str,
) -> Result<(), String> {
    let current_metadata =
        webui_task_session_tool_message_metadata(workspace_snapshot, parent_session_id, message_id);
    let Some(updated_metadata) = merge_task_mirror_tool_display_metadata(
        current_metadata,
        task_session_display_metadata_update(metadata_update, execution_status),
        execution_status == "running",
    ) else {
        return Err(format!(
            "task-session metadata merge produced no metadata for parent `{parent_session_id}` message `{message_id}`"
        ));
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
    if !updated {
        return Err(format!(
            "task-session metadata target message `{message_id}` was not found in parent `{parent_session_id}`"
        ));
    }
    Ok(())
}
