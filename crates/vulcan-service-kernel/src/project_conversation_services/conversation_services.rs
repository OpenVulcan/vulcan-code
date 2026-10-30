//! Conversation list service handlers.
//! 会话列表服务处理器。

use super::*;
use crate::session_v1_lifecycle::{
    archive_session_lifecycle_v1, delete_session_lifecycle_v1, rename_session_lifecycle_v1,
    restore_session_lifecycle_v1,
};
use vulcan_workspace_store::workspace_store::{
    SessionV1StoragePreflightOutcome, delete_session_v1, preflight_session_v1_storage,
    session_v1_format_is_unsupported, set_session_unread_v1,
};

/// Returns the current Unix millisecond timestamp within the V1 signed contract.
/// 返回符合 V1 有符号契约的当前 Unix 毫秒时间戳。
///
/// # Returns / 返回值
///
/// A checked signed timestamp, or an explicit range failure.
/// 返回受检的有符号时间戳，超出范围时返回显式错误。
fn current_session_lifecycle_unix_millis() -> Result<i64, String> {
    i64::try_from(current_unix_millis_value()).map_err(|_| {
        String::from("Current time exceeds the V1 session lifecycle millisecond range.")
    })
}

/// Renames one authoritative active conversation and reloads the workbench from persistence.
/// 重命名一个权威活动会话，并从持久化层重新加载工作台。
///
/// # Parameters
/// 参数
/// `app` emits the committed lifecycle delta through the ordered workbench event stream.
/// `app` 通过有序工作台事件流发送已提交生命周期增量。
/// `state` owns the controller runtime and retained WebUI projection.
/// `state` 持有控制器运行时与保留的 WebUI 投影。
/// `conversation_id` is the exact visible session UUID selected by the user.
/// `conversation_id` 是用户选择的精确可见会话 UUID。
/// `name` is the requested manual display title.
/// `name` 是请求设置的手动展示标题。
///
/// # Returns
/// 返回值
/// Returns the workbench snapshot reloaded after locator-driven rename succeeds.
/// 返回 locator 驱动重命名成功后重新加载的工作台快照。
pub(crate) async fn service_rename_conversation(
    state: SharedServiceBridgeState,
    conversation_id: String,
    name: String,
) -> Result<WorkbenchSnapshot, String> {
    // normalized_name is the validated manual title shared by both persistence documents.
    // normalized_name 是两个持久化文档共享的已验证手动标题。
    let normalized_name = normalize_non_empty_name(&name)?;
    // session_id is the stable identity used for retained visibility validation and locator lookup.
    // session_id 是用于保留可见性验证与定位查询的稳定身份。
    let session_id = conversation_id
        .parse::<SessionId>()
        .map_err(|error| format!("invalid session id '{conversation_id}': {error}"))?;
    // updated_at_ms is the single semantic timestamp committed to durable V1 metadata.
    // updated_at_ms 是提交到可靠 V1 metadata 的唯一语义时间戳。
    let updated_at_ms = current_session_lifecycle_unix_millis()?;
    // controller_runtime is returned only after the selected session is still proven visible.
    // controller_runtime 仅在所选会话仍被证明可见后返回。
    let controller_runtime = controller_runtime_for_visible_session_from_state(&state, session_id)?;
    let session_runtime_registry = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        Arc::clone(&guard.session_runtime_registry)
    };
    rename_session_lifecycle_v1(
        &controller_runtime,
        session_runtime_registry.as_ref(),
        session_id,
        normalized_name,
        updated_at_ms,
    )
    .await
    .map_err(|error| format!("Failed to rename session: {error}"))?;
    refresh_workbench_snapshot(state).await
}

