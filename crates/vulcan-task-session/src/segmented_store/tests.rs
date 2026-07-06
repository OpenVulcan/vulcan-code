//! Tests for the segmented TaskSession store layout.
//! 分段子任务会话存储布局测试。

use super::*;
use crate::TaskSessionMetadataInit;
use serde_json::{Value, json};
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

/// Verifies root task-link appends repair a main-session JSONL tail that is missing its newline.
/// 验证根任务关联追加会修复缺少尾部换行的主会话 JSONL 行。
#[test]
fn root_task_link_repairs_missing_jsonl_tail_newline() {
    let root = test_root("root_task_link_repairs_missing_jsonl_tail_newline");
    let store = SegmentedTaskSessionStore::new(&root);
    let root_session_id = SessionNodeId::new_v4();
    let task_session_id = SessionNodeId::new_v4();
    create_root_session_metadata(&root, root_session_id);
    let segment_path = root.join("s001.jsonl");
    let mut bytes = fs::read(&segment_path).expect("read root segment bytes");
    assert_eq!(bytes.pop(), Some(b'\n'));
    fs::write(&segment_path, bytes).expect("rewrite root segment without trailing newline");

    store
        .create_task_session(TaskSessionMetadata::new(TaskSessionMetadataInit {
            root_session_id,
            task_session_id,
            parent: ParentSessionRef::MainSession { root_session_id },
            public_task_id: "repair-child".to_string(),
            display_title: "repair-child".to_string(),
            requested_config: SessionRequestedConfig::default(),
            effective_config: SessionEffectiveConfig::default(),
            depth: 1,
            timestamp_ms: 200,
        }))
        .expect("create task session");

    let text = fs::read_to_string(&segment_path).expect("read repaired root segment text");
    let lines = text.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 2);
    assert!(
        lines
            .iter()
            .all(|line| serde_json::from_str::<Value>(line).is_ok())
    );
    let metadata = read_required_json::<Value>(&root.join(SEGMENTED_METADATA_FILE))
        .expect("read root metadata");
    assert_eq!(metadata["next_global_seq"], json!(2));
    assert_eq!(metadata["segments"][0]["line_count"], json!(2));
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
    let effective_config = SessionEffectiveConfig {
        subagent_profile_id: Some("reviewer".to_string()),
        ..Default::default()
    };
    let mut metadata = TaskSessionMetadata::new(TaskSessionMetadataInit {
        root_session_id,
        task_session_id,
        parent: parent.clone(),
        public_task_id: String::from("review-task"),
        display_title: String::from("Review task"),
        requested_config: SessionRequestedConfig::default(),
        effective_config,
        depth: 1,
        timestamp_ms: 200,
    });
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
    assert_eq!(task_mirror["public_task_id"], "review-task");
    assert_eq!(task_mirror["root_session_id"], root_session_id.to_string());
    assert_eq!(
        task_mirror["parent"],
        serde_json::to_value(&parent).expect("serialize parent")
    );
    assert_eq!(task_mirror["description"], "Inspect persistence");
    assert_eq!(task_mirror["subagent"]["profile_id"], "reviewer");
    assert_eq!(task_mirror["agent_kind"], "agent_md");
    assert_eq!(task_mirror["agent_profile"]["prompt_source"], "agent_md");
    assert_eq!(task_mirror["status"], "running");
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

