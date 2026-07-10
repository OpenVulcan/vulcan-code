use super::*;

/// Verifies an inserted transcript row triggers a physical rewrite instead of an unchecked suffix append.
/// 验证插入转录行会触发物理重写，而不是未经校验的尾部追加。
#[test]
fn append_message_update_physically_rewrites_inserted_history() {
    let root = test_root("append_message_update_physically_rewrites_inserted_history");
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        test_session_request(SegmentPolicy::default()),
    )
    .expect("create session");
    let first_messages = vec![
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
    ];
    append_segmented_session_message_update(&session_dir, &first_messages)
        .expect("append first messages");

    let changed_messages = vec![
        first_messages[0].clone(),
        test_message(
            "m-inserted",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::ThinkingOutput,
            "inserted thinking",
        ),
        first_messages[1].clone(),
    ];
    let changed_lines = append_segmented_session_message_update(&session_dir, &changed_messages)
        .expect("rewrite inserted transcript row");

    let metadata: SegmentedSessionMetadata =
        read_required_json(&session_dir.join(SEGMENTED_METADATA_FILE))
            .expect("read rewritten metadata");
    let replay = replay_segmented_session_context(&session_dir)
        .expect("replay rewritten transcript")
        .expect("rewritten transcript replay result");
    let replay_messages = conversation_state_to_session_messages(&replay.conversation_state);
    assert_eq!(changed_lines.len(), 2);
    assert_eq!(metadata.message_count, 3);
    assert_eq!(metadata.latest_rollback_seq, None);
    assert_eq!(replay_messages, changed_messages);
}

/// Verifies an incomplete persisted tail checkpoint forces a physical rewrite and checkpoint repair.
/// 验证不完整的持久化尾部检查点会强制执行物理重写并修复检查点。
#[test]
fn append_message_update_rewrites_incomplete_tail_checkpoint() {
    let root = test_root("append_message_update_rewrites_incomplete_tail_checkpoint");
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        test_session_request(SegmentPolicy::default()),
    )
    .expect("create session");
    let original_messages = vec![
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
    ];
    append_segmented_session_message_update(&session_dir, &original_messages)
        .expect("append original messages");

    let mut incomplete_metadata: SegmentedSessionMetadata =
        read_required_json(&session_dir.join(SEGMENTED_METADATA_FILE))
            .expect("read original metadata");
    incomplete_metadata.last_message_id = None;
    incomplete_metadata.last_message_fingerprint = None;
    write_metadata(&session_dir, &incomplete_metadata).expect("write incomplete checkpoint");

    let changed_messages = vec![
        original_messages[0].clone(),
        test_message(
            "m-assistant-1",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::AssistantOutput,
            "corrected answer",
        ),
    ];
    let changed_lines = append_segmented_session_message_update(&session_dir, &changed_messages)
        .expect("rewrite incomplete checkpoint transcript");

    let repaired_metadata: SegmentedSessionMetadata =
        read_required_json(&session_dir.join(SEGMENTED_METADATA_FILE))
            .expect("read repaired metadata");
    let replay = replay_segmented_session_context(&session_dir)
        .expect("replay repaired transcript")
        .expect("repaired transcript replay result");
    let replay_messages = conversation_state_to_session_messages(&replay.conversation_state);
    assert_eq!(changed_lines.len(), 1);
    assert_eq!(repaired_metadata.message_count, 2);
    assert!(repaired_metadata.last_message_id.is_some());
    assert!(repaired_metadata.last_message_fingerprint.is_some());
    assert_eq!(replay_messages, changed_messages);
}

