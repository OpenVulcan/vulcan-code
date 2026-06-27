//! Session hydration and refreshed workspace state preservation helpers.
//! 会话水合与刷新后工作区状态保留辅助逻辑。

use std::{collections::BTreeSet, sync::Arc};

use vulcan_agent_runtime::agent_runtime::task_runtime::TaskRuntime;
use vulcan_workbench_model::{
    SessionId, SessionMessageEntry, SessionRunPhase, SessionRuntimeStateEntry, SessionState,
    SessionSummary,
};
use vulcan_workspace_store::{
    data_plane::ControllerRuntimeHandle,
    workspace_store::{SessionHydrationSnapshot, WorkspaceSnapshot},
};

use super::{
    SELECTED_SESSION_TURN_LOAD_LIMIT, apply_persisted_task_mirror_statuses_to_workspace,
    current_unix_millis_value, filter_legacy_goal_internal_user_messages,
    is_active_session_run_phase, owning_project_id_for_session, parse_session_id,
    runtime_state_has_queued_pending_prompt_work,
    sanitize_runtime_state_legacy_goal_internal_messages, webui_subagent_task_has_live_run,
    webui_trace_if_slow, workspace_snapshot_contains_active_project,
    workspace_snapshot_contains_regular_session,
};

/// Minimal retained state needed to repair one refreshed session row.
/// 修复单个刷新后会话行所需的最小保留状态。
#[derive(Debug, Clone)]
struct PreservedSessionRunState {
    /// Session index in the retained sidebar list.
    /// 会话在保留侧栏列表中的位置。
    previous_index: usize,
    /// Project session index in the retained project tree.
    /// 会话在保留项目树中的位置。
    previous_project_index: Option<usize>,
    /// Session summary retained from memory.
    /// 从内存保留的会话摘要。
    session: SessionSummary,
    /// Session run phase retained from memory.
    /// 从内存保留的会话运行阶段。
    previous_phase: Option<SessionRunPhase>,
    /// Whether the runtime kernel still owns an active turn for this session.
    /// 运行时内核是否仍持有该会话的活动回合。
    had_active_run: bool,
    /// Whether the previous workspace already retained a hydrated transcript window for this session.
    /// 之前的工作区是否已经为该会话保留了已水合转录窗口。
    had_hydrated_window: bool,
    /// Runtime state retained from memory when it contains live context.
    /// 包含实时上下文时从内存保留的运行态。
    runtime_state: Option<SessionRuntimeStateEntry>,
    /// Visible messages retained from memory for hydrated or not-yet-persisted sessions.
    /// 为已水合或尚未持久化的会话从内存保留的可见消息。
    messages: Option<Vec<SessionMessageEntry>>,
}

/// Minimal retained workspace state used after a database-backed refresh.
/// 数据库刷新后使用的最小工作区保留状态。
#[derive(Debug, Clone)]
pub(super) struct PreservedWorkspaceRunState {
    /// Selected session retained from memory when it still exists after refresh.
    /// 刷新后仍存在时从内存保留的选中会话。
    selected_session: Option<SessionId>,
    /// Session-scoped state rows that must be restored after refresh.
    /// 刷新后必须恢复的会话级状态行。
    sessions: Vec<PreservedSessionRunState>,
}

impl PreservedWorkspaceRunState {
    /// Returns the number of preserved session rows carried by this snapshot repair state.
    /// 返回该快照修复状态携带的保留会话行数量。
    pub(super) fn session_count(&self) -> usize {
        self.sessions.len()
    }
}

