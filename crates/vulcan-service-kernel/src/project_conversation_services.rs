//! Project and conversation list service handlers.
//! 项目与会话列表服务处理器。

use super::*;

mod collaboration_grants;
mod conversation_services;
mod project_services;

pub(super) use collaboration_grants::{
    service_add_collaboration_grant, service_list_collaboration_grants,
    service_remove_collaboration_grant,
};

pub(super) use conversation_services::{
    service_archive_conversation, service_delete_archived_conversations,
    service_mark_conversation_read, service_rename_conversation,
    service_restore_archived_conversations, service_select_conversation,
    sync_selected_conversation_fields_for_retained_workbench,
};
pub use project_services::service_add_project_initial_directory;
pub(super) use project_services::{
    service_archive_project_conversations, service_open_project_directory,
    service_open_project_directory_path, service_remove_project, service_rename_project,
    service_reorder_projects, service_reset_project_icon, service_set_expanded_task_projects,
    service_set_project_icon, service_toggle_project_pinned,
};

/// Owns one in-progress session turn-admission transaction and clears only its reservations on exit.
/// 持有一次进行中的会话回合准入事务，并在退出时仅清理其自身预留。
pub(super) struct SessionTurnAdmissionGuard {
    /// Shared service state that owns the typed admission reservations.
    /// 持有类型化准入预留状态的共享服务状态。
    state: SharedServiceBridgeState,
    /// Unique owner identity shared by every session reservation in this guard.
    /// 此守卫中所有会话预留共享的唯一所有者身份。
    reservation_id: Uuid,
    /// Exact session identities reserved by this admission transaction.
    /// 本次准入事务预留的精确会话标识集合。
    session_ids: BTreeSet<SessionId>,
    /// Exact project scopes additionally reserved by a project lifecycle transaction.
    /// 项目生命周期事务额外预留的精确项目作用域集合。
    project_scopes: BTreeSet<ProjectAdmissionScope>,
}

/// Copyable authorization proving one outer transaction owns an exact prompt-submission reservation.
/// 可复制的授权凭据，证明一个外层事务拥有精确的提示词提交预留。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ExistingSubmissionAdmissionAuthorization {
    /// Exact existing session reserved by the outer transaction.
    /// 外层事务预留的精确已有会话。
    pub(crate) session_id: SessionId,
    /// Unique reservation owner accepted by preflight and kernel start.
    /// 预检与内核启动接受的唯一预留所有者。
    pub(crate) reservation_id: Uuid,
}

impl SessionTurnAdmissionGuard {
    /// Exposes the exact prompt-submission authorization owned by this single-session guard.
    /// 暴露此单会话守卫拥有的精确提示词提交授权。
    ///
    /// # Parameters
    /// 参数
    /// `session_id` is the exact existing session that the nested submission must start.
    /// `session_id` 是嵌套提交必须启动的精确已有会话。
    ///
    /// # Returns
    /// 返回值
    /// A copyable authorization while this guard retains ownership, or a structural ownership error.
    /// 在此守卫保留所有权期间返回可复制授权，否则返回结构所有权错误。
    pub(crate) fn existing_submission_authorization(
        &self,
        session_id: SessionId,
    ) -> Result<ExistingSubmissionAdmissionAuthorization, String> {
        if self.session_ids.len() != 1
            || !self.session_ids.contains(&session_id)
            || !self.project_scopes.is_empty()
        {
            return Err(format!(
                "Prompt submission admission guard does not exclusively own session `{session_id}`."
            ));
        }
        Ok(ExistingSubmissionAdmissionAuthorization {
            session_id,
            reservation_id: self.reservation_id,
        })
    }
}

