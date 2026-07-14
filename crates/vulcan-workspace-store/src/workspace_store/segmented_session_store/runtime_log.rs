//! Runtime log persistence for segmented sessions.
//! 分段会话的运行时日志持久化。

use super::*;
use vulcan_context::protocol::validate_session_conversation_record_identities;

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
    let append_guard = segmented_append_file_io_lock()
        .lock()
        .map_err(to_store_error)?;
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
    _append_guard: &std::sync::MutexGuard<'_, ()>,
) -> Result<RuntimeLogMetadata, String> {
    // active_segment is the sole implemented segment selected before destructive work begins.
    // active_segment 是在破坏性操作开始前选定的唯一已实现分段。
    let active_segment = runtime_segment_file_name();
    // lines is fully projected and resequenced before the previous runtime directory is touched.
    // lines 会在接触旧运行时目录前完成完整投影与重新排序。
    let mut lines = runtime_log_lines_from_conversation_state(conversation_state, updated_at, 1)
        .map_err(|error| error.to_string())?;
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
        segments: vec![segment],
    };
    write_runtime_metadata(session_dir, &metadata)?;
    write_runtime_index(session_dir, &rebuild_runtime_log_index(&lines))?;
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
    let append_guard = segmented_append_file_io_lock()
        .lock()
        .map_err(to_store_error)?;
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
    append_guard: &std::sync::MutexGuard<'_, ()>,
) -> Result<RuntimeLogMetadata, String> {
    let desired_lines =
        runtime_log_lines_from_conversation_state(conversation_state, updated_at, 1)
            .map_err(|error| error.to_string())?;
    // snapshot is absent only when every runtime persistence artifact is genuinely absent.
    // snapshot 仅在全部运行时持久化产物真正缺失时为空。
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
    // extra_events preserves audit-only records if synchronization must rebuild.
    // extra_events 在同步必须重建时保留仅审计记录。
    let extra_events = runtime_extra_event_lines(&snapshot.lines);
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
    // tail begins immediately after the validated existing provider projection.
    // tail 紧接在已验证的现有 Provider 投影之后开始。
    let tail = &desired_lines[existing_conversation_lines.len()..];
    append_runtime_log_tail_with_owned_lock(session_dir, snapshot, tail, append_guard)
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
    let append_guard = segmented_append_file_io_lock()
        .lock()
        .map_err(to_store_error)?;
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
    append_guard: &std::sync::MutexGuard<'_, ()>,
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
    // tail begins immediately after the validated existing provider projection.
    // tail 紧接在已验证的现有 Provider 投影之后开始。
    let tail = &desired_lines[existing_conversation_lines.len()..];
    append_runtime_log_tail_with_owned_lock(session_dir, snapshot, tail, append_guard)
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
    // append_guard serializes validation, repair publication, metadata refresh, and appends.
    // append_guard 将验证、修复发布、元数据刷新与追加串行化。
    let append_guard = segmented_append_file_io_lock()
        .lock()
        .map_err(to_store_error)?;
    Ok(
        read_runtime_log_snapshot_with_owned_lock(session_dir, &append_guard)?
            .map(|snapshot| snapshot.lines),
    )
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
    append_guard: &'guard std::sync::MutexGuard<'_, ()>,
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
    // append_guard keeps snapshot validation, sequence allocation, and publication atomic.
    // append_guard 使快照验证、序号分配与发布保持原子化。
    let append_guard = segmented_append_file_io_lock()
        .lock()
        .map_err(to_store_error)?;
    // snapshot is absent only when the session has no runtime artifacts at all.
    // snapshot 仅在会话完全没有运行时产物时为空。
    let Some(snapshot) = read_runtime_log_snapshot_with_owned_lock(session_dir, &append_guard)?
    else {
        return Ok(false);
    };
    append_runtime_log_event_to_snapshot_with_owned_lock(
        session_dir,
        snapshot,
        timestamp,
        event,
        &append_guard,
    )
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
    append_guard: &'guard std::sync::MutexGuard<'_, ()>,
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
            | vulcan_context::RuntimeLogEvent::ToolBatchCompleted(_) => {}
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