/// Archives one authoritative active conversation and reloads the workbench from persistence.
/// 归档一个权威活动会话，并从持久化层重新加载工作台。
///
/// # Parameters
/// 参数
/// `state` owns the controller runtime and retained WebUI projection.
/// `state` 持有控制器运行时与保留的 WebUI 投影。
/// `conversation_id` is the exact visible session UUID selected by the user.
/// `conversation_id` 是用户选择的精确可见会话 UUID。
///
/// # Returns
/// 返回值
/// Returns the workbench snapshot reloaded after locator-driven archival succeeds.
/// 返回 locator 驱动归档成功后重新加载的工作台快照。
pub(crate) async fn service_archive_conversation(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    conversation_id: String,
) -> Result<WorkbenchSnapshot, String> {
    // session_id is the stable identity validated by the authoritative SQLite lifecycle lookup.
    // session_id 是由权威 SQLite 生命周期查询校验的稳定身份。
    let session_id = conversation_id
        .parse::<SessionId>()
        .map_err(|error| format!("invalid session id '{conversation_id}': {error}"))?;
    // lifecycle_guard atomically blocks new turns and rejects archival while this session is active.
    // lifecycle_guard 原子阻止新回合，并在此会话活动时拒绝归档。
    let _lifecycle_guard = begin_session_lifecycle_transition(&state, [session_id], "archive")?;
    // controller_runtime deliberately does not revalidate the mutable active snapshot before the authoritative lookup.
    // controller_runtime 刻意不在权威查询前重复校验可变活动快照。
    let controller_runtime = controller_runtime_from_state(&state)?;
    let session_runtime_registry = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        Arc::clone(&guard.session_runtime_registry)
    };
    // vmm_lifecycle_fence blocks both stale references and replacement controllers until refresh commits.
    // vmm_lifecycle_fence 在刷新提交前同时阻止旧引用与替代控制器。
    let vmm_controller_registry = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?
        .vmm_controller_registry
        .clone();
    let vmm_lifecycle_fence = vmm_controller_registry.begin_lifecycle_transition([session_id])?;
    archive_session_lifecycle_v1(
        &controller_runtime,
        session_runtime_registry.as_ref(),
        session_id,
        current_session_lifecycle_unix_millis()?,
    )
    .await
    .map_err(|error| format!("Failed to archive session: {error}"))?;
    if let Some(runtime) = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?
        .agent_runtime
        .as_ref()
        .map(|runtime| runtime.public_ai_runtime.clone())
    {
        runtime.clear_session_selection_leases(session_id.to_string().as_str())?;
    }
    // refresh_result updates retained lifecycle state while stale writers remain behind the gate.
    // refresh_result 在过期写入者仍被闸门阻挡时更新保留生命周期状态。
    let refresh_result = refresh_workbench_snapshot(state.clone()).await;
    // workbench is the authoritative retained projection committed by the refresh inside the persistence gate.
    // workbench 是持久化闸门内刷新所提交的权威保留投影。
    let workbench = refresh_result?;
    vmm_lifecycle_fence.release()?;
    let plugin_cleanup_failures =
        drain_vulcan_plugin_sessions_for_owners(&state, &[session_id]).await;
    if !plugin_cleanup_failures.is_empty() {
        tracing::warn!(
            "[vulcan-plugin] archived conversation cleanup warnings: {}",
            plugin_cleanup_failures.join("; ")
        );
    }
    close_session_mcp_connections_for_owners(&state, &[session_id]).await;
    // lifecycle_event carries only the archive-owned sidebar transition at the same committed generation.
    // lifecycle_event 仅携带同一已提交 generation 下归档领域拥有的侧栏转换。
    let lifecycle_event = conversation_archived_delta_envelope(&workbench, session_id);
    emit_service_event_with_trace(
        &app,
        "workbench_conversation_archived_delta_emit",
        ServiceEvent::WorkbenchDelta(Box::new(lifecycle_event)),
    );
    Ok(workbench)
}

/// Identifies the authoritative lifecycle operation requested for archived conversations.
/// 标识归档会话请求执行的权威生命周期操作。
#[derive(Clone, Copy)]
enum ArchivedConversationAction {
    /// Move archived sessions back into the active lifecycle tree.
    /// 将归档会话移回活动生命周期目录树。
    Restore,
    /// Remove archived sessions and their index rows permanently.
    /// 永久移除归档会话及其索引行。
    Delete,
}

/// Parses and deduplicates the exact archived conversation identities from one batch request.
/// 解析并去重一次批量请求中的精确归档会话身份。
fn parse_archived_conversation_ids(
    conversation_ids: Vec<String>,
) -> Result<Vec<SessionId>, String> {
    if conversation_ids.is_empty() {
        return Err(String::from(
            "Archived session batch operation requires at least one session id.",
        ));
    }
    let mut unique_ids = BTreeSet::new();
    for conversation_id in conversation_ids {
        let session_id = conversation_id
            .parse::<SessionId>()
            .map_err(|error| format!("invalid session id '{conversation_id}': {error}"))?;
        if !unique_ids.insert(session_id) {
            return Err(format!(
                "Archived session batch operation contains a duplicate session id: {session_id}"
            ));
        }
    }
    Ok(unique_ids.into_iter().collect())
}