/// Loads the workspace snapshot and hydrates the selected submitted session when it already exists.
/// 加载工作区快照，并在提交目标会话已存在时水合该会话。
pub(super) async fn load_workspace_for_submission(
    controller_runtime: ControllerRuntimeHandle,
    conversation_id: Option<&str>,
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
        format!("conversation_id={}", conversation_id.unwrap_or("<draft>")),
    );
    if let Some(session_id) = conversation_id.and_then(parse_session_id) {
        // Measures selected session hydration when retained context cannot be reused.
        // 衡量无法复用保留上下文时的选中会话水合耗时。
        let hydration_started_at = current_unix_millis_value();
        match vulcan_workspace_store::workspace_store::load_session_hydration_snapshot(
            controller_runtime.clone(),
            session_id,
            SELECTED_SESSION_TURN_LOAD_LIMIT,
        )
        .await
        {
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
/// `true` for a draft submission or when the existing session is present in the retained snapshot.
/// 草稿提交或既有会话存在于保留快照中时返回 `true`。
pub(super) fn retained_submission_snapshot_is_usable(
    workspace_snapshot: &WorkspaceSnapshot,
    project_id: Option<&str>,
    session_id: Option<SessionId>,
) -> bool {
    match session_id {
        Some(session_id) => {
            workspace_snapshot_contains_regular_session(workspace_snapshot, session_id)
        }
        None => project_id.is_some_and(|project_id| {
            workspace_snapshot_contains_active_project(workspace_snapshot, project_id)
        }),
    }
}

/// Loads the selected-session display hydration snapshot with a missing-file fallback.
/// 加载选中会话的显示层水合快照，并在分段文件缺失时回退为空快照。
pub(super) async fn load_selected_session_display_hydration(
    controller_runtime: ControllerRuntimeHandle,
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
) -> Result<SessionHydrationSnapshot, String> {
    let project_id = owning_project_id_for_session(workspace_snapshot, session_id);
    match project_id.as_deref() {
        Some(project_id) => {
            vulcan_workspace_store::workspace_store::load_session_display_hydration_snapshot_for_project(
                controller_runtime,
                project_id,
                session_id,
                SELECTED_SESSION_TURN_LOAD_LIMIT,
            )
            .await
        }
        None => {
            vulcan_workspace_store::workspace_store::load_session_display_hydration_snapshot(
                controller_runtime,
                session_id,
                SELECTED_SESSION_TURN_LOAD_LIMIT,
            )
            .await
        }
    }
    .or_else(|error| {
        if is_missing_segmented_session_file_error(&error) {
            Ok(empty_session_hydration_snapshot(session_id))
        } else {
            Err(error)
        }
    })
}

/// Applies display hydration data to the selected workspace session.
/// 将显示层水合数据应用到选中的工作区会话。
pub(super) fn apply_hydration_to_selected_workspace_snapshot(
    controller_runtime: &ControllerRuntimeHandle,
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
    hydration: SessionHydrationSnapshot,
    task_runtime: Option<&Arc<TaskRuntime>>,
    live_listener_task_ids: &BTreeSet<SessionId>,
) -> Result<(), String> {
    workspace_snapshot
        .session_messages
        .insert(session_id, hydration.messages);
    workspace_snapshot
        .session_compactions
        .insert(session_id, hydration.compactions);
    if let Some(runtime_state) = hydration.runtime_state {
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
        |task_id| {
            webui_subagent_task_has_live_run(
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
fn empty_session_hydration_snapshot(session_id: SessionId) -> SessionHydrationSnapshot {
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
pub(super) async fn hydrate_retained_submission_session_if_needed(
    controller_runtime: ControllerRuntimeHandle,
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
) -> Result<(), String> {
    if retained_submission_snapshot_has_session_context(workspace_snapshot, session_id) {
        return Ok(());
    }
    let hydration_started_at = current_unix_millis_value();
    let hydration_result =
        if let Some(project_id) = owning_project_id_for_session(workspace_snapshot, session_id) {
            vulcan_workspace_store::workspace_store::load_session_hydration_snapshot_for_project(
                controller_runtime.clone(),
                &project_id,
                session_id,
                SELECTED_SESSION_TURN_LOAD_LIMIT,
            )
            .await
        } else {
            vulcan_workspace_store::workspace_store::load_session_hydration_snapshot(
                controller_runtime.clone(),
                session_id,
                SELECTED_SESSION_TURN_LOAD_LIMIT,
            )
            .await
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
pub(super) async fn hydrate_selected_session_runtime_if_needed(
    controller_runtime: ControllerRuntimeHandle,
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
) -> Result<(), String> {
    if retained_submission_snapshot_has_session_context(workspace_snapshot, session_id) {
        return Ok(());
    }
    let hydration_result =
        if let Some(project_id) = owning_project_id_for_session(workspace_snapshot, session_id) {
            vulcan_workspace_store::workspace_store::load_session_hydration_snapshot_for_project(
                controller_runtime.clone(),
                &project_id,
                session_id,
                SELECTED_SESSION_TURN_LOAD_LIMIT,
            )
            .await
        } else {
            vulcan_workspace_store::workspace_store::load_session_hydration_snapshot(
                controller_runtime.clone(),
                session_id,
                SELECTED_SESSION_TURN_LOAD_LIMIT,
            )
            .await
        };
    match hydration_result {
        Ok(hydration) => {
            apply_selected_session_runtime_hydration(workspace_snapshot, session_id, hydration);
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
/// Applies the runtime context and hydration marker without returning a fallible result.
/// 应用运行态上下文与水合标记，不返回可失败结果。
fn apply_selected_session_runtime_hydration(
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
    hydration: SessionHydrationSnapshot,
) {
    if !workspace_snapshot
        .session_messages
        .contains_key(&session_id)
    {
        let messages = filter_legacy_goal_internal_user_messages(hydration.messages);
        workspace_snapshot
            .session_messages
            .insert(session_id, messages);
    }
    if let Some(mut runtime_state) = hydration.runtime_state {
        sanitize_runtime_state_legacy_goal_internal_messages(&mut runtime_state);
        workspace_snapshot
            .session_runtime_state
            .insert(session_id, runtime_state);
    }
    sync_hydrated_session_dialogue_ready_marker(workspace_snapshot, session_id);
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
fn apply_submission_session_hydration(
    controller_runtime: &ControllerRuntimeHandle,
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
    hydration: SessionHydrationSnapshot,
) -> Result<(), String> {
    let messages = filter_legacy_goal_internal_user_messages(hydration.messages);
    workspace_snapshot
        .session_messages
        .insert(session_id, messages);
    if let Some(mut runtime_state) = hydration.runtime_state {
        sanitize_runtime_state_legacy_goal_internal_messages(&mut runtime_state);
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

/// Ensures a hydrated session owns concrete dialogue context for future API requests.
/// 确保已水合会话拥有后续 API 请求所需的具体对话层上下文。
fn ensure_hydrated_session_dialogue_context(
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
) -> Result<(), String> {
    let Some(runtime_state) = workspace_snapshot.session_runtime_state.get(&session_id) else {
        return Err(format!(
            "ERR_SESSION_DIALOGUE_CONTEXT_NOT_READY: session {session_id} runtime state is missing after hydration"
        ));
    };
    if !runtime_state_has_concrete_conversation_context(runtime_state) {
        return Err(format!(
            "ERR_SESSION_DIALOGUE_CONTEXT_NOT_READY: session {session_id} dialogue context is empty after hydration"
        ));
    }
    Ok(())
}

/// Returns whether a retained WebUI snapshot has enough dialogue context for prompt submission.
/// 判断保留的 WebUI 快照是否已有足够的对话层上下文可直接提交提示词。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` is the in-memory snapshot retained by the service bridge.
/// `workspace_snapshot` 是服务桥保留的内存工作区快照。
/// `session_id` selects the submitted conversation.
/// `session_id` 选择本次提交目标会话。
///
/// # Returns
/// 返回值
/// `true` when the session runtime state already owns concrete conversation context.
/// 当会话运行态已拥有具体会话上下文时返回 `true`。
pub(super) fn retained_submission_snapshot_has_session_context(
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
) -> bool {
    workspace_snapshot
        .session_runtime_state
        .get(&session_id)
        .is_some_and(runtime_state_has_concrete_conversation_context)
}

/// Returns whether a runtime state carries provider-visible canonical dialogue messages.
/// 判断运行态是否携带 Provider 可见的规范对话消息。
///
/// # Parameters
/// 参数
/// `runtime_state` is the session runtime state to inspect.
/// `runtime_state` 是待检查的会话运行态。
///
/// # Returns
/// 返回值
/// `true` when canonical messages are already loaded for future API requests.
/// 当后续 API 请求所需的规范消息已经加载时返回 `true`。
pub(super) fn runtime_state_has_concrete_conversation_context(
    runtime_state: &SessionRuntimeStateEntry,
) -> bool {
    !runtime_state.conversation_state.messages.is_empty()
}

/// Synchronizes the hydrated marker with concrete dialogue runtime readiness.
/// 根据具体对话层运行态就绪情况同步水合标记。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` is the workspace snapshot whose marker set should be updated.
/// `workspace_snapshot` 是需要更新标记集合的工作区快照。
/// `session_id` selects the session whose hydrated marker should be synchronized.
/// `session_id` 选择需要同步水合标记的会话。
///
/// # Returns
/// 返回值
/// `true` when the marker remains set because dialogue context exists.
/// 当对话层上下文存在且标记保持置位时返回 `true`。
pub(super) fn sync_hydrated_session_dialogue_ready_marker(
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
) -> bool {
    if workspace_snapshot
        .session_runtime_state
        .get(&session_id)
        .is_some_and(runtime_state_has_concrete_conversation_context)
    {
        workspace_snapshot.hydrated_session_ids.insert(session_id);
        return true;
    }
    workspace_snapshot.hydrated_session_ids.remove(&session_id);
    false
}

/// Captures the minimal in-memory state needed to repair one refreshed workspace snapshot.
/// 捕获修复刷新后工作区快照所需的最小内存状态。
pub(super) fn capture_preserved_workspace_run_state(
    previous_snapshot: Option<&WorkspaceSnapshot>,
    active_kernel_session_ids: &BTreeSet<SessionId>,
    submitted_session_id: Option<SessionId>,
) -> Option<PreservedWorkspaceRunState> {
    let Some(previous_snapshot) = previous_snapshot else {
        return None;
    };
    let mut sessions = Vec::new();
    for (previous_index, previous_session) in previous_snapshot.sessions.iter().enumerate() {
        let runtime_state = previous_snapshot
            .session_runtime_state
            .get(&previous_session.id);
        let has_pending_prompt =
            runtime_state.is_some_and(runtime_state_has_queued_pending_prompt_work);
        let had_runtime_context =
            runtime_state.is_some_and(runtime_state_has_concrete_conversation_context);
        let is_submitted_session = submitted_session_id == Some(previous_session.id);
        let had_active_run = active_kernel_session_ids.contains(&previous_session.id);
        let had_hydrated_window = previous_snapshot
            .hydrated_session_ids
            .contains(&previous_session.id);
        if !had_active_run
            && !has_pending_prompt
            && !is_submitted_session
            && !had_hydrated_window
            && !had_runtime_context
        {
            continue;
        }
        let previous_phase = previous_snapshot
            .session_run_phases
            .get(&previous_session.id)
            .copied();
        let previous_project_index =
            previous_session
                .project_id
                .as_deref()
                .and_then(|project_id| {
                    previous_snapshot
                        .task_projects
                        .iter()
                        .find(|project| project.id == project_id)
                        .and_then(|project| {
                            project
                                .session_ids
                                .iter()
                                .position(|session_id| *session_id == previous_session.id)
                        })
                });
        sessions.push(PreservedSessionRunState {
            previous_index,
            previous_project_index,
            session: previous_session.clone(),
            previous_phase,
            had_active_run,
            had_hydrated_window,
            runtime_state: runtime_state.cloned(),
            messages: previous_snapshot
                .session_messages
                .get(&previous_session.id)
                .cloned(),
        });
    }
    let selected_session = (previous_snapshot.selected_session != SessionId::nil())
        .then_some(previous_snapshot.selected_session);
    (selected_session.is_some() || !sessions.is_empty()).then_some(PreservedWorkspaceRunState {
        selected_session,
        sessions,
    })
}

/// Preserves in-memory running session markers across database-backed snapshot refreshes.
/// 在基于数据库重载快照时保留内存中的会话运行中标记。
pub(super) fn preserve_active_session_run_state(
    preserved_state: Option<&PreservedWorkspaceRunState>,
    refreshed_snapshot: &mut WorkspaceSnapshot,
) {
    let Some(preserved_state) = preserved_state else {
        return;
    };
    if let Some(selected_session) = preserved_state.selected_session
        && refreshed_snapshot
            .sessions
            .iter()
            .any(|session| session.id == selected_session)
    {
        refreshed_snapshot.selected_session = selected_session;
    }
    for preserved_session in &preserved_state.sessions {
        let previous_session = &preserved_session.session;
        if !refreshed_snapshot
            .sessions
            .iter()
            .any(|session| session.id == previous_session.id)
        {
            preserve_missing_visible_session_summary(
                preserved_state,
                refreshed_snapshot,
                preserved_session,
            );
        }
        if let Some(refreshed_session) = refreshed_snapshot
            .sessions
            .iter_mut()
            .find(|session| session.id == previous_session.id)
        {
            refreshed_session.status = if preserved_session.had_active_run {
                SessionState::Running
            } else if previous_session.status == SessionState::Running {
                refreshed_session.status
            } else {
                previous_session.status
            };
            refreshed_session.last_updated_label = previous_session.last_updated_label.clone();
        }
        if let Some(previous_phase) = preserved_session.previous_phase
            && (preserved_session.had_active_run || !is_active_session_run_phase(previous_phase))
        {
            let preserved_phase = if preserved_session.had_active_run
                && !is_active_session_run_phase(previous_phase)
            {
                SessionRunPhase::Preparing
            } else {
                previous_phase
            };
            refreshed_snapshot
                .session_run_phases
                .insert(previous_session.id, preserved_phase);
        }
        let mut inserted_runtime_context = false;
        if let Some(mut runtime_state) = preserved_session.runtime_state.clone() {
            if let Some(refreshed_runtime_state) = refreshed_snapshot
                .session_runtime_state
                .get(&previous_session.id)
                && !runtime_state_has_concrete_conversation_context(&runtime_state)
                && runtime_state_has_concrete_conversation_context(refreshed_runtime_state)
            {
                runtime_state.conversation_state =
                    refreshed_runtime_state.conversation_state.clone();
            }
            inserted_runtime_context =
                runtime_state_has_concrete_conversation_context(&runtime_state);
            refreshed_snapshot
                .session_runtime_state
                .insert(previous_session.id, runtime_state);
        }
        if let Some(messages) = preserved_session.messages.clone() {
            refreshed_snapshot
                .session_messages
                .insert(previous_session.id, messages);
        }
        if preserved_session.had_hydrated_window || inserted_runtime_context {
            refreshed_snapshot
                .hydrated_session_ids
                .insert(previous_session.id);
        }
    }
}

/// Preserves one in-memory visible session that has not reached the database snapshot yet.
/// 保留一个尚未进入数据库快照的内存可见会话。
///
/// # Parameters
/// 参数
/// `preserved_state` is the retained minimal state before database refresh.
/// `preserved_state` 是数据库刷新前保留的最小状态。
/// `refreshed_snapshot` is the database-backed snapshot being repaired.
/// `refreshed_snapshot` 是正在修复的数据库来源快照。
/// `preserved_session` is the active, queued, or submitted session to preserve.
/// `preserved_session` 是需要保留的运行中、排队中或已提交会话。
fn preserve_missing_visible_session_summary(
    preserved_state: &PreservedWorkspaceRunState,
    refreshed_snapshot: &mut WorkspaceSnapshot,
    preserved_session: &PreservedSessionRunState,
) {
    let previous_session = &preserved_session.session;
    let insert_index = preserved_session
        .previous_index
        .min(refreshed_snapshot.sessions.len());
    refreshed_snapshot
        .sessions
        .insert(insert_index, previous_session.clone());
    if preserved_state.selected_session == Some(previous_session.id) {
        refreshed_snapshot.selected_session = previous_session.id;
    }
    preserve_visible_project_session_membership(refreshed_snapshot, preserved_session);
}

/// Preserves the project sidebar membership for one in-memory session.
/// 保留一个内存会话在项目侧栏中的归属关系。
///
/// # Parameters
/// 参数
/// `refreshed_snapshot` is the database-backed snapshot being repaired.
/// `refreshed_snapshot` 是正在修复的数据库来源快照。
/// `preserved_session` carries the session and its retained project index.
/// `preserved_session` 携带会话及其保留的项目位置。
fn preserve_visible_project_session_membership(
    refreshed_snapshot: &mut WorkspaceSnapshot,
    preserved_session: &PreservedSessionRunState,
) {
    let previous_session = &preserved_session.session;
    let Some(project_id) = previous_session.project_id.as_deref() else {
        return;
    };
    let previous_index = preserved_session.previous_project_index.unwrap_or_default();
    if let Some(refreshed_project) = refreshed_snapshot
        .task_projects
        .iter_mut()
        .find(|project| project.id == project_id)
    {
        refreshed_project
            .session_ids
            .retain(|session_id| *session_id != previous_session.id);
        let insert_index = previous_index.min(refreshed_project.session_ids.len());
        refreshed_project
            .session_ids
            .insert(insert_index, previous_session.id);
    }
}

/// Returns whether a hydration failure is caused by missing segmented session files.
/// 判断水合失败是否源自缺失分段会话文件。
fn is_missing_segmented_session_file_error(error: &str) -> bool {
    (error.contains("metadata.json")
        || error.contains("state.json")
        || error.contains("s001.jsonl")
        || error.contains("s000001.jsonl"))
        && (error.contains("os error 3")
            || error.contains("os error 2")
            || error.contains("系统找不到指定的路径")
            || error.contains("The system cannot find the path specified")
            || error.contains("No such file or directory"))
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use vulcan_context::{SessionConversationMessage, SessionConversationPayload};
    use vulcan_workbench_model::{SessionMessageEntry, SessionMessageKind};

    use super::*;

    /// Verifies selected-session hydration preserves an already loaded display window.
    /// 验证选中会话水合会保留已加载的显示窗口。
    #[test]
    fn selected_session_runtime_hydration_preserves_existing_display_window() {
        // session_id identifies the selected conversation being hydrated.
        // session_id 标识正在水合的选中会话。
        let session_id = SessionId::new_v4();
        // display_entry represents the already-loaded s001 display layer.
        // display_entry 表示已经加载的 s001 显示层。
        let display_entry =
            SessionMessageEntry::new(SessionMessageKind::Assistant, "助手", "已有显示层正文")
                .with_runtime_metadata("display-entry", "1000");
        // hydrated_display_entry represents the lazy hydration window that must not overwrite display_entry.
        // hydrated_display_entry 表示不应覆盖 display_entry 的懒加载显示窗口。
        let hydrated_display_entry =
            SessionMessageEntry::new(SessionMessageKind::Assistant, "助手", "水合显示正文")
                .with_runtime_metadata("hydrated-display-entry", "1001");
        // runtime_state represents the runtime/r001 JSONL conversation state.
        // runtime_state 表示 runtime/r001 JSONL 对应的会话运行态。
        let mut runtime_state = SessionRuntimeStateEntry::default();
        runtime_state.conversation_state.messages = vec![SessionConversationMessage {
            id: String::from("runtime-user-1"),
            payload: SessionConversationPayload::UserText {
                text: String::from("运行态正文"),
                attachments: Vec::new(),
            },
        }];
        let hydration = SessionHydrationSnapshot {
            session_id,
            messages: vec![hydrated_display_entry],
            tool_detail_messages: Vec::new(),
            compactions: Vec::new(),
            runtime_state: Some(runtime_state),
            earliest_ordinal: None,
            has_more_history: false,
        };
        // workspace_snapshot already has the display layer but lacks runtime context.
        // workspace_snapshot 已有显示层但缺少运行态上下文。
        let mut workspace_snapshot = WorkspaceSnapshot {
            task_projects: Vec::new(),
            archived_task_projects: Vec::new(),
            sessions: Vec::new(),
            archived_sessions: Vec::new(),
            session_messages: BTreeMap::from([(session_id, vec![display_entry])]),
            archived_session_messages: BTreeMap::new(),
            session_compactions: BTreeMap::new(),
            archived_session_compactions: BTreeMap::new(),
            session_runtime_state: BTreeMap::new(),
            archived_session_runtime_state: BTreeMap::new(),
            session_run_phases: BTreeMap::new(),
            expanded_task_projects: BTreeSet::new(),
            selected_session: session_id,
            hydrated_session_ids: BTreeSet::new(),
        };

        apply_selected_session_runtime_hydration(&mut workspace_snapshot, session_id, hydration);

        assert_eq!(
            workspace_snapshot
                .session_messages
                .get(&session_id)
                .and_then(|messages| messages.first())
                .map(|message| message.body.as_str()),
            Some("已有显示层正文")
        );
        let runtime_state = workspace_snapshot
            .session_runtime_state
            .get(&session_id)
            .expect("runtime state should exist");
        assert_eq!(
            runtime_state
                .conversation_state
                .messages
                .first()
                .and_then(|message| match &message.payload {
                    SessionConversationPayload::UserText { text, .. } => Some(text.as_str()),
                    _ => None,
                }),
            Some("运行态正文")
        );
        assert!(
            workspace_snapshot
                .hydrated_session_ids
                .contains(&session_id)
        );
    }

    /// Verifies hydrated sessions require concrete runtime dialogue context before being marked ready.
    /// 验证已水合会话必须具备具体运行态对话层上下文后才能被视为就绪。
    #[test]
    fn hydrated_session_dialogue_context_requires_runtime_context() {
        // session_id identifies the hydrated conversation under validation.
        // session_id 标识正在校验的已水合会话。
        let session_id = SessionId::new_v4();
        // display_only_snapshot simulates a loaded UI window without runtime dialogue context.
        // display_only_snapshot 模拟只有 UI 窗口而缺少运行态对话层上下文的状态。
        let display_only_snapshot = WorkspaceSnapshot {
            task_projects: Vec::new(),
            archived_task_projects: Vec::new(),
            sessions: Vec::new(),
            archived_sessions: Vec::new(),
            session_messages: BTreeMap::from([(
                session_id,
                vec![SessionMessageEntry::new(
                    SessionMessageKind::Assistant,
                    "助手",
                    "显示层正文",
                )],
            )]),
            archived_session_messages: BTreeMap::new(),
            session_compactions: BTreeMap::new(),
            archived_session_compactions: BTreeMap::new(),
            session_runtime_state: BTreeMap::new(),
            archived_session_runtime_state: BTreeMap::new(),
            session_run_phases: BTreeMap::new(),
            expanded_task_projects: BTreeSet::new(),
            selected_session: session_id,
            hydrated_session_ids: BTreeSet::new(),
        };
        // ready_runtime_state stores concrete runtime context for the positive case.
        // ready_runtime_state 保存正向场景所需的具体运行态上下文。
        let mut ready_runtime_state = SessionRuntimeStateEntry::default();
        ready_runtime_state.conversation_state.messages = vec![SessionConversationMessage {
            id: String::from("runtime-user-2"),
            payload: SessionConversationPayload::UserText {
                text: String::from("真实对话层正文"),
                attachments: Vec::new(),
            },
        }];
        // ready_snapshot simulates a fully hydrated session with dialogue context.
        // ready_snapshot 模拟带有对话层上下文的完整水合会话。
        let ready_snapshot = WorkspaceSnapshot {
            session_runtime_state: BTreeMap::from([(session_id, ready_runtime_state)]),
            ..display_only_snapshot.clone()
        };

        assert!(
            ensure_hydrated_session_dialogue_context(&display_only_snapshot, session_id).is_err()
        );
        assert!(ensure_hydrated_session_dialogue_context(&ready_snapshot, session_id).is_ok());
    }
}
