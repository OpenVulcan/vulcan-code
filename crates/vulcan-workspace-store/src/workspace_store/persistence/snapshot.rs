use super::*;

/// Persists one session and its runtime artifacts without rewriting unrelated workspace sessions.
/// 仅持久化单个会话及其运行时工件，不重写无关工作区会话。
///
/// # Parameters
/// 参数
/// `controller` owns the authoritative SQLite binding and segmented data root.
/// `controller` 持有权威 SQLite 绑定与分段数据根。
/// `task_projects` defines the only accepted project membership for the session.
/// `task_projects` 定义该会话唯一可接受的项目成员关系。
/// `session` is the exact session summary being persisted.
/// `session` 是正在持久化的精确会话摘要。
/// `archived` selects the active or archived filesystem and locator state.
/// `archived` 选择活动或归档文件系统与定位行状态。
/// `messages` contains the caller-retained visible transcript rows.
/// `messages` 包含调用方保留的可见转录行。
/// `runtime_state` optionally provides the authoritative runtime checkpoint and records.
/// `runtime_state` 可选提供权威运行时检查点与记录。
///
/// # Returns
/// 返回值
/// Returns success only after segmented artifacts and the SQLite locator are both persisted.
/// 仅在分段工件与 SQLite 定位行都持久化后成功。
pub async fn persist_single_session_update(
    controller: ControllerRuntimeHandle,
    task_projects: &[TaskProjectSummary],
    session: &SessionSummary,
    archived: bool,
    messages: &[SessionMessageEntry],
    runtime_state: Option<&SessionRuntimeStateEntry>,
) -> Result<(), String> {
    // session_owner_map is the authoritative project-membership projection supplied by the caller.
    // session_owner_map 是调用方提供的权威项目成员关系投影。
    let session_owner_map = session_owner_map_from_projects(task_projects);
    let Some((project_id, _sort_order)) = session_owner_map.get(&session.id) else {
        return Err(format!(
            "单会话保存失败：未找到会话所属项目：{}",
            session.id
        ));
    };
    // active_locator_exists records whether the authoritative index currently owns the active lifecycle.
    // active_locator_exists 记录权威索引当前是否拥有活动生命周期。
    let active_locator_exists = query_session_index_locator(&controller, session.id, false)
        .await?
        .is_some();
    // archived_locator_exists records whether the authoritative index currently owns the archived lifecycle.
    // archived_locator_exists 记录权威索引当前是否拥有归档生命周期。
    let archived_locator_exists = query_session_index_locator(&controller, session.id, true)
        .await?
        .is_some();
    validate_single_session_update_lifecycle(
        session.id,
        archived,
        active_locator_exists,
        archived_locator_exists,
    )?;

    // rebuilt_messages preserves caller-visible rows while incorporating authoritative runtime records.
    // rebuilt_messages 在纳入权威运行时记录的同时保留调用方可见行。
    let rebuilt_messages = runtime_state
        .map(|runtime_state| effective_session_messages_for_persistence(messages, runtime_state));
    // effective_messages is the exact transcript written before the locator becomes discoverable.
    // effective_messages 是定位行可发现前写入的精确转录。
    let effective_messages = rebuilt_messages.as_deref().unwrap_or(messages);
    // Persist filesystem artifacts first so SQLite never publishes a locator for a failed file write.
    // 先持久化文件系统工件，避免 SQLite 为失败的文件写入发布定位行。
    persist_segmented_session_update(
        &controller,
        session,
        project_id,
        archived,
        effective_messages,
        runtime_state,
    )
    .await?;
    // Publish or update the locator only after its backing segmented state is durable.
    // 仅在底层分段状态耐久化后发布或更新定位行。
    upsert_session_index_row(
        &controller,
        session,
        project_id,
        archived,
        0,
        runtime_state.map(|state| state.updated_at.as_str()),
    )
    .await
}

/// Validates that an ordinary session snapshot write cannot change authoritative archive state.
/// 校验普通会话快照写入不能改变权威归档状态。
///
/// # Parameters
/// 参数
/// `session_id` identifies the exact session being persisted.
/// `session_id` 标识正在持久化的精确会话。
/// `requested_archived` is the lifecycle state carried by the caller snapshot.
/// `requested_archived` 是调用方快照携带的生命周期状态。
/// `active_locator_exists` reports whether SQLite currently owns an active locator.
/// `active_locator_exists` 表示 SQLite 当前是否拥有活动定位行。
/// `archived_locator_exists` reports whether SQLite currently owns an archived locator.
/// `archived_locator_exists` 表示 SQLite 当前是否拥有归档定位行。
///
/// # Returns
/// 返回值
/// Returns success only for a new active session or an update that preserves the existing lifecycle.
/// 仅对新活动会话或保持现有生命周期的更新返回成功。
fn validate_single_session_update_lifecycle(
    session_id: SessionId,
    requested_archived: bool,
    active_locator_exists: bool,
    archived_locator_exists: bool,
) -> Result<(), String> {
    if active_locator_exists && archived_locator_exists {
        return Err(format!(
            "单会话保存失败：会话 `{session_id}` 同时存在活动与归档定位行。"
        ));
    }
    if requested_archived {
        if archived_locator_exists && !active_locator_exists {
            return Ok(());
        }
        return Err(format!(
            "单会话保存失败：会话 `{session_id}` 的归档状态只能通过生命周期接口变更。"
        ));
    }
    if archived_locator_exists {
        return Err(format!(
            "单会话保存失败：会话 `{session_id}` 已归档，普通快照写入不能将其恢复为活动状态。"
        ));
    }
    Ok(())
}

