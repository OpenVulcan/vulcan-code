//! Runtime log persistence for segmented sessions.
//! 分段会话的运行时日志持久化。

use super::*;
use sha2::{Digest, Sha256};
use vulcan_context::protocol::{
    SessionConversationMessage, validate_session_conversation_record_identities,
};

/// Bounded canonical-conversation checkpoint persisted beside runtime JSONL metadata.
/// 与运行时 JSONL 元数据一起持久化的有界规范会话检查点。
#[derive(Debug)]
struct RuntimeConversationProjectionCheckpoint {
    /// Number of canonical messages covered by the checkpoint.
    /// 检查点覆盖的规范消息数量。
    message_count: u64,
    /// Number of conversation-projecting runtime events covered by the checkpoint.
    /// 检查点覆盖的会话投影运行时事件数量。
    event_count: u64,
    /// Absolute canonical message index where the bounded open anchor begins.
    /// 有界开放锚点开始处的规范消息绝对索引。
    anchor_message_index: u64,
    /// Number of already persisted events produced from the open anchor.
    /// 从开放锚点生成且已经持久化的事件数量。
    anchor_event_count: u64,
    /// Digest of the final canonical message covered by the checkpoint.
    /// 检查点覆盖的最后一条规范消息摘要。
    tail_message_sha256: Option<String>,
    /// Digest of already persisted event payloads produced from the open anchor.
    /// 从开放锚点生成且已经持久化的事件载荷摘要。
    anchor_projection_sha256: Option<String>,
}

/// Projects a bounded canonical message slice without carrying full conversation records.
/// 在不携带完整会话记录的情况下投影有界规范消息切片。
///
/// # Parameters
/// 参数
/// `messages` is the exact bounded message slice and `updated_at` supplies deterministic line timestamps.
/// `messages` 是精确的有界消息切片，`updated_at` 提供确定性的日志行时间戳。
///
/// # Returns
/// 返回值
/// Ordered runtime projection lines beginning at sequence one, or a strict projection error.
/// 返回从序号一开始的有序运行时投影行，或严格投影错误。
fn runtime_projection_lines_for_message_slice(
    messages: &[SessionConversationMessage],
    updated_at: &str,
) -> Result<Vec<RuntimeLogEventLine>, String> {
    let bounded_state = SessionConversationState {
        messages: messages.to_vec(),
        records: Vec::new(),
        continuation: Default::default(),
    };
    runtime_log_lines_from_conversation_state(&bounded_state, updated_at, 1)
        .map_err(|error| error.to_string())
}

/// Computes SHA-256 for one canonical conversation message.
/// 计算一条规范会话消息的 SHA-256。
///
/// # Parameters
/// 参数
/// `message` is the exact canonical message covered by a persistence checkpoint.
/// `message` 是持久化检查点覆盖的精确规范消息。
///
/// # Returns
/// 返回值
/// Lowercase hexadecimal SHA-256, or a serialization failure.
/// 返回小写十六进制 SHA-256，或序列化失败。
fn runtime_conversation_message_sha256(
    message: &SessionConversationMessage,
) -> Result<String, String> {
    let encoded = serde_json::to_vec(message).map_err(to_store_error)?;
    Ok(hex::encode(Sha256::digest(encoded)))
}

/// Computes a length-delimited SHA-256 over ordered runtime event payloads.
/// 对有序运行时事件载荷计算带长度分隔的 SHA-256。
///
/// # Parameters
/// 参数
/// `lines` supplies ordered event payloads; sequence numbers and write timestamps are intentionally excluded.
/// `lines` 提供有序事件载荷；序号与写入时间戳被有意排除。
///
/// # Returns
/// 返回值
/// Lowercase hexadecimal SHA-256 for a non-empty slice, `None` for an empty slice, or serialization failure.
/// 对非空切片返回小写十六进制 SHA-256；空切片返回 `None`；序列化失败时返回错误。
fn runtime_event_payloads_sha256(lines: &[RuntimeLogEventLine]) -> Result<Option<String>, String> {
    if lines.is_empty() {
        return Ok(None);
    }
    let mut digest = Sha256::new();
    for line in lines {
        let encoded = serde_json::to_vec(&line.event).map_err(to_store_error)?;
        let encoded_len = u64::try_from(encoded.len())
            .map_err(|_| String::from("Runtime event payload exceeds u64 digest capacity."))?;
        digest.update(encoded_len.to_le_bytes());
        digest.update(encoded);
    }
    Ok(Some(hex::encode(digest.finalize())))
}

/// Locates the earliest tool batch that remains open in one bounded projection.
/// 定位一个有界投影中最早仍保持开放的工具批次。
///
/// # Parameters
/// 参数
/// `lines` is an ordered conversation-only runtime projection.
/// `lines` 是有序的纯会话运行时投影。
///
/// # Returns
/// 返回值
/// The owning assistant message identifier for the earliest open batch, or `None` when all batches are closed.
/// 返回最早开放批次所属的助手消息标识；全部批次已关闭时返回 `None`。
fn earliest_open_tool_batch_assistant_message_id(lines: &[RuntimeLogEventLine]) -> Option<String> {
    let mut open_batches = BTreeMap::<String, (u64, String)>::new();
    for line in lines {
        match &line.event {
            RuntimeLogEvent::ToolBatchStarted(event) => {
                open_batches.insert(
                    event.tool_batch_id.clone(),
                    (line.seq, event.assistant_message_id.clone()),
                );
            }
            RuntimeLogEvent::ToolBatchCompleted(event) => {
                open_batches.remove(&event.tool_batch_id);
            }
            _ => {}
        }
    }
    open_batches
        .into_values()
        .min_by_key(|(seq, _)| *seq)
        .map(|(_, assistant_message_id)| assistant_message_id)
}

/// Computes the bounded active-anchor checkpoint for one projected message suffix.
/// 为一个已投影消息后缀计算有界活动锚点检查点。
///
/// # Parameters
/// 参数
/// `messages` is the bounded suffix, `absolute_start_index` is its canonical base index,
/// `projected_lines` is its complete projection, and `updated_at` supplies deterministic timestamps.
/// `messages` 是有界后缀，`absolute_start_index` 是其规范基准索引，
/// `projected_lines` 是完整投影，`updated_at` 提供确定性时间戳。
///
/// # Returns
/// 返回值
/// Absolute anchor index, persisted anchor-event count, and optional event-payload digest.
/// 返回绝对锚点索引、已持久化锚点事件数量及可选事件载荷摘要。
fn runtime_open_anchor_checkpoint(
    messages: &[SessionConversationMessage],
    absolute_start_index: usize,
    projected_lines: &[RuntimeLogEventLine],
    updated_at: &str,
) -> Result<(u64, u64, Option<String>), String> {
    let Some(assistant_message_id) = earliest_open_tool_batch_assistant_message_id(projected_lines)
    else {
        let absolute_end_index = absolute_start_index
            .checked_add(messages.len())
            .ok_or_else(|| String::from("Runtime conversation anchor index exceeds usize."))?;
        return Ok((
            u64::try_from(absolute_end_index).map_err(|_| {
                String::from("Runtime conversation anchor index exceeds u64 metadata capacity.")
            })?,
            0,
            None,
        ));
    };
    let relative_anchor_index = messages
        .iter()
        .position(|message| message.id == assistant_message_id)
        .ok_or_else(|| {
            format!(
                "Open runtime tool batch references missing assistant message `{assistant_message_id}`."
            )
        })?;
    let absolute_anchor_index = absolute_start_index
        .checked_add(relative_anchor_index)
        .ok_or_else(|| String::from("Runtime conversation anchor index exceeds usize."))?;
    let anchor_lines =
        runtime_projection_lines_for_message_slice(&messages[relative_anchor_index..], updated_at)?;
    let anchor_event_count = u64::try_from(anchor_lines.len()).map_err(|_| {
        String::from("Runtime conversation anchor event count exceeds u64 metadata capacity.")
    })?;
    Ok((
        u64::try_from(absolute_anchor_index).map_err(|_| {
            String::from("Runtime conversation anchor index exceeds u64 metadata capacity.")
        })?,
        anchor_event_count,
        runtime_event_payloads_sha256(&anchor_lines)?,
    ))
}

/// Builds the complete constant-size projection checkpoint after a cold rebuild.
/// 在冷重建后构建完整的常量大小投影检查点。
///
/// # Parameters
/// 参数
/// `conversation_state` is the canonical state, `projected_lines` is its conversation-only projection,
/// and `updated_at` supplies deterministic bounded-anchor timestamps.
/// `conversation_state` 是规范状态，`projected_lines` 是其纯会话投影，
/// `updated_at` 提供确定性的有界锚点时间戳。
///
/// # Returns
/// 返回值
/// A complete checkpoint suitable for metadata publication, or a strict conversion failure.
/// 返回可发布到元数据的完整检查点，或严格转换失败。
fn runtime_projection_checkpoint_from_conversation_state(
    conversation_state: &SessionConversationState,
    projected_lines: &[RuntimeLogEventLine],
    updated_at: &str,
) -> Result<RuntimeConversationProjectionCheckpoint, String> {
    let message_count = u64::try_from(conversation_state.messages.len()).map_err(|_| {
        String::from("Runtime conversation message count exceeds u64 metadata capacity.")
    })?;
    let event_count = u64::try_from(projected_lines.len()).map_err(|_| {
        String::from("Runtime conversation event count exceeds u64 metadata capacity.")
    })?;
    let (anchor_message_index, anchor_event_count, anchor_projection_sha256) =
        runtime_open_anchor_checkpoint(
            &conversation_state.messages,
            0,
            projected_lines,
            updated_at,
        )?;
    let tail_message_sha256 = conversation_state
        .messages
        .last()
        .map(runtime_conversation_message_sha256)
        .transpose()?;
    Ok(RuntimeConversationProjectionCheckpoint {
        message_count,
        event_count,
        anchor_message_index,
        anchor_event_count,
        tail_message_sha256,
        anchor_projection_sha256,
    })
}

