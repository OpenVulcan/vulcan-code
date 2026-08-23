use super::*;
use crate::workspace_store::segmented_session_store::{
    segmented_message_fingerprint, stable_segmented_message_id,
};

/// Persists one session and its runtime artifacts without rewriting unrelated workspace sessions.
/// 仅持久化单个会话及其运行时工件，不重写无关工作区会话。
///
/// # Parameters
/// 参数
/// `controller` owns the authoritative SQLite binding and segmented data root.
/// `controller` 持有权威 SQLite 绑定与分段数据根。
/// `project_id` is the exact project owner already carried by the retained session locator.
/// `project_id` 是保留会话定位信息已经携带的精确项目归属。
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
    project_id: &str,
    session: &SessionSummary,
    archived: bool,
    messages: &[SessionMessageEntry],
    runtime_state: Option<&SessionRuntimeStateEntry>,
) -> Result<(), String> {
    // retainedProjectId is the only accepted owner; ordinary persistence never scans every project row.
    // retainedProjectId 是唯一接受的归属；普通持久化绝不扫描全部项目行。
    let retained_project_id = session
        .project_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("单会话保存失败：会话未携带所属项目：{}", session.id))?;
    if project_id.trim().is_empty() || project_id != retained_project_id {
        return Err(format!(
            "单会话保存失败：会话 `{}` 的精确项目归属不匹配。",
            session.id,
        ));
    }
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
        retained_project_id,
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
        retained_project_id,
        archived,
        0,
        runtime_state.map(|state| state.updated_at.as_str()),
    )
    .await
}

