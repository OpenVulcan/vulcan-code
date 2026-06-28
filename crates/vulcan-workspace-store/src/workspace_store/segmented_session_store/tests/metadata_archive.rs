use super::*;

/// Verifies transcript mapping keeps thinking, tool, and hidden response records out of visible user/assistant kinds.
/// 验证转录映射会将思考、工具与隐藏响应记录排除在可见用户/助手类型之外。
#[test]
fn transcript_mapping_marks_only_user_prompts_and_assistant_outputs_as_visible_turns() {
    let user = segmented_event_input_from_message(
        0,
        &test_message(
            "m-user",
            SessionMessageKind::User,
            SessionMessageRecordKind::Transcript,
            "prompt",
        ),
        0,
    )
    .expect("map user prompt");
    let assistant = segmented_event_input_from_message(
        1,
        &test_message(
            "m-assistant",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::AssistantOutput,
            "answer",
        ),
        0,
    )
    .expect("map assistant output");
    let thinking = segmented_event_input_from_message(
        2,
        &test_message(
            "m-thinking",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::ThinkingOutput,
            "reasoning",
        ),
        0,
    )
    .expect("map thinking output");
    let tool = segmented_event_input_from_message(
        3,
        &test_message(
            "m-tool",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::ToolResultOutput,
            "tool result",
        ),
        0,
    )
    .expect("map tool output");
    let mut hidden = test_message(
        "m-hidden",
        SessionMessageKind::Assistant,
        SessionMessageRecordKind::AssistantOutput,
        "hidden answer",
    );
    hidden.visible = false;
    let hidden_response =
        segmented_event_input_from_message(4, &hidden, 0).expect("map hidden response");

    assert_eq!(user.kind, SegmentedSessionEventKind::UserPrompt);
    assert_eq!(assistant.kind, SegmentedSessionEventKind::AssistantOutput);
    assert_eq!(thinking.kind, SegmentedSessionEventKind::DisplayEvent);
    assert_eq!(tool.kind, SegmentedSessionEventKind::DisplayEvent);
    assert_eq!(
        hidden_response.kind,
        SegmentedSessionEventKind::ResponseItem
    );
}

/// Verifies segment turn-range pruning skips impossible segments and keeps unknown ranges safe.
/// 验证分段轮次范围裁剪会跳过不可能命中的分段，并对未知范围保持保守。
#[test]
fn segment_may_contain_turn_uses_known_bounds_conservatively() {
    let mut segment = SegmentMetadata::empty("s001.jsonl".to_string(), 0);
    assert!(segment_may_contain_turn(&segment, 3));

    segment.first_turn_index = Some(2);
    segment.last_turn_index = Some(4);
    assert!(segment_may_contain_turn(&segment, 3));
    assert!(!segment_may_contain_turn(&segment, 1));
    assert!(!segment_may_contain_turn(&segment, 5));

    segment.first_turn_index = Some(6);
    segment.last_turn_index = None;
    assert!(!segment_may_contain_turn(&segment, 5));
    assert!(segment_may_contain_turn(&segment, 6));
}

/// Verifies metadata patching updates metadata without modifying segment files.
/// 验证元数据补丁会更新元数据且不会修改分段文件。
#[test]
fn metadata_patch_does_not_touch_segment_lines() {
    let root = test_root("metadata_patch_does_not_touch_segment_lines");
    let session_dir =
        create_segmented_session(&root, test_session_request(SegmentPolicy::default()))
            .expect("create session");
    let before_lines = read_segment_event_lines(&session_dir.join("s001.jsonl"))
        .expect("read segment before patch");

    let patched = patch_segmented_session_metadata(
        &session_dir,
        SegmentedSessionMetadataPatch {
            updated_at: Some("2026-06-06T12:00:00Z".to_string()),
            title: Some(Some("Renamed".to_string())),
            title_source: Some(Some("manual".to_string())),
            archived_at: None,
        },
    )
    .expect("patch metadata");

    let after_lines = read_segment_event_lines(&session_dir.join("s001.jsonl"))
        .expect("read segment after patch");
    assert_eq!(before_lines, after_lines);
    assert_eq!(patched.title.as_deref(), Some("Renamed"));
    assert_eq!(patched.title_source.as_deref(), Some("manual"));
    assert_eq!(patched.updated_at, "2026-06-06T12:00:00Z");
}

