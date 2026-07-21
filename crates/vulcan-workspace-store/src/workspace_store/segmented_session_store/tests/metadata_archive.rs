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

/// Verifies metadata patching updates metadata without modifying segment files.
/// 验证元数据补丁会更新元数据且不会修改分段文件。
#[test]
fn metadata_patch_does_not_touch_segment_lines() {
    let root = test_root("metadata_patch_does_not_touch_segment_lines");
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        test_session_request(SegmentPolicy::default()),
    )
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

/// Verifies exact-locator rename updates metadata without rewriting append-only segment rows.
/// 验证精确定位重命名会更新元数据且不会重写追加式分段行。
#[test]
fn rename_by_created_date_updates_metadata_without_touching_segments() {
    // root isolates one exact active locator and its append-only content.
    // root 隔离一个精确活动定位身份及其追加式内容。
    let root = test_root("rename_by_created_date_updates_metadata_without_touching_segments");
    // request provides the exact project tag, date, and session identity used by rename.
    // request 提供重命名使用的精确项目标签、日期与会话身份。
    let request = test_session_request(SegmentPolicy::default());
    // session_dir is the active directory whose metadata may change while segments stay immutable.
    // session_dir 是元数据允许变更而分段必须保持不变的活动目录。
    let session_dir = create_segmented_session_in_location(&root, false, request.clone())
        .expect("create active session");
    append_segmented_session_events(
        &session_dir,
        &[test_event(
            "2026-06-06T10:01:00Z",
            SegmentedSessionEventKind::UserPrompt,
        )],
    )
    .expect("append event before rename");
    // before_lines capture the complete append-only segment before metadata mutation.
    // before_lines 捕获元数据变更前的完整追加式分段。
    let before_lines = read_segment_event_lines(&session_dir.join("s001.jsonl"))
        .expect("read lines before rename");

    // renamed_dir proves the exact locator resolved back to the expected active identity.
    // renamed_dir 证明精确定位信息解析回预期活动身份。
    let renamed_dir = rename_segmented_session_by_created_date(
        &root,
        &request.project_tag,
        &request.created_date,
        &request.session_id,
        "新标题",
        "2026-06-06T12:00:00Z",
    )
    .expect("rename by exact created date");
    // metadata is the authoritative display document after the rename commit.
    // metadata 是重命名提交后的权威展示文档。
    let metadata: SegmentedSessionMetadata =
        read_required_json(&renamed_dir.join(SEGMENTED_METADATA_FILE))
            .expect("read renamed metadata");
    // after_lines capture the same append-only segment after metadata mutation.
    // after_lines 捕获元数据变更后的同一追加式分段。
    let after_lines =
        read_segment_event_lines(&renamed_dir.join("s001.jsonl")).expect("read lines after rename");
    let _ = fs::remove_dir_all(&root);

    assert_eq!(renamed_dir, session_dir);
    assert_eq!(metadata.title.as_deref(), Some("新标题"));
    assert_eq!(metadata.title_source.as_deref(), Some("manual"));
    assert_eq!(metadata.updated_at, "2026-06-06T12:00:00Z");
    assert_eq!(after_lines, before_lines);
}