/// Persists only one session runtime-state document without rewriting transcript or project rows.
/// 仅持久化单个会话运行时状态文档，不重写转录或项目行。
pub async fn persist_single_session_runtime_state_update(
    controller: ControllerRuntimeHandle,
    session_id: SessionId,
    runtime_state: &SessionRuntimeStateEntry,
) -> Result<(), String> {
    persist_segmented_session_runtime_state_checkpoint(&controller, session_id, runtime_state)
        .await?;

    Ok(())
}

/// Persists the complete visible transcript window without touching runtime state.
/// 持久化完整可见转录窗口，且不触碰运行态。
///
/// # Parameters
/// # 参数
/// `controller` is the workspace data-plane controller handle.
/// `controller` 是工作区数据平面控制器句柄。
/// `session_id` identifies the segmented session receiving the transcript update.
/// `session_id` 标识接收转录更新的分段会话。
/// `messages` is the complete visible transcript window retained by the WebUI.
/// `messages` 是 WebUI 当前保留的完整可见转录窗口。
///
/// # Returns
/// # 返回值
/// Returns `Ok(())` after the transcript tail or replacement boundary is persisted.
/// 写入转录尾部或替换边界后返回 `Ok(())`。
pub async fn persist_single_session_transcript_messages(
    controller: ControllerRuntimeHandle,
    session_id: SessionId,
    messages: &[SessionMessageEntry],
) -> Result<(), String> {
    persist_single_session_transcript_messages_in_data_root(
        &controller.data_root,
        session_id,
        messages,
    )
}

/// Persists one complete transcript window into its main-session or TaskSession directory.
/// 将一个完整转录窗口持久化到所属主会话或 TaskSession 目录。
///
/// # Parameters
/// 参数
/// `data_root` is the segmented workspace data root.
/// `data_root` 是分段工作区数据根目录。
/// `session_id` identifies the exact main session or child task.
/// `session_id` 标识精确的主会话或子任务。
/// `messages` is the authoritative visible transcript window.
/// `messages` 是权威可见转录窗口。
///
/// # Returns
/// 返回值
/// Returns success after the resolved transcript directory is updated, or when no durable owner exists.
/// 在已解析转录目录完成更新后返回成功；不存在持久化归属时也返回成功。
pub(crate) fn persist_single_session_transcript_messages_in_data_root(
    data_root: &Path,
    session_id: SessionId,
    messages: &[SessionMessageEntry],
) -> Result<(), String> {
    if messages.is_empty() {
        return Ok(());
    }
    let Some(session_dir) =
        find_segmented_session_or_task_directory_by_id(data_root, &session_id.to_string())?
    else {
        return Ok(());
    };
    let _ = repair_segmented_session_display_if_needed(&session_dir)?;
    append_segmented_session_message_update(&session_dir, messages)?;
    Ok(())
}

/// Builds the visible transcript slice persisted together with runtime checkpoints.
/// 构建与运行态检查点一起持久化的可见转录切片。
///
/// # Parameters
/// # 参数
/// `stored_messages` is the WebUI display window already retained by the caller.
/// `stored_messages` 是调用方已保留的 WebUI 显示窗口。
/// `runtime_state` is the latest canonical runtime state whose display records should be upserted.
/// `runtime_state` 是需要增量合并显示记录的最新规范运行态。
///
/// # Returns
/// # 返回值
/// Returns display messages that preserve existing visible history while refreshing runtime-owned rows.
/// 返回既保留既有可见历史又刷新运行态显示行的消息列表。
pub(super) fn effective_session_messages_for_persistence(
    stored_messages: &[SessionMessageEntry],
    runtime_state: &SessionRuntimeStateEntry,
) -> Vec<SessionMessageEntry> {
    merge_session_messages_with_conversation_state_records(
        stored_messages,
        &runtime_state.conversation_state,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies ordinary snapshot writes preserve the authoritative session lifecycle boundary.
    /// 验证普通快照写入保持权威会话生命周期边界。
    #[test]
    fn single_session_update_lifecycle_rejects_archive_state_transitions() {
        // session_id identifies every lifecycle validation case in this test.
        // session_id 标识本测试中的每个生命周期校验场景。
        let session_id = SessionId::from_u128(0x91);

        assert!(validate_single_session_update_lifecycle(session_id, false, false, false).is_ok());
        assert!(validate_single_session_update_lifecycle(session_id, false, true, false).is_ok());
        assert!(validate_single_session_update_lifecycle(session_id, true, false, true).is_ok());
        assert!(validate_single_session_update_lifecycle(session_id, false, false, true).is_err());
        assert!(validate_single_session_update_lifecycle(session_id, true, true, false).is_err());
        assert!(validate_single_session_update_lifecycle(session_id, true, false, false).is_err());
        assert!(validate_single_session_update_lifecycle(session_id, false, true, true).is_err());
    }
}
