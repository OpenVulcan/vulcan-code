//! Project and conversation list service handlers.
//! 项目与会话列表服务处理器。

use super::*;

mod conversation_services;
mod project_services;

pub(super) use conversation_services::{
    service_archive_conversation, service_mark_conversation_read, service_rename_conversation,
    service_select_conversation,
};
pub use project_services::service_add_project_initial_directory;
pub(super) use project_services::{
    service_archive_project_conversations, service_open_project_directory,
    service_open_project_directory_path, service_remove_project, service_rename_project,
    service_reorder_projects, service_reset_project_icon, service_set_expanded_task_projects,
    service_set_project_icon, service_toggle_project_pinned,
};

/// Owns one in-progress session lifecycle transition and clears its reservation on every exit path.
/// 持有一次进行中的会话生命周期转换，并在所有退出路径上清理其预留状态。
pub(super) struct SessionLifecycleTransitionGuard {
    /// Shared service state that owns the transition reservations.
    /// 持有生命周期转换预留状态的共享服务状态。
    state: SharedServiceBridgeState,
    /// Exact session identities reserved by this transition.
    /// 本次转换预留的精确会话标识集合。
    session_ids: BTreeSet<SessionId>,
    /// Exact project identities reserved by this transition.
    /// 本次转换预留的精确项目标识集合。
    project_ids: BTreeSet<String>,
}

impl Drop for SessionLifecycleTransitionGuard {
    /// Clears every lifecycle reservation owned by this guard.
    /// 清理此守卫持有的全部生命周期预留状态。
    fn drop(&mut self) {
        if let Ok(mut guard) = self.state.lock() {
            for session_id in &self.session_ids {
                guard.session_lifecycle_transition_ids.remove(session_id);
            }
            for project_id in &self.project_ids {
                guard.project_lifecycle_transition_ids.remove(project_id);
            }
        }
    }
}

/// Atomically rejects active sessions and reserves the remaining ids for one lifecycle transition.
/// 原子拒绝活动会话，并为一次生命周期转换预留其余会话标识。
///
/// # Parameters
/// 参数
/// `state` owns runtime kernels and lifecycle transition reservations.
/// `state` 持有运行时内核与生命周期转换预留状态。
/// `session_ids` are the exact sessions that will be archived or removed.
/// `session_ids` 是即将归档或移除的精确会话集合。
/// `action_label` labels diagnostics for the requested lifecycle operation.
/// `action_label` 标记所请求生命周期操作的诊断文案。
///
/// # Returns
/// 返回值
/// Returns a guard that keeps new turns blocked until the lifecycle operation exits.
/// 返回一个在生命周期操作退出前持续阻止新回合的守卫。
pub(super) fn begin_session_lifecycle_transition(
    state: &SharedServiceBridgeState,
    session_ids: impl IntoIterator<Item = SessionId>,
    action_label: &str,
) -> Result<SessionLifecycleTransitionGuard, String> {
    begin_lifecycle_transition(state, session_ids, std::iter::empty(), action_label)
}

/// Atomically reserves one project and its current children for a lifecycle transition.
/// 原子预留一个项目及其当前子会话以执行生命周期转换。
///
/// # Parameters
/// 参数
/// `state` owns runtime kernels and lifecycle transition reservations.
/// `state` 持有运行时内核与生命周期转换预留状态。
/// `project_id` is the exact project that cannot accept new child sessions during the transition.
/// `project_id` 是转换期间不能接受新子会话的精确项目。
/// `session_ids` are the current child sessions that will be archived or removed.
/// `session_ids` 是即将归档或移除的当前子会话集合。
/// `action_label` labels diagnostics for the requested lifecycle operation.
/// `action_label` 标记所请求生命周期操作的诊断文案。
///
/// # Returns
/// 返回值
/// Returns a guard that blocks both new project children and new turns until exit.
/// 返回一个在退出前同时阻止新项目子会话与新回合的守卫。
pub(super) fn begin_project_lifecycle_transition(
    state: &SharedServiceBridgeState,
    project_id: &str,
    session_ids: impl IntoIterator<Item = SessionId>,
    action_label: &str,
) -> Result<SessionLifecycleTransitionGuard, String> {
    begin_lifecycle_transition(state, session_ids, [project_id.to_string()], action_label)
}

