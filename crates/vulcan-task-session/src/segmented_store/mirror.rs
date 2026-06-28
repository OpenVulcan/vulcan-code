use super::*;

pub(super) fn append_main_session_task_link_event(
    root_session_dir: &Path,
    event: &TaskInvocationEvent,
) -> Result<bool, SessionStoreError> {
    let metadata_path = root_session_dir.join(SEGMENTED_METADATA_FILE);
    if !metadata_path.exists() {
        return Ok(false);
    }
    let mut metadata = read_required_json::<Value>(&metadata_path)?;
    let mut segments = serde_json::from_value::<Vec<SegmentMetadata>>(
        metadata
            .get("segments")
            .cloned()
            .unwrap_or_else(|| json!([])),
    )
    .map_err(to_store_error)?;
    let mut active_segment = metadata
        .get("active_segment")
        .and_then(Value::as_str)
        .unwrap_or("s001.jsonl")
        .to_string();
    let mut next_seq = metadata
        .get("next_global_seq")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let segment_policy = serde_json::from_value::<SegmentPolicy>(
        metadata
            .get("segment_policy")
            .cloned()
            .unwrap_or_else(|| json!({})),
    )
    .unwrap_or_default();
    let task_dir = format!(
        "task/{}",
        sanitize_file_component(&event.task_session_id.to_string())
    );
    let task_metadata_file = format!("{task_dir}/{SEGMENTED_METADATA_FILE}");
    let task_projection = main_session_task_metadata_projection(root_session_dir, &task_dir)
        .map_err(|error| {
            SessionStoreError::new(format!(
                "Failed to read child task metadata for parent mirror `{}` at `{task_metadata_file}`: {error}",
                event.task_session_id
            ))
        })?;
    let task_active_segment = format!("{task_dir}/{}", task_projection.active_segment);
    let kind = match event.kind {
        TaskInvocationEventKind::TaskCreated | TaskInvocationEventKind::TaskStarted => {
            "subagent_task_started"
        }
        TaskInvocationEventKind::TaskCompleted => "subagent_task_completed",
        _ => "subagent_task_event_linked",
    };
    append_segment_event(
        root_session_dir,
        &mut segments,
        &mut active_segment,
        &mut next_seq,
        &segment_policy,
        kind.to_string(),
        event.timestamp_ms.to_string(),
        json!({
            "task_session_id": event.task_session_id,
            "task_dir": task_dir,
            "task_active_segment": task_active_segment,
            "task_metadata_file": task_metadata_file,
            "agent_kind": task_projection.agent_kind.clone(),
            "agent_profile": task_projection.agent_profile.clone(),
            "line_count": task_projection.line_count,
            "task_invocation_event": event,
            "status": event.status
        }),
        None,
    )?;
    metadata["segments"] = serde_json::to_value(segments).map_err(to_store_error)?;
    metadata["active_segment"] = Value::String(active_segment);
    metadata["next_global_seq"] = Value::from(next_seq);
    metadata["updated_at"] = Value::String(event.timestamp_ms.to_string());
    refresh_main_session_task_mirror(
        &mut metadata,
        event,
        &task_active_segment,
        task_projection.line_count,
        &task_projection.agent_kind,
        &task_projection.agent_profile,
    );
    write_json_file(&metadata_path, &metadata)?;
    Ok(true)
}

/// Parent-facing projection loaded from one child task metadata document.
/// 从单个子任务元数据文档加载的父级可见投影。
struct TaskMetadataProjection {
    /// Active child segment file name relative to the child task directory.
    /// 相对子任务目录的活动子分段文件名。
    active_segment: String,
    /// Total number of JSONL lines across child task segments.
    /// 子任务所有 JSONL 分段的总行数。
    line_count: u64,
    /// Agent source kind recorded by the child task metadata.
    /// 子任务元数据记录的代理来源类型。
    agent_kind: String,
    /// Agent profile payload recorded by the child task metadata.
    /// 子任务元数据记录的代理画像载荷。
    agent_profile: Value,
}