/// Rebuilds one provider-facing runtime log inside a serialized append transaction.
/// 在串行化追加事务中重建一份 Provider 可见运行时日志。
///
/// # Parameters
/// 参数
/// `session_dir` is the segmented session directory whose runtime artifacts are replaced.
/// `session_dir` 是运行时产物将被替换的分段会话目录。
/// `conversation_state` is the canonical provider-facing conversation projected into events.
/// `conversation_state` 是将被投影为事件的规范 Provider 会话。
/// `updated_at` is attached to rebuilt records and metadata.
/// `updated_at` 会附加到重建后的记录与元数据。
///
/// # Returns
/// 返回值
/// Metadata for the fully published rebuilt runtime log.
/// 返回完整发布后的重建运行时日志元数据。
pub(in crate::workspace_store) fn rebuild_runtime_log_from_conversation_state(
    session_dir: &Path,
    conversation_state: &SessionConversationState,
    updated_at: &str,
) -> Result<RuntimeLogMetadata, String> {
    validate_session_conversation_record_identities(&conversation_state.records)
        .map_err(|error| format!("Invalid canonical conversation record identity: {error}"))?;
    // append_guard keeps destructive rebuild publication serialized with every runtime append.
    // append_guard 使破坏性重建发布与每次运行时追加保持串行。
    let session_storage_lock = segmented_append_file_io_lock(session_dir);
    let append_guard = session_storage_lock.lock().map_err(to_store_error)?;
    rebuild_runtime_log_with_owned_lock(
        session_dir,
        conversation_state,
        updated_at,
        Vec::new(),
        &append_guard,
    )
}

/// Rebuilds provider-facing runtime JSONL while preserving audit-only runtime events.
/// 在保留仅审计运行时事件的同时重建 Provider 可见运行时 JSONL。
///
/// # Parameters
/// 参数
/// `session_dir` is the segmented session directory whose runtime artifacts are replaced.
/// `session_dir` 是运行时产物将被替换的分段会话目录。
/// `conversation_state` is the canonical provider-facing state projected into runtime lines.
/// `conversation_state` 是将被投影为运行时日志行的规范 Provider 状态。
/// `updated_at` is attached to rebuilt records and metadata.
/// `updated_at` 会附加到重建后的记录与元数据。
/// `extra_events` contains audit-only events preserved across the rebuild.
/// `extra_events` 包含跨重建保留的仅审计事件。
/// `_append_guard` proves the entire destructive rebuild shares one append-lock transaction.
/// `_append_guard` 证明完整破坏性重建共享一个追加锁事务。
///
/// # Returns
/// 返回值
/// Metadata for the fully published rebuilt runtime log.
/// 返回完整发布后的重建运行时日志元数据。
fn rebuild_runtime_log_with_owned_lock(
    session_dir: &Path,
    conversation_state: &SessionConversationState,
    updated_at: &str,
    extra_events: Vec<RuntimeLogEventLine>,
    _append_guard: &std::sync::RwLockWriteGuard<'_, ()>,
) -> Result<RuntimeLogMetadata, String> {
    // active_segment is the sole implemented segment selected before destructive work begins.
    // active_segment 是在破坏性操作开始前选定的唯一已实现分段。
    let active_segment = runtime_segment_file_name();
    // lines is fully projected and resequenced before the previous runtime directory is touched.
    // lines 会在接触旧运行时目录前完成完整投影与重新排序。
    let mut lines = runtime_log_lines_from_conversation_state(conversation_state, updated_at, 1)
        .map_err(|error| error.to_string())?;
    // conversation_checkpoint is computed before audit-only events are appended to the physical log.
    // conversation_checkpoint 在仅审计事件追加到物理日志前完成计算。
    let conversation_checkpoint = runtime_projection_checkpoint_from_conversation_state(
        conversation_state,
        &lines,
        updated_at,
    )?;
    append_runtime_extra_events_with_fresh_sequences(&mut lines, extra_events)?;
    // next_seq validates the complete projection before any prior runtime bytes are removed.
    // next_seq 在移除任何旧运行时字节前验证完整投影。
    let next_seq = validate_rebuilt_runtime_lines(&lines)?;
    // encoded_lines proves serialization succeeds before destructive directory replacement begins.
    // encoded_lines 证明序列化会在破坏性目录替换开始前成功。
    let encoded_lines = encode_json_lines(&lines)?;
    // line_count converts the already materialized record count into metadata units.
    // line_count 将已物化记录数转换为元数据单位。
    let line_count = u64::try_from(lines.len())
        .map_err(|_| String::from("Runtime line count exceeds u64 metadata capacity."))?;
    // byte_len derives from the exact candidate buffer that will be published.
    // byte_len 从即将发布的精确候选缓冲推导。
    let byte_len = u64::try_from(encoded_lines.len())
        .map_err(|_| String::from("Runtime JSONL bytes exceed u64 metadata capacity."))?;
    // runtime_dir is retained so the fixed segment can be replaced atomically without an absence gap.
    // runtime_dir 会被保留，使固定分段能够原子替换且不会出现缺失窗口。
    let runtime_dir = runtime_dir_path(session_dir);
    create_dir_all(&runtime_dir)?;
    invalidate_runtime_system_update_cache(session_dir)?;
    // active_segment_path receives the already encoded complete candidate.
    // active_segment_path 接收已经编码完成的完整候选内容。
    let active_segment_path = session_dir.join(&active_segment);
    write_file_bytes_atomic(
        &active_segment_path,
        &encoded_lines,
        "runtime JSONL document",
    )?;
    let mut segment =
        RuntimeSegmentMetadata::empty(active_segment.clone(), 1, updated_at.to_string());
    segment.line_count = line_count;
    segment.byte_len = byte_len;
    segment.end_seq = lines.last().map(|line| line.seq);
    segment.updated_at = lines.last().map(|line| line.created_at.clone());
    let metadata = RuntimeLogMetadata {
        schema_version: 1,
        active_segment,
        next_seq,
        created_at: updated_at.to_string(),
        updated_at: updated_at.to_string(),
        conversation_checkpoint_version: SEGMENTED_RUNTIME_PROJECTION_CHECKPOINT_VERSION,
        conversation_message_count: conversation_checkpoint.message_count,
        conversation_event_count: conversation_checkpoint.event_count,
        conversation_anchor_message_index: conversation_checkpoint.anchor_message_index,
        conversation_anchor_event_count: conversation_checkpoint.anchor_event_count,
        conversation_tail_message_sha256: conversation_checkpoint.tail_message_sha256,
        conversation_anchor_projection_sha256: conversation_checkpoint.anchor_projection_sha256,
        applied_compaction_id: None,
        segments: vec![segment],
    };
    write_runtime_metadata(session_dir, &metadata)?;
    write_runtime_index(session_dir, &rebuild_runtime_log_index(&lines))?;
    publish_runtime_system_update_cache(session_dir, metadata.next_seq, &lines)?;
    Ok(metadata)
}

/// Validates a complete rebuilt runtime projection before destructive publication begins.
/// 在破坏性发布开始前验证完整的重建运行时投影。
///
/// # Parameters
/// 参数
/// `lines` is the fully materialized conversation-plus-audit runtime log candidate.
/// `lines` 是完整物化的会话加审计运行时日志候选。
///
/// # Returns
/// 返回值
/// The checked next sequence after a schema-one contiguous log, or an integrity failure.
/// 返回模式一连续日志后的受检下一序号，否则返回完整性失败。
fn validate_rebuilt_runtime_lines(lines: &[RuntimeLogEventLine]) -> Result<u64, String> {
    // expected_seq starts at the only sequence supported by rebuilt runtime logs.
    // expected_seq 从重建运行时日志唯一支持的起始序号开始。
    let mut expected_seq = 1_u64;
    for (line_index, line) in lines.iter().enumerate() {
        if line.schema_version != RUNTIME_LOG_SCHEMA_VERSION {
            return Err(format!(
                "Unsupported rebuilt runtime line schema version {} at line {}.",
                line.schema_version,
                line_index.saturating_add(1)
            ));
        }
        if line.seq != expected_seq {
            return Err(format!(
                "Rebuilt runtime line sequence mismatch at line {}: expected {expected_seq}, found {}.",
                line_index.saturating_add(1),
                line.seq
            ));
        }
        expected_seq = line.seq.checked_add(1).ok_or_else(|| {
            format!(
                "Rebuilt runtime sequence overflows after line {} with sequence {}.",
                line_index.saturating_add(1),
                line.seq
            )
        })?;
    }
    Ok(expected_seq)
}

/// Appends runtime JSONL tail lines when possible and rebuilds when history no longer matches.
/// 在可能时追加 runtime JSONL 尾部，并在历史不再匹配时重建。
///
/// # Parameters
/// 参数
/// `session_dir` is the segmented session directory whose runtime log is synchronized.
/// `session_dir` 是运行时日志将被同步的分段会话目录。
/// `conversation_state` is the canonical provider-facing conversation.
/// `conversation_state` 是规范 Provider 会话。
/// `updated_at` is attached to newly materialized runtime records.
/// `updated_at` 会附加到新物化的运行时记录。
///
/// # Returns
/// 返回值
/// Updated runtime metadata after an append, rebuild, or no-op synchronization.
/// 追加、重建或空操作同步后返回更新后的运行时元数据。
pub(in crate::workspace_store) fn sync_runtime_log_from_conversation_state(
    session_dir: &Path,
    conversation_state: &SessionConversationState,
    updated_at: &str,
) -> Result<RuntimeLogMetadata, String> {
    validate_session_conversation_record_identities(&conversation_state.records)
        .map_err(|error| format!("Invalid canonical conversation record identity: {error}"))?;
    // append_guard keeps read, decision, append, and rebuild inside one runtime transaction.
    // append_guard 将读取、决策、追加与重建保持在同一个运行时事务内。
    let session_storage_lock = segmented_append_file_io_lock(session_dir);
    let append_guard = session_storage_lock.lock().map_err(to_store_error)?;
    sync_runtime_log_from_conversation_state_with_owned_lock(
        session_dir,
        conversation_state,
        updated_at,
        &append_guard,
    )
}