/// Verifies exact-locator rename rejects missing and non-directory active identities.
/// 验证精确定位重命名会拒绝缺失及非目录活动身份。
#[test]
fn rename_by_created_date_rejects_invalid_active_directory_state() {
    // root initially contains no active directory for the requested locator.
    // root 初始不包含请求定位身份对应的活动目录。
    let root = test_root("rename_by_created_date_rejects_invalid_active_directory_state");
    // request provides deterministic path components for both invalid states.
    // request 为两种无效状态提供确定性路径组成部分。
    let request = test_session_request(SegmentPolicy::default());
    // missing_error proves absence cannot be treated as a metadata-only success.
    // missing_error 证明目录缺失不能被视为仅更新元数据的成功状态。
    let missing_error = rename_segmented_session_by_created_date(
        &root,
        &request.project_tag,
        &request.created_date,
        &request.session_id,
        "新标题",
        "2026-06-06T12:00:00Z",
    )
    .expect_err("missing active directory should fail rename");
    // session_path is the exact locator path deliberately occupied by a regular file.
    // session_path 是被普通文件刻意占用的精确定位路径。
    let session_path = segmented_session_directory(
        &root,
        false,
        &request.project_tag,
        &request.created_date,
        &request.session_id,
    );
    // session_parent is the real parent required before creating the conflicting file.
    // session_parent 是创建冲突文件前必须存在的真实父目录。
    let session_parent = session_path
        .parent()
        .expect("session path should have parent");
    fs::create_dir_all(session_parent).expect("create active session parent");
    fs::write(&session_path, b"not-a-directory").expect("create conflicting session file");
    // non_directory_error proves lifecycle paths reject regular files before metadata IO.
    // non_directory_error 证明生命周期路径会在元数据 IO 前拒绝普通文件。
    let non_directory_error = rename_segmented_session_by_created_date(
        &root,
        &request.project_tag,
        &request.created_date,
        &request.session_id,
        "新标题",
        "2026-06-06T12:00:00Z",
    )
    .expect_err("non-directory active identity should fail rename");
    let _ = fs::remove_dir_all(&root);

    assert!(missing_error.contains("does not exist"));
    assert!(non_directory_error.contains("not a real directory"));
}

/// Verifies archive and restore move the session directory without rewriting segments.
/// 验证归档与恢复只移动会话目录且不会重写分段。
#[test]
fn archive_and_restore_preserve_segment_lines() {
    let root = test_root("archive_and_restore_preserve_segment_lines");
    let request = test_session_request(SegmentPolicy::default());
    let session_dir = create_segmented_session_in_location(&root, false, request.clone())
        .expect("create session");
    // vmm_state establishes one durable lease whose exact bytes and owner must survive both directory moves.
    // vmm_state 建立一份持久租约，其精确字节与所有者必须经过两次目录移动后保持不变。
    let mut vmm_state = vulcan_vmm::VmmSessionMemoryState::default();
    vmm_state
        .apply_recall_batch(
            &request.session_id,
            "archive-user-anchor",
            &vulcan_vmm::VmmRecallBatch {
                ttl_turns: 3,
                injection_tag: String::from("VMM_MEMORY"),
                injection_description: String::from("Relevant memories."),
                items: vec![vulcan_vmm::ContextItem {
                    text: String::from("archive-memory-body"),
                    memory_id: 71,
                    ..vulcan_vmm::ContextItem::default()
                }],
            },
            "2026-06-06T10:00:30Z",
        )
        .expect("archive VMM fixture should be built");
    write_vmm_state(&session_dir, &request.session_id, &vmm_state)
        .expect("archive VMM fixture should be written");
    // before_vmm_bytes preserve the exact independent document for byte-stability assertions.
    // before_vmm_bytes 保留独立文档的精确字节，用于断言字节稳定性。
    let before_vmm_bytes = fs::read(vmm_state_path(&session_dir)).expect("read active VMM state");
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
    // archived_vmm_bytes prove archival moved rather than regenerated the independent VMM document.
    // archived_vmm_bytes 证明归档移动而非重新生成了独立 VMM 文档。
    let archived_vmm_bytes =
        fs::read(vmm_state_path(&archived_dir)).expect("read archived VMM state");
    assert_eq!(before_vmm_bytes, archived_vmm_bytes);
    // archived_vmm_state verifies the moved document still validates against the same session identity.
    // archived_vmm_state 验证移动后的文档仍能通过同一会话身份校验。
    let archived_vmm_state: vulcan_vmm::VmmSessionMemoryState =
        read_required_json(&vmm_state_path(&archived_dir)).expect("load archived VMM state");
    archived_vmm_state
        .validate(&request.session_id)
        .expect("archived VMM state should retain its owner identity");
    assert_eq!(archived_vmm_state, vmm_state);
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
    // restored_vmm_bytes prove restoration preserves the exact archived VMM document.
    // restored_vmm_bytes 证明恢复保留了归档时的精确 VMM 文档。
    let restored_vmm_bytes =
        fs::read(vmm_state_path(&restored_dir)).expect("read restored VMM state");
    assert_eq!(before_vmm_bytes, restored_vmm_bytes);
    // restored_vmm_state verifies identity and lease state remain valid after the round trip.
    // restored_vmm_state 验证完整归档往返后身份与租约状态仍然有效。
    let restored_vmm_state: vulcan_vmm::VmmSessionMemoryState =
        read_required_json(&vmm_state_path(&restored_dir)).expect("load restored VMM state");
    restored_vmm_state
        .validate(&request.session_id)
        .expect("restored VMM state should retain its owner identity");
    assert_eq!(restored_vmm_state, vmm_state);
    let restored_metadata: SegmentedSessionMetadata =
        read_required_json(&restored_dir.join(SEGMENTED_METADATA_FILE))
            .expect("read restored metadata");
    assert_eq!(restored_metadata.archived_at, None);
}