/// Verifies saving child metadata refreshes the root task mirror without appending parent events.
/// 验证保存子任务元数据会刷新根任务镜像，且不会追加父级事件。
#[test]
fn save_task_session_refreshes_root_task_mirror_without_parent_event() {
    let root = test_root("save_task_session_refreshes_root_task_mirror_without_parent_event");
    let store = SegmentedTaskSessionStore::new(&root);
    let root_session_id = SessionNodeId::new_v4();
    let task_session_id = SessionNodeId::new_v4();
    create_root_session_metadata(&root, root_session_id);
    let parent = ParentSessionRef::MainSession { root_session_id };
    let mut metadata = TaskSessionMetadata::new(TaskSessionMetadataInit {
        root_session_id,
        task_session_id,
        parent,
        public_task_id: String::from("closable-task"),
        display_title: String::from("Closable task"),
        requested_config: SessionRequestedConfig::default(),
        effective_config: SessionEffectiveConfig::default(),
        depth: 1,
        timestamp_ms: 200,
    });
    store
        .create_task_session(metadata.clone())
        .expect("create task session");
    let root_lines_before =
        read_json_lines::<SegmentedEventLine>(&root.join("s001.jsonl")).expect("read root before");

    metadata.status = TaskSessionStatus::Archived;
    metadata.stop_reason = Some(String::from("close_agent"));
    metadata.updated_at = 300;
    store
        .save_task_session(metadata)
        .expect("save archived task session");

    let root_metadata =
        read_required_json::<Value>(&root.join(SEGMENTED_METADATA_FILE)).expect("root metadata");
    let task_mirror = root_metadata["tasks"]
        .as_array()
        .and_then(|tasks| tasks.first())
        .expect("task mirror");
    let root_lines_after =
        read_json_lines::<SegmentedEventLine>(&root.join("s001.jsonl")).expect("read root after");

    assert_eq!(task_mirror["session_id"], task_session_id.to_string());
    assert_eq!(task_mirror["status"], "archived");
    assert_eq!(task_mirror["updated_at"], "300");
    assert_eq!(root_lines_after.len(), root_lines_before.len());
}

/// Verifies root task mirrors restore child summaries without reading child metadata files.
/// 验证根任务镜像能在不读取子元数据文件的情况下恢复子任务摘要。
#[test]
fn root_task_mirror_summaries_do_not_require_child_metadata_files() {
    let root = test_root("root_task_mirror_summaries_do_not_require_child_metadata_files");
    let store = SegmentedTaskSessionStore::new(&root);
    let root_session_id = SessionNodeId::new_v4();
    let task_session_id = SessionNodeId::new_v4();
    create_root_session_metadata(&root, root_session_id);
    let parent = ParentSessionRef::MainSession { root_session_id };
    let mut metadata = TaskSessionMetadata::new(TaskSessionMetadataInit {
        root_session_id,
        task_session_id,
        parent: parent.clone(),
        public_task_id: String::from("review-task"),
        display_title: String::from("Review task"),
        requested_config: SessionRequestedConfig::default(),
        effective_config: SessionEffectiveConfig::default(),
        depth: 1,
        timestamp_ms: 100,
    });
    metadata.description = Some("Inspect persistence".to_string());

    store
        .create_task_session(metadata)
        .expect("create task session");
    let task_metadata_path = root
        .join(SEGMENTED_TASK_DIR)
        .join(task_session_id.to_string())
        .join(SEGMENTED_METADATA_FILE);
    std::fs::remove_file(task_metadata_path).expect("remove child metadata");

    let mirrors = store
        .load_task_session_mirror_summaries_by_parent()
        .expect("load task mirrors");
    let parent_mirrors = mirrors.get(&parent).expect("parent mirrors");

    assert_eq!(parent_mirrors.len(), 1);
    assert_eq!(parent_mirrors[0].task_session_id, task_session_id);
    assert_eq!(parent_mirrors[0].public_task_id, "review-task");
    assert_eq!(parent_mirrors[0].display_title, "Review task");
}

