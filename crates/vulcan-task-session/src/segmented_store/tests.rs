//! Tests for the segmented TaskSession store layout.
//! 分段子任务会话存储布局测试。

use super::*;
use serde_json::json;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};
use vulcan_session_core::{SessionEffectiveConfig, SessionEventKind, SessionRequestedConfig};

/// Builds a unique temporary root directory for segmented store tests.
/// 为分段存储测试构建唯一临时根目录。
fn test_root(test_name: &str) -> PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let root = std::env::temp_dir()
        .join("vulcan-segmented-task-session-store-tests")
        .join(format!("{test_name}-{suffix}"));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("create temp root");
    root
}

/// Creates the minimal root main-session metadata needed for parent link events.
/// 创建父级关联事件所需的最小根主会话元数据。
fn create_root_session_metadata(root: &Path, root_session_id: SessionNodeId) {
    let metadata = json!({
        "schema_version": 1,
        "session_id": root_session_id.to_string(),
        "project_tag": "project-alpha",
        "created_at": "100",
        "updated_at": "100",
        "project_id": "project-alpha",
        "title": "Root",
        "title_source": "manual",
        "canonical_source": "segmented_test",
        "archived_at": null,
        "active_segment": "s001.jsonl",
        "task_dir": "task",
        "next_global_seq": 1,
        "latest_rollback_seq": null,
        "segments": [
            {
                "file": "s001.jsonl",
                "start_seq": 0,
                "line_count": 1,
                "byte_len": 0,
                "first_timestamp": "100",
                "last_timestamp": "100",
                "first_turn_index": null,
                "last_turn_index": null,
                "last_compaction_seq": null
            }
        ],
        "segment_policy": {
            "max_lines": 3000,
            "max_bytes": 8388608
        },
        "tasks": []
    });
    write_json_file(&root.join(SEGMENTED_METADATA_FILE), &metadata).expect("write root metadata");
    append_json_line(
        &root.join("s001.jsonl"),
        &SegmentedEventLine {
            seq: 0,
            timestamp: "100".to_string(),
            kind: "session_meta".to_string(),
            payload: json!({ "session_id": root_session_id }),
            turn_index: None,
        },
    )
    .expect("write root segment");
}