/// Verifies invalid active metadata aborts archive before the directory leaves the active tree.
/// 验证无效活动元数据会在目录离开活动树前中止归档。
#[test]
fn archive_with_invalid_metadata_keeps_active_directory() {
    // root isolates the active and archived lifecycle trees.
    // root 隔离活动与归档生命周期树。
    let root = test_root("archive_with_invalid_metadata_keeps_active_directory");
    // request provides the deterministic source and destination layout identity.
    // request 提供确定性的源与目标布局身份。
    let request = test_session_request(SegmentPolicy::default());
    // active_dir is the authoritative source that must remain after failure.
    // active_dir 是失败后必须保留的权威源目录。
    let active_dir = create_segmented_session_in_location(&root, false, request.clone())
        .expect("create active session");
    // archived_dir is the destination that must never become visible.
    // archived_dir 是绝不能变为可见的目标目录。
    let archived_dir = segmented_session_directory(
        &root,
        true,
        &request.project_tag,
        &request.created_date,
        &request.session_id,
    );
    fs::write(
        active_dir.join(SEGMENTED_METADATA_FILE),
        b"{invalid metadata",
    )
    .expect("corrupt active metadata");

    // result must fail during authoritative metadata preflight before rename.
    // result 必须在重命名前的权威元数据预检阶段失败。
    let result = archive_segmented_session(
        &root,
        &request.project_tag,
        &request.created_date,
        &request.session_id,
        "2026-06-06T12:00:00Z",
    );
    // active_exists and archived_exists capture the lifecycle state before cleanup.
    // active_exists 与 archived_exists 在清理前捕获生命周期状态。
    let active_exists = active_dir.is_dir();
    let archived_exists = archived_dir.exists();
    let _ = fs::remove_dir_all(&root);

    assert!(result.is_err());
    assert!(active_exists);
    assert!(!archived_exists);
}

