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

/// Verifies optional JSON reads distinguish true absence from non-`NotFound` open failures.
/// 验证可选 JSON 读取会区分真正缺失与非 `NotFound` 打开失败。
#[test]
fn optional_json_read_distinguishes_missing_path_from_open_failure() {
    // root owns the missing-path fixture without affecting other store tests.
    // root 持有缺失路径夹具，且不影响其他存储测试。
    let root = test_root("optional_json_read_distinguishes_open_failures");
    // missing_path never exists and must retain the optional `None` contract.
    // missing_path 从未存在，必须保留可选 `None` 契约。
    let missing_path = root.join("missing.json");
    // missing captures the legitimate absence result from the authoritative open.
    // missing 捕获权威打开操作返回的合法缺失结果。
    let missing = read_json_file::<Value>(&missing_path).expect("missing JSON should be optional");
    // invalid_path contains an embedded NUL that every supported filesystem API rejects.
    // invalid_path 包含所有受支持文件系统 API 都会拒绝的内嵌 NUL。
    let invalid_path = root.join("invalid\0metadata.json");
    // invalid captures a deterministic non-`NotFound` open failure.
    // invalid 捕获一个确定性的非 `NotFound` 打开失败。
    let invalid = read_json_file::<Value>(&invalid_path);

    fs::remove_dir_all(&root).expect("optional JSON test root should be removable");

    assert!(missing.is_none());
    assert!(
        invalid
            .expect_err("invalid JSON path should remain observable")
            .to_string()
            .starts_with("Failed to open optional file `")
    );
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

/// Verifies concurrent JSONL atomic rewrites use independent staging files and publish complete rows.
/// 验证并发 JSONL 原子重写使用独立暂存文件并发布完整记录。
#[test]
fn concurrent_jsonl_atomic_rewrites_publish_complete_files() {
    /// Number of synchronized rewrite attempts issued by each worker.
    /// 每个工作线程执行的同步重写次数。
    const REWRITE_COUNT_PER_WORKER: usize = 16;

    // root isolates the shared JSONL target and its sibling staging files.
    // root 隔离共享 JSONL 目标及其同级暂存文件。
    let root = test_root("concurrent_jsonl_atomic_rewrites_publish_complete_files");
    // segment_path is deliberately rewritten by both workers without an outer append lock.
    // segment_path 由两个工作线程在没有外层追加锁的情况下刻意共同重写。
    let segment_path = root.join("s001.jsonl");
    // first_values form one complete candidate file with two distinguishable rows.
    // first_values 构成包含两条可区分记录的完整候选文件。
    let first_values = vec![
        SegmentedEventLine {
            seq: 1,
            timestamp: String::from("101"),
            kind: String::from("first-a"),
            payload: json!({ "producer": "first", "row": 1 }),
            turn_index: None,
        },
        SegmentedEventLine {
            seq: 2,
            timestamp: String::from("102"),
            kind: String::from("first-b"),
            payload: json!({ "producer": "first", "row": 2 }),
            turn_index: None,
        },
    ];
    // second_values form the other complete candidate file with three distinguishable rows.
    // second_values 构成包含三条可区分记录的另一完整候选文件。
    let second_values = vec![
        SegmentedEventLine {
            seq: 11,
            timestamp: String::from("201"),
            kind: String::from("second-a"),
            payload: json!({ "producer": "second", "row": 1 }),
            turn_index: None,
        },
        SegmentedEventLine {
            seq: 12,
            timestamp: String::from("202"),
            kind: String::from("second-b"),
            payload: json!({ "producer": "second", "row": 2 }),
            turn_index: None,
        },
        SegmentedEventLine {
            seq: 13,
            timestamp: String::from("203"),
            kind: String::from("second-c"),
            payload: json!({ "producer": "second", "row": 3 }),
            turn_index: None,
        },
    ];
    // barrier synchronizes every rewrite iteration to maximize staging-name collision pressure.
    // barrier 同步每轮重写，以最大化暂存名称碰撞压力。
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    // first_path is the first worker's owned handle to the shared target path.
    // first_path 是第一个工作线程独占的共享目标路径句柄。
    let first_path = segment_path.clone();
    // first_barrier participates in every paired rewrite release.
    // first_barrier 参与每次成对重写释放。
    let first_barrier = std::sync::Arc::clone(&barrier);
    // first_worker repeatedly publishes the first complete candidate.
    // first_worker 重复发布第一份完整候选内容。
    let first_worker = std::thread::spawn(move || {
        for _ in 0..REWRITE_COUNT_PER_WORKER {
            first_barrier.wait();
            write_json_lines_file_atomic(&first_path, &first_values)
                .expect("first concurrent JSONL rewrite");
        }
    });
    // second_worker repeatedly publishes the second complete candidate.
    // second_worker 重复发布第二份完整候选内容。
    let second_worker = std::thread::spawn(move || {
        for _ in 0..REWRITE_COUNT_PER_WORKER {
            barrier.wait();
            write_json_lines_file_atomic(&segment_path, &second_values)
                .expect("second concurrent JSONL rewrite");
        }
    });
    first_worker.join().expect("join first JSONL writer");
    second_worker.join().expect("join second JSONL writer");

    // published_lines must equal one whole candidate rather than a truncated or mixed file.
    // published_lines 必须等于某一份完整候选内容，不能是截断或混合文件。
    let published_lines = read_json_lines::<SegmentedEventLine>(&root.join("s001.jsonl"))
        .expect("read concurrently published JSONL");
    // staging_entries capture any leaked unique sibling files after successful publication.
    // staging_entries 捕获成功发布后可能泄漏的唯一同级暂存文件。
    let staging_entries = fs::read_dir(&root)
        .expect("read JSONL root")
        .collect::<Result<Vec<_>, _>>()
        .expect("collect JSONL root entries")
        .into_iter()
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .contains(".vulcan-stage-")
        })
        .collect::<Vec<_>>();
    let _ = fs::remove_dir_all(&root);

    // published_kinds provide an exact, compact identity for the final complete candidate.
    // published_kinds 为最终完整候选内容提供精确而紧凑的身份。
    let published_kinds = published_lines
        .iter()
        .map(|line| line.kind.as_str())
        .collect::<Vec<_>>();
    assert!(
        published_kinds == ["first-a", "first-b"]
            || published_kinds == ["second-a", "second-b", "second-c"]
    );
    assert!(staging_entries.is_empty());
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
    assert!(task_dir.join(SEGMENTED_TASK_METADATA_FILE).exists());
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
    let state_document = read_required_json::<Value>(&task_dir.join(SEGMENTED_STATE_FILE))
        .expect("read task state document");
    assert_eq!(
        state_document["runtime_state"]["active_provider_id"],
        "provider"
    );
    assert_eq!(state_document["runtime_state"]["active_model_id"], "model");
    assert_eq!(
        state_document["current_context"]["conversation_state"],
        json!({ "messages": [] })
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
    let task_main_metadata = read_required_json::<Value>(&task_dir.join(SEGMENTED_METADATA_FILE))
        .expect("task main metadata");
    assert_eq!(
        task_main_metadata["session_id"],
        task_session_id.to_string()
    );
    assert_eq!(
        task_main_metadata["canonical_source"],
        "segmented_task_session"
    );
    assert_eq!(task_main_metadata["next_global_seq"], 2);
    let task_metadata = read_required_json::<Value>(&task_dir.join(SEGMENTED_TASK_METADATA_FILE))
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
        format!("task/{}/task_metadata.json", task_session_id)
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
        format!("task/{}/task_metadata.json", task_session_id)
    );
    assert_eq!(root_lines[1].payload["agent_kind"], "agent_md");
    assert_eq!(
        root_lines[1].payload["agent_profile"]["prompt_source"],
        "agent_md"
    );
    assert_eq!(root_lines[1].payload["line_count"], 1);
}

