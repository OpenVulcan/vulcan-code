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

/// Verifies a resident transcript cursor observes intervening metadata writes and follows rollover.
/// 验证常驻转录游标能够观察中间元数据写入并跟随分段滚动。
#[test]
fn resident_transcript_cursor_reuses_metadata_across_intervening_append() {
    let root = test_root("resident_transcript_cursor_reuses_metadata_across_intervening_append");
    let session_id = Uuid::new_v4();
    let mut request = test_session_request(SegmentPolicy {
        max_lines: 2,
        max_bytes: 1024 * 1024,
    });
    request.session_id = session_id.to_string();
    let session_dir = create_segmented_session_in_location(&root, false, request)
        .expect("create segmented session");
    let (mut cursor, initial_checkpoint) =
        open_segmented_session_transcript_append_cursor(&session_dir, session_id)
            .expect("open resident transcript cursor");
    let first_message = test_message(
        "m-resident-1",
        SessionMessageKind::User,
        SessionMessageRecordKind::Transcript,
        "first",
    );
    let first_checkpoint = append_segmented_session_message_tail_with_cursor(
        &mut cursor,
        &initial_checkpoint,
        std::slice::from_ref(&first_message),
    )
    .expect("append first resident suffix");

    append_segmented_session_events(
        &session_dir,
        &[test_event(
            "2026-06-06T10:04:00Z",
            SegmentedSessionEventKind::AssistantOutput,
        )],
    )
    .expect("append intervening non-transcript event");

    let second_message = test_message(
        "m-resident-2",
        SessionMessageKind::Assistant,
        SessionMessageRecordKind::AssistantOutput,
        "second",
    );
    let second_checkpoint = append_segmented_session_message_tail_with_cursor(
        &mut cursor,
        &first_checkpoint,
        std::slice::from_ref(&second_message),
    )
    .expect("append second resident suffix");
    let metadata = read_session_metadata(&session_dir).expect("read final metadata");

    assert_eq!(second_checkpoint.message_count, 2);
    assert_eq!(metadata.message_count, 2);
    assert!(metadata.segments.len() >= 2);
    assert_eq!(metadata.storage_generation, metadata.next_global_seq);
    assert_eq!(metadata.integrity_version, 1);
}

/// Verifies one resident writer command can cross a segment boundary without losing its exact checkpoint.
/// 验证单个常驻写入器命令跨越分段边界时不会丢失精确检查点。
#[test]
fn resident_transcript_cursor_publishes_rollover_locator_before_tail_bytes() {
    // root isolates the deterministic rollover transaction fixture.
    // root 隔离确定性滚段事务夹具。
    let root = test_root("resident_transcript_cursor_publishes_rollover_locator_before_tail_bytes");
    // session_id is the exact owner identity required by the resident cursor.
    // session_id 是常驻游标要求的精确所属身份。
    let session_id = Uuid::new_v4();
    // request forces the three-message command to cross from s001 into s002.
    // request 强制三消息命令从 s001 跨入 s002。
    let mut request = test_session_request(SegmentPolicy {
        max_lines: 2,
        max_bytes: 1024 * 1024,
    });
    request.session_id = session_id.to_string();
    // session_dir is the exact deterministic session path used by every assertion.
    // session_dir 是所有断言使用的精确确定性会话路径。
    let session_dir = create_segmented_session_in_location(&root, false, request)
        .expect("create rollover session");
    // cursor and initial_checkpoint represent the single cold-open metadata read.
    // cursor 与 initial_checkpoint 表示唯一一次冷启动元数据读取。
    let (mut cursor, initial_checkpoint) =
        open_segmented_session_transcript_append_cursor(&session_dir, session_id)
            .expect("open rollover cursor");
    // messages form one immutable command whose first row fills s001 and remaining rows occupy s002.
    // messages 构成一个不可变命令：首行填满 s001，其余行进入 s002。
    let messages = vec![
        test_message(
            "m-rollover-1",
            SessionMessageKind::User,
            SessionMessageRecordKind::Transcript,
            "first",
        ),
        test_message(
            "m-rollover-2",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::AssistantOutput,
            "second",
        ),
        test_message(
            "m-rollover-3",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::AssistantOutput,
            "third",
        ),
    ];
    // final_checkpoint is returned only after every row and final metadata are durable.
    // final_checkpoint 仅在每行与最终元数据全部可靠落盘后返回。
    let final_checkpoint = append_segmented_session_message_tail_with_cursor(
        &mut cursor,
        &initial_checkpoint,
        &messages,
    )
    .expect("append cross-segment resident command");
    // metadata proves the published locator and logical transcript frontier agree.
    // metadata 证明已发布定位与逻辑转录前沿一致。
    let metadata = read_session_metadata(&session_dir).expect("read rollover metadata");
    // first_lines and second_lines prove physical ordering without enumerating the directory.
    // first_lines 与 second_lines 在不枚举目录的情况下证明物理顺序。
    let first_lines = read_segment_event_lines(&session_dir.join("s001.jsonl"))
        .expect("read first exact segment");
    let second_lines = read_segment_event_lines(&session_dir.join("s002.jsonl"))
        .expect("read second exact segment");

    assert_eq!(final_checkpoint.message_count, 3);
    assert_eq!(metadata.message_count, 3);
    assert_eq!(metadata.active_segment, "s002.jsonl");
    assert_eq!(metadata.segments.len(), 2);
    assert_eq!(first_lines.len(), 2);
    assert_eq!(second_lines.len(), 2);
    assert_eq!(second_lines[0].seq, 2);
    assert_eq!(second_lines[1].seq, 3);
    assert_eq!(
        metadata.last_message_id,
        Some(stable_segmented_message_id(2, &messages[2]))
    );
}