/// Verifies archive never replaces an existing destination directory, even when it is empty.
/// 验证归档绝不会替换已有目标目录，即使该目录为空。
#[test]
fn archive_rejects_existing_empty_destination_without_moving_source() {
    // root isolates the conflicting active and archived lifecycle entries.
    // root 隔离发生冲突的活动与归档生命周期目录项。
    let root = test_root("archive_rejects_existing_empty_destination_without_moving_source");
    // request provides the deterministic source and destination layout identity.
    // request 提供确定性的源与目标布局身份。
    let request = test_session_request(SegmentPolicy::default());
    // active_dir is the authoritative source that must not move on conflict.
    // active_dir 是冲突时不得移动的权威源目录。
    let active_dir = create_segmented_session_in_location(&root, false, request.clone())
        .expect("create active session");
    // archived_dir is deliberately pre-created as an empty conflicting destination.
    // archived_dir 被刻意预创建为空的冲突目标目录。
    let archived_dir = segmented_session_directory(
        &root,
        true,
        &request.project_tag,
        &request.created_date,
        &request.session_id,
    );
    fs::create_dir_all(&archived_dir).expect("create conflicting archived destination");

    // result must reject the occupied destination before rename.
    // result 必须在重命名前拒绝已占用目标。
    let result = archive_segmented_session(
        &root,
        &request.project_tag,
        &request.created_date,
        &request.session_id,
        "2026-06-06T12:00:00Z",
    );
    // archived_entries prove the destination was not populated from the source.
    // archived_entries 证明目标没有被源目录内容填充。
    let archived_entries = fs::read_dir(&archived_dir)
        .expect("read conflicting destination")
        .collect::<Result<Vec<_>, _>>()
        .expect("collect conflicting destination entries");
    // active_exists captures the authoritative source state before cleanup.
    // active_exists 在清理前捕获权威源状态。
    let active_exists = active_dir.is_dir();
    let _ = fs::remove_dir_all(&root);

    let error = result.expect_err("existing destination should reject archive");
    assert!(error.contains("destination already exists"));
    assert!(active_exists);
    assert!(archived_entries.is_empty());
}

/// Verifies invalid archived metadata aborts restore before the directory leaves the archived tree.
/// 验证无效归档元数据会在目录离开归档树前中止恢复。
#[test]
fn restore_with_invalid_metadata_keeps_archived_directory() {
    // root isolates the active and archived lifecycle trees.
    // root 隔离活动与归档生命周期树。
    let root = test_root("restore_with_invalid_metadata_keeps_archived_directory");
    // request provides the deterministic archive and restore layout identity.
    // request 提供确定性的归档与恢复布局身份。
    let request = test_session_request(SegmentPolicy::default());
    // active_dir is retained only as the expected absent restore destination.
    // active_dir 仅作为预期不存在的恢复目标保留。
    let active_dir = create_segmented_session_in_location(&root, false, request.clone())
        .expect("create active session");
    // archived_dir is the authoritative source that must remain after restore failure.
    // archived_dir 是恢复失败后必须保留的权威源目录。
    let archived_dir = archive_segmented_session(
        &root,
        &request.project_tag,
        &request.created_date,
        &request.session_id,
        "2026-06-06T12:00:00Z",
    )
    .expect("archive session");
    fs::write(
        archived_dir.join(SEGMENTED_METADATA_FILE),
        b"{invalid metadata",
    )
    .expect("corrupt archived metadata");

    // result must fail during authoritative metadata preflight before rename.
    // result 必须在重命名前的权威元数据预检阶段失败。
    let result = restore_segmented_session(
        &root,
        &request.project_tag,
        &request.created_date,
        &request.session_id,
        "2026-06-06T12:30:00Z",
    );
    // active_exists and archived_exists capture the lifecycle state before cleanup.
    // active_exists 与 archived_exists 在清理前捕获生命周期状态。
    let active_exists = active_dir.exists();
    let archived_exists = archived_dir.is_dir();
    let _ = fs::remove_dir_all(&root);

    assert!(result.is_err());
    assert!(!active_exists);
    assert!(archived_exists);
}

