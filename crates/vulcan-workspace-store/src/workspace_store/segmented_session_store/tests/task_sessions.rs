use super::*;

/// Verifies child task sessions use the same segment model and begin with SubagentTaskMeta.
/// 验证子任务会话使用同一分段模型并以 SubagentTaskMeta 开头。
#[test]
fn create_task_session_writes_canonical_task_meta_first_line() {
    let root = test_root("create_task_session_writes_canonical_task_meta_first_line");
    let session_dir =
        create_segmented_session(&root, test_session_request(SegmentPolicy::default()))
            .expect("create session");
    let task_dir = create_segmented_task_session(
        &session_dir,
        SegmentedTaskSessionCreateRequest {
            task_session_id: "task-alpha".to_string(),
            parent_start_seq: Some(1),
            agent_kind: SubagentAgentKind::AgentMd,
            agent_profile: SubagentAgentProfile {
                agent_id: Some("reviewer".to_string()),
                agent_name: Some("Reviewer".to_string()),
                agent_md_path: Some("agents/reviewer/agent.md".to_string()),
                prompt_source: Some("agent_md".to_string()),
            },
            spawned_by: SubagentSpawnedBy {
                parent_turn_id: Some("turn-1".to_string()),
                parent_event_seq: Some(1),
                tool_call_id: Some("tool-1".to_string()),
            },
            created_at: "2026-06-06T10:03:00Z".to_string(),
            status: "running".to_string(),
            segment_policy: SegmentPolicy::default(),
        },
    )
    .expect("create task session");

    let task_metadata: SegmentedTaskSessionMetadata =
        read_required_json(&task_dir.join(SEGMENTED_METADATA_FILE)).expect("read task metadata");
    assert_eq!(task_metadata.active_segment, "s001.jsonl");
    assert_eq!(task_metadata.next_local_seq, 1);
    let task_metadata_value: Value = read_required_json(&task_dir.join(SEGMENTED_METADATA_FILE))
        .expect("read task metadata value");
    assert_eq!(task_metadata_value["agent_kind"], "agent_md");
    assert_eq!(
        task_metadata_value["agent_profile"]["prompt_source"],
        "agent_md"
    );

    let task_lines =
        read_segment_event_lines(&task_dir.join("s001.jsonl")).expect("read task segment lines");
    assert_eq!(task_lines.len(), 1);
    assert_eq!(task_lines[0].seq, 0);
    assert_eq!(
        task_lines[0].kind,
        SegmentedSessionEventKind::SubagentTaskMeta
    );
    assert_eq!(task_lines[0].payload["agent_kind"], "agent_md");

    let parent_metadata: SegmentedSessionMetadata =
        read_required_json(&session_dir.join(SEGMENTED_METADATA_FILE))
            .expect("read parent metadata");
    assert_eq!(parent_metadata.tasks.len(), 1);
    assert_eq!(parent_metadata.tasks[0].dir, "task/task-alpha");
    assert_eq!(parent_metadata.tasks[0].line_count, 1);
    let parent_metadata_value: Value =
        read_required_json(&session_dir.join(SEGMENTED_METADATA_FILE))
            .expect("read parent metadata value");
    assert_eq!(parent_metadata_value["tasks"][0]["agent_kind"], "agent_md");

    let parent_lines = read_segment_event_lines(&session_dir.join("s001.jsonl"))
        .expect("read parent segment lines");
    assert_eq!(parent_lines.len(), 2);
    assert_eq!(
        parent_lines[1].kind,
        SegmentedSessionEventKind::SubagentTaskStarted
    );
    assert_eq!(parent_lines[1].payload["task_dir"], "task/task-alpha");
    assert_eq!(parent_metadata.tasks[0].parent_start_seq, Some(1));
}

