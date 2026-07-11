//! subagent event ingest helpers.
//! subagent event ingest 辅助逻辑。

use super::*;
use vulcan_workbench_model::SubagentFeedbackDisplayRecords;

/// Consumes one delegate runtime event into the WebUI subagent cache and child transcript state.
/// 将一条委派运行时事件吸收到 WebUI 子代理缓存与子会话转录状态中。
pub(crate) fn ingest_webui_subagent_runtime_event(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    _agent_runtime: &AgentRuntimeBridge,
    parent_session_id: SessionId,
    event: &RuntimeEvent,
) -> Result<bool, String> {
    ingest_webui_subagent_runtime_event_for_listener_execution(
        app,
        state,
        _agent_runtime,
        parent_session_id,
        event,
        None,
    )
}

/// Consumes one delegate event only while the optional detached listener still owns its execution.
/// 仅当可选分离监听器仍拥有其执行时吸收委派事件。
pub(crate) fn ingest_webui_subagent_runtime_event_for_listener_execution(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    _agent_runtime: &AgentRuntimeBridge,
    parent_session_id: SessionId,
    event: &RuntimeEvent,
    listener_execution_id: Option<&str>,
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
    let event_execution_id = subagent_feedback_execution_id_from_tool_metadata(metadata)?;
    let Some(panel_state) = parse_subagent_task_panel_metadata(parent_session_id, metadata, None)
    else {
        return Ok(true);
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
        if listener_execution_id.is_some_and(|execution_id| {
            guard
                .detached_subagent_task_listeners
                .get(&panel_state.task_id)
                .is_none_or(|current_execution_id| current_execution_id != execution_id)
        }) {
            return Ok(true);
        }
        let normalized = if is_finished {
            terminal_webui_subagent_task_state(panel_state.clone())
        } else {
            panel_state.clone()
        };
        guard
            .session_subagent_navigation_rounds
            .insert(parent_session_id);
        if is_finished && listener_execution_id.is_none() {
            guard
                .detached_subagent_task_listeners
                .remove(&normalized.task_id);
        }
        normalized
    };
    webui_trace(
        "subagent_runtime_event_normalized",
        format!(
            "parent={parent_session_id} task={} run_state={} has_conversation_state={} result_chars={} terminal={is_finished}",
            normalized_state.task_id,
            normalized_state.run_state.metadata_key(),
            normalized_state.conversation_state.is_some(),
            normalized_state.result_text.chars().count()
        ),
    );

    let message_window_events = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        if listener_execution_id.is_some_and(|execution_id| {
            guard
                .detached_subagent_task_listeners
                .get(&normalized_state.task_id)
                .is_none_or(|current_execution_id| current_execution_id != execution_id)
        }) {
            return Ok(true);
        }
        let parent_conversation_id = parent_session_id.to_string();
        let before_parent_visible_messages = guard
            .workbench
            .messages
            .iter()
            .filter(|message| message.conversation_id == parent_conversation_id)
            .cloned()
            .collect::<Vec<_>>();
        let conversation_id = normalized_state.task_id.to_string();
        let before_visible_messages = guard
            .workbench
            .messages
            .iter()
            .filter(|message| message.conversation_id == conversation_id)
            .cloned()
            .collect::<Vec<_>>();
        let feedback_projection_policy = if is_finished {
            parent_subagent_feedback_projection_policy(
                &guard,
                parent_session_id,
                &normalized_state,
                metadata,
            )
        } else {
            SubagentFeedbackParentProjectionPolicy::SkipAll
        };
        let parent_turn_binding = guard
            .subagent_parent_turn_bindings
            .get(&SubagentExecutionKey::new(
                normalized_state.task_id,
                &event_execution_id,
            ))
            .cloned();
        let (projected_messages, cached_messages, parent_projected_messages, parent_changed) = {
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
                let task_prompt_anchor = prompt
                    .as_deref()
                    .or(normalized_state.description.as_deref());
                replace_webui_subagent_task_conversation_state(
                    workspace,
                    normalized_state.task_id,
                    conversation_state,
                    task_prompt_anchor,
                );
            }
            let parent_changed = if is_finished {
                append_webui_subagent_feedback_records_if_missing(
                    workspace,
                    parent_session_id,
                    metadata,
                    &normalized_state,
                    feedback_projection_policy,
                    parent_turn_binding.as_ref(),
                )?
            } else {
                false
            };
            let parent_messages = if parent_changed {
                workspace
                    .session_messages
                    .get(&parent_session_id)
                    .cloned()
                    .unwrap_or_default()
            } else {
                Vec::new()
            };
            let messages = workspace
                .session_messages
                .get(&normalized_state.task_id)
                .cloned()
                .unwrap_or_default();
            (
                conversation_messages_from_session(&conversation_id, messages.as_slice(), false),
                messages,
                conversation_messages_from_session(
                    &parent_conversation_id,
                    parent_messages.as_slice(),
                    false,
                ),
                parent_changed,
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
        let mut message_window_events = Vec::new();
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
            message_window_events.push(WorkbenchDeltaEnvelope {
                generation,
                delta: WorkbenchDelta::ConversationMessagesPatched(message_patch),
            });
        }
        if parent_changed
            && workbench_surface_matches_conversation(&guard.workbench, &parent_conversation_id)
            && let Some(message_patch) = build_conversation_messages_patched_delta(
                &parent_conversation_id,
                before_parent_visible_messages.as_slice(),
                parent_projected_messages.as_slice(),
            )
        {
            guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
            let generation = guard.workbench_generation;
            guard.workbench.revision = generation;
            replace_conversation_messages_in_workbench_window(
                &mut guard.workbench.messages,
                &parent_conversation_id,
                parent_projected_messages,
            );
            message_window_events.push(WorkbenchDeltaEnvelope {
                generation,
                delta: WorkbenchDelta::ConversationMessagesPatched(message_patch),
            });
        }
        message_window_events
    };
    for event in message_window_events {
        emit_service_event_with_trace(
            app,
            "subagent_messages_patch_emit",
            ServiceEvent::WorkbenchDelta(Box::new(event)),
        );
    }
    emit_webui_subagent_navigation_changed_delta_for_parent(app, state, parent_session_id)?;
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

