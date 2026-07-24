//! Session hydration loading helpers.
//! 会话水合 loading 辅助逻辑。

use super::*;

pub(crate) async fn load_workspace_for_submission(
    controller_runtime: ControllerRuntimeHandle,
    session_id: Option<SessionId>,
) -> Result<WorkspaceSnapshot, String> {
    // Measures lightweight workspace index loading for prompt submission.
    // 衡量提示词提交时轻量工作区索引载入耗时。
    let workspace_load_started_at = current_unix_millis_value();
    let mut workspace_snapshot =
        vulcan_workspace_store::workspace_store::load_workspace_index_snapshot(
            controller_runtime.clone(),
        )
        .await?;
    webui_trace_if_slow(
        "submit_prompt_workspace_snapshot_load_slow",
        workspace_load_started_at,
        format!(
            "conversation_id={}",
            session_id
                .map(|session_id| session_id.to_string())
                .as_deref()
                .unwrap_or("<draft>")
        ),
    );
    if let Some(session_id) = session_id {
        // Measures selected session hydration when retained context cannot be reused.
        // 衡量无法复用保留上下文时的选中会话水合耗时。
        let hydration_started_at = current_unix_millis_value();
        let hydration = if let Some((project_id, created_at)) =
            session_project_created_at_for_hydration(&workspace_snapshot, session_id)
        {
            vulcan_workspace_store::workspace_store::load_session_hydration_snapshot_for_project_created_at(
                    controller_runtime.clone(),
                    &project_id,
                    &created_at,
                    session_id,
                    SELECTED_SESSION_TURN_LOAD_LIMIT,
                )
                .await
        } else {
            Ok(empty_session_hydration_snapshot(session_id))
        };
        match hydration {
            Ok(hydration) => {
                apply_submission_session_hydration(
                    &controller_runtime,
                    &mut workspace_snapshot,
                    session_id,
                    hydration,
                )?;
            }
            Err(error) if is_missing_segmented_session_file_error(&error) => {}
            Err(error) => return Err(error),
        }
        webui_trace_if_slow(
            "submit_prompt_session_hydration_slow",
            hydration_started_at,
            format!("session={session_id}"),
        );
    }
    Ok(workspace_snapshot)
}

/// Returns deterministic session location fields from one retained workspace snapshot.
/// 从保留的工作区快照返回确定性的会话定位字段。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` contains the retained session summaries.
/// `workspace_snapshot` 包含保留的会话摘要。
/// `session_id` selects the session to locate.
/// `session_id` 选择需要定位的会话。
///
/// # Returns
/// 返回值
/// Owned project id and raw created-at timestamp when both are present on the summary.
/// 当摘要同时具备项目标识与原始创建时间戳时返回它们的自有副本。
fn session_project_created_at_for_hydration(
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
) -> Option<(String, String)> {
    workspace_session_summary_for_session(workspace_snapshot, session_id)
        .and_then(session_summary_project_and_created_at)
        .map(|(project_id, created_at)| (project_id.to_string(), created_at.to_string()))
}

/// Returns whether a retained WebUI snapshot can be used as the submit base snapshot.
/// 判断保留的 WebUI 快照能否作为本次提交的基础快照。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` is the in-memory snapshot retained by the service bridge.
/// `workspace_snapshot` 是服务桥保留的内存工作区快照。
/// `session_id` optionally selects an existing submitted conversation.
/// `session_id` 可选选择本次提交的既有会话。
///
/// # Returns
/// 返回值
/// `true` for a draft submission or when the existing session is active in the retained snapshot.
/// 草稿提交或既有会话在保留快照中处于活动列表时返回 `true`。
pub(crate) fn retained_submission_snapshot_is_usable(
    workspace_snapshot: &WorkspaceSnapshot,
    project_id: Option<&str>,
    session_id: Option<SessionId>,
) -> bool {
    match session_id {
        Some(session_id) => {
            workspace_session_summary_for_session(workspace_snapshot, session_id).is_some()
        }
        None => project_id.map_or_else(
            || !workspace_snapshot.task_projects.is_empty(),
            |project_id| workspace_snapshot_contains_active_project(workspace_snapshot, project_id),
        ),
    }
}

