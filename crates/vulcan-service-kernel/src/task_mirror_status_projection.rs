//! task mirror status projection helpers.
//! task mirror status projection 辅助逻辑。

use super::*;

/// Lightweight task mirrors for the parent session, or an empty list when no segmented session exists.
/// 父会话的轻量任务镜像；没有分段会话时返回空列表。
pub(crate) fn load_segmented_session_task_mirrors_for_service(
    controller_runtime: &ControllerRuntimeHandle,
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
) -> Result<Vec<SessionTaskMirrorSnapshot>, String> {
    let Some(session_dir) = segmented_session_directory_for_workspace_session(
        &controller_runtime.data_root,
        workspace_snapshot,
        session_id,
    ) else {
        return Ok(Vec::new());
    };
    vulcan_workspace_store::workspace_store::load_segmented_session_task_mirrors_from_directory(
        &session_dir,
    )
}

/// Applies lightweight persisted child-task mirror states to parent spawn-agent tool cards.
/// 将持久化轻量子任务镜像状态应用到父级 spawn_agent 工具卡片。
///
/// # Parameters
/// 参数
/// `controller_runtime` provides the data root used to read parent session metadata.
/// `controller_runtime` 提供读取父会话元数据所需的数据根目录。
/// `workspace_snapshot` owns the already hydrated parent transcript window.
/// `workspace_snapshot` 持有已经水合的父会话转录窗口。
/// `parent_session_id` identifies the parent session whose tool cards should be repaired.
/// `parent_session_id` 标识需要修正工具卡片的父会话。
/// `is_task_live_running` reports whether the task still has a real in-process runtime.
/// `is_task_live_running` 表示该任务是否仍有真实进程内运行态。
///
/// # Returns
/// 返回值
/// `Ok(())` after best-effort state repair; missing mirrors are treated as no-op.
/// 尽力修复状态后返回 `Ok(())`；缺少镜像时视为无操作。
pub(crate) fn apply_persisted_task_mirror_statuses_to_workspace<F>(
    controller_runtime: &ControllerRuntimeHandle,
    workspace_snapshot: &mut WorkspaceSnapshot,
    parent_session_id: SessionId,
    is_task_live_running: F,
) -> Result<(), String>
where
    F: FnMut(SessionId) -> bool,
{
    let task_mirrors = load_segmented_session_task_mirrors_for_service(
        controller_runtime,
        workspace_snapshot,
        parent_session_id,
    )?;
    if task_mirrors.is_empty() {
        return Ok(());
    }
    let Some(messages) = workspace_snapshot
        .session_messages
        .get_mut(&parent_session_id)
    else {
        return Ok(());
    };
    let runtime_state = workspace_snapshot
        .session_runtime_state
        .get_mut(&parent_session_id);
    apply_task_mirror_statuses_to_parent_messages(
        messages.as_mut_slice(),
        runtime_state,
        task_mirrors.as_slice(),
        is_task_live_running,
    );
    Ok(())
}

/// Applies task mirror status rows to already loaded parent transcript messages.
/// 将任务镜像状态行应用到已加载的父级转录消息。
///
/// # Parameters
/// 参数
/// `messages` is the mutable projected parent transcript window.
/// `messages` 是可变的父级投影转录窗口。
/// `runtime_state` optionally carries canonical parent conversation records to repair in parallel.
/// `runtime_state` 可选携带需要同步修复的父级规范会话记录。
/// `task_mirrors` are lightweight parent metadata rows and never include child transcript bodies.
/// `task_mirrors` 是父元数据中的轻量行，不包含子任务转录正文。
/// `is_task_live_running` reports whether a task is truly running in the current process.
/// `is_task_live_running` 表示任务在当前进程中是否真实运行。
pub(crate) fn apply_task_mirror_statuses_to_parent_messages<F>(
    messages: &mut [SessionMessageEntry],
    mut runtime_state: Option<&mut SessionRuntimeStateEntry>,
    task_mirrors: &[SessionTaskMirrorSnapshot],
    mut is_task_live_running: F,
) where
    F: FnMut(SessionId) -> bool,
{
    for task_mirror in task_mirrors {
        let execution_status = normalized_task_mirror_execution_status(
            task_mirror.status.as_str(),
            is_task_live_running(task_mirror.task_session_id),
        );
        let mirror_update = task_session_mirror_display_metadata_update(
            task_mirror.task_session_id,
            execution_status.as_str(),
            task_mirror.line_count,
            task_mirror.updated_at.as_str(),
        );
        let message_ids = find_task_session_tool_message_ids_for_payload(
            messages.iter(),
            runtime_state
                .as_deref()
                .map(|state| state.conversation_state.records.iter())
                .into_iter()
                .flatten(),
            &mirror_update.payload,
        );
        if message_ids.is_empty() {
            continue;
        }
        let live_running_override = execution_status == "running";
        for message_id in message_ids {
            apply_task_mirror_metadata_update_to_parent_rows(
                messages,
                runtime_state.as_deref_mut(),
                &message_id,
                &mirror_update.metadata_update,
                live_running_override,
            );
        }
    }
}