/// Reads the child task metadata projection used by parent-link payloads.
/// 读取父会话关联载荷所需的子任务元数据投影。
fn main_session_task_metadata_projection(
    root_session_dir: &Path,
    task_dir: &str,
) -> Result<TaskMetadataProjection, SessionStoreError> {
    let metadata = read_required_task_metadata_document(
        &root_session_dir
            .join(task_dir)
            .join(SEGMENTED_METADATA_FILE),
    )?;
    Ok(TaskMetadataProjection {
        active_segment: metadata.active_segment,
        line_count: metadata
            .segments
            .iter()
            .map(|segment| segment.line_count)
            .sum(),
        agent_kind: metadata.agent_kind,
        agent_profile: metadata.agent_profile,
    })
}

/// Refreshes the parent metadata task mirror for one task invocation event.
/// 根据一条子任务调用事件刷新父元数据中的任务镜像。
fn refresh_main_session_task_mirror(
    metadata: &mut Value,
    event: &TaskInvocationEvent,
    task_active_segment: &str,
    line_count: u64,
    agent_kind: &str,
    agent_profile: &Value,
) {
    if !metadata.get("tasks").is_some_and(Value::is_array) {
        metadata["tasks"] = json!([]);
    }
    let Some(tasks) = metadata.get_mut("tasks").and_then(Value::as_array_mut) else {
        return;
    };
    let task_session_id = event.task_session_id.to_string();
    let task_dir = format!(
        "task/{}",
        sanitize_file_component(&event.task_session_id.to_string())
    );
    let task_metadata_file = format!("{task_dir}/{SEGMENTED_METADATA_FILE}");
    let existing_index = tasks.iter().position(|task| {
        task.get("session_id")
            .and_then(Value::as_str)
            .is_some_and(|value| value == task_session_id)
    });
    let payload = event.payload.as_object();
    let display_title = payload
        .and_then(|payload| payload.get("display_title"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let description = payload
        .and_then(|payload| payload.get("description"))
        .cloned()
        .unwrap_or(Value::Null);
    let subagent = payload
        .and_then(|payload| payload.get("subagent"))
        .cloned()
        .unwrap_or(Value::Null);
    let result = payload
        .and_then(|payload| payload.get("result"))
        .cloned()
        .unwrap_or(Value::Null);
    let error = payload
        .and_then(|payload| payload.get("error"))
        .cloned()
        .unwrap_or(Value::Null);
    let mut mirror = existing_index
        .and_then(|index| tasks.get(index).cloned())
        .unwrap_or_else(|| {
            json!({
                "session_id": task_session_id,
                "dir": task_dir,
                "metadata_file": task_metadata_file,
                "active_segment": task_active_segment,
                "created_at": event.timestamp_ms.to_string()
            })
        });
    mirror["session_id"] = Value::String(event.task_session_id.to_string());
    mirror["dir"] = Value::String(task_dir);
    mirror["metadata_file"] = Value::String(task_metadata_file);
    mirror["active_segment"] = Value::String(task_active_segment.to_string());
    mirror["status"] = serde_json::to_value(event.status).unwrap_or(Value::Null);
    mirror["agent_kind"] = Value::String(agent_kind.to_string());
    mirror["agent_profile"] = agent_profile.clone();
    mirror["line_count"] = Value::from(line_count);
    mirror["updated_at"] = Value::String(event.timestamp_ms.to_string());
    if let Some(display_title) = display_title {
        mirror["display_title"] = Value::String(display_title);
    }
    mirror["description"] = description;
    mirror["subagent"] = subagent;
    mirror["last_result_summary"] = result;
    mirror["last_error"] = error;
    match existing_index {
        Some(index) => tasks[index] = mirror,
        None => tasks.push(mirror),
    }
}
