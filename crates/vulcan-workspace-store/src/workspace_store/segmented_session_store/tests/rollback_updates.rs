use super::*;

/// Verifies a compacted runtime window cannot overwrite the complete persisted display history.
/// 验证压缩后的运行时窗口不能覆盖完整的持久化展示历史。
#[test]
fn compacted_runtime_window_cannot_rewrite_display_history() {
    // root isolates the destructive-compaction regression fixture.
    // root 隔离破坏性压缩回归夹具。
    let root = test_root("compacted_runtime_window_cannot_rewrite_display_history");
    // session_dir owns the exact segmented display history under test.
    // session_dir 持有本测试使用的精确分段展示历史。
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        test_session_request(SegmentPolicy::default()),
    )
    .expect("create session");
    // complete_history models the display rows that must survive context-anchor compaction.
    // complete_history 模拟上下文锚点压缩后仍必须保留的展示记录。
    let complete_history = vec![
        test_message(
            "m-user-old",
            SessionMessageKind::User,
            SessionMessageRecordKind::Transcript,
            "older prompt",
        ),
        test_message(
            "m-thinking-old",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::ThinkingOutput,
            "older reasoning",
        ),
        test_message(
            "m-tool-old",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::ToolCallOutput,
            "older tool",
        ),
        test_message(
            "m-assistant-old",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::AssistantOutput,
            "older answer",
        ),
    ];
    append_segmented_session_message_update(&session_dir, &complete_history)
        .expect("append complete display history");
    // compacted_window models the much shorter provider-facing record set after compaction.
    // compacted_window 模拟压缩后大幅缩短的 Provider 可见记录集合。
    let compacted_window = vec![
        test_message(
            "m-compaction-status",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::CompactionStatus,
            "上下文已压缩",
        ),
        test_message(
            "m-user-current",
            SessionMessageKind::User,
            SessionMessageRecordKind::Transcript,
            "continue",
        ),
    ];

    // error proves ordinary persistence stopped before any destructive rewrite.
    // error 证明普通持久化在任何破坏性重写前已经停止。
    let error = append_segmented_session_message_update(&session_dir, &compacted_window)
        .expect_err("reject compacted runtime window as display replacement");

    assert!(error.contains("Ordinary persistence cannot rewrite or shorten display history"));
    // replay reloads the canonical display rows after the rejected mutation.
    // replay 在变更被拒绝后重新加载规范展示记录。
    let replay = replay_segmented_session_context(&session_dir)
        .expect("replay retained display history")
        .expect("retained display replay result");
    // replay_messages is the exact retained row sequence visible after reload.
    // replay_messages 是重载后可见的精确保留记录序列。
    let replay_messages = conversation_state_to_session_messages(&replay.conversation_state);
    assert_eq!(replay_messages, complete_history);
    // metadata proves the durable message boundary did not shrink from four to two.
    // metadata 证明可靠消息边界没有从四条缩短为两条。
    let metadata = read_session_metadata(&session_dir).expect("read retained metadata");
    assert_eq!(metadata.message_count, 4);
}

