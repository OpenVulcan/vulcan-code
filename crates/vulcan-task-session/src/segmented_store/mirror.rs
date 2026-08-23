use super::*;
use std::collections::{BTreeSet, VecDeque};

/// Appends one task-link event while the caller owns the shared append transaction.
/// 在调用方持有共享追加事务时追加一条子任务关联事件。
///
/// # Parameters
/// 参数
/// `root_session_dir` is the owning root main-session directory.
/// `root_session_dir` 是所属根主会话目录。
/// `event` is the task invocation event projected into the root stream and mirror.
/// `event` 是要投影到根流与镜像中的子任务调用事件。
///
/// # Returns
/// 返回值
/// `true` when root metadata exists and receives the event, or `false` when it is absent.
/// 根元数据存在并接收事件时返回 `true`，不存在时返回 `false`。
pub(super) fn append_main_session_task_link_event_with_owned_append_lock(
    root_session_dir: &Path,
    event: &TaskInvocationEvent,
) -> Result<bool, SessionStoreError> {
    let metadata_path = root_session_dir.join(SEGMENTED_METADATA_FILE);
    // metadata is optional, while non-absence open failures remain observable.
    // metadata 是可选文件，而非缺失类打开错误保持可观察。
    let Some(mut metadata) = read_json_file::<Value>(&metadata_path)? else {
        return Ok(false);
    };
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
    validate_segmented_append_boundary_locked(
        root_session_dir,
        &active_segment,
        &segments,
        next_seq,
    )?;
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

/// Refreshes the root metadata task mirror while the caller owns the shared append lock.
/// 在调用方持有共享追加锁时刷新根元数据任务镜像。
///
/// # Parameters
/// 参数
/// `root_session_dir` is the root main-session directory whose metadata mirror is updated.
/// `root_session_dir` 是要更新元数据镜像的根主会话目录。
/// `document` is the authoritative saved child metadata projection.
/// `document` 是已保存的权威子任务元数据投影。
///
/// # Returns
/// 返回值
/// `true` when root metadata exists and is refreshed, or `false` when the root metadata file is absent.
/// 根元数据存在并完成刷新时返回 `true`，根元数据文件不存在时返回 `false`。
pub(super) fn refresh_main_session_task_mirror_from_metadata_with_owned_append_lock(
    root_session_dir: &Path,
    document: &SegmentedTaskMetadataDocument,
) -> Result<bool, SessionStoreError> {
    let metadata_path = root_session_dir.join(SEGMENTED_METADATA_FILE);
    // metadata uses the shared optional-open contract before mirror mutation.
    // metadata 在镜像修改前使用共享的可选打开契约。
    let Some(mut metadata) = read_json_file::<Value>(&metadata_path)? else {
        return Ok(false);
    };
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
    // metadata separates an absent root document from an unreadable one.
    // metadata 区分缺失的根文档与不可读的根文档。
    let Some(metadata) = read_json_file::<Value>(&metadata_path)? else {
        return Ok(Vec::new());
    };
    let Some(tasks) = metadata.get("tasks").and_then(Value::as_array) else {
        return Ok(Vec::new());
    };
    if tasks.is_empty() {
        return Ok(Vec::new());
    }
    tasks.iter().map(task_mirror_summary_from_value).collect()
}

/// Loads task mirrors reachable from one root main-session parent without building unrelated parent groups.
/// 加载一个根主会话父级可达的子任务镜像，避免构建无关父级分组。
pub(super) fn load_main_session_task_mirror_summaries_by_parent_for_root_tree(
    root_session_dir: &Path,
    root_session_id: RootSessionId,
) -> Result<BTreeMap<ParentSessionRef, Vec<TaskSessionMirrorSummary>>, SessionStoreError> {
    let metadata_path = root_session_dir.join(SEGMENTED_METADATA_FILE);
    // metadata preserves root open failures instead of returning an empty tree.
    // metadata 保留根文档打开失败，而不是返回空树。
    let Some(metadata) = read_json_file::<Value>(&metadata_path)? else {
        return Ok(BTreeMap::new());
    };
    let Some(tasks) = metadata.get("tasks").and_then(Value::as_array) else {
        return Ok(BTreeMap::new());
    };
    if tasks.is_empty() {
        return Ok(BTreeMap::new());
    }
    let task_values_by_parent = root_task_mirror_values_by_parent(tasks, root_session_id)?;
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
            let mirror = task_mirror_summary_from_value(task_value)?;
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

/// Sorts and deduplicates task mirror summaries by stable task-session id.
/// 按稳定子任务会话标识排序并去重子任务镜像摘要。
fn normalize_task_mirror_summaries(mirrors: &mut Vec<TaskSessionMirrorSummary>) {
    mirrors.sort_by_key(|mirror| mirror.task_session_id);
    mirrors.dedup_by_key(|mirror| mirror.task_session_id);
}

/// Converts one root metadata mirror entry into a stable task mirror summary.
/// 将单条根元数据镜像转换为稳定的子任务镜像摘要。
fn task_mirror_summary_from_value(
    task: &Value,
) -> Result<TaskSessionMirrorSummary, SessionStoreError> {
    let task_session_id = required_node_id_from_task_mirror(task, "session_id")?;
    // parent and rootSessionId come only from the authoritative root mirror document.
    // parent 与 rootSessionId 仅来自权威根镜像文档。
    let parent = required_parent_ref_from_task_mirror(task)?;
    let root_session_id = required_root_session_id_from_task_mirror(task)?;
    // publicTaskId and displayTitle must never be guessed from a session id or another field.
    // publicTaskId 与 displayTitle 绝不能根据会话标识或其他字段猜测。
    let public_task_id = required_nonempty_string_from_task_mirror(task, "public_task_id")?;
    let display_title = required_nonempty_string_from_task_mirror(task, "display_title")?;
    let status = task
        .get("status")
        .cloned()
        .map(serde_json::from_value::<TaskSessionStatus>)
        .transpose()
        .map_err(to_store_error)?
        .ok_or_else(|| {
            SessionStoreError::new(format!(
                "Task mirror `{task_session_id}` is missing status."
            ))
        })?;
    // timestamps are required lifecycle facts and cannot inherit each other during initialization.
    // 时间戳是必需生命周期事实，初始化时不能彼此继承。
    let created_at = required_u64_from_task_mirror(task, "created_at")?;
    let updated_at = required_u64_from_task_mirror(task, "updated_at")?;

    Ok(TaskSessionMirrorSummary {
        root_session_id,
        task_session_id,
        public_task_id,
        parent,
        display_title,
        description: optional_string_from_task_mirror(task, "description"),
        subagent: task.get("subagent").cloned().unwrap_or(Value::Null),
        status,
        agent_kind: required_nonempty_string_from_task_mirror(task, "agent_kind")?,
        agent_profile: required_value_from_task_mirror(task, "agent_profile")?,
        metadata_file: Some(required_nonempty_string_from_task_mirror(
            task,
            "metadata_file",
        )?),
        active_segment: Some(required_nonempty_string_from_task_mirror(
            task,
            "active_segment",
        )?),
        line_count: required_u64_from_task_mirror(task, "line_count")?,
        created_at,
        updated_at,
        last_result_summary: optional_string_from_task_mirror(task, "last_result_summary"),
        last_error: optional_string_from_task_mirror(task, "last_error"),
    })
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

/// Reads one required non-empty string from an authoritative mirror entry.
/// 从权威镜像条目读取一个必需的非空字符串。
fn required_nonempty_string_from_task_mirror(
    task: &Value,
    field: &str,
) -> Result<String, SessionStoreError> {
    optional_string_from_task_mirror(task, field)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| SessionStoreError::new(format!("Task mirror is missing `{field}`.")))
}

/// Reads one required integer from an authoritative mirror entry.
/// 从权威镜像条目读取一个必需整数。
fn required_u64_from_task_mirror(task: &Value, field: &str) -> Result<u64, SessionStoreError> {
    optional_u64_from_task_mirror(task, field)
        .ok_or_else(|| SessionStoreError::new(format!("Task mirror is missing `{field}`.")))
}

/// Reads one required JSON value from an authoritative mirror entry.
/// 从权威镜像条目读取一个必需 JSON 值。
fn required_value_from_task_mirror(task: &Value, field: &str) -> Result<Value, SessionStoreError> {
    task.get(field)
        .cloned()
        .ok_or_else(|| SessionStoreError::new(format!("Task mirror is missing `{field}`.")))
}