/// Applies one task-mirror metadata update to matching projected and canonical parent rows.
/// 将单个任务镜像元数据更新应用到匹配的父级投影行与规范行。
///
/// # Parameters
/// 参数
/// `messages` is the mutable projected parent transcript window.
/// `messages` 是可变父级投影转录窗口。
/// `runtime_state` optionally carries canonical parent conversation records.
/// `runtime_state` 可选携带父级规范会话记录。
/// `message_id` is the task-card row id to update in both stores.
/// `message_id` 是需要在两个存储中更新的任务卡片行标识。
/// `metadata_update` is the already normalized task-mirror display metadata.
/// `metadata_update` 是已规范化的任务镜像展示元数据。
/// `live_running_override` preserves live runtime state over stale terminal metadata.
/// `live_running_override` 保留实时运行态对陈旧终态元数据的覆盖。
///
/// # Returns
/// 返回值
/// No value; matching rows are updated in place.
/// 无返回值；匹配行会被原地更新。
fn apply_task_mirror_metadata_update_to_parent_rows(
    messages: &mut [SessionMessageEntry],
    runtime_state: Option<&mut SessionRuntimeStateEntry>,
    message_id: &str,
    metadata_update: &Value,
    live_running_override: bool,
) {
    for message in messages
        .iter_mut()
        .filter(|message| message.id == message_id)
    {
        apply_task_mirror_metadata_update_to_slot(
            &mut message.metadata,
            metadata_update,
            live_running_override,
        );
    }
    if let Some(runtime_state) = runtime_state {
        for record in runtime_state
            .conversation_state
            .records
            .iter_mut()
            .filter(|record| record.id == message_id)
        {
            apply_task_mirror_metadata_update_to_slot(
                &mut record.metadata,
                metadata_update,
                live_running_override,
            );
        }
    }
}

/// Merges one task-mirror metadata update into a message or record metadata slot.
/// 将单个任务镜像元数据更新合并进消息或记录的元数据槽位。
///
/// # Parameters
/// 参数
/// `metadata_slot` is the mutable metadata field from a projected message or canonical record.
/// `metadata_slot` 是来自投影消息或规范记录的可变元数据字段。
/// `metadata_update` is the display metadata update to merge.
/// `metadata_update` 是需要合并的展示元数据更新。
/// `live_running_override` preserves live runtime state over stale terminal metadata.
/// `live_running_override` 保留实时运行态对陈旧终态元数据的覆盖。
///
/// # Returns
/// 返回值
/// No value; the metadata slot is replaced only when merging produces metadata.
/// 无返回值；仅当合并产出元数据时替换元数据槽位。
fn apply_task_mirror_metadata_update_to_slot(
    metadata_slot: &mut Option<Value>,
    metadata_update: &Value,
    live_running_override: bool,
) {
    if let Some(updated_metadata) = merge_task_mirror_tool_display_metadata(
        metadata_slot.clone(),
        metadata_update.clone(),
        live_running_override,
    ) {
        *metadata_slot = Some(updated_metadata);
    }
}