/// Synchronizes one canonical conversation while the caller owns the append lock.
/// 在调用方持有追加锁时同步一个规范会话。
///
/// # Parameters
/// 参数
/// `session_dir` is the segmented session directory whose runtime log is synchronized.
/// `session_dir` 是运行时日志将被同步的分段会话目录。
/// `conversation_state` is the canonical provider-facing conversation.
/// `conversation_state` 是规范 Provider 会话。
/// `updated_at` is attached to newly materialized runtime records.
/// `updated_at` 会附加到新物化的运行时记录。
/// `append_guard` proves the read-to-write decision remains in one lock transaction.
/// `append_guard` 证明从读取到写入的决策保持在同一个锁事务内。
///
/// # Returns
/// 返回值
/// Updated runtime metadata after an append, rebuild, or no-op synchronization.
/// 追加、重建或空操作同步后返回更新后的运行时元数据。
fn sync_runtime_log_from_conversation_state_with_owned_lock(
    session_dir: &Path,
    conversation_state: &SessionConversationState,
    updated_at: &str,
    append_guard: &std::sync::RwLockWriteGuard<'_, ()>,
) -> Result<RuntimeLogMetadata, String> {
    // metadata is the constant-size hot-path checkpoint; runtime JSONL is not opened here.
    // metadata 是常量大小的热路径检查点；此处不会打开运行时 JSONL。
    let Some(mut metadata) = read_optional_runtime_metadata(session_dir)? else {
        ensure_runtime_metadata_absence_is_consistent(session_dir)?;
        return rebuild_runtime_log_with_owned_lock(
            session_dir,
            conversation_state,
            updated_at,
            Vec::new(),
            append_guard,
        );
    };
    if metadata.conversation_checkpoint_version != SEGMENTED_RUNTIME_PROJECTION_CHECKPOINT_VERSION {
        return rebuild_runtime_log_preserving_existing_audit_with_owned_lock(
            session_dir,
            conversation_state,
            updated_at,
            append_guard,
        );
    }
    validate_runtime_segment_append_boundary(session_dir, &metadata)?;
    let current_message_count = conversation_state.messages.len();
    let persisted_message_count =
        usize::try_from(metadata.conversation_message_count).map_err(|_| {
            String::from("Persisted runtime conversation message count exceeds usize capacity.")
        })?;
    if persisted_message_count > current_message_count {
        return rebuild_runtime_log_preserving_existing_audit_with_owned_lock(
            session_dir,
            conversation_state,
            updated_at,
            append_guard,
        );
    }
    if persisted_message_count > 0 {
        // persisted_tail is direct-indexed from metadata and proves the append-only message boundary.
        // persisted_tail 由元数据直接索引，用于证明追加式消息边界。
        let persisted_tail = &conversation_state.messages[persisted_message_count - 1];
        let current_tail_sha256 = runtime_conversation_message_sha256(persisted_tail)?;
        if metadata.conversation_tail_message_sha256.as_deref()
            != Some(current_tail_sha256.as_str())
        {
            return rebuild_runtime_log_preserving_existing_audit_with_owned_lock(
                session_dir,
                conversation_state,
                updated_at,
                append_guard,
            );
        }
    }
    let anchor_message_index = usize::try_from(metadata.conversation_anchor_message_index)
        .map_err(|_| String::from("Runtime conversation anchor index exceeds usize capacity."))?;
    if anchor_message_index > persisted_message_count {
        return Err(format!(
            "Runtime conversation anchor index {anchor_message_index} exceeds persisted message count {persisted_message_count}."
        ));
    }
    // bounded_messages begins at the only mutable open-batch anchor and normally contains one tool batch.
    // bounded_messages 从唯一可变开放批次锚点开始，通常只包含一个工具批次。
    let bounded_messages = &conversation_state.messages[anchor_message_index..];
    let bounded_projection =
        runtime_projection_lines_for_message_slice(bounded_messages, updated_at)?;
    let persisted_anchor_event_count = usize::try_from(metadata.conversation_anchor_event_count)
        .map_err(|_| String::from("Runtime anchor event count exceeds usize capacity."))?;
    if persisted_anchor_event_count > bounded_projection.len() {
        return rebuild_runtime_log_preserving_existing_audit_with_owned_lock(
            session_dir,
            conversation_state,
            updated_at,
            append_guard,
        );
    }
    let current_anchor_sha256 =
        runtime_event_payloads_sha256(&bounded_projection[..persisted_anchor_event_count])?;
    if metadata.conversation_anchor_projection_sha256 != current_anchor_sha256 {
        return rebuild_runtime_log_preserving_existing_audit_with_owned_lock(
            session_dir,
            conversation_state,
            updated_at,
            append_guard,
        );
    }
    // projected_tail contains only events not covered by the validated bounded anchor prefix.
    // projected_tail 只包含尚未被已验证有界锚点前缀覆盖的事件。
    let projected_tail = &bounded_projection[persisted_anchor_event_count..];
    let mut next_seq = metadata.next_seq;
    let mut append_tail = Vec::with_capacity(projected_tail.len());
    for projected_line in projected_tail {
        append_tail.push(RuntimeLogEventLine::new(
            next_seq,
            updated_at.to_string(),
            projected_line.event.clone(),
        ));
        next_seq = next_seq.checked_add(1).ok_or_else(|| {
            String::from("Runtime sequence exceeds u64 capacity while materializing bounded tail.")
        })?;
    }
    let appended_event_count = u64::try_from(append_tail.len()).map_err(|_| {
        String::from("Runtime bounded append event count exceeds u64 metadata capacity.")
    })?;
    let next_conversation_event_count = metadata
        .conversation_event_count
        .checked_add(appended_event_count)
        .ok_or_else(|| {
            String::from("Runtime conversation event count exceeds u64 metadata capacity.")
        })?;
    let (next_anchor_message_index, next_anchor_event_count, next_anchor_sha256) =
        runtime_open_anchor_checkpoint(
            bounded_messages,
            anchor_message_index,
            &bounded_projection,
            updated_at,
        )?;
    let next_message_count = u64::try_from(current_message_count).map_err(|_| {
        String::from("Runtime conversation message count exceeds u64 metadata capacity.")
    })?;
    let next_tail_message_sha256 = conversation_state
        .messages
        .last()
        .map(runtime_conversation_message_sha256)
        .transpose()?;
    let checkpoint_changed = metadata.conversation_message_count != next_message_count
        || metadata.conversation_event_count != next_conversation_event_count
        || metadata.conversation_anchor_message_index != next_anchor_message_index
        || metadata.conversation_anchor_event_count != next_anchor_event_count
        || metadata.conversation_tail_message_sha256 != next_tail_message_sha256
        || metadata.conversation_anchor_projection_sha256 != next_anchor_sha256;
    metadata.conversation_message_count = next_message_count;
    metadata.conversation_event_count = next_conversation_event_count;
    metadata.conversation_anchor_message_index = next_anchor_message_index;
    metadata.conversation_anchor_event_count = next_anchor_event_count;
    metadata.conversation_tail_message_sha256 = next_tail_message_sha256;
    metadata.conversation_anchor_projection_sha256 = next_anchor_sha256;
    if append_tail.is_empty() {
        if checkpoint_changed {
            metadata.updated_at = updated_at.to_string();
            write_runtime_metadata(session_dir, &metadata)?;
        }
        return Ok(metadata);
    }
    append_runtime_log_tail_from_metadata_with_owned_lock(
        session_dir,
        metadata,
        &append_tail,
        append_guard,
    )
}

/// Rebuilds a changed or legacy conversation while preserving exact audit-only events.
/// 在保留精确仅审计事件的同时重建发生变化或使用旧版检查点的会话。
///
/// # Parameters
/// 参数
/// `session_dir` is the exact session directory, `conversation_state` is the new canonical state,
/// `updated_at` supplies rebuild timestamps, and `append_guard` owns the complete transaction.
/// `session_dir` 是精确会话目录，`conversation_state` 是新的规范状态，
/// `updated_at` 提供重建时间戳，`append_guard` 持有完整事务。
///
/// # Returns
/// 返回值
/// Rebuilt metadata after one intentional cold full read, or a strict persistence failure.
/// 在一次有意冷路径完整读取后返回重建元数据，或严格持久化失败。
fn rebuild_runtime_log_preserving_existing_audit_with_owned_lock(
    session_dir: &Path,
    conversation_state: &SessionConversationState,
    updated_at: &str,
    append_guard: &std::sync::RwLockWriteGuard<'_, ()>,
) -> Result<RuntimeLogMetadata, String> {
    let Some(snapshot) = read_runtime_log_snapshot_with_owned_lock(session_dir, append_guard)?
    else {
        return rebuild_runtime_log_with_owned_lock(
            session_dir,
            conversation_state,
            updated_at,
            Vec::new(),
            append_guard,
        );
    };
    let extra_events = runtime_extra_event_lines(&snapshot.lines);
    rebuild_runtime_log_with_owned_lock(
        session_dir,
        conversation_state,
        updated_at,
        extra_events,
        append_guard,
    )
}