/// Verifies focused root-tree mirror loading ignores unrelated roots and includes nested children.
/// 验证聚焦根树镜像读取会忽略无关根会话并包含嵌套子任务。
#[test]
fn focused_root_task_mirror_summaries_include_only_reachable_tree() {
    let root = test_root("focused_root_task_mirror_summaries_include_only_reachable_tree");
    let store = SegmentedTaskSessionStore::new(&root);
    let root_session_id = SessionNodeId::new_v4();
    let direct_task_session_id = SessionNodeId::new_v4();
    let nested_task_session_id = SessionNodeId::new_v4();
    let unrelated_root_session_id = SessionNodeId::new_v4();
    let unrelated_task_session_id = SessionNodeId::new_v4();
    create_root_session_metadata(&root, root_session_id);
    let root_parent = ParentSessionRef::MainSession { root_session_id };
    let nested_parent = ParentSessionRef::TaskSession {
        root_session_id,
        parent_task_session_id: direct_task_session_id,
    };
    let unrelated_parent = ParentSessionRef::MainSession {
        root_session_id: unrelated_root_session_id,
    };
    for (task_session_id, parent, task_root_session_id, title) in [
        (
            direct_task_session_id,
            root_parent.clone(),
            root_session_id,
            "Direct task",
        ),
        (
            nested_task_session_id,
            nested_parent.clone(),
            root_session_id,
            "Nested task",
        ),
        (
            unrelated_task_session_id,
            unrelated_parent,
            unrelated_root_session_id,
            "Unrelated task",
        ),
    ] {
        store
            .create_task_session(TaskSessionMetadata::new(TaskSessionMetadataInit {
                root_session_id: task_root_session_id,
                task_session_id,
                parent,
                public_task_id: title.to_lowercase().replace(' ', "-"),
                display_title: title.to_string(),
                requested_config: SessionRequestedConfig::default(),
                effective_config: SessionEffectiveConfig::default(),
                depth: 1,
                timestamp_ms: 100,
            }))
            .expect("create task session");
    }

    let mirrors = store
        .load_task_session_mirror_summaries_by_parent_for_root_tree(root_session_id)
        .expect("load focused mirrors");

    assert_eq!(mirrors.len(), 2);
    assert_eq!(
        mirrors
            .get(&root_parent)
            .expect("root parent mirrors")
            .iter()
            .map(|mirror| mirror.task_session_id)
            .collect::<Vec<_>>(),
        vec![direct_task_session_id]
    );
    assert_eq!(
        mirrors
            .get(&nested_parent)
            .expect("nested parent mirrors")
            .iter()
            .map(|mirror| mirror.task_session_id)
            .collect::<Vec<_>>(),
        vec![nested_task_session_id]
    );
}