/// Verifies the segmented store writes task metadata, state, segments, and parent links without old tasks layout files.
/// 验证分段存储会写入子任务元数据、状态、分段和父关联，同时不产生旧 tasks 布局文件。
#[test]
fn segmented_store_round_trips_task_session_artifacts() {
    let root = test_root("segmented_store_round_trips_task_session_artifacts");
    let store = SegmentedTaskSessionStore::new(&root);
    let root_session_id = SessionNodeId::new_v4();
    let task_session_id = SessionNodeId::new_v4();
    create_root_session_metadata(&root, root_session_id);
    let parent = ParentSessionRef::MainSession { root_session_id };
    let mut effective_config = SessionEffectiveConfig::default();
    effective_config.subagent_profile_id = Some("reviewer".to_string());
    let mut metadata = TaskSessionMetadata::new(
        root_session_id,
        task_session_id,
        parent.clone(),
        "review-task",
        "Review task",
        SessionRequestedConfig::default(),
        effective_config,
        1,
        200,
    );
    metadata.description = Some("Inspect persistence".to_string());

    store
        .create_task_session(metadata.clone())
        .expect("create segmented task");
    metadata.status = TaskSessionStatus::Running;
    metadata.updated_at = 250;
    store
        .save_task_session(metadata.clone())
        .expect("save segmented metadata");
    store
        .append_event(SessionEventRecord::new(
            "event-1",
            task_session_id,
            0,
            SessionEventKind::AssistantOutput,
            260,
            json!({ "body": "done" }),
        ))
        .expect("append task event");
    store
        .save_context(SessionContextDocument {
            schema_version: 1,
            node_id: task_session_id,
            provider_id: Some("provider".to_string()),
            model_id: Some("model".to_string()),
            reasoning_effort: None,
            system_prompt_layers: vec!["system".to_string()],
            conversation_state: json!({ "messages": [] }),
            compaction_anchor: None,
            context_budget_fingerprint: None,
            tool_catalog_snapshot: Value::Null,
            provider_request_preview: None,
        })
        .expect("save context");
    store
        .save_runtime_checkpoint(SessionRuntimeCheckpoint {
            node_id: task_session_id,
            status: "running".to_string(),
            last_sequence: 1,
            payload: json!({ "ok": true }),
        })
        .expect("save checkpoint");

    let task_dir = root
        .join(SEGMENTED_TASK_DIR)
        .join(task_session_id.to_string());
    assert!(task_dir.join(SEGMENTED_METADATA_FILE).exists());
    assert!(task_dir.join(SEGMENTED_STATE_FILE).exists());
    assert!(task_dir.join("s001.jsonl").exists());
    assert!(!root.join("tasks").exists());
    let loaded = store
        .load_task_session(task_session_id)
        .expect("load segmented task")
        .expect("task exists");
    assert_eq!(loaded.status, TaskSessionStatus::Running);
    assert_eq!(
        store
            .load_context(task_session_id)
            .expect("load context")
            .and_then(|context| context.provider_id),
        Some("provider".to_string())
    );
    assert_eq!(
        store
            .load_events(task_session_id)
            .expect("load events")
            .len(),
        1
    );
    assert_eq!(
        store
            .list_child_task_session_ids(parent.clone())
            .expect("list child ids"),
        vec![task_session_id]
    );
    assert_eq!(
        store
            .load_latest_event_payload(task_session_id)
            .expect("load latest payload"),
        Some(json!({ "body": "done" }))
    );
    let root_metadata =
        read_required_json::<Value>(&root.join(SEGMENTED_METADATA_FILE)).expect("root metadata");
    assert_eq!(root_metadata["next_global_seq"], 2);
    let task_metadata = read_required_json::<Value>(&task_dir.join(SEGMENTED_METADATA_FILE))
        .expect("task metadata");
    assert_eq!(task_metadata["agent_kind"], "agent_md");
    assert_eq!(task_metadata["agent_profile"]["prompt_source"], "agent_md");
    let task_segment_lines =
        read_json_lines::<Value>(&task_dir.join("s001.jsonl")).expect("read task segment");
    assert_eq!(task_segment_lines[0]["type"], "subagent_task_meta");
    assert!(task_segment_lines[0].get("kind").is_none());
    let task_mirror = root_metadata["tasks"]
        .as_array()
        .and_then(|tasks| tasks.first())
        .expect("task mirror");
    assert_eq!(task_mirror["session_id"], task_session_id.to_string());
    assert_eq!(task_mirror["dir"], format!("task/{}", task_session_id));
    assert_eq!(
        task_mirror["metadata_file"],
        format!("task/{}/metadata.json", task_session_id)
    );
    assert_eq!(task_mirror["display_title"], "Review task");
    assert_eq!(task_mirror["description"], "Inspect persistence");
    assert_eq!(task_mirror["subagent"]["profile_id"], "reviewer");
    assert_eq!(task_mirror["agent_kind"], "agent_md");
    assert_eq!(task_mirror["agent_profile"]["prompt_source"], "agent_md");
    assert_eq!(task_mirror["line_count"], 1);
    assert_eq!(
        task_mirror["active_segment"],
        format!("task/{}/s001.jsonl", task_session_id)
    );
    assert_eq!(
        store
            .list_task_invocation_events(parent)
            .expect("load parent links")
            .len(),
        1
    );
    let root_segment_lines =
        read_json_lines::<Value>(&root.join("s001.jsonl")).expect("read root raw segment");
    assert_eq!(root_segment_lines[0]["type"], "session_meta");
    assert!(root_segment_lines[0].get("kind").is_none());
    assert!(root_segment_lines[1].get("type").is_some());
    assert!(root_segment_lines[1].get("kind").is_none());
    let root_lines =
        read_json_lines::<SegmentedEventLine>(&root.join("s001.jsonl")).expect("read root segment");
    assert_eq!(root_lines[1].seq, 1);
    assert_eq!(
        root_lines[1].payload["task_dir"],
        format!("task/{}", task_session_id)
    );
    assert_eq!(
        root_lines[1].payload["task_active_segment"],
        format!("task/{}/s001.jsonl", task_session_id)
    );
    assert_eq!(
        root_lines[1].payload["task_metadata_file"],
        format!("task/{}/metadata.json", task_session_id)
    );
    assert_eq!(root_lines[1].payload["agent_kind"], "agent_md");
    assert_eq!(
        root_lines[1].payload["agent_profile"]["prompt_source"],
        "agent_md"
    );
    assert_eq!(root_lines[1].payload["line_count"], 1);
}