/// Loads the selected-session display hydration snapshot with a missing-file fallback.
/// 加载选中会话的显示层水合快照，并在分段文件缺失时回退为空快照。
pub(crate) async fn load_selected_session_display_hydration(
    controller_runtime: ControllerRuntimeHandle,
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
) -> Result<SessionHydrationSnapshot, String> {
    if let Some((project_id, created_at)) =
        session_project_created_at_for_hydration(workspace_snapshot, session_id)
    {
        vulcan_workspace_store::workspace_store::load_session_display_hydration_snapshot_for_project_created_at(
                controller_runtime,
                &project_id,
                &created_at,
                session_id,
                SELECTED_SESSION_TURN_LOAD_LIMIT,
            )
            .await
            .or_else(|error| {
                if is_missing_segmented_session_file_error(&error) {
                    Ok(empty_session_hydration_snapshot(session_id))
                } else {
                    Err(error)
                }
            })
    } else {
        Ok(empty_session_hydration_snapshot(session_id))
    }
}

/// Applies display hydration data to the selected workspace session.
/// 将显示层水合数据应用到选中的工作区会话。
pub(crate) fn apply_hydration_to_selected_workspace_snapshot(
    controller_runtime: &ControllerRuntimeHandle,
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
    hydration: SessionHydrationSnapshot,
    task_runtime: Option<&Arc<TaskRuntime>>,
    live_listener_task_ids: &BTreeSet<SessionId>,
) -> Result<(), String> {
    let SessionHydrationSnapshot {
        messages,
        tool_detail_messages,
        compactions,
        runtime_state,
        ..
    } = hydration;
    let has_live_runtime_owner = webui_subagent_task_has_live_run_for_phase(
        workspace_snapshot
            .session_run_phases
            .get(&session_id)
            .copied(),
        task_runtime,
        live_listener_task_ids.contains(&session_id),
        session_id,
    );
    let mut messages = hydration_messages_with_tool_details(messages, tool_detail_messages);
    if !has_live_runtime_owner {
        normalize_orphaned_tool_request_messages(&mut messages);
    }
    workspace_snapshot
        .session_messages
        .insert(session_id, messages);
    workspace_snapshot
        .session_compactions
        .insert(session_id, compactions);
    if let Some(mut runtime_state) = runtime_state {
        if !has_live_runtime_owner {
            let now = current_unix_millis_value().to_string();
            normalize_orphaned_tool_request_records(&mut runtime_state, &now);
        }
        workspace_snapshot
            .session_runtime_state
            .insert(session_id, runtime_state);
    }
    // Display hydration may legally observe a newly created live session before provider dialogue context is persisted.
    // 显示层水合可能合法地观察到尚未持久化 Provider 对话上下文的新建实时会话。
    sync_hydrated_session_dialogue_ready_marker(workspace_snapshot, session_id);
    let hydrated_session_run_phases = workspace_snapshot.session_run_phases.clone();
    apply_persisted_task_mirror_statuses_to_workspace(
        controller_runtime,
        workspace_snapshot,
        session_id,
        |task_id| {
            webui_subagent_task_has_live_run_for_phase(
                hydrated_session_run_phases.get(&task_id).copied(),
                task_runtime,
                live_listener_task_ids.contains(&task_id),
                task_id,
            )
        },
    )?;
    Ok(())
}

/// Builds an empty hydration payload for a missing segmented session file.
/// 为缺失的分段会话文件构建空水合载荷。
pub(crate) fn empty_session_hydration_snapshot(session_id: SessionId) -> SessionHydrationSnapshot {
    SessionHydrationSnapshot {
        session_id,
        messages: Vec::new(),
        tool_detail_messages: Vec::new(),
        compactions: Vec::new(),
        runtime_state: None,
        earliest_ordinal: None,
        has_more_history: false,
    }
}

/// Hydrates the submitted retained session only when the retained snapshot lacks concrete context.
/// 仅在保留快照缺少具体上下文时水合本次提交的目标会话。
///
/// # Parameters
/// 参数
/// `controller_runtime` owns the workspace-store runtime bridge.
/// `controller_runtime` 持有工作区存储运行时桥。
/// `workspace_snapshot` is the retained WebUI workspace snapshot being submitted.
/// `workspace_snapshot` 是本次提交正在使用的保留 WebUI 工作区快照。
/// `session_id` selects the submitted existing session.
/// `session_id` 选择本次提交的既有会话。
///
/// # Returns
/// 返回值
/// `Ok(())` after the session either already had context or has been hydrated.
/// 当会话已具备上下文或已完成水合后返回 `Ok(())`。
pub(crate) async fn hydrate_retained_submission_session_if_needed(
    controller_runtime: ControllerRuntimeHandle,
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
) -> Result<(), String> {
    if retained_submission_snapshot_has_session_context(workspace_snapshot, session_id) {
        return Ok(());
    }
    let hydration_started_at = current_unix_millis_value();
    let hydration_result = if let Some((project_id, created_at)) =
        session_project_created_at_for_hydration(workspace_snapshot, session_id)
    {
        vulcan_workspace_store::workspace_store::load_session_hydration_snapshot_for_project_created_at(
                controller_runtime.clone(),
                &project_id,
                &created_at,
                session_id,
                SELECTED_SESSION_TURN_LOAD_LIMIT,
            )
            .await
    } else {
        Ok(empty_session_hydration_snapshot(session_id))
    };
    match hydration_result {
        Ok(hydration) => {
            apply_submission_session_hydration(
                &controller_runtime,
                workspace_snapshot,
                session_id,
                hydration,
            )?;
        }
        Err(error) if is_missing_segmented_session_file_error(&error) => {}
        Err(error) => return Err(error),
    }
    ensure_hydrated_session_dialogue_context(workspace_snapshot, session_id)?;
    webui_trace_if_slow(
        "submit_prompt_retained_session_hydration_slow",
        hydration_started_at,
        format!("session={session_id}"),
    );
    Ok(())
}

