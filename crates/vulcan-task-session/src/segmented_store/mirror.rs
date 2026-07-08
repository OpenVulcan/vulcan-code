use super::*;
use std::collections::{BTreeSet, VecDeque};

pub(super) fn append_main_session_task_link_event(
    root_session_dir: &Path,
    event: &TaskInvocationEvent,
) -> Result<bool, SessionStoreError> {
    let _guard = vulcan_session_core::segmented_file_io::segmented_append_file_io_lock()
        .lock()
        .map_err(to_store_error)?;
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
    next_seq = next_seq.max(next_main_session_sequence_after_existing_lines(
        root_session_dir,
        &segments,
    )?);
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
    let task_metadata_file = format!("{task_dir}/{SEGMENTED_TASK_METADATA_FILE}");
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
    let mut payload = json!({
        "task_session_id": event.task_session_id,
        "task_dir": task_dir,
        "task_active_segment": task_active_segment,
        "task_metadata_file": task_metadata_file,
        "agent_kind": task_projection.agent_kind.clone(),
        "agent_profile": task_projection.agent_profile.clone(),
        "line_count": task_projection.line_count,
        "task_invocation_event": event,
        "status": event.status
    });
    backfill_main_task_link_payload_sequence(&mut payload, next_seq);
    append_segment_event(
        SegmentEventAppendTarget {
            dir: root_session_dir,
            segments: &mut segments,
            active_segment: &mut active_segment,
            next_seq: &mut next_seq,
            segment_policy: &segment_policy,
        },
        SegmentEventAppendRequest {
            kind: kind.to_string(),
            timestamp: event.timestamp_ms.to_string(),
            payload,
            turn_index: None,
        },
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

/// Backfills the root JSONL sequence into the nested task-invocation payload.
/// 将根 JSONL 序号回填到嵌套的子任务调用载荷中。
///
/// # Parameters
/// 参数
/// `payload` is the mutable root task-link payload.
/// `payload` 是可变的根任务关联载荷。
/// `assigned_sequence` is the root JSONL sequence assigned by the segmented store.
/// `assigned_sequence` 是分段存储分配的根 JSONL 序号。
fn backfill_main_task_link_payload_sequence(payload: &mut Value, assigned_sequence: u64) {
    if let Some(task_invocation_event) = payload
        .get_mut("task_invocation_event")
        .and_then(Value::as_object_mut)
    {
        task_invocation_event.insert(String::from("sequence"), json!(assigned_sequence));
    }
}

/// Returns the next root-session sequence after the highest persisted segment line.
/// 返回已持久化根会话分段行最大序号之后的下一个序号。
fn next_main_session_sequence_after_existing_lines(
    root_session_dir: &Path,
    segments: &[SegmentMetadata],
) -> Result<u64, SessionStoreError> {
    Ok(
        read_segment_lines_with_owned_append_lock(root_session_dir, segments)?
            .into_iter()
            .map(|line| line.seq)
            .max()
            .map(|seq| seq.saturating_add(1))
            .unwrap_or(0),
    )
}

/// Refreshes the root metadata task mirror from a saved child metadata document without appending a parent event.
/// 从已保存的子任务元数据文档刷新根元数据任务镜像，且不追加父级事件。
pub(super) fn refresh_main_session_task_mirror_from_metadata(
    root_session_dir: &Path,
    document: &SegmentedTaskMetadataDocument,
) -> Result<bool, SessionStoreError> {
    let _guard = vulcan_session_core::segmented_file_io::segmented_append_file_io_lock()
        .lock()
        .map_err(to_store_error)?;
    let metadata_path = root_session_dir.join(SEGMENTED_METADATA_FILE);
    if !metadata_path.exists() {
        return Ok(false);
    }
    let mut metadata = read_required_json::<Value>(&metadata_path)?;
    let task_session = &document.task_session;
    let task_dir = format!(
        "task/{}",
        sanitize_file_component(&task_session.task_session_id.to_string())
    );
    let task_active_segment = format!("{task_dir}/{}", document.active_segment);
    let line_count = document
        .segments
        .iter()
        .map(|segment| segment.line_count)
        .sum();
    let event = TaskInvocationEvent {
        event_id: format!(
            "task-metadata-save-{}-{}",
            task_session.task_session_id, task_session.updated_at
        ),
        root_session_id: task_session.root_session_id,
        parent: task_session.parent.clone(),
        task_session_id: task_session.task_session_id,
        sequence: task_session.updated_at,
        kind: TaskInvocationEventKind::TaskCheckpoint,
        status: task_session.status,
        prompt: None,
        timestamp_ms: task_session.updated_at,
        payload: json!({
            "public_task_id": task_session.public_task_id,
            "display_title": task_session.display_title,
            "description": task_session.description,
            "subagent": {
                "profile_id": task_session.subagent_profile_id,
                "inherit_parent_config": task_session.inherit_parent_config
            },
            "result": task_session.last_result_summary,
            "error": task_session.last_error
        }),
    };
    refresh_main_session_task_mirror(
        &mut metadata,
        &event,
        &task_active_segment,
        line_count,
        &document.agent_kind,
        &document.agent_profile,
    );
    metadata["updated_at"] = Value::String(task_session.updated_at.to_string());
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
    let task_metadata = read_required_task_metadata_document(
        &root_session_dir
            .join(task_dir)
            .join(SEGMENTED_TASK_METADATA_FILE),
    )?;
    let main_metadata = read_task_main_segment_metadata(&root_session_dir.join(task_dir))?;
    Ok(TaskMetadataProjection {
        active_segment: main_metadata.active_segment,
        line_count: main_metadata
            .segments
            .iter()
            .map(|segment| segment.line_count)
            .sum(),
        agent_kind: task_metadata.agent_kind,
        agent_profile: task_metadata.agent_profile,
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
    let task_metadata_file = format!("{task_dir}/{SEGMENTED_TASK_METADATA_FILE}");
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
    let public_task_id = payload
        .and_then(|payload| {
            payload
                .get("public_task_id")
                .or_else(|| payload.get("task_id"))
        })
        .and_then(Value::as_str)
        .unwrap_or(task_session_id.as_str())
        .to_string();
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
    mirror["root_session_id"] = Value::String(event.root_session_id.to_string());
    mirror["public_task_id"] = Value::String(public_task_id);
    mirror["parent"] = serde_json::to_value(&event.parent).unwrap_or(Value::Null);
    mirror["dir"] = Value::String(task_dir);
    mirror["metadata_file"] = Value::String(task_metadata_file);
    mirror["active_segment"] = Value::String(task_active_segment.to_string());
    mirror["status"] = serde_json::to_value(event.status).unwrap_or(Value::Null);
    mirror["last_invocation_kind"] = serde_json::to_value(event.kind).unwrap_or(Value::Null);
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

/// Loads task mirrors from the root metadata without opening child task directories.
/// 从根元数据加载子任务镜像，且不打开子任务目录。
pub(super) fn load_main_session_task_mirror_summaries(
    root_session_dir: &Path,
) -> Result<Vec<TaskSessionMirrorSummary>, SessionStoreError> {
    let metadata_path = root_session_dir.join(SEGMENTED_METADATA_FILE);
    if !metadata_path.exists() {
        return Ok(Vec::new());
    }
    let metadata = read_required_json::<Value>(&metadata_path)?;
    let Some(tasks) = metadata.get("tasks").and_then(Value::as_array) else {
        return Ok(Vec::new());
    };
    if tasks.is_empty() {
        return Ok(Vec::new());
    }
    let invocation_events_by_task = if task_mirrors_require_invocation_event_backfill(tasks) {
        main_session_task_invocation_events_by_task(root_session_dir, &metadata)?
    } else {
        BTreeMap::new()
    };
    tasks
        .iter()
        .map(|task| task_mirror_summary_from_value(task, &invocation_events_by_task))
        .collect()
}

/// Loads task mirrors reachable from one root main-session parent without building unrelated parent groups.
/// 加载一个根主会话父级可达的子任务镜像，避免构建无关父级分组。
pub(super) fn load_main_session_task_mirror_summaries_by_parent_for_root_tree(
    root_session_dir: &Path,
    root_session_id: RootSessionId,
) -> Result<BTreeMap<ParentSessionRef, Vec<TaskSessionMirrorSummary>>, SessionStoreError> {
    let metadata_path = root_session_dir.join(SEGMENTED_METADATA_FILE);
    if !metadata_path.exists() {
        return Ok(BTreeMap::new());
    }
    let metadata = read_required_json::<Value>(&metadata_path)?;
    let Some(tasks) = metadata.get("tasks").and_then(Value::as_array) else {
        return Ok(BTreeMap::new());
    };
    if tasks.is_empty() {
        return Ok(BTreeMap::new());
    }
    if task_mirrors_require_invocation_event_backfill(tasks) {
        return reachable_task_mirror_summaries_by_parent(
            load_main_session_task_mirror_summaries(root_session_dir)?,
            root_session_id,
        );
    }
    let task_values_by_parent = root_task_mirror_values_by_parent(tasks, root_session_id)?;
    let empty_invocation_events = BTreeMap::new();
    let mut mirrors_by_parent = BTreeMap::new();
    let mut visited_parent_refs = BTreeSet::new();
    let mut pending_parent_refs =
        VecDeque::from([ParentSessionRef::MainSession { root_session_id }]);
    while let Some(parent) = pending_parent_refs.pop_front() {
        if !visited_parent_refs.insert(parent.clone()) {
            continue;
        }
        let Some(task_values) = task_values_by_parent.get(&parent) else {
            continue;
        };
        let mut mirrors = Vec::new();
        for task_value in task_values {
            let mirror = task_mirror_summary_from_value(task_value, &empty_invocation_events)?;
            pending_parent_refs.push_back(ParentSessionRef::TaskSession {
                root_session_id,
                parent_task_session_id: mirror.task_session_id,
            });
            mirrors.push(mirror);
        }
        normalize_task_mirror_summaries(&mut mirrors);
        if !mirrors.is_empty() {
            mirrors_by_parent.insert(parent, mirrors);
        }
    }
    Ok(mirrors_by_parent)
}

/// Groups modern root task mirror JSON entries by parent for a single root session.
/// 将现代根任务镜像 JSON 条目按单个根会话的父级分组。
fn root_task_mirror_values_by_parent(
    tasks: &[Value],
    root_session_id: RootSessionId,
) -> Result<BTreeMap<ParentSessionRef, Vec<&Value>>, SessionStoreError> {
    let mut task_values_by_parent: BTreeMap<ParentSessionRef, Vec<&Value>> = BTreeMap::new();
    for task in tasks {
        let task_root_session_id = required_root_session_id_from_task_mirror(task)?;
        if task_root_session_id != root_session_id {
            continue;
        }
        let parent = required_parent_ref_from_task_mirror(task)?;
        task_values_by_parent.entry(parent).or_default().push(task);
    }
    Ok(task_values_by_parent)
}

/// Groups already parsed task mirrors by reachable parent for legacy backfill data.
/// 将已解析的子任务镜像按旧格式补齐数据中的可达父级分组。
fn reachable_task_mirror_summaries_by_parent(
    mirrors: Vec<TaskSessionMirrorSummary>,
    root_session_id: RootSessionId,
) -> Result<BTreeMap<ParentSessionRef, Vec<TaskSessionMirrorSummary>>, SessionStoreError> {
    let mut all_mirrors_by_parent: BTreeMap<ParentSessionRef, Vec<TaskSessionMirrorSummary>> =
        BTreeMap::new();
    for mirror in mirrors {
        if mirror.root_session_id != root_session_id {
            continue;
        }
        all_mirrors_by_parent
            .entry(mirror.parent.clone())
            .or_default()
            .push(mirror);
    }
    let mut mirrors_by_parent = BTreeMap::new();
    let mut visited_parent_refs = BTreeSet::new();
    let mut pending_parent_refs =
        VecDeque::from([ParentSessionRef::MainSession { root_session_id }]);
    while let Some(parent) = pending_parent_refs.pop_front() {
        if !visited_parent_refs.insert(parent.clone()) {
            continue;
        }
        let Some(mut mirrors) = all_mirrors_by_parent.remove(&parent) else {
            continue;
        };
        normalize_task_mirror_summaries(&mut mirrors);
        for mirror in &mirrors {
            pending_parent_refs.push_back(ParentSessionRef::TaskSession {
                root_session_id,
                parent_task_session_id: mirror.task_session_id,
            });
        }
        if !mirrors.is_empty() {
            mirrors_by_parent.insert(parent, mirrors);
        }
    }
    Ok(mirrors_by_parent)
}

/// Sorts and deduplicates task mirror summaries by stable task-session id.
/// 按稳定子任务会话标识排序并去重子任务镜像摘要。
fn normalize_task_mirror_summaries(mirrors: &mut Vec<TaskSessionMirrorSummary>) {
    mirrors.sort_by_key(|mirror| mirror.task_session_id);
    mirrors.dedup_by_key(|mirror| mirror.task_session_id);
}

/// Returns whether mirrors need deterministic data from persisted root invocation events.
/// 返回镜像是否需要从持久化根调用事件补齐确定性数据。
fn task_mirrors_require_invocation_event_backfill(tasks: &[Value]) -> bool {
    tasks.iter().any(|task| {
        task.get("parent").is_none()
            || task.get("public_task_id").is_none()
            || task.get("root_session_id").is_none()
    })
}

/// Builds the latest task invocation event map from root session segments.
/// 从根会话分段构建最新子任务调用事件映射。
fn main_session_task_invocation_events_by_task(
    root_session_dir: &Path,
    root_metadata: &Value,
) -> Result<BTreeMap<TaskSessionId, TaskInvocationEvent>, SessionStoreError> {
    let segments = serde_json::from_value::<Vec<SegmentMetadata>>(
        root_metadata
            .get("segments")
            .cloned()
            .unwrap_or_else(|| json!([])),
    )
    .map_err(to_store_error)?;
    let mut events_by_task = BTreeMap::new();
    for line in read_segment_lines(root_session_dir, &segments)? {
        let Some(value) = line.payload.get("task_invocation_event").cloned() else {
            continue;
        };
        let mut event =
            serde_json::from_value::<TaskInvocationEvent>(value).map_err(to_store_error)?;
        event.sequence = line.seq;
        events_by_task.insert(event.task_session_id, event);
    }
    Ok(events_by_task)
}

/// Converts one root metadata mirror entry into a stable task mirror summary.
/// 将单条根元数据镜像转换为稳定的子任务镜像摘要。
fn task_mirror_summary_from_value(
    task: &Value,
    invocation_events_by_task: &BTreeMap<TaskSessionId, TaskInvocationEvent>,
) -> Result<TaskSessionMirrorSummary, SessionStoreError> {
    let task_session_id = required_node_id_from_task_mirror(task, "session_id")?;
    let invocation_event = invocation_events_by_task.get(&task_session_id);
    let parent = task
        .get("parent")
        .cloned()
        .map(serde_json::from_value::<ParentSessionRef>)
        .transpose()
        .map_err(to_store_error)?
        .or_else(|| invocation_event.map(|event| event.parent.clone()))
        .ok_or_else(|| {
            SessionStoreError::new(format!(
                "Task mirror `{task_session_id}` is missing parent and no root invocation event can backfill it."
            ))
        })?;
    let root_session_id = task
        .get("root_session_id")
        .cloned()
        .map(serde_json::from_value::<RootSessionId>)
        .transpose()
        .map_err(to_store_error)?
        .or_else(|| invocation_event.map(|event| event.root_session_id))
        .or_else(|| root_session_id_from_parent_ref(&parent))
        .ok_or_else(|| {
            SessionStoreError::new(format!(
                "Task mirror `{task_session_id}` is missing root_session_id."
            ))
        })?;
    let public_task_id = optional_string_from_task_mirror(task, "public_task_id")
        .or_else(|| {
            invocation_event
                .and_then(|event| task_invocation_payload_public_task_id(&event.payload))
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_else(|| task_session_id.to_string());
    let display_title = optional_string_from_task_mirror(task, "display_title")
        .or_else(|| {
            invocation_event
                .and_then(|event| event.payload.get("display_title"))
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_else(|| public_task_id.clone());
    let status = task
        .get("status")
        .cloned()
        .map(serde_json::from_value::<TaskSessionStatus>)
        .transpose()
        .map_err(to_store_error)?
        .or_else(|| invocation_event.map(|event| event.status))
        .ok_or_else(|| {
            SessionStoreError::new(format!(
                "Task mirror `{task_session_id}` is missing status."
            ))
        })?;
    let created_at = optional_u64_from_task_mirror(task, "created_at")
        .or_else(|| invocation_event.map(|event| event.timestamp_ms))
        .unwrap_or(0);
    let updated_at = optional_u64_from_task_mirror(task, "updated_at")
        .or_else(|| invocation_event.map(|event| event.timestamp_ms))
        .unwrap_or(created_at);

    Ok(TaskSessionMirrorSummary {
        root_session_id,
        task_session_id,
        public_task_id,
        parent,
        display_title,
        description: optional_string_from_task_mirror(task, "description"),
        subagent: task.get("subagent").cloned().unwrap_or(Value::Null),
        status,
        agent_kind: optional_string_from_task_mirror(task, "agent_kind").unwrap_or_default(),
        agent_profile: task.get("agent_profile").cloned().unwrap_or(Value::Null),
        metadata_file: optional_string_from_task_mirror(task, "metadata_file"),
        active_segment: optional_string_from_task_mirror(task, "active_segment"),
        line_count: optional_u64_from_task_mirror(task, "line_count").unwrap_or(0),
        created_at,
        updated_at,
        last_result_summary: optional_string_from_task_mirror(task, "last_result_summary"),
        last_error: optional_string_from_task_mirror(task, "last_error"),
    })
}

/// Reads the public task id from known task invocation payload shapes.
/// 从已知的子任务调用载荷结构读取公开任务标识符。
fn task_invocation_payload_public_task_id(payload: &Value) -> Option<&Value> {
    payload
        .get("public_task_id")
        .or_else(|| payload.get("task_id"))
}

/// Reads a required task-session identifier from a mirror entry.
/// 从镜像条目读取必需的子任务会话标识符。
fn required_node_id_from_task_mirror(
    task: &Value,
    field: &str,
) -> Result<TaskSessionId, SessionStoreError> {
    let text = task
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| SessionStoreError::new(format!("Task mirror is missing `{field}`.")))?;
    serde_json::from_value::<TaskSessionId>(Value::String(text.to_string())).map_err(to_store_error)
}

/// Reads a required root-session identifier from a mirror entry.
/// 从镜像条目读取必需的根会话标识符。
fn required_root_session_id_from_task_mirror(
    task: &Value,
) -> Result<RootSessionId, SessionStoreError> {
    task.get("root_session_id")
        .cloned()
        .map(serde_json::from_value::<RootSessionId>)
        .transpose()
        .map_err(to_store_error)?
        .ok_or_else(|| SessionStoreError::new("Task mirror is missing `root_session_id`."))
}

/// Reads a required parent reference from a mirror entry.
/// 从镜像条目读取必需的父级引用。
fn required_parent_ref_from_task_mirror(
    task: &Value,
) -> Result<ParentSessionRef, SessionStoreError> {
    task.get("parent")
        .cloned()
        .map(serde_json::from_value::<ParentSessionRef>)
        .transpose()
        .map_err(to_store_error)?
        .ok_or_else(|| SessionStoreError::new("Task mirror is missing `parent`."))
}

/// Reads one optional string from a mirror entry.
/// 从镜像条目读取一个可选字符串。
fn optional_string_from_task_mirror(task: &Value, field: &str) -> Option<String> {
    task.get(field).and_then(Value::as_str).map(str::to_string)
}

/// Reads one optional integer from a mirror entry.
/// 从镜像条目读取一个可选整数。
fn optional_u64_from_task_mirror(task: &Value, field: &str) -> Option<u64> {
    task.get(field).and_then(|value| {
        value
            .as_u64()
            .or_else(|| value.as_str().and_then(|text| text.parse::<u64>().ok()))
    })
}

/// Resolves the root session id carried by a parent reference.
/// 解析父级引用携带的根会话标识符。
fn root_session_id_from_parent_ref(parent: &ParentSessionRef) -> Option<RootSessionId> {
    match parent {
        ParentSessionRef::None => None,
        ParentSessionRef::MainSession { root_session_id }
        | ParentSessionRef::TaskSession {
            root_session_id, ..
        } => Some(*root_session_id),
    }
}
