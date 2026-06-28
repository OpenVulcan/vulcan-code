use super::*;

/// Verifies non-tail transcript changes are rejected instead of being silently appended out of order.
/// 验证非尾部转录变更会被拒绝，而不是被静默乱序追加。
#[test]
fn append_message_tail_rejects_non_tail_change() {
    let root = test_root("append_message_tail_rejects_non_tail_change");
    let session_dir =
        create_segmented_session(&root, test_session_request(SegmentPolicy::default()))
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
    append_segmented_session_message_tail(&session_dir, &first_messages)
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
    let error = append_segmented_session_message_tail(&session_dir, &changed_messages)
        .expect_err("reject non-tail change");

    assert!(error.contains("non-tail transcript change"));
}

/// Verifies shortened transcript updates become rollback-bounded replacement history.
/// 验证缩短的转录更新会变成带回滚边界的替换历史。
#[test]
fn append_message_update_records_rollback_replacement_for_shortened_history() {
    let root =
        test_root("append_message_update_records_rollback_replacement_for_shortened_history");
    let session_dir =
        create_segmented_session(&root, test_session_request(SegmentPolicy::default()))
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
    append_segmented_session_message_tail(&session_dir, &original_messages)
        .expect("append original messages");
    let replacement_messages = vec![test_message(
        "m-user-1",
        SessionMessageKind::User,
        SessionMessageRecordKind::Transcript,
        "edited prompt",
    )];

    let replacement_lines =
        append_segmented_session_message_update(&session_dir, &replacement_messages)
            .expect("append rollback replacement");

    assert_eq!(
        replacement_lines.first().map(|line| line.kind),
        Some(SegmentedSessionEventKind::ThreadRolledBack)
    );
    let replacement_metadata: SegmentedSessionMetadata =
        read_required_json(&session_dir.join(SEGMENTED_METADATA_FILE))
            .expect("read replacement metadata");
    assert_eq!(
        replacement_metadata.latest_rollback_seq,
        replacement_lines.first().map(|line| line.seq)
    );
    assert_eq!(replacement_metadata.message_count, 1);
    assert_eq!(replacement_lines.len(), replacement_messages.len() + 1);
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
        vec!["m-user-1", "m-assistant-1", "m-user-1"]
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

/// Verifies rollback replacement history becomes the replay baseline instead of stale earlier segments.
/// 验证回滚 replacement history 会成为重放基线，而不是继续依赖更早的陈旧分段。
#[test]
fn replay_uses_rollback_replacement_history_as_anchor() {
    let root = test_root("replay_uses_rollback_replacement_history_as_anchor");
    let session_dir =
        create_segmented_session(&root, test_session_request(SegmentPolicy::default()))
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
    append_segmented_session_message_tail(&session_dir, &original_messages)
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
    let anchor = scan.anchor.expect("replacement anchor");
    assert_eq!(
        anchor.anchor_kind,
        SegmentedSessionEventKind::ThreadRolledBack
    );

    let replay = replay_segmented_session_context(&session_dir).expect("replay segmented context");
    let replay_messages =
        conversation_state_to_session_messages(&replay.expect("replay result").conversation_state);
    assert_eq!(replay_messages.len(), 2);
    assert_eq!(replay_messages[1].body, "stream prefix and completed body");
}

/// Verifies caller-confirmed suffix append never creates a rollback replacement.
/// 验证调用方确认的尾部追加不会创建回滚替换。
#[test]
fn append_message_suffix_extends_tail_without_rollback_replacement() {
    let root = test_root("append_message_suffix_extends_tail_without_rollback_replacement");
    let session_dir =
        create_segmented_session(&root, test_session_request(SegmentPolicy::default()))
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
    append_segmented_session_message_tail(&session_dir, &original_messages)
        .expect("append original messages");
    let suffix_messages = vec![
        test_message(
            "m-compaction-start",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::CompactionStatus,
            "",
        ),
        test_message(
            "m-compaction-end",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::CompactionStatus,
            "",
        ),
    ];

    let suffix_lines = append_segmented_session_message_suffix(&session_dir, &suffix_messages)
        .expect("append suffix messages");

    assert_eq!(suffix_lines.len(), suffix_messages.len());
    assert!(
        suffix_lines
            .iter()
            .all(|line| line.kind != SegmentedSessionEventKind::ThreadRolledBack)
    );
    assert_eq!(
        suffix_lines[0]
            .payload
            .get("message_index")
            .and_then(Value::as_u64),
        Some(2)
    );
    let metadata: SegmentedSessionMetadata =
        read_required_json(&session_dir.join(SEGMENTED_METADATA_FILE))
            .expect("read suffix metadata");
    assert_eq!(metadata.message_count, 4);
    assert_eq!(metadata.latest_rollback_seq, None);
}

/// Verifies prompt-edit rewinds physically truncate later events without a rollback marker.
/// 验证提示词编辑回退会物理截断后续事件且不写入回滚标记。
#[test]
fn truncate_before_message_removes_tail_without_rollback_marker() {
    let root = test_root("truncate_before_message_removes_tail_without_rollback_marker");
    let session_dir = create_segmented_session(
        &root,
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
    append_segmented_session_message_tail(&session_dir, &messages)
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

/// Verifies a missing prompt id cannot partially mutate segment files.
/// 验证缺失的提示词标识不会导致分段文件发生局部改写。
#[test]
fn truncate_before_missing_message_leaves_segments_unchanged() {
    let root = test_root("truncate_before_missing_message_leaves_segments_unchanged");
    let session_dir = create_segmented_session(
        &root,
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
    append_segmented_session_message_tail(&session_dir, &messages)
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

/// Verifies messages removed by rollback can be reintroduced with the same id later.
/// 验证被回滚删除的消息后续可以用相同标识重新引入。
#[test]
fn append_after_rollback_ignores_removed_pre_rollback_messages() {
    let root = test_root("append_after_rollback_ignores_removed_pre_rollback_messages");
    let session_dir =
        create_segmented_session(&root, test_session_request(SegmentPolicy::default()))
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
    append_segmented_session_message_tail(&session_dir, &original_messages)
        .expect("append original messages");
    let replacement_messages = vec![original_messages[0].clone(), original_messages[1].clone()];
    let replacement_lines =
        append_segmented_session_message_update(&session_dir, &replacement_messages)
            .expect("append rollback replacement");
    assert_eq!(
        replacement_lines.first().map(|line| line.kind),
        Some(SegmentedSessionEventKind::ThreadRolledBack)
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

/// Verifies display maintenance backs up and rewrites bloated rollback replacement segments.
/// 验证显示层维护会备份并重写膨胀的回滚 replacement 分段。
#[test]
fn display_maintenance_rewrites_bloated_rollback_segments_with_backup() {
    let root = test_root("display_maintenance_rewrites_bloated_rollback_segments_with_backup");
    let mut policy = SegmentPolicy::default();
    policy.max_bytes = 700;
    let session_dir =
        create_segmented_session(&root, test_session_request(policy)).expect("create session");
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
    append_segmented_session_message_tail(&session_dir, &messages)
        .expect("append initial messages");
    for index in 0..4 {
        messages[1].body.push_str(&format!(
            " completed chunk {index} with enough text to force rollover"
        ));
        append_segmented_session_message_update(&session_dir, &messages)
            .expect("append replacement update");
    }
    let before_metadata = read_session_metadata(&session_dir).expect("read before metadata");
    assert!(before_metadata.segments.len() > 2);
    assert!(before_metadata.latest_rollback_seq.is_some());

    let report = repair_segmented_session_display_if_needed(&session_dir)
        .expect("repair display")
        .expect("maintenance report");

    assert!(Path::new(&report.backup_dir).join("original").exists());
    assert!(
        Path::new(&report.backup_dir)
            .join("maintenance_report.json")
            .exists()
    );
    let after_metadata = read_session_metadata(&session_dir).expect("read after metadata");
    assert!(after_metadata.segments.len() < before_metadata.segments.len());
    assert_eq!(after_metadata.latest_rollback_seq, None);
    let replay = replay_segmented_session_context(&session_dir).expect("replay repaired context");
    let replay_messages =
        conversation_state_to_session_messages(&replay.expect("replay result").conversation_state);
    assert_eq!(replay_messages.len(), 2);
    assert_eq!(replay_messages[1].body, messages[1].body);
}
