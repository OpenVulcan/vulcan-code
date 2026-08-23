//! Segmented session fork support.
//! 分段会话分叉支持。

use super::*;

/// Forks one already resolved segmented session directory without searching for the source again.
/// 分叉一个已经解析出的分段会话目录，不再重新搜索源目录。
pub(in crate::workspace_store) fn fork_segmented_session_directory_from_source(
    source_dir: &Path,
    data_root: &Path,
    request: &SegmentedSessionForkRequest,
) -> Result<PathBuf, String> {
    if !source_dir.is_dir() {
        return Err(String::from("ERR_SESSION_FORK_SOURCE_NOT_FOUND"));
    }
    let target_dir = segmented_session_directory(
        data_root,
        false,
        &request.target_project_tag,
        &request.target_created_date,
        &request.target_session_id,
    );
    if target_dir.exists() {
        return Err(String::from("ERR_SESSION_FORK_TARGET_EXISTS"));
    }
    let fork_result = (|| {
        // source_append_guard prevents runtime and display artifacts from crossing source versions.
        // source_append_guard 防止运行时与展示产物跨越不同源版本。
        let source_append_guard = segmented_append_file_io_lock()
            .lock()
            .map_err(to_store_error)?;
        // source_metadata and every copied runtime artifact are captured under one append lock.
        // source_metadata 与每个已复制运行时产物都在同一个追加锁下捕获。
        let source_metadata = copy_segmented_session_tree_without_tasks(source_dir, &target_dir)?;
        // identity_replacements is shared by locked display conversion and later target rewriting.
        // identity_replacements 由持锁展示转换与后续目标改写共享。
        let identity_replacements = forked_identity_replacements(request);
        write_forked_session_identity_events_from_source(
            source_dir,
            &target_dir,
            request,
            &source_metadata,
            &identity_replacements,
        )?;
        drop(source_append_guard);
        if request.source_anchor_message_id.is_none() {
            rewrite_forked_persisted_identity_artifacts(
                &target_dir,
                &request.target_session_id,
                &request.target_created_at,
                &identity_replacements,
            )?;
        }
        Ok::<(), String>(())
    })();
    if let Err(fork_error) = fork_result {
        match fs::remove_dir_all(&target_dir) {
            Ok(()) => {}
            Err(cleanup_error) if cleanup_error.kind() == std::io::ErrorKind::NotFound => {}
            Err(cleanup_error) => {
                return Err(format!(
                    "Fork failed with `{fork_error}` and incomplete target cleanup failed for `{}`: {cleanup_error}",
                    target_dir.display()
                ));
            }
        }
        return Err(fork_error);
    }
    Ok(target_dir)
}

/// Copies only the parent segmented session tree and creates an empty task root for the fork.
/// 只复制父级分段会话树，并为分叉会话创建空的子任务根目录。
fn copy_segmented_session_tree_without_tasks(
    source_dir: &Path,
    target_dir: &Path,
) -> Result<SegmentedSessionMetadata, String> {
    let metadata = read_session_metadata(source_dir)?;
    create_dir_all(target_dir)?;
    let exact_relative_files = [
        PathBuf::from(SEGMENTED_STATE_FILE),
        PathBuf::from(SEGMENTED_RUNTIME_METADATA_FILE),
        PathBuf::from(SEGMENTED_RUNTIME_INDEX_FILE),
        PathBuf::from(runtime_segment_file_name()),
        PathBuf::from(crate::workspace_store::session_addon_store::SESSION_ADDON_STATE_FILE),
    ];
    for relative_path in exact_relative_files {
        copy_optional_fork_file(source_dir, target_dir, &relative_path)?;
    }
    create_dir_all(&target_dir.join(&metadata.task_dir))?;
    create_dir_all(&target_dir.join(SEGMENTED_BLOB_DIR))?;
    Ok(metadata)
}