/// Verifies shortened transcript updates physically rewrite message rows without rollback bloat.
/// 验证缩短的转录更新会物理重写消息行且不产生回滚膨胀。
#[test]
fn append_message_update_physically_rewrites_shortened_history() {
    let root = test_root("append_message_update_physically_rewrites_shortened_history");
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        test_session_request(SegmentPolicy::default()),
    )
    .expect("create session");
    let original_messages = vec![
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
    ];
    append_segmented_session_message_update(&session_dir, &original_messages)
        .expect("append original messages");
    let replacement_messages = vec![test_message(
        "m-user-1",
        SessionMessageKind::User,
        SessionMessageRecordKind::Transcript,
        "edited prompt",
    )];

    let replacement_lines =
        append_segmented_session_message_update(&session_dir, &replacement_messages)
            .expect("rewrite shortened transcript");

    let replacement_metadata: SegmentedSessionMetadata =
        read_required_json(&session_dir.join(SEGMENTED_METADATA_FILE))
            .expect("read replacement metadata");
    assert_eq!(replacement_metadata.latest_rollback_seq, None);
    assert_eq!(replacement_metadata.message_count, 1);
    assert!(
        replacement_lines
            .iter()
            .all(|line| line.kind != SegmentedSessionEventKind::ThreadRolledBack)
    );
    assert_eq!(replacement_lines.len(), replacement_messages.len());
    let latest_window =
        read_segmented_session_turn_window(&session_dir, None, 10).expect("read latest window");
    assert_eq!(
        latest_window
            .lines
            .iter()
            .filter(|line| line.payload.get("message").is_some())
            .map(|line| line.payload["message_id"].as_str().unwrap_or_default())
            .collect::<Vec<_>>(),
        vec!["m-user-1"]
    );
    let latest_messages =
        segmented_lines_to_session_messages(&latest_window.lines).expect("project messages");
    assert_eq!(
        latest_messages
            .iter()
            .map(|message| message.body.as_str())
            .collect::<Vec<_>>(),
        vec!["edited prompt"]
    );
    assert!(!latest_window.has_more_history);
    let latest_event_window =
        read_segmented_session_event_window(&session_dir, None, 10).expect("read event window");
    assert_eq!(
        latest_event_window
            .iter()
            .filter(|line| line.payload.get("message").is_some())
            .map(|line| line.payload["message_id"].as_str().unwrap_or_default())
            .collect::<Vec<_>>(),
        vec!["m-user-1"]
    );
    let earliest_seq = latest_window.lines.first().map(|line| line.seq);
    let previous_window = read_segmented_session_turn_window(&session_dir, earliest_seq, 1)
        .expect("read previous window");
    assert!(previous_window.lines.is_empty());

    let replay = replay_segmented_session_context(&session_dir).expect("replay segmented context");
    let replay_messages =
        conversation_state_to_session_messages(&replay.expect("replay result").conversation_state);
    assert_eq!(replay_messages[0].body, "edited prompt");
    assert_eq!(replay_messages.len(), 1);
}

/// Verifies physical message rewrite becomes the replay baseline instead of stale stream prefixes.
/// 验证物理消息重写会成为重放基线，而不是继续依赖旧的流式前缀。
#[test]
fn replay_uses_physically_rewritten_events_as_baseline() {
    let root = test_root("replay_uses_physically_rewritten_events_as_baseline");
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        test_session_request(SegmentPolicy::default()),
    )
    .expect("create session");
    let original_messages = vec![
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
            "stream prefix",
        ),
    ];
    append_segmented_session_message_update(&session_dir, &original_messages)
        .expect("append original messages");
    let replacement_messages = vec![
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
            "stream prefix and completed body",
        ),
    ];
    append_segmented_session_message_update(&session_dir, &replacement_messages)
        .expect("append replacement");
    let metadata = read_session_metadata(&session_dir).expect("read metadata");
    let scan = find_latest_compaction_anchor(&session_dir, &metadata.segments)
        .expect("scan replacement anchor");
    assert!(scan.rollback_markers.is_empty());
    assert_eq!(metadata.latest_rollback_seq, None);
    assert!(scan.anchor.is_none());

    let replay = replay_segmented_session_context(&session_dir).expect("replay segmented context");
    let replay_messages =
        conversation_state_to_session_messages(&replay.expect("replay result").conversation_state);
    assert_eq!(replay_messages.len(), 2);
    assert_eq!(replay_messages[1].body, "stream prefix and completed body");
}

