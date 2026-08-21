//! Workspace session persistence tasks.
//! 工作区会话持久化任务。

use super::*;
use std::future::Future;

/// Persists only the changed session after a WebUI runtime mutation.
/// 在 WebUI 运行时变更后仅持久化被修改的单个会话。
///
/// # Parameters
/// 参数
/// `state` optionally supplies the authoritative retained workspace and invalidates older background writers.
/// `state` 可选提供权威保留工作区，并使更早的后台写入者失效。
/// `controller_runtime` owns the workspace-store persistence route.
/// `controller_runtime` 持有工作区存储持久化路由。
/// `workspace_snapshot` is the caller-owned fallback used only when no retained service state exists.
/// `workspace_snapshot` 是仅在不存在保留服务状态时使用的调用方候选回退。
/// `session_id` selects the exact regular session to persist.
/// `session_id` 选择需要持久化的精确普通会话。
///
/// # Returns
/// 返回值
/// Success after the newest authoritative session revision has crossed the shared write gate.
/// 最新权威会话修订通过共享写入闸门后返回成功。
pub(crate) async fn persist_workspace_session_update(
    state: Option<&SharedServiceBridgeState>,
    controller_runtime: ControllerRuntimeHandle,
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
) -> Result<(), String> {
    if let Some(state) = state {
        let (_persist_guard, authoritative_snapshot) =
            lock_latest_retained_workspace_session_persistence_snapshot(state, session_id).await?;
        return persist_workspace_session_update_unlocked(
            controller_runtime,
            &authoritative_snapshot,
            session_id,
        )
        .await;
    }
    let persist_lock = webui_workspace_persist_lock();
    let _persist_guard = persist_lock.lock().await;
    persist_workspace_session_update_unlocked(controller_runtime, workspace_snapshot, session_id)
        .await
}

/// Acquires the shared write gate and then clones the newest retained workspace revision.
/// 取得共享写入闸门后再克隆最新保留工作区修订。
///
/// # Parameters
/// 参数
/// `state` owns the authoritative retained workspace and background persistence versions.
/// `state` 持有权威保留工作区与后台持久化版本。
/// `session_id` selects the session whose older queued writers must be invalidated.
/// `session_id` 选择必须使旧排队写入者失效的会话。
///
/// # Returns
/// 返回值
/// The owned write guard and a snapshot read after that guard was acquired.
/// 返回有所有权的写入守卫，以及取得该守卫后读取的快照。
pub(crate) async fn lock_latest_retained_workspace_session_persistence_snapshot(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
) -> Result<(tokio::sync::OwnedMutexGuard<()>, WorkspaceSnapshot), String> {
    invalidate_webui_session_persist_version(state, session_id)?;
    let persist_guard = webui_workspace_persist_lock().lock_owned().await;
    // authoritativeSnapshot is read only after acquiring the write gate, so a newer retained mutation that queued first can never be overwritten by an older detached caller snapshot.
    // authoritativeSnapshot 仅在取得写入闸门后读取，使先排队的较新保留态变更绝不会被更旧的调用方分离快照反向覆盖。
    let authoritative_snapshot =
        current_workspace_snapshot_from_shared_state(state)?.ok_or_else(|| {
            String::from(
                "cannot persist a retained session without an authoritative workspace snapshot",
            )
        })?;
    Ok((persist_guard, authoritative_snapshot))
}