/// Applies one archived-conversation batch under shared lifecycle, persistence, and VMM fences.
/// 在共享生命周期、持久化与 VMM 栅栏下应用一次归档会话批量操作。
///
/// # Parameters
/// 参数
/// `state` owns the authoritative service runtime and retained workbench projection.
/// `state` 持有权威服务运行时与保留工作台投影。
/// `session_ids` are the exact unique archived conversation identities.
/// `session_ids` 是精确且唯一的归档会话身份集合。
/// `action` selects restore or permanent deletion.
/// `action` 选择恢复或彻底删除。
///
/// # Returns
/// 返回值
/// Returns the refreshed authoritative workbench snapshot after every requested transition commits.
/// 所有请求转换提交后返回刷新的权威工作台快照。
async fn apply_archived_conversation_batch(
    state: SharedServiceBridgeState,
    session_ids: Vec<SessionId>,
    action: ArchivedConversationAction,
) -> Result<WorkbenchSnapshot, String> {
    let action_label = match action {
        ArchivedConversationAction::Restore => "restore",
        ArchivedConversationAction::Delete => "permanently delete",
    };
    let _lifecycle_guard =
        begin_session_lifecycle_transition(&state, session_ids.iter().copied(), action_label)?;
    let controller_runtime = controller_runtime_from_state(&state)?;
    let session_runtime_registry = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        Arc::clone(&guard.session_runtime_registry)
    };
    let vmm_controller_registry = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?
        .vmm_controller_registry
        .clone();
    let vmm_lifecycle_fence =
        vmm_controller_registry.begin_lifecycle_transition(session_ids.iter().copied())?;
    if matches!(action, ArchivedConversationAction::Delete) {
        let plugin_cleanup_failures =
            drain_vulcan_plugin_sessions_for_owners(&state, &session_ids).await;
        if !plugin_cleanup_failures.is_empty() {
            return Err(format!(
                "Failed to clean up plugin runtime before permanently deleting archived session: {}",
                plugin_cleanup_failures.join("; ")
            ));
        }
        if let Some(runtime) = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?
            .agent_runtime
            .as_ref()
            .map(|runtime| runtime.public_ai_runtime.clone())
        {
            for session_id in &session_ids {
                runtime.clear_session_selection_leases(session_id.to_string().as_str())?;
            }
        }
    }
    let restore_now_ms = current_session_lifecycle_unix_millis()?;
    for session_id in &session_ids {
        match action {
            ArchivedConversationAction::Restore => {
                restore_session_lifecycle_v1(
                    &controller_runtime,
                    session_runtime_registry.as_ref(),
                    *session_id,
                    restore_now_ms,
                )
                .await
                .map_err(|error| format!("Failed to restore session `{session_id}`: {error}"))?;
            }
            ArchivedConversationAction::Delete => {
                delete_session_lifecycle_v1(
                    &controller_runtime,
                    session_runtime_registry.as_ref(),
                    *session_id,
                )
                .await
                .map_err(|error| format!("Failed to delete session `{session_id}`: {error}"))?;
            }
        }
    }
    let workbench = refresh_workbench_snapshot(state.clone()).await?;
    vmm_lifecycle_fence.release()?;
    Ok(workbench)
}

/// Restores every selected archived conversation in one authoritative batch.
/// 在一次权威批量操作中恢复所有选中的归档会话。
pub(crate) async fn service_restore_archived_conversations(
    state: SharedServiceBridgeState,
    conversation_ids: Vec<String>,
) -> Result<WorkbenchSnapshot, String> {
    let session_ids = parse_archived_conversation_ids(conversation_ids)?;
    apply_archived_conversation_batch(state, session_ids, ArchivedConversationAction::Restore).await
}

/// Permanently deletes every selected archived conversation in one authoritative batch.
/// 在一次权威批量操作中彻底删除所有选中的归档会话。
pub(crate) async fn service_delete_archived_conversations(
    state: SharedServiceBridgeState,
    conversation_ids: Vec<String>,
) -> Result<WorkbenchSnapshot, String> {
    let session_ids = parse_archived_conversation_ids(conversation_ids)?;
    apply_archived_conversation_batch(state, session_ids, ArchivedConversationAction::Delete).await
}