/// Repairs one display-only surface window with lightweight parent task mirrors.
/// 使用轻量父级任务镜像修复一个仅显示用途的消息窗口。
///
/// # Parameters
/// 参数
/// `guard` provides controller and live task-runtime state.
/// `guard` 提供控制器与实时任务运行态。
/// `session_id` identifies the surface session whose messages are being displayed.
/// `session_id` 标识当前正在显示的界面会话。
/// `messages` is the projected message window to repair in place.
/// `messages` 是需要原地修复的投影消息窗口。
pub(crate) fn repair_task_mirror_statuses_for_surface_messages_locked(
    guard: &ServiceBridgeState,
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
    messages: &mut [SessionMessageEntry],
) {
    if messages.is_empty() {
        return;
    }
    if !surface_contains_task_session_tool_card(messages.iter(), None) {
        return;
    }
    let Some(controller_runtime) = guard.controller_runtime.as_ref() else {
        return;
    };
    let Ok(task_mirrors) = load_segmented_session_task_mirrors_for_service(
        controller_runtime,
        workspace_snapshot,
        session_id,
    ) else {
        return;
    };
    if task_mirrors.is_empty() {
        return;
    }
    apply_task_mirror_statuses_to_parent_messages(
        messages,
        None,
        task_mirrors.as_slice(),
        |task_id| {
            webui_subagent_task_has_live_run_in_workspace(
                Some(workspace_snapshot),
                guard
                    .agent_runtime
                    .as_ref()
                    .map(|runtime| &runtime.task_runtime),
                guard.detached_subagent_task_listeners.contains(&task_id),
                task_id,
            )
        },
    );
}

/// Finds every parent task-card message id that belongs to one task-session payload.
/// 查找属于同一 task-session 载荷的所有父级任务卡片消息标识。
///
/// # Parameters
/// 参数
/// `messages` yields projected parent transcript messages.
/// `messages` 产出父级转录投影消息。
/// `records` yields canonical parent conversation records.
/// `records` 产出父级规范会话记录。
/// `payload` is the task-session payload used for matching.
/// `payload` 是用于匹配的 task-session 载荷。
///
/// # Returns
/// 返回值
/// Stable set of every matching task-card message id.
/// 返回所有匹配任务卡片消息标识的稳定集合。
pub(crate) fn find_task_session_tool_message_ids_for_payload<'a>(
    messages: impl Iterator<Item = &'a SessionMessageEntry>,
    records: impl Iterator<Item = &'a SessionConversationRecord>,
    payload: &Value,
) -> BTreeSet<String> {
    let mut message_ids = BTreeSet::new();
    for message in messages {
        if task_session_tool_metadata_matches_payload(
            message.record_kind,
            message.metadata.as_ref(),
            payload,
        ) {
            message_ids.insert(message.id.clone());
        }
    }
    for record in records {
        if task_session_tool_metadata_matches_payload(
            record.record_kind,
            record.metadata.as_ref(),
            payload,
        ) {
            message_ids.insert(record.id.clone());
        }
    }
    message_ids
}

/// Returns whether the current surface contains any visible task-session tool card.
/// 返回当前界面是否包含任何可见 task-session 工具卡片。
///
/// # Parameters
/// 参数
/// `messages` yields projected transcript messages that may contain tool-display metadata.
/// `messages` 产出可能包含工具展示元数据的投影转录消息。
/// `runtime_state` optionally carries canonical conversation records for the same surface.
/// `runtime_state` 可选携带同一界面的规范会话记录。
///
/// # Returns
/// 返回值
/// `true` when a visible tool row has parseable task-session metadata.
/// 当可见工具行拥有可解析的 task-session 元数据时返回 `true`。
pub(crate) fn surface_contains_task_session_tool_card<'a>(
    messages: impl Iterator<Item = &'a SessionMessageEntry>,
    runtime_state: Option<&SessionRuntimeStateEntry>,
) -> bool {
    messages
        .into_iter()
        .any(session_message_is_task_session_tool_card)
        || runtime_state.is_some_and(|runtime_state| {
            runtime_state
                .conversation_state
                .records
                .iter()
                .any(session_record_is_task_session_tool_card)
        })
}

/// Returns whether one projected message is a visible task-session tool card.
/// 返回一条投影消息是否是可见 task-session 工具卡片。
///
/// # Parameters
/// 参数
/// `message` is the projected transcript message to inspect.
/// `message` 是待检查的投影转录消息。
///
/// # Returns
/// 返回值
/// `true` when the message kind can display status and metadata has a task-session payload.
/// 当消息类型可展示状态且元数据包含 task-session 载荷时返回 `true`。
pub(crate) fn session_message_is_task_session_tool_card(message: &SessionMessageEntry) -> bool {
    task_session_metadata_represents_tool_card(message.record_kind, message.metadata.as_ref())
}