/// Verifies prompt-edit rewinds physically truncate later events without a rollback marker.
/// 验证提示词编辑回退会物理截断后续事件且不写入回滚标记。
#[test]
fn truncate_before_message_removes_tail_without_rollback_marker() {
    let root = test_root("truncate_before_message_removes_tail_without_rollback_marker");
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        test_session_request(SegmentPolicy {
            max_lines: 2,
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
    append_segmented_session_message_update(&session_dir, &messages)
        .expect("append original messages");
    write_segmented_session_fork_source(
        &session_dir,
        SegmentedSessionForkSource {
            source_project_tag: String::from("project-a"),
            source_session_id: String::from("source-session"),
            source_title: String::from("source title"),
            anchor_message_id: Some(String::from("m-user-2")),
            created_at: String::from("123"),
        },
    )
    .expect("write fork source");

    truncate_segmented_session_before_message(&session_dir, "m-user-2", &messages[..2])
        .expect("truncate before second prompt");

    let metadata: SegmentedSessionMetadata =
        read_required_json(&session_dir.join(SEGMENTED_METADATA_FILE))
            .expect("read truncated metadata");
    assert_eq!(metadata.message_count, 2);
    assert_eq!(metadata.latest_rollback_seq, None);
    let retained_lines = metadata
        .segments
        .iter()
        .flat_map(|segment| {
            read_segment_event_lines(&session_dir.join(&segment.file))
                .expect("read retained segment")
        })
        .collect::<Vec<_>>();
    assert!(
        retained_lines
            .iter()
            .all(|line| line.kind != SegmentedSessionEventKind::ThreadRolledBack)
    );
    assert!(
        retained_lines
            .iter()
            .all(|line| segmented_line_message_id(line) != Some("m-user-2"))
    );
    assert_eq!(metadata.fork_source, None);
    let replay = replay_segmented_session_context(&session_dir).expect("replay truncated context");
    let replay_messages =
        conversation_state_to_session_messages(&replay.expect("replay result").conversation_state);
    assert_eq!(
        replay_messages
            .iter()
            .map(|message| message.id.as_str())
            .collect::<Vec<_>>(),
        vec!["m-user-1", "m-assistant-1"]
    );
}

/// Verifies anchor truncation keeps the selected message and drops only later rows.
/// 验证锚点截断会保留所选消息且只丢弃后续行。
#[test]
fn truncate_after_message_keeps_anchor_and_rewrites_segments() {
    let root = test_root("truncate_after_message_keeps_anchor_and_rewrites_segments");
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        test_session_request(SegmentPolicy {
            max_lines: 2,
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
    append_segmented_session_message_update(&session_dir, &messages)
        .expect("append original messages");

    truncate_segmented_session_after_message(&session_dir, "m-assistant-1", &messages[..2])
        .expect("truncate after first answer");

    let metadata: SegmentedSessionMetadata =
        read_required_json(&session_dir.join(SEGMENTED_METADATA_FILE))
            .expect("read truncated metadata");
    assert_eq!(metadata.message_count, 2);
    assert_eq!(metadata.latest_rollback_seq, None);
    let retained_lines = metadata
        .segments
        .iter()
        .flat_map(|segment| {
            read_segment_event_lines(&session_dir.join(&segment.file))
                .expect("read retained segment")
        })
        .collect::<Vec<_>>();
    assert!(
        retained_lines
            .iter()
            .all(|line| line.kind != SegmentedSessionEventKind::ThreadRolledBack)
    );
    assert!(
        retained_lines
            .iter()
            .any(|line| segmented_line_message_id(line) == Some("m-assistant-1"))
    );
    assert!(
        retained_lines
            .iter()
            .all(|line| segmented_line_message_id(line) != Some("m-user-2"))
    );
    let replay = replay_segmented_session_context(&session_dir).expect("replay truncated context");
    let replay_messages =
        conversation_state_to_session_messages(&replay.expect("replay result").conversation_state);
    assert_eq!(
        replay_messages
            .iter()
            .map(|message| message.id.as_str())
            .collect::<Vec<_>>(),
        vec!["m-user-1", "m-assistant-1"]
    );
}

/// Verifies a missing prompt id cannot partially mutate segment files.
/// 验证缺失的提示词标识不会导致分段文件发生局部改写。
#[test]
fn truncate_before_missing_message_leaves_segments_unchanged() {
    let root = test_root("truncate_before_missing_message_leaves_segments_unchanged");
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        test_session_request(SegmentPolicy {
            max_lines: 2,
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
    ];
    append_segmented_session_message_update(&session_dir, &messages)
        .expect("append original messages");
    let before_metadata =
        fs::read_to_string(session_dir.join(SEGMENTED_METADATA_FILE)).expect("read metadata");
    let before_segments = read_segment_texts_for_test(&session_dir);

    let result =
        truncate_segmented_session_before_message(&session_dir, "missing-message", &messages);

    assert_eq!(result, Err(String::from("ERR_USER_PROMPT_EDIT_NOT_FOUND")));
    let after_metadata =
        fs::read_to_string(session_dir.join(SEGMENTED_METADATA_FILE)).expect("read metadata");
    let after_segments = read_segment_texts_for_test(&session_dir);
    assert_eq!(after_metadata, before_metadata);
    assert_eq!(after_segments, before_segments);
}

/// Verifies physically removed messages can be reintroduced with the same id later.
/// 验证被物理删除的消息后续可以用相同标识重新引入。
#[test]
fn append_after_physical_rewrite_reintroduces_removed_message_id() {
    let root = test_root("append_after_physical_rewrite_reintroduces_removed_message_id");
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        test_session_request(SegmentPolicy::default()),
    )
    .expect("create session");
    let original_messages = vec![
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
            "m-assistant-removed",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::AssistantOutput,
            "removed answer",
        ),
    ];
    append_segmented_session_message_update(&session_dir, &original_messages)
        .expect("append original messages");
    let replacement_messages = vec![original_messages[0].clone(), original_messages[1].clone()];
    let replacement_lines =
        append_segmented_session_message_update(&session_dir, &replacement_messages)
            .expect("rewrite without removed message");
    assert!(
        replacement_lines
            .iter()
            .all(|line| line.kind != SegmentedSessionEventKind::ThreadRolledBack)
    );

    let restored_messages = vec![
        replacement_messages[0].clone(),
        replacement_messages[1].clone(),
        original_messages[2].clone(),
    ];
    let restored_lines = append_segmented_session_message_update(&session_dir, &restored_messages)
        .expect("append restored message");

    assert_eq!(restored_lines.len(), 1);
    assert_eq!(
        restored_lines[0].payload["message_id"],
        serde_json::json!("m-assistant-removed")
    );
    let latest_window =
        read_segmented_session_turn_window(&session_dir, None, 10).expect("read latest window");
    let latest_message_ids = latest_window
        .lines
        .iter()
        .map(|line| line.payload["message_id"].as_str().unwrap_or_default())
        .collect::<Vec<_>>();
    assert_eq!(
        latest_message_ids,
        vec!["m-user-1", "m-assistant-1", "m-assistant-removed"]
    );
}

/// Verifies repeated message updates rewrite in place instead of creating display maintenance work.
/// 验证重复消息更新会就地重写，而不是制造显示层维护任务。
#[test]
fn repeated_message_updates_do_not_create_rollback_maintenance_work() {
    let root = test_root("repeated_message_updates_do_not_create_rollback_maintenance_work");
    let policy = SegmentPolicy {
        max_bytes: 700,
        ..Default::default()
    };
    let session_dir =
        create_segmented_session_in_location(&root, false, test_session_request(policy))
            .expect("create session");
    let mut messages = vec![
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
            "stream prefix",
        ),
    ];
    append_segmented_session_message_update(&session_dir, &messages)
        .expect("append initial messages");
    for index in 0..4 {
        messages[1].body.push_str(&format!(
            " completed chunk {index} with enough text to force rollover"
        ));
        append_segmented_session_message_update(&session_dir, &messages)
            .expect("append replacement update");
    }
    let metadata = read_session_metadata(&session_dir).expect("read metadata");
    assert_eq!(metadata.latest_rollback_seq, None);
    let retained_lines = metadata
        .segments
        .iter()
        .flat_map(|segment| {
            read_segment_event_lines(&session_dir.join(&segment.file))
                .expect("read retained segment")
        })
        .collect::<Vec<_>>();
    assert!(
        retained_lines
            .iter()
            .all(|line| line.kind != SegmentedSessionEventKind::ThreadRolledBack)
    );

    let report = repair_segmented_session_display_if_needed(&session_dir).expect("repair display");

    assert!(report.is_none());
    let replay = replay_segmented_session_context(&session_dir).expect("replay repaired context");
    let replay_messages =
        conversation_state_to_session_messages(&replay.expect("replay result").conversation_state);
    assert_eq!(replay_messages.len(), 2);
    assert_eq!(replay_messages[1].body, messages[1].body);
}

/// Verifies physical message rewrites keep the next sequence after retained non-message rows.
/// 验证消息物理重写后，下一个序号仍位于保留的非消息行之后。
#[test]
fn append_message_update_keeps_next_sequence_after_retained_event_gap() {
    let root = test_root("append_message_update_keeps_next_sequence_after_retained_event_gap");
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        test_session_request(SegmentPolicy::default()),
    )
    .expect("create session");
    let original_messages = vec![
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
    ];
    append_segmented_session_message_update(&session_dir, &original_messages)
        .expect("append original messages");
    append_segmented_session_events(
        &session_dir,
        &[SegmentedSessionEventInput {
            timestamp: String::from("2026-06-06T10:00:03Z"),
            kind: SegmentedSessionEventKind::SessionBoundary,
            payload: json!({ "kind": "turn_finished" }),
            turn_index: None,
        }],
    )
    .expect("append retained non-message event");

    let replacement_messages = vec![test_message(
        "m-user-1",
        SessionMessageKind::User,
        SessionMessageRecordKind::Transcript,
        "edited prompt",
    )];
    append_segmented_session_message_update(&session_dir, &replacement_messages)
        .expect("rewrite shortened transcript");
    let metadata_after_rewrite: SegmentedSessionMetadata =
        read_required_json(&session_dir.join(SEGMENTED_METADATA_FILE))
            .expect("read rewritten metadata");
    assert_eq!(metadata_after_rewrite.next_global_seq, 4);

    let appended = append_segmented_session_events(
        &session_dir,
        &[SegmentedSessionEventInput {
            timestamp: String::from("2026-06-06T10:00:04Z"),
            kind: SegmentedSessionEventKind::SessionBoundary,
            payload: json!({ "kind": "turn_started" }),
            turn_index: None,
        }],
    )
    .expect("append event after rewrite");
    assert_eq!(appended[0].seq, 4);
    let all_lines =
        read_segment_event_lines(&session_dir.join("s001.jsonl")).expect("read segment lines");
    let unique_sequences = all_lines
        .iter()
        .map(|line| line.seq)
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(unique_sequences.len(), all_lines.len());
}