impl Drop for SessionTurnAdmissionGuard {
    /// Clears every session reservation still owned by this guard and its project reservations.
    /// 清理仍由此守卫持有的全部会话预留及其项目预留。
    fn drop(&mut self) {
        // guard recovers poisoned state only to remove reservations still owned by this exact transaction.
        // guard 仅为删除仍由当前精确事务拥有的预留而恢复中毒状态。
        let mut guard = lock_service_state_for_admission_cleanup(&self.state, "session-turn guard");
        guard
            .session_turn_admission_reservations
            .retain(|session_id, reservation| {
                !self.session_ids.contains(session_id)
                    || reservation.reservation_id != self.reservation_id
            });
        guard
            .project_admission_reservations
            .retain(|project_scope, reservation| {
                !self.project_scopes.contains(project_scope)
                    || reservation.reservation_id != self.reservation_id
            });
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
) -> Result<SessionTurnAdmissionGuard, String> {
    begin_lifecycle_transition(state, session_ids, std::iter::empty(), action_label)
}

/// Reserves exact sessions against new kernel turns while an in-flight mutation changes dispatch authority.
/// 在进行中的变更修改分发权威状态时，预留精确会话以阻止新的 kernel 回合。
///
/// Unlike destructive lifecycle transitions, this guard allows an already-running turn to finish. It only
/// closes the race in which a previously validated queued turn registers while Goal state is being changed.
/// 与破坏性生命周期转换不同，此守卫允许已经运行的回合结束；它只关闭已校验排队回合在 Goal 状态变更期间注册的竞态窗口。
///
/// # Parameters
/// 参数
/// `state` owns runtime kernels and lifecycle transition reservations.
/// `state` 持有运行时 kernel 与生命周期转换预留状态。
/// `session_ids` are the exact sessions whose new turn registration must be excluded.
/// `session_ids` 是必须排除新回合注册的精确会话集合。
/// `action_label` labels diagnostics for conflicting mutations.
/// `action_label` 标记冲突变更的诊断文案。
///
/// # Returns
/// 返回值
/// Returns a guard that blocks new turns until the authoritative mutation and queue cleanup finish.
/// 返回一个在权威状态变更与队列清理完成前阻止新回合的守卫。
pub(super) fn begin_session_turn_start_exclusion(
    state: &SharedServiceBridgeState,
    session_ids: impl IntoIterator<Item = SessionId>,
    action_label: &str,
) -> Result<SessionTurnAdmissionGuard, String> {
    let reserved_session_ids = session_ids.into_iter().collect::<BTreeSet<_>>();
    let reservation_id = Uuid::new_v4();
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    reserve_session_turn_admission(
        &mut guard,
        &reserved_session_ids,
        reservation_id,
        SessionTurnAdmissionReservationKind::GoalMutation,
        action_label,
    )?;
    drop(guard);
    Ok(SessionTurnAdmissionGuard {
        state: state.clone(),
        reservation_id,
        session_ids: reserved_session_ids,
        project_scopes: BTreeSet::new(),
    })
}

/// Reserves one exact session while Stop tears down its current run and commits the stopped state.
/// 在 Stop 拆除当前运行并提交停止状态期间预留一个精确会话。
///
/// # Parameters
/// 参数
/// `state` owns the typed admission reservation table shared with prompt submission.
/// `state` 持有与提示词提交共享的类型化准入预留表。
/// `session_id` is the exact conversation whose replacement turns must remain blocked.
/// `session_id` 是替代回合必须继续被阻止的精确会话。
///
/// # Returns
/// 返回值
/// A guard that blocks new turns until Stop persistence and projection finish.
/// 返回一个在 Stop 持久化与投影完成前阻止新回合的守卫。
pub(super) fn begin_session_run_stop_exclusion(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
) -> Result<SessionTurnAdmissionGuard, String> {
    // reservationId uniquely owns the stop row so stale cleanup cannot release a successor transaction.
    // reservationId 唯一拥有停止预留行，使过期清理无法释放后继事务。
    let reservation_id = Uuid::new_v4();
    let reserved_session_ids = BTreeSet::from([session_id]);
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    if guard
        .session_terminal_persistence_owners
        .contains_key(&session_id)
    {
        return Err(format!(
            "会话 `{session_id}` 正在可靠提交终态，暂时不能重复停止。"
        ));
    }
    reserve_session_turn_admission(
        &mut guard,
        &reserved_session_ids,
        reservation_id,
        SessionTurnAdmissionReservationKind::RunStop,
        "停止",
    )?;
    drop(guard);
    Ok(SessionTurnAdmissionGuard {
        state: state.clone(),
        reservation_id,
        session_ids: reserved_session_ids,
        project_scopes: BTreeSet::new(),
    })
}
/// Tries to reserve one idle session for exactly one queued user-prompt preparation.
/// 尝试为一次排队用户提示词准备预留一个空闲会话。
///
/// # Parameters
/// 参数
/// `state` owns runtime activity and typed turn-admission reservations.
/// `state` 持有运行活动与类型化回合准入预留。
/// `session_id` identifies the exact session whose queue is being prepared.
/// `session_id` 标识正在准备队列的精确会话。
///
/// # Returns
/// 返回值
/// A guard for the sole accepted preparer, `None` for a concurrent or active owner, or a lock error.
/// 返回唯一获准准备者的守卫；并发或活动所有者存在时返回 `None`，加锁失败时返回错误。
pub(super) fn try_begin_pending_prompt_dispatch_exclusion(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
) -> Result<Option<SessionTurnAdmissionGuard>, String> {
    let reservation_id = Uuid::new_v4();
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    if webui_session_kernel_has_active_turn(&guard, session_id)
        || webui_session_has_active_run(&guard, session_id)
        || guard
            .session_turn_admission_reservations
            .contains_key(&session_id)
    {
        return Ok(None);
    }
    reserve_session_turn_admission(
        &mut guard,
        &BTreeSet::from([session_id]),
        reservation_id,
        SessionTurnAdmissionReservationKind::PromptSubmission,
        "分发排队提示词",
    )?;
    drop(guard);
    Ok(Some(SessionTurnAdmissionGuard {
        state: state.clone(),
        reservation_id,
        session_ids: BTreeSet::from([session_id]),
        project_scopes: BTreeSet::new(),
    }))
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
) -> Result<SessionTurnAdmissionGuard, String> {
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
) -> Result<SessionTurnAdmissionGuard, String> {
    // reserved_session_ids deduplicates project membership before validation and reservation.
    // reserved_session_ids 在校验与预留前去重项目成员关系。
    let mut reserved_session_ids = session_ids.into_iter().collect::<BTreeSet<_>>();
    // reserved_project_scopes deduplicates exact project targets before validation and reservation.
    // reserved_project_scopes 在校验与预留前去重精确项目目标。
    let reserved_project_scopes = project_ids
        .into_iter()
        .map(ProjectAdmissionScope::Exact)
        .collect::<BTreeSet<_>>();
    // reservation_id uniquely owns both session and project rows in this lifecycle transaction.
    // reservation_id 唯一拥有本次生命周期事务中的会话行与项目行。
    let reservation_id = Uuid::new_v4();
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    // Completed parents can still own active descendants; use canonical runtime edges rather than the visible list.
    // 已完成父任务仍可能拥有活动后代；使用规范运行时父边，而非可见列表。
    if let Some(runtime) = &guard.agent_runtime {
        // This snapshot includes completed business records with independent auxiliary work.
        // 此快照包含拥有独立辅助工作的已完成业务记录。
        let registry = runtime.task_runtime.subagent_registry_snapshot()?;
        // Every target can be either an indexed root or an embedded subtree root.
        // 每个目标既可以是索引根，也可以是嵌入子树根。
        let mut pending = reserved_session_ids.iter().copied().collect::<Vec<_>>();
        while let Some(parent) = pending.pop() {
            for child in registry.list_for_parent(parent.into()) {
                if reserved_session_ids.insert(child.session_id.as_uuid()) {
                    pending.push(child.session_id.as_uuid());
                }
            }
        }
    }
    if let Some(child) = crate::agent_collaboration::child_maintenance::lifecycle_conflict(
        &guard,
        &reserved_session_ids,
        &reserved_project_scopes,
    ) {
        return Err(format!(
            "子会话 `{child}` 正在维护上下文或等待可靠清理，请先停止后再{action_label}。"
        ));
    }
    for session_id in &reserved_session_ids {
        // Embedded business runs are owned by TaskRuntime rather than a main-session kernel slot.
        // 嵌入业务运行由 TaskRuntime 持有，而非主会话内核槽位。
        let child_is_running = match &guard.agent_runtime {
            Some(runtime) => runtime.task_runtime.is_task_running(*session_id)?,
            None => false,
        };
        if webui_session_kernel_has_active_turn(&guard, *session_id)
            || webui_session_has_active_run(&guard, *session_id)
            || guard
                .child_question_execution_owners
                .contains_key(session_id)
            || child_is_running
        {
            return Err(format!(
                "会话 `{session_id}` 正在运行，请先停止后再{action_label}。"
            ));
        }
    }
    if let Some(reservation) = guard
        .project_admission_reservations
        .get(&ProjectAdmissionScope::UnresolvedSubmission)
    {
        return Err(format!(
            "Project selection is reserved by {}; cannot {action_label} project lifecycle state",
            reservation.kind.diagnostic_label()
        ));
    }
    for project_scope in &reserved_project_scopes {
        if let Some(reservation) = guard.project_admission_reservations.get(project_scope) {
            // project_id is extracted only from exact lifecycle scopes constructed above.
            // project_id 仅从上方构造的精确生命周期作用域中提取。
            let ProjectAdmissionScope::Exact(project_id) = project_scope else {
                unreachable!("lifecycle project scopes are always exact")
            };
            return Err(format!(
                "Project `{project_id}` is reserved by {}; cannot {action_label}",
                reservation.kind.diagnostic_label()
            ));
        }
    }
    reserve_session_turn_admission(
        &mut guard,
        &reserved_session_ids,
        reservation_id,
        SessionTurnAdmissionReservationKind::LifecycleTransition,
        action_label,
    )?;
    guard
        .project_admission_reservations
        .extend(
            reserved_project_scopes
                .iter()
                .cloned()
                .map(|project_scope| {
                    (
                        project_scope,
                        ProjectAdmissionReservation {
                            reservation_id,
                            kind: ProjectAdmissionReservationKind::LifecycleTransition,
                        },
                    )
                }),
        );
    drop(guard);
    Ok(SessionTurnAdmissionGuard {
        state: state.clone(),
        reservation_id,
        session_ids: reserved_session_ids,
        project_scopes: reserved_project_scopes,
    })
}

/// Atomically validates and inserts one owned turn-admission reservation across exact sessions.
/// 跨精确会话原子校验并插入一个有所有权的回合准入预留。
///
/// # Parameters
/// 参数
/// `state` owns the typed reservation table under the caller's shared-state lock.
/// `state` 在调用方共享状态锁下持有类型化预留表。
/// `session_ids` are the deduplicated sessions reserved as one transaction.
/// `session_ids` 是作为单个事务预留的已去重会话集合。
/// `reservation_id` uniquely owns every inserted row for guarded release.
/// `reservation_id` 唯一拥有每个插入行，用于受保护释放。
/// `kind` identifies the domain reason that blocks new turns.
/// `kind` 标识阻止新回合的领域原因。
/// `action_label` labels the requested operation in conflict diagnostics.
/// `action_label` 在冲突诊断中标记请求的操作。
///
/// # Returns
/// 返回值
/// Success after inserting all rows, or a conflict before any row is inserted.
/// 全部行插入后返回成功，或在插入任何行前返回冲突。
fn reserve_session_turn_admission(
    state: &mut ServiceBridgeState,
    session_ids: &BTreeSet<SessionId>,
    reservation_id: Uuid,
    kind: SessionTurnAdmissionReservationKind,
    action_label: &str,
) -> Result<(), String> {
    for session_id in session_ids {
        if let Some(existing) = state.session_turn_admission_reservations.get(session_id) {
            return Err(format!(
                "会话 `{session_id}` 已由 {} 预留，无法执行{action_label}。",
                existing.kind.diagnostic_label()
            ));
        }
    }
    state
        .session_turn_admission_reservations
        .extend(session_ids.iter().copied().map(|session_id| {
            (
                session_id,
                SessionTurnAdmissionReservation {
                    reservation_id,
                    kind,
                },
            )
        }));
    Ok(())
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
) -> Result<HostRuntimeHandle, String> {
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
            assert_eq!(
                guard
                    .project_admission_reservations
                    .get(&ProjectAdmissionScope::Exact(project_id.clone()))
                    .map(|reservation| reservation.reservation_id),
                Some(transition_guard.reservation_id),
            );
            assert_eq!(
                guard
                    .session_turn_admission_reservations
                    .get(&session_id)
                    .expect("session lifecycle reservation should exist")
                    .kind,
                SessionTurnAdmissionReservationKind::LifecycleTransition
            );
            assert!(
                start_webui_session_kernel_user_turn(&mut guard, session_id, "blocked-turn")
                    .is_err()
            );
        }