/// Hydrates the selected session runtime context when the retained snapshot has no dialogue memory.
/// 当保留快照缺少对话记忆时，水合选中会话的运行态上下文。
///
/// # Parameters
/// 参数
/// `controller_runtime` owns the workspace-store runtime bridge.
/// `controller_runtime` 持有工作区存储运行时桥。
/// `workspace_snapshot` is the retained WebUI workspace snapshot that receives runtime memory.
/// `workspace_snapshot` 是接收运行态记忆的保留 WebUI 工作区快照。
/// `session_id` selects the session whose runtime JSONL should be hydrated.
/// `session_id` 选择需要从运行时 JSONL 水合的会话。
///
/// # Returns
/// 返回值
/// `Ok(())` after runtime context exists or a concrete hydration error has been reported.
/// 当运行态上下文已存在或已报告明确水合错误后返回 `Ok(())`。
pub(crate) async fn hydrate_selected_session_runtime_if_needed(
    controller_runtime: ControllerRuntimeHandle,
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
) -> Result<(), String> {
    if retained_submission_snapshot_has_session_context(workspace_snapshot, session_id) {
        return Ok(());
    }
    let hydration_result = if let Some((project_id, created_at)) =
        session_project_created_at_for_hydration(workspace_snapshot, session_id)
    {
        vulcan_workspace_store::workspace_store::load_session_hydration_snapshot_for_project_created_at(
                controller_runtime.clone(),
                &project_id,
                &created_at,
                session_id,
                SELECTED_SESSION_TURN_LOAD_LIMIT,
            )
            .await
    } else {
        Ok(empty_session_hydration_snapshot(session_id))
    };
    match hydration_result {
        Ok(hydration) => {
            apply_selected_session_runtime_hydration(workspace_snapshot, session_id, hydration)?;
        }
        Err(error) if is_missing_segmented_session_file_error(&error) => {}
        Err(error) => return Err(error),
    }
    ensure_hydrated_session_dialogue_context(workspace_snapshot, session_id)?;
    Ok(())
}

/// Applies a selected-session hydration snapshot without replacing an existing display window.
/// 应用选中会话水合快照，同时不替换已有显示窗口。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` receives the hydrated runtime context.
/// `workspace_snapshot` 接收水合后的运行态上下文。
/// `session_id` selects the hydrated session.
/// `session_id` 选择被水合的会话。
/// `hydration` contains runtime JSONL and optional display-window data.
/// `hydration` 包含运行时 JSONL 与可选显示窗口数据。
///
/// # Returns
/// 返回值
/// Applies the validated runtime context and hydration marker, or returns the exact replay identity failure.
/// 应用经过校验的运行态上下文与水合标记，或返回准确的重放身份失败。
pub(crate) fn apply_selected_session_runtime_hydration(
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
    hydration: SessionHydrationSnapshot,
) -> Result<(), String> {
    let SessionHydrationSnapshot {
        mut messages,
        mut tool_detail_messages,
        mut runtime_state,
        ..
    } = hydration;
    if let Some(runtime_state) = runtime_state.as_mut() {
        // Runtime-only hydration is performed before a new live owner is installed for this session.
        // 仅运行态水合发生在为该会话安装新的实时所有者之前。
        let now = current_unix_millis_value().to_string();
        normalize_orphaned_tool_request_records(runtime_state, &now);
    }
    normalize_orphaned_tool_request_messages(&mut messages);
    normalize_orphaned_tool_request_messages(&mut tool_detail_messages);
    match workspace_snapshot.session_messages.entry(session_id) {
        std::collections::btree_map::Entry::Vacant(entry) => {
            let messages = hydration_messages_with_tool_details(messages, tool_detail_messages);
            entry.insert(filter_legacy_goal_internal_user_messages(messages));
        }
        std::collections::btree_map::Entry::Occupied(mut entry) => {
            normalize_orphaned_tool_request_messages(entry.get_mut());
            append_unique_session_messages(entry.get_mut(), tool_detail_messages);
        }
    }
    if let Some(runtime_state) = runtime_state {
        workspace_snapshot
            .session_runtime_state
            .insert(session_id, runtime_state);
    }
    sync_hydrated_session_dialogue_ready_marker(workspace_snapshot, session_id);
    Ok(())
}