/// Syncs provider-facing runtime JSONL after compaction while dropping pre-compaction audit tails.
/// 在压缩后同步 Provider 可见运行时 JSONL，并丢弃压缩前审计尾巴。
///
/// # Parameters
/// # 参数
/// `session_dir` is the segmented session directory that owns the runtime log.
/// `session_dir` 是拥有运行时日志的分段会话目录。
/// `conversation_state` is the compacted canonical provider conversation.
/// `conversation_state` 是已压缩的规范 Provider 会话。
/// `updated_at` is written onto rebuilt runtime lines.
/// `updated_at` 会写入重建后的运行时行。
/// `compaction_id` identifies the compaction boundary that must remain in the main runtime log.
/// `compaction_id` 标识必须保留在主运行时日志中的压缩边界。
///
/// # Returns
/// # 返回值
/// Returns updated runtime-log metadata after append or rebuild.
/// 返回追加或重建后的运行时日志元数据。
pub(in crate::workspace_store) fn sync_runtime_log_from_compacted_conversation_state(
    session_dir: &Path,
    conversation_state: &SessionConversationState,
    updated_at: &str,
    compaction_id: &str,
) -> Result<RuntimeLogMetadata, String> {
    validate_session_conversation_record_identities(&conversation_state.records)
        .map_err(|error| format!("Invalid canonical conversation record identity: {error}"))?;
    // append_guard serializes compaction synchronization with runtime audit appends and rebuilds.
    // append_guard 将压缩同步与运行时审计追加及重建串行化。
    let session_storage_lock = segmented_append_file_io_lock(session_dir);
    let append_guard = session_storage_lock.lock().map_err(to_store_error)?;
    sync_runtime_log_from_compacted_conversation_state_with_owned_lock(
        session_dir,
        conversation_state,
        updated_at,
        compaction_id,
        &append_guard,
    )
}

/// Synchronizes a compacted conversation while the caller owns the append lock.
/// 在调用方持有追加锁时同步已压缩会话。
///
/// # Parameters
/// 参数
/// `session_dir` is the segmented session directory whose runtime log is synchronized.
/// `session_dir` 是运行时日志将被同步的分段会话目录。
/// `conversation_state` is the compacted canonical provider conversation.
/// `conversation_state` 是已压缩的规范 Provider 会话。
/// `updated_at` is attached to newly materialized runtime records.
/// `updated_at` 会附加到新物化的运行时记录。
/// `compaction_id` identifies the audit boundary retained after compaction.
/// `compaction_id` 标识压缩后保留的审计边界。
/// `append_guard` proves the read-to-write decision remains in one lock transaction.
/// `append_guard` 证明从读取到写入的决策保持在同一个锁事务内。
///
/// # Returns
/// 返回值
/// Updated runtime metadata after an append, rebuild, or no-op synchronization.
/// 追加、重建或空操作同步后返回更新后的运行时元数据。
fn sync_runtime_log_from_compacted_conversation_state_with_owned_lock(
    session_dir: &Path,
    conversation_state: &SessionConversationState,
    updated_at: &str,
    compaction_id: &str,
    append_guard: &std::sync::RwLockWriteGuard<'_, ()>,
) -> Result<RuntimeLogMetadata, String> {
    if compaction_id.trim().is_empty() {
        return Err(String::from(
            "Runtime compaction synchronization requires a non-empty compaction identifier.",
        ));
    }
    if let Some(metadata) = read_optional_runtime_metadata(session_dir)?
        && metadata.applied_compaction_id.as_deref() == Some(compaction_id)
    {
        // The physical boundary is already current, so later checkpoints use the bounded ordinary tail path.
        // 物理边界已经是当前边界，因此后续检查点使用有界普通尾部路径。
        return sync_runtime_log_from_conversation_state_with_owned_lock(
            session_dir,
            conversation_state,
            updated_at,
            append_guard,
        );
    }
    let mut metadata = sync_runtime_log_from_compacted_conversation_state_cold_with_owned_lock(
        session_dir,
        conversation_state,
        updated_at,
        compaction_id,
        append_guard,
    )?;
    metadata.applied_compaction_id = Some(compaction_id.to_string());
    metadata.updated_at = updated_at.to_string();
    write_runtime_metadata(session_dir, &metadata)?;
    Ok(metadata)
}

/// Applies one new compaction boundary through an intentional cold runtime-log reconciliation.
/// 通过一次有意的冷路径运行时日志协调来应用新的压缩边界。
///
/// # Parameters
/// 参数
/// `session_dir` is the exact session directory, `conversation_state` is the compacted canonical state,
/// `updated_at` supplies persistence timestamps, `compaction_id` selects retained audit history,
/// and `append_guard` owns the complete runtime transaction.
/// `session_dir` 是精确会话目录，`conversation_state` 是已压缩规范状态，
/// `updated_at` 提供持久化时间戳，`compaction_id` 选择保留审计历史，
/// `append_guard` 持有完整运行时事务。
///
/// # Returns
/// 返回值
/// Reconciled metadata before the caller publishes the applied compaction identifier.
/// 返回调用方发布已应用压缩标识之前的协调后元数据。
fn sync_runtime_log_from_compacted_conversation_state_cold_with_owned_lock(
    session_dir: &Path,
    conversation_state: &SessionConversationState,
    updated_at: &str,
    compaction_id: &str,
    append_guard: &std::sync::RwLockWriteGuard<'_, ()>,
) -> Result<RuntimeLogMetadata, String> {
    let desired_lines =
        runtime_log_lines_from_conversation_state(conversation_state, updated_at, 1)
            .map_err(|error| error.to_string())?;
    // snapshot distinguishes a wholly absent runtime log from damaged persisted state.
    // snapshot 区分完全缺失的运行时日志与已损坏的持久化状态。
    let Some(snapshot) = read_runtime_log_snapshot_with_owned_lock(session_dir, append_guard)?
    else {
        return rebuild_runtime_log_with_owned_lock(
            session_dir,
            conversation_state,
            updated_at,
            Vec::new(),
            append_guard,
        );
    };
    // existing_extra_events captures every current audit-only event before boundary pruning.
    // existing_extra_events 在边界裁剪前捕获当前全部仅审计事件。
    let existing_extra_events = runtime_extra_event_lines(&snapshot.lines);
    // extra_events retains only events at or after the selected compaction boundary.
    // extra_events 仅保留所选压缩边界及其后的事件。
    let extra_events =
        runtime_extra_event_lines_from_compaction_boundary(&snapshot.lines, compaction_id);
    // existing_conversation_lines isolates provider-projected history for prefix comparison.
    // existing_conversation_lines 隔离用于前缀比较的 Provider 投影历史。
    let existing_conversation_lines = runtime_conversation_event_lines(&snapshot.lines);
    if !runtime_log_prefix_matches(&existing_conversation_lines, &desired_lines) {
        return rebuild_runtime_log_with_owned_lock(
            session_dir,
            conversation_state,
            updated_at,
            extra_events,
            append_guard,
        );
    }
    if existing_conversation_lines.len() == desired_lines.len() {
        if !runtime_extra_event_lines_match(&existing_extra_events, &extra_events) {
            return rebuild_runtime_log_with_owned_lock(
                session_dir,
                conversation_state,
                updated_at,
                extra_events,
                append_guard,
            );
        }
        return Ok(snapshot.metadata);
    }
    if !extra_events.is_empty() {
        return rebuild_runtime_log_with_owned_lock(
            session_dir,
            conversation_state,
            updated_at,
            extra_events,
            append_guard,
        );
    }
    // Compaction is an intentional cold boundary and republishes a complete projection checkpoint.
    // 压缩是有意的冷路径边界，并会重新发布完整投影检查点。
    rebuild_runtime_log_with_owned_lock(
        session_dir,
        conversation_state,
        updated_at,
        Vec::new(),
        append_guard,
    )
}

/// Reads an existing provider-facing runtime log while repairing corrupt JSONL tails.
/// 读取既有 Provider 可见运行时日志，并修复损坏的 JSONL 尾行。
///
/// # Parameters
/// 参数
/// `session_dir` is the segmented session directory whose runtime conversation is hydrated.
/// `session_dir` 是需要 hydration 运行时会话的分段会话目录。
///
/// # Returns
/// 返回值
/// A non-empty runtime conversation, `None` for consistent absence/empty projection, or failure.
/// 返回非空运行时会话；一致缺失或投影为空时返回 `None`，否则返回失败。
pub(in crate::workspace_store) fn ensure_runtime_log_from_segmented_context(
    session_dir: &Path,
) -> Result<Option<SessionConversationState>, String> {
    // conversation_state is absent only when all runtime artifacts are consistently absent.
    // conversation_state 仅在全部运行时产物一致缺失时为空。
    let Some(conversation_state) = read_runtime_conversation_state(session_dir)? else {
        return Ok(None);
    };
    if conversation_state.messages.is_empty() {
        Ok(None)
    } else {
        Ok(Some(conversation_state))
    }
}