/// Persists one session snapshot after the caller has acquired the shared WebUI write gate.
/// 在调用方已取得共享 WebUI 写入闸门后持久化单个会话快照。
///
/// # Parameters
/// 参数
/// `controller_runtime` owns the workspace store runtime bridge.
/// `controller_runtime` 持有工作区存储运行时桥。
/// `workspace_snapshot` is the workspace snapshot that contains the changed session.
/// `workspace_snapshot` 是包含变更会话的工作区快照。
/// `session_id` selects the session to persist.
/// `session_id` 选择要持久化的会话。
///
/// # Returns
/// 返回值
/// `Ok(())` when the session has been saved.
/// 会话保存完成时返回 `Ok(())`。
pub(crate) async fn persist_workspace_session_update_unlocked(
    controller_runtime: ControllerRuntimeHandle,
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
) -> Result<(), String> {
    let (session, archived) = workspace_session_with_archive_status(workspace_snapshot, session_id)
        .ok_or_else(|| format!("单会话保存失败：未找到会话：{session_id}"))?;
    let session_messages = if archived {
        &workspace_snapshot.archived_session_messages
    } else {
        &workspace_snapshot.session_messages
    };
    let session_runtime_state = if archived {
        &workspace_snapshot.archived_session_runtime_state
    } else {
        &workspace_snapshot.session_runtime_state
    };
    let runtime_state = session_runtime_state.get(&session_id);
    let rebuilt_messages;
    let messages = if let Some(runtime_state) = runtime_state {
        rebuilt_messages = merge_session_messages_with_conversation_state_records(
            session_messages
                .get(&session_id)
                .map(Vec::as_slice)
                .unwrap_or(&[]),
            &runtime_state.conversation_state,
        );
        rebuilt_messages.as_slice()
    } else if let Some(messages) = session_messages.get(&session_id) {
        messages.as_slice()
    } else {
        &[]
    };
    let task_projects = workspace_task_projects_for_persistence(workspace_snapshot);

    vulcan_workspace_store::workspace_store::persist_single_session_update(
        controller_runtime,
        &task_projects,
        session,
        archived,
        messages,
        runtime_state,
    )
    .await
}

/// Returns whether a WebUI snapshot owns the id as a regular workspace-store session.
/// 返回 WebUI 快照是否将该标识拥有为普通 workspace-store 会话。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` is the retained WebUI workspace snapshot.
/// `workspace_snapshot` 是当前保留的 WebUI 工作区快照。
/// `session_id` is the candidate session identifier.
/// `session_id` 是候选会话标识。
///
/// # Returns
/// 返回值
/// `true` when the id belongs to a normal active or archived session, not a persisted TaskSession.
/// 当该标识属于普通活动或归档会话、而不是持久化 TaskSession 时返回 `true`。
pub(crate) fn workspace_snapshot_contains_regular_session(
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
) -> bool {
    workspace_session_with_archive_status(workspace_snapshot, session_id).is_some()
}

/// Returns one session summary and whether it comes from the archived list.
/// 返回单个会话摘要以及它是否来自归档列表。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` is the retained WebUI workspace snapshot.
/// `workspace_snapshot` 是保留的 WebUI 工作区快照。
/// `session_id` identifies the session to find in active or archived rows.
/// `session_id` 标识需要在活动或归档行中查找的会话。
///
/// # Returns
/// 返回值
/// The matched session summary and archived flag, or `None` when absent.
/// 返回匹配的会话摘要和归档标记；不存在时返回 `None`。
pub(crate) fn workspace_session_with_archive_status(
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
) -> Option<(&SessionSummary, bool)> {
    workspace_snapshot
        .sessions
        .iter()
        .find(|candidate| candidate.id == session_id)
        .map(|session| (session, false))
        .or_else(|| {
            workspace_snapshot
                .archived_sessions
                .iter()
                .find(|candidate| candidate.id == session_id)
                .map(|session| (session, true))
        })
}

/// Collects active and archived task projects for workspace-store persistence calls.
/// 为 workspace-store 持久化调用收集活动与归档任务项目。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` is the retained WebUI workspace snapshot.
/// `workspace_snapshot` 是保留的 WebUI 工作区快照。
///
/// # Returns
/// 返回值
/// A cloned project list containing active projects followed by archived projects.
/// 返回克隆后的项目列表，顺序为活动项目在前、归档项目在后。
pub(crate) fn workspace_task_projects_for_persistence(
    workspace_snapshot: &WorkspaceSnapshot,
) -> Vec<TaskProjectSummary> {
    workspace_task_projects_including_archived(workspace_snapshot)
        .cloned()
        .collect()
}

