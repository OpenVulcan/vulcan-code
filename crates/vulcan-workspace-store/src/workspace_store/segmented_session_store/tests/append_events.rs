use super::*;

/// Verifies append writes only new lines and updates sequence metadata.
/// 验证追加只写入新行并更新序号元数据。
#[test]
fn append_session_events_advances_sequence_without_rewriting_first_line() {
    let root = test_root("append_session_events_advances_sequence_without_rewriting_first_line");
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        test_session_request(SegmentPolicy::default()),
    )
    .expect("create segmented session");
    let before_lines = read_segment_event_lines(&session_dir.join("s001.jsonl"))
        .expect("read lines before append");

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
        ],
    )
    .expect("append events");

    let after_lines =
        read_segment_event_lines(&session_dir.join("s001.jsonl")).expect("read lines after append");
    assert_eq!(after_lines[0], before_lines[0]);
    assert_eq!(after_lines.len(), 3);
    assert_eq!(after_lines[1].seq, 1);
    assert_eq!(after_lines[2].seq, 2);

    let metadata: SegmentedSessionMetadata =
        read_required_json(&session_dir.join(SEGMENTED_METADATA_FILE)).expect("read metadata");
    assert_eq!(metadata.next_global_seq, 3);
    assert_eq!(metadata.segments[0].line_count, 3);
}

/// Verifies an explicit load upgrades legacy segment metadata with exact sequence and message locators.
/// 验证显式加载会为旧版分段元数据补齐精确序号与消息定位信息。
#[test]
fn explicit_load_upgrades_legacy_segment_locators() {
    let root = test_root("explicit_load_upgrades_legacy_segment_locators");
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        test_session_request(SegmentPolicy::default()),
    )
    .expect("create segmented session");
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
    append_segmented_session_message_update(&session_dir, &messages)
        .expect("append transcript messages");
    let metadata_path = session_dir.join(SEGMENTED_METADATA_FILE);
    let mut legacy_metadata: Value =
        read_required_json(&metadata_path).expect("read current metadata as JSON");
    let legacy_segment = legacy_metadata["segments"]
        .as_array_mut()
        .and_then(|segments| segments.first_mut())
        .and_then(Value::as_object_mut)
        .expect("read first segment object");
    legacy_segment.remove("last_seq");
    legacy_segment.remove("first_message_index");
    legacy_segment.remove("last_message_index");
    fs::write(
        &metadata_path,
        serde_json::to_vec_pretty(&legacy_metadata).expect("serialize legacy metadata"),
    )
    .expect("write legacy metadata fixture");

    read_segmented_session_event_window(&session_dir, None, 16)
        .expect("explicit load upgrades segment locators");

    let upgraded_metadata: SegmentedSessionMetadata =
        read_required_json(&metadata_path).expect("read upgraded metadata");
    let upgraded_segment = upgraded_metadata
        .segments
        .first()
        .expect("read upgraded segment");
    assert_eq!(upgraded_segment.last_seq, Some(2));
    assert_eq!(upgraded_segment.first_message_index, Some(0));
    assert_eq!(upgraded_segment.last_message_index, Some(1));
}

/// Verifies hot append rejects a missing newline until an explicit load performs repair.
/// 验证热路径追加会拒绝缺失换行，直至显式加载执行修复。
#[test]
fn append_session_events_reject_missing_jsonl_tail_newline_until_load_repair() {
    let root = test_root("append_session_events_repairs_missing_jsonl_tail_newline");
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        test_session_request(SegmentPolicy::default()),
    )
    .expect("create segmented session");
    let segment_path = session_dir.join("s001.jsonl");
    let mut bytes = fs::read(&segment_path).expect("read segment bytes");
    assert_eq!(bytes.pop(), Some(b'\n'));
    fs::write(&segment_path, bytes).expect("rewrite segment without trailing newline");

    let append_error = append_segmented_session_events(
        &session_dir,
        &[test_event(
            "2026-06-06T10:01:00Z",
            SegmentedSessionEventKind::UserPrompt,
        )],
    )
    .expect_err("hot append must reject a missing newline without scanning history");
    assert!(append_error.contains("load-time repair is required"));
    read_segmented_session_event_window(&session_dir, None, 16)
        .expect("explicit read repairs missing newline");
    append_segmented_session_events(
        &session_dir,
        &[test_event(
            "2026-06-06T10:01:00Z",
            SegmentedSessionEventKind::UserPrompt,
        )],
    )
    .expect("append event after explicit load repair");

    let text = fs::read_to_string(&segment_path).expect("read repaired segment text");
    let lines = text.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 2);
    assert!(
        lines
            .iter()
            .all(|line| serde_json::from_str::<Value>(line).is_ok())
    );
    let events = read_segment_event_lines(&segment_path).expect("read repaired lines");
    assert_eq!(events[0].seq, 0);
    assert_eq!(events[1].seq, 1);
}