/// Parent transcript projection policy for one completed child-agent execution.
/// 单次智能体执行完成后写入父会话转录的投影策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SubagentFeedbackParentProjectionPolicy {
    /// Append both the display feedback card and the child-agent statistics row.
    /// 同时追加反馈展示卡与智能体统计行。
    AppendFeedbackAndStats,
    /// Skip every display row because wait_agent owns feedback and usage display.
    /// 完全跳过显示记录，因为 wait_agent 已经接管反馈与用量展示。
    SkipAll,
}

/// Appends parent-visible child-agent feedback rows without adding anything to provider context.
/// 追加父会话可见的智能体反馈记录，同时不向 Provider 上下文添加任何内容。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` owns the retained parent transcript state.
/// `workspace_snapshot` 持有保留的父级转录状态。
/// `parent_session_id` identifies the parent transcript that should show the feedback.
/// `parent_session_id` 标识需要展示反馈的父级转录。
/// `metadata` is the delegate-finished metadata that carries usage and mailbox payloads.
/// `metadata` 是携带用量和 mailbox 载荷的委派完成元数据。
/// `state` is the normalized child panel state used for labels and result text.
/// `state` 是用于标签与结果文本的规范化子面板状态。
/// `projection_policy` decides whether feedback and statistics should be projected to the parent transcript.
/// `projection_policy` 决定反馈与统计是否投影到父会话转录。
/// `parent_turn_binding` is the captured spawn-time parent turn used for durable UI grouping.
/// `parent_turn_binding` 是创建智能体时捕获的父回合绑定，用于持久稳定的 UI 分组。
///
/// # Returns
/// 返回值
/// `true` when new display rows were appended.
/// 当追加了新的显示行时返回 `true`。
fn append_webui_subagent_feedback_records_if_missing(
    workspace_snapshot: &mut WorkspaceSnapshot,
    parent_session_id: SessionId,
    metadata: &Value,
    state: &SubagentTaskPanelState,
    projection_policy: SubagentFeedbackParentProjectionPolicy,
    parent_turn_binding: Option<&WebuiSubagentParentTurnBinding>,
) -> Result<bool, String> {
    if projection_policy == SubagentFeedbackParentProjectionPolicy::SkipAll {
        return Ok(false);
    }
    let now = current_unix_millis_text();
    let task_id_text = state.task_id.to_string();
    let mut runtime_state = workspace_snapshot
        .session_runtime_state
        .get(&parent_session_id)
        .cloned()
        .unwrap_or_else(|| SessionRuntimeStateEntry {
            updated_at: now.clone(),
            ..SessionRuntimeStateEntry::default()
        });
    let execution_id = subagent_feedback_execution_id_from_tool_metadata(metadata)?;
    if runtime_state
        .conversation_state
        .records
        .iter()
        .any(|record| {
            subagent_feedback_record_matches(
                record.metadata.as_ref(),
                &task_id_text,
                execution_id.as_str(),
            )
        })
    {
        return Ok(false);
    }

    let mut display_records =
        subagent_feedback_display_records_from_tool_metadata(parent_session_id, metadata, state)?;
    if let Some(binding) = parent_turn_binding
        && binding.parent_session_id == parent_session_id
    {
        attach_parent_turn_binding_to_subagent_feedback_records(&mut display_records, binding);
    }
    if projection_policy == SubagentFeedbackParentProjectionPolicy::AppendFeedbackAndStats {
        append_webui_session_message_entry_to_state(
            &mut runtime_state,
            parent_session_id,
            display_records.feedback_entry,
        )?;
    }
    append_webui_session_message_entry_to_state(
        &mut runtime_state,
        parent_session_id,
        display_records.stats_entry,
    )?;

    let runtime_state_snapshot = runtime_state.clone();
    workspace_snapshot
        .session_runtime_state
        .insert(parent_session_id, runtime_state);
    merge_runtime_state_records_into_display_messages(
        workspace_snapshot,
        parent_session_id,
        &runtime_state_snapshot,
    );
    Ok(true)
}

