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
    /// Explicit transcript authority available to this exact retained write.
    /// 本次精确保留写入可使用的显式转录权威范围。
    transcript: RetainedTranscriptPersistence,
    /// Optional history-free `state.json` projection produced while borrowing runtime state.
    /// 借用运行态期间生成的可选无历史 `state.json` 投影。
    runtime_projection:
        Option<vulcan_workspace_store::workspace_store::SessionRuntimeStatePersistenceProjection>,
}

/// Transcript authority available to one retained-session persistence operation.
/// 单次保留会话持久化操作可用的转录权威范围。
enum RetainedTranscriptPersistence {
    /// The retained window starts at absolute index zero and contains the complete transcript.
    /// 保留窗口从绝对下标零开始并包含完整转录。
    Complete(Vec<SessionMessageEntry>),
    /// The retained partial window can be reconciled after the global state lock is released.
    /// 保留的局部窗口可在释放全局状态锁后进行协调。
    PartialWindow {
        /// Absolute persisted index represented by the first retained message.
        /// 第一条保留消息所表示的绝对持久化下标。
        message_start_index: usize,
        /// Bounded retained rows used to select a checkpointed immutable suffix later.
        /// 稍后用于选择带检查点不可变后缀的有界保留行。
        messages: Vec<SessionMessageEntry>,
    },
    /// Only the existing index row and runtime projection may be updated.
    /// 仅允许更新既有索引行与运行时投影。
    ExistingIndexOnly,
}

/// Classifies retained transcript authority without inferring completeness from vector length.
/// 在不根据向量长度推断完整性的前提下分类保留转录权威。
///
/// # Parameters
/// 参数
/// `messages` is the retained display cache for the exact session.
/// `messages` 是精确会话的保留展示缓存。
/// `window` is the explicit hydration boundary retained for that cache.
/// `window` 是为该缓存保留的显式水合边界。
/// # Returns
/// 返回值
/// Complete authority for an index-zero window, deferred partial authority for a bounded window,
/// or index-only authority when no exact transcript coordinate exists.
/// 对零起点窗口返回完整权威，对有界窗口返回延迟局部权威；没有精确转录坐标时仅返回索引权威。
fn classify_retained_transcript_persistence(
    messages: &[SessionMessageEntry],
    window: Option<&HydratedConversationWindowState>,
) -> RetainedTranscriptPersistence {
    match window {
        Some(window) if window.message_start_index == Some(0) && !window.has_more_history => {
            RetainedTranscriptPersistence::Complete(messages.to_vec())
        }
        Some(window) => match window.message_start_index {
            Some(message_start_index) => RetainedTranscriptPersistence::PartialWindow {
                message_start_index,
                messages: messages.to_vec(),
            },
            None => RetainedTranscriptPersistence::ExistingIndexOnly,
        },
        None => RetainedTranscriptPersistence::ExistingIndexOnly,
    }
}