/// Applies one session hydration snapshot to the submit workspace snapshot.
/// 将单个会话水合快照应用到提交工作区快照。
///
/// # Parameters
/// 参数
/// `controller_runtime` owns the workspace-store runtime bridge.
/// `controller_runtime` 持有工作区存储运行时桥。
/// `workspace_snapshot` receives the hydrated messages and runtime state.
/// `workspace_snapshot` 接收已水合的消息与运行态。
/// `session_id` selects the hydrated session.
/// `session_id` 选择被水合的会话。
/// `hydration` contains the lazy-loaded session data.
/// `hydration` 包含懒加载出的会话数据。
///
/// # Returns
/// 返回值
/// `Ok(())` after hydration data and task mirror statuses have been applied.
/// 应用水合数据与任务镜像状态后返回 `Ok(())`。
pub(crate) fn apply_submission_session_hydration(
    controller_runtime: &ControllerRuntimeHandle,
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
    hydration: SessionHydrationSnapshot,
) -> Result<(), String> {
    let SessionHydrationSnapshot {
        messages,
        tool_detail_messages,
        runtime_state,
        ..
    } = hydration;
    let messages = hydration_messages_with_tool_details(messages, tool_detail_messages);
    let mut messages = filter_legacy_goal_internal_user_messages(messages);
    normalize_orphaned_tool_request_messages(&mut messages);
    workspace_snapshot
        .session_messages
        .insert(session_id, messages);
    if let Some(mut runtime_state) = runtime_state {
        // Disk hydration is reached only when no retained live dialogue context owns this session.
        // 仅当没有保留的实时对话上下文拥有该会话时才会进入磁盘水合。
        let now = current_unix_millis_value().to_string();
        normalize_orphaned_tool_request_records(&mut runtime_state, &now);
        workspace_snapshot
            .session_runtime_state
            .insert(session_id, runtime_state);
    }
    ensure_hydrated_session_dialogue_context(workspace_snapshot, session_id)?;
    sync_hydrated_session_dialogue_ready_marker(workspace_snapshot, session_id);
    apply_persisted_task_mirror_statuses_to_workspace(
        controller_runtime,
        workspace_snapshot,
        session_id,
        |_| false,
    )
}

/// Combines a loaded display window with hidden tool-detail rows from the same load.
/// 将已加载显示窗口与同次加载得到的隐藏工具详情行合并。
///
/// # Parameters
/// 参数
/// `messages` are the primary loaded transcript rows for the visible window.
/// `messages` 是可见窗口对应的主要已加载转录行。
/// `tool_detail_messages` are rows required for expanding visible tool nodes.
/// `tool_detail_messages` 是展开可见工具节点所需的行。
///
/// # Returns
/// 返回值
/// A retained in-memory window containing both display rows and their hidden details.
/// 返回同时包含显示行及其隐藏详情的内存保留窗口。
fn hydration_messages_with_tool_details(
    mut messages: Vec<SessionMessageEntry>,
    tool_detail_messages: Vec<SessionMessageEntry>,
) -> Vec<SessionMessageEntry> {
    append_unique_session_messages(&mut messages, tool_detail_messages);
    messages
}

/// Appends session messages by stable identity without duplicating already retained rows.
/// 按稳定身份追加会话消息，避免重复已保留行。
///
/// # Parameters
/// 参数
/// `messages` is the retained in-memory message window to extend.
/// `messages` 是需要扩展的内存保留消息窗口。
/// `additional_messages` are candidate rows loaded for the same visible window.
/// `additional_messages` 是为同一可见窗口加载的候选行。
fn append_unique_session_messages(
    messages: &mut Vec<SessionMessageEntry>,
    additional_messages: Vec<SessionMessageEntry>,
) {
    let mut seen_keys = messages
        .iter()
        .map(SessionMessageEntry::stable_merge_key)
        .collect::<BTreeSet<_>>();
    for message in additional_messages {
        if seen_keys.insert(message.stable_merge_key()) {
            messages.push(message);
        }
    }
}
