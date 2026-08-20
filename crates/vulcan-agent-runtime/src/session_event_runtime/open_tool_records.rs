use super::*;

/// Closes persisted display-window request cards that no longer have a live runtime owner.
/// 闭合已持久化到显示窗口但不再具有实时运行时所有者的请求阶段工具卡片。
///
/// # Parameters
/// 参数
/// `messages` contains the persisted transcript rows restored for the visible session window.
/// `messages` 包含为可见会话窗口恢复的持久化转录行。
///
/// # Returns
/// 返回值
/// The number of request-stage cards changed to the stopped terminal state.
/// 被改为已停止终态的请求阶段卡片数量。
pub fn normalize_orphaned_tool_request_messages(messages: &mut [SessionMessageEntry]) -> usize {
    let mut normalized_count = 0;
    for message in messages {
        if message.record_kind != SessionMessageRecordKind::ToolCallOutput {
            continue;
        }
        if normalize_orphaned_tool_request_metadata(message.metadata.as_mut()) {
            normalized_count += 1;
        }
    }
    normalized_count
}

/// Closes persisted request-stage tool cards that no longer have a live runtime owner.
/// 闭合已持久化但不再具有实时运行时所有者的请求阶段工具卡片。
///
/// # Parameters
/// 参数
/// `runtime_state` owns the persisted request cards and their correlation maps.
/// `runtime_state` 持有已持久化的请求卡片及其关联映射。
/// `now` is the hydration timestamp recorded when orphaned cards are repaired.
/// `now` 是修复孤儿卡片时记录的水合时间戳。
///
/// # Returns
/// 返回值
/// The number of request-stage cards changed to the stopped terminal state.
/// 被改为已停止终态的请求阶段卡片数量。
pub fn normalize_orphaned_tool_request_records(
    runtime_state: &mut SessionRuntimeStateEntry,
    now: &str,
) -> usize {
    // orphanedRecordIds contains only cards whose persisted display state proves execution never started.
    // orphanedRecordIds 仅包含持久化展示状态能够证明执行尚未开始的卡片。
    let mut orphaned_record_ids = BTreeSet::new();
    for record in &mut runtime_state.conversation_state.records {
        if record.record_kind != SessionMessageRecordKind::ToolCallOutput {
            continue;
        }
        if !normalize_orphaned_tool_request_metadata(record.metadata.as_mut()) {
            continue;
        }
        orphaned_record_ids.insert(record.id.clone());
    }
    if orphaned_record_ids.is_empty() {
        return 0;
    }
    // Both correlation maps are released by record identity so temporary and canonical aliases cannot revive.
    // 两张关联表都会按记录身份释放，确保临时别名与规范别名都无法复活。
    runtime_state
        .tool_request_record_ids
        .retain(|_, record_id| !orphaned_record_ids.contains(record_id));
    runtime_state
        .tool_call_record_ids
        .retain(|_, record_id| !orphaned_record_ids.contains(record_id));
    runtime_state.updated_at = now.to_string();
    orphaned_record_ids.len()
}

/// Supersedes exact pre-execution request cards owned by a failed Provider attempt.
/// 将失败 Provider 尝试精确持有的执行前请求卡片标记为已被取代。
///
/// # Parameters
/// 参数
/// `runtime_state` owns the open request correlations and retained tool cards.
/// `runtime_state` 持有未闭合请求关联与保留工具卡片。
/// `now` is the retry-boundary timestamp written after a successful transition.
/// `now` 是成功转换后写入的重试边界时间戳。
/// `reason` is the stable terminal reason attached to every superseded request.
/// `reason` 是附加到每条已被取代请求的稳定终止原因。
///
/// # Returns
/// 返回值
/// The number of exact open request cards changed to the superseded state.
/// 被改为已取代状态的精确未闭合请求卡片数量。
pub(super) fn supersede_open_tool_request_records(
    runtime_state: &mut SessionRuntimeStateEntry,
    now: &str,
    reason: &str,
) -> usize {
    // requestRecordIds selects only request formation that has not crossed ToolStarted.
    // requestRecordIds 仅选择尚未跨过 ToolStarted 的请求构造记录。
    let request_record_ids = std::mem::take(&mut runtime_state.tool_request_record_ids)
        .into_values()
        .collect::<BTreeSet<_>>();
    if request_record_ids.is_empty() {
        return 0;
    }
    let mut superseded_record_ids = BTreeSet::new();
    for record in &mut runtime_state.conversation_state.records {
        if !request_record_ids.contains(&record.id)
            || record.record_kind != SessionMessageRecordKind::ToolCallOutput
        {
            continue;
        }
        let Some(serde_json::Value::Object(metadata)) = record.metadata.as_mut() else {
            continue;
        };
        metadata.insert(
            String::from("tool_display_state"),
            serde_json::Value::String(String::from("superseded")),
        );
        metadata.insert(
            String::from("request_terminal_reason"),
            serde_json::Value::String(reason.to_string()),
        );
        superseded_record_ids.insert(record.id.clone());
    }
    // Canonical call aliases are released by record identity so a later retry cannot attach to the old card.
    // 规范调用别名按记录身份释放，确保后续重试无法挂接到旧卡片。
    runtime_state
        .tool_call_record_ids
        .retain(|_, record_id| !request_record_ids.contains(record_id));
    if !superseded_record_ids.is_empty() {
        runtime_state.updated_at = now.to_string();
    }
    superseded_record_ids.len()
}