/// Deletes one active session whose metadata uses a format this build no longer reads.
/// 删除一个元数据格式已不受本构建支持的活动会话。
///
/// # Parameters
/// 参数
/// `conversation_id` is the exact session UUID the user confirmed for deletion.
/// `conversation_id` 是用户确认删除的精确会话 UUID。
///
/// # Returns
/// 返回值
/// Returns the reloaded workbench. Such a session can be neither loaded nor archived, so its
/// directory and index row are removed directly; readable sessions are refused and keep the
/// archive-then-delete path with its collaboration checks.
/// 返回重新加载的工作台。此类会话既无法加载也无法归档，因此直接删除其目录与索引记录；
/// 可读会话一律拒绝，继续走带协作检查的“先归档再删除”路径。
pub(crate) async fn service_delete_unsupported_conversation(
    state: SharedServiceBridgeState,
    conversation_id: String,
) -> Result<WorkbenchSnapshot, String> {
    let session_id = conversation_id
        .parse::<SessionId>()
        .map_err(|error| format!("invalid session id '{conversation_id}': {error}"))?;
    let _lifecycle_guard =
        begin_session_lifecycle_transition(&state, [session_id], "delete unsupported")?;
    let controller_runtime = controller_runtime_from_state(&state)?;
    let unsupported =
        session_v1_format_is_unsupported(&controller_runtime, &session_id.to_string())
            .await
            .map_err(|error| format!("Failed to inspect session `{session_id}`: {error}"))?;
    if !unsupported {
        return Err(format!(
            "ERR_SESSION_FORMAT_SUPPORTED: session `{session_id}` is readable; archive it before deleting."
        ));
    }
    if let Some(runtime) = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?
        .agent_runtime
        .as_ref()
        .map(|runtime| runtime.public_ai_runtime.clone())
    {
        runtime.clear_session_selection_leases(session_id.to_string().as_str())?;
    }
    delete_session_v1(&controller_runtime, &session_id.to_string())
        .await
        .map_err(|error| format!("Failed to delete session `{session_id}`: {error}"))?;
    refresh_workbench_snapshot(state).await
}

/// Builds the ordered lifecycle delta that represents one committed archive refresh.
/// 构建表示单次已提交归档刷新的有序生命周期增量。
///
/// # Parameters
/// 参数
/// `workbench` is the authoritative post-archive retained projection.
/// `workbench` 是归档后的权威保留投影。
/// `session_id` identifies the conversation removed from the active collection.
/// `session_id` 标识从活动集合移除的会话。
///
/// # Returns
/// 返回值
/// Returns an envelope at the exact generation committed by the archive refresh.
/// 返回使用归档刷新所提交精确 generation 的增量信封。
fn conversation_archived_delta_envelope(
    workbench: &WorkbenchSnapshot,
    session_id: SessionId,
) -> WorkbenchDeltaEnvelope {
    WorkbenchDeltaEnvelope {
        generation: workbench.revision,
        delta: WorkbenchDelta::ConversationArchived(conversation_archived_delta(
            workbench, session_id,
        )),
    }
}

/// Builds one exact conversation archive payload from the committed workbench projection.
/// 从已提交工作台投影构建一条精确会话归档载荷。
///
/// # Parameters
/// 参数
/// `workbench` is the authoritative post-archive retained projection.
/// `workbench` 是归档后的权威保留投影。
/// `session_id` identifies the conversation removed from the active collection.
/// `session_id` 标识从活动集合移除的会话。
///
/// # Returns
/// 返回值
/// Returns the target-owned lifecycle payload without unrelated workbench fields.
/// 返回仅拥有目标生命周期字段且不含无关工作台字段的载荷。
pub(super) fn conversation_archived_delta(
    workbench: &WorkbenchSnapshot,
    session_id: SessionId,
) -> ConversationArchivedDelta {
    // conversation_id is shared by active removal and archived-summary lookup.
    // conversation_id 由活动移除与归档摘要查询共同使用。
    let conversation_id = session_id.to_string();
    // archived_conversation remains optional because the workbench intentionally hides non-restorable archived sessions.
    // archived_conversation 保持可选，因为工作台会刻意隐藏不可恢复的归档会话。
    let archived_conversation = workbench
        .archived_conversations
        .iter()
        .find(|conversation| conversation.id == conversation_id)
        .cloned();
    ConversationArchivedDelta {
        conversation_id,
        archived_conversation,
        selected_conversation_id: workbench.selected_conversation_id.clone(),
    }
}

