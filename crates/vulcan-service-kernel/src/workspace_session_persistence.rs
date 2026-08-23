//! Workspace session persistence tasks.
//! 工作区会话持久化任务。

use super::*;
use std::future::Future;

/// Detached data required to persist exactly one regular workspace session.
/// 持久化一个精确普通工作区会话所需的分离数据。
struct WorkspaceSessionPersistencePayload {
    /// Exact project owner carried by the retained session locator.
    /// 保留会话定位信息携带的精确项目归属。
    project_id: String,
    /// Exact session summary being persisted.
    /// 正在持久化的精确会话摘要。
    session: SessionSummary,
    /// Whether the exact session currently belongs to archived storage.
    /// 精确会话当前是否属于归档存储。
    archived: bool,
    /// Stored display transcript supplied to the workspace-store canonical merge.
    /// 提供给 workspace-store 规范合并的已存展示转录。
    messages: Vec<SessionMessageEntry>,
    /// Optional canonical runtime state for the exact session.
    /// 精确会话的可选规范运行态。
    runtime_state: Option<SessionRuntimeStateEntry>,
}

/// Detached history-free payload for one authoritative retained-session write.
/// 单个权威保留会话写入使用的分离无历史载荷。
struct RetainedWorkspaceSessionPersistencePayload {
    /// Exact deterministic directory resolved before releasing retained state.
    /// 释放保留状态前解析出的精确确定性目录。
    session_directory: PathBuf,
    /// Exact project owner carried by the retained session locator.
    /// 保留会话定位信息携带的精确项目归属。
    project_id: String,
    /// Exact session summary being persisted.
    /// 正在持久化的精确会话摘要。
    session: SessionSummary,
    /// Whether the exact session currently belongs to archived storage.
    /// 精确会话当前是否属于归档存储。
    archived: bool,
    /// Exact display transcript for this session only.
    /// 仅属于该会话的精确展示转录。
    messages: Vec<SessionMessageEntry>,
    /// Optional history-free `state.json` projection produced while borrowing runtime state.
    /// 借用运行态期间生成的可选无历史 `state.json` 投影。
    runtime_projection:
        Option<vulcan_workspace_store::workspace_store::SessionRuntimeStatePersistenceProjection>,
}

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
/// Success after the newest authoritative session revision has crossed its session-keyed write channel.
/// 最新权威会话修订通过其会话键写入通道后返回成功。
pub(crate) async fn persist_workspace_session_update(
    state: Option<&SharedServiceBridgeState>,
    controller_runtime: ControllerRuntimeHandle,
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
) -> Result<(), String> {
    if let Some(state) = state {
        return persist_latest_retained_workspace_session_update(
            state,
            controller_runtime,
            session_id,
        )
        .await;
    }
    let persist_lock = webui_session_persist_lock(session_id);
    let _persist_guard = persist_lock.lock().await;
    persist_workspace_session_update_unlocked(controller_runtime, workspace_snapshot, session_id)
        .await
}

/// Persists the newest authoritative revision of one retained session without cloning its workspace.
/// 在不克隆工作区的情况下持久化一个保留会话的最新权威修订。
///
/// # Parameters
/// 参数
/// `state` owns the authoritative retained workspace and invalidates older background writers.
/// `state` 持有权威保留工作区，并使较早的后台写入者失效。
/// `controller_runtime` owns the exact workspace-store persistence route.
/// `controller_runtime` 持有精确的工作区存储持久化路由。
/// `session_id` selects the only retained session captured after write admission.
/// `session_id` 选择取得写入许可后唯一捕获的保留会话。
///
/// # Returns
/// 返回值
/// Success after the newest exact session payload crosses its session-keyed write channel.
/// 最新精确会话载荷通过其会话键写入通道后返回成功。
pub(crate) async fn persist_latest_retained_workspace_session_update(
    state: &SharedServiceBridgeState,
    controller_runtime: ControllerRuntimeHandle,
    session_id: SessionId,
) -> Result<(), String> {
    invalidate_webui_session_persist_version(state, session_id)?;
    let _persist_guard = webui_session_persist_lock(session_id).lock_owned().await;
    persist_latest_retained_workspace_session_update_unlocked(state, controller_runtime, session_id)
        .await
}