/// Verifies task metadata and parent mirrors write every required task field.
/// 验证子任务元数据与父级镜像会写出所有必需任务字段。
#[test]
fn task_metadata_and_parent_mirror_include_required_fields() {
    let root = test_root("task_metadata_and_parent_mirror_include_required_fields");
    let session_dir =
        create_segmented_session(&root, test_session_request(SegmentPolicy::default()))
            .expect("create session");
    let task_dir = create_segmented_task_session(&session_dir, test_task_session_request())
        .expect("create task session");
    let task_metadata_path = task_dir.join(SEGMENTED_METADATA_FILE);
    let task_metadata: Value = read_required_json(&task_metadata_path).expect("read task metadata");
    assert_eq!(task_metadata["agent_kind"], "main_clone");
    assert!(task_metadata["agent_profile"].is_object());
    assert!(task_metadata["spawned_by"].is_object());
    assert_eq!(task_metadata["status"], "running");
    assert_eq!(task_metadata["active_segment"], "s001.jsonl");
    assert_eq!(task_metadata["next_local_seq"], 1);
    assert!(task_metadata["segment_policy"].is_object());
    assert_eq!(task_metadata["segments"][0]["line_count"], 1);

    append_segmented_task_events(
        &task_dir,
        &[SegmentedSessionEventInput {
            timestamp: "2026-06-06T10:04:00Z".to_string(),
            kind: SegmentedSessionEventKind::AssistantOutput,
            payload: json!({ "body": "progress" }),
            turn_index: Some(1),
        }],
    )
    .expect("append task event");

    let linked_line = record_segmented_task_link_update(
        &session_dir,
        SegmentedTaskLinkUpdate {
            task_session_id: "task-alpha".to_string(),
            timestamp: "2026-06-06T10:05:00Z".to_string(),
            status: "completed".to_string(),
            terminal: true,
            payload: json!({ "result": "done" }),
            turn_index: Some(1),
        },
    )
    .expect("record task link update");

    let parent_metadata_path = session_dir.join(SEGMENTED_METADATA_FILE);
    let parent_metadata: Value =
        read_required_json(&parent_metadata_path).expect("read parent metadata");
    assert_eq!(parent_metadata["tasks"][0]["agent_kind"], "main_clone");
    assert_eq!(
        parent_metadata["tasks"][0]["active_segment"],
        "task/task-alpha/s001.jsonl"
    );
    assert_eq!(
        parent_metadata["tasks"][0]["metadata_file"],
        "task/task-alpha/metadata.json"
    );
    assert_eq!(parent_metadata["tasks"][0]["status"], "completed");
    assert_eq!(parent_metadata["tasks"][0]["line_count"], 2);
    assert_eq!(
        parent_metadata["tasks"][0]["created_at"],
        "2026-06-06T10:03:00Z"
    );
    assert_eq!(
        parent_metadata["tasks"][0]["updated_at"],
        "2026-06-06T10:05:00Z"
    );
    assert!(parent_metadata["tasks"][0]["agent_profile"].is_object());
    assert_eq!(linked_line.payload["agent_kind"], "main_clone");
    assert!(linked_line.payload["agent_profile"].is_object());
}