/// Selects one immutable transcript suffix from absolute persisted and retained-window coordinates.
/// 根据绝对持久化坐标与保留窗口坐标选择一段不可变转录后缀。
///
/// # Parameters
/// 参数
/// `messages` is the retained window in ascending display order.
/// `messages` 是按展示正序排列的保留窗口。
/// `message_start_index` is the absolute index represented by `messages[0]`.
/// `message_start_index` 是 `messages[0]` 所表示的绝对下标。
/// `persisted_message_count` is the exact durable prefix length read from metadata.
/// `persisted_message_count` 是从元数据读取的精确可靠前缀长度。
///
/// # Returns
/// 返回值
/// No work when the window end is already durable, otherwise the exact immutable suffix.
/// 当窗口末端已可靠持久化时无工作，否则返回精确的不可变后缀。
fn retained_transcript_tail_for_persisted_boundary(
    messages: &[SessionMessageEntry],
    message_start_index: usize,
    persisted_message_count: usize,
) -> Result<Option<Vec<SessionMessageEntry>>, String> {
    let retained_message_end = message_start_index
        .checked_add(messages.len())
        .ok_or_else(|| {
            format!(
                "Retained transcript window boundary overflowed: start={message_start_index} retained={}",
                messages.len()
            )
        })?;
    if persisted_message_count < message_start_index
        || persisted_message_count > retained_message_end
    {
        return Err(format!(
            "Persisted transcript checkpoint {persisted_message_count} is outside retained absolute window {message_start_index}..={retained_message_end}."
        ));
    }
    if persisted_message_count == retained_message_end {
        return Ok(None);
    }
    let local_boundary = persisted_message_count - message_start_index;
    Ok(Some(messages[local_boundary..].to_vec()))
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
    let retained_messages = session_messages
        .get(&session_id)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let retained_window = guard.hydrated_conversation_windows.get(&session_id);
    let transcript = classify_retained_transcript_persistence(retained_messages, retained_window);
    Ok(RetainedWorkspaceSessionPersistencePayload {
        session_directory,
        project_id,
        session: session.clone(),
        archived,
        transcript,
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
    let RetainedWorkspaceSessionPersistencePayload {
        session_directory,
        project_id,
        session,
        archived,
        transcript,
        runtime_projection,
    } = payload;
    match transcript {
        RetainedTranscriptPersistence::Complete(messages) => {
            vulcan_workspace_store::workspace_store::persist_single_session_update(
                controller_runtime,
                &project_id,
                &session,
                archived,
                &messages,
                None,
            )
            .await?;
        }
        RetainedTranscriptPersistence::PartialWindow {
            message_start_index,
            messages,
        } => {
            persist_partial_retained_transcript_and_index(
                controller_runtime,
                &session_directory,
                &project_id,
                &session,
                archived,
                message_start_index,
                &messages,
            )
            .await?;
        }
        RetainedTranscriptPersistence::ExistingIndexOnly => {
            vulcan_workspace_store::workspace_store::persist_existing_session_index_update(
                controller_runtime,
                &project_id,
                &session,
                archived,
            )
            .await?;
        }
    }
    if let Some(runtime_projection) = runtime_projection.as_ref() {
        vulcan_workspace_store::workspace_store::
            persist_projected_single_session_runtime_state_update(
                session_directory,
                session.id,
                runtime_projection,
            )
            .await?;
    }
    Ok(())
}

/// Persists one partial retained transcript window after leaving the global service-state lock.
/// 在离开全局服务状态锁后持久化一个局部保留转录窗口。
///
/// # Parameters
/// 参数
/// `controller_runtime` owns the exact locator index update route.
/// `controller_runtime` 持有精确定位索引更新路由。
/// `session_directory` is the deterministic storage target for the exact session.
/// `session_directory` 是精确会话的确定性存储目标。
/// `project_id` identifies the existing locator row owner.
/// `project_id` 标识既有定位行的所属项目。
/// `session` is the exact retained session summary being persisted.
/// `session` 是正在持久化的精确保留会话摘要。
/// `archived` records which deterministic storage branch owns the session.
/// `archived` 记录该会话属于哪个确定性存储分支。
/// `message_start_index` is the absolute transcript coordinate represented by `messages[0]`.
/// `message_start_index` 是 `messages[0]` 所表示的绝对转录坐标。
/// `messages` is the bounded retained display window in ascending order.
/// `messages` 是按正序排列的有界保留展示窗口。
///
/// # Returns
/// 返回值
/// Success after any exact immutable suffix and the existing locator row are durable.
/// 精确不可变后缀（若存在）与既有定位行可靠落盘后返回成功。
async fn persist_partial_retained_transcript_and_index(
    controller_runtime: ControllerRuntimeHandle,
    session_directory: &Path,
    project_id: &str,
    session: &SessionSummary,
    archived: bool,
    message_start_index: usize,
    messages: &[SessionMessageEntry],
) -> Result<(), String> {
    // Metadata is read only after the global service-state lock has been released.
    // 元数据仅在全局服务状态锁释放后读取。
    let checkpoint = vulcan_workspace_store::workspace_store::
        load_single_session_transcript_checkpoint_in_directory(
            session_directory,
            session.id,
        )?;
    if let Some(tail) = retained_transcript_tail_for_persisted_boundary(
        messages,
        message_start_index,
        checkpoint.message_count,
    )? {
        vulcan_workspace_store::workspace_store::
            persist_single_session_transcript_tail_from_checkpoint_in_directory(
                session_directory,
                session.id,
                &checkpoint,
                &tail,
            )?;
    }
    vulcan_workspace_store::workspace_store::persist_existing_session_index_update(
        controller_runtime,
        project_id,
        session,
        archived,
    )
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
        // persistedCheckpoint is read from metadata only; historical JSONL segments are never opened here.
        // persistedCheckpoint 仅从元数据读取；此处绝不打开历史 JSONL 分段。
        let persisted_checkpoint = match vulcan_workspace_store::workspace_store::
            load_single_session_transcript_checkpoint_in_directory(
                &session_directory,
                session_id,
            ) {
            Ok(checkpoint) => checkpoint,
            Err(error) => {
                eprintln!(
                    "[webui-service] live transcript checkpoint read failed: session={session_id} error={error}"
                );
                return;
            }
        };
        // Only the new immutable suffix is cloned after writer coalescing.
        // 写入任务合并后仅克隆新的不可变后缀。
        let tail_messages = match state.lock() {
            Ok(guard) => {
                let Some(workspace) = guard.workspace_snapshot.as_ref() else {
                    return;
                };
                let Some(messages) = workspace.session_messages.get(&session_id) else {
                    return;
                };
                let message_start_index = match guard
                    .hydrated_conversation_windows
                    .get(&session_id)
                    .and_then(|window| window.message_start_index)
                {
                    Some(index) => index,
                    None => {
                        eprintln!(
                            "[webui-service] live transcript window lacks an absolute message boundary: session={session_id}"
                        );
                        return;
                    }
                };
                match retained_transcript_tail_for_persisted_boundary(
                    messages,
                    message_start_index,
                    persisted_checkpoint.message_count,
                ) {
                    Ok(Some(tail)) => tail,
                    Ok(None) => return,
                    Err(error) => {
                        eprintln!(
                            "[webui-service] live transcript retained-window boundary rejected: session={session_id} error={error}"
                        );
                        return;
                    }
                }
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
            persist_single_session_transcript_tail_from_checkpoint_in_directory(
                &session_directory,
                session_id,
                &persisted_checkpoint,
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

    /// Verifies a partial or untracked display cache can never authorize a complete transcript rewrite.
    /// 验证局部或未跟踪的展示缓存永远不能授权完整转录重写。
    #[test]
    fn partial_retained_window_never_authorizes_complete_transcript_persistence() {
        let messages = vec![SessionMessageEntry {
            id: String::from("tail-message"),
            kind: vulcan_context::SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::AssistantOutput,
            visible: true,
            title: String::from("output"),
            body: String::from("tail"),
            metadata: None,
            created_at: String::from("1"),
        }];
        let partial_window = HydratedConversationWindowState {
            message_start_index: Some(40),
            earliest_ordinal: Some(100),
            has_more_history: true,
        };

        assert!(matches!(
            classify_retained_transcript_persistence(&messages, Some(&partial_window)),
            RetainedTranscriptPersistence::PartialWindow {
                message_start_index: 40,
                messages: retained_messages,
            } if retained_messages == messages
        ));
        assert!(matches!(
            classify_retained_transcript_persistence(&messages, None),
            RetainedTranscriptPersistence::ExistingIndexOnly
        ));
    }

    /// Verifies only a complete index-zero window retains full transcript persistence authority.
    /// 验证仅从下标零开始的完整窗口保留完整转录持久化权威。
    #[test]
    fn complete_retained_window_authorizes_complete_transcript_persistence() {
        let complete_window = HydratedConversationWindowState {
            message_start_index: Some(0),
            earliest_ordinal: Some(0),
            has_more_history: false,
        };

        assert!(matches!(
            classify_retained_transcript_persistence(&[], Some(&complete_window)),
            RetainedTranscriptPersistence::Complete(messages) if messages.is_empty()
        ));
    }

    /// Verifies a partial retained window defers exact suffix selection until after capture.
    /// 验证局部保留窗口会将精确后缀选择延迟到捕获之后。
    #[test]
    fn partial_retained_window_preserves_absolute_coordinate_for_deferred_append() {
        let messages = (40..43)
            .map(|index| SessionMessageEntry {
                id: format!("message-{index}"),
                kind: vulcan_context::SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::AssistantOutput,
                visible: true,
                title: String::from("output"),
                body: index.to_string(),
                metadata: None,
                created_at: index.to_string(),
            })
            .collect::<Vec<_>>();
        let partial_window = HydratedConversationWindowState {
            message_start_index: Some(40),
            earliest_ordinal: Some(100),
            has_more_history: true,
        };
        let authority = classify_retained_transcript_persistence(&messages, Some(&partial_window));

        match authority {
            RetainedTranscriptPersistence::PartialWindow {
                message_start_index,
                messages: retained_messages,
            } => {
                assert_eq!(message_start_index, 40);
                assert_eq!(retained_messages, messages);
            }
            _ => panic!("partial window should retain deferred append authority"),
        }
    }

    /// Verifies absolute persisted counts are translated into local retained-window coordinates.
    /// 验证绝对持久化数量会转换为保留窗口的局部坐标。
    #[test]
    fn retained_transcript_tail_translates_absolute_window_boundary() {
        let messages = (40..44)
            .map(|index| SessionMessageEntry {
                id: format!("message-{index}"),
                kind: vulcan_context::SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::AssistantOutput,
                visible: true,
                title: String::from("output"),
                body: index.to_string(),
                metadata: None,
                created_at: index.to_string(),
            })
            .collect::<Vec<_>>();

        let tail = retained_transcript_tail_for_persisted_boundary(&messages, 40, 42)
            .expect("translate absolute boundary")
            .expect("two new messages remain");
        assert_eq!(
            tail.into_iter()
                .map(|message| message.id)
                .collect::<Vec<_>>(),
            vec![String::from("message-42"), String::from("message-43")]
        );
    }

    /// Verifies a metadata checkpoint allows a partial window to append from its exact start.
    /// 验证元数据检查点允许局部窗口从其精确起点执行追加。
    #[test]
    fn retained_transcript_tail_can_begin_at_partial_window_start() {
        let messages = vec![SessionMessageEntry {
            id: String::from("message-40"),
            kind: vulcan_context::SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::AssistantOutput,
            visible: true,
            title: String::from("output"),
            body: String::from("40"),
            metadata: None,
            created_at: String::from("40"),
        }];

        let tail = retained_transcript_tail_for_persisted_boundary(&messages, 40, 40)
            .expect("translate partial window start")
            .expect("one new message remains");
        assert_eq!(tail[0].id, "message-40");
    }
}
