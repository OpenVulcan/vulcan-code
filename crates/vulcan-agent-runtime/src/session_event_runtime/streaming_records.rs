use super::*;

/// Appends or extends one streaming transcript record inside the background state.
/// 在后台状态中追加或扩展一条流式转录记录。
pub fn append_stream_delta_record_to_state(
    runtime_state: &mut SessionRuntimeStateEntry,
    session_id: SessionId,
    now: &str,
    record_id_slot: &mut Option<String>,
    record_kind: SessionMessageRecordKind,
    title: &str,
    delta: &str,
    collapsed_by_default: bool,
) -> Result<(), String> {
    if delta.is_empty() {
        return Ok(());
    }
    if let Some(record_id) = record_id_slot.as_deref() {
        // existingRecord distinguishes a legitimately missing prior segment from a stale sealed identity.
        // existingRecord 区分合法缺失的先前分段与过期的已封口身份。
        let existing_record = runtime_state
            .conversation_state
            .records
            .iter()
            .find(|record| record.id == record_id);
        if let Some(existing_record) = existing_record {
            let is_active = existing_record
                .metadata
                .as_ref()
                .and_then(serde_json::Value::as_object)
                .and_then(|metadata| metadata.get(STREAMING_RECORD_STATE_METADATA_KEY))
                .and_then(serde_json::Value::as_str)
                == Some(STREAMING_RECORD_STATE_ACTIVE);
            if !is_active {
                return Err(format!(
                    "cannot append to non-active streaming record {record_id}"
                ));
            }
        }
        if !append_record_body_delta(&mut runtime_state.conversation_state, record_id, delta) {
            *record_id_slot = None;
        }
    }
    if record_id_slot.is_none() {
        // metadata records an explicit active lifecycle so later hydration never guesses from run phase.
        // metadata 记录显式活跃生命周期，使后续水合无需根据运行阶段猜测。
        let mut metadata = serde_json::Map::from_iter([(
            STREAMING_RECORD_STATE_METADATA_KEY.to_string(),
            serde_json::Value::String(STREAMING_RECORD_STATE_ACTIVE.to_string()),
        )]);
        if collapsed_by_default {
            metadata.insert(
                "collapsed_by_default".to_string(),
                serde_json::Value::Bool(true),
            );
        }
        let record_id = build_session_record_message_id(
            session_id,
            now,
            runtime_state.conversation_state.records.len(),
        );
        append_record(
            &mut runtime_state.conversation_state,
            SessionConversationRecord {
                id: record_id.clone(),
                kind: SessionMessageKind::Assistant,
                record_kind,
                visible: true,
                title: title.to_string(),
                body: delta.to_string(),
                metadata: Some(serde_json::Value::Object(metadata)),
                created_at: now.to_string(),
            },
        )
        .map_err(|error| format!("Invalid streaming record identity: {error}"))?;
        *record_id_slot = Some(record_id);
    }
    runtime_state.updated_at = now.to_string();
    Ok(())
}