/// Builds one ordered batch archive envelope for a project-level lifecycle transaction.
/// 为一次项目级生命周期事务构建一条有序批量归档信封。
///
/// # Parameters
/// 参数
/// `workbench` is the authoritative post-archive retained projection.
/// `workbench` 是归档后的权威保留投影。
/// `session_ids` are the exact child sessions archived by the transaction.
/// `session_ids` 是事务归档的精确子会话集合。
///
/// # Returns
/// 返回值
/// Returns one envelope so the transport never drops same-generation sibling transitions.
/// 返回单个信封，避免传输层丢弃同 generation 的同级转换。
pub(super) fn conversations_archived_delta_envelope(
    workbench: &WorkbenchSnapshot,
    session_ids: &[SessionId],
) -> WorkbenchDeltaEnvelope {
    // archived_conversations retains one target-owned transition per archived child session.
    // archived_conversations 为每个已归档子会话保留一条目标自有转换。
    let archived_conversations = session_ids
        .iter()
        .map(|session_id| conversation_archived_delta(workbench, *session_id))
        .collect();
    WorkbenchDeltaEnvelope {
        generation: workbench.revision,
        delta: WorkbenchDelta::ConversationsArchived(ConversationsArchivedDelta {
            archived_conversations,
        }),
    }
}

/// Returns the controller runtime after validating that one session remains visible in retained state.
/// 在验证一个会话仍处于保留可见状态后返回控制器运行时。
///
/// # Parameters
/// 参数
/// `state` owns the controller runtime and retained workspace snapshot.
/// `state` 持有控制器运行时与保留的工作区快照。
/// `session_id` is the exact visible session identity selected by the user.
/// `session_id` 是用户选择的精确可见会话身份。
///
/// # Returns
/// 返回值
/// Returns the controller runtime without deriving project or directory identity from the snapshot.
/// 返回控制器运行时，且不从快照推导项目或目录身份。
fn controller_runtime_for_visible_session_from_state(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
) -> Result<HostRuntimeHandle, String> {
    let guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let controller_runtime = guard.controller_runtime.clone().ok_or_else(|| {
        String::from("service is still starting; controller runtime is not ready")
    })?;
    let workspace_snapshot = guard.workspace_snapshot.as_ref().ok_or_else(|| {
        String::from("Workspace snapshot is not loaded; cannot process the session.")
    })?;
    workspace_session_summary_for_session(workspace_snapshot, session_id)
        .ok_or_else(|| format!("Active session not found: {session_id}"))?;
    Ok(controller_runtime)
}

/// Marks one conversation as read and returns the retained workbench snapshot.
/// 将单个会话标记为已读并返回保留的工作台快照。
pub(crate) async fn service_mark_conversation_read(
    state: SharedServiceBridgeState,
    conversation_id: String,
) -> Result<WorkbenchSnapshot, String> {
    let session_id = conversation_id
        .parse::<SessionId>()
        .map_err(|error| format!("invalid session id '{conversation_id}': {error}"))?;
    let (controller_runtime, workbench) = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let controller_runtime = guard.controller_runtime.clone().ok_or_else(|| {
            String::from("service is still starting; controller runtime is not ready")
        })?;
        if let Some(runtime_state) =
            guard
                .workspace_snapshot
                .as_mut()
                .and_then(|workspace_snapshot| {
                    workspace_snapshot
                        .session_runtime_state
                        .get_mut(&session_id)
                })
        {
            runtime_state.ui_state.unread = false;
            runtime_state.updated_at = current_unix_millis_text();
        }
        for conversation in guard
            .workbench
            .conversations
            .iter_mut()
            .filter(|conversation| conversation.id == conversation_id)
        {
            conversation.unread = false;
            // Unread attention mirrors the flag; leaving it would keep the dot in later retained snapshots.
            // 未读关注是该标志的镜像；保留它会使后续保留快照中的圆点持续存在。
            if conversation
                .attention
                .as_ref()
                .is_some_and(|attention| attention.kind == "unread")
            {
                conversation.attention = None;
            }
        }
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        guard.workbench.revision = guard.workbench_generation;
        let workbench = guard.workbench.clone();
        (controller_runtime, workbench)
    };

    // The V1 SQLite row is the sole durable owner of the lightweight list unread projection.
    // V1 SQLite 记录是轻量列表未读投影的唯一持久化所有者。
    set_session_unread_v1(&controller_runtime, &session_id.to_string(), false)
        .await
        .map_err(|error| format!("Failed to mark session as read: {error}"))?;
    Ok(workbench)
}