/// Attaches the captured parent-turn binding to feedback rows for durable UI grouping.
/// 将捕获的父回合绑定写入反馈记录，供 UI 持久稳定分组。
fn attach_parent_turn_binding_to_subagent_feedback_records(
    records: &mut SubagentFeedbackDisplayRecords,
    binding: &WebuiSubagentParentTurnBinding,
) {
    attach_parent_turn_binding_to_metadata(&mut records.feedback_entry.metadata, binding);
    attach_parent_turn_binding_to_metadata(&mut records.stats_entry.metadata, binding);
}

/// Writes parent-turn binding fields into one display metadata object.
/// 将父回合绑定字段写入单条显示元数据对象。
fn attach_parent_turn_binding_to_metadata(
    metadata: &mut Option<Value>,
    binding: &WebuiSubagentParentTurnBinding,
) {
    let Some(Value::Object(metadata)) = metadata.as_mut() else {
        return;
    };
    metadata.insert(
        String::from("source_generation"),
        Value::from(binding.source_generation),
    );
    metadata.insert(String::from("parent_turn_binding"), Value::Bool(true));
    if let Some(parent_turn_id) = binding.parent_turn_id.as_ref() {
        metadata.insert(
            String::from("parent_turn_id"),
            Value::String(parent_turn_id.clone()),
        );
    }
    let mut payload = metadata
        .get_mut("host_result")
        .and_then(Value::as_object_mut)
        .and_then(|host_result| host_result.get_mut("payload"))
        .and_then(Value::as_object_mut);
    if let Some(payload) = payload.as_mut() {
        payload.insert(
            String::from("source_generation"),
            Value::from(binding.source_generation),
        );
    }
    if let Some(parent_turn_id) = binding.parent_turn_id.as_ref()
        && let Some(payload) = payload.as_mut()
    {
        payload.insert(
            String::from("parent_turn_id"),
            Value::String(parent_turn_id.clone()),
        );
    }
}

