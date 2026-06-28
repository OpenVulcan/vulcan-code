//! Runtime-state loading and recovery helpers for session hydration.
//! 会话水合的运行态加载与恢复辅助逻辑。

use super::*;
use crate::conversation_protocol::conversation_state_to_session_messages;
/// Loads the overwrite-only runtime state from the segmented session state file when available.
/// 可用时从分段会话状态文件加载覆盖式运行态。
pub(super) fn load_segmented_runtime_state(
    controller: &ControllerRuntimeHandle,
    locator: Option<&StoredSessionLocatorRow>,
    session_id: SessionId,
) -> Result<Option<SessionRuntimeStateEntry>, String> {
    let Some(locator) = locator else {
        return Ok(None);
    };
    let Some(session_dir) =
        find_segmented_session_dir_for_locator(controller, locator, session_id)?
    else {
        return Ok(None);
    };
    let runtime_conversation_state = ensure_runtime_log_from_segmented_context(&session_dir)?;
    let Some(state_document) = read_state(&session_dir)? else {
        let mut runtime_state = runtime_state_from_runtime_or_segmented_replay(
            session_id,
            runtime_conversation_state,
            &session_dir,
        )?;
        let _ =
            apply_session_addon_state_to_loaded_runtime_state(&session_dir, &mut runtime_state)?;
        return Ok(runtime_state);
    };
    let has_runtime_state = !state_document.runtime_state.is_null();
    if !has_runtime_state && state_document.current_context.is_null() {
        let mut runtime_state = runtime_state_from_runtime_or_segmented_replay(
            session_id,
            runtime_conversation_state,
            &session_dir,
        )?;
        let _ =
            apply_session_addon_state_to_loaded_runtime_state(&session_dir, &mut runtime_state)?;
        return Ok(runtime_state);
    }
    let mut runtime_state = if has_runtime_state {
        serde_json::from_value::<SessionRuntimeStateEntry>(state_document.runtime_state)
            .map_err(|error| format!("解析分段会话状态文件失败：{error}"))?
    } else {
        SessionRuntimeStateEntry::default()
    };
    let applied_context =
        apply_segmented_current_context(&mut runtime_state, &state_document.current_context)?;
    if runtime_state.updated_at.trim().is_empty() && !state_document.updated_at.trim().is_empty() {
        runtime_state.updated_at = state_document.updated_at;
    }
    if let Some(conversation_state) = runtime_conversation_state {
        runtime_state.conversation_state = conversation_state;
    }
    let mut runtime_state = Some(runtime_state);
    let addon_applied =
        apply_session_addon_state_to_loaded_runtime_state(&session_dir, &mut runtime_state)?;
    let Some(mut runtime_state) = runtime_state else {
        return Ok(None);
    };
    if !has_runtime_state
        && !applied_context
        && !addon_applied
        && runtime_state.conversation_state.messages.is_empty()
        && runtime_state.conversation_state.records.is_empty()
    {
        return Ok(None);
    }
    let _ =
        repair_duplicate_conversation_record_ids(session_id, &mut runtime_state.conversation_state);
    let _ = sanitize_loaded_runtime_state_for_fresh_process(&mut runtime_state);
    Ok(Some(runtime_state))
}

/// Applies addon state to an optional loaded runtime state.
/// 将附属状态应用到一个可选的已加载运行态。
///
/// # Parameters
/// 参数
/// `session_dir` is the resolved segmented session directory.
/// `session_dir` 是已解析的分段会话目录。
/// `runtime_state` is the optional runtime state that receives addon-owned fields.
/// `runtime_state` 是接收附属状态字段的可选运行态。
///
/// # Returns
/// 返回值
/// `true` when an addon document existed and was applied.
/// 当附属状态文档存在且已应用时返回 `true`。
fn apply_session_addon_state_to_loaded_runtime_state(
    session_dir: &std::path::Path,
    runtime_state: &mut Option<SessionRuntimeStateEntry>,
) -> Result<bool, String> {
    let Some(addon_state) = read_session_addon_state(session_dir)? else {
        return Ok(false);
    };
    addon_state.apply_to_runtime_state(
        runtime_state.get_or_insert_with(SessionRuntimeStateEntry::default),
    );
    Ok(true)
}

/// Rebuilds runtime state from runtime JSONL when available, otherwise falls back to segmented replay.
/// 优先从 runtime JSONL 重建运行态，不可用时回退到分段展示重放。
fn runtime_state_from_runtime_or_segmented_replay(
    session_id: SessionId,
    runtime_conversation_state: Option<SessionConversationState>,
    session_dir: &std::path::Path,
) -> Result<Option<SessionRuntimeStateEntry>, String> {
    if let Some(conversation_state) = runtime_conversation_state {
        let mut runtime_state = SessionRuntimeStateEntry {
            conversation_state,
            updated_at: current_unix_millis_text(),
            ..SessionRuntimeStateEntry::default()
        };
        let _ = repair_duplicate_conversation_record_ids(
            session_id,
            &mut runtime_state.conversation_state,
        );
        return Ok(Some(runtime_state));
    }
    runtime_state_from_segmented_replay(session_id, session_dir)
}