/// Selects one conversation in the retained WebUI workspace state.
/// 在 WebUI 保留的工作区状态中选中一个会话。
pub(crate) async fn service_select_conversation(
    state: SharedServiceBridgeState,
    conversation_id: String,
) -> Result<WorkbenchSnapshot, String> {
    let session_id = conversation_id
        .parse::<SessionId>()
        .map_err(|error| format!("invalid session id '{conversation_id}': {error}"))?;
    // controller_runtime is optional only while the retained in-memory service fixture is not bootstrapped.
    // controller_runtime 仅在保留内存服务夹具尚未启动时可选。
    let controller_runtime = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        guard.controller_runtime.clone()
    };
    let cleanup_result = if let Some(controller_runtime) = controller_runtime.as_ref() {
        // Selection is a read/navigation operation and must remain available while the target session runs.
        // 选择属于读取/导航操作，即使目标会话正在运行也必须保持可用。
        match preflight_session_v1_storage(controller_runtime, &session_id.to_string())
            .await
            .map_err(|error| error.to_string())?
        {
            SessionV1StoragePreflightOutcome::Ready { .. } => false,
            SessionV1StoragePreflightOutcome::MissingStorageIndexRemoved { .. } => true,
        }
    } else {
        false
    };
    if cleanup_result {
        // A missing session is already removed; publish the remaining workspace without a navigation error.
        // 丢失会话已删除；发布其余工作台，不产生导航错误。
        return refresh_workbench_snapshot(state).await;
    }
    // Resolve metadata before entering the retained navigation path, which deliberately never loads history.
    // 进入保留导航路径前解析 metadata；该路径刻意不加载历史。
    let registry = Arc::clone(
        &state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?
            .session_runtime_registry,
    );
    // The optional controller exists only outside the retained in-memory service fixtures.
    // 仅保留内存服务夹具以外的场景存在可选控制器。
    let model_projection = if let Some(controller_runtime) = controller_runtime.as_ref() {
        Some(
            crate::session_model_projection_v1::read_session_model_projection_v1(
                controller_runtime,
                registry.as_ref(),
                session_id,
            )
            .await?,
        )
    } else {
        None
    };
    if let Some(projection) = model_projection.as_ref() {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        if let Some(workspace) = guard.workspace_snapshot.as_mut() {
            projection.apply(registry.as_ref(), &mut workspace.session_runtime_state)?;
        }
    }
    if let Some(workbench) =
        select_conversation_in_retained_state(&state, session_id, &conversation_id)?
    {
        return Ok(workbench);
    }
    // The full refresh resolves the same metadata owner, so release this navigation guard first.
    // 完整刷新会解析同一 metadata owner，因此先释放本次导航守卫。
    drop(model_projection);
    refresh_workbench_snapshot_for_selected_session(state, session_id).await
}