/// Persists only the exact existing session index row without touching transcript files.
/// 仅持久化精确的既有会话索引行，不触碰转录文件。
///
/// # Parameters
/// 参数
/// `controller` owns the workspace database connection and exact locator query route.
/// `controller` 持有工作区数据库连接与精确定位查询路径。
/// `project_id` is the retained authoritative project owner.
/// `project_id` 是保留态中的权威项目归属。
/// `session` is the updated session summary.
/// `session` 是更新后的会话摘要。
/// `archived` selects the already-existing lifecycle locator.
/// `archived` 选择已经存在的生命周期定位行。
///
/// # Returns
/// 返回值
/// Success only after the exact existing locator has been validated and updated.
/// 仅在精确既有定位行完成校验与更新后返回成功。
pub async fn persist_existing_session_index_update(
    controller: ControllerRuntimeHandle,
    project_id: &str,
    session: &SessionSummary,
    archived: bool,
) -> Result<(), String> {
    let retained_project_id = session
        .project_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("单会话索引保存失败：会话未携带所属项目：{}", session.id))?;
    if project_id.trim().is_empty() || project_id != retained_project_id {
        return Err(format!(
            "单会话索引保存失败：会话 `{}` 的精确项目归属不匹配。",
            session.id,
        ));
    }
    let active_locator_exists = query_session_index_locator(&controller, session.id, false)
        .await?
        .is_some();
    let archived_locator_exists = query_session_index_locator(&controller, session.id, true)
        .await?
        .is_some();
    validate_single_session_update_lifecycle(
        session.id,
        archived,
        active_locator_exists,
        archived_locator_exists,
    )?;
    let selected_locator_exists = if archived {
        archived_locator_exists
    } else {
        active_locator_exists
    };
    if !selected_locator_exists {
        return Err(format!(
            "单会话索引保存失败：会话 `{}` 的精确既有定位行不存在。",
            session.id,
        ));
    }
    upsert_session_index_row(&controller, session, retained_project_id, archived, 0, None).await
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
    session_directory: PathBuf,
    session_id: SessionId,
    runtime_state: &SessionRuntimeStateEntry,
) -> Result<(), String> {
    persist_segmented_session_runtime_state_checkpoint(
        &session_directory,
        session_id,
        runtime_state,
    )
    .await?;

    Ok(())
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
pub fn persist_single_session_transcript_messages_in_directory(
    session_directory: &Path,
    session_id: SessionId,
    messages: &[SessionMessageEntry],
) -> Result<(), String> {
    if messages.is_empty() {
        return Ok(());
    }
    validate_exact_segmented_session_directory_owner(session_directory, session_id)?;
    append_segmented_session_message_update(session_directory, messages)?;
    Ok(())
}

/// Reads only the exact persisted transcript message count for one known session directory.
/// 仅为一个已知会话目录读取精确的已持久化转录消息数量。
///
/// # Parameters
/// 参数
/// `session_directory` is the exact main-session or TaskSession directory.
/// `session_directory` 是精确的主会话或 TaskSession 目录。
/// `session_id` is the expected directory owner.
/// `session_id` 是预期的目录所有者。
///
/// # Returns
/// 返回值
/// The complete metadata checkpoint without opening or scanning any transcript segment.
/// 返回完整元数据检查点，且不打开或扫描任何转录分段。
pub fn load_single_session_transcript_checkpoint_in_directory(
    session_directory: &Path,
    session_id: SessionId,
) -> Result<PersistedSessionTranscriptCheckpoint, String> {
    validate_exact_segmented_session_directory_owner(session_directory, session_id)?;
    let metadata = read_session_metadata(session_directory)?;
    Ok(PersistedSessionTranscriptCheckpoint {
        message_count: usize::try_from(metadata.message_count).map_err(to_store_error)?,
        last_message_id: metadata.last_message_id,
        last_message_fingerprint: metadata.last_message_fingerprint,
    })
}

/// Opens one resident transcript append cursor for an exact existing session directory.
/// 为一个精确的既有会话目录打开常驻转录追加游标。
///
/// # Parameters
/// 参数
/// `session_directory` is the exact deterministic main-session directory.
/// `session_directory` 是精确的确定性主会话目录。
/// `session_id` is the expected stable owner.
/// `session_id` 是预期的稳定所有者。
///
/// # Returns
/// 返回值
/// The opaque cursor and its initial durable checkpoint after one metadata read.
/// 在单次读取元数据后返回不透明游标及其初始可靠检查点。
pub fn open_single_session_transcript_append_cursor_in_directory(
    session_directory: &Path,
    session_id: SessionId,
) -> Result<
    (
        SessionTranscriptAppendCursor,
        PersistedSessionTranscriptCheckpoint,
    ),
    String,
> {
    validate_exact_segmented_session_directory_owner(session_directory, session_id)?;
    open_segmented_session_transcript_append_cursor(session_directory, session_id)
}

/// Appends one exact immutable transcript suffix without cloning or scanning the historical prefix.
/// 在不克隆或扫描历史前缀的情况下追加一个精确的不可变转录后缀。
///
/// # Parameters
/// 参数
/// `session_directory` is the exact main-session or TaskSession directory.
/// `session_directory` 是精确的主会话或 TaskSession 目录。
/// `session_id` is the expected directory owner.
/// `session_id` 是预期的目录所有者。
/// `expected_message_count` is the metadata count observed before the caller captured the suffix.
/// `expected_message_count` 是调用方捕获后缀前观察到的元数据数量。
/// `previous_message` is the exact final row of the observed prefix when non-empty.
/// `previous_message` 是非空已观察前缀的精确末行。
/// `tail_messages` contains only new immutable rows.
/// `tail_messages` 仅包含新的不可变行。
///
/// # Returns
/// 返回值
/// Success after the exact suffix and metadata checkpoint are durably appended.
/// 精确后缀与元数据检查点持久追加后返回成功。
pub fn persist_single_session_transcript_tail_in_directory(
    session_directory: &Path,
    session_id: SessionId,
    expected_message_count: usize,
    previous_message: Option<&SessionMessageEntry>,
    tail_messages: &[SessionMessageEntry],
) -> Result<(), String> {
    validate_exact_segmented_session_directory_owner(session_directory, session_id)?;
    append_segmented_session_message_tail_update(
        session_directory,
        expected_message_count,
        previous_message,
        tail_messages,
    )?;
    Ok(())
}

/// Appends one exact immutable suffix using a metadata checkpoint captured from the same session.
/// 使用从同一会话捕获的元数据检查点追加一个精确的不可变后缀。
///
/// # Parameters
/// 参数
/// `session_directory` is the exact main-session or TaskSession directory.
/// `session_directory` 是精确的主会话或 TaskSession 目录。
/// `session_id` is the expected directory owner.
/// `session_id` 是预期的目录所有者。
/// `checkpoint` is the metadata-only durable prefix token captured before suffix selection.
/// `checkpoint` 是选择后缀前捕获的纯元数据可靠前缀令牌。
/// `tail_messages` contains only new immutable rows.
/// `tail_messages` 仅包含新的不可变行。
///
/// # Returns
/// 返回值
/// Success only when the same exact metadata boundary still owns the append.
/// 仅当同一个精确元数据边界仍然持有本次追加时返回成功。
pub fn persist_single_session_transcript_tail_from_checkpoint_in_directory(
    session_directory: &Path,
    session_id: SessionId,
    checkpoint: &PersistedSessionTranscriptCheckpoint,
    tail_messages: &[SessionMessageEntry],
) -> Result<(), String> {
    validate_exact_segmented_session_directory_owner(session_directory, session_id)?;
    append_segmented_session_message_tail_from_checkpoint(
        session_directory,
        checkpoint.message_count,
        checkpoint.last_message_id.as_deref(),
        checkpoint.last_message_fingerprint.as_deref(),
        tail_messages,
    )?;
    Ok(())
}

/// Persists one immutable suffix through a resident cursor without rereading metadata.
/// 通过常驻游标持久化一个不可变后缀且不重新读取元数据。
///
/// # Parameters
/// 参数
/// `cursor` owns the exact in-memory metadata frontier.
/// `cursor` 持有精确的内存元数据前沿。
/// `checkpoint` is the immediate durable predecessor.
/// `checkpoint` 是直接可靠前驱。
/// `tail_messages` is the immutable canonical suffix.
/// `tail_messages` 是不可变规范后缀。
///
/// # Returns
/// 返回值
/// The advanced durable checkpoint after the append and metadata publication complete.
/// 在追加和元数据发布完成后返回推进后的可靠检查点。
pub fn persist_single_session_transcript_tail_with_cursor(
    cursor: &mut SessionTranscriptAppendCursor,
    checkpoint: &PersistedSessionTranscriptCheckpoint,
    tail_messages: &[SessionMessageEntry],
) -> Result<PersistedSessionTranscriptCheckpoint, String> {
    append_segmented_session_message_tail_with_cursor(cursor, checkpoint, tail_messages)
}

/// Retries one retained immutable suffix after bounded reconciliation of the exact active file tail.
/// 在有界协调精确活动文件尾部后重试一个保留的不可变后缀。
///
/// # Parameters
/// 参数
/// `cursor` is the same resident writer cursor used by the failed first attempt.
/// `cursor` 是失败的首次尝试使用的同一个常驻写入器游标。
/// `checkpoint` and `tail_messages` are the original immutable command boundary and row.
/// `checkpoint` 与 `tail_messages` 是最初的不可变命令边界与记录。
///
/// # Returns
/// 返回值
/// The exact durable checkpoint after recovering only a complete active tail or truncating one partial row.
/// 在仅恢复完整活动尾部或截断一条不完整记录后返回精确可靠检查点。
pub fn retry_single_session_transcript_tail_with_cursor(
    cursor: &mut SessionTranscriptAppendCursor,
    checkpoint: &PersistedSessionTranscriptCheckpoint,
    tail_messages: &[SessionMessageEntry],
) -> Result<PersistedSessionTranscriptCheckpoint, String> {
    retry_segmented_session_message_tail_with_cursor(cursor, checkpoint, tail_messages)
}

/// Advances one transcript checkpoint from an immutable suffix without reading storage.
/// 在不读取存储的情况下，根据不可变后缀推进转录检查点。
///
/// # Parameters
/// 参数
/// `checkpoint` is the exact durable or committed prefix before the suffix.
/// `checkpoint` 是后缀之前的精确可靠或已提交前缀。
/// `tail_messages` contains the ordered immutable suffix accepted by the writer.
/// `tail_messages` 包含写入器接纳的有序不可变后缀。
///
/// # Returns
/// 返回值
/// The deterministic checkpoint after the suffix, or an overflow or fingerprint error.
/// 返回后缀之后的确定性检查点，或溢出、指纹错误。
pub fn advance_single_session_transcript_checkpoint(
    checkpoint: &PersistedSessionTranscriptCheckpoint,
    tail_messages: &[SessionMessageEntry],
) -> Result<PersistedSessionTranscriptCheckpoint, String> {
    if tail_messages.is_empty() {
        return Ok(checkpoint.clone());
    }
    // message_count is the exact absolute coordinate after this immutable suffix.
    // message_count 是该不可变后缀之后的精确绝对坐标。
    let message_count = checkpoint
        .message_count
        .checked_add(tail_messages.len())
        .ok_or_else(|| String::from("Segmented transcript message count overflowed."))?;
    // final_message owns both deterministic last-row identity fields.
    // final_message 持有两个确定性的末行身份字段。
    let final_message = tail_messages
        .last()
        .expect("non-empty transcript tail guarantees a final message");
    let final_index = message_count - 1;
    Ok(PersistedSessionTranscriptCheckpoint {
        message_count,
        last_message_id: Some(stable_segmented_message_id(final_index, final_message)),
        last_message_fingerprint: Some(segmented_message_fingerprint(final_message)?),
    })
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
