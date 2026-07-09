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
        let source_metadata = copy_segmented_session_tree_without_tasks(&source_dir, &target_dir)?;
        let identity_replacements = forked_identity_replacements(request);
        write_forked_session_identity_events_from_source(
            &source_dir,
            &target_dir,
            request,
            &source_metadata,
            &identity_replacements,
        )?;
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
    if let Err(error) = fork_result {
        let _ = fs::remove_dir_all(&target_dir);
        return Err(error);
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
    let display_segment_files: BTreeSet<&str> = metadata
        .segments
        .iter()
        .map(|segment| segment.file.as_str())
        .collect();
    for entry in fs::read_dir(source_dir).map_err(to_store_error)? {
        let entry = entry.map_err(to_store_error)?;
        let source_path = entry.path();
        let target_path = target_dir.join(entry.file_name());
        let file_type = entry.file_type().map_err(to_store_error)?;
        let file_name = entry.file_name();
        let file_name_text = file_name.to_string_lossy();
        if file_type.is_dir() {
            let directory_name = source_path.file_name().and_then(|name| name.to_str());
            if forked_session_should_skip_top_level_directory(directory_name, &metadata.task_dir) {
                continue;
            }
        }
        if file_type.is_dir() {
            copy_segmented_directory_tree(&source_path, &target_path)?;
        } else if file_type.is_file() {
            if file_name_text == SEGMENTED_METADATA_FILE
                || display_segment_files.contains(file_name_text.as_ref())
            {
                continue;
            }
            fs::copy(&source_path, &target_path).map_err(to_store_error)?;
        }
    }
    create_dir_all(&target_dir.join(&metadata.task_dir))?;
    Ok(metadata)
}

/// Returns whether a top-level source directory should be omitted from a forked session copy.
/// 返回源会话顶层目录是否应从分叉复制中排除。
fn forked_session_should_skip_top_level_directory(
    directory_name: Option<&str>,
    task_dir_name: &str,
) -> bool {
    matches!(
        directory_name,
        Some(name)
            if name == task_dir_name
                || name == SEGMENTED_RUNTIME_BACKUP_DIR
                || name == SEGMENTED_DISPLAY_MAINTENANCE_BACKUP_DIR
    )
}

/// Copies a segmented session directory recursively without changing file contents.
/// 递归复制分段会话目录，且不改变文件内容。
pub(in crate::workspace_store) fn copy_segmented_directory_tree(
    source_dir: &Path,
    target_dir: &Path,
) -> Result<(), String> {
    create_dir_all(target_dir)?;
    for entry in fs::read_dir(source_dir).map_err(to_store_error)? {
        let entry = entry.map_err(to_store_error)?;
        let source_path = entry.path();
        let target_path = target_dir.join(entry.file_name());
        let file_type = entry.file_type().map_err(to_store_error)?;
        if file_type.is_dir() {
            copy_segmented_directory_tree(&source_path, &target_path)?;
        } else if file_type.is_file() {
            fs::copy(&source_path, &target_path).map_err(to_store_error)?;
        }
    }
    Ok(())
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
    rewrite_forked_runtime_identity_artifacts(session_dir, replacements)
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
fn rewrite_forked_runtime_identity_artifacts(
    session_dir: &Path,
    replacements: &BTreeMap<String, String>,
) -> Result<(), String> {
    if !runtime_metadata_path(session_dir).exists() {
        return Ok(());
    }
    let mut metadata = read_runtime_metadata(session_dir)?;
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
        segment.line_count = typed_segment_lines.len() as u64;
        segment.byte_len = fs::metadata(&segment_path)
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        segment.end_seq = typed_segment_lines.last().map(|line| line.seq);
        segment.updated_at = typed_segment_lines
            .last()
            .map(|line| line.created_at.clone());
        all_lines.extend(typed_segment_lines);
    }
    all_lines.sort_by_key(|line| line.seq);
    metadata.next_seq = next_runtime_line_seq(&all_lines);
    metadata.updated_at = all_lines
        .last()
        .map(|line| line.created_at.clone())
        .unwrap_or_else(current_unix_millis_text);
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
