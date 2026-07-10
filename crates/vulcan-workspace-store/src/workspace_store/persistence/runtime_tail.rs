use super::*;

/// Mirrors a runtime-state checkpoint into the segmented session state file without touching append-only JSONL segments.
/// 将运行时状态检查点镜像到分段会话状态文件，且不触碰追加式 JSONL 分段。
pub(super) async fn persist_segmented_session_runtime_state_checkpoint(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
    runtime_state: &SessionRuntimeStateEntry,
) -> Result<(), String> {
    let session_id_text = session_id.to_string();
    let Some(session_dir) =
        find_segmented_session_directory_by_id(&controller.data_root, &session_id_text)?
    else {
        return Ok(());
    };
    let state_document = state_document_from_runtime_state(&session_id_text, runtime_state)?;
    write_state(&session_dir, &state_document)?;
    sync_segmented_runtime_log_for_state(&session_dir, runtime_state)?;
    Ok(())
}

/// Syncs one segmented runtime log using the active compaction-aware persistence policy.
/// 使用当前压缩感知持久化策略同步单个分段运行时日志。
///
/// # Parameters
/// # 参数
/// `session_dir` is the segmented session directory that owns the runtime files.
/// `session_dir` 是拥有运行时文件的分段会话目录。
/// `runtime_state` is the canonical runtime state to mirror into provider-facing JSONL.
/// `runtime_state` 是要镜像到 Provider 可见 JSONL 的规范运行态。
///
/// # Returns
/// # 返回值
/// Returns `Ok(())` after the runtime log is synchronized.
/// 运行时日志同步完成后返回 `Ok(())`。
pub(super) fn sync_segmented_runtime_log_for_state(
    session_dir: &std::path::Path,
    runtime_state: &SessionRuntimeStateEntry,
) -> Result<(), String> {
    if let Some(compaction_id) = runtime_state.last_compaction_id.as_deref() {
        sync_runtime_log_from_compacted_conversation_state(
            session_dir,
            &runtime_state.conversation_state,
            &runtime_state.updated_at,
            compaction_id,
        )?;
    } else {
        sync_runtime_log_from_conversation_state(
            session_dir,
            &runtime_state.conversation_state,
            &runtime_state.updated_at,
        )?;
    }
    Ok(())
}

/// Persists one complete session snapshot into segmented transcript and runtime-state files.
/// 将一份完整会话快照持久化到分段转录与运行态文件。
///
/// # Parameters
/// # 参数
/// `controller` owns the segmented data root used for session directory resolution.
/// `controller` 持有用于解析会话目录的分段数据根。
/// `session` supplies the authoritative session identity and timestamps.
/// `session` 提供权威会话身份与时间戳。
/// `project_id` identifies the session owner used by the filesystem locator.
/// `project_id` 标识文件系统定位器使用的会话归属项目。
/// `archived` selects the active or archived segmented directory tree.
/// `archived` 选择活动或归档分段目录树。
/// `messages` is the complete transcript snapshot to append or physically reconcile.
/// `messages` 是需要追加或物理校正的完整转录快照。
/// `runtime_state` optionally supplies the runtime checkpoint and provider-facing log state.
/// `runtime_state` 可选提供运行态检查点与 Provider 可见日志状态。
///
/// # Returns
/// # 返回值
/// Returns success only after transcript reconciliation and every supplied runtime artifact are durable.
/// 仅在转录校正与全部已提供运行态工件均完成耐久化后返回成功。
pub(super) async fn persist_segmented_session_update(
    controller: &ControllerRuntimeHandle,
    session: &SessionSummary,
    project_id: &str,
    archived: bool,
    messages: &[SessionMessageEntry],
    runtime_state: Option<&SessionRuntimeStateEntry>,
) -> Result<(), String> {
    // locator is the single filesystem identity derived from the authoritative session owner and lifecycle state.
    // locator 是从权威会话归属与生命周期状态派生的唯一文件系统身份。
    let locator = segmented_session_locator_from_summary(session, project_id, archived);
    // session_id_text is the canonical textual session identity stored in segmented metadata and state files.
    // session_id_text 是写入分段元数据与状态文件的规范会话文本身份。
    let session_id_text = session.id.to_string();
    // session_dir is created or resolved before transcript reconciliation begins.
    // session_dir 会在转录校正开始前完成创建或解析。
    let session_dir =
        ensure_segmented_session_directory_for_locator(controller, &session_id_text, &locator)?;
    append_segmented_session_message_update(&session_dir, messages)?;
    if let Some(runtime_state) = runtime_state {
        // state_document is the exact overwrite snapshot paired with the reconciled transcript.
        // state_document 是与已校正转录配套的精确覆盖式快照。
        let state_document = state_document_from_runtime_state(&session_id_text, runtime_state)?;
        write_state(&session_dir, &state_document)?;
        sync_segmented_runtime_log_for_state(&session_dir, runtime_state)?;
    }
    Ok(())
}
