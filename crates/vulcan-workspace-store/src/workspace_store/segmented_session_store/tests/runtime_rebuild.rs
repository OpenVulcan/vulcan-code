use super::*;

/// Verifies runtime JSONL can be rebuilt from canonical conversation state and read back.
/// 验证 runtime JSONL 可从规范会话状态重建并读取还原。
#[test]
fn runtime_log_rebuilds_and_restores_conversation_state() {
    let root = test_root("runtime_log_rebuilds_and_restores_conversation_state");
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
            "m-assistant-1",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::AssistantOutput,
            "first answer",
        ),
    ];
    let conversation_state = conversation_state_from_session_messages(&messages);

    let metadata = rebuild_runtime_log_from_conversation_state(
        &session_dir,
        &conversation_state,
        "2026-06-09T10:00:00Z",
    )
    .expect("rebuild runtime log");
    let restored = read_runtime_conversation_state(&session_dir)
        .expect("read runtime conversation")
        .expect("runtime conversation exists");

    assert_eq!(metadata.active_segment, "runtime/r001.jsonl");
    assert!(session_dir.join(SEGMENTED_RUNTIME_METADATA_FILE).exists());
    assert!(session_dir.join(SEGMENTED_RUNTIME_INDEX_FILE).exists());
    assert!(session_dir.join("runtime").join("r001.jsonl").exists());
    assert_eq!(restored.messages.len(), conversation_state.messages.len());
}

/// Verifies missing runtime logs are rebuilt from metadata-indexed display segments.
/// 验证缺失的 runtime 日志会从 metadata 索引的展示分段重建。
#[test]
fn runtime_log_missing_rebuilds_from_segmented_display_segments() {
    let root = test_root("runtime_log_missing_rebuilds_from_segmented_display_segments");
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
            "m-assistant-1",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::AssistantOutput,
            "first answer",
        ),
    ];
    append_segmented_session_message_tail(&session_dir, &messages).expect("append messages");

    let restored = ensure_runtime_log_from_segmented_context(&session_dir)
        .expect("ensure runtime log")
        .expect("restored conversation");

    assert!(session_dir.join(SEGMENTED_RUNTIME_METADATA_FILE).exists());
    assert_eq!(restored.messages.len(), 2);
    assert_eq!(restored.messages[0].id, "m-user-1");
}

/// Verifies runtime rebuild follows metadata-indexed display segments after rollover.
/// 验证 runtime 重建会在展示分段滚动后继续按 metadata 索引读取。
#[test]
fn runtime_log_missing_rebuilds_from_non_initial_display_segments() {
    let root = test_root("runtime_log_missing_rebuilds_from_non_initial_display_segments");
    let policy = SegmentPolicy {
        max_lines: 2,
        ..SegmentPolicy::default()
    };
    let session_dir =
        create_segmented_session(&root, test_session_request(policy)).expect("create session");
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
        test_message(
            "m-user-3",
            SessionMessageKind::User,
            SessionMessageRecordKind::Transcript,
            "third prompt",
        ),
    ];
    append_segmented_session_message_tail(&session_dir, &messages).expect("append messages");
    let metadata = read_session_metadata(&session_dir).expect("read metadata");

    assert_eq!(metadata.active_segment, "s003.jsonl");
    assert_eq!(metadata.segments.len(), 3);

    let restored = ensure_runtime_log_from_segmented_context(&session_dir)
        .expect("ensure runtime log")
        .expect("restored conversation");

    assert_eq!(restored.messages.len(), messages.len());
    assert_eq!(
        restored.messages.last().map(|message| message.id.as_str()),
        Some("m-user-3")
    );
    assert!(session_dir.join("runtime").join("r001.jsonl").exists());
}

/// Verifies a corrupt runtime JSONL row is backed up and removed without losing valid rows.
/// 验证损坏 runtime JSONL 行会被备份并移除，同时不丢失有效行。
#[test]
fn runtime_log_tolerates_and_repairs_corrupt_jsonl_tail() {
    let root = test_root("runtime_log_tolerates_and_repairs_corrupt_jsonl_tail");
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
            "m-assistant-1",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::AssistantOutput,
            "first answer",
        ),
    ];
    let conversation_state = conversation_state_from_session_messages(&messages);
    rebuild_runtime_log_from_conversation_state(
        &session_dir,
        &conversation_state,
        "2026-06-09T10:00:00Z",
    )
    .expect("rebuild runtime log");
    let runtime_segment_path = session_dir.join("runtime").join("r001.jsonl");
    {
        let mut file = OpenOptions::new()
            .append(true)
            .open(&runtime_segment_path)
            .expect("open runtime segment");
        writeln!(file, "{{ broken").expect("append corrupt line");
    }

    let lines = read_runtime_log_lines(&session_dir)
        .expect("read repaired runtime lines")
        .expect("runtime lines exist");
    let cleaned = read_json_lines::<RuntimeLogEventLine>(&runtime_segment_path)
        .expect("read cleaned runtime segment");
    let corrupt_backups = fs::read_dir(session_dir.join("runtime"))
        .expect("read runtime dir")
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().contains(".corrupt-"))
        .count();

    assert_eq!(lines.len(), cleaned.len());
    assert_eq!(lines.len(), 3);
    let repair_event = match lines.last().map(|line| &line.event) {
        Some(RuntimeLogEvent::RuntimeRepair(event)) => event,
        _ => panic!("expected runtime repair event"),
    };
    assert!(repair_event.detail.contains("runtime/r001.jsonl lines 3"));
    assert_eq!(corrupt_backups, 1);
    assert!(session_dir.join(SEGMENTED_RUNTIME_INDEX_FILE).exists());
}

/// Verifies fully damaged runtime logs are regenerated from display transcript segments.
/// 验证完全损坏的 runtime 日志会从展示转录分段重新生成。
#[test]
fn runtime_log_full_corruption_falls_back_to_segmented_replay() {
    let root = test_root("runtime_log_full_corruption_falls_back_to_segmented_replay");
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
            "m-assistant-1",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::AssistantOutput,
            "first answer",
        ),
    ];
    append_segmented_session_message_tail(&session_dir, &messages).expect("append messages");
    let conversation_state = conversation_state_from_session_messages(&messages);
    rebuild_runtime_log_from_conversation_state(
        &session_dir,
        &conversation_state,
        "2026-06-09T10:00:00Z",
    )
    .expect("rebuild runtime log");
    fs::write(session_dir.join("runtime").join("r001.jsonl"), "{ broken\n")
        .expect("damage runtime segment");

    let restored = ensure_runtime_log_from_segmented_context(&session_dir)
        .expect("ensure runtime log")
        .expect("restored conversation");
    let lines = read_runtime_log_lines(&session_dir)
        .expect("read rebuilt runtime lines")
        .expect("runtime lines exist");

    assert_eq!(restored.messages.len(), 2);
    assert_eq!(restored.messages[0].id, "m-user-1");
    assert_eq!(lines.len(), 2);
}