/// Verifies hot append rejects a truncated tail until an explicit load performs repair.
/// 验证热路径追加会拒绝截断尾部，直至显式加载执行修复。
#[test]
fn append_session_events_reject_truncated_jsonl_tail_until_load_repair() {
    let root = test_root("append_session_events_repairs_truncated_jsonl_tail_line");
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        test_session_request(SegmentPolicy::default()),
    )
    .expect("create segmented session");
    append_segmented_session_events(
        &session_dir,
        &[test_event(
            "2026-06-06T10:01:00Z",
            SegmentedSessionEventKind::UserPrompt,
        )],
    )
    .expect("append first event");
    let segment_path = session_dir.join("s001.jsonl");
    let mut file = OpenOptions::new()
        .append(true)
        .open(&segment_path)
        .expect("open segment for corruption");
    write!(file, "{{\"seq\":999,\"timestamp\":\"broken\"").expect("append truncated json tail");

    let append_error = append_segmented_session_events(
        &session_dir,
        &[test_event(
            "2026-06-06T10:02:00Z",
            SegmentedSessionEventKind::AssistantOutput,
        )],
    )
    .expect_err("hot append must reject a truncated tail without scanning history");
    assert!(append_error.contains("load-time repair is required"));
    read_segmented_session_event_window(&session_dir, None, 16)
        .expect("explicit read repairs truncated tail");
    append_segmented_session_events(
        &session_dir,
        &[test_event(
            "2026-06-06T10:02:00Z",
            SegmentedSessionEventKind::AssistantOutput,
        )],
    )
    .expect("append event after explicit load repair");

    let text = fs::read_to_string(&segment_path).expect("read repaired segment text");
    assert!(
        text.lines()
            .all(|line| serde_json::from_str::<Value>(line).is_ok())
    );
    let events = read_segment_event_lines(&segment_path).expect("read repaired lines");
    assert_eq!(events.len(), 3);
    assert_eq!(events[0].seq, 0);
    assert_eq!(events[1].seq, 1);
    assert_eq!(events[2].seq, 2);
    let metadata: SegmentedSessionMetadata =
        read_required_json(&session_dir.join(SEGMENTED_METADATA_FILE))
            .expect("read repaired metadata");
    assert_eq!(metadata.next_global_seq, 3);
    assert_eq!(metadata.segments[0].line_count, 3);
}

/// Unknown historical segment event types are skipped instead of failing the whole session load.
/// 未知历史分段事件类型会被跳过，而不是导致整个会话加载失败。
#[test]
fn read_segment_event_lines_skips_unknown_historical_event_types() {
    let root = test_root("read_segment_event_lines_skips_unknown_historical_event_types");
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        test_session_request(SegmentPolicy::default()),
    )
    .expect("create session");
    let segment_path = session_dir.join("s001.jsonl");
    let mut file = OpenOptions::new()
        .append(true)
        .open(&segment_path)
        .expect("open segment");
    writeln!(
        file,
        "{}",
        json!({
            "seq": 99,
            "timestamp": "2026",
            "type": "codex_boundary",
            "payload": {"kind": "turn_started"}
        })
    )
    .expect("append unknown line");

    let lines = read_segment_event_lines(&segment_path).expect("read tolerant lines");

    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].kind, SegmentedSessionEventKind::SessionMeta);
}

/// Verifies line-count based rollover creates ordered segments.
/// 验证基于行数的滚动会创建有序分段。
#[test]
fn append_session_events_rolls_over_by_line_count() {
    let root = test_root("append_session_events_rolls_over_by_line_count");
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
        ],
    )
    .expect("append rollover events");

    let metadata: SegmentedSessionMetadata =
        read_required_json(&session_dir.join(SEGMENTED_METADATA_FILE)).expect("read metadata");
    assert_eq!(metadata.active_segment, "s002.jsonl");
    assert_eq!(metadata.segments.len(), 2);
    assert_eq!(metadata.segments[0].line_count, 2);
    assert_eq!(metadata.segments[1].line_count, 1);
    assert_eq!(metadata.segments[1].start_seq, 2);
}