/// Verifies task metadata writes every required agent field without relying on read repair.
/// 验证子任务元数据会直接写出所有必需代理字段，而不依赖读取修复。
#[test]
fn task_metadata_write_includes_required_agent_fields() {
    let root = test_root("task_metadata_write_includes_required_agent_fields");
    let store = SegmentedTaskSessionStore::new(&root);
    let root_session_id = SessionNodeId::new_v4();
    let task_session_id = SessionNodeId::new_v4();
    create_root_session_metadata(&root, root_session_id);
    let parent = ParentSessionRef::MainSession { root_session_id };
    let mut effective_config = SessionEffectiveConfig::default();
    effective_config.subagent_profile_id = Some("reviewer".to_string());
    let metadata = TaskSessionMetadata::new(
        root_session_id,
        task_session_id,
        parent,
        "review-task",
        "Review task",
        SessionRequestedConfig::default(),
        effective_config,
        1,
        200,
    );
    store.create_task_session(metadata).expect("create task");

    let task_dir = root
        .join(SEGMENTED_TASK_DIR)
        .join(task_session_id.to_string());
    let task_metadata_path = task_dir.join(SEGMENTED_METADATA_FILE);
    let written_metadata =
        read_required_json::<Value>(&task_metadata_path).expect("read written metadata");
    assert_eq!(written_metadata["agent_kind"], "agent_md");
    assert_eq!(
        written_metadata["agent_profile"]["prompt_source"],
        "agent_md"
    );
    assert!(written_metadata.get("task_session").is_some());
    assert!(written_metadata.get("segments").is_some());
    assert!(written_metadata.get("segment_policy").is_some());
}

/// Verifies nested task sessions stay under the root task directory while parent metadata preserves hierarchy.
/// 验证嵌套子代理会话仍位于根 task 目录下，并通过父级元数据保留层级关系。
#[test]
fn nested_task_session_uses_flat_root_task_directory() {
    let root = test_root("nested_task_session_uses_flat_root_task_directory");
    let store = SegmentedTaskSessionStore::new(&root);
    let root_session_id = SessionNodeId::new_v4();
    let parent_task_id = SessionNodeId::new_v4();
    let child_task_id = SessionNodeId::new_v4();
    create_root_session_metadata(&root, root_session_id);

    let parent_ref = ParentSessionRef::MainSession { root_session_id };
    store
        .create_task_session(TaskSessionMetadata::new(
            root_session_id,
            parent_task_id,
            parent_ref,
            "parent-task",
            "Parent task",
            SessionRequestedConfig::default(),
            SessionEffectiveConfig::default(),
            1,
            200,
        ))
        .expect("create parent task");

    let child_parent = ParentSessionRef::TaskSession {
        root_session_id,
        parent_task_session_id: parent_task_id,
    };
    store
        .create_task_session(TaskSessionMetadata::new(
            root_session_id,
            child_task_id,
            child_parent.clone(),
            "child-task",
            "Child task",
            SessionRequestedConfig::default(),
            SessionEffectiveConfig::default(),
            2,
            220,
        ))
        .expect("create child task");

    let flat_child_dir = root
        .join(SEGMENTED_TASK_DIR)
        .join(child_task_id.to_string());
    let nested_child_dir = root
        .join(SEGMENTED_TASK_DIR)
        .join(parent_task_id.to_string())
        .join(SEGMENTED_TASK_DIR)
        .join(child_task_id.to_string());
    assert!(flat_child_dir.join(SEGMENTED_METADATA_FILE).exists());
    assert!(!nested_child_dir.exists());
    assert_eq!(
        store
            .list_child_task_session_ids(child_parent)
            .expect("list nested child ids"),
        vec![child_task_id]
    );
    assert!(
        store
            .load_task_session(child_task_id)
            .expect("load child task")
            .is_some()
    );

    let root_lines =
        read_json_lines::<SegmentedEventLine>(&root.join("s001.jsonl")).expect("read root");
    assert!(
        root_lines.iter().any(|line| {
            line.payload
                .get("task_session_id")
                .and_then(Value::as_str)
                .is_some_and(|value| value == child_task_id.to_string())
        }),
        "root session JSONL must link to nested task sessions"
    );
    let root_metadata =
        read_required_json::<Value>(&root.join(SEGMENTED_METADATA_FILE)).expect("root metadata");
    assert!(
        root_metadata["tasks"].as_array().is_some_and(|tasks| {
            tasks.iter().any(|task| {
                task.get("session_id")
                    .and_then(Value::as_str)
                    .is_some_and(|value| value == child_task_id.to_string())
            })
        }),
        "root metadata task mirror must include nested task sessions"
    );
}