/// Returns whether one canonical record is a visible task-session tool card.
/// 返回一条规范记录是否是可见 task-session 工具卡片。
///
/// # Parameters
/// 参数
/// `record` is the canonical conversation record to inspect.
/// `record` 是待检查的规范会话记录。
///
/// # Returns
/// 返回值
/// `true` when the record kind can display status and metadata has a task-session payload.
/// 当记录类型可展示状态且元数据包含 task-session 载荷时返回 `true`。
pub(crate) fn session_record_is_task_session_tool_card(record: &SessionConversationRecord) -> bool {
    task_session_metadata_represents_tool_card(record.record_kind, record.metadata.as_ref())
}

/// Returns whether one metadata value belongs to a visible task-session tool card.
/// 返回单个元数据值是否属于可见 task-session 工具卡片。
///
/// # Parameters
/// 参数
/// `record_kind` is the transcript record kind attached to the message or record.
/// `record_kind` 是消息或记录携带的转录记录类型。
/// `metadata` is the optional tool metadata attached to the message or record.
/// `metadata` 是消息或记录携带的可选工具元数据。
///
/// # Returns
/// 返回值
/// `true` when the record kind is displayable and the metadata has a task-session payload.
/// 当记录类型可展示且元数据包含 task-session 载荷时返回 `true`。
fn task_session_metadata_represents_tool_card(
    record_kind: SessionMessageRecordKind,
    metadata: Option<&Value>,
) -> bool {
    task_session_record_kind_can_display_status(record_kind)
        && metadata
            .is_some_and(|metadata| task_session_payload_from_tool_metadata(metadata).is_some())
}

/// Returns whether one metadata value matches a task-session payload on a displayable row.
/// 返回单个元数据值是否在可展示行上匹配 task-session 载荷。
///
/// # Parameters
/// 参数
/// `record_kind` is the transcript record kind attached to the message or record.
/// `record_kind` 是消息或记录携带的转录记录类型。
/// `metadata` is the optional tool metadata attached to the message or record.
/// `metadata` 是消息或记录携带的可选工具元数据。
/// `payload` is the task-session payload used for identity matching.
/// `payload` 是用于身份匹配的 task-session 载荷。
///
/// # Returns
/// 返回值
/// `true` when the row can display status and the metadata matches the payload.
/// 当该行可展示状态且元数据匹配载荷时返回 `true`。
fn task_session_tool_metadata_matches_payload(
    record_kind: SessionMessageRecordKind,
    metadata: Option<&Value>,
    payload: &Value,
) -> bool {
    task_session_record_kind_can_display_status(record_kind)
        && metadata.is_some_and(|metadata| task_session_message_match_score(metadata, payload) > 0)
}

/// Returns whether one record kind can be a visible task-session tool row.
/// 返回某记录类型是否可能是可见的 task-session 工具行。
pub(crate) fn task_session_record_kind_can_display_status(
    record_kind: SessionMessageRecordKind,
) -> bool {
    matches!(
        record_kind,
        SessionMessageRecordKind::ToolCallOutput | SessionMessageRecordKind::ToolResultOutput
    )
}

/// Resolves display status for one task mirror, letting live runtime state win over stale metadata.
/// 为单条任务镜像解析展示状态，并让实时运行态覆盖陈旧元数据。
///
/// # Parameters
/// 参数
/// `status` is the status text mirrored in parent metadata.
/// `status` 是父级元数据镜像中的状态文本。
/// `is_live_running` tells whether the current process still owns active task work.
/// `is_live_running` 表示当前进程是否仍持有活动任务。
///
/// # Returns
/// 返回值
/// A canonical display execution status.
/// 规范化后的展示执行状态。
pub(crate) fn normalized_task_mirror_execution_status(
    status: &str,
    is_live_running: bool,
) -> String {
    if is_live_running {
        return String::from("running");
    }
    match status.trim().to_ascii_lowercase().as_str() {
        "failed" | "error" | "errored" => String::from("failed"),
        "stopped" | "cancelled" | "canceled" | "aborted" => String::from("stopped"),
        "completed" | "complete" | "success" | "succeeded" => String::from("completed"),
        "running" | "created" | "pending" | "in_progress" => String::from("stopped"),
        _ => String::from("stopped"),
    }
}