/// Selects the parent transcript projection policy for one completed child-agent execution.
/// 为一次完成的智能体执行选择父会话转录投影策略。
fn parent_subagent_feedback_projection_policy(
    state: &ServiceBridgeState,
    parent_session_id: SessionId,
    subagent_state: &SubagentTaskPanelState,
    metadata: &Value,
) -> SubagentFeedbackParentProjectionPolicy {
    if parent_wait_agent_will_display_subagent_feedback(
        state,
        parent_session_id,
        subagent_state,
        metadata,
    ) {
        return SubagentFeedbackParentProjectionPolicy::SkipAll;
    }
    SubagentFeedbackParentProjectionPolicy::AppendFeedbackAndStats
}

/// Returns whether wait_agent will display the child feedback inside its own tool result.
/// 返回 wait_agent 是否会在自身工具结果中展示智能体反馈。
fn parent_wait_agent_will_display_subagent_feedback(
    state: &ServiceBridgeState,
    parent_session_id: SessionId,
    subagent_state: &SubagentTaskPanelState,
    metadata: &Value,
) -> bool {
    state
        .parent_sessions_waiting_for_subagents
        .contains(&parent_session_id)
        || state
            .session_runtime_kernels
            .get(&parent_session_id)
            .is_some_and(SessionRuntimeKernel::has_wait_scopes)
        || subagent_feedback_execution_id_from_tool_metadata(metadata)
            .ok()
            .is_some_and(|execution_id| {
                webui_subagent_mailbox_input_consumed_for_execution(
                    state,
                    parent_session_id,
                    subagent_state.task_id,
                    execution_id.as_str(),
                )
            })
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
/// The child task id when one visible message was appended, `None` for unrelated or duplicate events.
/// 当成功追加一条可见消息时返回子任务标识；无关事件或重复事件返回 `None`。
#[cfg(test)]
pub(crate) fn append_webui_subagent_send_message_from_event_if_needed(
    workspace_snapshot: &mut WorkspaceSnapshot,
    event: &RuntimeEvent,
) -> Result<Option<SessionId>, String> {
    let Some((task_id, message)) = webui_subagent_send_message_from_event(event)? else {
        return Ok(None);
    };
    let duplicate_message_id = message.id.clone();
    let appended = append_webui_subagent_session_message_if_missing(
        workspace_snapshot,
        task_id,
        |entry| entry.id == duplicate_message_id,
        || message,
    );
    if !appended {
        return Ok(None);
    }
    Ok(Some(task_id))
}

/// Parses one visible child send-message projection without mutating workspace state.
/// 解析一条可见子会话 send-message 投影且不修改工作区状态。
///
/// # Parameters
/// 参数
/// `event` is the runtime tool event whose structured host result may describe a child message.
/// `event` 是结构化宿主结果可能描述子会话消息的运行时工具事件。
///
/// # Returns
/// 返回值
/// Returns the child task and display row, `None` for unrelated events, or the exact protocol error.
/// 返回子任务与显示行；无关事件返回 `None`；协议无效时返回准确错误。
pub(crate) fn webui_subagent_send_message_from_event(
    event: &RuntimeEvent,
) -> Result<Option<(SessionId, SessionMessageEntry)>, String> {
    if event.kind != RuntimeEventKind::ToolFinished {
        return Ok(None);
    }
    let Some(metadata) = event.metadata.as_ref() else {
        return Ok(None);
    };
    let Some(projection) =
        subagent_send_message_projection_from_tool_metadata(metadata, &event.title)?
    else {
        return Ok(None);
    };
    Ok(Some((
        projection.task_session_id,
        build_webui_subagent_send_message(projection),
    )))
}

/// Extracts a terminal child execution consumed by a synchronous `send_message` result.
/// 提取已由同步 `send_message` 结果消费的终态子执行。
///
/// # Parameters
/// 参数
/// `event` is the runtime tool-finished event being projected.
/// `event` 是正在投影的运行时工具完成事件。
///
/// # Returns
/// 返回值
/// Child task id plus execution id when the parent tool result already exposed a terminal child result.
/// 当父级工具结果已经暴露终态子结果时，返回子任务标识与执行标识。
pub(crate) fn terminal_subagent_send_message_resume_from_event(
    event: &RuntimeEvent,
) -> Result<Option<(SessionId, String)>, String> {
    let Some((task_session_id, execution_id, execution_status)) =
        subagent_send_message_resume_from_event(event)?
    else {
        return Ok(None);
    };
    if canonical_subagent_mailbox_terminal_status_label(execution_status).is_none() {
        return Ok(None);
    }
    Ok(Some((task_session_id, execution_id)))
}

/// Extracts a running child execution started synchronously by `send_message`.
/// 提取由 `send_message` 同步启动且仍在运行的子执行。
///
/// # Parameters
/// 参数
/// `event` is the runtime tool-finished event being projected.
/// `event` 是正在投影的运行时工具完成事件。
///
/// # Returns
/// 返回值
/// Child task id plus execution id only when this tool call started a running execution.
/// 仅当本次工具调用启动了仍在运行的执行时，返回子任务标识与执行标识。
pub(crate) fn running_subagent_send_message_resume_from_event(
    event: &RuntimeEvent,
) -> Result<Option<(SessionId, String)>, String> {
    let Some((task_session_id, execution_id, execution_status)) =
        subagent_send_message_resume_from_event(event)?
    else {
        return Ok(None);
    };
    if execution_status != "running" {
        return Ok(None);
    }
    Ok(Some((task_session_id, execution_id)))
}

/// Parses the concrete execution synchronously resumed by one `send_message` result.
/// 解析单个 `send_message` 结果同步续跑的具体执行。
///
/// # Parameters
/// 参数
/// `event` is the runtime tool-finished event carrying the host result.
/// `event` 是携带宿主结果的运行时工具完成事件。
///
/// # Returns
/// 返回值
/// The task, execution identity, and validated execution status, or `None` when no run started.
/// 返回任务、执行身份与已校验执行状态；未启动执行时返回 `None`。
fn subagent_send_message_resume_from_event(
    event: &RuntimeEvent,
) -> Result<Option<(SessionId, String, &str)>, String> {
    if event.kind != RuntimeEventKind::ToolFinished {
        return Ok(None);
    }
    let Some(metadata) = event.metadata.as_ref() else {
        return Ok(None);
    };
    let Some(host_result) = metadata.get("host_result").and_then(Value::as_object) else {
        return Ok(None);
    };
    if host_result.get("kind").and_then(Value::as_str) != Some("subagent_message") {
        return Ok(None);
    }
    let Some(payload) = host_result
        .get("payload")
        .filter(|payload| payload.is_object())
    else {
        return Ok(None);
    };
    if payload.get("tool").and_then(Value::as_str) != Some(SEND_MESSAGE_TOOL) {
        return Ok(None);
    }
    if payload.get("target").and_then(Value::as_str) == Some("root") {
        return Ok(None);
    }
    // status is present only when this tool call synchronously started a concrete child run.
    // status 仅在本次工具调用同步启动了具体子运行时存在。
    let status = payload
        .get("resumed_execution_status")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    // execution_id is paired with status by the subagent-message host protocol.
    // execution_id 按子代理消息宿主协议与 status 成对出现。
    let execution_id = payload
        .get("resumed_execution_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let (Some(status), Some(execution_id)) = (status, execution_id) else {
        if status.is_none() && execution_id.is_none() {
            return Ok(None);
        }
        return Err(String::from(
            "send_message resume payload must contain both resumed_execution_id and resumed_execution_status",
        ));
    };
    if status != "running" && canonical_subagent_mailbox_terminal_status_label(status).is_none() {
        return Err(format!(
            "send_message resume payload has unsupported execution status `{status}`"
        ));
    }
    let task_id_text = payload
        .get(SUBAGENT_TASK_ID_KEY)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| String::from("send_message resume payload is missing task_session_id"))?;
    let task_session_id = task_id_text.parse::<SessionId>().map_err(|error| {
        format!("send_message resume payload has invalid task_session_id `{task_id_text}`: {error}")
    })?;
    Ok(Some((task_session_id, execution_id.to_string(), status)))
}

