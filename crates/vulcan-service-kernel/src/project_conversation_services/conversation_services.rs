//! Conversation list service handlers.
//! 会话列表服务处理器。

use super::*;

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
    // updated_at is the single commit timestamp written to metadata and SQLite.
    // updated_at 是写入元数据与 SQLite 的单一提交时间戳。
    let updated_at = current_unix_millis_text();
    // controller_runtime is returned only after the selected session is still proven visible.
    // controller_runtime 仅在所选会话仍被证明可见后返回。
    let controller_runtime = controller_runtime_for_visible_session_from_state(&state, session_id)?;
    vulcan_workspace_store::workspace_store::rename_single_session(
        controller_runtime,
        session_id,
        &normalized_name,
        &updated_at,
    )
    .await
    .map_err(|error| format!("重命名会话失败：{error}"))?;
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
    let _lifecycle_guard = begin_session_lifecycle_transition(&state, [session_id], "归档")?;
    // controller_runtime deliberately does not revalidate the mutable active snapshot before the authoritative lookup.
    // controller_runtime 刻意不在权威查询前重复校验可变活动快照。
    let controller_runtime = controller_runtime_from_state(&state)?;
    invalidate_webui_session_lifecycle_persist_versions(&state, session_id)?;
    // persist_lock serializes the directory/index transition with every background session writer.
    // persist_lock 将目录/索引转换与每个后台会话写入者串行化。
    let persist_lock = webui_workspace_persist_lock();
    // persist_guard remains held until the authoritative lifecycle mutation finishes.
    // persist_guard 持有至权威生命周期变更完成。
    let persist_guard = persist_lock.lock().await;
    // archive_result is inspected only after writers queued during the storage attempt are invalidated.
    // archive_result 仅在存储尝试期间排队的写入者失效后才进行检查。
    let archive_result = vulcan_workspace_store::workspace_store::archive_single_session(
        controller_runtime,
        session_id,
    )
    .await;
    invalidate_webui_session_lifecycle_persist_versions(&state, session_id)?;
    archive_result?;
    // refresh_result updates retained lifecycle state while stale writers remain behind the gate.
    // refresh_result 在过期写入者仍被闸门阻挡时更新保留生命周期状态。
    let refresh_result = refresh_workbench_snapshot(state.clone()).await;
    invalidate_webui_session_lifecycle_persist_versions(&state, session_id)?;
    drop(persist_guard);
    // workbench is the authoritative retained projection committed by the refresh inside the persistence gate.
    // workbench 是持久化闸门内刷新所提交的权威保留投影。
    let workbench = refresh_result?;
    let plugin_cleanup_failures =
        drain_vulcan_plugin_sessions_for_owners(&state, &[session_id]).await;
    if !plugin_cleanup_failures.is_empty() {
        eprintln!(
            "[vulcan-plugin] archived conversation cleanup warnings: {}",
            plugin_cleanup_failures.join("; ")
        );
    }
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
) -> Result<ControllerRuntimeHandle, String> {
    let guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let controller_runtime = guard.controller_runtime.clone().ok_or_else(|| {
        String::from("service is still starting; controller runtime is not ready")
    })?;
    let workspace_snapshot = guard
        .workspace_snapshot
        .as_ref()
        .ok_or_else(|| String::from("工作区快照尚未加载，无法处理会话。"))?;
    workspace_session_summary_for_session(workspace_snapshot, session_id)
        .ok_or_else(|| format!("未找到活动会话：{session_id}"))?;
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
    let (controller_runtime, runtime_state_update, workbench) = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let controller_runtime = guard.controller_runtime.clone().ok_or_else(|| {
            String::from("service is still starting; controller runtime is not ready")
        })?;
        let mut runtime_state_update = None;
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
            runtime_state_update = Some(runtime_state.clone());
        }
        for conversation in guard
            .workbench
            .conversations
            .iter_mut()
            .filter(|conversation| conversation.id == conversation_id)
        {
            conversation.unread = false;
        }
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        guard.workbench.revision = guard.workbench_generation;
        let mut workbench = guard.workbench.clone();
        refresh_selected_workbench_messages_from_runtime(&guard, &mut workbench);
        (controller_runtime, runtime_state_update, workbench)
    };

    if let Some(runtime_state_update) = runtime_state_update {
        vulcan_workspace_store::workspace_store::persist_single_session_runtime_state_update(
            controller_runtime,
            session_id,
            &runtime_state_update,
        )
        .await?;
    }
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
    if let Some(workbench) =
        select_conversation_in_retained_state(&state, session_id, &conversation_id)?
    {
        return Ok(workbench);
    }
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
    retain_hydrated_conversation_windows_for_workspace(
        &mut guard.hydrated_conversation_windows,
        &workspace_snapshot,
    );
    let selected_runtime_state = workspace_snapshot.session_runtime_state.get(&session_id);
    if let Some(agent_runtime) = guard.agent_runtime.as_mut() {
        restore_webui_runtime_controls_for_session(
            &mut agent_runtime.provider_configuration,
            selected_runtime_state,
        );
    }
    let messages =
        retained_selected_conversation_messages(&workspace_snapshot, session_id, conversation_id);
    let subagent_navigation =
        subagent_navigation_snapshot_from_cached_state(&mut guard, &workspace_snapshot)?;
    let hydrated_windows = hydrated_conversation_window_summaries(
        &guard.hydrated_conversation_windows,
        &guard.webui_preloaded_unclosed_subagent_roots,
    );

    guard.workbench.selected_conversation_id = Some(conversation_id.to_string());
    guard.workbench.messages = messages;
    guard.workbench.subagent_navigation = subagent_navigation;
    guard.workbench.hydrated_conversation_windows = hydrated_windows;
    guard.workbench.conversation_hydration = guard
        .workbench
        .conversation_hydration
        .take()
        .filter(|progress| progress.session_id == conversation_id);
    sync_selected_conversation_fields_for_retained_workbench(&mut guard.workbench, conversation_id);
    guard.workspace_snapshot = Some(workspace_snapshot);
    guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
    guard.workbench.revision = guard.workbench_generation;
    Ok(Some(guard.workbench.clone()))
}