/// Verifies absent runtime metadata is not contradicted by remaining runtime artifacts.
/// 验证运行时元数据缺失时不存在与之矛盾的残留运行时产物。
///
/// # Parameters
/// 参数
/// `session_dir` is the segmented session directory whose runtime state is classified.
/// `session_dir` 是需要分类运行时状态的分段会话目录。
///
/// # Returns
/// 返回值
/// Success only when both the runtime index and runtime segment directory are also absent.
/// 仅当运行时索引与运行时分段目录也同时缺失时返回成功。
pub(in crate::workspace_store) fn ensure_runtime_metadata_absence_is_consistent(
    session_dir: &Path,
) -> Result<(), String> {
    // candidate_paths are the active runtime artifacts whose presence requires metadata.
    // candidate_paths 是存在时必须配套元数据的活动运行时产物。
    let candidate_paths = [
        runtime_index_path(session_dir),
        runtime_dir_path(session_dir),
    ];
    // existing_paths records exact contradictory artifacts for an actionable diagnostic.
    // existing_paths 记录精确的矛盾产物，用于生成可操作诊断。
    let mut existing_paths = Vec::new();
    for path in candidate_paths {
        match fs::symlink_metadata(&path) {
            Ok(_) => existing_paths.push(path),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!(
                    "Failed to inspect runtime artifact `{}` while runtime metadata is absent: {error}",
                    path.display()
                ));
            }
        }
    }
    if existing_paths.is_empty() {
        return Ok(());
    }
    // rendered_paths keeps every conflicting artifact visible without guessing which is authoritative.
    // rendered_paths 保留每个冲突产物，不猜测其中哪个具有权威性。
    let rendered_paths = existing_paths
        .iter()
        .map(|path| vulcan_runtime_assets::native_host_path_text_from_path(path.as_path()))
        .collect::<Vec<_>>()
        .join(", ");
    Err(format!(
        "Runtime metadata is missing while persisted runtime artifacts remain: {rendered_paths}"
    ))
}

/// Returns the process-local cache registry for exact-session runtime system updates.
/// 返回按精确会话保存运行时系统更新的进程内缓存注册表。
fn runtime_system_update_cache() -> &'static Mutex<BTreeMap<PathBuf, RuntimeSystemUpdateCacheEntry>>
{
    RUNTIME_SYSTEM_UPDATE_CACHE.get_or_init(|| Mutex::new(BTreeMap::new()))
}

/// Invalidates one exact resident system-update projection before destructive runtime replacement.
/// 在破坏性替换运行时存储前使一个精确常驻系统更新投影失效。
///
/// # Parameters
/// 参数
/// `session_dir` is the exact deterministic cache key to remove.
/// `session_dir` 是要移除的精确定性缓存键。
pub(in crate::workspace_store) fn invalidate_runtime_system_update_cache(
    session_dir: &Path,
) -> Result<(), String> {
    // cache_guard protects only removal of the one exact cache entry.
    // cache_guard 仅保护单个精确缓存条目的移除。
    let mut cache_guard = runtime_system_update_cache()
        .lock()
        .map_err(to_store_error)?;
    cache_guard.remove(session_dir);
    Ok(())
}

