//! Retained workspace preflight for conversation submission.
//! 会话提交的保留工作区预检。

use vulcan_workspace_store::workspace_store::WorkspaceSnapshot;

use super::*;
use crate::{
    MaterializedPromptSession, ProjectAdmissionReservation, ProjectAdmissionReservationKind,
    ProjectAdmissionScope, PromptSessionMaterializationKind, PromptSessionTarget,
    ServiceBridgeState, SessionId, SessionTurnAdmissionReservation,
    SessionTurnAdmissionReservationKind, lock_service_state_for_admission_cleanup,
    session_hydration::PreservedWorkspaceRunState, workspace_session_summary_for_session,
};

/// Submission-scoped admission ownership retained until one materialized kernel turn starts.
/// 保留到一个已物化 kernel 回合启动为止的提交级准入所有权。
pub(crate) enum SubmissionTurnAdmission {
    /// A new-session submission needs no reservation because materialization generates a unique id.
    /// 新会话提交无需预留，因为物化阶段会生成唯一标识。
    Unreserved,
    /// An existing-session submission owns one exact reservation row.
    /// 已有会话提交拥有一个精确预留行。
    Existing(ExistingSubmissionTurnAdmission),
    /// An outer transaction retains one exact reservation accepted by this nested submission.
    /// 外层事务保留一个由此嵌套提交接受的精确预留。
    Preauthorized(ExistingSubmissionAdmissionAuthorization),
}

/// Owned existing-session reservation released only when this submission leaves scope.
/// 仅在本次提交离开作用域时释放的已有会话有所有权预留。
pub(crate) struct ExistingSubmissionTurnAdmission {
    /// Shared state containing the authoritative reservation row.
    /// 包含权威预留行的共享状态。
    state: SharedServiceBridgeState,
    /// Exact existing session reserved by this submission.
    /// 本次提交预留的精确已有会话。
    session_id: SessionId,
    /// Unique owner identity protecting against stale-guard deletion.
    /// 防止过期守卫删除替代预留的唯一所有者身份。
    reservation_id: Uuid,
}

impl SubmissionTurnAdmission {
    /// Returns whether this submission still owns the exact current prompt-admission row.
    /// 返回本次提交是否仍拥有精确的当前提示词准入行。
    ///
    /// # Parameters
    /// 参数
    /// `state` is the already locked authoritative service state and `session_id` is the existing Continue target.
    /// `state` 是已经锁定的权威服务状态，`session_id` 是已有 Continue 目标。
    ///
    /// # Returns
    /// 返回值
    /// `true` only when the current row has the same session, reservation id, and prompt-submission kind.
    /// 仅当当前行具有相同会话、预留标识及提示词提交类型时返回 `true`。
    pub(crate) fn owns_current_prompt_submission_reservation(
        &self,
        state: &ServiceBridgeState,
        session_id: SessionId,
    ) -> bool {
        // expectedReservationId is available only for an existing-session owner targeting this exact session.
        // expectedReservationId 仅在已有会话所有者指向该精确会话时存在。
        let expected_reservation_id = match self {
            Self::Existing(admission) if admission.session_id == session_id => {
                admission.reservation_id
            }
            Self::Preauthorized(authorization) if authorization.session_id == session_id => {
                authorization.reservation_id
            }
            _ => return false,
        };
        state
            .session_turn_admission_reservations
            .get(&session_id)
            .is_some_and(|reservation| {
                reservation.reservation_id == expected_reservation_id
                    && reservation.kind == SessionTurnAdmissionReservationKind::PromptSubmission
            })
    }

