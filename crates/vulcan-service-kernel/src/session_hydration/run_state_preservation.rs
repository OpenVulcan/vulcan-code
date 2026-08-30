//! Session hydration run state preservation helpers.
//! 会话水合 run state preservation 辅助逻辑。

use super::*;

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
}

/// Minimal retained workspace state used after a database-backed refresh.
/// 数据库刷新后使用的最小工作区保留状态。
#[derive(Debug, Clone)]
pub(crate) struct PreservedWorkspaceRunState {
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
    pub(crate) fn session_count(&self) -> usize {
        self.sessions.len()
    }
}

/// Captures the minimal in-memory state needed to repair one refreshed workspace snapshot.
/// 捕获修复刷新后工作区快照所需的最小内存状态。
pub(crate) fn capture_preserved_workspace_run_state(
    previous_snapshot: Option<&WorkspaceSnapshot>,
    active_kernel_session_ids: &BTreeSet<SessionId>,
    submitted_session_id: Option<SessionId>,
) -> Option<PreservedWorkspaceRunState> {
    capture_preserved_workspace_run_state_inner(
        previous_snapshot,
        active_kernel_session_ids,
        submitted_session_id,
    )
}

/// Captures retained in-memory state for the current workspace refresh.
/// 为当前工作区刷新捕获保留的内存状态。
///
/// # Parameters
/// 参数
/// `previous_snapshot` is the workspace snapshot that may contain live-only state.
/// `previous_snapshot` 是可能包含仅内存状态的工作区快照。
/// `active_kernel_session_ids` identifies sessions that still have active runtime work.
/// `active_kernel_session_ids` 标识仍有活动运行时工作的会话。
/// `submitted_session_id` optionally forces one recently submitted session to be preserved.
/// `submitted_session_id` 可选地强制保留一个刚提交的会话。
/// # Returns
/// 返回值
/// The retained workspace repair state, or `None` when no state needs preservation.
/// 返回工作区修复保留状态；没有需要保留的状态时返回 `None`。
fn capture_preserved_workspace_run_state_inner(
    previous_snapshot: Option<&WorkspaceSnapshot>,
    active_kernel_session_ids: &BTreeSet<SessionId>,
    submitted_session_id: Option<SessionId>,
) -> Option<PreservedWorkspaceRunState> {
    let previous_snapshot = previous_snapshot?;
    let mut sessions = Vec::new();
    for (previous_index, previous_session) in previous_snapshot.sessions.iter().enumerate() {
        let runtime_state = previous_snapshot
            .session_runtime_state
            .get(&previous_session.id);
        let had_runtime_context =
            runtime_state.is_some_and(runtime_state_has_concrete_conversation_context);
        let is_submitted_session = submitted_session_id == Some(previous_session.id);
        let had_active_run = active_kernel_session_ids.contains(&previous_session.id);
        let had_hydrated_window = previous_snapshot
            .hydrated_session_ids
            .contains(&previous_session.id);
        // Metadata-only model controls must survive index refreshes even before history is ever hydrated.
        // 即使历史从未水合，纯 metadata 模型控件也必须跨索引刷新保留。
        let had_request_selection = runtime_state.is_some_and(|state| {
            state.active_provider_id.is_some()
                || state.active_connection_id.is_some()
                || state.active_model_id.is_some()
                || state.pending_model_selection.is_some()
        });
        if !had_active_run
            && !is_submitted_session
            && !had_hydrated_window
            && !had_runtime_context
            && !had_request_selection
        {
            continue;
        }
        let previous_phase = previous_snapshot
            .session_run_phases
            .get(&previous_session.id)
            .copied();
        let previous_project_index =
            session_summary_project_id(previous_session).and_then(|project_id| {
                workspace_task_project_for_project_id(previous_snapshot, project_id).and_then(
                    |project| {
                        project
                            .session_ids
                            .iter()
                            .position(|session_id| *session_id == previous_session.id)
                    },
                )
            });
        sessions.push(PreservedSessionRunState {
            previous_index,
            previous_project_index,
            session: previous_session.clone(),
            previous_phase,
            had_active_run,
            had_hydrated_window,
            runtime_state: runtime_state.cloned(),
        });
    }
    let selected_session = selected_workspace_session_id(previous_snapshot);
    (selected_session.is_some() || !sessions.is_empty()).then_some(PreservedWorkspaceRunState {
        selected_session,
        sessions,
    })
}

/// Preserves in-memory running session markers across database-backed snapshot refreshes.
/// 在基于数据库重载快照时保留内存中的会话运行中标记。
pub(crate) fn preserve_active_session_run_state(
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
        // is_authoritatively_archived prevents retained hydration or run state from reviving a committed archive tombstone.
        // is_authoritatively_archived 防止保留的水合态或运行态复活已提交的归档墓碑。
        let is_authoritatively_archived = refreshed_snapshot
            .archived_sessions
            .iter()
            .any(|session| session.id == previous_session.id);
        if is_authoritatively_archived {
            continue;
        }
        // has_authoritative_owner_project distinguishes a not-yet-persisted session from a removed-project tombstone.
        // has_authoritative_owner_project 区分尚未持久化的会话与已删除项目墓碑。
        let has_authoritative_owner_project = session_summary_project_id(previous_session)
            .is_none_or(|project_id| {
                workspace_task_project_for_project_id(refreshed_snapshot, project_id).is_some()
            });
        if !has_authoritative_owner_project {
            continue;
        }
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
    let Some(project_id) = session_summary_project_id(previous_session) else {
        return;
    };
    let previous_index = preserved_session.previous_project_index.unwrap_or_default();
    if let Some(refreshed_project) =
        workspace_task_project_for_project_id_mut(refreshed_snapshot, project_id)
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