/// Returns whether the retained workspace snapshot contains one active project.
/// 返回保留工作区快照是否包含一个活动项目。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` is the current retained WebUI workspace snapshot.
/// `workspace_snapshot` 是当前保留的 WebUI 工作区快照。
/// `project_id` is the project id required for a new draft submission.
/// `project_id` 是新建草稿提交所需的项目标识。
///
/// # Returns
/// 返回值
/// `true` when the active project list contains the requested project id.
/// 当活动项目列表包含请求的项目标识时返回 `true`。
pub(crate) fn workspace_snapshot_contains_active_project(
    workspace_snapshot: &WorkspaceSnapshot,
    project_id: &str,
) -> bool {
    workspace_snapshot
        .task_projects
        .iter()
        .any(|project| project.id == project_id)
}

/// Spawns one session-versioned persistence task behind the shared workspace write gate.
/// 在共享工作区写入闸门后启动一个按会话版本保护的持久化任务。
///
/// # Parameters
/// 参数
/// `state` owns the session persistence version table.
/// `state` 持有会话持久化版本表。
/// `session_id` selects the session whose write version is being checked.
/// `session_id` 选择需要检查写入版本的会话。
/// `version_error_label` is the diagnostic text used when version allocation fails.
/// `version_error_label` 是分配版本失败时使用的诊断文本。
/// `persist` performs the concrete persistence write after the version is still current.
/// `persist` 在版本仍为当前版本后执行具体持久化写入。
fn spawn_session_versioned_workspace_persist_task<F, Fut>(
    state: SharedServiceBridgeState,
    session_id: SessionId,
    version_error_label: &'static str,
    persist: F,
) where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    // session_persist_version fences only the ordinary session snapshot.
    // session_persist_version 仅隔离普通会话快照。
    let session_persist_version =
        match next_webui_session_runtime_persist_version(&state, session_id) {
            Ok(version) => version,
            Err(error) => {
                eprintln!("[webui-service] {version_error_label}: {error}");
                return;
            }
        };
    spawn_service_task(async move {
        let persist_lock = webui_workspace_persist_lock();
        let _persist_guard = persist_lock.lock().await;
        if !webui_session_runtime_persist_version_is_current(
            &state,
            session_id,
            session_persist_version,
        ) {
            return;
        }
        persist().await;
    });
}

/// Persists one WebUI session snapshot in the background without blocking visible UI updates.
/// 在后台持久化一个 WebUI 会话快照，不阻塞可见 UI 更新。
///
/// # Parameters
/// 参数
/// `controller_runtime` owns the workspace store runtime bridge.
/// `controller_runtime` 持有工作区存储运行时桥。
/// `workspace_snapshot` is the already-rendered snapshot to save.
/// `workspace_snapshot` 是已经渲染过、待保存的快照。
/// `session_id` selects the session that changed.
/// `session_id` 选择发生变更的会话。
/// `reason` labels the write site for diagnostics.
/// `reason` 标记写入来源，供诊断使用。
pub(crate) fn spawn_workspace_session_persist_task(
    state: SharedServiceBridgeState,
    controller_runtime: ControllerRuntimeHandle,
    workspace_snapshot: WorkspaceSnapshot,
    session_id: SessionId,
    reason: Option<String>,
) {
    if !workspace_snapshot_contains_regular_session(&workspace_snapshot, session_id) {
        return;
    }
    spawn_session_versioned_workspace_persist_task(
        state,
        session_id,
        "background persist version failed",
        move || async move {
            if let Err(error) = persist_workspace_session_update_unlocked(
                controller_runtime,
                &workspace_snapshot,
                session_id,
            )
            .await
            {
                let reason = reason.unwrap_or_else(|| String::from("session_update"));
                eprintln!("[webui-service] background persist failed ({reason}): {error}");
            }
        },
    );
}