/// Rebuilds runtime state from segmented JSONL replay when state.json has no usable context.
/// 当 state.json 没有可用上下文时，通过分段 JSONL 重放重建运行态。
fn runtime_state_from_segmented_replay(
    session_id: SessionId,
    session_dir: &std::path::Path,
) -> Result<Option<SessionRuntimeStateEntry>, String> {
    let _ = repair_segmented_session_display_if_needed(session_dir)?;
    let Some(replay) = replay_segmented_session_context(session_dir)? else {
        return Ok(None);
    };
    let mut runtime_state = runtime_state_entry_from_segmented_replay(replay);
    let _ =
        repair_duplicate_conversation_record_ids(session_id, &mut runtime_state.conversation_state);
    Ok(Some(runtime_state))
}

/// Converts a segmented replay result into the existing runtime-state model.
/// 将分段重放结果转换为现有运行态模型。
pub(super) fn runtime_state_entry_from_segmented_replay(
    replay: SegmentedSessionReplayResult,
) -> SessionRuntimeStateEntry {
    let mut runtime_state = SessionRuntimeStateEntry::default();
    runtime_state.conversation_state = replay.conversation_state;
    runtime_state.last_compaction_id = replay.last_compaction_id;
    runtime_state.context_start_message_id = replay.context_start_message_id;
    if let Some(updated_at) = replay.updated_at {
        runtime_state.updated_at = updated_at;
    }
    runtime_state
}

/// Applies a segmented current-context document onto one runtime-state entry.
/// 将分段当前上下文文档应用到一条运行态记录上。
pub(super) fn apply_segmented_current_context(
    runtime_state: &mut SessionRuntimeStateEntry,
    current_context: &serde_json::Value,
) -> Result<bool, String> {
    let Some(context_object) = current_context.as_object() else {
        return Ok(false);
    };
    let mut applied = false;
    if let Some(conversation_state_value) = context_object
        .get("conversation_state")
        .filter(|value| !value.is_null())
    {
        runtime_state.conversation_state =
            serde_json::from_value::<SessionConversationState>(conversation_state_value.clone())
                .map_err(|error| format!("解析分段当前上下文会话状态失败：{error}"))?;
        applied = true;
    }
    if let Some(value) =
        segmented_current_context_optional_string(current_context, "last_compaction_id")
    {
        runtime_state.last_compaction_id = value;
        applied = true;
    }
    if let Some(value) =
        segmented_current_context_optional_string(current_context, "context_start_message_id")
    {
        runtime_state.context_start_message_id = value;
        applied = true;
    }
    if let Some(value) = segmented_current_context_optional_string(
        current_context,
        "live_context_budget_fingerprint",
    ) {
        runtime_state.live_context_budget_fingerprint = value;
        applied = true;
    }
    if let Some(value) = segmented_current_context_optional_string(
        current_context,
        "live_context_assembly_fingerprint",
    ) {
        runtime_state.live_context_assembly_fingerprint = value;
        applied = true;
    }
    if let Some(value) = segmented_current_context_optional_string(
        current_context,
        "anchor_context_budget_fingerprint",
    ) {
        runtime_state.anchor_context_budget_fingerprint = value;
        applied = true;
    }
    if let Some(value) = segmented_current_context_optional_string(
        current_context,
        "anchor_context_assembly_fingerprint",
    ) {
        runtime_state.anchor_context_assembly_fingerprint = value;
        applied = true;
    }
    if let Some(value) = segmented_current_context_optional_string(
        current_context,
        "last_provider_request_preview_markdown",
    ) {
        runtime_state.last_provider_request_preview_markdown = value;
        applied = true;
    }
    if let Some(updated_at) = segmented_current_context_string(current_context, "updated_at") {
        runtime_state.updated_at = updated_at;
        applied = true;
    }
    Ok(applied)
}

/// Extracts one nullable string field from a segmented current-context value.
/// 从分段当前上下文值中提取一个可空字符串字段。
fn segmented_current_context_optional_string(
    current_context: &serde_json::Value,
    key: &str,
) -> Option<Option<String>> {
    let value = current_context.get(key)?;
    if value.is_null() {
        return Some(None);
    }
    Some(segmented_json_string(value))
}

/// Extracts one non-empty string field from a segmented current-context value.
/// 从分段当前上下文值中提取一个非空字符串字段。
fn segmented_current_context_string(
    current_context: &serde_json::Value,
    key: &str,
) -> Option<String> {
    current_context.get(key).and_then(segmented_json_string)
}