/// Verifies shutdown retry republishes an already completed row without duplicating its JSONL record.
/// 验证关闭重试会重新发布已完成记录，且不会复制其 JSONL 记录。
#[test]
fn resident_transcript_retry_is_idempotent_after_data_commit() {
    // root isolates the exact resident retry fixture.
    // root 隔离精确的常驻重试夹具。
    let root = test_root("resident_transcript_retry_is_idempotent_after_data_commit");
    // session_id is the exact cursor owner.
    // session_id 是精确的游标所有者。
    let session_id = Uuid::new_v4();
    // request creates one deterministic active transcript segment.
    // request 创建一个确定性的活动转录分段。
    let mut request = test_session_request(SegmentPolicy::default());
    request.session_id = session_id.to_string();
    // session_dir is resolved directly and never discovered by enumeration.
    // session_dir 通过直接解析获得，绝不通过枚举发现。
    let session_dir = create_segmented_session_in_location(&root, false, request)
        .expect("create idempotent retry session");
    // cursor and initial_checkpoint own the original immutable append boundary.
    // cursor 与 initial_checkpoint 持有最初的不可变追加边界。
    let (mut cursor, initial_checkpoint) =
        open_segmented_session_transcript_append_cursor(&session_dir, session_id)
            .expect("open idempotent retry cursor");
    // message is the single stable semantic record required by the writer contract.
    // message 是写入器契约要求的单条稳定语义记录。
    let message = test_message(
        "m-idempotent-retry",
        SessionMessageKind::Assistant,
        SessionMessageRecordKind::AssistantOutput,
        "durable output",
    );
    // committed_checkpoint represents the successful data and metadata publication.
    // committed_checkpoint 表示成功的数据与元数据发布。
    let committed_checkpoint = append_segmented_session_message_tail_with_cursor(
        &mut cursor,
        &initial_checkpoint,
        std::slice::from_ref(&message),
    )
    .expect("persist original row");
    // lines_before captures the exact physical cardinality before retry.
    // lines_before 捕获重试前的精确物理记录数量。
    let lines_before =
        read_segment_event_lines(&session_dir.join("s001.jsonl")).expect("read rows before retry");
    // retried_checkpoint must equal the already committed checkpoint without another append.
    // retried_checkpoint 必须等于已经提交的检查点，且不发生再次追加。
    let retried_checkpoint = retry_segmented_session_message_tail_with_cursor(
        &mut cursor,
        &initial_checkpoint,
        std::slice::from_ref(&message),
    )
    .expect("retry already durable row");
    // lines_after proves the retry changed no physical transcript cardinality.
    // lines_after 证明重试未改变物理转录记录数量。
    let lines_after =
        read_segment_event_lines(&session_dir.join("s001.jsonl")).expect("read rows after retry");

    assert_eq!(retried_checkpoint, committed_checkpoint);
    assert_eq!(lines_after, lines_before);
}

