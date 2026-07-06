use super::*;

/// Verifies reverse scanning finds the latest replacement-history compaction anchor.
/// 验证倒序扫描会找到最新的 replacement-history 压缩锚点。
#[test]
fn reverse_scan_finds_latest_compaction_anchor_and_suffix() {
    let root = test_root("reverse_scan_finds_latest_compaction_anchor_and_suffix");
    let policy = SegmentPolicy {
        max_lines: 2,
        max_bytes: DEFAULT_SEGMENT_MAX_BYTES,
    };
    let session_dir =
        create_segmented_session_in_location(&root, false, test_session_request(policy))
            .expect("create session");
    let summary_message = serde_json::to_value(test_message(
        "summary-message",
        SessionMessageKind::Assistant,
        SessionMessageRecordKind::AssistantOutput,
        "summary",
    ))
    .expect("serialize replacement history message");
    append_segmented_session_events(
        &session_dir,
        &[
            test_event(
                "2026-06-06T10:01:00Z",
                SegmentedSessionEventKind::UserPrompt,
            ),
            SegmentedSessionEventInput {
                timestamp: "2026-06-06T10:02:00Z".to_string(),
                kind: SegmentedSessionEventKind::Compacted,
                payload: json!({
                    "replacement_history": [summary_message]
                }),
                turn_index: Some(2),
            },
            test_event(
                "2026-06-06T10:03:00Z",
                SegmentedSessionEventKind::AssistantOutput,
            ),
            test_event(
                "2026-06-06T10:04:00Z",
                SegmentedSessionEventKind::UserPrompt,
            ),
        ],
    )
    .expect("append compaction events");
    let metadata: SegmentedSessionMetadata =
        read_required_json(&session_dir.join(SEGMENTED_METADATA_FILE)).expect("read metadata");

    let scan = find_latest_compaction_anchor(&session_dir, &metadata.segments)
        .expect("scan compaction anchor");

    let anchor = scan.anchor.expect("expected compaction anchor");
    assert_eq!(anchor.anchor_seq, 2);
    assert_eq!(anchor.anchor_kind, SegmentedSessionEventKind::Compacted);
    assert_eq!(anchor.segment_file, "s002.jsonl");
    assert_eq!(anchor.replacement_history[0]["body"], "summary");
    assert_eq!(scan.suffix.len(), 2);
    assert_eq!(scan.suffix[0].seq, 3);
    assert_eq!(scan.suffix[1].seq, 4);
    assert_eq!(scan.scanned_segment_count, 2);
}

/// Verifies reverse scanning returns all events in replay order when no anchor exists.
/// 验证没有锚点时倒序扫描会按重放顺序返回全部事件。
#[test]
fn reverse_scan_without_anchor_returns_all_events_in_forward_order() {
    let root = test_root("reverse_scan_without_anchor_returns_all_events_in_forward_order");
    let policy = SegmentPolicy {
        max_lines: 2,
        max_bytes: DEFAULT_SEGMENT_MAX_BYTES,
    };
    let session_dir =
        create_segmented_session_in_location(&root, false, test_session_request(policy))
            .expect("create session");
    append_segmented_session_events(
        &session_dir,
        &[
            test_event(
                "2026-06-06T10:01:00Z",
                SegmentedSessionEventKind::UserPrompt,
            ),
            test_event(
                "2026-06-06T10:02:00Z",
                SegmentedSessionEventKind::AssistantOutput,
            ),
            test_event(
                "2026-06-06T10:03:00Z",
                SegmentedSessionEventKind::UserPrompt,
            ),
        ],
    )
    .expect("append events");
    let metadata: SegmentedSessionMetadata =
        read_required_json(&session_dir.join(SEGMENTED_METADATA_FILE)).expect("read metadata");

    let scan = find_latest_compaction_anchor(&session_dir, &metadata.segments)
        .expect("scan without anchor");

    assert_eq!(scan.anchor, None);
    assert_eq!(
        scan.suffix.iter().map(|line| line.seq).collect::<Vec<_>>(),
        vec![0, 1, 2, 3]
    );
    assert_eq!(scan.scanned_segment_count, metadata.segments.len());
}