/// Extracts child executions already consumed by a synchronous `wait_agent` result.
/// 提取已由同步 `wait_agent` 结果消费的子执行。
///
/// # Parameters
/// 参数
/// `event` is the runtime tool-finished event being projected.
/// `event` 是正在投影的运行时工具完成事件。
///
/// # Returns
/// 返回值
/// Child task ids plus execution ids that should not be injected again through the idle mailbox.
/// 返回不应再通过空闲 mailbox 注入的子任务标识与执行标识。
pub(crate) fn terminal_subagent_wait_consumed_executions_from_event(
    event: &RuntimeEvent,
) -> Result<Vec<(SessionId, String)>, String> {
    if event.kind != RuntimeEventKind::ToolFinished {
        return Ok(Vec::new());
    }
    let Some(metadata) = event.metadata.as_ref() else {
        return Ok(Vec::new());
    };
    let Some(host_result) = metadata.get("host_result").and_then(Value::as_object) else {
        return Ok(Vec::new());
    };
    if host_result.get("kind").and_then(Value::as_str) != Some("subagent_wait") {
        return Ok(Vec::new());
    }
    let Some(payload) = host_result
        .get("payload")
        .filter(|payload| payload.is_object())
    else {
        return Ok(Vec::new());
    };
    if payload.get("tool").and_then(Value::as_str) != Some(WAIT_AGENT_TOOL) {
        return Ok(Vec::new());
    }
    let Some(agents) = payload.get("agents").and_then(Value::as_array) else {
        return Ok(Vec::new());
    };
    let mut consumed = Vec::new();
    for agent in agents {
        let Some(agent) = agent.as_object() else {
            continue;
        };
        let Some(task_id_text) = agent
            .get(SUBAGENT_TASK_ID_KEY)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        let Some(execution_id) = agent
            .get("execution_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        let task_session_id = task_id_text.parse::<SessionId>().map_err(|error| {
            format!(
                "wait_agent consumed payload has invalid task_session_id `{task_id_text}`: {error}"
            )
        })?;
        consumed.push((task_session_id, execution_id.to_string()));
    }
    Ok(consumed)
}

/// Builds one display-only child transcript row from a parent-to-child send_message payload.
/// 根据父级发往子任务的 send_message 载荷构建一条仅显示的子转录行。
fn build_webui_subagent_send_message(
    projection: SubagentSendMessageProjection,
) -> SessionMessageEntry {
    let now = current_unix_millis_text();
    let mut entry =
        SessionMessageEntry::new(SessionMessageKind::User, "智能体消息", &projection.message)
            .with_runtime_metadata(projection.message_id, now);
    entry.record_kind = SessionMessageRecordKind::DisplayNote;
    entry.metadata = Some(projection.display_metadata);
    entry
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
    let Some(execution_status) = task_session_execution_status_from_tool_metadata(metadata) else {
        webui_trace(
            "subagent_parent_tool_metadata_status_missing",
            format!("parent={parent_session_id}"),
        );
        return;
    };
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