/// Copies one exact optional fork artifact without enumerating its parent directory.
/// 无需枚举父目录即可复制一个精确可选分叉工件。
fn copy_optional_fork_file(
    source_dir: &Path,
    target_dir: &Path,
    relative_path: &Path,
) -> Result<(), String> {
    let source_path = source_dir.join(relative_path);
    let target_path = target_dir.join(relative_path);
    match fs::metadata(&source_path) {
        Ok(metadata) if metadata.is_file() => {
            if let Some(parent) = target_path.parent() {
                create_dir_all(parent)?;
            }
            fs::copy(&source_path, &target_path).map_err(to_store_error)?;
        }
        Ok(_) => {
            return Err(format!(
                "Fork source artifact is not a regular file: {}",
                source_path.display()
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(to_store_error(error)),
    }
    Ok(())
}

/// Returns the exact files that a persisted history rewrite may mutate.
/// 返回持久化历史重写可能修改的精确文件集合。
pub(in crate::workspace_store) fn history_rewrite_mutable_relative_paths(
    metadata: &SegmentedSessionMetadata,
) -> Vec<PathBuf> {
    let mut paths = BTreeSet::new();
    paths.insert(PathBuf::from(SEGMENTED_METADATA_FILE));
    paths.insert(PathBuf::from(SEGMENTED_STATE_FILE));
    paths.insert(PathBuf::from(SEGMENTED_RUNTIME_METADATA_FILE));
    paths.insert(PathBuf::from(SEGMENTED_RUNTIME_INDEX_FILE));
    paths.insert(PathBuf::from(runtime_segment_file_name()));
    paths.insert(PathBuf::from(SEGMENTED_VMM_STATE_FILE));
    paths.insert(PathBuf::from(SEGMENTED_VMM_MIGRATION_FILE));
    paths.insert(PathBuf::from(SEGMENTED_VMM_MIGRATION_COMPLETED_FILE));
    paths.insert(PathBuf::from(SEGMENTED_VMM_CHECKPOINT_FILE));
    for segment in &metadata.segments {
        paths.insert(PathBuf::from(&segment.file));
    }
    paths.into_iter().collect()
}

/// Builds target metadata from source metadata before the transformed display segments are attached.
/// 在挂接转换后的展示分段前，根据源元数据构建目标元数据。
fn forked_session_metadata_from_source(
    source_metadata: &SegmentedSessionMetadata,
    request: &SegmentedSessionForkRequest,
    replacements: &BTreeMap<String, String>,
) -> SegmentedSessionMetadata {
    let mut metadata = source_metadata.clone();
    metadata.session_id = request.target_session_id.clone();
    metadata.project_tag = request.target_project_tag.clone();
    metadata.project_id = Some(request.target_project_tag.clone());
    metadata.created_at = request.target_created_at.clone();
    metadata.updated_at = request.target_created_at.clone();
    metadata.archived_at = None;
    metadata.title = Some(request.target_title.clone());
    metadata.title_source = Some(request.target_title_source.clone());
    metadata.tasks.clear();
    metadata.fork_source = None;
    if let Some(message_id) = metadata.last_message_id.as_mut() {
        rewrite_string_identity_tokens(message_id, replacements);
    }
    if let Some(fingerprint) = metadata.last_message_fingerprint.as_mut() {
        rewrite_string_identity_tokens(fingerprint, replacements);
    }
    metadata
}

/// Builds exact identity-token replacements for a forked parent session.
/// 为分叉后的父会话构建精确身份 token 替换表。
fn forked_identity_replacements(request: &SegmentedSessionForkRequest) -> BTreeMap<String, String> {
    let mut replacements = BTreeMap::new();
    replacements.insert(
        request.source_session_id.clone(),
        request.target_session_id.clone(),
    );
    replacements
}

/// Rewrites copied persisted runtime/state artifacts without replaying or dropping events.
/// 在不重放、不丢弃事件的前提下改写已复制的运行态与状态产物。
fn rewrite_forked_persisted_identity_artifacts(
    session_dir: &Path,
    session_id: &str,
    updated_at: &str,
    replacements: &BTreeMap<String, String>,
) -> Result<(), String> {
    rewrite_forked_state_identity_artifact(session_dir, session_id, updated_at, replacements)?;
    rewrite_forked_runtime_identity_artifacts(session_dir, updated_at, replacements)?;
    purge_vmm_state_from_fork_target(session_dir, session_id, updated_at)
}

/// Rewrites state.json ownership fields while preserving copied runtime state content.
/// 改写 state.json 归属字段，同时保留复制来的运行状态内容。
fn rewrite_forked_state_identity_artifact(
    session_dir: &Path,
    session_id: &str,
    updated_at: &str,
    replacements: &BTreeMap<String, String>,
) -> Result<(), String> {
    let Some(mut state_document) = read_state(session_dir)? else {
        return Ok(());
    };
    state_document.session_id = session_id.to_string();
    state_document.updated_at = updated_at.to_string();
    rewrite_json_value_identity_tokens(&mut state_document.runtime_state, replacements);
    rewrite_json_value_identity_tokens(&mut state_document.current_context, replacements);
    rewrite_json_value_identity_tokens(&mut state_document.session_boundary_state, replacements);
    if !state_document.session_boundary_state.is_object() {
        state_document.session_boundary_state = json!({});
    }
    if let Some(boundary_object) = state_document.session_boundary_state.as_object_mut() {
        boundary_object.insert(
            String::from("thread_id"),
            Value::String(session_id.to_string()),
        );
    }
    write_state(session_dir, &state_document)
}

/// Rewrites provider-facing runtime JSONL identity tokens and rebuilds its index.
/// 改写 Provider 可见 runtime JSONL 的身份 token，并重建其索引。
///
/// # Parameters
/// 参数
/// `session_dir` is the unpublished fork target whose copied runtime artifacts are rewritten.
/// `session_dir` 是已复制运行时产物将被改写的未发布分叉目标。
/// `updated_at` is the target fork timestamp recorded as the metadata update time.
/// `updated_at` 是记录为元数据更新时间的目标分叉时间戳。
/// `replacements` maps exact source identity tokens to target identity tokens.
/// `replacements` 将精确源身份 token 映射为目标身份 token。
///
/// # Returns
/// 返回值
/// Success for consistent absence or a fully validated rewrite, otherwise a strict failure.
/// 运行时产物一致缺失或完成完整验证改写时返回成功，否则返回严格失败。
fn rewrite_forked_runtime_identity_artifacts(
    session_dir: &Path,
    updated_at: &str,
    replacements: &BTreeMap<String, String>,
) -> Result<(), String> {
    // append_guard keeps validation, identity rewrite, metadata refresh, and index rebuild atomic.
    // append_guard 使验证、身份改写、元数据刷新与索引重建保持原子化。
    let append_guard = segmented_append_file_io_lock()
        .lock()
        .map_err(to_store_error)?;
    // snapshot is absent only when metadata, index, and runtime directory are all absent.
    // snapshot 仅在元数据、索引与运行时目录全部缺失时为空。
    let Some(snapshot) = read_runtime_log_snapshot_with_owned_lock(session_dir, &append_guard)?
    else {
        return Ok(());
    };
    // metadata comes from the same validated lock transaction used by the rewrite.
    // metadata 来自改写所使用的同一个已验证锁事务。
    let RuntimeLogSnapshot {
        mut metadata,
        lines: _,
        _append_guard: _,
    } = snapshot;
    let mut all_lines = Vec::new();
    for segment in &mut metadata.segments {
        let segment_path = session_dir.join(&segment.file);
        let mut line_values = read_json_lines::<Value>(&segment_path)?;
        let mut changed = false;
        for line_value in &mut line_values {
            if rewrite_json_value_identity_tokens(line_value, replacements) {
                changed = true;
            }
        }
        if changed {
            write_json_lines_file_atomic(&segment_path, &line_values)?;
        }
        let mut typed_segment_lines = Vec::with_capacity(line_values.len());
        for line_value in line_values {
            typed_segment_lines.push(
                serde_json::from_value::<RuntimeLogEventLine>(line_value)
                    .map_err(to_store_error)?,
            );
        }
        segment.line_count = u64::try_from(typed_segment_lines.len())
            .map_err(|_| String::from("Runtime line count exceeds u64 metadata capacity."))?;
        segment.byte_len = fs::metadata(&segment_path)
            .map_err(|error| {
                format!(
                    "Failed to inspect forked runtime segment `{}`: {error}",
                    segment_path.display()
                )
            })?
            .len();
        segment.end_seq = typed_segment_lines.last().map(|line| line.seq);
        segment.updated_at = typed_segment_lines
            .last()
            .map(|line| line.created_at.clone());
        all_lines.extend(typed_segment_lines);
    }
    metadata.next_seq = next_runtime_line_seq(&all_lines)?;
    metadata.updated_at = updated_at.to_string();
    write_runtime_metadata(session_dir, &metadata)?;
    write_runtime_index(session_dir, &rebuild_runtime_log_index(&all_lines))
}

/// Rewrites exact known identity tokens anywhere inside a JSON value.
/// 在 JSON 值内部任意位置改写已知精确身份 token。
pub(in crate::workspace_store) fn rewrite_json_value_identity_tokens(
    value: &mut Value,
    replacements: &BTreeMap<String, String>,
) -> bool {
    match value {
        Value::String(text) => rewrite_string_identity_tokens(text, replacements),
        Value::Array(items) => {
            let mut changed = false;
            for item in items {
                if rewrite_json_value_identity_tokens(item, replacements) {
                    changed = true;
                }
            }
            changed
        }
        Value::Object(object) => {
            let mut changed = false;
            let mut rewritten = serde_json::Map::new();
            for (key, mut child) in std::mem::take(object) {
                let mut next_key = key;
                if rewrite_string_identity_tokens(&mut next_key, replacements) {
                    changed = true;
                }
                if rewrite_json_value_identity_tokens(&mut child, replacements) {
                    changed = true;
                }
                rewritten.insert(next_key, child);
            }
            *object = rewritten;
            changed
        }
        _ => false,
    }
}

/// Rewrites exact known identity tokens inside one string.
/// 改写单个字符串中的已知精确身份 token。
pub(in crate::workspace_store) fn rewrite_string_identity_tokens(
    text: &mut String,
    replacements: &BTreeMap<String, String>,
) -> bool {
    let mut changed = false;
    for (source, target) in replacements {
        if text.contains(source) {
            *text = text.replace(source, target);
            changed = true;
        }
    }
    changed
}

/// Writes transformed display JSONL segments from the source session into the target directory.
/// 将源会话转换后的展示 JSONL 分段写入目标目录。
fn write_forked_session_identity_events_from_source(
    source_dir: &Path,
    target_dir: &Path,
    request: &SegmentedSessionForkRequest,
    source_metadata: &SegmentedSessionMetadata,
    replacements: &BTreeMap<String, String>,
) -> Result<(), String> {
    let mut metadata = forked_session_metadata_from_source(source_metadata, request, replacements);
    let original_segments = source_metadata.segments.clone();
    let mut retained_segments = Vec::new();
    let mut max_retained_seq = None::<u64>;
    let mut latest_rollback_seq = None;
    let mut found_anchor = request.source_anchor_message_id.is_none();
    for segment in original_segments {
        let source_segment_path = source_dir.join(&segment.file);
        let target_segment_path = target_dir.join(&segment.file);
        let lines = read_segment_event_lines(&source_segment_path)?;
        let mut retained_lines = Vec::with_capacity(lines.len());
        for mut line in lines {
            if forked_session_should_drop_subagent_state_event(line.kind) {
                continue;
            }
            let is_anchor = request
                .source_anchor_message_id
                .as_deref()
                .is_some_and(|message_id| segmented_line_message_id(&line) == Some(message_id));
            rewrite_forked_parent_message_task_metadata(&mut line.payload, request);
            rewrite_json_value_identity_tokens(&mut line.payload, replacements);
            match line.kind {
                SegmentedSessionEventKind::SessionMeta => {
                    if let Some(payload) = line.payload.as_object_mut() {
                        payload.insert(
                            String::from("session_id"),
                            Value::String(request.target_session_id.clone()),
                        );
                        payload.insert(
                            String::from("project_tag"),
                            Value::String(request.target_project_tag.clone()),
                        );
                    }
                }
                SegmentedSessionEventKind::SessionBoundary => {
                    if let Some(payload) = line.payload.as_object_mut() {
                        payload.insert(
                            String::from("thread_id"),
                            Value::String(request.target_session_id.clone()),
                        );
                    }
                }
                _ => {}
            }
            if line.kind == SegmentedSessionEventKind::ThreadRolledBack {
                latest_rollback_seq = Some(line.seq);
            }
            retained_lines.push(line);
            if is_anchor {
                found_anchor = true;
                break;
            }
        }
        if !retained_lines.is_empty() {
            if let Some(segment_max_seq) = retained_lines.iter().map(|line| line.seq).max() {
                max_retained_seq = Some(
                    max_retained_seq
                        .map(|current_max_seq| current_max_seq.max(segment_max_seq))
                        .unwrap_or(segment_max_seq),
                );
            }
            retained_segments.push(write_segment_event_lines_atomic_with_metadata(
                &target_segment_path,
                &segment.file,
                &retained_lines,
            )?);
        }
        if found_anchor {
            break;
        }
    }
    if !found_anchor {
        return Err(String::from(
            "ERR_SEGMENTED_SESSION_TRUNCATE_ANCHOR_NOT_FOUND",
        ));
    }
    if retained_segments.is_empty() {
        let empty_file = segment_file_name(1);
        retained_segments.push(write_segment_event_lines_atomic_with_metadata(
            &target_dir.join(&empty_file),
            &empty_file,
            &[],
        )?);
    }
    metadata.segments = retained_segments;
    metadata.active_segment = metadata
        .segments
        .last()
        .map(|segment| segment.file.clone())
        .unwrap_or_else(|| segment_file_name(1));
    metadata.next_global_seq = max_retained_seq
        .map(|seq| seq.saturating_add(1))
        .unwrap_or(1);
    metadata.latest_rollback_seq = latest_rollback_seq;
    metadata.updated_at = request.target_created_at.clone();
    write_metadata(target_dir, &metadata)?;
    Ok(())
}

/// Returns whether a copied parent event is child-agent runtime state instead of durable parent transcript.
/// 返回复制出的父级事件是否属于子代理运行状态，而不是持久父会话正文。
fn forked_session_should_drop_subagent_state_event(kind: SegmentedSessionEventKind) -> bool {
    matches!(
        kind,
        SegmentedSessionEventKind::SubagentTaskStarted
            | SegmentedSessionEventKind::SubagentTaskEventLinked
            | SegmentedSessionEventKind::SubagentTaskCompleted
    )
}

/// Rewrites structured task references carried by one parent transcript message payload.
/// 改写单条父级转录消息载荷中携带的结构化子任务引用。
fn rewrite_forked_parent_message_task_metadata(
    payload: &mut Value,
    request: &SegmentedSessionForkRequest,
) -> bool {
    let Some(metadata) = payload
        .get_mut("message")
        .and_then(Value::as_object_mut)
        .and_then(|message| message.get_mut("metadata"))
        .and_then(Value::as_object_mut)
    else {
        return false;
    };
    let mut changed = false;
    if rewrite_string_field_if_equal(
        metadata,
        "parent_thread_id",
        &request.source_session_id,
        &request.target_session_id,
    ) {
        changed = true;
    }
    if let Some(mailbox_event) = metadata
        .get_mut("subagent_mailbox_event")
        .and_then(Value::as_object_mut)
    {
        if rewrite_string_field_if_equal(
            mailbox_event,
            "parent_thread_id",
            &request.source_session_id,
            &request.target_session_id,
        ) {
            changed = true;
        }
        if let Some(mailbox_payload) = mailbox_event
            .get_mut("payload")
            .and_then(Value::as_object_mut)
        {
            changed |= rewrite_string_field_if_equal(
                mailbox_payload,
                "parent_thread_id",
                &request.source_session_id,
                &request.target_session_id,
            );
        }
    }
    changed
}

/// Rewrites one string field when its current value exactly matches the expected source value.
/// 当当前值精确匹配预期源值时改写一个字符串字段。
fn rewrite_string_field_if_equal(
    object: &mut serde_json::Map<String, Value>,
    key: &str,
    source_value: &str,
    target_value: &str,
) -> bool {
    if object.get(key).and_then(Value::as_str) != Some(source_value) {
        return false;
    }
    object.insert(key.to_string(), Value::String(target_value.to_string()));
    true
}