/// Verifies reverse scanning captures TurnContext and rollback markers found after an anchor.
/// 验证倒序扫描会捕获锚点之后发现的 TurnContext 与回滚标记。
#[test]
fn reverse_scan_captures_turn_context_and_rollback_markers() {
    let root = test_root("reverse_scan_captures_turn_context_and_rollback_markers");
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        test_session_request(SegmentPolicy::default()),
    )
    .expect("create session");
    let compressed_message = serde_json::to_value(test_message(
        "compressed-assistant",
        SessionMessageKind::Assistant,
        SessionMessageRecordKind::AssistantOutput,
        "compressed",
    ))
    .expect("serialize replacement history message");
    append_segmented_session_events(
        &session_dir,
        &[
            SegmentedSessionEventInput {
                timestamp: "2026-06-06T10:01:00Z".to_string(),
                kind: SegmentedSessionEventKind::Compacted,
                payload: json!({
                    "replacement_history": [compressed_message]
                }),
                turn_index: Some(1),
            },
            SegmentedSessionEventInput {
                timestamp: "2026-06-06T10:02:00Z".to_string(),
                kind: SegmentedSessionEventKind::TurnContext,
                payload: json!({
                    "previous_turn_settings": {
                        "model": "qwen"
                    },
                    "reference_context_item": {
                        "id": "ctx-1"
                    }
                }),
                turn_index: Some(2),
            },
            SegmentedSessionEventInput {
                timestamp: "2026-06-06T10:03:00Z".to_string(),
                kind: SegmentedSessionEventKind::ThreadRolledBack,
                payload: json!({ "drop_user_turns": 1 }),
                turn_index: Some(3),
            },
        ],
    )
    .expect("append recovery events");
    let metadata: SegmentedSessionMetadata =
        read_required_json(&session_dir.join(SEGMENTED_METADATA_FILE)).expect("read metadata");

    let scan = find_latest_compaction_anchor(&session_dir, &metadata.segments)
        .expect("scan recovery anchors");

    assert_eq!(
        scan.anchor.as_ref().map(|anchor| anchor.anchor_seq),
        Some(1)
    );
    let turn_context = scan
        .latest_turn_context
        .expect("expected latest turn context");
    assert_eq!(turn_context.seq, 2);
    assert_eq!(
        turn_context.payload["previous_turn_settings"]["model"],
        "qwen"
    );
    assert_eq!(scan.rollback_markers.len(), 1);
    assert_eq!(scan.rollback_markers[0].seq, 3);
    assert_eq!(scan.rollback_markers[0].payload["drop_user_turns"], 1);
}

/// Verifies replay uses replacement history as baseline and applies the newest rollback boundary.
/// 验证重放会使用 replacement history 作为基线，并应用最新回滚边界。
#[test]
fn replay_segmented_context_applies_compaction_suffix_and_rollback() {
    let root = test_root("replay_segmented_context_applies_compaction_suffix_and_rollback");
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        test_session_request(SegmentPolicy::default()),
    )
    .expect("create session");
    let summary_user = serde_json::to_value(test_message(
        "summary-user",
        SessionMessageKind::User,
        SessionMessageRecordKind::Transcript,
        "compressed prompt",
    ))
    .expect("serialize summary user");
    let summary_assistant = serde_json::to_value(test_message(
        "summary-assistant",
        SessionMessageKind::Assistant,
        SessionMessageRecordKind::AssistantOutput,
        "compressed answer",
    ))
    .expect("serialize summary assistant");
    append_segmented_session_events(
        &session_dir,
        &[
            SegmentedSessionEventInput {
                timestamp: "10".to_string(),
                kind: SegmentedSessionEventKind::Compacted,
                payload: json!({
                    "replacement_history": [summary_user, summary_assistant]
                }),
                turn_index: Some(0),
            },
            segmented_event_input_from_message(
                1,
                &test_message(
                    "rolled-back-user",
                    SessionMessageKind::User,
                    SessionMessageRecordKind::Transcript,
                    "rolled back prompt",
                ),
                1,
            )
            .expect("rolled back message"),
            SegmentedSessionEventInput {
                timestamp: "30".to_string(),
                kind: SegmentedSessionEventKind::ThreadRolledBack,
                payload: json!({ "drop_user_turns": 1 }),
                turn_index: Some(1),
            },
            segmented_event_input_from_message(
                2,
                &test_message(
                    "kept-user",
                    SessionMessageKind::User,
                    SessionMessageRecordKind::Transcript,
                    "kept prompt",
                ),
                2,
            )
            .expect("kept user"),
            segmented_event_input_from_message(
                3,
                &test_message(
                    "kept-assistant",
                    SessionMessageKind::Assistant,
                    SessionMessageRecordKind::AssistantOutput,
                    "kept answer",
                ),
                2,
            )
            .expect("kept assistant"),
        ],
    )
    .expect("append replay events");

    let replay = replay_segmented_session_context(&session_dir)
        .expect("replay segmented context")
        .expect("replay result");
    let messages = conversation_state_to_session_messages(&replay.conversation_state);

    assert_eq!(
        messages
            .iter()
            .map(|message| message.id.as_str())
            .collect::<Vec<_>>(),
        vec![
            "summary-user",
            "summary-assistant",
            "kept-user",
            "kept-assistant"
        ]
    );
    assert_eq!(
        replay.last_compaction_id.as_deref(),
        Some("segmented-compaction-1")
    );
    assert_eq!(
        replay.context_start_message_id.as_deref(),
        Some("summary-user")
    );
    assert!(
        replay
            .updated_at
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
    );
}

/// Verifies loose replacement-history items are rejected instead of defaulting missing fields.
/// 验证宽松 replacement-history 条目会被拒绝，而不是为缺失字段生成默认值。
#[test]
fn replacement_history_rejects_loose_items_without_session_message_shape() {
    let loose_history = json!([
        {
            "role": "assistant",
            "content": "loose summary",
            "created_at": "10"
        }
    ]);

    let error = replacement_history_to_session_messages(&loose_history)
        .expect_err("loose replacement history must fail");

    assert!(
        error.contains("replacement_history must be a conversation state or session message array")
    );
}