/// Projects retained transcript rows together with authoritative live runtime records.
/// 将已保留转录行与权威实时运行时记录共同投影。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` contains retained hydrated rows and live runtime records.
/// `workspace_snapshot` 包含已保留水合行与实时运行时记录。
/// `session_id` identifies the selected root conversation.
/// `session_id` 标识选中的根会话。
/// `conversation_id` is the string identifier used by WebUI messages.
/// `conversation_id` 是 WebUI 消息使用的字符串标识。
///
/// # Returns
/// 返回值
/// A stable-id merged message list whose live stream bodies come from runtime state.
/// 返回按稳定标识合并的消息列表，其中实时流正文来自运行时状态。
fn retained_selected_conversation_messages(
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
    conversation_id: &str,
) -> Vec<ConversationMessage> {
    // retained_messages is the durable or hydrated transcript window for this session.
    // retained_messages 是当前会话的持久化或已水合转录窗口。
    let retained_messages = workspace_snapshot
        .session_messages
        .get(&session_id)
        .map(Vec::as_slice)
        .unwrap_or_default();
    // runtime_entries projects the exact records that continue changing while the session is not selected.
    // runtime_entries 投影会话未选中期间仍持续变化的精确记录。
    let runtime_entries = workspace_snapshot
        .session_runtime_state
        .get(&session_id)
        .map(|runtime_state| {
            runtime_state
                .conversation_state
                .records
                .iter()
                .map(session_message_entry_from_conversation_record)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    // merged_messages preserves retained order and replaces matching ids with authoritative runtime bodies.
    // merged_messages 保留既有顺序，并使用权威运行时正文替换相同标识的记录。
    let merged_messages = merge_session_messages_with_runtime_event_entries(
        retained_messages,
        runtime_entries.as_slice(),
    );
    conversation_messages_from_session(
        conversation_id,
        merged_messages.as_slice(),
        workspace_surface_allows_user_prompt_editing(workspace_snapshot, session_id, None),
    )
}

/// Replaces one workbench snapshot's selected messages with same-lock authoritative runtime projection.
/// 使用同一锁内的权威运行时投影替换工作台快照的当前会话消息。
///
/// # Parameters
/// # 参数
/// `guard` owns the retained workspace and runtime records at one consistent revision.
/// `guard` 在同一一致 revision 下持有保留工作区与运行时记录。
/// `workbench` is the snapshot being prepared for a full-response or recovery boundary.
/// `workbench` 是正在为全量响应或恢复边界准备的快照。
///
/// # Returns
/// # 返回值
/// Nothing; invalid draft selections or unavailable workspaces leave the snapshot unchanged.
/// 无返回值；草稿选择无效或工作区不可用时保持快照不变。
pub(crate) fn refresh_selected_workbench_messages_from_runtime(
    guard: &ServiceBridgeState,
    workbench: &mut WorkbenchSnapshot,
) {
    let Some(workspace_snapshot) = guard.workspace_snapshot.as_ref() else {
        return;
    };
    let Some(conversation_id) = workbench.selected_conversation_id.as_deref() else {
        return;
    };
    let Ok(session_id) = conversation_id.parse::<SessionId>() else {
        return;
    };
    if !workspace_snapshot
        .sessions
        .iter()
        .any(|session| session.id == session_id)
    {
        return;
    }
    if workspace_snapshot
        .session_messages
        .contains_key(&session_id)
    {
        workbench.messages = retained_selected_conversation_messages(
            workspace_snapshot,
            session_id,
            conversation_id,
        );
        return;
    }
    // runtimeMessages overlays exact live rows when the full workbench owns the only stable message window.
    // runtimeMessages 在全量工作台持有唯一稳定消息窗口时叠加精确实时行。
    let runtime_entries = workspace_snapshot
        .session_runtime_state
        .get(&session_id)
        .map(|runtime_state| {
            runtime_state
                .conversation_state
                .records
                .iter()
                .map(session_message_entry_from_conversation_record)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let runtime_messages = conversation_messages_from_session(
        conversation_id,
        runtime_entries.as_slice(),
        workspace_surface_allows_user_prompt_editing(workspace_snapshot, session_id, None),
    );
    for runtime_message in runtime_messages {
        if let Some(existing_message) = workbench
            .messages
            .iter_mut()
            .find(|message| message.id == runtime_message.id)
        {
            *existing_message = runtime_message;
        } else {
            workbench.messages.push(runtime_message);
        }
    }
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
fn sync_selected_conversation_fields_for_retained_workbench(
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
            title: String::from("归档事件测试"),
            status: String::from("Archived"),
            run_phase: String::from("idle"),
            last_updated_label: String::from("刚刚"),
            last_updated_at: String::from("1783671000000"),
            unread: false,
            attention: None,
            project_id: Some(String::from("project-a")),
            fork_source: None,
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
}
