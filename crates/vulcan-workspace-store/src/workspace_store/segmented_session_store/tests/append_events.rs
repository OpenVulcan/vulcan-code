use super::*;

/// Verifies append writes only new lines and updates sequence metadata.
/// 验证追加只写入新行并更新序号元数据。
#[test]
fn append_session_events_advances_sequence_without_rewriting_first_line() {
    let root = test_root("append_session_events_advances_sequence_without_rewriting_first_line");
    let session_dir =
        create_segmented_session(&root, test_session_request(SegmentPolicy::default()))
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

/// Unknown historical segment event types are skipped instead of failing the whole session load.
/// 未知历史分段事件类型会被跳过，而不是导致整个会话加载失败。
#[test]
fn read_segment_event_lines_skips_unknown_historical_event_types() {
    let root = test_root("read_segment_event_lines_skips_unknown_historical_event_types");
    let session_dir =
        create_segmented_session(&root, test_session_request(SegmentPolicy::default()))
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
        create_segmented_session(&root, test_session_request(policy)).expect("create session");

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

/// Verifies transcript tail appends are idempotent and preserve transcript-facing event kinds.
/// 验证转录尾部追加具备幂等性，并保留面向转录的事件类型。
#[test]
fn append_message_tail_is_idempotent_and_maps_transcript_kinds() {
    let root = test_root("append_message_tail_is_idempotent_and_maps_transcript_kinds");
    let session_dir = create_segmented_session(
        &root,
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

    let first_append = append_segmented_session_message_tail(&session_dir, &first_messages)
        .expect("append first tail");
    assert_eq!(first_append.len(), 2);
    assert_eq!(first_append[0].kind, SegmentedSessionEventKind::UserPrompt);
    assert_eq!(
        first_append[1].kind,
        SegmentedSessionEventKind::AssistantOutput
    );

    let second_append = append_segmented_session_message_tail(&session_dir, &first_messages)
        .expect("repeat append");
    assert!(second_append.is_empty());

    let mut second_messages = first_messages.clone();
    second_messages.push(test_message(
        "m-tool-1",
        SessionMessageKind::Assistant,
        SessionMessageRecordKind::ToolResultOutput,
        "tool output",
    ));
    let tail_append = append_segmented_session_message_tail(&session_dir, &second_messages)
        .expect("append second tail");
    assert_eq!(tail_append.len(), 1);
    assert_eq!(tail_append[0].kind, SegmentedSessionEventKind::DisplayEvent);

    let metadata: SegmentedSessionMetadata =
        read_required_json(&session_dir.join(SEGMENTED_METADATA_FILE)).expect("read metadata");
    assert_eq!(metadata.next_global_seq, 4);
    assert_eq!(metadata.message_count, 3);
}

/// Verifies tool-call detail references are backfilled with assigned JSONL sequence numbers.
/// 验证工具调用详情引用会回填已分配的 JSONL 序号。
#[test]
fn append_message_tail_backfills_tool_detail_ref_sequence() {
    let root = test_root("append_message_tail_backfills_tool_detail_ref_sequence");
    let session_dir =
        create_segmented_session(&root, test_session_request(SegmentPolicy::default()))
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
            "tool_result_id": "tool-result-call-1",
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
        "tool_result_id": "tool-result-call-1"
    }));

    let lines = append_segmented_session_message_tail(&session_dir, &[user, call, detail])
        .expect("append messages");

    assert_eq!(
        lines[1].payload["message"]["metadata"]["result"]["detail_ref"]["seq"],
        json!(3)
    );
    assert_eq!(lines[2].seq, 3);
}