/// Persists one retained session after the caller has acquired its session-keyed write channel.
/// 在调用方已经取得会话键写入通道后持久化一个保留会话。
///
/// # Parameters
/// 参数
/// `state` owns the authoritative retained session borrowed for bounded runtime synchronization.
/// `state` 持有为有界运行态同步而借用的权威保留会话。
/// `controller_runtime` owns the exact data root and locator index.
/// `controller_runtime` 持有精确数据根与定位索引。
/// `session_id` selects the only retained session written by this transaction.
/// `session_id` 选择该事务唯一写入的保留会话。
///
/// # Returns
/// 返回值
/// Success after transcript, runtime metadata, and locator persistence complete.
/// 转录、运行态元数据与定位信息持久化完成后返回成功。
async fn persist_latest_retained_workspace_session_update_unlocked(
    state: &SharedServiceBridgeState,
    controller_runtime: ControllerRuntimeHandle,
    session_id: SessionId,
) -> Result<(), String> {
    // payload is captured after write admission and never clones canonical runtime history.
    // payload 在取得写入许可后捕获，且绝不克隆规范运行态历史。
    let payload = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        retained_workspace_session_persistence_payload(&guard, &controller_runtime, session_id)?
    };
    persist_retained_workspace_session_payload(controller_runtime, payload).await
}

/// Captures one retained session after synchronizing its borrowed bounded runtime tail.
/// 在同步借用的有界运行态尾部后捕获一个保留会话。
///
/// # Parameters
/// 参数
/// `guard` owns authoritative retained state, `controller_runtime` owns exact storage,
/// and `session_id` selects one regular session.
/// `guard` 持有权威保留状态，`controller_runtime` 持有精确存储，`session_id`
/// 选择一个普通会话。
///
/// # Returns
/// 返回值
/// A detached payload without canonical conversation history, or an exact locator or persistence error.
/// 返回不含规范会话历史的分离载荷，或精确定位与持久化错误。
fn retained_workspace_session_persistence_payload(
    guard: &ServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    session_id: SessionId,
) -> Result<RetainedWorkspaceSessionPersistencePayload, String> {
    let session_directory = require_segmented_session_directory_for_retained_session(
        guard,
        &controller_runtime.data_root,
        session_id,
    )?;
    let workspace_snapshot = guard.workspace_snapshot.as_ref().ok_or_else(|| {
        String::from(
            "cannot persist a retained session without an authoritative workspace snapshot",
        )
    })?;
    let (session, archived) = workspace_session_with_archive_status(workspace_snapshot, session_id)
        .ok_or_else(|| format!("单会话保存失败：未找到会话：{session_id}"))?;
    let project_id = session_summary_project_id(session)
        .map(str::to_string)
        .ok_or_else(|| format!("单会话保存失败：会话未携带所属项目：{session_id}"))?;
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
    let runtime_projection = match session_runtime_state.get(&session_id) {
        Some(runtime_state) => {
            // The append synchronizer reads constant metadata plus only the open active tail.
            // 追加同步器仅读取常量元数据以及尚未闭合的活动尾部。
            vulcan_workspace_store::workspace_store::sync_single_session_runtime_log_update(
                &session_directory,
                session_id,
                runtime_state,
            )?;
            Some(
                vulcan_workspace_store::workspace_store::
                    project_single_session_runtime_state_update(session_id, runtime_state)?,
            )
        }
        None => None,
    };
    Ok(RetainedWorkspaceSessionPersistencePayload {
        session_directory,
        project_id,
        session: session.clone(),
        archived,
        messages: session_messages
            .get(&session_id)
            .cloned()
            .unwrap_or_default(),
        runtime_projection,
    })
}