/// Verifies concurrent context and checkpoint saves preserve both field families in shared state.json.
/// 验证并发上下文与检查点保存会在共享 state.json 中保留两类字段。
#[test]
fn concurrent_state_saves_preserve_context_and_checkpoint() {
    /// Number of writes issued by each concurrent state producer.
    /// 每个并发状态生产者执行的写入次数。
    const WRITE_COUNT_PER_PRODUCER: u64 = 32;

    // root isolates one segmented TaskSession hierarchy.
    // root 隔离一套分段 TaskSession 层级。
    let root = test_root("concurrent_state_saves_preserve_context_and_checkpoint");
    // store owns the shared task state document exercised by both worker threads.
    // store 持有两个 worker 线程共同覆盖的任务状态文档。
    let store = SegmentedTaskSessionStore::new(&root);
    // root_session_id identifies the parent session required by task creation.
    // root_session_id 标识创建任务所需的父会话。
    let root_session_id = SessionNodeId::new_v4();
    // task_session_id identifies the single shared state.json target.
    // task_session_id 标识单个共享 state.json 目标。
    let task_session_id = SessionNodeId::new_v4();
    create_root_session_metadata(&root, root_session_id);
    store
        .create_task_session(TaskSessionMetadata::new(TaskSessionMetadataInit {
            root_session_id,
            task_session_id,
            parent: ParentSessionRef::MainSession { root_session_id },
            public_task_id: String::from("concurrent-state"),
            display_title: String::from("Concurrent state"),
            requested_config: SessionRequestedConfig::default(),
            effective_config: SessionEffectiveConfig::default(),
            depth: 1,
            timestamp_ms: 200,
        }))
        .expect("concurrent state task session should create");

    // barrier releases context and checkpoint producers into the transaction at the same time.
    // barrier 使上下文与检查点生产者同时进入事务。
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    // context_store is the clone used by the context producer.
    // context_store 是上下文生产者使用的克隆。
    let context_store = store.clone();
    // context_barrier participates in the shared simultaneous release point.
    // context_barrier 参与共享的同时释放点。
    let context_barrier = std::sync::Arc::clone(&barrier);
    // context_worker repeatedly updates only the context field family.
    // context_worker 重复更新仅属于上下文的字段族。
    let context_worker = std::thread::spawn(move || {
        context_barrier.wait();
        for revision in 0..WRITE_COUNT_PER_PRODUCER {
            context_store
                .save_context(SessionContextDocument {
                    schema_version: 1,
                    node_id: task_session_id,
                    provider_id: Some(String::from("provider")),
                    model_id: Some(String::from("model")),
                    reasoning_effort: None,
                    system_prompt_layers: Vec::new(),
                    conversation_state: json!({ "revision": revision }),
                    compaction_anchor: None,
                    context_budget_fingerprint: None,
                    tool_catalog_snapshot: Value::Null,
                    provider_request_preview: None,
                })
                .expect("concurrent context save should succeed");
        }
    });
    // checkpoint_store is the clone used by the checkpoint producer.
    // checkpoint_store 是检查点生产者使用的克隆。
    let checkpoint_store = store.clone();
    // checkpoint_worker repeatedly updates only the runtime checkpoint field family.
    // checkpoint_worker 重复更新仅属于运行时检查点的字段族。
    let checkpoint_worker = std::thread::spawn(move || {
        barrier.wait();
        for sequence in 0..WRITE_COUNT_PER_PRODUCER {
            checkpoint_store
                .save_runtime_checkpoint(SessionRuntimeCheckpoint {
                    node_id: task_session_id,
                    status: String::from("running"),
                    last_sequence: sequence,
                    payload: json!({ "sequence": sequence }),
                })
                .expect("concurrent checkpoint save should succeed");
        }
    });

    context_worker
        .join()
        .expect("context producer should not panic");
    checkpoint_worker
        .join()
        .expect("checkpoint producer should not panic");
    // loaded_context proves checkpoint writes did not erase the latest context family.
    // loaded_context 证明检查点写入没有擦除最新上下文字段族。
    let loaded_context = store
        .load_context(task_session_id)
        .expect("concurrent context should load")
        .expect("concurrent context should remain present");
    // loaded_checkpoint proves context writes did not erase the latest checkpoint family.
    // loaded_checkpoint 证明上下文写入没有擦除最新检查点字段族。
    let loaded_checkpoint = store
        .load_runtime_checkpoint(task_session_id)
        .expect("concurrent checkpoint should load")
        .expect("concurrent checkpoint should remain present");
    let _ = fs::remove_dir_all(&root);

    assert_eq!(loaded_context.provider_id.as_deref(), Some("provider"));
    assert_eq!(loaded_checkpoint.status, "running");
    assert_eq!(
        loaded_checkpoint.last_sequence,
        WRITE_COUNT_PER_PRODUCER - 1
    );
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

/// Verifies metadata saves join the same transaction lock used by segmented event appends.
/// 验证元数据保存加入分段事件追加所使用的同一事务锁。
#[test]
fn save_task_session_waits_for_segment_transaction_lock() {
    // root isolates the persistence layout owned by this regression test.
    // root 隔离本回归测试持有的持久化布局。
    let root = test_root("save_task_session_waits_for_segment_transaction_lock");
    // store is the production segmented store handle exercised by the worker.
    // store 是工作线程实际调用的生产分段存储句柄。
    let store = SegmentedTaskSessionStore::new(&root);
    // root_session_id identifies the owning main session.
    // root_session_id 标识所属主会话。
    let root_session_id = SessionNodeId::new_v4();
    // task_session_id identifies the child whose metadata is updated.
    // task_session_id 标识元数据被更新的子任务。
    let task_session_id = SessionNodeId::new_v4();
    create_root_session_metadata(&root, root_session_id);
    // metadata is the authoritative child snapshot passed to save_task_session.
    // metadata 是传给 save_task_session 的权威子任务快照。
    let mut metadata = TaskSessionMetadata::new(TaskSessionMetadataInit {
        root_session_id,
        task_session_id,
        parent: ParentSessionRef::MainSession { root_session_id },
        public_task_id: String::from("transactional-save"),
        display_title: String::from("Transactional save"),
        requested_config: SessionRequestedConfig::default(),
        effective_config: SessionEffectiveConfig::default(),
        depth: 1,
        timestamp_ms: 200,
    });
    store
        .create_task_session(metadata.clone())
        .expect("create task session");
    metadata.status = TaskSessionStatus::Archived;
    metadata.updated_at = 300;

    // transaction_guard deliberately owns the append transaction before the worker starts saving.
    // transaction_guard 在工作线程开始保存前刻意持有追加事务。
    let transaction_guard = vulcan_session_core::segmented_file_io::segmented_append_file_io_lock()
        .lock()
        .expect("lock segmented append transaction");
    // ready_channel proves the worker reached the call boundary before the blocking assertion.
    // ready_channel 证明工作线程在阻塞断言前已到达调用边界。
    let (ready_sender, ready_receiver) = std::sync::mpsc::channel();
    // result_channel carries the save result only after the transaction finishes.
    // result_channel 仅在事务结束后传递保存结果。
    let (result_sender, result_receiver) = std::sync::mpsc::channel();
    // worker_store is an independent store handle sharing the same persistence root.
    // worker_store 是共享同一持久化根目录的独立存储句柄。
    let worker_store = store.clone();
    // worker attempts the metadata transaction while the test owns the shared lock.
    // worker 在测试持有共享锁期间尝试执行元数据事务。
    let worker = std::thread::spawn(move || {
        ready_sender.send(()).expect("announce save attempt");
        // save_result remains blocked until the shared transaction lock is released.
        // save_result 会一直阻塞到共享事务锁被释放。
        let save_result = worker_store.save_task_session(metadata);
        result_sender.send(save_result).expect("send save result");
    });
    ready_receiver.recv().expect("worker reached save boundary");

    // A completed result here would prove save_task_session bypassed the append transaction.
    // 此时若收到完成结果，就证明 save_task_session 绕过了追加事务。
    assert!(matches!(
        result_receiver.recv_timeout(std::time::Duration::from_millis(100)),
        Err(std::sync::mpsc::RecvTimeoutError::Timeout)
    ));
    drop(transaction_guard);

    // save_result proves the transaction completes without recursive-lock deadlock after release.
    // save_result 证明释放锁后事务能够完成，且不存在递归加锁死锁。
    let save_result = result_receiver
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("save should finish after transaction release");
    save_result.expect("save task session");
    worker.join().expect("join save worker");
    // persisted_metadata verifies the committed child metadata is visible after the transaction.
    // persisted_metadata 验证事务完成后可见已提交的子任务元数据。
    let persisted_metadata = store
        .load_task_session(task_session_id)
        .expect("load saved task session")
        .expect("saved task session should exist");
    let _ = fs::remove_dir_all(&root);

    assert_eq!(persisted_metadata.status, TaskSessionStatus::Archived);
    assert_eq!(persisted_metadata.updated_at, 300);
}

/// Verifies concurrent creation of one task id publishes exactly one complete task layout.
/// 验证同一子任务标识的并发创建只会发布一份完整子任务布局。
#[test]
fn concurrent_task_creation_publishes_one_complete_layout() {
    // root isolates the task hierarchy shared by both creation workers.
    // root 隔离两个创建工作线程共享的子任务层级。
    let root = test_root("concurrent_task_creation_publishes_one_complete_layout");
    // store provides the first handle into the shared persistence root.
    // store 提供进入共享持久化根目录的第一个句柄。
    let store = SegmentedTaskSessionStore::new(&root);
    // root_session_id identifies the parent main session.
    // root_session_id 标识父级主会话。
    let root_session_id = SessionNodeId::new_v4();
    // task_session_id is deliberately shared by both creation attempts.
    // task_session_id 由两个创建尝试刻意共享。
    let task_session_id = SessionNodeId::new_v4();
    create_root_session_metadata(&root, root_session_id);
    // metadata is the identical authoritative creation request cloned into both workers.
    // metadata 是克隆给两个工作线程的相同权威创建请求。
    let metadata = TaskSessionMetadata::new(TaskSessionMetadataInit {
        root_session_id,
        task_session_id,
        parent: ParentSessionRef::MainSession { root_session_id },
        public_task_id: String::from("concurrent-create"),
        display_title: String::from("Concurrent create"),
        requested_config: SessionRequestedConfig::default(),
        effective_config: SessionEffectiveConfig::default(),
        depth: 1,
        timestamp_ms: 200,
    });
    // barrier releases both creation requests at the same point.
    // barrier 在同一点释放两个创建请求。
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    // first_store is the independent handle used by the first worker.
    // first_store 是第一个工作线程使用的独立句柄。
    let first_store = store.clone();
    // first_metadata is the first worker's owned creation request.
    // first_metadata 是第一个工作线程独占的创建请求。
    let first_metadata = metadata.clone();
    // first_barrier joins the common simultaneous release point.
    // first_barrier 加入公共同时释放点。
    let first_barrier = std::sync::Arc::clone(&barrier);
    // first_worker executes one of the competing creation transactions.
    // first_worker 执行其中一个竞争创建事务。
    let first_worker = std::thread::spawn(move || {
        first_barrier.wait();
        first_store.create_task_session(first_metadata)
    });
    // second_store is the independent handle used by the second worker.
    // second_store 是第二个工作线程使用的独立句柄。
    let second_store = store.clone();
    // second_worker executes the other competing creation transaction.
    // second_worker 执行另一个竞争创建事务。
    let second_worker = std::thread::spawn(move || {
        barrier.wait();
        second_store.create_task_session(metadata)
    });
    // first_result captures the first transaction outcome without assuming scheduling order.
    // first_result 捕获第一个事务结果，且不假定调度顺序。
    let first_result = first_worker.join().expect("join first creation worker");
    // second_result captures the second transaction outcome without assuming scheduling order.
    // second_result 捕获第二个事务结果，且不假定调度顺序。
    let second_result = second_worker.join().expect("join second creation worker");
    // results allow symmetric assertions over the scheduling-independent outcomes.
    // results 用于对与调度无关的结果进行对称断言。
    let results = [first_result, second_result];

    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    // creation_error is the one rejected duplicate transaction.
    // creation_error 是唯一被拒绝的重复创建事务。
    let creation_error = results
        .iter()
        .find_map(|result| result.as_ref().err())
        .expect("one duplicate creation should fail");
    assert!(creation_error.message.contains("directory already exists"));
    // task_dir is the final directory atomically published by the winning transaction.
    // task_dir 是获胜事务原子发布的最终目录。
    let task_dir = root
        .join(SEGMENTED_TASK_DIR)
        .join(task_session_id.to_string());
    assert!(task_main_metadata_path(&task_dir).is_file());
    assert!(task_metadata_path(&task_dir).is_file());
    assert!(task_dir.join(SEGMENTED_STATE_FILE).is_file());
    // task_lines prove the initial child event was not duplicated by the losing transaction.
    // task_lines 证明失败事务没有重复写入初始子任务事件。
    let task_lines = read_json_lines::<SegmentedEventLine>(&task_dir.join("s001.jsonl"))
        .expect("read child task lines");
    // root_lines prove the parent invocation link was published exactly once.
    // root_lines 证明父级调用关联只发布了一次。
    let root_lines =
        read_json_lines::<SegmentedEventLine>(&root.join("s001.jsonl")).expect("read root lines");
    // task_entries prove no unpublished staging directory remains beside the final task.
    // task_entries 证明最终子任务旁没有遗留未发布暂存目录。
    let task_entries = fs::read_dir(root.join(SEGMENTED_TASK_DIR))
        .expect("read task root")
        .collect::<Result<Vec<_>, _>>()
        .expect("collect task root entries");
    // expected_task_dir_name retains the stable id text used as the final directory component.
    // expected_task_dir_name 保留作为最终目录片段的稳定标识文本。
    let expected_task_dir_name = task_session_id.to_string();
    let _ = fs::remove_dir_all(&root);

    assert_eq!(task_lines.len(), 1);
    assert_eq!(root_lines.len(), 2);
    assert_eq!(task_entries.len(), 1);
    assert_eq!(
        task_entries[0].file_name(),
        std::ffi::OsStr::new(&expected_task_dir_name)
    );
}

/// Verifies an existing partial final directory is rejected instead of being reused and completed.
/// 验证已存在的残缺最终目录会被拒绝，而不是被复用并补写。
#[test]
fn task_creation_rejects_existing_partial_final_directory() {
    // root isolates the intentionally malformed task layout.
    // root 隔离刻意构造的异常子任务布局。
    let root = test_root("task_creation_rejects_existing_partial_final_directory");
    // store is the production creation path under test.
    // store 是被测的生产创建路径。
    let store = SegmentedTaskSessionStore::new(&root);
    // root_session_id identifies the valid owning main session.
    // root_session_id 标识有效的所属主会话。
    let root_session_id = SessionNodeId::new_v4();
    // task_session_id identifies the malformed pre-existing directory.
    // task_session_id 标识预先存在的异常目录。
    let task_session_id = SessionNodeId::new_v4();
    create_root_session_metadata(&root, root_session_id);
    // partial_task_dir simulates an interrupted legacy creation before metadata publication.
    // partial_task_dir 模拟元数据发布前中断的旧创建流程。
    let partial_task_dir = root
        .join(SEGMENTED_TASK_DIR)
        .join(task_session_id.to_string());
    fs::create_dir_all(&partial_task_dir).expect("create partial task directory");
    // result must reject the occupied final identity without mutating the partial directory.
    // result 必须拒绝已占用的最终身份，且不能改写异常目录。
    let result = store.create_task_session(TaskSessionMetadata::new(TaskSessionMetadataInit {
        root_session_id,
        task_session_id,
        parent: ParentSessionRef::MainSession { root_session_id },
        public_task_id: String::from("partial-create"),
        display_title: String::from("Partial create"),
        requested_config: SessionRequestedConfig::default(),
        effective_config: SessionEffectiveConfig::default(),
        depth: 1,
        timestamp_ms: 200,
    }));
    // partial_entries prove no production artifact was written into the occupied directory.
    // partial_entries 证明生产路径没有向已占用目录写入任何产物。
    let partial_entries = fs::read_dir(&partial_task_dir)
        .expect("read partial task directory")
        .collect::<Result<Vec<_>, _>>()
        .expect("collect partial task entries");
    let _ = fs::remove_dir_all(&root);

    let error = result.expect_err("partial final directory should be rejected");
    assert!(error.message.contains("directory already exists"));
    assert!(partial_entries.is_empty());
}

/// Verifies task discovery excludes complete-looking metadata inside an unpublished staging directory.
/// 验证子任务发现会排除尚未发布暂存目录中看似完整的元数据。
#[test]
fn task_discovery_ignores_unpublished_staging_layouts() {
    // root isolates the staged layout and discovery scan.
    // root 隔离暂存布局与发现扫描。
    let root = test_root("task_discovery_ignores_unpublished_staging_layouts");
    // store provides the production metadata discovery path.
    // store 提供生产元数据发现路径。
    let store = SegmentedTaskSessionStore::new(&root);
    // root_session_id identifies the metadata's owning main session.
    // root_session_id 标识元数据所属的主会话。
    let root_session_id = SessionNodeId::new_v4();
    // task_session_id derives both the canonical final name and the non-canonical staging name.
    // task_session_id 同时派生规范最终名称与非规范暂存名称。
    let task_session_id = SessionNodeId::new_v4();
    create_dir_all(&store.task_root_dir()).expect("create task root");
    // metadata is the single authoritative document source used by path derivation and staging.
    // metadata 是路径派生与暂存共同使用的唯一权威文档来源。
    let metadata = TaskSessionMetadata::new(TaskSessionMetadataInit {
        root_session_id,
        task_session_id,
        parent: ParentSessionRef::MainSession { root_session_id },
        public_task_id: String::from("staged-discovery"),
        display_title: String::from("Staged discovery"),
        requested_config: SessionRequestedConfig::default(),
        effective_config: SessionEffectiveConfig::default(),
        depth: 1,
        timestamp_ms: 200,
    });
    // final_task_dir is the canonical path that remains unpublished during this test.
    // final_task_dir 是本测试期间始终未发布的规范路径。
    let final_task_dir = store
        .new_task_dir(&metadata)
        .expect("resolve final task directory");
    // staged_task_dir is a sibling populated with a valid metadata document before publication.
    // staged_task_dir 是发布前写入有效元数据文档的同级暂存目录。
    let staged_task_dir =
        create_task_directory_staging(&final_task_dir).expect("create staged task directory");
    // staged_metadata is intentionally valid so only canonical path identity can exclude it.
    // staged_metadata 刻意保持有效，使其只能通过规范路径身份被排除。
    let staged_metadata = SegmentedTaskMetadataDocument::new(metadata);
    write_json_file(&task_metadata_path(&staged_task_dir), &staged_metadata)
        .expect("write staged task metadata");

    // discovered_metadata must remain empty until the directory receives its canonical final name.
    // discovered_metadata 在目录取得规范最终名称前必须保持为空。
    let discovered_metadata = store
        .load_all_task_session_metadata()
        .expect("discover published task metadata");
    remove_task_directory_staging(&staged_task_dir);
    let _ = fs::remove_dir_all(&root);

    assert!(discovered_metadata.is_empty());
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
        .join(SEGMENTED_TASK_METADATA_FILE);
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
        .join(SEGMENTED_TASK_METADATA_FILE);
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
    let task_metadata_path = task_dir.join(SEGMENTED_TASK_METADATA_FILE);
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
    let main_metadata = read_required_json::<Value>(&task_dir.join(SEGMENTED_METADATA_FILE))
        .expect("read main-compatible metadata");
    assert_eq!(main_metadata["session_id"], task_session_id.to_string());
    assert_eq!(main_metadata["canonical_source"], "segmented_task_session");
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
    let task_metadata_path = task_dir.join(SEGMENTED_TASK_METADATA_FILE);
    let task_main_metadata_path = task_dir.join(SEGMENTED_METADATA_FILE);
    let mut task_main_metadata =
        read_required_json::<SegmentedTaskSessionMainMetadataDocument>(&task_main_metadata_path)
            .expect("task main metadata");
    task_main_metadata.segment_policy = SegmentPolicy {
        max_lines: 2,
        max_bytes: DEFAULT_SEGMENT_MAX_BYTES,
    };
    write_json_file(&task_main_metadata_path, &task_main_metadata).expect("write task policy");

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
    let task_main_metadata =
        read_required_json::<SegmentedTaskSessionMainMetadataDocument>(&task_main_metadata_path)
            .expect("task main metadata after rollover");
    assert_eq!(task_main_metadata.active_segment, "s002.jsonl");
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

    // transaction_guard satisfies the owned-lock contract exercised by the root helper.
    // transaction_guard 满足根 helper 所要求的已持锁契约。
    let _transaction_guard =
        vulcan_session_core::segmented_file_io::segmented_append_file_io_lock()
            .lock()
            .expect("lock root append transaction");
    let result = append_main_session_task_link_event_with_owned_append_lock(
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

/// Verifies task-local event appends follow the highest persisted sequence and backfill nested event sequence.
/// 验证子任务本地事件追加会跟随已持久化最大序号并回填嵌套事件序号。
#[test]
fn task_segment_append_uses_highest_existing_sequence_and_backfills_event() {
    let root = test_root("task_segment_append_uses_highest_existing_sequence_and_backfills_event");
    let store = SegmentedTaskSessionStore::new(&root);
    let root_session_id = SessionNodeId::new_v4();
    let task_session_id = SessionNodeId::new_v4();
    create_root_session_metadata(&root, root_session_id);
    store
        .create_task_session(TaskSessionMetadata::new(TaskSessionMetadataInit {
            root_session_id,
            task_session_id,
            parent: ParentSessionRef::MainSession { root_session_id },
            public_task_id: "stale-task-seq-child".to_string(),
            display_title: "stale-task-seq-child".to_string(),
            requested_config: SessionRequestedConfig::default(),
            effective_config: SessionEffectiveConfig::default(),
            depth: 1,
            timestamp_ms: 200,
        }))
        .expect("create task session");
    let task_dir = root
        .join(SEGMENTED_TASK_DIR)
        .join(task_session_id.to_string());
    append_json_line(
        &task_dir.join("s001.jsonl"),
        &SegmentedEventLine {
            seq: 5,
            timestamp: "250".to_string(),
            kind: "display_event".to_string(),
            payload: json!({ "retained": true }),
            turn_index: None,
        },
    )
    .expect("append retained high-sequence task row");
    let metadata_path = task_dir.join(SEGMENTED_METADATA_FILE);
    let mut metadata =
        read_required_json::<Value>(&metadata_path).expect("read stale task metadata");
    metadata["next_global_seq"] = json!(1);
    metadata["segments"][0]["line_count"] = json!(2);
    write_json_file(&metadata_path, &metadata).expect("write stale task metadata");

    store
        .append_event(SessionEventRecord::new(
            "task-local-event",
            task_session_id,
            0,
            SessionEventKind::AssistantOutput,
            300,
            json!({ "body": "done" }),
        ))
        .expect("append task-local event");

    let task_lines =
        read_json_lines::<SegmentedEventLine>(&task_dir.join("s001.jsonl")).expect("read task");
    let appended = task_lines.last().expect("appended task row");
    assert_eq!(appended.seq, 6);
    assert_eq!(appended.payload["session_event"]["sequence"], json!(6));
    let loaded_events = store
        .load_events(task_session_id)
        .expect("load task events");
    assert_eq!(
        loaded_events
            .last()
            .expect("loaded appended event")
            .sequence,
        6
    );
    let updated_metadata =
        read_required_json::<Value>(&metadata_path).expect("read updated task metadata");
    assert_eq!(updated_metadata["next_global_seq"], json!(7));
}

/// Verifies task-local appends repair a truncated JSONL tail before writing the next event.
/// 验证子任务本地追加会在写入下一条事件前修复截断的 JSONL 尾行。
#[test]
fn task_segment_append_repairs_truncated_jsonl_tail_line() {
    let root = test_root("task_segment_append_repairs_truncated_jsonl_tail_line");
    let store = SegmentedTaskSessionStore::new(&root);
    let root_session_id = SessionNodeId::new_v4();
    let task_session_id = SessionNodeId::new_v4();
    create_root_session_metadata(&root, root_session_id);
    store
        .create_task_session(TaskSessionMetadata::new(TaskSessionMetadataInit {
            root_session_id,
            task_session_id,
            parent: ParentSessionRef::MainSession { root_session_id },
            public_task_id: "repair-task-tail-child".to_string(),
            display_title: "repair-task-tail-child".to_string(),
            requested_config: SessionRequestedConfig::default(),
            effective_config: SessionEffectiveConfig::default(),
            depth: 1,
            timestamp_ms: 200,
        }))
        .expect("create task session");
    store
        .append_event(SessionEventRecord::new(
            "before-broken-tail",
            task_session_id,
            0,
            SessionEventKind::AssistantOutput,
            250,
            json!({ "body": "before" }),
        ))
        .expect("append first task-local event");
    let task_dir = root
        .join(SEGMENTED_TASK_DIR)
        .join(task_session_id.to_string());
    let segment_path = task_dir.join("s001.jsonl");
    {
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&segment_path)
            .expect("open task segment for broken tail");
        use std::io::Write as _;
        write!(file, "{{\"seq\":999,\"timestamp\":\"broken\"")
            .expect("append truncated task segment tail");
    }

    store
        .append_event(SessionEventRecord::new(
            "after-broken-tail",
            task_session_id,
            0,
            SessionEventKind::AssistantOutput,
            300,
            json!({ "body": "after" }),
        ))
        .expect("append after repairing broken tail");

    let text = fs::read_to_string(&segment_path).expect("read repaired task segment");
    assert!(
        text.lines()
            .all(|line| serde_json::from_str::<Value>(line).is_ok())
    );
    let task_lines =
        read_json_lines::<SegmentedEventLine>(&segment_path).expect("read repaired task lines");
    assert_eq!(task_lines.last().expect("last repaired line").seq, 2);
    let updated_metadata = read_required_json::<Value>(&task_dir.join(SEGMENTED_METADATA_FILE))
        .expect("read updated task metadata");
    assert_eq!(updated_metadata["next_global_seq"], json!(3));
    assert_eq!(updated_metadata["segments"][0]["line_count"], json!(3));
}

/// Verifies latest-payload reads repair truncated task JSONL and refresh segment metadata.
/// 验证最新载荷读取会修复截断的子任务 JSONL，并刷新分段元数据。
#[test]
fn load_latest_event_payload_repairs_truncated_task_jsonl_tail() {
    let root = test_root("load_latest_event_payload_repairs_truncated_task_jsonl_tail");
    let store = SegmentedTaskSessionStore::new(&root);
    let root_session_id = SessionNodeId::new_v4();
    let task_session_id = SessionNodeId::new_v4();
    create_root_session_metadata(&root, root_session_id);
    store
        .create_task_session(TaskSessionMetadata::new(TaskSessionMetadataInit {
            root_session_id,
            task_session_id,
            parent: ParentSessionRef::MainSession { root_session_id },
            public_task_id: "repair-latest-payload-child".to_string(),
            display_title: "repair-latest-payload-child".to_string(),
            requested_config: SessionRequestedConfig::default(),
            effective_config: SessionEffectiveConfig::default(),
            depth: 1,
            timestamp_ms: 200,
        }))
        .expect("create task session");
    store
        .append_event(SessionEventRecord::new(
            "latest-before-broken-tail",
            task_session_id,
            0,
            SessionEventKind::AssistantOutput,
            250,
            json!({ "body": "latest-valid" }),
        ))
        .expect("append latest valid task event");

    let task_dir = root
        .join(SEGMENTED_TASK_DIR)
        .join(task_session_id.to_string());
    let segment_path = task_dir.join("s001.jsonl");
    {
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&segment_path)
            .expect("open task segment for broken tail");
        use std::io::Write as _;
        write!(file, "{{\"seq\":999,\"timestamp\":\"broken\"")
            .expect("append truncated task segment tail");
    }
    let main_metadata_path = task_dir.join(SEGMENTED_METADATA_FILE);
    let mut stale_metadata =
        read_required_json::<Value>(&main_metadata_path).expect("read stale task main metadata");
    stale_metadata["segments"][0]["line_count"] = json!(99);
    write_json_file(&main_metadata_path, &stale_metadata).expect("write stale task metadata");

    let payload = store
        .load_latest_event_payload(task_session_id)
        .expect("load latest payload after repair")
        .expect("latest payload");

    assert_eq!(payload["body"], json!("latest-valid"));
    let text = fs::read_to_string(&segment_path).expect("read repaired task segment");
    assert!(
        text.lines()
            .all(|line| serde_json::from_str::<Value>(line).is_ok())
    );
    let refreshed_main_metadata =
        read_required_json::<Value>(&main_metadata_path).expect("read refreshed main metadata");
    assert_eq!(
        refreshed_main_metadata["segments"][0]["line_count"],
        json!(2)
    );
    assert_eq!(refreshed_main_metadata["next_global_seq"], json!(2));
    let refreshed_task_metadata =
        read_required_json::<Value>(&task_dir.join(SEGMENTED_TASK_METADATA_FILE))
            .expect("read refreshed task metadata");
    assert_eq!(
        refreshed_task_metadata["segments"][0]["line_count"],
        json!(2)
    );
    assert_eq!(refreshed_task_metadata["next_seq"], json!(2));
}
