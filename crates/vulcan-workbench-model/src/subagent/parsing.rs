use super::*;

/// Parses one persisted task-session payload object into a cacheable subagent task panel state.
/// 将一份持久化 task 会话载荷对象解析为可缓存的子代理任务面板状态。
pub fn parse_subagent_task_panel_payload(
    parent_session_id: SessionId,
    payload: &Value,
    prompt_preview: Option<&str>,
) -> Option<SubagentTaskPanelState> {
    let task_id = task_session_internal_id_from_payload(payload)?;
    let run_state = subagent_run_state_from_execution_status(
        payload.get("execution_status").and_then(Value::as_str),
    );
    let description = payload
        .get("description")
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|value| !value.trim().is_empty());
    let public_task_id = task_session_public_id_from_payload(payload)
        .map(str::to_string)
        .unwrap_or_else(|| task_id.to_string());
    let subagent_label = task_session_tool_display_label(
        task_session_display_title_from_payload(payload),
        description.as_deref(),
        payload.get("subagent").and_then(Value::as_str),
        Some(public_task_id.as_str()),
        prompt_preview,
    );

    Some(SubagentTaskPanelState {
        task_id,
        public_task_id,
        parent_session_id,
        subagent_label,
        description,
        run_state,
        result_text: payload
            .get("result")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_default(),
        // Tool-card payload snapshots are intentionally not treated as reviewable TaskSession transcripts; full child context must come from the segmented TaskSession store.
        // 工具卡片载荷快照会被有意排除在可复看 TaskSession 转录之外；完整子会话上下文必须来自分段 TaskSession 存储。
        conversation_state: None,
    })
}

/// Parses one merged task-session metadata object into a cacheable subagent task panel state.
/// 将一份合并后的 task 会话元数据对象解析为可缓存的子代理任务面板状态。
pub fn parse_subagent_task_panel_metadata(
    parent_session_id: SessionId,
    metadata: &Value,
    prompt_preview: Option<&str>,
) -> Option<SubagentTaskPanelState> {
    let payload = task_session_payload_from_tool_metadata(metadata)?;
    parse_subagent_task_panel_payload(parent_session_id, payload, prompt_preview)
}

/// Extracts the task prompt text from one merged task-session metadata object when the host payload keeps it.
/// 当宿主载荷保留了任务提示词时，从一份合并后的 task 会话元数据对象中提取该提示词文本。
pub fn parse_subagent_task_prompt_metadata(metadata: &Value) -> Option<String> {
    task_session_payload_from_tool_metadata(metadata)?
        .get("prompt")
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|value| !value.trim().is_empty())
}