/// Verifies complete transcript updates append verified tails idempotently and preserve event kinds.
/// 验证完整转录更新会幂等追加已校验尾部，并保留事件类型。
#[test]
fn append_message_update_is_idempotent_and_maps_transcript_kinds() {
    let root = test_root("append_message_update_is_idempotent_and_maps_transcript_kinds");
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        test_session_request(SegmentPolicy {
            max_lines: 4,
            max_bytes: DEFAULT_SEGMENT_MAX_BYTES,
        }),
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

    let first_append = append_segmented_session_message_update(&session_dir, &first_messages)
        .expect("append first tail");
    assert_eq!(first_append.len(), 2);
    assert_eq!(first_append[0].kind, SegmentedSessionEventKind::UserPrompt);
    assert_eq!(
        first_append[1].kind,
        SegmentedSessionEventKind::AssistantOutput
    );

    let second_append = append_segmented_session_message_update(&session_dir, &first_messages)
        .expect("repeat append");
    assert!(second_append.is_empty());

    let mut second_messages = first_messages.clone();
    second_messages.push(test_message(
        "m-tool-1",
        SessionMessageKind::Assistant,
        SessionMessageRecordKind::ToolResultOutput,
        "tool output",
    ));
    let appended_tail = append_segmented_session_message_update(&session_dir, &second_messages)
        .expect("append second tail");
    assert_eq!(appended_tail.len(), 1);
    assert_eq!(
        appended_tail[0].kind,
        SegmentedSessionEventKind::DisplayEvent
    );

    let metadata: SegmentedSessionMetadata =
        read_required_json(&session_dir.join(SEGMENTED_METADATA_FILE)).expect("read metadata");
    assert_eq!(metadata.next_global_seq, 4);
    assert_eq!(metadata.message_count, 3);
}

/// Verifies exact suffix append opens only the active segment and ignores an unreadable inactive segment.
/// 验证精确后缀追加仅打开活动分段，并忽略不可读的非活动分段。
#[test]
fn append_exact_message_tail_does_not_read_inactive_segments() {
    let root = test_root("append_exact_message_tail_does_not_read_inactive_segments");
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        test_session_request(SegmentPolicy {
            max_lines: 2,
            max_bytes: DEFAULT_SEGMENT_MAX_BYTES,
        }),
    )
    .expect("create session");
    let user = test_message(
        "m-user-1",
        SessionMessageKind::User,
        SessionMessageRecordKind::Transcript,
        "prompt",
    );
    let assistant = test_message(
        "m-assistant-1",
        SessionMessageKind::Assistant,
        SessionMessageRecordKind::AssistantOutput,
        "answer",
    );
    append_segmented_session_message_update(&session_dir, &[user, assistant.clone()])
        .expect("append initial messages with rollover");
    let metadata: SegmentedSessionMetadata =
        read_required_json(&session_dir.join(SEGMENTED_METADATA_FILE)).expect("read metadata");
    assert_eq!(metadata.active_segment, "s002.jsonl");
    fs::write(
        session_dir.join("s001.jsonl"),
        b"unreadable historical segment",
    )
    .expect("corrupt inactive segment");
    let tool_result = test_message(
        "m-tool-1",
        SessionMessageKind::Assistant,
        SessionMessageRecordKind::ToolResultOutput,
        "tool output",
    );

    let appended = append_segmented_session_message_tail_update(
        &session_dir,
        2,
        Some(&assistant),
        &[tool_result],
    )
    .expect("append exact tail without reading inactive segment");

    assert_eq!(appended.len(), 1);
    let metadata: SegmentedSessionMetadata =
        read_required_json(&session_dir.join(SEGMENTED_METADATA_FILE)).expect("read metadata");
    assert_eq!(metadata.message_count, 3);
}

/// Verifies exact suffix append rejects a stale caller checkpoint without repairing or scanning history.
/// 验证精确后缀追加会拒绝过期调用方检查点，且不会修复或扫描历史。
#[test]
fn append_exact_message_tail_rejects_stale_checkpoint() {
    let root = test_root("append_exact_message_tail_rejects_stale_checkpoint");
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        test_session_request(SegmentPolicy::default()),
    )
    .expect("create session");
    let user = test_message(
        "m-user-1",
        SessionMessageKind::User,
        SessionMessageRecordKind::Transcript,
        "prompt",
    );
    append_segmented_session_message_update(&session_dir, std::slice::from_ref(&user))
        .expect("append initial message");
    let assistant = test_message(
        "m-assistant-1",
        SessionMessageKind::Assistant,
        SessionMessageRecordKind::AssistantOutput,
        "answer",
    );

    let error = append_segmented_session_message_tail_update(
        &session_dir,
        0,
        None,
        std::slice::from_ref(&assistant),
    )
    .expect_err("reject stale checkpoint");

    assert!(error.contains("checkpoint advanced from 0 to 1"));
    let metadata: SegmentedSessionMetadata =
        read_required_json(&session_dir.join(SEGMENTED_METADATA_FILE)).expect("read metadata");
    assert_eq!(metadata.message_count, 1);
}