/// Verifies cold writer recovery adopts complete rows and discards only one trailing half-row.
/// 验证冷态写入器恢复会接纳完整记录，并且只丢弃最后一条半行。
#[test]
fn resident_transcript_cursor_recovers_bounded_active_tail() {
    // root isolates the simulated crash-between-data-and-metadata fixture.
    // root 隔离“数据已写入但元数据未发布”的模拟崩溃夹具。
    let root = test_root("resident_transcript_cursor_recovers_bounded_active_tail");
    // session_id is the exact owner required by cursor admission.
    // session_id 是游标准入要求的精确所属身份。
    let session_id = Uuid::new_v4();
    // request creates one empty transcript after the SessionMeta audit row.
    // request 在 SessionMeta 审计记录之后创建空转录。
    let mut request = test_session_request(SegmentPolicy::default());
    request.session_id = session_id.to_string();
    let session_dir = create_segmented_session_in_location(&root, false, request)
        .expect("create recovery session");
    // metadata_before captures the only authorized historical prefix.
    // metadata_before 捕获唯一授权的历史前缀。
    let metadata_before = read_session_metadata(&session_dir).expect("read pre-crash metadata");
    // recovered_message is the complete canonical row physically durable before the crash.
    // recovered_message 是崩溃前已经物理耐久的完整规范记录。
    let recovered_message = test_message(
        "recovered-assistant",
        SessionMessageKind::Assistant,
        SessionMessageRecordKind::AssistantOutput,
        "durable before metadata",
    );
    // recovered_input carries the exact next message and turn coordinates.
    // recovered_input 携带精确的下一消息与轮次坐标。
    let recovered_input = segmented_event_input_from_message(0, &recovered_message, 0)
        .expect("build recoverable event");
    // recovered_line receives the sequence the interrupted writer had reserved.
    // recovered_line 接收被中断写入器已经预留的序号。
    let recovered_line = SegmentedSessionEventLine {
        seq: metadata_before.next_global_seq,
        timestamp: recovered_input.timestamp,
        kind: recovered_input.kind,
        payload: recovered_input.payload,
    };
    // segment_path comes directly from the active metadata row without enumeration.
    // segment_path 直接来自活动元数据记录，不进行枚举。
    let segment_path = session_dir.join(&metadata_before.active_segment);
    // interrupted_file simulates a synced complete row followed by an unsynced JSON half-row.
    // interrupted_file 模拟一个已同步完整记录及其后的未同步 JSON 半行。
    let mut interrupted_file = OpenOptions::new()
        .append(true)
        .open(&segment_path)
        .expect("open interrupted active segment");
    writeln!(
        interrupted_file,
        "{}",
        serde_json::to_string(&recovered_line).expect("encode recoverable row")
    )
    .expect("append recoverable row");
    interrupted_file
        .write_all(b"{\"incomplete\"")
        .expect("append half row");
    interrupted_file.sync_data().expect("sync interrupted tail");
    drop(interrupted_file);

    // checkpoint proves cold cursor recovery advanced only the complete transcript row.
    // checkpoint 证明冷态游标恢复只推进了完整转录记录。
    let (_cursor, checkpoint) =
        open_segmented_session_transcript_append_cursor(&session_dir, session_id)
            .expect("recover bounded active tail");
    // metadata_after must now describe the recovered durable file exactly.
    // metadata_after 现在必须精确描述恢复后的耐久文件。
    let metadata_after = read_session_metadata(&session_dir).expect("read recovered metadata");
    // lines verifies the incomplete suffix was removed and the complete row remains canonical.
    // lines 验证不完整后缀已被移除，完整记录仍保持规范。
    let lines = read_segment_event_lines(&segment_path).expect("read recovered segment");

    assert_eq!(checkpoint.message_count, 1);
    assert_eq!(metadata_after.message_count, 1);
    assert_eq!(
        metadata_after.next_global_seq,
        metadata_before.next_global_seq + 1
    );
    assert_eq!(
        metadata_after
            .segments
            .last()
            .expect("recovered active metadata")
            .byte_len,
        fs::metadata(&segment_path)
            .expect("inspect recovered segment")
            .len()
    );
    assert_eq!(lines.len(), 2);
    let messages = segmented_lines_to_session_messages(&lines).expect("project recovered messages");
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].body, "durable before metadata");
}