/// Verifies parent task links point at the child task's current active segment after rollover.
/// 验证子任务滚动分段后父级任务关联会指向子任务当前活动分段。
#[test]
fn parent_task_link_uses_current_child_active_segment_after_rollover() {
    let root = test_root("parent_task_link_uses_current_child_active_segment_after_rollover");
    let store = SegmentedTaskSessionStore::new(&root);
    let root_session_id = SessionNodeId::new_v4();
    let task_session_id = SessionNodeId::new_v4();
    create_root_session_metadata(&root, root_session_id);
    let parent = ParentSessionRef::MainSession { root_session_id };
    let metadata = TaskSessionMetadata::new(
        root_session_id,
        task_session_id,
        parent.clone(),
        "long-task",
        "Long task",
        SessionRequestedConfig::default(),
        SessionEffectiveConfig::default(),
        1,
        200,
    );

    store
        .create_task_session(metadata)
        .expect("create segmented task");
    let task_dir = root
        .join(SEGMENTED_TASK_DIR)
        .join(task_session_id.to_string());
    let task_metadata_path = task_dir.join(SEGMENTED_METADATA_FILE);
    let mut task_metadata =
        read_required_json::<SegmentedTaskMetadataDocument>(&task_metadata_path)
            .expect("task metadata");
    task_metadata.segment_policy = SegmentPolicy {
        max_lines: 2,
        max_bytes: DEFAULT_SEGMENT_MAX_BYTES,
    };
    write_json_file(&task_metadata_path, &task_metadata).expect("write task policy");

    store
        .append_event(SessionEventRecord::new(
            "event-1",
            task_session_id,
            0,
            SessionEventKind::AssistantOutput,
            260,
            json!({ "body": "first" }),
        ))
        .expect("append first task event");
    store
        .append_event(SessionEventRecord::new(
            "event-2",
            task_session_id,
            1,
            SessionEventKind::AssistantOutput,
            270,
            json!({ "body": "second" }),
        ))
        .expect("append second task event");
    let task_metadata = read_required_json::<SegmentedTaskMetadataDocument>(&task_metadata_path)
        .expect("task metadata after rollover");
    assert_eq!(task_metadata.active_segment, "s002.jsonl");
    assert!(task_dir.join("s002.jsonl").exists());

    store
        .append_task_invocation_event(
            parent.clone(),
            TaskInvocationEvent {
                event_id: "task-completed".to_string(),
                root_session_id,
                parent: parent.clone(),
                task_session_id,
                sequence: 2,
                kind: TaskInvocationEventKind::TaskCompleted,
                status: TaskSessionStatus::Completed,
                prompt: None,
                timestamp_ms: 300,
                payload: json!({ "result": "done" }),
            },
        )
        .expect("append parent completion link");

    let root_lines =
        read_json_lines::<SegmentedEventLine>(&root.join("s001.jsonl")).expect("read root segment");
    let last_root_line = root_lines.last().expect("parent link event");
    assert_eq!(
        last_root_line.payload["task_active_segment"],
        format!("task/{}/s002.jsonl", task_session_id)
    );
    assert_eq!(last_root_line.payload["line_count"], 3);
    let root_metadata =
        read_required_json::<Value>(&root.join(SEGMENTED_METADATA_FILE)).expect("root metadata");
    let task_mirror = root_metadata["tasks"]
        .as_array()
        .and_then(|tasks| tasks.first())
        .expect("task mirror");
    assert_eq!(
        task_mirror["active_segment"],
        format!("task/{}/s002.jsonl", task_session_id)
    );
    assert_eq!(task_mirror["line_count"], 3);
    assert_eq!(task_mirror["agent_kind"], "main_clone");
    assert!(task_mirror["agent_profile"].is_object());
}

/// Verifies parent task mirrors fail explicitly when the child metadata document is missing.
/// 验证子任务元数据文档缺失时父级任务镜像会显式失败。
#[test]
fn parent_task_link_rejects_missing_child_metadata() {
    let root = test_root("parent_task_link_rejects_missing_child_metadata");
    let root_session_id = SessionNodeId::new_v4();
    let task_session_id = SessionNodeId::new_v4();
    create_root_session_metadata(&root, root_session_id);
    let parent = ParentSessionRef::MainSession { root_session_id };

    let result = append_main_session_task_link_event(
        &root,
        &TaskInvocationEvent {
            event_id: "task-created".to_string(),
            root_session_id,
            parent,
            task_session_id,
            sequence: 0,
            kind: TaskInvocationEventKind::TaskCreated,
            status: TaskSessionStatus::Created,
            prompt: None,
            timestamp_ms: 300,
            payload: json!({ "display_title": "Missing metadata task" }),
        },
    );

    let error = result.expect_err("missing task metadata should fail");
    assert!(error.message.contains("Failed to read child task metadata"));
    assert!(error.message.contains(&task_session_id.to_string()));
    let root_metadata =
        read_required_json::<Value>(&root.join(SEGMENTED_METADATA_FILE)).expect("root metadata");
    assert!(root_metadata["tasks"].as_array().is_some_and(Vec::is_empty));
    let root_lines =
        read_json_lines::<SegmentedEventLine>(&root.join("s001.jsonl")).expect("root lines");
    assert_eq!(root_lines.len(), 1);
}