/// Returns a cloned metadata object for one WebUI task-session parent message.
/// 返回一条 WebUI task-session 父级消息的元数据克隆。
pub(crate) fn webui_task_session_tool_message_metadata(
    workspace_snapshot: &WorkspaceSnapshot,
    parent_session_id: SessionId,
    message_id: &str,
) -> Option<Value> {
    workspace_snapshot
        .session_messages
        .get(&parent_session_id)
        .and_then(|messages| messages.iter().find(|message| message.id == message_id))
        .and_then(|message| message.metadata.clone())
        .or_else(|| {
            workspace_snapshot
                .session_runtime_state
                .get(&parent_session_id)
                .and_then(|runtime_state| {
                    runtime_state
                        .conversation_state
                        .records
                        .iter()
                        .find(|record| record.id == message_id)
                        .and_then(|record| record.metadata.clone())
                })
        })
}

/// Marks one WebUI subagent task as stopped in the child transcript and parent task card.
/// 在子转录与父级 task 卡片中将单个 WebUI 子代理任务标记为已停止。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` is the retained WebUI workspace snapshot to mutate.
/// `workspace_snapshot` 是需要变更的 WebUI 保留工作区快照。
/// `parent_session_id` owns the visible task card.
/// `parent_session_id` 拥有可见 task 卡片。
/// `task_id` is the child task session identifier.
/// `task_id` 是子任务会话标识。
/// `state_snapshot` is the latest cached subagent state when available.
/// `state_snapshot` 是可用时最近缓存的子代理状态。
pub(crate) fn mark_webui_subagent_task_stopped_in_workspace(
    workspace_snapshot: &mut WorkspaceSnapshot,
    parent_session_id: SessionId,
    task_id: SessionId,
    state_snapshot: Option<&SubagentTaskPanelState>,
) {
    append_webui_subagent_stop_notice_if_needed(workspace_snapshot, task_id);
    let payload = json!({
        "task_id": task_id.to_string(),
        (SUBAGENT_TASK_ID_KEY): task_id.to_string(),
    });
    let Some(message_id) = find_webui_task_session_tool_message_id_for_payload(
        workspace_snapshot,
        parent_session_id,
        &payload,
    ) else {
        return;
    };
    let current_metadata = webui_task_session_tool_message_metadata(
        workspace_snapshot,
        parent_session_id,
        &message_id,
    );
    let updated_at = current_unix_millis_text();
    let updated_metadata = build_stopped_task_session_metadata(
        current_metadata.as_ref(),
        state_snapshot,
        task_id,
        SUBAGENT_TASK_STOPPED_SUMMARY_TEXT,
        &updated_at,
    );
    if let Err(error) = merge_webui_task_session_tool_message_metadata(
        workspace_snapshot,
        parent_session_id,
        &message_id,
        updated_metadata,
        "stopped",
    ) {
        webui_trace(
            "subagent_stop_parent_tool_metadata_merge_failed",
            format!(
                "parent={parent_session_id} task={task_id} message_id={message_id} error={error}"
            ),
        );
    }
}

/// Finds the best parent task-card message id for a WebUI task-session payload.
/// 为 WebUI task-session 载荷查找最匹配的父级 task 卡片消息标识。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` provides projected messages and canonical runtime records.
/// `workspace_snapshot` 提供投影消息与规范运行时记录。
/// `parent_session_id` selects the parent session to scan.
/// `parent_session_id` 选择需要扫描的父会话。
/// `payload` is the task-session payload to match.
/// `payload` 是需要匹配的 task-session 载荷。
///
/// # Returns
/// 返回值
/// The best matching message id when one can be found.
/// 找到匹配项时返回最佳消息标识。
pub(crate) fn find_webui_task_session_tool_message_id_for_payload(
    workspace_snapshot: &WorkspaceSnapshot,
    parent_session_id: SessionId,
    payload: &Value,
) -> Option<String> {
    find_task_session_tool_message_id_for_payload(
        reversed_webui_task_session_messages(workspace_snapshot, parent_session_id),
        reversed_webui_task_session_records(workspace_snapshot, parent_session_id),
        payload,
    )
}