/// Selects one retained conversation without reading segmented files or rebuilding the full workbench.
/// 不读取分段文件且不重建完整工作台，选中一个已保留的会话。
///
/// # Parameters
/// 参数
/// `state` is the shared WebUI service state that owns the retained workspace snapshot.
/// `state` 是持有保留工作区快照的共享 WebUI 服务状态。
/// `session_id` identifies the conversation selected by the sidebar.
/// `session_id` 标识侧栏选中的会话。
/// `conversation_id` is the string form returned to the WebUI snapshot.
/// `conversation_id` 是返回给 WebUI 快照的字符串形式。
/// `subagent_surface_preload` is an optional lock-free surface preload captured before selection.
/// `subagent_surface_preload` 是选择前在锁外完成的可选界面预载结果。
///
/// # Returns
/// 返回值
/// `Some(workbench)` when the retained workspace contains the session, or `None` when a full refresh is required.
/// 当保留工作区包含该会话时返回 `Some(workbench)`；需要完整刷新时返回 `None`。
fn select_conversation_in_retained_state(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    conversation_id: &str,
) -> Result<Option<WorkbenchSnapshot>, String> {
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let Some(mut workspace_snapshot) = guard.workspace_snapshot.take() else {
        return Err(String::from(
            "service is still starting; workspace snapshot is not ready",
        ));
    };
    let workspace_contains_session = workspace_snapshot
        .sessions
        .iter()
        .any(|session| session.id == session_id);
    let workbench_contains_conversation = guard
        .workbench
        .conversations
        .iter()
        .any(|conversation| conversation.id == conversation_id);
    if !workspace_contains_session || !workbench_contains_conversation {
        guard.workspace_snapshot = Some(workspace_snapshot);
        return Ok(None);
    }

    workspace_snapshot.selected_session = session_id;
    guard.active_subagent_panel = None;
    let selected_runtime_state = workspace_snapshot.session_runtime_state.get(&session_id);
    if let Some(agent_runtime) = guard.agent_runtime.as_mut() {
        restore_webui_runtime_controls_for_session(
            &mut agent_runtime.provider_configuration,
            selected_runtime_state,
        );
    }
    let provider_configuration = guard
        .agent_runtime
        .as_ref()
        .map(|agent_runtime| agent_runtime.provider_configuration.clone());
    let subagent_navigation =
        subagent_navigation_snapshot_from_cached_state(&mut guard, &workspace_snapshot)?;

    guard.workbench.selected_conversation_id = Some(conversation_id.to_string());
    guard.workbench.subagent_navigation = subagent_navigation;
    // Retained addon rows drive both backend and frontend selection; refresh the node from metadata.
    // 保留的附属行同时驱动后端和前端选择；根据元数据刷新节点。
    if let Some(runtime) = selected_runtime_state {
        // An explicit None clears the node; an absent runtime supplies no new projection authority.
        // 显式 None 清除节点；缺失运行态则不提供新的投影权威。
        let node_id = runtime
            .context_compaction_window_node_id
            .map(|node| node.as_str().to_string());
        if let Some(addons) = guard
            .workbench
            .session_addons_by_conversation
            .iter_mut()
            .find(|entry| entry.conversation_id == conversation_id)
        {
            addons.session_context_compaction_window_node_id = node_id;
        } else if let Some(addons) = session_addons_changed_delta_for_retained_list(
            conversation_id.to_string(),
            None,
            None,
            None,
            None,
            None,
            node_id,
            None,
        ) {
            guard.workbench.session_addons_by_conversation.push(addons);
        }
    }
    sync_selected_conversation_fields_for_retained_workbench(&mut guard.workbench, conversation_id);
    guard.workspace_snapshot = Some(workspace_snapshot);
    if let Some(provider_configuration) = provider_configuration.as_ref() {
        sync_workbench_runtime_control_fields_locked(&mut guard, provider_configuration);
    }
    guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
    guard.workbench.revision = guard.workbench_generation;
    Ok(Some(guard.workbench.clone()))
}