/// Persists one detached retained-session payload without revisiting retained history.
/// 在不重新访问保留历史的情况下持久化一个分离的保留会话载荷。
///
/// # Parameters
/// 参数
/// `controller_runtime` owns the locator database and `payload` owns one exact history-free revision.
/// `controller_runtime` 持有定位数据库，`payload` 持有一个精确无历史修订。
///
/// # Returns
/// 返回值
/// Success after transcript, locator, and optional runtime metadata writes complete.
/// 转录、定位信息与可选运行态元数据写入完成后返回成功。
async fn persist_retained_workspace_session_payload(
    controller_runtime: ControllerRuntimeHandle,
    payload: RetainedWorkspaceSessionPersistencePayload,
) -> Result<(), String> {
    vulcan_workspace_store::workspace_store::persist_single_session_update(
        controller_runtime,
        &payload.project_id,
        &payload.session,
        payload.archived,
        &payload.messages,
        None,
    )
    .await?;
    if let Some(runtime_projection) = payload.runtime_projection.as_ref() {
        vulcan_workspace_store::workspace_store::
            persist_projected_single_session_runtime_state_update(
                payload.session_directory,
                payload.session.id,
                runtime_projection,
            )
            .await?;
    }
    Ok(())
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
    let payload = workspace_session_persistence_payload(workspace_snapshot, session_id)?;
    persist_workspace_session_payload(controller_runtime, payload).await
}

/// Captures only the exact session fields required by the workspace-store persistence API.
/// 仅捕获 workspace-store 持久化 API 所需的精确会话字段。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` is the authoritative retained or transaction-local workspace revision.
/// `workspace_snapshot` 是权威保留或事务本地工作区修订。
/// `session_id` selects the exact regular session to detach.
/// `session_id` 选择需要分离的精确普通会话。
///
/// # Returns
/// 返回值
/// A single-session payload without unrelated session maps, or an explicit missing-session error.
/// 返回不包含无关会话映射的单会话载荷，或明确的会话缺失错误。
fn workspace_session_persistence_payload(
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
) -> Result<WorkspaceSessionPersistencePayload, String> {
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
    let runtime_state = session_runtime_state.get(&session_id).cloned();
    // messages remains the stored display slice because workspace-store owns the single canonical merge.
    // messages 保持为已存展示切片，因为 workspace-store 唯一负责规范合并。
    let messages = session_messages
        .get(&session_id)
        .cloned()
        .unwrap_or_default();
    let project_id = session_summary_project_id(session)
        .map(str::to_string)
        .ok_or_else(|| format!("单会话保存失败：会话未携带所属项目：{session_id}"))?;
    Ok(WorkspaceSessionPersistencePayload {
        project_id,
        session: session.clone(),
        archived,
        messages,
        runtime_state,
    })
}