/// Verifies linked and completed child-task updates are reflected in parent and child metadata.
/// 验证子任务 linked 与 completed 更新会同步反映到父元数据与子元数据。
#[test]
fn task_link_updates_write_parent_events_and_refresh_metadata() {
    let root = test_root("task_link_updates_write_parent_events_and_refresh_metadata");
    let session_dir =
        create_segmented_session(&root, test_session_request(SegmentPolicy::default()))
            .expect("create session");
    let task_dir = create_segmented_task_session(&session_dir, test_task_session_request())
        .expect("create task session");

    let linked_event = record_segmented_task_link_update(
        &session_dir,
        SegmentedTaskLinkUpdate {
            task_session_id: "task-alpha".to_string(),
            timestamp: "2026-06-06T10:04:00Z".to_string(),
            status: "running".to_string(),
            terminal: false,
            payload: json!({ "progress": "half" }),
            turn_index: Some(2),
        },
    )
    .expect("record linked event");
    assert_eq!(
        linked_event.kind,
        SegmentedSessionEventKind::SubagentTaskEventLinked
    );
    assert_eq!(linked_event.payload["detail"]["progress"], "half");
    assert_eq!(linked_event.payload["agent_kind"], "main_clone");
    assert!(linked_event.payload["agent_profile"].is_object());

    let completed_event = record_segmented_task_link_update(
        &session_dir,
        SegmentedTaskLinkUpdate {
            task_session_id: "task-alpha".to_string(),
            timestamp: "2026-06-06T10:05:00Z".to_string(),
            status: "completed".to_string(),
            terminal: true,
            payload: json!({ "result": "done" }),
            turn_index: Some(2),
        },
    )
    .expect("record completed event");
    assert_eq!(
        completed_event.kind,
        SegmentedSessionEventKind::SubagentTaskCompleted
    );
    assert_eq!(completed_event.payload["detail"]["result"], "done");
    assert_eq!(completed_event.payload["agent_kind"], "main_clone");
    assert!(completed_event.payload["agent_profile"].is_object());

    let parent_lines = read_segment_event_lines(&session_dir.join("s001.jsonl"))
        .expect("read parent segment lines");
    assert_eq!(
        parent_lines
            .iter()
            .map(|line| line.kind)
            .collect::<Vec<_>>(),
        vec![
            SegmentedSessionEventKind::SessionMeta,
            SegmentedSessionEventKind::SubagentTaskStarted,
            SegmentedSessionEventKind::SubagentTaskEventLinked,
            SegmentedSessionEventKind::SubagentTaskCompleted,
        ]
    );

    let parent_metadata: SegmentedSessionMetadata =
        read_required_json(&session_dir.join(SEGMENTED_METADATA_FILE))
            .expect("read parent metadata");
    assert_eq!(parent_metadata.tasks[0].status, "completed");
    assert_eq!(
        parent_metadata.tasks[0].parent_end_seq,
        Some(completed_event.seq)
    );

    let task_metadata: SegmentedTaskSessionMetadata =
        read_required_json(&task_dir.join(SEGMENTED_METADATA_FILE)).expect("read task metadata");
    assert_eq!(task_metadata.status, "completed");
    assert_eq!(task_metadata.updated_at, "2026-06-06T10:05:00Z");
}

/// Verifies parent task-link payloads point at the child's latest active segment after rollover.
/// 验证子任务滚动分段后父任务关联载荷会指向子任务最新活动分段。
#[test]
fn task_link_update_uses_child_active_segment_after_rollover() {
    let root = test_root("task_link_update_uses_child_active_segment_after_rollover");
    let session_dir =
        create_segmented_session(&root, test_session_request(SegmentPolicy::default()))
            .expect("create session");
    let mut request = test_task_session_request();
    request.segment_policy = SegmentPolicy {
        max_lines: 1,
        max_bytes: DEFAULT_SEGMENT_MAX_BYTES,
    };
    let task_dir =
        create_segmented_task_session(&session_dir, request).expect("create task session");
    append_segmented_task_events(
        &task_dir,
        &[SegmentedSessionEventInput {
            timestamp: "2026-06-06T10:04:00Z".to_string(),
            kind: SegmentedSessionEventKind::DisplayEvent,
            payload: json!({ "progress": "rolled" }),
            turn_index: Some(2),
        }],
    )
    .expect("append task event after rollover");

    let linked_event = record_segmented_task_link_update(
        &session_dir,
        SegmentedTaskLinkUpdate {
            task_session_id: "task-alpha".to_string(),
            timestamp: "2026-06-06T10:05:00Z".to_string(),
            status: "running".to_string(),
            terminal: false,
            payload: json!({ "progress": "linked" }),
            turn_index: Some(2),
        },
    )
    .expect("record linked event");
    let parent_metadata: SegmentedSessionMetadata =
        read_required_json(&session_dir.join(SEGMENTED_METADATA_FILE))
            .expect("read parent metadata");

    assert_eq!(
        linked_event.payload["task_active_segment"],
        "task/task-alpha/s002.jsonl"
    );
    assert_eq!(
        parent_metadata.tasks[0].active_segment,
        "task/task-alpha/s002.jsonl"
    );
}