/// Publishes a complete system-update projection for one validated runtime snapshot.
/// 为一个已验证运行时快照发布完整系统更新投影。
///
/// # Parameters
/// 参数
/// `session_dir` is the exact owner directory, `next_seq` is the represented durable frontier,
/// and `lines` is the immutable runtime snapshot used to derive updates.
/// `session_dir` 是精确所属目录，`next_seq` 是所表示的耐久前沿，`lines` 是用于推导更新的不可变运行时快照。
fn publish_runtime_system_update_cache(
    session_dir: &Path,
    next_seq: u64,
    lines: &[RuntimeLogEventLine],
) -> Result<(), String> {
    // updates retain only the small provider-visible audit projection required by later turns.
    // updates 仅保留后续回合需要的小型 Provider 可见审计投影。
    let updates = lines
        .iter()
        .filter_map(|line| match &line.event {
            RuntimeLogEvent::SystemUpdate(event) => Some(event.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    // cache_guard serializes exact-key publication without touching any filesystem path.
    // cache_guard 在不接触任何文件系统路径的情况下串行化精确键发布。
    let mut cache_guard = runtime_system_update_cache()
        .lock()
        .map_err(to_store_error)?;
    if cache_guard
        .get(session_dir)
        .is_some_and(|current| current.next_seq > next_seq)
    {
        return Ok(());
    }
    cache_guard.insert(
        session_dir.to_path_buf(),
        RuntimeSystemUpdateCacheEntry { next_seq, updates },
    );
    Ok(())
}

/// Returns the retained system-update projection without opening runtime storage.
/// 在不打开运行时存储的情况下返回常驻系统更新投影。
///
/// # Parameters
/// 参数
/// `session_dir` is the exact deterministic session directory cache key.
/// `session_dir` 是用作缓存键的精确定性会话目录。
///
/// # Returns
/// 返回值
/// A cloned chronological projection on a cache hit, or `None` before cold recovery.
/// 缓存命中时返回克隆后的按时间顺序投影，冷恢复前返回 `None`。
pub(in crate::workspace_store) fn cached_runtime_system_updates(
    session_dir: &Path,
) -> Result<Option<Vec<RuntimeSystemUpdateEvent>>, String> {
    // cache_guard protects only one small in-memory lookup and clone.
    // cache_guard 仅保护一次小型内存查询与克隆。
    let cache_guard = runtime_system_update_cache()
        .lock()
        .map_err(to_store_error)?;
    Ok(cache_guard
        .get(session_dir)
        .map(|entry| entry.updates.clone()))
}

/// Advances a resident system-update cache after one durable contiguous runtime append.
/// 在一次耐久连续运行时追加后推进常驻系统更新缓存。
///
/// # Parameters
/// 参数
/// `session_dir` identifies the cache owner, `previous_next_seq` is the append precondition,
/// `next_seq` is the durable result, and `tail` contains the newly committed rows.
/// `session_dir` 标识缓存所属者，`previous_next_seq` 是追加前置条件，`next_seq` 是耐久结果，`tail` 包含新提交行。
pub(in crate::workspace_store) fn advance_runtime_system_update_cache(
    session_dir: &Path,
    previous_next_seq: u64,
    next_seq: u64,
    tail: &[RuntimeLogEventLine],
) -> Result<(), String> {
    // cache_guard synchronizes the exact resident projection with its durable sequence frontier.
    // cache_guard 将精确常驻投影与其耐久序号前沿同步。
    let mut cache_guard = runtime_system_update_cache()
        .lock()
        .map_err(to_store_error)?;
    let Some(entry) = cache_guard.get_mut(session_dir) else {
        return Ok(());
    };
    if entry.next_seq != previous_next_seq {
        cache_guard.remove(session_dir);
        return Ok(());
    }
    for line in tail {
        if let RuntimeLogEvent::SystemUpdate(event) = &line.event {
            entry.updates.push(event.clone());
        }
    }
    entry.next_seq = next_seq;
    Ok(())
}

/// Reads provider-facing runtime log lines in metadata segment order.
/// 按元数据分段顺序读取 Provider 可见运行时日志行。
///
/// # Parameters
/// 参数
/// `session_dir` is the segmented session directory whose runtime log is validated and read.
/// `session_dir` 是需要验证并读取运行时日志的分段会话目录。
///
/// # Returns
/// 返回值
/// Ordered lines, `None` only for consistently absent runtime artifacts, or a strict failure.
/// 返回有序日志行；仅在运行时产物一致缺失时返回 `None`，否则返回严格错误。
pub(in crate::workspace_store) fn read_runtime_log_lines(
    session_dir: &Path,
) -> Result<Option<Vec<RuntimeLogEventLine>>, String> {
    // read_started_at measures the complete strict cold-read path.
    // read_started_at 测量完整的严格冷读路径。
    let read_started_at = std::time::Instant::now();
    // captured_snapshot owns immutable bytes copied while the exact session read lock is held.
    // captured_snapshot 持有在精确会话读锁期间复制的不可变字节。
    let captured_snapshot = {
        // session_storage_lock coordinates only this root session during byte capture.
        // session_storage_lock 仅在捕获字节期间协调当前根会话。
        let session_storage_lock = segmented_append_file_io_lock(session_dir);
        // lock_wait_started_at measures contention only for this exact session.
        // lock_wait_started_at 仅测量当前精确会话的锁竞争。
        let lock_wait_started_at = std::time::Instant::now();
        // read_guard prevents a concurrent append from changing metadata or bytes mid-capture.
        // read_guard 防止并发追加在捕获过程中改变元数据或字节。
        let _read_guard = session_storage_lock.read().map_err(to_store_error)?;
        // lock_wait_ms excludes all file metadata and byte reads.
        // lock_wait_ms 不包含任何文件元数据或字节读取。
        let lock_wait_ms = lock_wait_started_at.elapsed().as_millis();
        // lock_hold_started_at measures only the immutable byte-capture transaction.
        // lock_hold_started_at 仅测量不可变字节捕获事务。
        let lock_hold_started_at = std::time::Instant::now();
        // metadata is optional only when every runtime artifact is consistently absent.
        // metadata 仅在所有运行时产物一致缺失时可选。
        let Some(metadata) = read_optional_runtime_metadata(session_dir)? else {
            ensure_runtime_metadata_absence_is_consistent(session_dir)?;
            return Ok(None);
        };
        // segment is unique because runtime metadata validation enforces one fixed segment.
        // segment 具有唯一性，因为运行时元数据校验强制使用一个固定分段。
        let segment = &metadata.segments[0];
        // segment_path is resolved directly from authoritative metadata without enumeration.
        // segment_path 直接从权威元数据解析，不进行枚举。
        let segment_path = session_dir.join(&segment.file);
        // segment_file_metadata rejects links and validates the exact immutable byte boundary.
        // segment_file_metadata 拒绝链接并验证精确不可变字节边界。
        let segment_file_metadata = fs::symlink_metadata(&segment_path).map_err(|error| {
            format!(
                "Failed to inspect required runtime JSONL segment `{}`: {error}",
                segment_path.display()
            )
        })?;
        if segment_file_metadata.file_type().is_symlink() || !segment_file_metadata.is_file() {
            return Err(format!(
                "Required runtime JSONL segment is not a regular file: {}",
                segment_path.display()
            ));
        }
        if segment_file_metadata.len() != segment.byte_len {
            return Err(format!(
                "Runtime segment byte length mismatch: metadata {}, file {}.",
                segment.byte_len,
                segment_file_metadata.len()
            ));
        }
        // bytes performs one whole-file read while the stable metadata boundary is protected.
        // bytes 在稳定元数据边界受保护期间执行一次整文件读取。
        let bytes = fs::read(&segment_path).map_err(|error| {
            format!(
                "Failed to read required runtime JSONL segment `{}`: {error}",
                segment_path.display()
            )
        })?;
        // lock_hold_ms ends before the read guard leaves this capture scope.
        // lock_hold_ms 在读锁离开当前捕获作用域前结束。
        let lock_hold_ms = lock_hold_started_at.elapsed().as_millis();
        (metadata, segment_path, bytes, lock_wait_ms, lock_hold_ms)
    };
    // metadata, segment_path, and bytes are parsed only after the session lock is released.
    // metadata、segment_path 与 bytes 仅在会话锁释放后解析。
    let (metadata, segment_path, bytes, lock_wait_ms, lock_hold_ms) = captured_snapshot;
    // parse_started_at isolates typed JSONL decoding and sequence validation.
    // parse_started_at 单独测量强类型 JSONL 解码与序号校验。
    let parse_started_at = std::time::Instant::now();
    // lines directly decode the immutable bytes without tolerant repair or storage mutation.
    // lines 直接解码不可变字节，不执行容错修复或存储修改。
    let lines = decode_runtime_segment_lines_strict(&segment_path, &bytes)?;
    // strict_read reuses the authoritative sequence validator with an explicitly clean suffix state.
    // strict_read 使用显式无损坏后缀状态复用权威序号校验器。
    let strict_read = RuntimeLogRepairRead {
        lines,
        corrupt_line_numbers: Vec::new(),
    };
    validate_runtime_segment_snapshot(
        &metadata,
        &strict_read,
        u64::try_from(bytes.len())
            .map_err(|_| String::from("Runtime JSONL byte length exceeds u64 capacity."))?,
    )?;
    // parse_ms excludes the session lock and physical file read.
    // parse_ms 不包含会话锁与物理文件读取。
    let parse_ms = parse_started_at.elapsed().as_millis();
    // total_ms provides the exact end-to-end cold-read latency.
    // total_ms 提供精确的冷读端到端延迟。
    let total_ms = read_started_at.elapsed().as_millis();
    if total_ms > 20 {
        eprintln!(
            "[workspace-store][runtime-system-updates] source=cold_disk bytes_read={} lines_parsed={} lock_wait_ms={lock_wait_ms} lock_hold_ms={lock_hold_ms} parse_ms={parse_ms} elapsed_ms={total_ms}",
            bytes.len(),
            strict_read.lines.len()
        );
    }
    publish_runtime_system_update_cache(session_dir, metadata.next_seq, &strict_read.lines)?;
    Ok(Some(strict_read.lines))
}

/// Explicitly repairs a corrupt runtime JSONL suffix and rebuilds its derived index.
/// 显式修复损坏的运行时 JSONL 后缀并重建其派生索引。
///
/// # Parameters
/// 参数
/// `session_dir` is the exact session directory selected by an explicit maintenance action.
/// `session_dir` 是由显式维护操作选择的精确会话目录。
///
/// # Returns
/// 返回值
/// Repaired validated lines, `None` for consistent absence, or a strict maintenance failure.
/// 返回修复并验证后的日志行；一致缺失时返回 `None`，否则返回严格维护失败。
pub(in crate::workspace_store) fn repair_runtime_log_lines(
    session_dir: &Path,
) -> Result<Option<Vec<RuntimeLogEventLine>>, String> {
    // session_storage_lock grants exclusive ownership only for this explicit repair transaction.
    // session_storage_lock 仅为当前显式修复事务授予独占所有权。
    let session_storage_lock = segmented_append_file_io_lock(session_dir);
    // append_guard covers validation, optional repair publication, and derived-index publication.
    // append_guard 覆盖验证、可选修复发布与派生索引发布。
    let append_guard = session_storage_lock.lock().map_err(to_store_error)?;
    let Some(snapshot) = read_runtime_log_snapshot_with_owned_lock(session_dir, &append_guard)?
    else {
        return Ok(None);
    };
    write_runtime_index(session_dir, &rebuild_runtime_log_index(&snapshot.lines))?;
    publish_runtime_system_update_cache(session_dir, snapshot.metadata.next_seq, &snapshot.lines)?;
    Ok(Some(snapshot.lines))
}

/// Reads and repairs one validated runtime snapshot while the caller owns the append lock.
/// 在调用方持有追加锁时读取并修复一个已验证运行时快照。
///
/// # Parameters
/// 参数
/// `session_dir` is the segmented session directory whose runtime log is read.
/// `session_dir` 是需要读取运行时日志的分段会话目录。
/// `append_guard` proves validation and any repair publication share one lock lifetime.
/// `append_guard` 证明验证与任何修复发布共享同一个锁生命周期。
///
/// # Returns
/// 返回值
/// A validated metadata-and-lines snapshot, `None` for consistent absence, or a strict failure.
/// 返回已验证的元数据与日志行快照；一致缺失时返回 `None`，否则返回严格错误。
pub(in crate::workspace_store) fn read_runtime_log_snapshot_with_owned_lock<'guard>(
    session_dir: &Path,
    append_guard: &'guard std::sync::RwLockWriteGuard<'_, ()>,
) -> Result<Option<RuntimeLogSnapshot<'guard>>, String> {
    // metadata is optional, while parse and open failures remain observable.
    // metadata 是可选文档，而解析与打开失败保持可观察。
    let Some(metadata) = read_optional_runtime_metadata(session_dir)? else {
        ensure_runtime_metadata_absence_is_consistent(session_dir)?;
        return Ok(None);
    };
    // segment is unique because optional metadata reads enforce the implemented contract.
    // segment 是唯一分段，因为可选元数据读取会强制执行已实现契约。
    let segment = &metadata.segments[0];
    // segment_path is the validated fixed runtime/r001.jsonl path.
    // segment_path 是已验证的固定 runtime/r001.jsonl 路径。
    let segment_path = session_dir.join(&segment.file);
    // repair_read classifies a valid prefix plus an optional corrupt physical suffix.
    // repair_read 将文件分类为有效前缀与可选损坏物理后缀。
    let repair_read = read_runtime_segment_lines_tolerant(&segment_path)?;
    // actual_byte_len is compared with metadata only when no corruption requires repair.
    // actual_byte_len 仅在不存在待修复损坏时与元数据比较。
    let actual_byte_len = fs::metadata(&segment_path)
        .map_err(|error| {
            format!(
                "Failed to inspect runtime segment `{}`: {error}",
                segment_path.display()
            )
        })?
        .len();
    validate_runtime_segment_snapshot(&metadata, &repair_read, actual_byte_len)?;
    // repair_summary is present only for a physical JSONL suffix that passed all validation.
    // repair_summary 仅在物理 JSONL 后缀通过全部验证后存在。
    let repair_summary = (!repair_read.corrupt_line_numbers.is_empty()).then(|| {
        format!(
            "{} lines {}",
            segment.file,
            compact_runtime_line_number_summary(&repair_read.corrupt_line_numbers)
        )
    });
    if repair_read.lines.is_empty()
        && let Some(repair_summary) = repair_summary.as_deref()
    {
        return Err(format!(
            "Runtime log is fully damaged; no segment was rewritten: {repair_summary}"
        ));
    }
    // lines owns the validated prefix and becomes the returned transactional snapshot.
    // lines 持有已验证前缀，并成为返回的事务快照。
    let mut lines = repair_read.lines;
    // snapshot_metadata is replaced with refreshed metadata only after a repair publication.
    // snapshot_metadata 仅在修复发布后替换为刷新后的元数据。
    let mut snapshot_metadata = metadata;
    if let Some(repair_summary) = repair_summary {
        // repair_detail records the exact source segment and physical line numbers removed.
        // repair_detail 记录被移除的精确源分段与物理行号。
        let repair_detail =
            format!("Removed corrupt runtime JSONL rows from segment: {repair_summary}");
        // repair_line receives a checked sequence immediately after the validated prefix.
        // repair_line 接收紧随已验证前缀之后的受检序号。
        let repair_line = RuntimeLogEventLine::new(
            next_runtime_line_seq(&lines)?,
            current_unix_millis_text(),
            RuntimeLogEvent::RuntimeRepair(RuntimeRepairEvent {
                repair_id: format!(
                    "runtime-repair-{}-{}",
                    current_unix_millis_text(),
                    uuid::Uuid::new_v4()
                ),
                source: String::from("runtime_jsonl_tolerant_read"),
                detail: repair_detail,
                downgraded_tool_segments: 0,
            }),
        );
        lines.push(repair_line);
        invalidate_runtime_system_update_cache(session_dir)?;
        backup_corrupt_runtime_segment(&segment_path)?;
        // lines is encoded completely before one atomic replacement publishes prefix plus repair event.
        // lines 会在一次原子替换发布前缀与修复事件前完成完整编码。
        write_json_lines_file_atomic(&segment_path, &lines)?;
        snapshot_metadata = refresh_runtime_metadata_from_lines_with_owned_lock(
            session_dir,
            &snapshot_metadata,
            &lines,
            append_guard,
        )?;
        write_runtime_index(session_dir, &rebuild_runtime_log_index(&lines))?;
    }
    publish_runtime_system_update_cache(session_dir, snapshot_metadata.next_seq, &lines)?;
    Ok(Some(RuntimeLogSnapshot {
        metadata: snapshot_metadata,
        lines,
        _append_guard: std::marker::PhantomData,
    }))
}

/// Validates persisted runtime lines against authoritative single-segment metadata.
/// 针对权威单分段元数据验证持久化运行时日志行。
///
/// # Parameters
/// 参数
/// `metadata` is the already shape-validated runtime metadata document.
/// `metadata` 是已经完成结构验证的运行时元数据文档。
/// `repair_read` contains the parsed prefix and any corrupt physical suffix line numbers.
/// `repair_read` 包含已解析前缀与任何损坏物理后缀行号。
/// `actual_byte_len` is the current on-disk segment byte length.
/// `actual_byte_len` 是当前磁盘分段字节长度。
///
/// # Returns
/// 返回值
/// Success only when sequences and undamaged metadata fields are strictly consistent.
/// 仅当序号与未损坏元数据字段严格一致时返回成功。
fn validate_runtime_segment_snapshot(
    metadata: &RuntimeLogMetadata,
    repair_read: &RuntimeLogRepairRead,
    actual_byte_len: u64,
) -> Result<(), String> {
    // segment is unique under the validated runtime metadata contract.
    // segment 在已验证运行时元数据契约下具有唯一性。
    let segment = &metadata.segments[0];
    // expected_seq validates every decoded line without max or saturating fallbacks.
    // expected_seq 不使用最大值或饱和回退来验证每条已解码日志行。
    let mut expected_seq = segment.start_seq;
    for (line_index, line) in repair_read.lines.iter().enumerate() {
        if line.schema_version != RUNTIME_LOG_SCHEMA_VERSION {
            return Err(format!(
                "Unsupported runtime line schema version {} at line {}.",
                line.schema_version,
                line_index.saturating_add(1)
            ));
        }
        if line.seq != expected_seq {
            return Err(format!(
                "Runtime line sequence mismatch at line {}: expected {expected_seq}, found {}.",
                line_index.saturating_add(1),
                line.seq
            ));
        }
        expected_seq = line.seq.checked_add(1).ok_or_else(|| {
            format!(
                "Runtime line sequence overflows after line {} with sequence {}.",
                line_index.saturating_add(1),
                line.seq
            )
        })?;
    }
    // valid_line_count converts the in-memory prefix length into persisted metadata units.
    // valid_line_count 将内存前缀长度转换为持久化元数据单位。
    let valid_line_count = u64::try_from(repair_read.lines.len())
        .map_err(|_| String::from("Runtime line count exceeds u64 metadata capacity."))?;
    if !repair_read.corrupt_line_numbers.is_empty() {
        if valid_line_count > segment.line_count {
            return Err(format!(
                "Runtime segment contains {valid_line_count} valid lines but metadata declares only {}.",
                segment.line_count
            ));
        }
        if expected_seq > metadata.next_seq {
            return Err(format!(
                "Runtime valid prefix requires next sequence {expected_seq}, but metadata declares {}.",
                metadata.next_seq
            ));
        }
        return Ok(());
    }
    if valid_line_count != segment.line_count {
        return Err(format!(
            "Runtime segment line count mismatch: metadata {}, file {valid_line_count}.",
            segment.line_count
        ));
    }
    if segment.byte_len != actual_byte_len {
        return Err(format!(
            "Runtime segment byte length mismatch: metadata {}, file {actual_byte_len}.",
            segment.byte_len
        ));
    }
    // actual_end_seq represents the final validated line without guessing for an empty file.
    // actual_end_seq 表示最后一条已验证日志行，空文件时不进行猜测。
    let actual_end_seq = repair_read.lines.last().map(|line| line.seq);
    if segment.end_seq != actual_end_seq {
        return Err(format!(
            "Runtime segment end sequence mismatch: metadata {:?}, file {actual_end_seq:?}.",
            segment.end_seq
        ));
    }
    // actual_updated_at mirrors the exact timestamp of the final validated line.
    // actual_updated_at 镜像最后一条已验证日志行的精确时间戳。
    let actual_updated_at = repair_read
        .lines
        .last()
        .map(|line| line.created_at.as_str());
    if segment.updated_at.as_deref() != actual_updated_at {
        return Err(format!(
            "Runtime segment updated timestamp mismatch: metadata {:?}, file {actual_updated_at:?}.",
            segment.updated_at
        ));
    }
    if metadata.next_seq != expected_seq {
        return Err(format!(
            "Runtime metadata next sequence mismatch: expected {expected_seq}, found {}.",
            metadata.next_seq
        ));
    }
    Ok(())
}

/// Reads the provider-facing runtime log as a canonical conversation state.
/// 将 Provider 可见运行时日志读取为规范会话状态。
pub(in crate::workspace_store) fn read_runtime_conversation_state(
    session_dir: &Path,
) -> Result<Option<SessionConversationState>, String> {
    let Some(lines) = read_runtime_log_lines(session_dir)? else {
        return Ok(None);
    };
    Ok(Some(
        conversation_state_from_runtime_log_lines(&lines).map_err(|error| error.to_string())?,
    ))
}

/// Copies active runtime-log files before context compaction mutates the conversation window.
/// 在上下文压缩修改会话窗口前复制当前运行时日志文件。
pub(in crate::workspace_store) fn record_runtime_injection_applied(
    session_dir: &Path,
    timestamp: &str,
    event: RuntimeInjectionAppliedEvent,
) -> Result<bool, String> {
    append_runtime_log_event_if_present(
        session_dir,
        timestamp,
        RuntimeLogEvent::InjectionApplied(event),
    )
}

/// Records one runtime-only audit event without projecting it into the visible conversation.
/// 记录单个仅运行时审计事件，不将其投影进可见会话。
pub(in crate::workspace_store) fn record_runtime_audit_event(
    session_dir: &Path,
    timestamp: &str,
    event: RuntimeLogEvent,
) -> Result<bool, String> {
    append_runtime_log_event_if_present(session_dir, timestamp, event)
}

/// Appends one runtime event when the session already owns a runtime log.
/// 当会话已经拥有 runtime 日志时追加一条运行时事件。
///
/// # Parameters
/// 参数
/// `session_dir` is the segmented session directory that may own a runtime log.
/// `session_dir` 是可能拥有运行时日志的分段会话目录。
/// `timestamp` is attached to the appended runtime-only event line.
/// `timestamp` 会附加到追加的仅运行时事件日志行。
/// `event` is the runtime event whose sequence is allocated inside the append transaction.
/// `event` 是在追加事务内分配序号的运行时事件。
///
/// # Returns
/// 返回值
/// `true` after append, `false` for absent or empty logs, or a strict persistence failure.
/// 追加后返回 `true`；日志缺失或为空时返回 `false`，否则返回严格持久化错误。
pub(in crate::workspace_store) fn append_runtime_log_event_if_present(
    session_dir: &Path,
    timestamp: &str,
    event: RuntimeLogEvent,
) -> Result<bool, String> {
    // _append_guard serializes the exact metadata check and append without hydrating runtime history.
    // _append_guard 在不加载运行时历史的情况下串行化精确元数据检查与追加。
    let session_storage_lock = segmented_append_file_io_lock(session_dir);
    let _append_guard = session_storage_lock.lock().map_err(to_store_error)?;
    // metadata is the constant-size authoritative append checkpoint for the fixed runtime segment.
    // metadata 是固定运行时分段使用的常量大小权威追加检查点。
    let Some(metadata) = read_optional_runtime_metadata(session_dir)? else {
        ensure_runtime_metadata_absence_is_consistent(session_dir)?;
        return Ok(false);
    };
    if metadata.segments[0].line_count == 0 {
        return Ok(false);
    }
    // line receives the next sequence only after the constant-size append boundary is validated.
    // line 仅在常量大小追加边界完成校验后接收下一序号。
    let line = RuntimeLogEventLine::new(metadata.next_seq, timestamp.to_string(), event);
    append_runtime_log_tail_from_metadata_with_owned_lock(
        session_dir,
        metadata,
        std::slice::from_ref(&line),
        &_append_guard,
    )?;
    Ok(true)
}

/// Appends one runtime event to an already validated caller-owned snapshot.
/// 向一个已经验证且由调用方持有的快照追加一条运行时事件。
///
/// # Parameters
/// 参数
/// `session_dir` is the segmented session directory that owns the runtime snapshot.
/// `session_dir` 是拥有运行时快照的分段会话目录。
/// `snapshot` is the validated metadata-and-lines state tied to `append_guard`.
/// `snapshot` 是与 `append_guard` 绑定的已验证元数据与日志行状态。
/// `timestamp` is attached to the appended runtime-only event line.
/// `timestamp` 会附加到追加的仅运行时事件日志行。
/// `event` is the runtime event whose sequence is allocated from the snapshot.
/// `event` 是从快照分配序号的运行时事件。
/// `append_guard` proves validation and append remain in one lock transaction.
/// `append_guard` 证明验证与追加保持在同一个锁事务内。
///
/// # Returns
/// 返回值
/// `true` after append, or `false` when the validated runtime log is empty.
/// 追加后返回 `true`；已验证运行时日志为空时返回 `false`。
pub(in crate::workspace_store) fn append_runtime_log_event_to_snapshot_with_owned_lock<'guard>(
    session_dir: &Path,
    snapshot: RuntimeLogSnapshot<'guard>,
    timestamp: &str,
    event: RuntimeLogEvent,
    append_guard: &'guard std::sync::RwLockWriteGuard<'_, ()>,
) -> Result<bool, String> {
    if snapshot.lines.is_empty() {
        return Ok(false);
    }
    // line receives the sequence from the validated snapshot while the lock remains owned.
    // line 在锁仍被持有时从已验证快照取得序号。
    let line = RuntimeLogEventLine::new(snapshot.metadata.next_seq, timestamp.to_string(), event);
    append_runtime_log_tail_with_owned_lock(
        session_dir,
        snapshot,
        std::slice::from_ref(&line),
        append_guard,
    )?;
    Ok(true)
}