    /// Resolves the reservation id that may authorize the materialized session's kernel start.
    /// 解析可授权已物化会话启动 kernel 的预留标识。
    ///
    /// # Parameters
    /// 参数
    /// `materialized_session` is the authoritative existing-or-created materialization result.
    /// `materialized_session` 是权威的已有或新建物化结果。
    ///
    /// # Returns
    /// 返回值
    /// The exact existing-session reservation id, no reservation for a created session, or an ownership error.
    /// 返回精确的已有会话预留标识；新建会话返回无预留；所有权不一致时返回错误。
    pub(crate) fn kernel_reservation_id(
        &self,
        materialized_session: &MaterializedPromptSession,
    ) -> Result<Option<Uuid>, String> {
        match (self, materialized_session.kind) {
            (Self::Unreserved, PromptSessionMaterializationKind::Created) => Ok(None),
            (Self::Existing(_), PromptSessionMaterializationKind::Created) => Ok(None),
            (Self::Preauthorized(_), PromptSessionMaterializationKind::Created) => Ok(None),
            (Self::Existing(admission), PromptSessionMaterializationKind::Existing)
                if admission.session_id == materialized_session.session_id =>
            {
                Ok(Some(admission.reservation_id))
            }
            (Self::Preauthorized(authorization), PromptSessionMaterializationKind::Existing)
                if authorization.session_id == materialized_session.session_id =>
            {
                Ok(Some(authorization.reservation_id))
            }
            (Self::Unreserved, PromptSessionMaterializationKind::Existing) => Err(format!(
                "Existing session `{}` was materialized without submission admission ownership",
                materialized_session.session_id
            )),
            (Self::Existing(admission), PromptSessionMaterializationKind::Existing) => {
                Err(format!(
                    "Submission admission owns session `{}`, but materialization selected `{}`",
                    admission.session_id, materialized_session.session_id
                ))
            }
            (Self::Preauthorized(authorization), PromptSessionMaterializationKind::Existing) => {
                Err(format!(
                    "Preauthorized submission owns session `{}`, but materialization selected `{}`",
                    authorization.session_id, materialized_session.session_id
                ))
            }
        }
    }
}

impl Drop for ExistingSubmissionTurnAdmission {
    /// Releases only the reservation row still owned by this exact submission.
    /// 仅释放仍由本次精确提交拥有的预留行。
    fn drop(&mut self) {
        // guard recovers poisoned state only to remove this exact submission-owned reservation.
        // guard 仅为删除当前提交精确拥有的预留而恢复中毒状态。
        let mut guard =
            lock_service_state_for_admission_cleanup(&self.state, "existing-submission-turn guard");
        guard
            .session_turn_admission_reservations
            .retain(|session_id, reservation| {
                *session_id != self.session_id || reservation.reservation_id != self.reservation_id
            });
    }
}

/// Submission-scoped project ownership retained through materialization and workspace commit.
/// 跨物化与工作区提交保留的提交级项目所有权。
pub(crate) struct SubmissionProjectAdmission {
    /// Shared state containing the authoritative project reservation row.
    /// 包含权威项目预留行的共享状态。
    state: SharedServiceBridgeState,
    /// Exact or unresolved project scope owned by this submission.
    /// 本次提交拥有的精确或未解析项目作用域。
    scope: ProjectAdmissionScope,
    /// Unique owner identity protecting successor rows from stale cleanup.
    /// 保护后继行不被过期清理删除的唯一所有者身份。
    reservation_id: Uuid,
}

impl SubmissionProjectAdmission {
    /// Returns the exact preflight project selected for new-session materialization, when known.
    /// 返回预检为新会话物化选择的精确项目（若已知）。
    ///
    /// # Returns
    /// 返回值
    /// The exact project id, or `None` while selection remains globally unresolved.
    /// 返回精确项目标识；选择仍为全局未解析时返回 `None`。
    pub(crate) fn exact_project_id(&self) -> Option<&str> {
        match &self.scope {
            ProjectAdmissionScope::Exact(project_id) => Some(project_id.as_str()),
            ProjectAdmissionScope::UnresolvedSubmission => None,
        }
    }

    /// Verifies that materialization did not escape the project scope reserved by preflight.
    /// 校验物化结果没有逃逸预检预留的项目作用域。
    ///
    /// # Parameters
    /// 参数
    /// `materialized_session` carries the authoritative project selected by materialization.
    /// `materialized_session` 携带物化阶段选择的权威项目。
    ///
    /// # Returns
    /// 返回值
    /// Success for the reserved exact project or globally unresolved scope; otherwise an ownership error.
    /// 精确项目匹配或全局未解析作用域时成功，否则返回所有权错误。
    pub(crate) fn validate_materialized_session(
        &self,
        materialized_session: &MaterializedPromptSession,
    ) -> Result<(), String> {
        match (&self.scope, materialized_session.project_id.as_deref()) {
            (ProjectAdmissionScope::Exact(expected), Some(actual)) if expected == actual => Ok(()),
            (ProjectAdmissionScope::UnresolvedSubmission, _) => Ok(()),
            (ProjectAdmissionScope::Exact(expected), actual) => Err(format!(
                "Submission admission owns project `{expected}`, but materialization selected `{}`",
                actual.unwrap_or("<none>")
            )),
        }
    }
}