/// Verifies archive and restore move the session directory without rewriting segments.
/// 验证归档与恢复只移动会话目录且不会重写分段。
#[test]
fn archive_and_restore_preserve_segment_lines() {
    let root = test_root("archive_and_restore_preserve_segment_lines");
    let request = test_session_request(SegmentPolicy::default());
    let session_dir = create_segmented_session(&root, request.clone()).expect("create session");
    append_segmented_session_events(
        &session_dir,
        &[test_event(
            "2026-06-06T10:01:00Z",
            SegmentedSessionEventKind::UserPrompt,
        )],
    )
    .expect("append event");
    let before_lines =
        read_segment_event_lines(&session_dir.join("s001.jsonl")).expect("read active lines");

    let archived_dir = archive_segmented_session(
        &root,
        &request.project_tag,
        &request.created_date,
        &request.session_id,
        "2026-06-06T12:00:00Z",
    )
    .expect("archive session");
    assert!(!session_dir.exists());
    assert!(archived_dir.exists());
    let archived_lines =
        read_segment_event_lines(&archived_dir.join("s001.jsonl")).expect("read archived lines");
    assert_eq!(before_lines, archived_lines);
    let archived_metadata: SegmentedSessionMetadata =
        read_required_json(&archived_dir.join(SEGMENTED_METADATA_FILE))
            .expect("read archived metadata");
    assert_eq!(
        archived_metadata.archived_at.as_deref(),
        Some("2026-06-06T12:00:00Z")
    );

    let restored_dir = restore_segmented_session(
        &root,
        &request.project_tag,
        &request.created_date,
        &request.session_id,
        "2026-06-06T12:30:00Z",
    )
    .expect("restore session");
    assert!(!archived_dir.exists());
    assert!(restored_dir.exists());
    let restored_lines =
        read_segment_event_lines(&restored_dir.join("s001.jsonl")).expect("read restored lines");
    assert_eq!(before_lines, restored_lines);
    let restored_metadata: SegmentedSessionMetadata =
        read_required_json(&restored_dir.join(SEGMENTED_METADATA_FILE))
            .expect("read restored metadata");
    assert_eq!(restored_metadata.archived_at, None);
}

/// Verifies archive, restore, and delete can locate segmented directories by session id alone.
/// 验证归档、恢复与删除可仅按会话标识定位分段目录。
#[test]
fn archive_restore_and_delete_by_id_locate_session_directory() {
    let root = test_root("archive_restore_and_delete_by_id_locate_session_directory");
    let request = test_session_request(SegmentPolicy::default());
    let session_dir = create_segmented_session(&root, request.clone()).expect("create session");

    let archived_dir = archive_segmented_session_by_id(
        &root,
        &request.project_tag,
        &request.session_id,
        "2026-06-06T12:00:00Z",
    )
    .expect("archive by id")
    .expect("archived path");
    assert!(!session_dir.exists());
    assert!(archived_dir.exists());

    let restored_dir = restore_segmented_session_by_id(
        &root,
        &request.project_tag,
        &request.session_id,
        "2026-06-06T12:30:00Z",
    )
    .expect("restore by id")
    .expect("restored path");
    assert!(!archived_dir.exists());
    assert!(restored_dir.exists());

    let archived_dir = archive_segmented_session_by_id(
        &root,
        &request.project_tag,
        &request.session_id,
        "2026-06-06T13:00:00Z",
    )
    .expect("archive again")
    .expect("archived path");
    assert!(archived_dir.exists());
    assert!(
        delete_segmented_session_by_id(&root, true, &request.project_tag, &request.session_id)
            .expect("delete archived by id")
    );
    assert!(!archived_dir.exists());
    assert!(
        !delete_segmented_session_by_id(&root, true, &request.project_tag, &request.session_id)
            .expect("missing delete is no-op")
    );
}