/// Permanently closes one exact streaming transcript record in runtime state.
/// 在运行时状态中永久封口一条精确的流式转录记录。
///
/// # Parameters
/// # 参数
/// `runtime_state` owns the authoritative runtime conversation records.
/// `runtime_state` 持有权威运行时会话记录。
/// `now` is the retained-state update timestamp.
/// `now` 是保留状态更新时间戳。
/// `record_id` identifies the exact streamed record to close.
/// `record_id` 标识需要封口的精确流式记录。
///
/// # Returns
/// # 返回值
/// The sealed authoritative record, `None` when the record no longer exists, or an explicit metadata error.
/// 返回封口后的权威记录；记录已不存在时返回 `None`；元数据非法时返回显式错误。
pub fn seal_streaming_record_in_state(
    runtime_state: &mut SessionRuntimeStateEntry,
    now: &str,
    record_id: &str,
) -> Result<Option<SessionConversationRecord>, String> {
    // record is the exact runtime-owned row referenced by the request-local stream identity.
    // record 是请求局部流身份指向的精确运行时记录。
    let Some(record) = runtime_state
        .conversation_state
        .records
        .iter_mut()
        .find(|record| record.id == record_id)
    else {
        return Ok(None);
    };
    // metadata must be the object created by append_stream_delta_record_to_state for streamed rows.
    // metadata 必须是 append_stream_delta_record_to_state 为流式行创建的对象。
    let metadata = record
        .metadata
        .as_mut()
        .and_then(serde_json::Value::as_object_mut)
        .ok_or_else(|| format!("streaming record {record_id} has invalid lifecycle metadata"))?;
    let lifecycle = metadata
        .get(STREAMING_RECORD_STATE_METADATA_KEY)
        .and_then(serde_json::Value::as_str);
    if lifecycle == Some(STREAMING_RECORD_STATE_SEALED) {
        return Ok(None);
    }
    if lifecycle != Some(STREAMING_RECORD_STATE_ACTIVE) {
        return Err(format!(
            "streaming record {record_id} has invalid lifecycle state"
        ));
    }
    metadata.insert(
        STREAMING_RECORD_STATE_METADATA_KEY.to_string(),
        serde_json::Value::String(STREAMING_RECORD_STATE_SEALED.to_string()),
    );
    runtime_state.updated_at = now.to_string();
    Ok(Some(record.clone()))
}

/// Permanently closes every active streamed transcript record during an external run stop.
/// 在外部停止运行时永久封口全部活跃流式转录记录。
///
/// # Parameters
/// # 参数
/// `runtime_state` owns the authoritative runtime conversation records.
/// `runtime_state` 持有权威运行时会话记录。
/// `now` is the retained-state update timestamp.
/// `now` 是保留状态更新时间戳。
///
/// # Returns
/// # 返回值
/// The number of records whose lifecycle changed from active to sealed.
/// 返回生命周期从活跃变为已封口的记录数量。
pub fn seal_active_streaming_records_in_state(
    runtime_state: &mut SessionRuntimeStateEntry,
    now: &str,
) -> usize {
    // sealed_count makes the state timestamp change only when an authoritative lifecycle changed.
    // sealed_count 确保仅在权威生命周期确实变化时更新时间戳。
    let mut sealed_count = 0;
    for record in &mut runtime_state.conversation_state.records {
        // metadata is optional for stable historical rows and must not be inferred from record kind.
        // 稳定历史行允许没有 metadata，不能根据记录类型推断生命周期。
        let Some(metadata) = record
            .metadata
            .as_mut()
            .and_then(serde_json::Value::as_object_mut)
        else {
            continue;
        };
        let is_active = metadata
            .get(STREAMING_RECORD_STATE_METADATA_KEY)
            .and_then(serde_json::Value::as_str)
            == Some(STREAMING_RECORD_STATE_ACTIVE);
        if !is_active {
            continue;
        }
        metadata.insert(
            STREAMING_RECORD_STATE_METADATA_KEY.to_string(),
            serde_json::Value::String(STREAMING_RECORD_STATE_SEALED.to_string()),
        );
        sealed_count += 1;
    }
    if sealed_count > 0 {
        runtime_state.updated_at = now.to_string();
    }
    sealed_count
}

/// Keeps the active streaming thinking record collapsed after the thinking phase completes.
/// 在思考阶段完成后保持当前流式思考记录折叠。
pub fn collapse_streaming_thinking_in_state(
    runtime_state: &mut SessionRuntimeStateEntry,
    now: &str,
    record_ids: &SessionStreamingRecordIds,
) {
    let Some(thinking_record_id) = record_ids.thinking_record_id.as_deref() else {
        return;
    };
    runtime_state
        .ui_state
        .collapsed_message_ids
        .insert(thinking_record_id.to_string());
    runtime_state.updated_at = now.to_string();
}