impl Drop for SubmissionProjectAdmission {
    /// Releases only the project reservation row still owned by this exact submission.
    /// 仅释放仍由本次精确提交拥有的项目预留行。
    fn drop(&mut self) {
        // guard recovers poisoned state only to remove this exact submission-owned project reservation.
        // guard 仅为删除当前提交精确拥有的项目预留而恢复中毒状态。
        let mut guard =
            lock_service_state_for_admission_cleanup(&self.state, "submission-project guard");
        guard
            .project_admission_reservations
            .retain(|scope, reservation| {
                scope != &self.scope || reservation.reservation_id != self.reservation_id
            });
    }
}

/// Single-lock retained-state result used before submission workspace loading or hydration.
/// 在提交工作区加载或水合前使用的单锁保留状态结果。
pub(crate) struct RetainedSubmissionPreflight {
    /// Turn-admission ownership retained across workspace load and materialization.
    /// 跨工作区加载与物化阶段保留的回合准入所有权。
    pub(crate) turn_admission: SubmissionTurnAdmission,
    /// Project ownership retained across workspace loading, materialization, and commit.
    /// 跨工作区加载、物化与提交保留的项目所有权。
    pub(crate) project_admission: SubmissionProjectAdmission,
    /// Minimal live-only state that must survive a fallback disk workspace reload.
    /// 回退到磁盘工作区重载时必须保留的最小仅内存状态。
    pub(crate) preserved_workspace_run_state: Option<PreservedWorkspaceRunState>,
    /// Whether the retained target session already owns concrete provider dialogue context.
    /// 保留目标会话是否已拥有具体 Provider 对话上下文。
    pub(crate) retained_dialogue_context_available: bool,
    /// Existing-session workspace clone or new-session single-project slice accepted as the submission base.
    /// 接受为提交基础的已有会话工作区克隆或新会话单项目切片。
    pub(crate) retained_workspace_snapshot: Option<WorkspaceSnapshot>,
}

/// Builds the bounded workspace slice used to accept one new conversation without historical session data.
/// 构建用于接纳单个新会话且不包含历史会话数据的有界工作区切片。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` is the retained authoritative workspace already visible to the composer.
/// `workspace_snapshot` 是输入区已经可见的保留权威工作区。
/// `project_id` identifies the exact active project selected for the new conversation.
/// `project_id` 标识为新会话选择的精确活动项目。
///
/// # Returns
/// 返回值
/// A single-project empty-session slice, or an explicit initialization mismatch error.
/// 返回单项目空会话切片，或明确的初始化不一致错误。
fn new_submission_workspace_slice(
    workspace_snapshot: &WorkspaceSnapshot,
    project_id: &str,
) -> Result<WorkspaceSnapshot, String> {
    // project is the exact retained project whose summary was already offered to the user.
    // project 是已经向用户展示的精确保留项目摘要。
    let mut project = workspace_snapshot
        .task_projects
        .iter()
        .find(|candidate| candidate.id == project_id)
        .cloned()
        .ok_or_else(|| {
            format!("项目 `{project_id}` 尚未进入当前工作区，请等待工作区初始化完成后重试。")
        })?;
    // Historical membership is intentionally excluded because this transaction creates one independent session.
    // 此事务只创建一个独立会话，因此有意排除历史会话成员关系。
    project.session_ids.clear();
    let mut expanded_task_projects = std::collections::BTreeSet::new();
    if workspace_snapshot
        .expanded_task_projects
        .contains(project_id)
    {
        expanded_task_projects.insert(project_id.to_string());
    }
    Ok(WorkspaceSnapshot {
        task_projects: vec![project],
        archived_task_projects: Vec::new(),
        sessions: Vec::new(),
        archived_sessions: Vec::new(),
        session_messages: std::collections::BTreeMap::new(),
        archived_session_messages: std::collections::BTreeMap::new(),
        session_compactions: std::collections::BTreeMap::new(),
        archived_session_compactions: std::collections::BTreeMap::new(),
        session_runtime_state: std::collections::BTreeMap::new(),
        archived_session_runtime_state: std::collections::BTreeMap::new(),
        session_run_phases: std::collections::BTreeMap::new(),
        expanded_task_projects,
        selected_session: SessionId::nil(),
        hydrated_session_ids: std::collections::BTreeSet::new(),
    })
}

