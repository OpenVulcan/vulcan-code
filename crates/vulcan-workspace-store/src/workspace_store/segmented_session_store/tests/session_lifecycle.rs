use super::*;

/// Verifies the directory layout and first SessionMeta line for a new main session.
/// 验证新主会话的目录布局与首条 SessionMeta 行。
#[test]
fn create_session_writes_metadata_state_and_first_segment() {
    let root = test_root("create_session_writes_metadata_state_and_first_segment");
    let session_dir =
        create_segmented_session(&root, test_session_request(SegmentPolicy::default()))
            .expect("create segmented session");

    assert!(session_dir.join(SEGMENTED_METADATA_FILE).exists());
    assert!(session_dir.join(SEGMENTED_STATE_FILE).exists());
    assert!(session_dir.join(SEGMENTED_TASK_DIR).is_dir());
    assert!(session_dir.join(SEGMENTED_BLOB_DIR).is_dir());

    let metadata: SegmentedSessionMetadata =
        read_required_json(&session_dir.join(SEGMENTED_METADATA_FILE)).expect("read metadata");
    assert_eq!(metadata.active_segment, "s001.jsonl");
    assert_eq!(metadata.next_global_seq, 1);
    assert_eq!(metadata.segments[0].line_count, 1);

    let lines = read_segment_event_lines(&session_dir.join("s001.jsonl"))
        .expect("read first segment lines");
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].seq, 0);
    assert_eq!(lines[0].kind, SegmentedSessionEventKind::SessionMeta);
}

/// Verifies deterministic task-store directory lookup derives the date from raw creation time.
/// 验证确定性任务存储目录查找会从原始创建时间推导日期。
#[test]
fn task_store_locator_finds_session_directory_by_created_at() {
    let root = test_root("task_store_locator_finds_session_directory_by_created_at");
    let mut request = test_session_request(SegmentPolicy::default());
    request.created_date = String::from("1970-01-02");
    request.created_at = String::from("86400000");
    let session_id = request.session_id.clone();
    let project_tag = request.project_tag.clone();
    let created_at = request.created_at.clone();
    let session_dir = create_segmented_session(&root, request).expect("create segmented session");

    let located = find_segmented_session_directory_for_task_store_created_at(
        &root,
        &project_tag,
        &created_at,
        &session_id,
    )
    .expect("deterministic locator should find session");

    assert_eq!(located, session_dir);
}

/// Verifies task mirror rows load from an already resolved segmented session directory.
/// 验证任务镜像行可以从已解析的分段会话目录加载。
#[test]
fn task_mirror_rows_load_from_resolved_session_directory() {
    let root = test_root("task_mirror_rows_load_from_resolved_session_directory");
    let session_dir =
        create_segmented_session(&root, test_session_request(SegmentPolicy::default()))
            .expect("create segmented session");
    let task_session_id = Uuid::new_v4();
    let mut task_request = test_task_session_request();
    task_request.task_session_id = task_session_id.to_string();
    create_segmented_task_session(&session_dir, task_request).expect("create task session");

    let mirrors =
        crate::workspace_store::load_segmented_session_task_mirrors_from_directory(&session_dir)
            .expect("load task mirrors from resolved directory");

    assert_eq!(mirrors.len(), 1);
    assert_eq!(mirrors[0].status, "running");
    assert_eq!(mirrors[0].task_session_id, task_session_id);
}