/// Persists only one WebUI session runtime-state document in the background.
/// 在后台仅持久化一个 WebUI 会话运行态文档。
///
/// # Parameters
/// 参数
/// `state` owns the retained service state and per-session write versions.
/// `state` 持有保留服务状态与按会话划分的写入版本。
/// `controller_runtime` owns the workspace store runtime bridge.
/// `controller_runtime` 持有工作区存储运行时桥。
/// `workspace_snapshot` is the already-rendered snapshot containing the runtime state.
/// `workspace_snapshot` 是已经渲染过且包含运行态的快照。
/// `session_id` selects the session runtime-state document to persist.
/// `session_id` 选择需要持久化的会话运行态文档。
/// `reason` labels the write site for diagnostics.
/// `reason` 标记写入来源，供诊断使用。
pub(crate) fn spawn_workspace_session_runtime_state_persist_task(
    state: SharedServiceBridgeState,
    controller_runtime: ControllerRuntimeHandle,
    workspace_snapshot: WorkspaceSnapshot,
    session_id: SessionId,
    reason: Option<String>,
) {
    if !workspace_snapshot_contains_regular_session(&workspace_snapshot, session_id) {
        return;
    }
    spawn_session_versioned_workspace_persist_task(
        state,
        session_id,
        "runtime-state persist version failed",
        move || async move {
            if let Err(error) = persist_workspace_session_update_unlocked(
                controller_runtime,
                &workspace_snapshot,
                session_id,
            )
            .await
            {
                let reason_label = reason.as_deref().unwrap_or("runtime_state");
                eprintln!(
                    "[webui-service] background runtime-state session persist failed ({reason_label}): {error}"
                );
            }
        },
    );
}

/// Spawns a background transcript write from an already-scoped session message snapshot.
/// 从已经限定到单个会话的消息快照启动后台转录写入。
///
/// # Parameters
/// 参数
/// `state` owns the live transcript persistence version table.
/// `state` 持有运行中转录持久化版本表。
/// `controller_runtime` provides workspace persistence.
/// `controller_runtime` 提供工作区持久化能力。
/// `session_id` selects the session transcript to persist.
/// `session_id` 选择要持久化的会话转录。
/// `loaded_messages` is the complete retained transcript for only that session.
/// `loaded_messages` 是仅属于该会话的完整保留转录。
/// `reason` is an optional diagnostic label for logs.
/// `reason` 是用于日志的可选诊断标签。
pub(crate) fn spawn_session_transcript_messages_persist_task(
    state: SharedServiceBridgeState,
    controller_runtime: ControllerRuntimeHandle,
    session_id: SessionId,
    loaded_messages: Vec<SessionMessageEntry>,
    reason: Option<String>,
) {
    if loaded_messages.is_empty() {
        return;
    }
    let persist_version = match next_webui_session_transcript_persist_version(&state, session_id) {
        Ok(version) => version,
        Err(error) => {
            eprintln!("[webui-service] live transcript persist version failed: {error}");
            return;
        }
    };
    spawn_service_task(async move {
        let persist_lock = webui_workspace_persist_lock();
        let _persist_guard = persist_lock.lock().await;
        if !webui_session_transcript_persist_version_is_current(&state, session_id, persist_version)
        {
            return;
        }
        if let Err(error) =
            vulcan_workspace_store::workspace_store::persist_single_session_transcript_messages(
                controller_runtime,
                session_id,
                &loaded_messages,
            )
            .await
        {
            let reason_label = reason.as_deref().unwrap_or("live_transcript");
            eprintln!(
                "[webui-service] background live transcript persist failed ({reason_label}): {error}"
            );
        }
    });
}