/// Rebuilds the lightweight runtime index from runtime JSONL lines.
/// 从运行时 JSONL 行重建轻量运行时索引。
pub(in crate::workspace_store) fn rebuild_runtime_log_index(
    lines: &[RuntimeLogEventLine],
) -> RuntimeLogIndex {
    let mut index = RuntimeLogIndex {
        schema_version: 1,
        last_seq: lines.last().map(|line| line.seq),
        line_count: lines.len() as u64,
        latest_compaction_seq: None,
        message_offsets: Default::default(),
        tool_call_offsets: Default::default(),
        provider_checkpoint_offsets: Default::default(),
        repair_offsets: Default::default(),
        injection_offsets: Default::default(),
        system_update_offsets: Default::default(),
        injection_cancelled_offsets: Default::default(),
        injection_merged_offsets: Default::default(),
        provider_response_checkpoint_offsets: Default::default(),
        provider_lowering_repair_offsets: Default::default(),
    };
    for line in lines {
        match &line.event {
            vulcan_context::RuntimeLogEvent::UserMessage(event) => {
                index
                    .message_offsets
                    .insert(event.message_id.clone(), line.seq);
            }
            vulcan_context::RuntimeLogEvent::AssistantMessage(event) => {
                index
                    .message_offsets
                    .insert(event.message_id.clone(), line.seq);
            }
            vulcan_context::RuntimeLogEvent::ToolCall(event) => {
                index
                    .tool_call_offsets
                    .insert(event.runtime_tool_call_id.clone(), line.seq);
                if let Some(provider_tool_call_id) = &event.provider_tool_call_id {
                    index
                        .tool_call_offsets
                        .insert(provider_tool_call_id.clone(), line.seq);
                }
            }
            vulcan_context::RuntimeLogEvent::ToolResult(event) => {
                index
                    .message_offsets
                    .insert(event.message_id.clone(), line.seq);
            }
            vulcan_context::RuntimeLogEvent::ContextCompactionApplied(_) => {
                index.latest_compaction_seq = Some(line.seq);
            }
            vulcan_context::RuntimeLogEvent::ProviderTurnCheckpoint(event) => {
                index.provider_checkpoint_offsets.insert(
                    format!(
                        "{}:{}:{}",
                        event.provider_id, event.model_id, event.converter_version
                    ),
                    line.seq,
                );
            }
            vulcan_context::RuntimeLogEvent::RuntimeRepair(event) => {
                index
                    .repair_offsets
                    .insert(event.repair_id.clone(), line.seq);
            }
            vulcan_context::RuntimeLogEvent::InjectionApplied(event) => {
                if let Some(fingerprint) = event
                    .transient_fingerprint
                    .as_ref()
                    .or(event.persistent_fingerprint.as_ref())
                {
                    index
                        .injection_offsets
                        .insert(fingerprint.clone(), line.seq);
                }
            }
            vulcan_context::RuntimeLogEvent::SystemUpdate(event) => {
                index
                    .system_update_offsets
                    .insert(event.update_id.clone(), line.seq);
            }
            vulcan_context::RuntimeLogEvent::InjectionCancelled(event) => {
                index
                    .injection_cancelled_offsets
                    .insert(event.cancellation_id.clone(), line.seq);
            }
            vulcan_context::RuntimeLogEvent::InjectionMerged(event) => {
                index
                    .injection_merged_offsets
                    .insert(event.merge_id.clone(), line.seq);
            }
            vulcan_context::RuntimeLogEvent::ProviderRequestCheckpoint(event) => {
                index.provider_checkpoint_offsets.insert(
                    format!(
                        "{}:{}:{}:{}",
                        event.provider_family,
                        event.provider_id,
                        event.model_id,
                        event.checkpoint_id
                    ),
                    line.seq,
                );
            }
            vulcan_context::RuntimeLogEvent::ProviderResponseCheckpoint(event) => {
                index.provider_response_checkpoint_offsets.insert(
                    format!(
                        "{}:{}:{}:{}",
                        event.provider_family,
                        event.provider_id,
                        event.model_id,
                        event.checkpoint_id
                    ),
                    line.seq,
                );
            }
            vulcan_context::RuntimeLogEvent::ProviderLoweringRepair(event) => {
                index
                    .provider_lowering_repair_offsets
                    .insert(event.repair_id.clone(), line.seq);
            }
            vulcan_context::RuntimeLogEvent::ToolBatchStarted(_)
            | vulcan_context::RuntimeLogEvent::ToolBatchCompleted(_)
            | vulcan_context::RuntimeLogEvent::VmmLifecycle(_) => {}
        }
    }
    index
}