/// Converts one JSON value into a trimmed string when possible.
/// 尽可能将一个 JSON 值转换为去除首尾空白后的字符串。
fn segmented_json_string(value: &serde_json::Value) -> Option<String> {
    value
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

/// 通过分段检查点和规范事件流为单个会话恢复水合后的运行时状态。
pub(super) async fn hydrate_session_runtime_state(
    session_id: SessionId,
    runtime_state_row: Option<StoredRuntimeStateRow>,
    full_messages: &[SessionMessageEntry],
) -> Result<Option<SessionRuntimeStateEntry>, String> {
    let Some(mut runtime_state) = runtime_state_row.map(stored_runtime_state_row_to_entry) else {
        if full_messages.is_empty() {
            return Ok(None);
        }
        let mut runtime_state = SessionRuntimeStateEntry::default();
        runtime_state.conversation_state = conversation_state_from_session_messages(full_messages);
        runtime_state.updated_at = current_unix_millis_text();
        let _ = repair_duplicate_conversation_record_ids(
            session_id,
            &mut runtime_state.conversation_state,
        );
        return Ok(Some(runtime_state));
    };
    if runtime_state.updated_at.trim().is_empty() {
        runtime_state.updated_at = current_unix_millis_text();
    }
    if runtime_state.conversation_state.records.is_empty() && !full_messages.is_empty() {
        runtime_state.conversation_state = conversation_state_from_session_messages(full_messages);
    }
    let _ =
        repair_duplicate_conversation_record_ids(session_id, &mut runtime_state.conversation_state);

    Ok(Some(runtime_state))
}

/// Rebuilds the visible hydration transcript from runtime state when the persisted turn window is unexpectedly empty.
/// 当持久化 turn 窗口意外为空时，根据运行时状态回建可见的水合转录窗口。
pub(super) fn restore_hydration_window_from_runtime_state_if_empty(
    messages: &mut Vec<SessionMessageEntry>,
    tool_detail_messages: &mut Vec<SessionMessageEntry>,
    runtime_state: Option<&SessionRuntimeStateEntry>,
) {
    if !messages.is_empty() {
        return;
    }
    let Some(runtime_state) = runtime_state else {
        return;
    };
    let rebuilt_messages =
        conversation_state_to_session_messages(&runtime_state.conversation_state);
    if rebuilt_messages.is_empty() {
        return;
    }
    *tool_detail_messages = rebuilt_messages
        .iter()
        .filter(|message| {
            matches!(
                message.record_kind,
                SessionMessageRecordKind::ToolCallOutput
                    | SessionMessageRecordKind::ToolResultOutput
            )
        })
        .cloned()
        .collect();
    *messages = rebuilt_messages;
}

/// Resolves the startup hydration turn target by taking the larger value between the configured page size and the overlap required to expose five turns before the active compaction boundary.
/// 通过取“配置分页大小”和“为暴露活动压缩边界前五轮所需的重叠范围”中的较大值，解析启动水合轮次目标。
pub(super) fn resolve_initial_hydration_turn_count(
    requested_turn_count: usize,
    compactions: &[SessionCompactionEntry],
    runtime_state_row: Option<&StoredRuntimeStateRow>,
    locator: Option<&StoredSessionLocatorRow>,
) -> usize {
    let mut effective_turn_count = requested_turn_count.max(1);
    let Some(boundary_turn_index) =
        active_compaction_overlap_boundary_turn_index(compactions, runtime_state_row)
    else {
        return effective_turn_count;
    };
    let Some(latest_turn_index) = locator
        .map(|row| row.latest_turn_index)
        .filter(|value| *value >= 0)
    else {
        return effective_turn_count;
    };
    let overlap_turn_count = SESSION_TRANSCRIPT_COMPACTION_OVERLAP_TURN_COUNT as i64;
    let earliest_required_turn_index = boundary_turn_index
        .saturating_sub(overlap_turn_count)
        .max(0);
    let turns_needed = latest_turn_index
        .saturating_sub(earliest_required_turn_index)
        .saturating_add(1) as usize;
    effective_turn_count = effective_turn_count.max(turns_needed);
    effective_turn_count
}

/// Resolves the active compaction overlap boundary from persisted runtime-state metadata and checkpoint rows.
/// 根据持久化运行时状态元数据与检查点行解析当前活动压缩重叠边界。
fn active_compaction_overlap_boundary_turn_index(
    compactions: &[SessionCompactionEntry],
    runtime_state_row: Option<&StoredRuntimeStateRow>,
) -> Option<i64> {
    let active_compaction = runtime_state_row
        .and_then(|row| row.last_compaction_id.as_deref())
        .and_then(|compaction_id| compactions.iter().find(|entry| entry.id == compaction_id))
        .or_else(|| compactions.last())?;

    active_compaction
        .retained_tail_start_turn_index
        .or_else(|| {
            active_compaction
                .source_until_turn_index
                .map(|turn_index| turn_index + 1)
        })
}