/// Spawns a background write for one session addon-state document.
/// 为单个会话附属状态文档启动后台写入任务。
///
/// # Parameters
/// 参数
/// `app` is the event emitter used after the file write succeeds.
/// `app` 是文件写入成功后使用的事件发送器。
/// `state` owns the addon persistence version table used to reject stale writes.
/// `state` 持有用于拒绝过期写入的附属状态持久化版本表。
/// `controller_runtime` is the data-plane controller handle.
/// `controller_runtime` 是数据平面控制器句柄。
/// `workspace_snapshot` supplies session ownership and runtime compatibility fields.
/// `workspace_snapshot` 提供会话归属与运行态兼容字段。
/// `session_id` selects the addon document to persist.
/// `session_id` 选择要持久化的附属状态文档。
/// `addon_state` is the already-derived addon document to persist.
/// `addon_state` 是已经派生完成且需要持久化的附属状态文档。
/// `changed_namespaces` are the namespace keys represented by this write.
/// `changed_namespaces` 是本次写入代表的命名空间键。
/// `reason` labels the write site for diagnostics.
/// `reason` 标记写入来源，供诊断使用。
pub(crate) fn spawn_workspace_session_addon_state_persist_task(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    controller_runtime: ControllerRuntimeHandle,
    workspace_snapshot: WorkspaceSnapshot,
    session_id: SessionId,
    addon_state: SessionAddonState,
    changed_namespaces: Vec<SessionAddonNamespace>,
    reason: Option<String>,
) {
    if !workspace_snapshot_contains_regular_session(&workspace_snapshot, session_id) {
        return;
    }
    let Some((session, archived)) =
        workspace_session_with_archive_status(&workspace_snapshot, session_id)
            .map(|(session, archived)| (session.clone(), archived))
    else {
        return;
    };
    let task_projects = workspace_task_projects_for_persistence(&workspace_snapshot);
    spawn_session_addon_state_persist_task(
        app,
        state,
        controller_runtime,
        SessionAddonPersistenceScope {
            task_projects,
            session,
            archived,
        },
        addon_state,
        changed_namespaces,
        reason,
    );
}

/// Session-scoped ownership data required to persist one addon document.
/// 持久化单个附属文档所需的会话级所有权数据。
pub(crate) struct SessionAddonPersistenceScope {
    /// Project ownership index required by workspace persistence.
    /// 工作区持久化所需的项目所有权索引。
    pub(crate) task_projects: Vec<TaskProjectSummary>,
    /// Single active or archived session summary being updated.
    /// 正在更新的单个活动或归档会话摘要。
    pub(crate) session: SessionSummary,
    /// Whether the session belongs to archived storage.
    /// 会话是否属于归档存储。
    pub(crate) archived: bool,
}