/// Returns runtime lines that rebuild the canonical provider conversation.
/// 返回用于重建规范 Provider 会话的运行时日志行。
fn runtime_conversation_event_lines(lines: &[RuntimeLogEventLine]) -> Vec<RuntimeLogEventLine> {
    lines
        .iter()
        .filter(|line| runtime_event_projects_to_conversation(&line.event))
        .cloned()
        .collect()
}

/// Returns runtime-only audit lines that must survive conversation projection rebuilds.
/// 返回必须在会话投影重建后继续保留的仅审计运行时日志行。
fn runtime_extra_event_lines(lines: &[RuntimeLogEventLine]) -> Vec<RuntimeLogEventLine> {
    lines
        .iter()
        .filter(|line| !runtime_event_projects_to_conversation(&line.event))
        .cloned()
        .collect()
}

/// Returns non-conversation audit lines at or after one compaction boundary.
/// 返回某个压缩边界及其之后的非会话审计行。
fn runtime_extra_event_lines_from_compaction_boundary(
    lines: &[RuntimeLogEventLine],
    compaction_id: &str,
) -> Vec<RuntimeLogEventLine> {
    let Some(boundary_index) = lines.iter().rposition(|line| {
        matches!(
            &line.event,
            RuntimeLogEvent::ContextCompactionApplied(event)
                if event.compaction_id == compaction_id
        )
    }) else {
        return Vec::new();
    };
    lines[boundary_index..]
        .iter()
        .filter(|line| !runtime_event_projects_to_conversation(&line.event))
        .cloned()
        .collect()
}

/// Returns whether two runtime extra-event slices describe the same retained audit facts.
/// 返回两组运行时额外事件切片是否描述相同的保留审计事实。
fn runtime_extra_event_lines_match(
    left: &[RuntimeLogEventLine],
    right: &[RuntimeLogEventLine],
) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(left, right)| {
            left.created_at == right.created_at
                && serde_json::to_value(&left.event).ok() == serde_json::to_value(&right.event).ok()
        })
}

/// Reports whether one runtime event contributes to canonical conversation replay.
/// 判断单条运行时事件是否参与规范会话回放。
fn runtime_event_projects_to_conversation(event: &RuntimeLogEvent) -> bool {
    matches!(
        event,
        RuntimeLogEvent::UserMessage(_)
            | RuntimeLogEvent::AssistantMessage(_)
            | RuntimeLogEvent::ToolBatchStarted(_)
            | RuntimeLogEvent::ToolCall(_)
            | RuntimeLogEvent::ToolResult(_)
            | RuntimeLogEvent::ToolBatchCompleted(_)
    )
}

/// Appends audit events after conversation lines while assigning fresh sequence numbers.
/// 将审计事件追加到会话日志之后，并分配新的连续序号。
///
/// # Parameters
/// 参数
/// `lines` is the validated conversation-event prefix extended in place.
/// `lines` 是将被原地扩展的已验证会话事件前缀。
/// `extra_events` contains audit-only events whose old sequences are deliberately replaced.
/// `extra_events` 包含旧序号将被有意替换的仅审计事件。
///
/// # Returns
/// 返回值
/// Success after every event receives a checked contiguous sequence, or overflow failure.
/// 每个事件取得受检连续序号后返回成功，序号溢出时返回失败。
fn append_runtime_extra_events_with_fresh_sequences(
    lines: &mut Vec<RuntimeLogEventLine>,
    extra_events: Vec<RuntimeLogEventLine>,
) -> Result<(), String> {
    // next_seq advances through preserved events without saturating into duplicate sequences.
    // next_seq 沿保留事件推进，且不会通过饱和运算产生重复序号。
    let mut next_seq = next_runtime_line_seq(lines)?;
    for event_line in extra_events {
        lines.push(RuntimeLogEventLine::new(
            next_seq,
            event_line.created_at,
            event_line.event,
        ));
        next_seq = next_seq.checked_add(1).ok_or_else(|| {
            format!("Runtime sequence overflows after preserved event {next_seq}.")
        })?;
    }
    Ok(())
}

/// Computes the next runtime sequence number after the provided lines.
/// 根据已提供日志行计算下一个运行时序号。
///
/// # Parameters
/// 参数
/// `lines` is an already validated, sequence-ordered runtime log.
/// `lines` 是已经验证且按序号排序的运行时日志。
///
/// # Returns
/// 返回值
/// Sequence one for an empty log, the checked successor of the final line, or overflow failure.
/// 空日志返回序号一；否则返回最后一行序号的受检后继，溢出时返回失败。
pub(in crate::workspace_store) fn next_runtime_line_seq(
    lines: &[RuntimeLogEventLine],
) -> Result<u64, String> {
    let Some(last_line) = lines.last() else {
        return Ok(1);
    };
    last_line.seq.checked_add(1).ok_or_else(|| {
        format!(
            "Runtime sequence overflows after final sequence {}.",
            last_line.seq
        )
    })
}