/// Atomically reserves exact session and project identities for one lifecycle transaction.
/// 为一次生命周期事务原子预留精确会话与项目标识。
///
/// # Parameters
/// 参数
/// `state` owns runtime kernels and lifecycle transition reservations.
/// `state` 持有运行时内核与生命周期转换预留状态。
/// `session_ids` are the exact session targets reserved by the transaction.
/// `session_ids` 是事务预留的精确会话目标。
/// `project_ids` are the exact project targets reserved by the transaction.
/// `project_ids` 是事务预留的精确项目目标。
/// `action_label` labels diagnostics for the requested lifecycle operation.
/// `action_label` 标记所请求生命周期操作的诊断文案。
///
/// # Returns
/// 返回值
/// Returns a guard that releases every reservation on all exit paths.
/// 返回一个在所有退出路径上释放全部预留状态的守卫。
fn begin_lifecycle_transition(
    state: &SharedServiceBridgeState,
    session_ids: impl IntoIterator<Item = SessionId>,
    project_ids: impl IntoIterator<Item = String>,
    action_label: &str,
) -> Result<SessionLifecycleTransitionGuard, String> {
    // reserved_session_ids deduplicates project membership before validation and reservation.
    // reserved_session_ids 在校验与预留前去重项目成员关系。
    let reserved_session_ids = session_ids.into_iter().collect::<BTreeSet<_>>();
    // reserved_project_ids deduplicates project targets before validation and reservation.
    // reserved_project_ids 在校验与预留前去重项目目标。
    let reserved_project_ids = project_ids.into_iter().collect::<BTreeSet<_>>();
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    for session_id in &reserved_session_ids {
        if guard.session_lifecycle_transition_ids.contains(session_id) {
            return Err(format!(
                "会话 `{session_id}` 正在执行生命周期转换，无法重复{action_label}。"
            ));
        }
        if webui_session_kernel_has_active_turn(&guard, *session_id)
            || webui_session_has_active_run(&guard, *session_id)
        {
            return Err(format!(
                "会话 `{session_id}` 正在运行，请先停止后再{action_label}。"
            ));
        }
    }
    for project_id in &reserved_project_ids {
        if guard.project_lifecycle_transition_ids.contains(project_id) {
            return Err(format!(
                "项目 `{project_id}` 正在执行生命周期转换，无法重复{action_label}。"
            ));
        }
    }
    guard
        .session_lifecycle_transition_ids
        .extend(reserved_session_ids.iter().copied());
    guard
        .project_lifecycle_transition_ids
        .extend(reserved_project_ids.iter().cloned());
    drop(guard);
    Ok(SessionLifecycleTransitionGuard {
        state: state.clone(),
        session_ids: reserved_session_ids,
        project_ids: reserved_project_ids,
    })
}

/// Normalizes and validates a user-provided display name.
/// 规范化并校验用户提供的显示名称。
pub(super) fn normalize_non_empty_name(name: &str) -> Result<String, String> {
    let normalized_name = name.trim();
    if normalized_name.is_empty() {
        return Err(String::from("名称不能为空。"));
    }
    Ok(normalized_name.to_string())
}

/// Reads the controller runtime handle from shared service state.
/// 从共享服务状态中读取控制器运行时句柄。
pub(super) fn controller_runtime_from_state(
    state: &SharedServiceBridgeState,
) -> Result<ControllerRuntimeHandle, String> {
    state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?
        .controller_runtime
        .clone()
        .ok_or_else(|| String::from("service is still starting; controller runtime is not ready"))
}

/// Reads the selected session identifier from the retained workspace state.
/// 从保留的工作区状态中读取当前选中会话标识。
pub(super) fn selected_session_id_from_state(
    state: &SharedServiceBridgeState,
) -> Result<SessionId, String> {
    let guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    if let Some(workspace_snapshot) = guard.workspace_snapshot.as_ref()
        && let Some(selected_session_id) = selected_workspace_session_id(workspace_snapshot)
    {
        return Ok(selected_session_id);
    }
    guard
        .workbench
        .selected_conversation_id
        .as_deref()
        .and_then(parse_session_id)
        .ok_or_else(|| String::from("当前没有选中的会话。"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal shell host used by lifecycle transition guard tests.
    /// 生命周期转换守卫测试使用的最小 shell 宿主。
    struct LifecycleTransitionTestShellHost;

    impl ServiceShellHost for LifecycleTransitionTestShellHost {
        /// Accepts path-open requests without touching the host environment.
        /// 接受打开路径请求且不触碰宿主环境。
        fn open_path(&self, _path: &str) -> Result<(), String> {
            Ok(())
        }

        /// Accepts URL-open requests without touching the host environment.
        /// 接受打开 URL 请求且不触碰宿主环境。
        fn open_url(&self, _url: &str) -> Result<(), String> {
            Ok(())
        }
    }

    /// Verifies one lifecycle reservation blocks new turns and is released on guard drop.
    /// 验证生命周期预留会阻止新回合，并在守卫释放时解除。
    #[test]
    fn lifecycle_transition_guard_blocks_and_releases_kernel_turns() {
        // session_id identifies the lifecycle target shared by reservation and kernel start.
        // session_id 标识生命周期预留与内核启动共享的目标。
        let session_id = SessionId::from_u128(0x95);
        // project_id identifies the project that cannot accept new children during the transition.
        // project_id 标识转换期间不能接受新子会话的项目。
        let project_id = String::from("project-lifecycle-test");
        // state owns the exact runtime kernel and lifecycle reservation tables under test.
        // state 持有测试中的精确运行内核与生命周期预留表。
        let state = create_service_bridge_state(Arc::new(LifecycleTransitionTestShellHost));
        // transition_guard keeps the target reserved across the first kernel start attempt.
        // transition_guard 在首次内核启动尝试期间持续预留目标。
        let transition_guard =
            begin_project_lifecycle_transition(&state, &project_id, [session_id], "归档")
                .expect("idle project should enter lifecycle transition");
        {
            let mut guard = state.lock().expect("service state should lock");
            assert!(guard.project_lifecycle_transition_ids.contains(&project_id));
            assert!(
                start_webui_session_kernel_user_turn(&mut guard, session_id, "blocked-turn")
                    .is_err()
            );
        }

        drop(transition_guard);

        let mut guard = state
            .lock()
            .expect("service state should lock after release");
        assert!(!guard.project_lifecycle_transition_ids.contains(&project_id));
        start_webui_session_kernel_user_turn(&mut guard, session_id, "accepted-turn")
            .expect("released lifecycle reservation should allow a turn");
    }
}
