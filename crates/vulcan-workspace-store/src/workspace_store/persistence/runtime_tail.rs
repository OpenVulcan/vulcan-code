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

/// Mirrors one full session update into the segmented append-only stream and overwrite state file.
/// 将一次完整会话更新镜像到分段追加流与覆盖式状态文件。
pub(super) async fn persist_segmented_session_tail_update(
    controller: &ControllerRuntimeHandle,
    session: &SessionSummary,
    project_id: &str,
    archived: bool,
    messages: &[SessionMessageEntry],
    runtime_state: Option<&SessionRuntimeStateEntry>,
) -> Result<(), String> {
    let locator = segmented_session_locator_from_summary(session, project_id, archived);
    let session_id_text = session.id.to_string();
    let session_dir =
        ensure_segmented_session_directory_for_locator(controller, &session_id_text, &locator)?;
    let transcript_tail_error = match append_segmented_session_message_tail(&session_dir, messages)
    {
        Ok(_) => None,
        Err(error) if error.contains("non-tail transcript change") => Some(error),
        Err(error) => return Err(error),
    };
    if let Some(runtime_state) = runtime_state {
        let state_document = state_document_from_runtime_state(&session_id_text, runtime_state)?;
        write_state(&session_dir, &state_document)?;
        sync_segmented_runtime_log_for_state(&session_dir, runtime_state)?;
    }
    if let Some(error) = transcript_tail_error {
        return Err(error);
    }
    Ok(())
}