        drop(transition_guard);

        let mut guard = state
            .lock()
            .expect("service state should lock after release");
        assert!(
            !guard
                .project_admission_reservations
                .contains_key(&ProjectAdmissionScope::Exact(project_id))
        );
        start_webui_session_kernel_user_turn(&mut guard, session_id, "accepted-turn")
            .expect("released lifecycle reservation should allow a turn");
    }

    /// Verifies a stale guard cannot delete a newer reservation for the same session.
    /// 验证过期守卫不能删除同一会话的更新预留。
    #[test]
    fn turn_admission_guard_releases_only_its_owned_reservation() {
        // session_id identifies the admission slot whose owner is deliberately replaced.
        // session_id 标识被刻意替换所有者的准入槽位。
        let session_id = SessionId::from_u128(0x96);
        // state owns the typed reservation table under test.
        // state 持有测试中的类型化预留表。
        let state = create_service_bridge_state(Arc::new(LifecycleTransitionTestShellHost));
        // stale_guard owns the first Goal-mutation reservation generation.
        // stale_guard 持有第一代 Goal 变更预留。
        let stale_guard = begin_session_turn_start_exclusion(&state, [session_id], "更新目标")
            .expect("first reservation should succeed");
        // replacement_id identifies a later reservation generation for the same session.
        // replacement_id 标识同一会话后续一代预留。
        let replacement_id = Uuid::new_v4();
        {
            // guard replaces the table entry to simulate a newer authoritative owner.
            // guard 替换表项以模拟更新的权威所有者。
            let mut guard = state.lock().expect("service state should lock");
            guard.session_turn_admission_reservations.insert(
                session_id,
                SessionTurnAdmissionReservation {
                    reservation_id: replacement_id,
                    kind: SessionTurnAdmissionReservationKind::LifecycleTransition,
                },
            );
        }

        drop(stale_guard);

        // guard verifies the stale drop retained the replacement generation exactly.
        // guard 验证过期释放精确保留了替代代际。
        let guard = state.lock().expect("service state should lock after drop");
        assert_eq!(
            guard
                .session_turn_admission_reservations
                .get(&session_id)
                .copied(),
            Some(SessionTurnAdmissionReservation {
                reservation_id: replacement_id,
                kind: SessionTurnAdmissionReservationKind::LifecycleTransition,
            })
        );
    }

    /// Verifies a Stop reservation blocks replacement turns and exposes its precise reservation kind.
    /// 验证 Stop 预留会阻止替代回合，并暴露精确的预留类型。
    #[test]
    fn run_stop_exclusion_blocks_replacement_turn_until_release() {
        // sessionId identifies the exact conversation crossing the Stop teardown boundary.
        // sessionId 标识正在跨越 Stop 拆除边界的精确会话。
        let session_id = SessionId::from_u128(0x996);
        // state owns the runtime kernel and typed reservation under test.
        // state 持有被测运行时内核与类型化预留。
        let state = create_service_bridge_state(Arc::new(LifecycleTransitionTestShellHost));
        // stopGuard holds admission closed through the simulated teardown and persistence window.
        // stopGuard 在模拟的拆除与持久化窗口内持续关闭准入。
        let stop_guard = begin_session_run_stop_exclusion(&state, session_id)
            .expect("Stop should reserve an unowned session");
        {
            let mut guard = state.lock().expect("service state should lock");
            assert_eq!(
                guard
                    .session_turn_admission_reservations
                    .get(&session_id)
                    .map(|reservation| reservation.kind),
                Some(SessionTurnAdmissionReservationKind::RunStop)
            );
            assert!(
                start_webui_session_kernel_user_turn(&mut guard, session_id, "replacement-turn")
                    .is_err()
            );
        }

        drop(stop_guard);

        let mut guard = state
            .lock()
            .expect("service state should lock after Stop release");
        start_webui_session_kernel_user_turn(&mut guard, session_id, "replacement-turn")
            .expect("replacement turn should start after Stop finishes");
    }

    /// Verifies project conflicts leave no partial session reservation behind.
    /// 验证项目冲突不会遗留局部会话预留。
    #[test]
    fn project_reservation_conflict_is_atomic_across_session_and_project_rows() {
        // session_id identifies the child row that must remain unreserved on project conflict.
        // session_id 标识项目冲突时必须保持未预留的子会话行。
        let session_id = SessionId::from_u128(0x97);
        // project_id identifies the already-reserved lifecycle target.
        // project_id 标识已经被预留的生命周期目标。
        let project_id = String::from("project-conflict-test");
        // state owns both reservation tables committed as one logical transaction.
        // state 持有作为单个逻辑事务提交的两张预留表。
        let state = create_service_bridge_state(Arc::new(LifecycleTransitionTestShellHost));
        {
            // guard seeds the authoritative conflicting project owner.
            // guard 写入权威的冲突项目所有者。
            let mut guard = state.lock().expect("service state should lock");
            guard.project_admission_reservations.insert(
                ProjectAdmissionScope::Exact(project_id.clone()),
                ProjectAdmissionReservation {
                    reservation_id: Uuid::new_v4(),
                    kind: ProjectAdmissionReservationKind::LifecycleTransition,
                },
            );
        }

        let result = begin_project_lifecycle_transition(&state, &project_id, [session_id], "归档");

        assert!(result.is_err());
        let guard = state
            .lock()
            .expect("service state should lock after conflict");
        assert!(
            !guard
                .session_turn_admission_reservations
                .contains_key(&session_id)
        );
    }

    /// Verifies a stale lifecycle guard preserves newer project and session reservation owners.
    /// 验证过期生命周期守卫会保留更新的项目与会话预留所有者。
    #[test]
    fn lifecycle_guard_releases_only_matching_transaction_owner() {
        // session_id identifies the replaced session reservation row.
        // session_id 标识被替换的会话预留行。
        let session_id = SessionId::from_u128(0x98);
        // project_id identifies the replaced project reservation row.
        // project_id 标识被替换的项目预留行。
        let project_id = String::from("project-owner-test");
        // state owns both typed transaction resources.
        // state 持有两类带类型的事务资源。
        let state = create_service_bridge_state(Arc::new(LifecycleTransitionTestShellHost));
        // stale_guard owns the first cross-resource transaction generation.
        // stale_guard 持有第一代跨资源事务。
        let stale_guard =
            begin_project_lifecycle_transition(&state, &project_id, [session_id], "归档")
                .expect("first lifecycle reservation should succeed");
        // replacement_id identifies the newer owner installed for both resources.
        // replacement_id 标识为两类资源安装的更新所有者。
        let replacement_id = Uuid::new_v4();
        {
            // guard replaces both rows before the stale guard exits.
            // guard 在过期守卫退出前替换两行。
            let mut guard = state.lock().expect("service state should lock");
            guard.session_turn_admission_reservations.insert(
                session_id,
                SessionTurnAdmissionReservation {
                    reservation_id: replacement_id,
                    kind: SessionTurnAdmissionReservationKind::LifecycleTransition,
                },
            );
            guard.project_admission_reservations.insert(
                ProjectAdmissionScope::Exact(project_id.clone()),
                ProjectAdmissionReservation {
                    reservation_id: replacement_id,
                    kind: ProjectAdmissionReservationKind::PromptSubmission,
                },
            );
        }

        drop(stale_guard);

        let guard = state.lock().expect("service state should lock after drop");
        assert_eq!(
            guard
                .session_turn_admission_reservations
                .get(&session_id)
                .map(|reservation| reservation.reservation_id),
            Some(replacement_id)
        );
        assert_eq!(
            guard
                .project_admission_reservations
                .get(&ProjectAdmissionScope::Exact(project_id))
                .map(|reservation| reservation.reservation_id),
            Some(replacement_id)
        );
    }

    /// Verifies a lifecycle Guard recovers a poisoned state lock, removes its rows, and preserves unrelated owners.
    /// 验证生命周期 Guard 会恢复中毒状态锁、删除自身行并保留无关所有者。
    ///
    /// # Parameters
    /// # 参数
    /// This test has no parameters.
    /// 此测试没有参数。
    ///
    /// # Returns
    /// # 返回值
    /// This test returns no value and fails if poisoned cleanup leaks owned rows or deletes unrelated reservations.
    /// 此测试无返回值；中毒清理泄漏自身行或删除无关预留时测试失败。
    #[test]
    fn lifecycle_guard_recovers_poisoned_lock_for_exact_cleanup() {
        // owned_session_id identifies the session reservation owned by the tested Guard.
        // owned_session_id 标识被测 Guard 拥有的会话预留。
        let owned_session_id = SessionId::from_u128(0x99);
        // unrelated_session_id identifies a reservation owned by a different transaction.
        // unrelated_session_id 标识由其他事务拥有的预留。
        let unrelated_session_id = SessionId::from_u128(0x9a);
        // owned_project_id identifies the project reservation owned by the tested Guard.
        // owned_project_id 标识被测 Guard 拥有的项目预留。
        let owned_project_id = String::from("project-poison-owned");
        // unrelated_project_scope identifies a project reservation outside the tested Guard target set.
        // unrelated_project_scope 标识被测 Guard 目标集合之外的项目预留。
        let unrelated_project_scope =
            ProjectAdmissionScope::Exact(String::from("project-poison-unrelated"));
        // unrelated_reservation_id identifies the transaction that must survive cleanup.
        // unrelated_reservation_id 标识必须在清理后保留的事务。
        let unrelated_reservation_id = Uuid::new_v4();
        // state owns the poisoned reservation tables exercised by Drop recovery.
        // state 持有 Drop 恢复所操作的中毒预留表。
        let state = create_service_bridge_state(Arc::new(LifecycleTransitionTestShellHost));
        // lifecycle_guard owns the exact session and project rows that must be removed after poisoning.
        // lifecycle_guard 拥有中毒后必须删除的精确会话与项目行。
        let lifecycle_guard = begin_project_lifecycle_transition(
            &state,
            &owned_project_id,
            [owned_session_id],
            "归档",
        )
        .expect("lifecycle reservation should succeed before poisoning");
        {
            // guard installs unrelated reservation owners before the mutex is poisoned.
            // guard 在互斥锁中毒前安装无关预留所有者。
            let mut guard = state.lock().expect("service state should lock");
            guard.session_turn_admission_reservations.insert(
                unrelated_session_id,
                SessionTurnAdmissionReservation {
                    reservation_id: unrelated_reservation_id,
                    kind: SessionTurnAdmissionReservationKind::LifecycleTransition,
                },
            );
            guard.project_admission_reservations.insert(
                unrelated_project_scope.clone(),
                ProjectAdmissionReservation {
                    reservation_id: unrelated_reservation_id,
                    kind: ProjectAdmissionReservationKind::LifecycleTransition,
                },
            );
        }
        // poisoned_state shares the exact mutex later recovered by the Guard Drop.
        // poisoned_state 共享随后由 Guard Drop 恢复的精确互斥锁。
        let poisoned_state = Arc::clone(&state);
        // poison_result proves the deliberate panic poisoned the service-state mutex.
        // poison_result 证明刻意 panic 已使服务状态互斥锁中毒。
        let poison_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // _guard keeps the mutex held until the deliberate panic poisons it.
            // _guard 将互斥锁保持到刻意 panic，使其被标记为中毒。
            let _guard = poisoned_state
                .lock()
                .expect("service state should lock before deliberate panic");
            panic!("poison lifecycle admission cleanup state");
        }));
        assert!(poison_result.is_err());

        drop(lifecycle_guard);

        // recovered_guard inspects the still-poisoned mutex contents after exact Drop cleanup.
        // recovered_guard 在精确 Drop 清理后检查仍标记中毒的互斥锁内容。
        let recovered_guard = match state.lock() {
            Ok(_) => panic!("service state should remain poison-marked"),
            Err(poisoned) => {
                // poisoned carries the post-cleanup state retained by the intentionally poisoned mutex.
                // poisoned 携带刻意中毒互斥锁保留的清理后状态。
                poisoned.into_inner()
            }
        };
        assert!(
            !recovered_guard
                .session_turn_admission_reservations
                .contains_key(&owned_session_id)
        );
        assert!(
            !recovered_guard
                .project_admission_reservations
                .contains_key(&ProjectAdmissionScope::Exact(owned_project_id))
        );
        assert_eq!(
            recovered_guard
                .session_turn_admission_reservations
                .get(&unrelated_session_id)
                .map(|reservation| reservation.reservation_id),
            Some(unrelated_reservation_id)
        );
        assert_eq!(
            recovered_guard
                .project_admission_reservations
                .get(&unrelated_project_scope)
                .map(|reservation| reservation.reservation_id),
            Some(unrelated_reservation_id)
        );
    }
}