/// Verifies explicit maintenance upgrades legacy segment metadata before ordinary read.
/// 验证显式维护会在普通读取前补齐旧版分段元数据。
#[test]
fn explicit_maintenance_upgrades_legacy_segment_locators() {
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

    repair_segmented_session_display_if_needed(&session_dir)
        .expect("explicit maintenance upgrades segment locators");
    read_segmented_session_event_window(&session_dir, None, 16)
        .expect("ordinary read accepts maintained locators");

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

/// Verifies hot append rejects a missing newline until explicit maintenance performs repair.
/// 验证热路径追加会拒绝缺失换行，直至显式维护执行修复。
#[test]
fn append_session_events_reject_missing_jsonl_tail_newline_until_explicit_repair() {
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
    assert!(append_error.contains("explicit maintenance repair is required"));
    repair_segmented_session_display_if_needed(&session_dir)
        .expect("explicit maintenance repairs missing newline");
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

/// Verifies hot append rejects a truncated tail until explicit maintenance performs repair.
/// 验证热路径追加会拒绝截断尾部，直至显式维护执行修复。
#[test]
fn append_session_events_reject_truncated_jsonl_tail_until_explicit_repair() {
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
    assert!(append_error.contains("explicit maintenance repair is required"));
    repair_segmented_session_display_if_needed(&session_dir)
        .expect("explicit maintenance repairs truncated tail");
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

/// Verifies a stable terminal detail lets final persistence append after an earlier tool-card lifecycle update.
/// 验证稳定终态详情可让最终持久化在早期工具卡生命周期更新后继续追加。
#[test]
fn terminal_detail_preserves_append_boundary_after_tool_card_update() {
    let root = test_root("terminal_detail_preserves_append_boundary_after_tool_card_update");
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        test_session_request(SegmentPolicy::default()),
    )
    .expect("create session");
    let user = test_message(
        "m-user-tool-boundary",
        SessionMessageKind::User,
        SessionMessageRecordKind::Transcript,
        "run tool",
    );
    let thinking = test_message(
        "m-thinking-tool-boundary",
        SessionMessageKind::Assistant,
        SessionMessageRecordKind::ThinkingOutput,
        "select tool",
    );
    let mut ready_card = test_message(
        "m-tool-card-boundary",
        SessionMessageKind::Assistant,
        SessionMessageRecordKind::ToolCallOutput,
        "",
    );
    ready_card.metadata = Some(json!({
        "tool_call_id": "call-boundary",
        "tool_display_state": "ready",
    }));
    append_segmented_session_message_update(
        &session_dir,
        &[user.clone(), thinking.clone(), ready_card.clone()],
    )
    .expect("append stable request boundary");

    let mut detail = test_message(
        "m-tool-detail-boundary",
        SessionMessageKind::Assistant,
        SessionMessageRecordKind::ToolDetailOutput,
        "done",
    );
    detail.visible = false;
    detail.metadata = Some(json!({
        "tool_call_id": "call-boundary",
        "display": { "status": "completed" },
    }));
    append_segmented_session_message_update(
        &session_dir,
        &[
            user.clone(),
            thinking.clone(),
            ready_card.clone(),
            detail.clone(),
        ],
    )
    .expect("append immutable terminal detail");

    let mut completed_card = ready_card;
    completed_card.metadata = Some(json!({
        "tool_call_id": "call-boundary",
        "tool_display_state": "running",
        "result": { "status": "completed" },
    }));
    let assistant = test_message(
        "m-assistant-tool-boundary",
        SessionMessageKind::Assistant,
        SessionMessageRecordKind::AssistantOutput,
        "finished",
    );
    let appended = append_segmented_session_message_update(
        &session_dir,
        &[user, thinking, completed_card, detail, assistant],
    )
    .expect("terminal snapshot should append after the immutable detail boundary");

    assert_eq!(appended.len(), 1);
    assert_eq!(appended[0].kind, SegmentedSessionEventKind::AssistantOutput);
    let metadata: SegmentedSessionMetadata =
        read_required_json(&session_dir.join(SEGMENTED_METADATA_FILE)).expect("read metadata");
    assert_eq!(metadata.message_count, 5);
}

/// Verifies an immutable ready card may advance in memory while its terminal detail is appended durably.
/// 验证不可变的就绪工具卡可在内存中推进状态，同时将其终态详情可靠追加到磁盘。
#[test]
fn tool_card_lifecycle_boundary_appends_terminal_detail_without_rewriting_history() {
    let root =
        test_root("tool_card_lifecycle_boundary_appends_terminal_detail_without_rewriting_history");
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        test_session_request(SegmentPolicy::default()),
    )
    .expect("create session");
    // deletedLines models the complete pre-write body that must survive one large JSONL row.
    // deletedLines 模拟必须在一条大型 JSONL 记录中完整保留的写入前正文。
    let deleted_lines = (1..=208)
        .map(|line| json!({ "line": line, "content": format!("old-{line}") }))
        .collect::<Vec<_>>();
    // insertedLines models the complete post-write body paired with the deleted side.
    // insertedLines 模拟与删除侧配对的完整写入后正文。
    let inserted_lines = (1..=140)
        .map(|line| json!({ "line": line, "content": format!("new-{line}") }))
        .collect::<Vec<_>>();
    // insertedContent is the exact request body owned by the persisted invocation card.
    // insertedContent 是由持久化调用卡片持有的精确请求正文。
    let inserted_content = format!(
        "{}\n",
        (1..=140)
            .map(|line| format!("new-{line}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    let mut ready_card = test_message(
        "m-tool-card-lifecycle",
        SessionMessageKind::Assistant,
        SessionMessageRecordKind::ToolCallOutput,
        "",
    );
    ready_card.title.clear();
    ready_card.metadata = Some(json!({
        "preview": null,
        "request_revision": 7,
        "tool_call_id": "call-lifecycle",
        "tool_display_state": "ready",
        "tool_name": "file",
        "arguments": {
            "action": "write",
            "path": "README.md",
            "content": inserted_content
        },
    }));
    append_segmented_session_message_update(&session_dir, std::slice::from_ref(&ready_card))
        .expect("append ready tool card");

    let mut completed_card = ready_card.clone();
    let completed_metadata = completed_card
        .metadata
        .as_mut()
        .and_then(Value::as_object_mut)
        .expect("tool metadata object");
    completed_metadata.insert(
        String::from("tool_display_state"),
        Value::String(String::from("completed")),
    );
    completed_metadata.insert(String::from("result"), json!({ "status": "completed" }));
    let mut detail = test_message(
        "m-tool-detail-lifecycle",
        SessionMessageKind::Assistant,
        SessionMessageRecordKind::ToolDetailOutput,
        "read result",
    );
    detail.visible = false;
    detail.metadata = Some(json!({
        "display": { "status": "completed" },
        "tool_call_id": "call-lifecycle",
        "tool_name": "file",
        "host_result": {
            "kind": "change_set",
            "payload": {
                "mode": "applied",
                "files": [{
                    "resource_id": "resource-1",
                    "change": "modify",
                    "path": "README.md",
                    "hunks": [{
                        "before": "",
                        "delete": deleted_lines,
                        "insert": inserted_lines,
                        "after": ""
                    }]
                }]
            }
        }
    }));

    let appended =
        append_segmented_session_message_update(&session_dir, &[completed_card, detail.clone()])
            .expect("append detail across a lifecycle-only boundary change");

    assert_eq!(appended.len(), 1);
    assert_eq!(appended[0].kind, SegmentedSessionEventKind::DisplayEvent);
    let lines =
        read_segment_event_lines(&session_dir.join("s001.jsonl")).expect("read physical segment");
    let persisted_card_line = lines
        .iter()
        .find(|line| line.payload.get("message_index").and_then(Value::as_u64) == Some(0))
        .expect("physical ready card line");
    let persisted_card = segmented_line_to_session_message(persisted_card_line)
        .expect("decode physical ready card")
        .expect("physical ready card");
    assert_eq!(
        persisted_card
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("tool_display_state"))
            .and_then(Value::as_str),
        Some("ready")
    );
    // persistedDetail is decoded from the physical JSONL row, not from the in-memory fixture.
    // persistedDetail 从物理 JSONL 记录解码，而不是来自内存夹具。
    let persisted_detail = lines
        .iter()
        .filter_map(|line| {
            segmented_line_to_session_message(line).expect("decode persisted session row")
        })
        .find(|message| message.id == detail.id)
        .expect("physical terminal detail row");
    assert_eq!(
        persisted_detail
            .metadata
            .as_ref()
            .and_then(|metadata| {
                metadata.pointer("/host_result/payload/files/0/hunks/0/delete/207/content")
            })
            .and_then(Value::as_str),
        Some("old-208")
    );
    assert_eq!(
        persisted_detail
            .metadata
            .as_ref()
            .and_then(|metadata| {
                metadata.pointer("/host_result/payload/files/0/hunks/0/insert/139/content")
            })
            .and_then(Value::as_str),
        Some("new-140")
    );
    assert_eq!(
        persisted_card
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.pointer("/arguments/content"))
            .and_then(Value::as_str),
        Some(inserted_content.as_str())
    );
    assert!(
        persisted_detail
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("arguments"))
            .is_none()
    );
    // replayedMessages proves restart projection restores the exact persisted change set onto the visible card.
    // replayedMessages 证明重启投影会把精确持久化变更集恢复到可见卡片上。
    let replayed_messages =
        segmented_lines_to_session_messages(&lines).expect("replay persisted lifecycle rows");
    let replayed_card = replayed_messages
        .iter()
        .find(|message| message.id == "m-tool-card-lifecycle")
        .expect("replayed visible tool card");
    assert_eq!(
        replayed_card
            .metadata
            .as_ref()
            .and_then(|metadata| {
                metadata.pointer("/host_result/payload/files/0/hunks/0/delete/0/content")
            })
            .and_then(Value::as_str),
        Some("old-1")
    );
    assert_eq!(
        replayed_card
            .metadata
            .as_ref()
            .and_then(|metadata| {
                metadata.pointer("/host_result/payload/files/0/hunks/0/insert/0/content")
            })
            .and_then(Value::as_str),
        Some("new-1")
    );
    let metadata: SegmentedSessionMetadata =
        read_required_json(&session_dir.join(SEGMENTED_METADATA_FILE)).expect("read metadata");
    assert_eq!(metadata.message_count, 2);
    assert_eq!(
        metadata.last_message_id.as_deref(),
        Some(detail.id.as_str())
    );
}

