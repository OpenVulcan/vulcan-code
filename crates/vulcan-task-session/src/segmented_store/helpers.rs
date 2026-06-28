use super::*;

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
    serde_json::to_value(kind)
        .ok()
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| "runtime_marker".to_string())
}