/// Verifies a forked session rewrites only structural ownership fields.
/// 验证分叉会话只改写结构化归属字段。
#[test]
fn fork_session_directory_rewrites_structural_identity() {
    let root = test_root("fork_session_directory_rewrites_structural_identity");
    let source_session_id = Uuid::new_v4().to_string();
    let target_session_id = Uuid::new_v4().to_string();
    let mut request = test_session_request(SegmentPolicy::default());
    request.session_id = source_session_id.clone();
    let source_dir = create_segmented_session(&root, request).expect("create segmented session");
    let source_runtime_state = SessionRuntimeStateEntry {
        active_provider_id: Some(String::from("source-provider")),
        updated_at: "2026-06-06T10:00:00Z".to_string(),
        ..SessionRuntimeStateEntry::default()
    };
    let mut source_state =
        state_document_from_runtime_state(&source_session_id, &source_runtime_state)
            .expect("build source state");
    source_state.session_boundary_state = json!({
        "thread_id": source_session_id,
        "next_sequence": 2,
        "status": "idle",
        "active_turn_id": null,
        "last_turn_id": "turn-alpha"
    });
    write_state(&source_dir, &source_state).expect("write source state");
    let source_runtime_conversation = conversation_state_from_session_messages(&[test_message(
        &format!("{source_session_id}-runtime-user"),
        SessionMessageKind::User,
        SessionMessageRecordKind::Transcript,
        "runtime prompt",
    )]);
    sync_runtime_log_from_conversation_state(
        &source_dir,
        &source_runtime_conversation,
        "2026-06-06T10:00:00Z",
    )
    .expect("write source runtime log");
    append_runtime_log_event_if_present(
        &source_dir,
        "2026-06-06T10:00:01Z",
        RuntimeLogEvent::ProviderRequestCheckpoint(RuntimeProviderRequestCheckpointEvent {
            turn_id: Some(format!("{source_session_id}-turn")),
            checkpoint_id: format!("provider-request-{source_session_id}-alpha"),
            provider_id: String::from("deepseek"),
            model_id: String::from("deepseek-v4-pro"),
            provider_family: String::from("deepseek"),
            capability_fingerprint: Some(String::from("cap-alpha")),
            prompt_fingerprint: Some(String::from("prompt-alpha")),
            injection_fingerprint: Some(String::from("inject-alpha")),
            request_fingerprint: Some(String::from("request-alpha")),
            cache_policy_fingerprint: None,
            cache_policy_diagnostic: None,
            converter_version: 1,
        }),
    )
    .expect("write source runtime checkpoint");
    let source_runtime_lines = read_runtime_log_lines(&source_dir)
        .expect("read source runtime lines")
        .expect("source runtime exists");
    let source_task_dir = create_segmented_task_session(&source_dir, test_task_session_request())
        .expect("create source task");
    assert!(source_task_dir.ends_with("task/task-alpha"));
    let source_task_runtime_state = SessionRuntimeStateEntry {
        active_provider_id: Some(String::from("task-provider")),
        updated_at: "2026-06-06T10:00:00Z".to_string(),
        ..SessionRuntimeStateEntry::default()
    };
    let mut source_task_state =
        state_document_from_runtime_state("task-alpha", &source_task_runtime_state)
            .expect("build source task state");
    source_task_state.session_boundary_state = json!({
        "thread_id": "task-alpha",
        "parent_thread_id": source_session_id,
    });
    write_state(&source_task_dir, &source_task_state).expect("write source task state");
    sync_runtime_log_from_conversation_state(
        &source_task_dir,
        &conversation_state_from_session_messages(&[test_message(
            "task-alpha-runtime-user",
            SessionMessageKind::User,
            SessionMessageRecordKind::Transcript,
            "task runtime prompt",
        )]),
        "2026-06-06T10:00:00Z",
    )
    .expect("write source task runtime log");
    append_segmented_session_events(
        &source_dir,
        &[SegmentedSessionEventInput {
            timestamp: "2026-06-06T10:00:01Z".to_string(),
            kind: SegmentedSessionEventKind::SessionBoundary,
            payload: json!({
                "event_id": "event-alpha",
                "thread_id": source_session_id,
                "turn_id": "turn-alpha",
                "item_id": null,
                "kind": "turn_started",
                "scope": "turn",
                "visibility": "hidden",
                "sequence": 1,
                "payload": null
            }),
            turn_index: None,
        }],
    )
    .expect("append source boundary event");
    let mut task_card_message = test_message(
        "task-card",
        SessionMessageKind::Assistant,
        SessionMessageRecordKind::ToolResultOutput,
        "task done",
    );
    task_card_message.metadata = Some(json!({
        "task_session_id": "task-alpha",
        "task_id": "task-alpha",
        "child_thread_id": "task-alpha",
        "parent_thread_id": source_session_id.clone(),
        "subagent_mailbox_event": {
            "child_thread_id": "task-alpha",
            "parent_thread_id": source_session_id.clone(),
            "payload": {
                "task_session_id": "task-alpha",
                "task_id": "task-alpha",
                "child_thread_id": "task-alpha"
            }
        }
    }));
    append_segmented_session_message_tail(&source_dir, &[task_card_message])
        .expect("append source task transcript card");

    let target_dir = fork_segmented_session_directory(
        &root,
        &SegmentedSessionForkRequest {
            source_archived: false,
            source_project_tag: "Project Alpha".to_string(),
            source_session_id: source_session_id.clone(),
            target_project_tag: "Project Alpha".to_string(),
            target_session_id: target_session_id.clone(),
            target_created_date: "2026-06-07".to_string(),
            target_created_at: "2026-06-07T10:00:00Z".to_string(),
            target_title: "Alpha Fork".to_string(),
            target_title_source: "user".to_string(),
        },
    )
    .expect("fork segmented session");

    let source_metadata = read_session_metadata(&source_dir).expect("read source metadata");
    assert_eq!(source_metadata.session_id, source_session_id);

    let target_metadata = read_session_metadata(&target_dir).expect("read target metadata");
    assert_eq!(target_metadata.session_id, target_session_id);
    assert_eq!(target_metadata.title.as_deref(), Some("Alpha Fork"));
    assert_eq!(target_metadata.created_at, "2026-06-07T10:00:00Z");
    assert_eq!(target_metadata.tasks.len(), 1);
    let target_task = &target_metadata.tasks[0];
    assert_ne!(target_task.session_id, "task-alpha");
    assert!(target_task.dir.starts_with("task/"));
    assert!(target_task.active_segment.starts_with(&target_task.dir));
    assert!(target_task.metadata_file.starts_with(&target_task.dir));

    let target_state = read_state(&target_dir)
        .expect("read target state")
        .expect("target state exists");
    assert_eq!(target_state.session_id, target_session_id);
    assert_eq!(target_state.updated_at, "2026-06-07T10:00:00Z");
    assert_eq!(
        target_state.session_boundary_state["thread_id"],
        target_session_id
    );
    assert_eq!(
        target_state.runtime_state["active_provider_id"],
        "source-provider"
    );
    let target_metadata_text = fs::read_to_string(target_dir.join(SEGMENTED_METADATA_FILE))
        .expect("read target metadata text");
    assert!(!target_metadata_text.contains(&source_session_id));
    let target_state_text =
        fs::read_to_string(target_dir.join(SEGMENTED_STATE_FILE)).expect("read target state text");
    assert!(!target_state_text.contains(&source_session_id));
    let target_runtime_lines = read_runtime_log_lines(&target_dir)
        .expect("read target runtime lines")
        .expect("target runtime exists");
    assert_eq!(target_runtime_lines.len(), source_runtime_lines.len());
    let target_runtime_text =
        serde_json::to_string(&target_runtime_lines).expect("serialize target runtime lines");
    assert!(!target_runtime_text.contains(&source_session_id));
    assert!(target_runtime_text.contains(&target_session_id));
    let target_runtime_index =
        read_required_json::<RuntimeLogIndex>(&runtime_index_path(&target_dir))
            .expect("read target runtime index");
    assert!(
        target_runtime_index
            .provider_checkpoint_offsets
            .contains_key(&format!(
                "deepseek:deepseek:deepseek-v4-pro:provider-request-{target_session_id}-alpha"
            ))
    );

    let target_lines =
        read_segment_event_lines(&target_dir.join("s001.jsonl")).expect("read target lines");
    let target_segment_text =
        fs::read_to_string(target_dir.join("s001.jsonl")).expect("read target segment text");
    assert!(!target_segment_text.contains(&source_session_id));
    assert_eq!(target_lines[0].kind, SegmentedSessionEventKind::SessionMeta);
    assert_eq!(target_lines[0].payload["session_id"], target_session_id);
    assert_eq!(target_lines[0].payload["project_tag"], "Project Alpha");
    assert_eq!(
        target_lines[1].kind,
        SegmentedSessionEventKind::SubagentTaskStarted
    );
    assert_eq!(
        target_lines[1].payload["task_session_id"],
        target_task.session_id
    );
    assert_eq!(target_lines[1].payload["task_dir"], target_task.dir);
    assert_eq!(
        target_lines[2].kind,
        SegmentedSessionEventKind::SessionBoundary
    );
    assert_eq!(target_lines[2].payload["thread_id"], target_session_id);
    let task_card_line = target_lines
        .iter()
        .find(|line| line.payload["message_id"] == "task-card")
        .expect("target task card line exists");
    let task_card_metadata = &task_card_line.payload["message"]["metadata"];
    assert_eq!(
        task_card_metadata["task_session_id"],
        target_task.session_id
    );
    assert_eq!(task_card_metadata["task_id"], target_task.session_id);
    assert_eq!(
        task_card_metadata["child_thread_id"],
        target_task.session_id
    );
    assert_eq!(task_card_metadata["parent_thread_id"], target_session_id);
    assert_eq!(
        task_card_metadata["subagent_mailbox_event"]["child_thread_id"],
        target_task.session_id
    );
    assert_eq!(
        task_card_metadata["subagent_mailbox_event"]["parent_thread_id"],
        target_session_id
    );
    assert_eq!(
        task_card_metadata["subagent_mailbox_event"]["payload"]["task_session_id"],
        target_task.session_id
    );
    assert_eq!(
        task_card_metadata["subagent_mailbox_event"]["payload"]["task_id"],
        target_task.session_id
    );

    let target_task_dir = target_dir.join(&target_task.dir);
    let target_task_metadata =
        read_task_session_metadata(&target_task_dir).expect("read target task metadata");
    assert_eq!(target_task_metadata.task_session_id, target_task.session_id);
    assert_eq!(target_task_metadata.parent_session_id, target_session_id);
    let target_task_state = read_state(&target_task_dir)
        .expect("read target task state")
        .expect("target task state exists");
    assert_eq!(target_task_state.session_id, target_task.session_id);
    assert_eq!(
        target_task_state.runtime_state["active_provider_id"],
        "task-provider"
    );
    assert_eq!(
        target_task_state.session_boundary_state["thread_id"],
        target_task.session_id
    );
    assert_eq!(
        target_task_state.session_boundary_state["parent_thread_id"],
        target_session_id
    );
    assert!(runtime_metadata_path(&target_task_dir).exists());
    assert!(runtime_index_path(&target_task_dir).exists());
    assert!(runtime_dir_path(&target_task_dir).exists());
    let target_task_runtime_lines = read_runtime_log_lines(&target_task_dir)
        .expect("read target task runtime lines")
        .expect("target task runtime exists");
    let target_task_runtime_text = serde_json::to_string(&target_task_runtime_lines)
        .expect("serialize target task runtime lines");
    assert!(!target_task_runtime_text.contains("task-alpha"));
    assert!(target_task_runtime_text.contains(&target_task.session_id));
    let target_task_lines = read_segment_event_lines(&target_task_dir.join("s001.jsonl"))
        .expect("read target task lines");
    assert_eq!(
        target_task_lines[0].kind,
        SegmentedSessionEventKind::SubagentTaskMeta
    );
    assert_eq!(
        target_task_lines[0].payload["task_session_id"],
        target_task.session_id
    );
    assert_eq!(
        target_task_lines[0].payload["parent_session_id"],
        target_session_id
    );
}