/// Persists one already-scoped session payload after the caller has acquired its session write channel.
/// 在调用方取得会话写入通道后持久化一个已限定范围的会话载荷。
///
/// # Parameters
/// 参数
/// `controller_runtime` owns the workspace-store route and `payload` owns one exact session revision.
/// `controller_runtime` 持有 workspace-store 路由，`payload` 持有一个精确会话修订。
///
/// # Returns
/// 返回值
/// Success after the single-session segmented persistence transaction completes.
/// 单会话分段持久化事务完成后返回成功。
async fn persist_workspace_session_payload(
    controller_runtime: ControllerRuntimeHandle,
    payload: WorkspaceSessionPersistencePayload,
) -> Result<(), String> {
    vulcan_workspace_store::workspace_store::persist_single_session_update(
        controller_runtime,
        &payload.project_id,
        &payload.session,
        payload.archived,
        &payload.messages,
        payload.runtime_state.as_ref(),
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

/// Spawns one session-versioned persistence task behind its deterministic session write channel.
/// 在确定性会话写入通道后启动一个按会话版本保护的持久化任务。
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
        if !webui_session_runtime_persist_version_is_current(
            &state,
            session_id,
            session_persist_version,
        ) {
            return;
        }
        let persist_lock = webui_session_persist_lock(session_id);
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
/// `session_id` selects the session that changed.
/// `session_id` 选择发生变更的会话。
/// `reason` labels the write site for diagnostics.
/// `reason` 标记写入来源，供诊断使用。
pub(crate) fn spawn_workspace_session_persist_task(
    state: SharedServiceBridgeState,
    controller_runtime: ControllerRuntimeHandle,
    session_id: SessionId,
    reason: Option<String>,
) {
    // persistence_state is read only after version admission so the caller never clones a workspace.
    // persistence_state 仅在版本准入后读取，因此调用方绝不克隆工作区。
    let persistence_state = state.clone();
    spawn_session_versioned_workspace_persist_task(
        state,
        session_id,
        "background persist version failed",
        move || async move {
            if let Err(error) = persist_latest_retained_workspace_session_update_unlocked(
                &persistence_state,
                controller_runtime,
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

/// Persists one canonical conversation checkpoint without cloning runtime or workspace history.
/// 在不克隆运行时历史或工作区历史的情况下持久化一个规范会话检查点。
///
/// # Parameters
/// 参数
/// `state` owns the authoritative retained checkpoint, `controller_runtime` owns exact storage,
/// `session_id` selects the main session, and `reason` labels diagnostic failures.
/// `state` 持有权威保留检查点，`controller_runtime` 持有精确存储，
/// `session_id` 选择主会话，`reason` 标记诊断失败来源。
pub(crate) fn spawn_latest_conversation_checkpoint_persist_task(
    state: SharedServiceBridgeState,
    controller_runtime: ControllerRuntimeHandle,
    session_id: SessionId,
    reason: Option<String>,
) {
    let persistence_state = state.clone();
    spawn_session_versioned_workspace_persist_task(
        state,
        session_id,
        "conversation checkpoint persist version failed",
        move || async move {
            // persistence_snapshot contains only history-free state.json fields after the bounded log sync.
            // persistence_snapshot 在有界日志同步后仅包含无历史的 state.json 字段。
            let persistence_snapshot = match persistence_state.lock() {
                Ok(guard) => {
                    let session_directory =
                        match require_segmented_session_directory_for_retained_session(
                            &guard,
                            &controller_runtime.data_root,
                            session_id,
                        ) {
                            Ok(session_directory) => session_directory,
                            Err(error) => {
                                eprintln!(
                                    "[webui-service] checkpoint exact locator failed: session={session_id} error={error}"
                                );
                                return;
                            }
                        };
                    let Some(workspace) = guard.workspace_snapshot.as_ref() else {
                        return;
                    };
                    let Some(runtime_state) = workspace.session_runtime_state.get(&session_id)
                    else {
                        return;
                    };
                    let projection = match vulcan_workspace_store::workspace_store::
                        project_single_session_runtime_state_update(session_id, runtime_state)
                    {
                        Ok(projection) => projection,
                        Err(error) => {
                            eprintln!(
                                "[webui-service] checkpoint state projection failed: session={session_id} error={error}"
                            );
                            return;
                        }
                    };
                    // Runtime synchronization borrows the authoritative state and projects only its bounded active tail.
                    // 运行时同步借用权威状态，并且只投影其有界活动尾部。
                    let runtime_log_result = vulcan_workspace_store::workspace_store::
                        sync_single_session_runtime_log_update(
                            &session_directory,
                            session_id,
                            runtime_state,
                        );
                    Some((session_directory, projection, runtime_log_result))
                }
                Err(error) => {
                    eprintln!(
                        "[webui-service] checkpoint retained-state lock failed: session={session_id} error={error}"
                    );
                    return;
                }
            };
            let Some((session_directory, projection, runtime_log_result)) = persistence_snapshot
            else {
                return;
            };
            let reason_label = reason.as_deref().unwrap_or("conversation_checkpoint");
            if let Err(error) = runtime_log_result {
                eprintln!(
                    "[webui-service] checkpoint bounded runtime-log persist failed ({reason_label}): {error}"
                );
            }
            if let Err(error) = vulcan_workspace_store::workspace_store::
                persist_projected_single_session_runtime_state_update(
                    session_directory,
                    session_id,
                    &projection,
                )
                .await
            {
                eprintln!(
                    "[webui-service] checkpoint runtime-state persist failed ({reason_label}): {error}"
                );
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
/// `session_id` selects the session runtime-state document to persist.
/// `session_id` 选择需要持久化的会话运行态文档。
/// `reason` labels the write site for diagnostics.
/// `reason` 标记写入来源，供诊断使用。
pub(crate) fn spawn_workspace_session_runtime_state_persist_task(
    state: SharedServiceBridgeState,
    controller_runtime: ControllerRuntimeHandle,
    session_id: SessionId,
    reason: Option<String>,
) {
    spawn_latest_session_runtime_state_persist_task(state, controller_runtime, session_id, reason);
}

/// Persists the newest history-free runtime metadata for one retained session synchronously.
/// 同步持久化一个保留会话的最新无历史运行态元数据。
///
/// # Parameters
/// 参数
/// `state` owns the authoritative runtime entry and invalidates older runtime-only writers.
/// `state` 持有权威运行态条目，并使较早的仅运行态写入者失效。
/// `controller_runtime` owns exact session storage.
/// `controller_runtime` 持有精确会话存储。
/// `session_id` selects the only runtime metadata document to replace.
/// `session_id` 选择唯一需要替换的运行态元数据文档。
///
/// # Returns
/// 返回值
/// Success after a projection produced under the retained-state lock is durably written.
/// 在保留状态锁下生成的投影可靠写入后返回成功。
pub(crate) async fn persist_latest_retained_session_runtime_state_update(
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    session_id: SessionId,
) -> Result<(), String> {
    invalidate_webui_session_runtime_only_persist_version(state, session_id)?;
    let persist_lock = webui_session_persist_lock(session_id);
    let _persist_guard = persist_lock.lock().await;
    let (session_directory, projection) =
        {
            let guard = state
                .lock()
                .map_err(|error| format!("failed to lock service state: {error}"))?;
            let session_directory = require_segmented_session_directory_for_retained_session(
                &guard,
                &controller_runtime.data_root,
                session_id,
            )?;
            let workspace = guard.workspace_snapshot.as_ref().ok_or_else(|| {
                String::from("cannot persist runtime state before workspace hydration completes")
            })?;
            let runtime_state = workspace
                .session_runtime_state
                .get(&session_id)
                .or_else(|| workspace.archived_session_runtime_state.get(&session_id))
                .ok_or_else(|| format!("Session `{session_id}` has no retained runtime state."))?;
            let projection = vulcan_workspace_store::workspace_store::
            project_single_session_runtime_state_update(session_id, runtime_state)?;
            (session_directory, projection)
        };
    vulcan_workspace_store::workspace_store::persist_projected_single_session_runtime_state_update(
        session_directory,
        session_id,
        &projection,
    )
    .await
}

/// Persists the latest retained runtime state without cloning a complete workspace snapshot.
/// 在不克隆完整工作区快照的情况下持久化最新保留运行态。
///
/// # Parameters
/// 参数
/// `state` owns the authoritative retained runtime entry, `controller_runtime` owns storage,
/// `session_id` selects the exact owner, and `reason` labels diagnostic failures.
/// `state` 持有权威保留运行态，`controller_runtime` 持有存储，`session_id` 选择精确归属，
/// `reason` 标记诊断失败来源。
pub(crate) fn spawn_latest_session_runtime_state_persist_task(
    state: SharedServiceBridgeState,
    controller_runtime: ControllerRuntimeHandle,
    session_id: SessionId,
    reason: Option<String>,
) {
    if let Err(error) = invalidate_webui_session_runtime_only_persist_version(&state, session_id) {
        eprintln!(
            "[webui-service] latest runtime-state persist fence failed: session={session_id} error={error}"
        );
        return;
    }
    spawn_service_task(async move {
        let persist_lock = webui_session_persist_lock(session_id);
        let _persist_guard = persist_lock.lock().await;
        let persistence_snapshot = match state.lock() {
            Ok(guard) => {
                // session_directory is captured from the same retained snapshot as runtime_state.
                // session_directory 与 runtime_state 从同一份保留快照中捕获。
                let session_directory =
                    match require_segmented_session_directory_for_retained_session(
                        &guard,
                        &controller_runtime.data_root,
                        session_id,
                    ) {
                        Ok(session_directory) => session_directory,
                        Err(error) => {
                            eprintln!(
                                "[webui-service] latest runtime-state exact locator failed: session={session_id} error={error}"
                            );
                            return;
                        }
                    };
                let Some(workspace) = guard.workspace_snapshot.as_ref() else {
                    return;
                };
                let Some(runtime_state) = workspace
                    .session_runtime_state
                    .get(&session_id)
                    .or_else(|| workspace.archived_session_runtime_state.get(&session_id))
                else {
                    return;
                };
                // projection serializes only state.json fields while the authoritative entry is borrowed.
                // projection 在借用权威条目期间只序列化 state.json 字段。
                match vulcan_workspace_store::workspace_store::
                    project_single_session_runtime_state_update(session_id, runtime_state)
                {
                    Ok(projection) => Some((session_directory, projection)),
                    Err(error) => {
                        eprintln!(
                            "[webui-service] latest runtime-state projection failed: session={session_id} error={error}"
                        );
                        return;
                    }
                }
            }
            Err(error) => {
                eprintln!(
                    "[webui-service] latest runtime-state snapshot lock failed: session={session_id} error={error}"
                );
                return;
            }
        };
        let Some((session_directory, projection)) = persistence_snapshot else {
            return;
        };
        if let Err(error) = vulcan_workspace_store::workspace_store::
            persist_projected_single_session_runtime_state_update(
                session_directory,
                session_id,
                &projection,
            )
            .await
        {
            let reason_label = reason.as_deref().unwrap_or("latest_runtime_state");
            eprintln!(
                "[webui-service] background latest runtime-state persist failed ({reason_label}): {error}"
            );
        }
    });
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
/// `reason` is an optional diagnostic label for logs.
/// `reason` 是用于日志的可选诊断标签。
pub(crate) fn spawn_session_transcript_messages_persist_task(
    state: SharedServiceBridgeState,
    controller_runtime: ControllerRuntimeHandle,
    session_id: SessionId,
    reason: Option<String>,
) {
    let persist_version = match next_webui_session_transcript_persist_version(&state, session_id) {
        Ok(version) => version,
        Err(error) => {
            eprintln!("[webui-service] live transcript persist version failed: {error}");
            return;
        }
    };
    spawn_service_task(async move {
        if !webui_session_transcript_persist_version_is_current(&state, session_id, persist_version)
        {
            return;
        }
        let persist_lock = webui_session_persist_lock(session_id);
        let _persist_guard = persist_lock.lock().await;
        if !webui_session_transcript_persist_version_is_current(&state, session_id, persist_version)
        {
            return;
        }
        // sessionDirectory comes only from the retained exact main-session or TaskSession locator.
        // sessionDirectory 仅来自保留态中的精确主会话或 TaskSession 定位信息。
        let session_directory = match state.lock() {
            Ok(guard) => match segmented_session_directory_for_retained_session(
                &guard,
                &controller_runtime.data_root,
                session_id,
            ) {
                Ok(Some(session_directory)) => session_directory,
                Ok(None) => {
                    eprintln!(
                        "[webui-service] live transcript exact session directory is unavailable: session={session_id}"
                    );
                    return;
                }
                Err(error) => {
                    eprintln!(
                        "[webui-service] live transcript exact session directory resolution failed: session={session_id} error={error}"
                    );
                    return;
                }
            },
            Err(error) => {
                eprintln!(
                    "[webui-service] live transcript locator lock failed: session={session_id} error={error}"
                );
                return;
            }
        };
        // persistedMessageCount is read from metadata only; historical JSONL segments are never opened here.
        // persistedMessageCount 仅从元数据读取；此处绝不打开历史 JSONL 分段。
        let persisted_message_count = match vulcan_workspace_store::workspace_store::
            load_single_session_transcript_message_count_in_directory(
                &session_directory,
                session_id,
            ) {
            Ok(message_count) => message_count,
            Err(error) => {
                eprintln!(
                    "[webui-service] live transcript checkpoint read failed: session={session_id} error={error}"
                );
                return;
            }
        };
        // Only one boundary row plus the new immutable suffix is cloned after writer coalescing.
        // 写入任务合并后仅克隆一个边界行及新的不可变后缀。
        let (previous_message, tail_messages) = match state.lock() {
            Ok(guard) => {
                let Some(workspace) = guard.workspace_snapshot.as_ref() else {
                    return;
                };
                let Some(messages) = workspace.session_messages.get(&session_id) else {
                    return;
                };
                if persisted_message_count > messages.len() {
                    eprintln!(
                        "[webui-service] live transcript checkpoint exceeds retained messages: session={session_id} persisted={persisted_message_count} retained={} ",
                        messages.len()
                    );
                    return;
                }
                if persisted_message_count == messages.len() {
                    return;
                }
                (
                    persisted_message_count
                        .checked_sub(1)
                        .and_then(|index| messages.get(index))
                        .cloned(),
                    messages[persisted_message_count..].to_vec(),
                )
            }
            Err(error) => {
                eprintln!(
                    "[webui-service] live transcript suffix lock failed: session={session_id} error={error}"
                );
                return;
            }
        };
        if !webui_session_transcript_persist_version_is_current(&state, session_id, persist_version)
        {
            return;
        }
        if let Err(error) = vulcan_workspace_store::workspace_store::
            persist_single_session_transcript_tail_in_directory(
                &session_directory,
                session_id,
                persisted_message_count,
                previous_message.as_ref(),
                &tail_messages,
            )
        {
            let reason_label = reason.as_deref().unwrap_or("live_transcript");
            eprintln!(
                "[webui-service] background live transcript persist failed ({reason_label}): {error}"
            );
        }
    });
}

/// Spawns a background write for one exact session addon-state document.
/// 为单个精确会话附属状态文档启动后台写入任务。
///
/// # Parameters
/// 参数
/// `app` is the event emitter used after the file write succeeds.
/// `app` 是文件写入成功后使用的事件发送器。
/// `state` owns the addon persistence version table used to reject stale writes.
/// `state` 持有用于拒绝过期写入的附属状态持久化版本表。
/// `session_directory` is the deterministic target resolved before this task starts.
/// `session_directory` 是任务启动前已经解析完成的确定性目标。
/// `session_id` selects the addon document to persist.
/// `session_id` 选择要持久化的附属状态文档。
/// `addon_state` is the already-derived addon document to persist.
/// `addon_state` 是已经派生完成且需要持久化的附属状态文档。
/// `changed_namespaces` are the namespace keys represented by this write.
/// `changed_namespaces` 是本次写入代表的命名空间键。
/// `reason` labels the write site for diagnostics.
/// `reason` 标记写入来源，供诊断使用。
pub(crate) fn spawn_exact_session_addon_state_persist_task(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    session_directory: PathBuf,
    session_id: SessionId,
    addon_state: SessionAddonState,
    changed_namespaces: Vec<SessionAddonNamespace>,
    reason: Option<String>,
) {
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
        let persist_lock = webui_session_persist_lock(session_id);
        let _persist_guard = persist_lock.lock().await;
        if !webui_session_addon_persist_versions_are_current(&state, session_id, &persist_versions)
        {
            return;
        }
        match vulcan_workspace_store::workspace_store::persist_single_session_addon_state_update_in_directory(
            session_directory,
            session_id,
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
}