/// Verifies deterministic date-based archive and restore compose correctly.
/// 验证基于确定日期的归档与恢复能够正确组合。
#[test]
fn archive_and_restore_by_created_date() {
    let root = test_root("archive_and_restore_by_created_date");
    let request = test_session_request(SegmentPolicy::default());
    let session_dir = create_segmented_session_in_location(&root, false, request.clone())
        .expect("create session");

    let archived_dir = archive_segmented_session_by_created_date(
        &root,
        &request.project_tag,
        &request.created_date,
        &request.session_id,
        "2026-06-06T12:00:00Z",
    )
    .expect("archive by created date");
    assert!(!session_dir.exists());
    assert!(archived_dir.exists());

    let restored_dir = restore_segmented_session_by_created_date(
        &root,
        &request.project_tag,
        &request.created_date,
        &request.session_id,
        "2026-06-06T12:30:00Z",
    )
    .expect("restore by created date");
    assert!(!archived_dir.exists());
    assert!(restored_dir.exists());
    // Remove the restored fixture because this test no longer exercises permanent deletion.
    // 删除已恢复测试夹具，因为本测试不再执行永久删除。
    let _ = fs::remove_dir_all(&root);
}

/// Verifies archive and restore resume directory moves that completed before metadata publication.
/// 验证归档与恢复会续作目录已移动但元数据尚未发布的状态。
#[test]
fn archive_and_restore_resume_interrupted_directory_moves() {
    // root isolates the simulated interrupted lifecycle transitions.
    // root 隔离模拟的生命周期中断转换。
    let root = test_root("archive_and_restore_resume_interrupted_directory_moves");
    // request provides the deterministic active and archived identities.
    // request 提供确定性的活动与归档身份。
    let request = test_session_request(SegmentPolicy::default());
    // active_dir begins as the canonical active session source.
    // active_dir 初始为规范活动会话源。
    let active_dir = create_segmented_session_in_location(&root, false, request.clone())
        .expect("create active session");
    // archived_dir is the destination published before the simulated archive interruption.
    // archived_dir 是模拟归档中断前已发布的目标。
    let archived_dir = segmented_session_directory(
        &root,
        true,
        &request.project_tag,
        &request.created_date,
        &request.session_id,
    );
    move_segmented_session_directory(&active_dir, &archived_dir)
        .expect("simulate interrupted archive move");
    synchronize_segmented_session_directory_move(&active_dir, &archived_dir)
        .expect("synchronize simulated archive move");

    // resumed_archive must repair metadata without requiring the vanished active source.
    // resumed_archive 必须在活动源已消失时修复元数据。
    let resumed_archive = archive_segmented_session_by_created_date(
        &root,
        &request.project_tag,
        &request.created_date,
        &request.session_id,
        "2026-06-06T12:00:00Z",
    )
    .expect("resume interrupted archive");
    // archived_metadata proves recovery committed the intended lifecycle marker.
    // archived_metadata 证明恢复过程提交了预期生命周期标记。
    let archived_metadata: SegmentedSessionMetadata =
        read_required_json(&resumed_archive.join(SEGMENTED_METADATA_FILE))
            .expect("read resumed archive metadata");
    assert_eq!(resumed_archive, archived_dir);
    assert_eq!(
        archived_metadata.archived_at.as_deref(),
        Some("2026-06-06T12:00:00Z")
    );

    move_segmented_session_directory(&archived_dir, &active_dir)
        .expect("simulate interrupted restore move");
    synchronize_segmented_session_directory_move(&archived_dir, &active_dir)
        .expect("synchronize simulated restore move");
    // resumed_restore must clear the stale archived marker at the already-published active path.
    // resumed_restore 必须在已发布的活动路径清除陈旧归档标记。
    let resumed_restore = restore_segmented_session_by_created_date(
        &root,
        &request.project_tag,
        &request.created_date,
        &request.session_id,
        "2026-06-06T12:30:00Z",
    )
    .expect("resume interrupted restore");
    // restored_metadata proves recovery completed the active lifecycle commit.
    // restored_metadata 证明恢复过程完成了活动生命周期提交。
    let restored_metadata: SegmentedSessionMetadata =
        read_required_json(&resumed_restore.join(SEGMENTED_METADATA_FILE))
            .expect("read resumed restore metadata");
    let _ = fs::remove_dir_all(&root);

    assert_eq!(resumed_restore, active_dir);
    assert_eq!(restored_metadata.archived_at, None);
}