/// Verifies message update repairs duplicate persisted message indexes before checkpoint validation.
/// 验证消息更新会在检查点校验前修复重复的持久化消息索引。
#[test]
fn append_message_update_rejects_duplicate_index_until_load_repair() {
    let root = test_root("append_message_update_repairs_duplicate_persisted_message_index");
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        test_session_request(SegmentPolicy::default()),
    )
    .expect("create session");
    let original_user = test_message(
        "m-user-1",
        SessionMessageKind::User,
        SessionMessageRecordKind::Transcript,
        "original prompt",
    );
    let assistant = test_message(
        "m-assistant-1",
        SessionMessageKind::Assistant,
        SessionMessageRecordKind::AssistantOutput,
        "answer",
    );
    append_segmented_session_message_update(
        &session_dir,
        &[original_user.clone(), assistant.clone()],
    )
    .expect("append original messages");

    let replacement_user = test_message(
        "m-user-1",
        SessionMessageKind::User,
        SessionMessageRecordKind::Transcript,
        "replacement prompt",
    );
    let duplicate_event =
        segmented_event_input_from_message(0, &replacement_user, 0).expect("duplicate event");
    let duplicate_line = SegmentedSessionEventLine {
        seq: 99,
        timestamp: duplicate_event.timestamp,
        kind: duplicate_event.kind,
        payload: duplicate_event.payload,
    };
    let segment_path = session_dir.join("s001.jsonl");
    let mut file = OpenOptions::new()
        .append(true)
        .open(&segment_path)
        .expect("open segment");
    writeln!(
        file,
        "{}",
        serde_json::to_string(&duplicate_line).expect("serialize duplicate line")
    )
    .expect("append duplicate message index");

    let append_error = append_segmented_session_message_update(
        &session_dir,
        &[replacement_user.clone(), assistant.clone()],
    )
    .expect_err("hot message append must reject external segment growth");
    assert!(append_error.contains("load-time repair is required"));
    read_segmented_session_event_window(&session_dir, None, 16)
        .expect("explicit read repairs duplicate message index");
    append_segmented_session_message_update(
        &session_dir,
        &[replacement_user.clone(), assistant.clone()],
    )
    .expect("update messages after explicit load repair");

    let lines = read_segment_event_lines(&segment_path).expect("read repaired lines");
    let messages = segmented_lines_to_session_messages(&lines).expect("project messages");
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].body, "replacement prompt");
    assert_eq!(messages[1].body, "answer");
    let message_indexes = lines
        .iter()
        .filter_map(|line| line.payload.get("message_index").and_then(Value::as_u64))
        .collect::<Vec<_>>();
    assert_eq!(message_indexes, vec![0, 1]);
    let metadata: SegmentedSessionMetadata =
        read_required_json(&session_dir.join(SEGMENTED_METADATA_FILE)).expect("read metadata");
    assert_eq!(metadata.message_count, 2);
}

/// Verifies tool-call detail references are backfilled with assigned JSONL sequence numbers.
/// 验证工具调用详情引用会回填已分配的 JSONL 序号。
#[test]
fn append_message_update_backfills_tool_detail_ref_sequence() {
    let root = test_root("append_message_update_backfills_tool_detail_ref_sequence");
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        test_session_request(SegmentPolicy::default()),
    )
    .expect("create session");
    let user = test_message(
        "m-user",
        SessionMessageKind::User,
        SessionMessageRecordKind::Transcript,
        "prompt",
    );
    let mut call = test_message(
        "m-call",
        SessionMessageKind::Assistant,
        SessionMessageRecordKind::ToolCallOutput,
        "",
    );
    call.metadata = Some(json!({
        "tool_call_id": "call-1",
        "tool_name": "shell_command",
        "result": {
            "status": "completed",
            "detail_ref": {
                "kind": "session_record",
                "record_id": "m-detail",
                "message_index": 2
            }
        }
    }));
    let mut detail = test_message(
        "m-detail",
        SessionMessageKind::Assistant,
        SessionMessageRecordKind::ToolDetailOutput,
        "result",
    );
    detail.visible = false;
    detail.metadata = Some(json!({
        "tool_detail": true,
        "tool_call_id": "call-1",
    }));

    let lines = append_segmented_session_message_update(&session_dir, &[user, call, detail])
        .expect("append messages");

    assert_eq!(
        lines[1].payload["message"]["metadata"]["result"]["detail_ref"]["seq"],
        json!(3)
    );
    assert_eq!(lines[2].seq, 3);
}