/// Converts request-stage metadata into the stopped state used after ownerless hydration.
/// 将请求阶段元数据转换为无所有者水合后使用的已停止状态。
///
/// # Parameters
/// 参数
/// `metadata` is the optional transcript metadata attached to one candidate tool card.
/// `metadata` 是附着于一张候选工具卡片的可选转录元数据。
///
/// # Returns
/// 返回值
/// `true` when request-stage metadata was recognized and normalized; otherwise `false`.
/// 识别并归一化请求阶段元数据时返回 `true`，否则返回 `false`。
fn normalize_orphaned_tool_request_metadata(metadata: Option<&mut serde_json::Value>) -> bool {
    let Some(serde_json::Value::Object(metadata)) = metadata else {
        return false;
    };
    let is_request_stage = metadata
        .get("tool_display_state")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|state| matches!(state, "receiving" | "ready"));
    if !is_request_stage {
        return false;
    }
    metadata.insert(
        String::from("tool_display_state"),
        serde_json::Value::String(String::from("stopped")),
    );
    metadata.insert(
        String::from("request_terminal_reason"),
        serde_json::Value::String(String::from("runtime_owner_missing_after_hydration")),
    );
    true
}

pub(super) fn complete_open_tool_records(
    runtime_state: &mut SessionRuntimeStateEntry,
    session_id: SessionId,
    now: &str,
    display_state: &str,
    message: &str,
) -> Result<(), String> {
    // requestRecordIds are cleared together because terminal synthesis closes every pre-execution request.
    // requestRecordIds 会一并清空，因为终态合成会闭合所有执行前请求。
    runtime_state.tool_request_record_ids.clear();
    let open_records = std::mem::take(&mut runtime_state.tool_call_record_ids);
    for (tool_call_id, record_id) in open_records {
        let tool_name = runtime_state
            .conversation_state
            .records
            .iter()
            .find(|record| record.id == record_id)
            .and_then(|record| tool_record_name_for_call_id(record, &tool_call_id));
        let detail_ref = append_tool_result_detail_record(
            runtime_state,
            session_id,
            now,
            &tool_call_id,
            tool_name.as_deref(),
            display_state,
            None,
            message,
        )?;
        let metadata = normalize_tool_result_metadata(
            &tool_call_id,
            tool_name.as_deref(),
            display_state,
            None,
            detail_ref.as_ref(),
            true,
        );
        attach_tool_result_to_record(&mut runtime_state.conversation_state, &record_id, metadata);
    }
    Ok(())
}

/// Finds the tool name stored on an earlier tool-call card for the same call identifier.
/// 查找同一调用标识早前工具调用卡片中保存的工具名称。
pub(super) fn tool_record_name_for_call_id(
    record: &SessionConversationRecord,
    tool_call_id: &str,
) -> Option<String> {
    if record.record_kind != SessionMessageRecordKind::ToolCallOutput {
        return None;
    }
    let metadata = record.metadata.as_ref()?;
    let record_call_id = metadata
        .get("tool_call_id")
        .and_then(serde_json::Value::as_str)?;
    if record_call_id != tool_call_id {
        return None;
    }
    metadata
        .get("tool_name")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
}