/// Verifies legacy root task mirrors backfill missing parent fields from root invocation events.
/// 验证旧根任务镜像能从根调用事件补齐缺失的父级字段。
#[test]
fn legacy_root_task_mirror_backfills_parent_from_root_invocation_events() {
    let root = test_root("legacy_root_task_mirror_backfills_parent_from_root_invocation_events");
    let store = SegmentedTaskSessionStore::new(&root);
    let root_session_id = SessionNodeId::new_v4();
    let task_session_id = SessionNodeId::new_v4();
    create_root_session_metadata(&root, root_session_id);
    let parent = ParentSessionRef::MainSession { root_session_id };
    let metadata = TaskSessionMetadata::new(TaskSessionMetadataInit {
        root_session_id,
        task_session_id,
        parent: parent.clone(),
        public_task_id: String::from("legacy-review-task"),
        display_title: String::from("Legacy review task"),
        requested_config: SessionRequestedConfig::default(),
        effective_config: SessionEffectiveConfig::default(),
        depth: 1,
        timestamp_ms: 100,
    });

    store
        .create_task_session(metadata)
        .expect("create task session");
    let metadata_path = root.join(SEGMENTED_METADATA_FILE);
    let mut root_metadata =
        read_required_json::<Value>(&metadata_path).expect("read root metadata");
    let task_mirror = root_metadata["tasks"]
        .as_array_mut()
        .and_then(|tasks| tasks.first_mut())
        .expect("task mirror");
    task_mirror
        .as_object_mut()
        .expect("task mirror object")
        .remove("parent");
    task_mirror
        .as_object_mut()
        .expect("task mirror object")
        .remove("public_task_id");
    task_mirror
        .as_object_mut()
        .expect("task mirror object")
        .remove("root_session_id");
    write_json_file(&metadata_path, &root_metadata).expect("write legacy root metadata");
    let task_metadata_path = root
        .join(SEGMENTED_TASK_DIR)
        .join(task_session_id.to_string())
        .join(SEGMENTED_METADATA_FILE);
    std::fs::remove_file(task_metadata_path).expect("remove child metadata");

    let mirrors = store
        .load_task_session_mirror_summaries_by_parent()
        .expect("load legacy task mirrors");
    let parent_mirrors = mirrors.get(&parent).expect("parent mirrors");

    assert_eq!(parent_mirrors.len(), 1);
    assert_eq!(parent_mirrors[0].task_session_id, task_session_id);
    assert_eq!(parent_mirrors[0].public_task_id, "legacy-review-task");
    assert_eq!(parent_mirrors[0].root_session_id, root_session_id);
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
    let effective_config = SessionEffectiveConfig {
        subagent_profile_id: Some("reviewer".to_string()),
        ..Default::default()
    };
    let metadata = TaskSessionMetadata::new(TaskSessionMetadataInit {
        root_session_id,
        task_session_id,
        parent,
        public_task_id: String::from("review-task"),
        display_title: String::from("Review task"),
        requested_config: SessionRequestedConfig::default(),
        effective_config,
        depth: 1,
        timestamp_ms: 200,
    });
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
        .create_task_session(TaskSessionMetadata::new(TaskSessionMetadataInit {
            root_session_id,
            task_session_id: parent_task_id,
            parent: parent_ref,
            public_task_id: String::from("parent-task"),
            display_title: String::from("Parent task"),
            requested_config: SessionRequestedConfig::default(),
            effective_config: SessionEffectiveConfig::default(),
            depth: 1,
            timestamp_ms: 200,
        }))
        .expect("create parent task");

    let child_parent = ParentSessionRef::TaskSession {
        root_session_id,
        parent_task_session_id: parent_task_id,
    };
    store
        .create_task_session(TaskSessionMetadata::new(TaskSessionMetadataInit {
            root_session_id,
            task_session_id: child_task_id,
            parent: child_parent.clone(),
            public_task_id: String::from("child-task"),
            display_title: String::from("Child task"),
            requested_config: SessionRequestedConfig::default(),
            effective_config: SessionEffectiveConfig::default(),
            depth: 2,
            timestamp_ms: 220,
        }))
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

/// Verifies parent-grouped metadata loads direct and nested task sessions with one indexed shape.
/// 验证按父级分组的元数据会以一份索引结构加载直属与嵌套子任务会话。
#[test]
fn load_task_session_metadata_by_parent_groups_direct_and_nested_children() {
    let root = test_root("load_task_session_metadata_by_parent_groups_direct_and_nested_children");
    let store = SegmentedTaskSessionStore::new(&root);
    let root_session_id = SessionNodeId::new_v4();
    let parent_task_id = SessionNodeId::new_v4();
    let sibling_task_id = SessionNodeId::new_v4();
    let child_task_id = SessionNodeId::new_v4();
    create_root_session_metadata(&root, root_session_id);

    let direct_parent = ParentSessionRef::MainSession { root_session_id };
    let nested_parent = ParentSessionRef::TaskSession {
        root_session_id,
        parent_task_session_id: parent_task_id,
    };
    for (task_session_id, parent, public_task_id) in [
        (parent_task_id, direct_parent.clone(), "parent-task"),
        (sibling_task_id, direct_parent.clone(), "sibling-task"),
        (child_task_id, nested_parent.clone(), "child-task"),
    ] {
        store
            .create_task_session(TaskSessionMetadata::new(TaskSessionMetadataInit {
                root_session_id,
                task_session_id,
                parent,
                public_task_id: public_task_id.to_string(),
                display_title: public_task_id.to_string(),
                requested_config: SessionRequestedConfig::default(),
                effective_config: SessionEffectiveConfig::default(),
                depth: 1,
                timestamp_ms: 200,
            }))
            .expect("create grouped task metadata");
    }

    let metadata_by_parent = store
        .load_task_session_metadata_by_parent()
        .expect("load grouped metadata");
    let mut expected_direct_ids = vec![parent_task_id, sibling_task_id];
    expected_direct_ids.sort();

    assert_eq!(
        metadata_by_parent
            .get(&direct_parent)
            .expect("direct parent group")
            .iter()
            .map(|metadata| metadata.task_session_id)
            .collect::<Vec<_>>(),
        expected_direct_ids
    );
    assert_eq!(
        metadata_by_parent
            .get(&nested_parent)
            .expect("nested parent group")
            .iter()
            .map(|metadata| metadata.task_session_id)
            .collect::<Vec<_>>(),
        vec![child_task_id]
    );
    let _ = fs::remove_dir_all(root);
}

/// Verifies direct-parent metadata loading does not allocate an all-parent grouping table.
/// 验证直属父级元数据加载不需要构建全父级分组表。
#[test]
fn load_task_session_metadata_for_parent_returns_only_direct_children() {
    let root = test_root("load_task_session_metadata_for_parent_returns_only_direct_children");
    let store = SegmentedTaskSessionStore::new(&root);
    let root_session_id = SessionNodeId::new_v4();
    let parent_task_id = SessionNodeId::new_v4();
    let sibling_task_id = SessionNodeId::new_v4();
    let child_task_id = SessionNodeId::new_v4();
    create_root_session_metadata(&root, root_session_id);

    let direct_parent = ParentSessionRef::MainSession { root_session_id };
    let nested_parent = ParentSessionRef::TaskSession {
        root_session_id,
        parent_task_session_id: parent_task_id,
    };
    for (task_session_id, parent, public_task_id) in [
        (parent_task_id, direct_parent.clone(), "parent-task"),
        (sibling_task_id, direct_parent.clone(), "sibling-task"),
        (child_task_id, nested_parent.clone(), "child-task"),
    ] {
        store
            .create_task_session(TaskSessionMetadata::new(TaskSessionMetadataInit {
                root_session_id,
                task_session_id,
                parent,
                public_task_id: public_task_id.to_string(),
                display_title: public_task_id.to_string(),
                requested_config: SessionRequestedConfig::default(),
                effective_config: SessionEffectiveConfig::default(),
                depth: 1,
                timestamp_ms: 200,
            }))
            .expect("create parent-filtered task metadata");
    }

    let mut expected_direct_ids = vec![parent_task_id, sibling_task_id];
    expected_direct_ids.sort();
    let direct_ids = store
        .load_task_session_metadata_for_parent(direct_parent)
        .expect("load direct parent metadata")
        .into_iter()
        .map(|metadata| metadata.task_session_id)
        .collect::<Vec<_>>();
    let nested_ids = store
        .load_task_session_metadata_for_parent(nested_parent)
        .expect("load nested parent metadata")
        .into_iter()
        .map(|metadata| metadata.task_session_id)
        .collect::<Vec<_>>();

    assert_eq!(direct_ids, expected_direct_ids);
    assert_eq!(nested_ids, vec![child_task_id]);
    let _ = fs::remove_dir_all(root);
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
    let metadata = TaskSessionMetadata::new(TaskSessionMetadataInit {
        root_session_id,
        task_session_id,
        parent: parent.clone(),
        public_task_id: String::from("long-task"),
        display_title: String::from("Long task"),
        requested_config: SessionRequestedConfig::default(),
        effective_config: SessionEffectiveConfig::default(),
        depth: 1,
        timestamp_ms: 200,
    });

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

/// Verifies root task-link appends follow the highest persisted sequence, not stale metadata.
/// 验证根任务关联追加跟随已持久化最大序号，而不是陈旧元数据。
#[test]
fn root_task_link_uses_highest_existing_sequence_when_metadata_is_stale() {
    let root = test_root("root_task_link_uses_highest_existing_sequence_when_metadata_is_stale");
    let store = SegmentedTaskSessionStore::new(&root);
    let root_session_id = SessionNodeId::new_v4();
    let task_session_id = SessionNodeId::new_v4();
    create_root_session_metadata(&root, root_session_id);
    append_json_line(
        &root.join("s001.jsonl"),
        &SegmentedEventLine {
            seq: 5,
            timestamp: "150".to_string(),
            kind: "session_boundary".to_string(),
            payload: json!({ "kind": "retained_after_rewrite" }),
            turn_index: None,
        },
    )
    .expect("write retained high-sequence root line");
    let metadata_path = root.join(SEGMENTED_METADATA_FILE);
    let mut metadata = read_required_json::<Value>(&metadata_path).expect("read root metadata");
    metadata["next_global_seq"] = json!(1);
    metadata["segments"][0]["line_count"] = json!(2);
    write_json_file(&metadata_path, &metadata).expect("write stale root metadata");

    store
        .create_task_session(TaskSessionMetadata::new(TaskSessionMetadataInit {
            root_session_id,
            task_session_id,
            parent: ParentSessionRef::MainSession { root_session_id },
            public_task_id: "stale-seq-child".to_string(),
            display_title: "stale-seq-child".to_string(),
            requested_config: SessionRequestedConfig::default(),
            effective_config: SessionEffectiveConfig::default(),
            depth: 1,
            timestamp_ms: 200,
        }))
        .expect("create task session");

    let root_lines =
        read_json_lines::<SegmentedEventLine>(&root.join("s001.jsonl")).expect("read root lines");
    let task_link = root_lines.last().expect("task link line");
    assert_eq!(task_link.seq, 6);
    let updated_metadata =
        read_required_json::<Value>(&metadata_path).expect("read updated root metadata");
    assert_eq!(updated_metadata["next_global_seq"], json!(7));
}