/// Reprojects selected-only workbench fields from retained per-conversation caches.
/// 从按会话保留的缓存重新投影仅当前会话使用的工作台字段。
///
/// # Parameters
/// 参数
/// `workbench` is the retained snapshot being updated in place.
/// `workbench` 是正在原地更新的保留快照。
/// `conversation_id` identifies the newly selected conversation.
/// `conversation_id` 标识新选中的会话。
pub(crate) fn sync_selected_conversation_fields_for_retained_workbench(
    workbench: &mut WorkbenchSnapshot,
    conversation_id: &str,
) {
    workbench.pending_question = workbench
        .pending_questions
        .iter()
        .find(|question| question.session_id == conversation_id)
        .cloned();
    workbench.pending_prompts = workbench
        .pending_prompts_by_conversation
        .iter()
        .find(|entry| entry.conversation_id == conversation_id)
        .map(|entry| entry.pending_prompts.clone())
        .unwrap_or_default();
    let selected_addons = workbench
        .session_addons_by_conversation
        .iter()
        .find(|entry| entry.conversation_id == conversation_id);
    workbench.session_goal = selected_addons.and_then(|entry| entry.session_goal.clone());
    workbench.session_todolist = selected_addons.and_then(|entry| entry.session_todolist.clone());
    workbench.session_plan_mode = selected_addons.and_then(|entry| entry.session_plan_mode.clone());
    workbench.session_context_compaction_window_node_id =
        selected_addons.and_then(|entry| entry.session_context_compaction_window_node_id.clone());
    workbench.session_vmm_policy = workbench
        .session_vmm_policies_by_conversation
        .iter()
        .find(|entry| entry.conversation_id == conversation_id)
        .map(|entry| entry.session_vmm_policy.clone())
        .or_else(|| selected_addons.and_then(|entry| entry.session_vmm_policy.clone()));
    workbench.context_injection = workbench
        .context_injection_by_conversation
        .iter()
        .find(|entry| entry.conversation_id == conversation_id)
        .and_then(|entry| entry.context_injection.clone());
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies archive lifecycle deltas preserve the committed generation and authoritative projection.
    /// 验证归档生命周期增量保留已提交 generation 与权威投影。
    #[test]
    fn conversation_archive_delta_uses_committed_workbench_projection() {
        // session_id identifies the archived conversation under test.
        // session_id 标识测试中的已归档会话。
        let session_id = SessionId::from_u128(0x52);
        // archived_conversation is the exact restorable row exposed by the post-archive workbench.
        // archived_conversation 是归档后工作台暴露的精确可恢复行。
        let archived_conversation = ConversationSummary {
            id: session_id.to_string(),
            title: String::from("Archive event test"),
            status: String::from("Archived"),
            run_phase: String::from("idle"),
            last_updated_label: String::from("just now"),
            last_updated_at: String::from("1783671000000"),
            unread: false,
            attention: None,
            project_id: Some(String::from("project-a")),
            dedicated: false,
            assistant_name: None,
        };
        // workbench supplies the authoritative lifecycle domain after refresh commit.
        // workbench 提供刷新提交后的权威生命周期领域。
        let mut workbench = crate::fallback_workbench_projection::fallback_workbench_snapshot(&[]);
        workbench.revision = 52;
        workbench.archived_conversations = vec![archived_conversation.clone()];
        workbench.selected_conversation_id = Some(String::from("next-session"));

        // envelope is the transport projection emitted before the archive RPC returns.
        // envelope 是归档 RPC 返回前发送的传输投影。
        let envelope = conversation_archived_delta_envelope(&workbench, session_id);

        assert_eq!(envelope.generation, 52);
        assert_eq!(
            envelope.delta,
            WorkbenchDelta::ConversationArchived(ConversationArchivedDelta {
                conversation_id: session_id.to_string(),
                archived_conversation: Some(archived_conversation),
                selected_conversation_id: Some(String::from("next-session")),
            })
        );
    }

    /// Verifies project-level archive operations keep every child transition in one generation envelope.
    /// 验证项目级归档操作会把全部子转换保留在单个 generation 信封中。
    #[test]
    fn conversation_archive_batch_uses_one_transport_envelope() {
        // first_session_id identifies the first archived child transition.
        // first_session_id 标识第一条已归档子转换。
        let first_session_id = SessionId::from_u128(0x61);
        // second_session_id identifies the second archived child transition.
        // second_session_id 标识第二条已归档子转换。
        let second_session_id = SessionId::from_u128(0x62);
        // workbench supplies the shared committed generation and final selection.
        // workbench 提供共享的已提交 generation 与最终选择。
        let mut workbench = crate::fallback_workbench_projection::fallback_workbench_snapshot(&[]);
        workbench.revision = 61;
        workbench.selected_conversation_id = Some(String::from("remaining-session"));

        let envelope = conversations_archived_delta_envelope(
            &workbench,
            &[first_session_id, second_session_id],
        );

        assert_eq!(envelope.generation, 61);
        let WorkbenchDelta::ConversationsArchived(delta) = envelope.delta else {
            panic!("expected conversations archived delta");
        };
        assert_eq!(delta.archived_conversations.len(), 2);
        assert_eq!(
            delta.archived_conversations[0].conversation_id,
            first_session_id.to_string()
        );
        assert_eq!(
            delta.archived_conversations[1].conversation_id,
            second_session_id.to_string()
        );
    }

    /// Verifies archived batch parsing rejects empty and duplicate identity requests.
    /// 验证归档批量解析会拒绝空请求与重复身份请求。
    #[test]
    fn archived_conversation_batch_parser_rejects_empty_and_duplicate_ids() {
        assert!(parse_archived_conversation_ids(Vec::new()).is_err());
        let session_id = SessionId::from_u128(0x71).to_string();
        let error = parse_archived_conversation_ids(vec![session_id.clone(), session_id]);
        assert!(error.is_err());
    }

    /// Verifies archived batch parsing preserves every unique stable session identity.
    /// 验证归档批量解析保留每个唯一稳定会话身份。
    #[test]
    fn archived_conversation_batch_parser_returns_unique_ids() {
        let first_session_id = SessionId::from_u128(0x72);
        let second_session_id = SessionId::from_u128(0x73);
        let parsed = parse_archived_conversation_ids(vec![
            first_session_id.to_string(),
            second_session_id.to_string(),
        ])
        .expect("unique archived conversation ids should parse");
        assert_eq!(parsed, vec![first_session_id, second_session_id]);
    }
}