/// Builds one existing-session submission slice without cloning presentation history or unrelated state.
/// 构建单个已有会话的提交切片，且不复制展示历史或无关状态。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` is the retained authoritative workspace.
/// `workspace_snapshot` 是保留的权威工作区。
/// `session_id` identifies the exact active session admitted for submission.
/// `session_id` 标识获准提交的精确活动会话。
///
/// # Returns
/// 返回值
/// A single-project, single-session detached slice with canonical runtime context but no transcript or compaction copies,
/// or an explicit ownership mismatch.
/// 返回携带规范运行时上下文但不复制转录与压缩记录的单项目、单会话分离切片，或明确的归属不一致错误。
pub(crate) fn existing_submission_workspace_slice(
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
) -> Result<WorkspaceSnapshot, String> {
    // session is the sole active summary copied into the detached transaction.
    // session 是复制到分离事务中的唯一活动摘要。
    let session = workspace_session_summary_for_session(workspace_snapshot, session_id)
        .cloned()
        .ok_or_else(|| {
            format!("Conversation `{session_id}` is not retained as an active session.")
        })?;
    let project_id =
        owning_project_id_for_session(workspace_snapshot, session_id).ok_or_else(|| {
            format!("Conversation `{session_id}` has no retained active project owner.")
        })?;
    // project retains only the admitted membership so later materialization cannot inspect unrelated sessions.
    // project 仅保留已准入成员关系，使后续物化无法检查无关会话。
    let mut project = workspace_snapshot
        .task_projects
        .iter()
        .find(|candidate| candidate.id == project_id)
        .cloned()
        .ok_or_else(|| format!("Project `{project_id}` is not retained for `{session_id}`."))?;
    project.session_ids.clear();
    project.session_ids.push(session_id);
    let mut expanded_task_projects = std::collections::BTreeSet::new();
    if workspace_snapshot
        .expanded_task_projects
        .contains(&project_id)
    {
        expanded_task_projects.insert(project_id);
    }
    // Presentation history remains authoritative in retained state; this detached transaction carries only records it creates.
    // 展示历史继续以保留状态为权威；该分离事务只携带自身新建的记录。
    let session_messages = std::collections::BTreeMap::new();
    let session_compactions = std::collections::BTreeMap::new();
    let mut session_runtime_state = std::collections::BTreeMap::new();
    if let Some(runtime_state) = workspace_snapshot.session_runtime_state.get(&session_id) {
        session_runtime_state.insert(session_id, runtime_state.clone());
    }
    let mut session_run_phases = std::collections::BTreeMap::new();
    if let Some(run_phase) = workspace_snapshot.session_run_phases.get(&session_id) {
        session_run_phases.insert(session_id, *run_phase);
    }
    let mut hydrated_session_ids = std::collections::BTreeSet::new();
    if workspace_snapshot
        .hydrated_session_ids
        .contains(&session_id)
    {
        hydrated_session_ids.insert(session_id);
    }
    Ok(WorkspaceSnapshot {
        task_projects: vec![project],
        archived_task_projects: Vec::new(),
        sessions: vec![session],
        archived_sessions: Vec::new(),
        session_messages,
        archived_session_messages: std::collections::BTreeMap::new(),
        session_compactions,
        archived_session_compactions: std::collections::BTreeMap::new(),
        session_runtime_state,
        archived_session_runtime_state: std::collections::BTreeMap::new(),
        session_run_phases,
        expanded_task_projects,
        selected_session: session_id,
        hydrated_session_ids,
    })
}

