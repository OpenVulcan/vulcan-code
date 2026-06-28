use super::*;

/// Verifies segmented event windows can be read lazily and projected back to transcript messages.
/// 验证分段事件窗口可以懒读取并投影回转录消息。
#[test]
fn read_event_window_projects_back_to_session_messages() {
    let root = test_root("read_event_window_projects_back_to_session_messages");
    let session_dir = create_segmented_session(
        &root,
        test_session_request(SegmentPolicy {
            max_lines: 3,
            max_bytes: DEFAULT_SEGMENT_MAX_BYTES,
        }),
    )
    .expect("create session");
    let messages = vec![
        test_message(
            "m-user-1",
            SessionMessageKind::User,
            SessionMessageRecordKind::Transcript,
            "first prompt",
        ),
        test_message(
            "m-assistant-1",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::AssistantOutput,
            "first answer",
        ),
        test_message(
            "m-user-2",
            SessionMessageKind::User,
            SessionMessageRecordKind::Transcript,
            "second prompt",
        ),
        test_message(
            "m-assistant-2",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::AssistantOutput,
            "second answer",
        ),
    ];
    append_segmented_session_message_tail(&session_dir, &messages).expect("append messages");

    let active_tail =
        read_segmented_session_active_tail(&session_dir, 1).expect("read active tail");
    assert_eq!(active_tail.len(), 1);
    assert_eq!(active_tail[0].payload["message_id"], "m-assistant-2");

    let window =
        read_segmented_session_event_window(&session_dir, Some(4), 2).expect("read event window");
    assert_eq!(
        window.iter().map(|line| line.seq).collect::<Vec<_>>(),
        vec![2, 3]
    );
    let projected = segmented_lines_to_session_messages(&window).expect("project session messages");
    assert_eq!(
        projected
            .iter()
            .map(|message| message.id.as_str())
            .collect::<Vec<_>>(),
        vec!["m-assistant-1", "m-user-2"]
    );
}

/// Verifies turn windows keep complete recent turns even when one turn has multiple detail rows.
/// 验证轮次窗口在单轮包含多条详情行时仍保留完整最近轮次。
#[test]
fn read_turn_window_preserves_complete_display_turns() {
    let root = test_root("read_turn_window_preserves_complete_display_turns");
    let session_dir =
        create_segmented_session(&root, test_session_request(SegmentPolicy::default()))
            .expect("create session");
    let messages = vec![
        test_message(
            "m-user-1",
            SessionMessageKind::User,
            SessionMessageRecordKind::Transcript,
            "first prompt",
        ),
        test_message(
            "m-tool-1",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::ToolCallOutput,
            "first tool detail",
        ),
        test_message(
            "m-assistant-1",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::AssistantOutput,
            "first answer",
        ),
        test_message(
            "m-user-2",
            SessionMessageKind::User,
            SessionMessageRecordKind::Transcript,
            "second prompt",
        ),
        test_message(
            "m-tool-2",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::ToolResultOutput,
            "second tool detail",
        ),
        test_message(
            "m-assistant-2",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::AssistantOutput,
            "second answer",
        ),
        test_message(
            "m-user-3",
            SessionMessageKind::User,
            SessionMessageRecordKind::Transcript,
            "third prompt",
        ),
        test_message(
            "m-assistant-3",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::AssistantOutput,
            "third answer",
        ),
    ];
    append_segmented_session_message_tail(&session_dir, &messages).expect("append messages");

    let latest_window =
        read_segmented_session_turn_window(&session_dir, None, 2).expect("read turn window");
    assert_eq!(
        latest_window
            .lines
            .iter()
            .map(|line| line.payload["message_id"].as_str().unwrap_or_default())
            .collect::<Vec<_>>(),
        vec![
            "m-user-2",
            "m-tool-2",
            "m-assistant-2",
            "m-user-3",
            "m-assistant-3"
        ]
    );
    assert!(latest_window.has_more_history);
    let earliest_seq = latest_window.lines.first().map(|line| line.seq);

    let previous_window = read_segmented_session_turn_window(&session_dir, earliest_seq, 1)
        .expect("read previous turn window");
    assert_eq!(
        previous_window
            .lines
            .iter()
            .map(|line| line.payload["message_id"].as_str().unwrap_or_default())
            .collect::<Vec<_>>(),
        vec!["m-user-1", "m-tool-1", "m-assistant-1"]
    );
    assert!(!previous_window.has_more_history);
}