/// Finds every WebUI parent task-card message id for one task-session payload.
/// 为单个 task-session 载荷查找所有 WebUI 父级任务卡片消息标识。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` provides projected messages and canonical runtime records.
/// `workspace_snapshot` 提供投影消息与规范运行时记录。
/// `parent_session_id` selects the parent session to scan.
/// `parent_session_id` 选择需要扫描的父会话。
/// `payload` is the task-session payload to match.
/// `payload` 是需要匹配的 task-session 载荷。
///
/// # Returns
/// 返回值
/// Stable set of every matching WebUI parent task-card message id.
/// 返回所有匹配 WebUI 父级任务卡片消息标识的稳定集合。
pub(crate) fn find_webui_task_session_tool_message_ids_for_payload(
    workspace_snapshot: &WorkspaceSnapshot,
    parent_session_id: SessionId,
    payload: &Value,
) -> BTreeSet<String> {
    find_task_session_tool_message_ids_for_payload(
        reversed_webui_task_session_messages(workspace_snapshot, parent_session_id),
        reversed_webui_task_session_records(workspace_snapshot, parent_session_id),
        payload,
    )
}

/// Returns projected WebUI parent messages in newest-first order for task-card lookup.
/// 返回用于 task 卡片查找的 WebUI 父级投影消息，顺序为最新优先。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` provides the retained WebUI transcript projection.
/// `workspace_snapshot` 提供保留的 WebUI 转录投影。
/// `parent_session_id` selects the parent session that owns the task card.
/// `parent_session_id` 选择拥有 task 卡片的父会话。
///
/// # Returns
/// 返回值
/// An iterator over projected parent messages from newest to oldest.
/// 返回从新到旧遍历父级投影消息的迭代器。
fn reversed_webui_task_session_messages(
    workspace_snapshot: &WorkspaceSnapshot,
    parent_session_id: SessionId,
) -> impl Iterator<Item = &SessionMessageEntry> + '_ {
    workspace_snapshot
        .session_messages
        .get(&parent_session_id)
        .into_iter()
        .flat_map(|messages| messages.iter().rev())
}

/// Returns canonical WebUI parent records in newest-first order for task-card lookup.
/// 返回用于 task 卡片查找的 WebUI 父级规范记录，顺序为最新优先。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` provides the retained WebUI runtime state.
/// `workspace_snapshot` 提供保留的 WebUI 运行时状态。
/// `parent_session_id` selects the parent session that owns the task card.
/// `parent_session_id` 选择拥有 task 卡片的父会话。
///
/// # Returns
/// 返回值
/// An iterator over canonical parent records from newest to oldest.
/// 返回从新到旧遍历父级规范记录的迭代器。
fn reversed_webui_task_session_records(
    workspace_snapshot: &WorkspaceSnapshot,
    parent_session_id: SessionId,
) -> impl Iterator<Item = &SessionConversationRecord> + '_ {
    workspace_snapshot
        .session_runtime_state
        .get(&parent_session_id)
        .into_iter()
        .flat_map(|runtime_state| runtime_state.conversation_state.records.iter().rev())
}

/// Replaces one child task transcript with the canonical conversation returned by the subagent.
/// 使用子代理返回的规范会话替换单个子任务转录。
pub(crate) fn replace_webui_subagent_task_conversation_state(
    workspace_snapshot: &mut WorkspaceSnapshot,
    task_id: SessionId,
    conversation_state: vulcan_agent_runtime::conversation_protocol::SessionConversationState,
    task_prompt_anchor: Option<&str>,
) {
    let now = current_unix_millis_text();
    let runtime_state = workspace_snapshot
        .session_runtime_state
        .entry(task_id)
        .or_insert_with(|| SessionRuntimeStateEntry {
            updated_at: now.clone(),
            ..SessionRuntimeStateEntry::default()
        });
    runtime_state.conversation_state = conversation_state;
    runtime_state.updated_at = now;
    let runtime_state_snapshot = runtime_state.clone();
    replace_task_display_messages_with_runtime_state_records(
        workspace_snapshot,
        task_id,
        &runtime_state_snapshot,
        task_prompt_anchor,
    );
    sync_hydrated_session_dialogue_ready_marker(workspace_snapshot, task_id);
}