/// Validates retained admission gates and captures one coherent submission snapshot under a single state lock.
/// 在单个状态锁内校验保留准入闸门并捕获一致的提交快照。
///
/// # Parameters
/// # 参数
/// `state` owns admission reservations, lifecycle reservations, kernels, and the retained workspace.
/// `state` 持有准入预留、生命周期预留、内核与保留工作区。
/// `project_id` is the request project candidate used for draft submissions or explicit targeting; lifecycle matching trims it locally.
/// `project_id` 是草稿提交或显式目标选择使用的请求项目候选；生命周期匹配会在本地裁剪它。
/// `target` is the already-validated create-or-existing submission target.
/// `target` 是已完成校验的新建或已有会话提交目标。
///
/// # Returns
/// # 返回值
/// One coherent retained preflight result, or the exact admission/lifecycle rejection.
/// 返回一致的保留预检结果，或精确的准入/生命周期拒绝错误。
pub(crate) fn prepare_retained_submission_preflight(
    state: &SharedServiceBridgeState,
    project_id: Option<&str>,
    target: PromptSessionTarget,
    preauthorized_turn_admission: Option<ExistingSubmissionAdmissionAuthorization>,
) -> Result<RetainedSubmissionPreflight, String> {
    // submission_session_id is derived once from the strong target for retained-state lookups.
    // submission_session_id 从强类型目标一次性派生，用于保留状态查找。
    let submission_session_id = target.existing_session_id();
    if let Some(authorization) = preauthorized_turn_admission
        && submission_session_id != Some(authorization.session_id)
    {
        return Err(format!(
            "Preauthorized submission owns session `{}`, but request targets `{}`.",
            authorization.session_id,
            submission_session_id
                .map(|session_id| session_id.to_string())
                .unwrap_or_else(|| String::from("<new>"))
        ));
    }
    // guard guarantees every admission decision and retained snapshot field observes the same service-state revision.
    // guard 保证每项准入决策与保留快照字段观察同一个服务状态版本。
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    if let Some(session_id) = submission_session_id
        && guard
            .session_terminal_persistence_owners
            .contains_key(&session_id)
    {
        return Err(format!(
            "会话 `{session_id}` 正在可靠提交终态，暂时不能启动新的提示词。"
        ));
    }
    if let Some(session_id) = submission_session_id
        && let Some(reservation) = guard.session_turn_admission_reservations.get(&session_id)
    {
        let matches_preauthorization = preauthorized_turn_admission.is_some_and(|authorization| {
            authorization.session_id == session_id
                && authorization.reservation_id == reservation.reservation_id
                && reservation.kind == SessionTurnAdmissionReservationKind::PromptSubmission
        });
        if !matches_preauthorization {
            return Err(format!(
                "会话 `{session_id}` 已由 {} 预留，无法提交新的提示词。",
                reservation.kind.diagnostic_label()
            ));
        }
    } else if let Some(authorization) = preauthorized_turn_admission {
        return Err(format!(
            "Preauthorized submission reservation for session `{}` is no longer current.",
            authorization.session_id
        ));
    }
    if let Some(session_id) = submission_session_id
        && let Some(workspace_snapshot) = guard.workspace_snapshot.as_ref()
        && workspace_session_summary_for_session(workspace_snapshot, session_id).is_none()
        && workspace_snapshot
            .archived_sessions
            .iter()
            .any(|session| session.id == session_id)
    {
        return Err(format!("Conversation `{session_id}` is archived"));
    }

    // retained_owner_project_id is the authoritative owner only when an existing retained session was selected.
    // retained_owner_project_id 仅在选择已有保留会话时表示其权威所属项目。
    let retained_owner_project_id = submission_session_id.and_then(|session_id| {
        guard
            .workspace_snapshot
            .as_ref()
            .and_then(|workspace_snapshot| {
                owning_project_id_for_session(workspace_snapshot, session_id)
            })
    });
    // requested_project_id is the normalized explicit project candidate for a new-session submission.
    // requested_project_id 是新会话提交的标准化显式项目候选。
    let requested_project_id = project_id
        .map(str::trim)
        .filter(|project_id| !project_id.is_empty());
    // project_scope locks the existing owner, explicit new target, retained default, or global unresolved selection.
    // project_scope 锁定已有所属项目、显式新目标、保留默认项目或全局未解析选择。
    let project_scope = if submission_session_id.is_some() {
        retained_owner_project_id
            .map(ProjectAdmissionScope::Exact)
            .unwrap_or(ProjectAdmissionScope::UnresolvedSubmission)
    } else if let Some(requested_project_id) = requested_project_id {
        ProjectAdmissionScope::Exact(requested_project_id.to_string())
    } else if let Some(default_project_id) = guard
        .workspace_snapshot
        .as_ref()
        .and_then(|workspace_snapshot| workspace_snapshot.task_projects.first())
        .map(|project| project.id.clone())
    {
        ProjectAdmissionScope::Exact(default_project_id)
    } else {
        ProjectAdmissionScope::UnresolvedSubmission
    };
    // project_reservation_id uniquely owns project admission independently from the session row.
    // project_reservation_id 独立于会话行唯一拥有项目准入。
    let project_reservation_id = Uuid::new_v4();
    match &project_scope {
        ProjectAdmissionScope::Exact(project_id) => {
            if let Some(reservation) = guard
                .project_admission_reservations
                .get(&ProjectAdmissionScope::UnresolvedSubmission)
                .or_else(|| guard.project_admission_reservations.get(&project_scope))
            {
                return Err(format!(
                    "项目 `{project_id}` 已由 {} 预留，无法提交新的提示词。",
                    reservation.kind.diagnostic_label()
                ));
            }
        }
        ProjectAdmissionScope::UnresolvedSubmission => {
            if let Some((reserved_scope, reservation)) =
                guard.project_admission_reservations.iter().next()
            {
                return Err(format!(
                    "项目选择已与作用域 `{reserved_scope:?}` 的 {} 冲突，无法提交新的提示词。",
                    reservation.kind.diagnostic_label()
                ));
            }
        }
    }
    // retained_workspace_snapshot is built only after cheap admission checks pass and always
    // contains at most one project and one admitted session.
    // retained_workspace_snapshot 仅在低成本准入检查通过后构建，且始终至多包含一个项目与
    // 一个已准入会话。
    let retained_workspace_snapshot = if let Some(session_id) = submission_session_id {
        match guard.workspace_snapshot.as_ref() {
            Some(snapshot)
                if retained_submission_snapshot_is_usable(
                    snapshot,
                    project_id,
                    submission_session_id,
                ) =>
            {
                Some(existing_submission_workspace_slice(snapshot, session_id)?)
            }
            _ => None,
        }
    } else {
        let exact_project_id = match &project_scope {
            ProjectAdmissionScope::Exact(project_id) => project_id.as_str(),
            ProjectAdmissionScope::UnresolvedSubmission => {
                return Err(String::from(
                    "工作区仍在初始化，当前没有可用于创建会话的项目。",
                ));
            }
        };
        let retained_workspace = guard
            .workspace_snapshot
            .as_ref()
            .ok_or_else(|| String::from("工作区仍在初始化，请等待项目列表加载完成后再发送。"))?;
        Some(new_submission_workspace_slice(
            retained_workspace,
            exact_project_id,
        )?)
    };
    guard.project_admission_reservations.insert(
        project_scope.clone(),
        ProjectAdmissionReservation {
            reservation_id: project_reservation_id,
            kind: ProjectAdmissionReservationKind::PromptSubmission,
        },
    );

    // active_kernel_sessions defines the live-only run state that a disk fallback must not erase.
    // active_kernel_sessions 定义磁盘回退不得清除的仅内存运行状态。
    let active_kernel_sessions = active_kernel_session_ids(&guard);
    // retained_dialogue_context_available records whether later hydration can remain on the retained fast path.
    // retained_dialogue_context_available 记录后续水合是否可以保持在保留态快速路径。
    let retained_dialogue_context_available = submission_session_id.is_some_and(|session_id| {
        guard
            .workspace_snapshot
            .as_ref()
            .is_some_and(|workspace_snapshot| {
                retained_submission_snapshot_has_session_context(workspace_snapshot, session_id)
            })
    });
    // preserved_workspace_run_state captures live state from the same revision as the selected base snapshot.
    // preserved_workspace_run_state 从与所选基础快照相同的版本捕获实时状态。
    let preserved_workspace_run_state = capture_preserved_workspace_run_state(
        guard.workspace_snapshot.as_ref(),
        &active_kernel_sessions,
        submission_session_id,
    );

    // turn_admission closes every competing start after this coherent preflight revision.
    // turn_admission 关闭此一致预检版本之后的全部竞争启动。
    let turn_admission = if let Some(session_id) = submission_session_id {
        if let Some(authorization) = preauthorized_turn_admission {
            SubmissionTurnAdmission::Preauthorized(authorization)
        } else {
            // reservation_id uniquely owns the row so stale request cleanup cannot remove a successor.
            // reservation_id 唯一拥有该行，使过期请求清理无法删除后继预留。
            let reservation_id = Uuid::new_v4();
            guard.session_turn_admission_reservations.insert(
                session_id,
                SessionTurnAdmissionReservation {
                    reservation_id,
                    kind: SessionTurnAdmissionReservationKind::PromptSubmission,
                },
            );
            SubmissionTurnAdmission::Existing(ExistingSubmissionTurnAdmission {
                state: state.clone(),
                session_id,
                reservation_id,
            })
        }
    } else {
        SubmissionTurnAdmission::Unreserved
    };
    // project_admission retains the row inserted in the same coherent preflight transaction.
    // project_admission 保留在同一个一致预检事务中插入的项目行。
    let project_admission = SubmissionProjectAdmission {
        state: state.clone(),
        scope: project_scope,
        reservation_id: project_reservation_id,
    };
    drop(guard);

    Ok(RetainedSubmissionPreflight {
        turn_admission,
        project_admission,
        preserved_workspace_run_state,
        retained_dialogue_context_available,
        retained_workspace_snapshot,
    })
}