/// Verifies lifecycle boundary tolerance never accepts an immutable tool identity rewrite.
/// 验证生命周期边界容差绝不会接受不可变工具身份改写。
#[test]
fn tool_card_lifecycle_boundary_rejects_tool_name_rewrite() {
    let root = test_root("tool_card_lifecycle_boundary_rejects_tool_name_rewrite");
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        test_session_request(SegmentPolicy::default()),
    )
    .expect("create session");
    let mut ready_card = test_message(
        "m-tool-card-immutable",
        SessionMessageKind::Assistant,
        SessionMessageRecordKind::ToolCallOutput,
        "",
    );
    ready_card.title.clear();
    ready_card.metadata = Some(json!({
        "tool_call_id": "call-immutable",
        "tool_display_state": "ready",
        "tool_name": "file",
    }));
    append_segmented_session_message_update(&session_dir, std::slice::from_ref(&ready_card))
        .expect("append ready tool card");

    let mut rewritten_card = ready_card;
    let rewritten_metadata = rewritten_card
        .metadata
        .as_mut()
        .and_then(Value::as_object_mut)
        .expect("tool metadata object");
    rewritten_metadata.insert(
        String::from("tool_display_state"),
        Value::String(String::from("completed")),
    );
    rewritten_metadata.insert(
        String::from("tool_name"),
        Value::String(String::from("shell_command")),
    );
    let detail = test_message(
        "m-tool-detail-immutable",
        SessionMessageKind::Assistant,
        SessionMessageRecordKind::ToolDetailOutput,
        "result",
    );

    let error = append_segmented_session_message_update(&session_dir, &[rewritten_card, detail])
        .expect_err("immutable tool identity rewrite must be rejected");

    assert!(error.contains("non-append display mutation"));
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