/// Spawns a background addon write from one pre-scoped session persistence context.
/// 从一个预先限定的会话持久化上下文启动后台附属状态写入。
///
/// # Parameters
/// 参数
/// `app` emits the post-persistence addon revision event.
/// `app` 发出持久化后的附属状态修订事件。
/// `state` owns namespace persistence versions.
/// `state` 持有命名空间持久化版本。
/// `controller_runtime` provides the data-plane persistence handle.
/// `controller_runtime` 提供数据面持久化句柄。
/// `scope` contains the project index, session summary, and archive ownership required by persistence.
/// `scope` 包含持久化所需的项目索引、会话摘要与归档所有权。
/// `addon_state` is the already-derived addon document.
/// `addon_state` 是已经派生完成的附属状态文档。
/// `changed_namespaces` identifies namespaces represented by this write.
/// `changed_namespaces` 标识本次写入代表的命名空间。
/// `reason` labels diagnostics.
/// `reason` 标记诊断信息。
pub(crate) fn spawn_session_addon_state_persist_task(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    controller_runtime: ControllerRuntimeHandle,
    scope: SessionAddonPersistenceScope,
    addon_state: SessionAddonState,
    changed_namespaces: Vec<SessionAddonNamespace>,
    reason: Option<String>,
) {
    let session_id = scope.session.id;
    let mut persist_versions = Vec::new();
    for namespace in &changed_namespaces {
        match next_webui_session_addon_persist_version(&state, session_id, *namespace) {
            Ok(version) => persist_versions.push((*namespace, version)),
            Err(error) => {
                eprintln!(
                    "[webui-service] background session-addon persist version failed: {error}"
                );
                return;
            }
        }
    }
    spawn_service_task(async move {
        let persist_lock = webui_workspace_persist_lock();
        let _persist_guard = persist_lock.lock().await;
        if !webui_session_addon_persist_versions_are_current(&state, session_id, &persist_versions)
        {
            return;
        }
        match vulcan_workspace_store::workspace_store::persist_single_session_addon_state_update(
            controller_runtime,
            &scope.task_projects,
            &scope.session,
            scope.archived,
            &addon_state,
        )
        .await
        {
            Ok(merged_state) => {
                emit_session_addon_changed_event(
                    &app,
                    session_id,
                    &changed_namespaces,
                    merged_state.revision,
                    latest_session_addon_updated_at(&merged_state),
                );
            }
            Err(error) => {
                let reason_label = reason.as_deref().unwrap_or("session_addon");
                eprintln!(
                    "[webui-service] background session-addon persist failed ({reason_label}): {error}"
                );
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a workspace snapshot with one active and one archived session/project row.
    /// 构建包含一条活动与一条归档会话/项目行的工作区快照。
    fn test_workspace_snapshot_with_archived_rows(
        active_session_id: SessionId,
        archived_session_id: SessionId,
    ) -> WorkspaceSnapshot {
        WorkspaceSnapshot {
            task_projects: vec![TaskProjectSummary::new(
                "active-project",
                "Active Project",
                r"D:\active-project",
                false,
                0,
                None,
                None,
                vec![active_session_id],
            )],
            archived_task_projects: vec![TaskProjectSummary::new(
                "archived-project",
                "Archived Project",
                r"D:\archived-project",
                false,
                1,
                None,
                None,
                vec![archived_session_id],
            )],
            sessions: vec![SessionSummary::new(
                active_session_id,
                Some(String::from("active-project")),
                "Active Session",
                "",
                SessionState::Running,
                0,
                0,
                "now",
            )],
            archived_sessions: vec![SessionSummary::new(
                archived_session_id,
                Some(String::from("archived-project")),
                "Archived Session",
                "",
                SessionState::Completed,
                0,
                0,
                "earlier",
            )],
            session_messages: BTreeMap::new(),
            archived_session_messages: BTreeMap::new(),
            session_compactions: BTreeMap::new(),
            archived_session_compactions: BTreeMap::new(),
            session_runtime_state: BTreeMap::new(),
            archived_session_runtime_state: BTreeMap::new(),
            session_run_phases: BTreeMap::new(),
            expanded_task_projects: BTreeSet::new(),
            selected_session: active_session_id,
            hydrated_session_ids: BTreeSet::new(),
        }
    }

    /// Verifies session lookup reports active and archived rows through one helper.
    /// 验证会话查找通过同一个 helper 区分活动与归档行。
    #[test]
    fn workspace_session_with_archive_status_reports_active_and_archived_rows() {
        let active_session_id = SessionId::new_v4();
        let archived_session_id = SessionId::new_v4();
        let missing_session_id = SessionId::new_v4();
        let workspace_snapshot =
            test_workspace_snapshot_with_archived_rows(active_session_id, archived_session_id);

        let (active_session, active_archived) =
            workspace_session_with_archive_status(&workspace_snapshot, active_session_id)
                .expect("active session should be found");
        let (archived_session, archived_archived) =
            workspace_session_with_archive_status(&workspace_snapshot, archived_session_id)
                .expect("archived session should be found");

        assert_eq!(active_session.id, active_session_id);
        assert!(!active_archived);
        assert_eq!(archived_session.id, archived_session_id);
        assert!(archived_archived);
        assert!(workspace_snapshot_contains_regular_session(
            &workspace_snapshot,
            active_session_id
        ));
        assert!(workspace_snapshot_contains_regular_session(
            &workspace_snapshot,
            archived_session_id
        ));
        assert!(!workspace_snapshot_contains_regular_session(
            &workspace_snapshot,
            missing_session_id
        ));
    }

    /// Verifies persistence project collection keeps active projects before archived projects.
    /// 验证持久化项目收集会保持活动项目先于归档项目。
    #[test]
    fn workspace_task_projects_for_persistence_keeps_active_then_archived_projects() {
        let workspace_snapshot =
            test_workspace_snapshot_with_archived_rows(SessionId::new_v4(), SessionId::new_v4());

        let project_ids = workspace_task_projects_for_persistence(&workspace_snapshot)
            .into_iter()
            .map(|project| project.id)
            .collect::<Vec<_>>();

        assert_eq!(
            project_ids,
            vec![
                String::from("active-project"),
                String::from("archived-project")
            ]
        );
    }

    /// Verifies a synchronous terminal writer reloads retained state after waiting for the shared persistence gate.
    /// 验证同步终态写入者在等待共享持久化闸门后会重新读取保留状态。
    #[tokio::test]
    async fn retained_session_persistence_source_is_loaded_after_write_gate() {
        // sessionId identifies the exact regular session whose retained revision changes while persistence waits.
        // sessionId 标识持久化等待期间保留修订发生变化的精确普通会话。
        let session_id = SessionId::new_v4();
        // serviceState starts with the standard retained parent-session shell.
        // serviceState 从标准保留父会话外壳开始。
        let mut service_state =
            crate::tests::test_support::test_service_state_with_parent_session(session_id);
        {
            // workspace owns the initial stale marker captured before the write gate is released.
            // workspace 持有写入闸门释放前捕获的初始旧标记。
            let workspace = service_state
                .workspace_snapshot
                .as_mut()
                .expect("test workspace should exist");
            workspace.session_runtime_state.insert(
                session_id,
                SessionRuntimeStateEntry {
                    updated_at: String::from("stale-before-write-gate"),
                    ..SessionRuntimeStateEntry::default()
                },
            );
        }
        // state is the shared authoritative owner read again by the synchronous persistence path.
        // state 是同步持久化路径会重新读取的共享权威所有者。
        let state = Arc::new(std::sync::Mutex::new(service_state));
        // writeGate blocks the tested persistence call after it invalidates older background versions.
        // writeGate 在被测持久化调用使旧后台版本失效后阻塞该调用。
        let persist_lock = webui_workspace_persist_lock();
        let write_gate = persist_lock.lock().await;
        // persistenceTask enters the exact production source-selection helper and waits behind writeGate.
        // persistenceTask 进入精确的生产源选择辅助函数，并在 writeGate 后等待。
        let persistence_state = state.clone();
        let persistence_task = tokio::spawn(async move {
            lock_latest_retained_workspace_session_persistence_snapshot(
                &persistence_state,
                session_id,
            )
            .await
        });
        // versionObserved proves the spawned call reached its invalidation boundary before retained state changes.
        // versionObserved 证明已启动调用在保留态变化前到达版本失效边界。
        loop {
            let version_observed = state
                .lock()
                .expect("test state should remain lockable")
                .session_persist_versions
                .contains_key(&session_id);
            if version_observed {
                break;
            }
            tokio::task::yield_now().await;
        }
        {
            // runtimeState is the later authoritative mutation that must survive the delayed terminal write.
            // runtimeState 是必须在延迟终态写入后保留的较新权威变更。
            let mut guard = state.lock().expect("test state should remain lockable");
            let runtime_state = guard
                .workspace_snapshot
                .as_mut()
                .and_then(|workspace| workspace.session_runtime_state.get_mut(&session_id))
                .expect("test runtime state should exist");
            runtime_state.updated_at = String::from("latest-after-write-gate");
        }
        drop(write_gate);
        let (_persistence_guard, authoritative_snapshot) = persistence_task
            .await
            .expect("persistence task should join")
            .expect("authoritative source selection should succeed");
        // persistedRuntimeState is the exact source production persistence receives while its guard remains held.
        // persistedRuntimeState 是生产持久化在守卫仍被持有时接收的精确来源。
        let persisted_runtime_state = authoritative_snapshot
            .session_runtime_state
            .get(&session_id)
            .expect("authoritative runtime state should exist");
        assert_eq!(
            persisted_runtime_state.updated_at,
            "latest-after-write-gate"
        );
    }
}
