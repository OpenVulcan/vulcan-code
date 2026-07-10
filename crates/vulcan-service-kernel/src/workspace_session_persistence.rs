//! Workspace session persistence tasks.
//! 工作区会话持久化任务。

use super::*;
use std::future::Future;

/// Persists only the changed session after a WebUI runtime mutation.
/// 在 WebUI 运行时变更后仅持久化被修改的单个会话。
pub(crate) async fn persist_workspace_session_update(
    state: Option<&SharedServiceBridgeState>,
    controller_runtime: ControllerRuntimeHandle,
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
) -> Result<(), String> {
    if let Some(state) = state {
        invalidate_webui_session_persist_version(state, session_id)?;
    }
    let persist_lock = webui_workspace_persist_lock();
    let _persist_guard = persist_lock.lock().await;
    persist_workspace_session_update_unlocked(controller_runtime, workspace_snapshot, session_id)
        .await
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
    // persist_version records the newest whole-session persistence generation before spawning.
    // persist_version 记录启动后台任务前最新的整会话持久化代际。
    let persist_version = match next_webui_session_persist_version(&state, session_id) {
        Ok(version) => version,
        Err(error) => {
            eprintln!("[webui-service] {version_error_label}: {error}");
            return;
        }
    };
    spawn_service_task(async move {
        let persist_lock = webui_workspace_persist_lock();
        let _persist_guard = persist_lock.lock().await;
        if !webui_session_persist_version_is_current(&state, session_id, persist_version) {
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
            let Some(runtime_state) = workspace_snapshot
                .session_runtime_state
                .get(&session_id)
                .cloned()
                .or_else(|| {
                    workspace_snapshot
                        .archived_session_runtime_state
                        .get(&session_id)
                        .cloned()
                })
            else {
                return;
            };
            // persist_result keeps the store call readable inside the version-gated task.
            // persist_result 让版本门闸任务内的存储调用保持易读。
            let persist_result =
                vulcan_workspace_store::workspace_store::persist_single_session_runtime_state_update(
                    controller_runtime,
                    session_id,
                    &runtime_state,
                )
                .await;
            if let Err(error) = persist_result {
                let reason_label = reason.as_deref().unwrap_or("runtime_state");
                eprintln!(
                    "[webui-service] background runtime-state persist failed ({reason_label}): {error}"
                );
            }
        },
    );
}

/// Spawns a background write for the current loaded transcript without touching runtime state.
/// 为当前已载入转录启动后台写入，且不触碰运行态。
///
/// # Parameters
/// # 参数
/// `state` owns the live transcript persistence version table.
/// `state` 持有运行中转录持久化版本表。
/// `controller_runtime` provides workspace persistence.
/// `controller_runtime` 提供工作区持久化能力。
/// `workspace_snapshot` is the current workspace snapshot containing loaded transcript rows.
/// `workspace_snapshot` 是包含已载入转录行的当前工作区快照。
/// `session_id` selects the session transcript to persist.
/// `session_id` 选择要持久化的会话转录。
/// `reason` is an optional diagnostic label for logs.
/// `reason` 是用于日志的可选诊断标签。
pub(crate) fn spawn_workspace_session_transcript_persist_task(
    state: SharedServiceBridgeState,
    controller_runtime: ControllerRuntimeHandle,
    workspace_snapshot: WorkspaceSnapshot,
    session_id: SessionId,
    reason: Option<String>,
) {
    if !workspace_snapshot_contains_regular_session(&workspace_snapshot, session_id) {
        return;
    }
    let loaded_messages =
        loaded_session_messages_from_workspace(&workspace_snapshot, session_id).unwrap_or_default();
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
        let Some((session, archived)) =
            workspace_session_with_archive_status(&workspace_snapshot, session_id)
                .map(|(session, archived)| (session.clone(), archived))
        else {
            return;
        };
        let task_projects = workspace_task_projects_for_persistence(&workspace_snapshot);
        match vulcan_workspace_store::workspace_store::persist_single_session_addon_state_update(
            controller_runtime,
            &task_projects,
            &session,
            archived,
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
}
