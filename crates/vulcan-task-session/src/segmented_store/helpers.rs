use super::*;

/// Returns the task-specific metadata path inside one task directory.
/// 返回单个子任务目录内的子任务专属元数据路径。
pub(super) fn task_metadata_path(task_dir: &Path) -> PathBuf {
    task_dir.join(SEGMENTED_TASK_METADATA_FILE)
}

/// Returns the main-session compatible metadata path inside one task directory.
/// 返回单个子任务目录内的主会话兼容元数据路径。
pub(super) fn task_main_metadata_path(task_dir: &Path) -> PathBuf {
    task_dir.join(SEGMENTED_METADATA_FILE)
}

/// Reads the root main-session metadata as raw JSON when it exists.
/// 在根主会话元数据存在时以原始 JSON 读取。
pub(super) fn read_root_metadata_value(
    root_session_dir: &Path,
) -> Result<Option<Value>, SessionStoreError> {
    read_json_file(&root_session_dir.join(SEGMENTED_METADATA_FILE))
}

/// Reads the main-compatible segment metadata for one task directory.
/// 读取单个子任务目录的主会话兼容分段元数据。
pub(super) fn read_task_main_segment_metadata(
    task_dir: &Path,
) -> Result<SegmentedTaskSessionMainMetadataDocument, SessionStoreError> {
    read_required_task_main_metadata_document(&task_main_metadata_path(task_dir))
}

/// Copies segment counters from the main-compatible metadata into task-specific metadata.
/// 将主会话兼容元数据中的分段计数同步到子任务专属元数据。
pub(super) fn sync_task_metadata_segments_from_main(
    task_metadata: &mut SegmentedTaskMetadataDocument,
    main_metadata: &SegmentedTaskSessionMainMetadataDocument,
) {
    task_metadata.active_segment = main_metadata.active_segment.clone();
    task_metadata.segments = main_metadata.segments.clone();
    task_metadata.next_seq = main_metadata.next_global_seq;
    task_metadata.segment_policy = main_metadata.segment_policy.clone();
}

/// Builds the main-session runtime-state payload from one task context document.
/// 根据单个子任务上下文文档构建主会话运行态载荷。
pub(super) fn task_main_runtime_state_from_context(
    context: &SessionContextDocument,
    updated_at: &str,
) -> Value {
    json!({
        "active_provider_id": context.provider_id.clone(),
        "active_model_id": context.model_id.clone(),
        "updated_at": updated_at
    })
}

/// Builds the main-session current-context payload from one task context document.
/// 根据单个子任务上下文文档构建主会话当前上下文载荷。
pub(super) fn task_main_current_context_from_context(
    context: &SessionContextDocument,
    updated_at: &str,
) -> Value {
    json!({
        "conversation_state": context.conversation_state.clone(),
        "last_compaction_id": context.compaction_anchor.clone(),
        "context_start_message_id": context.compaction_anchor.clone(),
        "anchor_context_budget_fingerprint": context.context_budget_fingerprint.clone(),
        "updated_at": updated_at
    })
}

/// Reads one task-state document from a task directory.
/// 从子任务目录读取状态文档。
pub(super) fn read_task_state(
    task_dir: &Path,
) -> Result<Option<SegmentedTaskStateDocument>, SessionStoreError> {
    read_json_file(&task_dir.join(SEGMENTED_STATE_FILE))
}

/// Builds an agent kind label for task metadata.
/// 为子任务元数据构建代理类型标签。
pub(super) fn segmented_agent_kind(metadata: &TaskSessionMetadata) -> String {
    if metadata.subagent_profile_id.is_some() {
        "agent_md".to_string()
    } else if metadata.inherit_parent_config {
        "main_clone".to_string()
    } else {
        "custom".to_string()
    }
}

/// Builds an agent profile audit payload for task metadata.
/// 为子任务元数据构建代理配置审计载荷。
pub(super) fn segmented_agent_profile(metadata: &TaskSessionMetadata) -> Value {
    json!({
        "agent_id": metadata.subagent_profile_id,
        "agent_name": metadata.display_title,
        "agent_md_path": metadata.effective_config.subagent_profile_id,
        "prompt_source": segmented_agent_kind(metadata),
        "inherit_parent_config": metadata.inherit_parent_config
    })
}

/// Converts a shared session event kind into a stable segmented kind label.
/// 将共享会话事件类型转换为稳定的分段类型标签。
pub(super) fn session_event_kind_string(kind: &vulcan_session_core::SessionEventKind) -> String {
    match kind {
        vulcan_session_core::SessionEventKind::UserPrompt => "user_prompt".to_string(),
        vulcan_session_core::SessionEventKind::AssistantOutput => "assistant_output".to_string(),
        _ => "display_event".to_string(),
    }
}