/// Verifies explicit maintenance can insert a transcript row through a physical rewrite.
/// 验证显式维护可通过物理重写插入转录行。
#[test]
fn explicit_repair_physically_rewrites_inserted_history() {
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
    let changed_lines =
        replace_segmented_session_message_records_for_repair(&session_dir, &changed_messages)
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

/// Verifies explicit maintenance repairs an incomplete persisted tail checkpoint.
/// 验证显式维护会修复不完整的持久化尾部检查点。
#[test]
fn explicit_repair_rewrites_incomplete_tail_checkpoint() {
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
    let changed_lines =
        replace_segmented_session_message_records_for_repair(&session_dir, &changed_messages)
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

/// Verifies explicit maintenance can shorten message rows without rollback bloat.
/// 验证显式维护可缩短消息行且不产生回滚膨胀。
#[test]
fn explicit_repair_physically_rewrites_shortened_history() {
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
        replace_segmented_session_message_records_for_repair(&session_dir, &replacement_messages)
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
    replace_segmented_session_message_records_for_repair(&session_dir, &replacement_messages)
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

/// Verifies prompt edit can rewind an already-durable UserInput boundary when its display row was overtaken.
/// 验证展示记录被抢占时，提示词编辑仍可回退已经可靠落盘的 UserInput 边界。
#[test]
fn truncate_before_boundary_user_prompt_recovers_missing_display_append() {
    // root isolates the missing-user-display regression fixture.
    // root 隔离用户展示记录缺失的回归夹具。
    let root = test_root("truncate_before_boundary_user_prompt_recovers_missing_display_append");
    // sessionId owns both the segmented directory and boundary thread identity.
    // sessionId 同时拥有分段目录与边界线程身份。
    let session_id = Uuid::new_v4();
    // sessionDir uses numeric timestamps so its metadata index can address the target file exactly.
    // sessionDir 使用数字时间戳，使元数据索引能够精确定位目标文件。
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        SegmentedSessionCreateRequest {
            session_id: session_id.to_string(),
            project_tag: String::from("project-alpha"),
            created_date: String::from("2026-08-25"),
            created_at: String::from("100"),
            project_id: Some(String::from("project-alpha")),
            title: Some(String::from("boundary edit")),
            title_source: Some(String::from("test")),
            segment_policy: SegmentPolicy {
                max_lines: 2,
                max_bytes: DEFAULT_SEGMENT_MAX_BYTES,
            },
        },
    )
    .expect("create boundary edit session");
    // threadId is the exact main-session boundary owner.
    // threadId 是精确的主会话边界所有者。
    let thread_id = vulcan_session_core::SessionNodeId::from(session_id);
    // boundaryEvents persist the user turn even though the display append below intentionally omits it.
    // boundaryEvents 会持久化用户回合，而下方展示追加会有意省略该用户记录。
    let boundary_events = vec![
        SessionBoundaryEvent::new(
            "turn-edit-started",
            thread_id,
            0,
            SessionBoundaryEventKind::TurnStarted,
            vulcan_session_boundary::SessionEventScope::Turn,
            SessionEventVisibility::Hidden,
            101,
        )
        .with_turn_id("turn-edit"),
        SessionBoundaryEvent::new(
            "turn-edit-user-input",
            thread_id,
            1,
            SessionBoundaryEventKind::UserInput,
            vulcan_session_boundary::SessionEventScope::Turn,
            SessionEventVisibility::Visible,
            101,
        )
        .with_turn_id("turn-edit")
        .with_payload(json!({
            "prompt": "edited prompt",
            "attachments": [],
            "source": "user_followup",
            "editable": true,
        })),
    ];
    append_segmented_boundary_events(&session_dir, &boundary_events)
        .expect("append durable user boundary");
    // assistantOnlyMessage reproduces the overtaking write that occupied message index zero.
    // assistantOnlyMessage 复现抢先占用展示消息索引零的助手写入。
    let mut assistant_only_message =
        SessionMessageEntry::new(SessionMessageKind::Assistant, "", "answer")
            .with_runtime_metadata("assistant-only", "200");
    assistant_only_message.record_kind = SessionMessageRecordKind::AssistantOutput;
    append_segmented_session_message_update(&session_dir, &[assistant_only_message])
        .expect("append overtaking assistant display row");
    // targetMessage is the boundary-projected user row already displayed by hydration.
    // targetMessage 是水合过程已经展示的边界投影用户记录。
    let mut target_message =
        SessionMessageEntry::new(SessionMessageKind::User, "", "edited prompt")
            .with_runtime_metadata("live-visible-user", "101");
    target_message.record_kind = SessionMessageRecordKind::Transcript;
    target_message.metadata = Some(json!({
        "source": "session_boundary_event",
        "payload": {
            "prompt": "edited prompt",
            "attachments": [],
            "source": "user_followup",
            "editable": true,
        }
    }));

    truncate_segmented_session_before_boundary_user_prompt(&session_dir, &target_message, &[])
        .expect("truncate through durable boundary fallback");

    // metadata proves the overtaking assistant row and the complete target turn were removed.
    // metadata 证明抢占的助手记录与完整目标回合均已移除。
    let metadata: SegmentedSessionMetadata =
        read_required_json(&session_dir.join(SEGMENTED_METADATA_FILE)).expect("read metadata");
    assert_eq!(metadata.message_count, 0);
    assert_eq!(metadata.session_id, session_id.to_string());
    // retainedLines proves no UserInput or later display row survived the semantic rewind.
    // retainedLines 证明语义回退后没有 UserInput 或更晚的展示记录残留。
    let retained_lines = read_segment_event_lines(&session_dir.join(&metadata.active_segment))
        .expect("read retained exact segment");
    assert!(
        retained_lines
            .iter()
            .all(|line| line.kind != SegmentedSessionEventKind::SessionBoundary)
    );
    assert!(
        retained_lines
            .iter()
            .all(|line| line.payload.get("message").is_none())
    );
}

/// Verifies prompt-edit truncation reads only the metadata-selected target segment.
/// 验证提示词编辑截断仅读取元数据选定的目标分段。
#[test]
fn truncate_before_message_does_not_read_earlier_segments() {
    let root = test_root("truncate_before_message_does_not_read_earlier_segments");
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
        .expect("append messages across segments");
    let corrupt_earlier_segment = b"unreadable earlier segment";
    fs::write(session_dir.join("s001.jsonl"), corrupt_earlier_segment)
        .expect("corrupt earlier segment");

    truncate_segmented_session_before_message(&session_dir, "m-user-2", &messages[..2])
        .expect("truncate through exact target segment");

    assert_eq!(
        fs::read(session_dir.join("s001.jsonl")).expect("read untouched earlier segment"),
        corrupt_earlier_segment
    );
    let metadata: SegmentedSessionMetadata =
        read_required_json(&session_dir.join(SEGMENTED_METADATA_FILE)).expect("read metadata");
    assert_eq!(metadata.message_count, 2);
    assert_eq!(metadata.active_segment, "s002.jsonl");
    assert!(!session_dir.join("s003.jsonl").exists());
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
        replace_segmented_session_message_records_for_repair(&session_dir, &replacement_messages)
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

/// Verifies ordinary persistence rejects an in-place display rewrite without mutating history.
/// 验证普通持久化会拒绝就地展示重写，且不会修改历史。
#[test]
fn ordinary_message_update_rejects_in_place_display_rewrite() {
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
    messages[1]
        .body
        .push_str(" completed body that must not replace the persisted display row");
    let rewrite_error = append_segmented_session_message_update(&session_dir, &messages)
        .expect_err("ordinary update must reject rewritten display rows");
    assert!(rewrite_error.contains("non-append display mutation"));
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
    assert_eq!(replay_messages[1].body, "stream prefix");
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
    replace_segmented_session_message_records_for_repair(&session_dir, &replacement_messages)
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