/// Verifies lifecycle operations reject a locator whose active and archived directories are both absent.
/// 验证生命周期操作会拒绝活动与归档目录均缺失的定位信息。
#[test]
fn archive_and_restore_reject_missing_lifecycle_directories() {
    // root intentionally contains neither lifecycle directory.
    // root 刻意不包含任何生命周期目录。
    let root = test_root("archive_and_restore_reject_missing_lifecycle_directories");
    // request provides the exact missing locator components.
    // request 提供精确的缺失定位组成部分。
    let request = test_session_request(SegmentPolicy::default());
    // archive_error captures the active-to-archived missing-state rejection.
    // archive_error 捕获活动到归档的缺失状态拒绝。
    let archive_error = archive_segmented_session_by_created_date(
        &root,
        &request.project_tag,
        &request.created_date,
        &request.session_id,
        "2026-06-06T12:00:00Z",
    )
    .expect_err("archive with both directories missing should fail");
    // restore_error captures the archived-to-active missing-state rejection.
    // restore_error 捕获归档到活动的缺失状态拒绝。
    let restore_error = restore_segmented_session_by_created_date(
        &root,
        &request.project_tag,
        &request.created_date,
        &request.session_id,
        "2026-06-06T12:30:00Z",
    )
    .expect_err("restore with both directories missing should fail");

    assert!(archive_error.contains("neither active nor archived"));
    assert!(restore_error.contains("neither archived nor active"));
}

/// Verifies lifecycle operations reject simultaneous active and archived directories without choosing a winner.
/// 验证生命周期操作会拒绝活动与归档目录同时存在，且不会擅自选择权威目录。
#[test]
fn archive_and_restore_reject_ambiguous_lifecycle_directories() {
    // root isolates the intentionally ambiguous lifecycle state.
    // root 隔离刻意构造的生命周期歧义状态。
    let root = test_root("archive_and_restore_reject_ambiguous_lifecycle_directories");
    // request provides the deterministic active and archived identities.
    // request 提供确定性的活动与归档身份。
    let request = test_session_request(SegmentPolicy::default());
    // active_dir is the valid canonical session that must remain untouched.
    // active_dir 是必须保持不变的有效规范会话。
    let active_dir = create_segmented_session_in_location(&root, false, request.clone())
        .expect("create active session");
    // archived_dir deliberately conflicts with the still-existing active identity.
    // archived_dir 刻意与仍存在的活动身份冲突。
    let archived_dir = segmented_session_directory(
        &root,
        true,
        &request.project_tag,
        &request.created_date,
        &request.session_id,
    );
    fs::create_dir_all(&archived_dir).expect("create conflicting archived directory");

    // archive_error and restore_error must both preserve the ambiguous state for diagnosis.
    // archive_error 与 restore_error 都必须保留歧义状态以供诊断。
    let archive_error = archive_segmented_session_by_created_date(
        &root,
        &request.project_tag,
        &request.created_date,
        &request.session_id,
        "2026-06-06T12:00:00Z",
    )
    .expect_err("ambiguous archive state should fail");
    let restore_error = restore_segmented_session_by_created_date(
        &root,
        &request.project_tag,
        &request.created_date,
        &request.session_id,
        "2026-06-06T12:30:00Z",
    )
    .expect_err("ambiguous restore state should fail");
    // active_exists and archived_exists capture the untouched state before cleanup.
    // active_exists 与 archived_exists 在清理前捕获未被修改的状态。
    let active_exists = active_dir.is_dir();
    let archived_exists = archived_dir.is_dir();
    let _ = fs::remove_dir_all(&root);

    assert!(archive_error.contains("both active and archived"));
    assert!(restore_error.contains("both archived and active"));
    assert!(active_exists);
    assert!(archived_exists);
}