/// Verifies a captured metadata token validates the durable boundary independently of UI ordering.
/// 验证捕获的元数据令牌可独立于界面顺序校验可靠边界。
#[test]
fn append_exact_message_tail_accepts_captured_metadata_checkpoint() {
    let root = test_root("append_exact_message_tail_accepts_captured_metadata_checkpoint");
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
    let checkpoint: SegmentedSessionMetadata =
        read_required_json(&session_dir.join(SEGMENTED_METADATA_FILE)).expect("read checkpoint");
    let assistant = test_message(
        "m-assistant-1",
        SessionMessageKind::Assistant,
        SessionMessageRecordKind::AssistantOutput,
        "answer",
    );

    let appended = append_segmented_session_message_tail_from_checkpoint(
        &session_dir,
        1,
        checkpoint.last_message_id.as_deref(),
        checkpoint.last_message_fingerprint.as_deref(),
        std::slice::from_ref(&assistant),
    )
    .expect("append from exact metadata checkpoint");

    assert_eq!(appended.len(), 1);
    let metadata: SegmentedSessionMetadata =
        read_required_json(&session_dir.join(SEGMENTED_METADATA_FILE)).expect("read metadata");
    assert_eq!(metadata.message_count, 2);
}

/// Verifies a metadata token rejects a same-length prefix whose final content changed concurrently.
/// 验证元数据令牌会拒绝末消息内容并发变化但长度相同的前缀。
#[test]
fn append_exact_message_tail_rejects_changed_metadata_checkpoint() {
    let root = test_root("append_exact_message_tail_rejects_changed_metadata_checkpoint");
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        test_session_request(SegmentPolicy::default()),
    )
    .expect("create session");
    let original = test_message(
        "m-user-1",
        SessionMessageKind::User,
        SessionMessageRecordKind::Transcript,
        "original prompt",
    );
    append_segmented_session_message_update(&session_dir, std::slice::from_ref(&original))
        .expect("append original message");
    let checkpoint: SegmentedSessionMetadata =
        read_required_json(&session_dir.join(SEGMENTED_METADATA_FILE)).expect("read checkpoint");
    let changed = test_message(
        "m-user-1",
        SessionMessageKind::User,
        SessionMessageRecordKind::Transcript,
        "changed prompt",
    );
    let ordinary_rewrite_error =
        append_segmented_session_message_update(&session_dir, std::slice::from_ref(&changed))
            .expect_err("ordinary persistence must reject same-length display rewrites");
    assert!(ordinary_rewrite_error.contains("non-append display mutation"));
    replace_segmented_session_message_records_for_repair(
        &session_dir,
        std::slice::from_ref(&changed),
    )
    .expect("explicitly repair same-length message prefix");
    let assistant = test_message(
        "m-assistant-1",
        SessionMessageKind::Assistant,
        SessionMessageRecordKind::AssistantOutput,
        "answer",
    );

    let error = append_segmented_session_message_tail_from_checkpoint(
        &session_dir,
        1,
        checkpoint.last_message_id.as_deref(),
        checkpoint.last_message_fingerprint.as_deref(),
        std::slice::from_ref(&assistant),
    )
    .expect_err("reject changed metadata checkpoint");

    assert!(error.contains("no longer matches the exact metadata boundary"));
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

    assert!(error.contains("does not match the retained immutable command"));
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
    assert!(append_error.contains("explicit maintenance repair is required"));
    repair_segmented_session_display_if_needed(&session_dir)
        .expect("explicit maintenance repairs duplicate message index");
    replace_segmented_session_message_records_for_repair(
        &session_dir,
        &[replacement_user.clone(), assistant.clone()],
    )
    .expect("replace messages after explicit load repair");

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