/// Verifies state documents are overwritten independently from JSONL event lines.
/// 验证状态文档独立于 JSONL 事件行被覆盖写入。
#[test]
fn write_state_overwrites_without_touching_segment_lines() {
    let root = test_root("write_state_overwrites_without_touching_segment_lines");
    let session_dir =
        create_segmented_session(&root, test_session_request(SegmentPolicy::default()))
            .expect("create session");
    let before_lines = read_segment_event_lines(&session_dir.join("s001.jsonl"))
        .expect("read lines before state overwrite");

    write_state(
        &session_dir,
        &SegmentedSessionStateDocument {
            schema_version: 1,
            session_id: "session-alpha".to_string(),
            updated_at: "2026-06-06T11:00:00Z".to_string(),
            runtime_state: json!({ "phase": "checkpoint" }),
            current_context: json!({ "cursor": 7 }),
            session_boundary_state: Value::Null,
        },
    )
    .expect("write state");

    let after_lines = read_segment_event_lines(&session_dir.join("s001.jsonl"))
        .expect("read lines after state overwrite");
    assert_eq!(before_lines, after_lines);
    let state: SegmentedSessionStateDocument =
        read_required_json(&session_dir.join(SEGMENTED_STATE_FILE)).expect("read state");
    assert_eq!(state.runtime_state["phase"], "checkpoint");
}

/// Verifies runtime-state writes preserve the independently managed session-boundary partition.
/// 验证运行态写入会保留独立管理的会话边界状态分区。
#[test]
fn write_state_preserves_existing_session_boundary_partition() {
    let root = test_root("write_state_preserves_existing_session_boundary_partition");
    let session_dir =
        create_segmented_session(&root, test_session_request(SegmentPolicy::default()))
            .expect("create session");

    replace_session_boundary_state(
        &session_dir,
        json!({
            "status": "idle",
            "last_turn_id": "turn-alpha"
        }),
    )
    .expect("write boundary state");
    write_state(
        &session_dir,
        &SegmentedSessionStateDocument {
            schema_version: 1,
            session_id: "session-alpha".to_string(),
            updated_at: "2026-06-08T10:00:00Z".to_string(),
            runtime_state: json!({ "phase": "runtime-checkpoint" }),
            current_context: json!({ "cursor": 9 }),
            session_boundary_state: Value::Null,
        },
    )
    .expect("write runtime state");

    let state = read_state(&session_dir)
        .expect("read state")
        .expect("state exists");
    assert_eq!(state.runtime_state["phase"], "runtime-checkpoint");
    assert_eq!(state.session_boundary_state["status"], "idle");
    assert_eq!(state.session_boundary_state["last_turn_id"], "turn-alpha");
}

/// Verifies runtime state entries are mapped into overwrite-only state documents.
/// 验证运行态条目会映射进覆盖式状态文档。
#[test]
fn runtime_state_entry_maps_to_segmented_state_document() {
    let mut runtime_state = SessionRuntimeStateEntry {
        active_model_id: Some("model-alpha".to_string()),
        context_start_message_id: Some("message-1".to_string()),
        last_compaction_id: Some("compaction-1".to_string()),
        updated_at: "2026-06-06T13:00:00Z".to_string(),
        ..SessionRuntimeStateEntry::default()
    };
    runtime_state.anchor_context_budget_fingerprint = Some("budget-a".to_string());
    runtime_state.ensure_ai_tool_configuration(&Default::default());
    runtime_state
        .ai_tool_configuration
        .as_mut()
        .expect("ai capability configuration")
        .disabled_capability_kinds
        .insert(AiToolKind::ImageGeneration);

    let state = state_document_from_runtime_state("session-alpha", &runtime_state)
        .expect("build state document");

    assert_eq!(state.session_id, "session-alpha");
    assert_eq!(state.updated_at, "2026-06-06T13:00:00Z");
    assert_eq!(state.runtime_state["active_model_id"], "model-alpha");
    assert_eq!(
        state.current_context["context_start_message_id"],
        "message-1"
    );
    assert_eq!(state.current_context["last_compaction_id"], "compaction-1");
    assert_eq!(
        state.current_context["anchor_context_budget_fingerprint"],
        "budget-a"
    );
    assert_eq!(
        state.runtime_state["ai_tool_configuration"]["disabledCapabilityKinds"][0],
        "image_generation"
    );
    assert!(state.runtime_state.get("conversation_state").is_none());
    assert!(state.current_context.get("conversation_state").is_none());
}
