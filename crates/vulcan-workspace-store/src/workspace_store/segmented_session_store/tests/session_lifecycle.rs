use super::*;

/// Verifies the directory layout and first SessionMeta line for a new main session.
/// 验证新主会话的目录布局与首条 SessionMeta 行。
#[test]
fn create_session_writes_metadata_state_and_first_segment() {
    let root = test_root("create_session_writes_metadata_state_and_first_segment");
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        test_session_request(SegmentPolicy::default()),
    )
    .expect("create segmented session");

    assert!(session_dir.join(SEGMENTED_METADATA_FILE).exists());
    assert!(session_dir.join(SEGMENTED_STATE_FILE).exists());
    assert!(session_dir.join(SEGMENTED_TASK_DIR).is_dir());
    assert!(session_dir.join(SEGMENTED_BLOB_DIR).is_dir());
    assert!(!vmm_state_path(&session_dir).exists());
    assert!(!session_dir.join(SEGMENTED_VMM_MIGRATION_FILE).exists());
    assert!(
        !session_dir
            .join(SEGMENTED_VMM_MIGRATION_COMPLETED_FILE)
            .exists()
    );

    let metadata: SegmentedSessionMetadata =
        read_required_json(&session_dir.join(SEGMENTED_METADATA_FILE)).expect("read metadata");
    assert_eq!(metadata.active_segment, "s001.jsonl");
    assert_eq!(metadata.next_global_seq, 1);
    assert_eq!(metadata.segments[0].line_count, 1);

    let lines = read_segment_event_lines(&session_dir.join("s001.jsonl"))
        .expect("read first segment lines");
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].seq, 0);
    assert_eq!(lines[0].kind, SegmentedSessionEventKind::SessionMeta);
}

/// Verifies deterministic task-store directory lookup derives the date from raw creation time.
/// 验证确定性任务存储目录查找会从原始创建时间推导日期。
#[test]
fn task_store_locator_finds_session_directory_by_created_at() {
    let root = test_root("task_store_locator_finds_session_directory_by_created_at");
    let mut request = test_session_request(SegmentPolicy::default());
    request.created_date = String::from("1970-01-02");
    request.created_at = String::from("86400000");
    let session_id = request.session_id.clone();
    let project_tag = request.project_tag.clone();
    let created_at = request.created_at.clone();
    let session_dir = create_segmented_session_in_location(&root, false, request)
        .expect("create segmented session");

    let located = find_segmented_session_directory_for_task_store_created_at(
        &root,
        &project_tag,
        &created_at,
        &session_id,
    )
    .expect("deterministic locator should find session");
    // invalid_created_at proves the optional locator no longer maps malformed input onto 1970-01-01.
    // invalid_created_at 证明可选定位器不再把格式错误输入映射到 1970-01-01。
    let invalid_created_at = find_segmented_session_directory_for_task_store_created_at(
        &root,
        &project_tag,
        "not-a-timestamp",
        &session_id,
    );

    assert_eq!(located, session_dir);
    assert_eq!(invalid_created_at, None);
}

/// Verifies task mirror rows load from an already resolved segmented session directory.
/// 验证任务镜像行可以从已解析的分段会话目录加载。
#[test]
fn task_mirror_rows_load_from_resolved_session_directory() {
    let root = test_root("task_mirror_rows_load_from_resolved_session_directory");
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        test_session_request(SegmentPolicy::default()),
    )
    .expect("create segmented session");
    let task_session_id = Uuid::new_v4();
    // Keep the generated UUID string stable across fixture writing and mirror assertions.
    // 在写入夹具与镜像断言之间保持生成的 UUID 字符串稳定。
    let task_session_id_text = task_session_id.to_string();
    write_legacy_task_session_fixture(&session_dir, &task_session_id_text);

    let mirrors =
        crate::workspace_store::load_segmented_session_task_mirrors_from_directory(&session_dir)
            .expect("load task mirrors from resolved directory");

    assert_eq!(mirrors.len(), 1);
    assert_eq!(mirrors[0].status, "running");
    assert_eq!(mirrors[0].task_session_id, task_session_id);
}

/// Verifies a forked session rewrites only structural ownership fields.
/// 验证分叉会话只改写结构化归属字段。
#[test]
fn fork_session_directory_rewrites_structural_identity() {
    let root = test_root("fork_session_directory_rewrites_structural_identity");
    let source_session_id = Uuid::new_v4().to_string();
    let target_session_id = Uuid::new_v4().to_string();
    let mut request = test_session_request(SegmentPolicy::default());
    request.session_id = source_session_id.clone();
    let source_dir = create_segmented_session_in_location(&root, false, request)
        .expect("create segmented session");
    let source_runtime_state = SessionRuntimeStateEntry {
        active_provider_id: Some(String::from("source-provider")),
        updated_at: "2026-06-06T10:00:00Z".to_string(),
        ..SessionRuntimeStateEntry::default()
    };
    let mut source_state =
        state_document_from_runtime_state(&source_session_id, &source_runtime_state)
            .expect("build source state");
    source_state.session_boundary_state = json!({
        "thread_id": source_session_id,
        "next_sequence": 2,
        "status": "idle",
        "active_turn_id": null,
        "last_turn_id": "turn-alpha"
    });
    write_state(&source_dir, &source_state).expect("write source state");
    let mut source_vmm_state = vulcan_vmm::VmmSessionMemoryState::default();
    source_vmm_state
        .apply_recall_batch(
            &source_session_id,
            "source-user-anchor",
            &vulcan_vmm::VmmRecallBatch {
                ttl_turns: 3,
                injection_tag: String::from("VMM_MEMORY"),
                injection_description: String::from("Relevant memories."),
                items: vec![vulcan_vmm::ContextItem {
                    text: String::from("source-only-memory"),
                    memory_id: 91,
                    ..vulcan_vmm::ContextItem::default()
                }],
            },
            "2026-06-06T10:00:00Z",
        )
        .expect("source VMM state should be built");
    write_vmm_state(&source_dir, &source_session_id, &source_vmm_state)
        .expect("source VMM state should be written");
    fs::write(
        source_dir.join(SEGMENTED_VMM_MIGRATION_COMPLETED_FILE),
        b"source-only-completion-marker",
    )
    .expect("source VMM migration completion fixture should be written");
    fs::write(
        source_dir.join("vmm.corrupt.1.test.json"),
        b"source-only-corrupt-evidence",
    )
    .expect("source VMM quarantine fixture should be written");
    fs::write(
        source_dir.join(".vmm.json.vulcan-stage-test"),
        b"source-only-staging-evidence",
    )
    .expect("source VMM staging fixture should be written");
    let source_runtime_conversation = conversation_state_from_session_messages(&[test_message(
        &format!("{source_session_id}-runtime-user"),
        SessionMessageKind::User,
        SessionMessageRecordKind::Transcript,
        "runtime prompt",
    )])
    .expect("valid replayed conversation identities");
    sync_runtime_log_from_conversation_state(
        &source_dir,
        &source_runtime_conversation,
        "2026-06-06T10:00:00Z",
    )
    .expect("write source runtime log");
    append_runtime_log_event_if_present(
        &source_dir,
        "2026-06-06T10:00:01Z",
        RuntimeLogEvent::ProviderRequestCheckpoint(RuntimeProviderRequestCheckpointEvent {
            turn_id: Some(format!("{source_session_id}-turn")),
            checkpoint_id: format!("provider-request-{source_session_id}-alpha"),
            provider_id: String::from("deepseek"),
            model_id: String::from("deepseek-v4-pro"),
            provider_family: String::from("deepseek"),
            capability_fingerprint: Some(String::from("cap-alpha")),
            prompt_fingerprint: Some(String::from("prompt-alpha")),
            injection_fingerprint: Some(String::from("inject-alpha")),
            request_fingerprint: Some(String::from("request-alpha")),
            cache_policy_fingerprint: None,
            cache_policy_diagnostic: None,
            converter_version: 1,
        }),
    )
    .expect("write source runtime checkpoint");
    let source_runtime_lines = read_runtime_log_lines(&source_dir)
        .expect("read source runtime lines")
        .expect("source runtime exists");
    let source_task_dir = write_legacy_task_session_fixture(&source_dir, "task-alpha");
    assert!(source_task_dir.ends_with("task/task-alpha"));
    let source_task_runtime_state = SessionRuntimeStateEntry {
        active_provider_id: Some(String::from("task-provider")),
        updated_at: "2026-06-06T10:00:00Z".to_string(),
        ..SessionRuntimeStateEntry::default()
    };
    let mut source_task_state =
        state_document_from_runtime_state("task-alpha", &source_task_runtime_state)
            .expect("build source task state");
    source_task_state.session_boundary_state = json!({
        "thread_id": "task-alpha",
        "parent_thread_id": source_session_id,
    });
    write_state(&source_task_dir, &source_task_state).expect("write source task state");
    sync_runtime_log_from_conversation_state(
        &source_task_dir,
        &conversation_state_from_session_messages(&[test_message(
            "task-alpha-runtime-user",
            SessionMessageKind::User,
            SessionMessageRecordKind::Transcript,
            "task runtime prompt",
        )])
        .expect("valid replayed conversation identities"),
        "2026-06-06T10:00:00Z",
    )
    .expect("write source task runtime log");
    append_segmented_session_events(
        &source_dir,
        &[SegmentedSessionEventInput {
            timestamp: "2026-06-06T10:00:01Z".to_string(),
            kind: SegmentedSessionEventKind::SessionBoundary,
            payload: json!({
                "event_id": "event-alpha",
                "thread_id": source_session_id,
                "turn_id": "turn-alpha",
                "item_id": null,
                "kind": "turn_started",
                "scope": "turn",
                "visibility": "hidden",
                "sequence": 1,
                "payload": null
            }),
            turn_index: None,
        }],
    )
    .expect("append source boundary event");
    let mut task_card_message = test_message(
        "task-card",
        SessionMessageKind::Assistant,
        SessionMessageRecordKind::ToolResultOutput,
        "task done",
    );
    task_card_message.metadata = Some(json!({
        "task_session_id": "task-alpha",
        "task_id": "task-alpha",
        "child_thread_id": "task-alpha",
        "parent_thread_id": source_session_id.clone(),
        "subagent_mailbox_event": {
            "child_thread_id": "task-alpha",
            "parent_thread_id": source_session_id.clone(),
            "payload": {
                "task_session_id": "task-alpha",
                "task_id": "task-alpha",
                "child_thread_id": "task-alpha"
            }
        }
    }));
    append_segmented_session_message_update(&source_dir, &[task_card_message])
        .expect("append source task transcript card");

    let target_dir = fork_segmented_session_directory_from_source(
        &source_dir,
        &root,
        &SegmentedSessionForkRequest {
            source_archived: false,
            source_project_tag: "Project Alpha".to_string(),
            source_session_id: source_session_id.clone(),
            target_project_tag: "Project Alpha".to_string(),
            target_session_id: target_session_id.clone(),
            target_created_date: "2026-06-07".to_string(),
            target_created_at: "2026-06-07T10:00:00Z".to_string(),
            target_title: "Alpha Fork".to_string(),
            target_title_source: "user".to_string(),
            source_anchor_message_id: None,
        },
    )
    .expect("fork segmented session");

    let source_metadata = read_session_metadata(&source_dir).expect("read source metadata");
    assert_eq!(source_metadata.session_id, source_session_id);

    let target_metadata = read_session_metadata(&target_dir).expect("read target metadata");
    assert_eq!(target_metadata.session_id, target_session_id);
    assert_eq!(target_metadata.title.as_deref(), Some("Alpha Fork"));
    assert_eq!(target_metadata.created_at, "2026-06-07T10:00:00Z");
    assert!(target_metadata.tasks.is_empty());
    let target_task_root = target_dir.join(&target_metadata.task_dir);
    assert!(target_task_root.is_dir());
    assert!(
        fs::read_dir(&target_task_root)
            .expect("read target task root")
            .next()
            .is_none()
    );

    let target_state = read_state(&target_dir)
        .expect("read target state")
        .expect("target state exists");
    assert_eq!(target_state.session_id, target_session_id);
    assert_eq!(target_state.updated_at, "2026-06-07T10:00:00Z");
    assert!(vmm_state_path(&source_dir).exists());
    assert!(
        source_dir
            .join(SEGMENTED_VMM_MIGRATION_COMPLETED_FILE)
            .exists()
    );
    assert!(source_dir.join("vmm.corrupt.1.test.json").exists());
    assert!(source_dir.join(".vmm.json.vulcan-stage-test").exists());
    assert!(!vmm_state_path(&target_dir).exists());
    assert!(!target_dir.join(SEGMENTED_VMM_MIGRATION_FILE).exists());
    assert!(
        !target_dir
            .join(SEGMENTED_VMM_MIGRATION_COMPLETED_FILE)
            .exists()
    );
    assert!(!target_dir.join("vmm.corrupt.1.test.json").exists());
    assert!(!target_dir.join(".vmm.json.vulcan-stage-test").exists());
    assert_eq!(
        target_state.session_boundary_state["thread_id"],
        target_session_id
    );
    assert_eq!(
        target_state.runtime_state["active_provider_id"],
        "source-provider"
    );
    let target_metadata_text = fs::read_to_string(target_dir.join(SEGMENTED_METADATA_FILE))
        .expect("read target metadata text");
    assert!(!target_metadata_text.contains(&source_session_id));
    let target_state_text =
        fs::read_to_string(target_dir.join(SEGMENTED_STATE_FILE)).expect("read target state text");
    assert!(!target_state_text.contains(&source_session_id));
    let target_runtime_lines = read_runtime_log_lines(&target_dir)
        .expect("read target runtime lines")
        .expect("target runtime exists");
    assert_eq!(target_runtime_lines.len(), source_runtime_lines.len());
    let target_runtime_text =
        serde_json::to_string(&target_runtime_lines).expect("serialize target runtime lines");
    assert!(!target_runtime_text.contains(&source_session_id));
    assert!(target_runtime_text.contains(&target_session_id));
    let target_runtime_index =
        read_required_json::<RuntimeLogIndex>(&runtime_index_path(&target_dir))
            .expect("read target runtime index");
    assert!(
        target_runtime_index
            .provider_checkpoint_offsets
            .contains_key(&format!(
                "deepseek:deepseek:deepseek-v4-pro:provider-request-{target_session_id}-alpha"
            ))
    );

    let target_lines =
        read_segment_event_lines(&target_dir.join("s001.jsonl")).expect("read target lines");
    let target_segment_text =
        fs::read_to_string(target_dir.join("s001.jsonl")).expect("read target segment text");
    assert!(!target_segment_text.contains(&source_session_id));
    assert_eq!(target_lines[0].kind, SegmentedSessionEventKind::SessionMeta);
    assert_eq!(target_lines[0].payload["session_id"], target_session_id);
    assert_eq!(target_lines[0].payload["project_tag"], "Project Alpha");
    assert!(!target_lines.iter().any(|line| matches!(
        line.kind,
        SegmentedSessionEventKind::SubagentTaskStarted
            | SegmentedSessionEventKind::SubagentTaskEventLinked
            | SegmentedSessionEventKind::SubagentTaskCompleted
    )));
    let boundary_line = target_lines
        .iter()
        .find(|line| line.kind == SegmentedSessionEventKind::SessionBoundary)
        .expect("target boundary line exists");
    assert_eq!(boundary_line.payload["thread_id"], target_session_id);
    let task_card_line = target_lines
        .iter()
        .find(|line| line.payload["message_id"] == "task-card")
        .expect("target task card line exists");
    let task_card_metadata = &task_card_line.payload["message"]["metadata"];
    assert_eq!(task_card_metadata["task_session_id"], "task-alpha");
    assert_eq!(task_card_metadata["task_id"], "task-alpha");
    assert_eq!(task_card_metadata["child_thread_id"], "task-alpha");
    assert_eq!(task_card_metadata["parent_thread_id"], target_session_id);
    assert_eq!(
        task_card_metadata["subagent_mailbox_event"]["child_thread_id"],
        "task-alpha"
    );
    assert_eq!(
        task_card_metadata["subagent_mailbox_event"]["parent_thread_id"],
        target_session_id
    );
    assert_eq!(
        task_card_metadata["subagent_mailbox_event"]["payload"]["task_session_id"],
        "task-alpha"
    );
    assert_eq!(
        task_card_metadata["subagent_mailbox_event"]["payload"]["task_id"],
        "task-alpha"
    );
}

/// Verifies a fork rejects copied runtime artifacts when their metadata is missing.
/// 验证分叉会拒绝缺少元数据的已复制运行时产物。
#[test]
fn fork_session_directory_rejects_orphaned_runtime_artifacts_without_metadata() {
    // root isolates the orphaned-runtime fork fixture.
    // root 隔离孤儿运行时分叉夹具。
    let root = test_root("fork_session_directory_rejects_orphaned_runtime_artifacts");
    // source_session_id appears in the runtime event copied by the fork.
    // source_session_id 会出现在分叉复制的运行时事件中。
    let source_session_id = Uuid::new_v4().to_string();
    // target_session_id identifies the target directory that must be cleaned after failure.
    // target_session_id 标识失败后必须清理的目标目录。
    let target_session_id = Uuid::new_v4().to_string();
    // create_request assigns the generated source identity to the fixture session.
    // create_request 将生成的源身份分配给夹具会话。
    let mut create_request = test_session_request(SegmentPolicy::default());
    create_request.session_id = source_session_id.clone();
    // source_dir owns the runtime artifacts whose metadata will be removed.
    // source_dir 持有元数据将被删除的运行时产物。
    let source_dir = create_segmented_session_in_location(&root, false, create_request)
        .expect("create source session");
    // runtime_state embeds the source identity in one canonical provider event.
    // runtime_state 将源身份嵌入一条规范 Provider 事件。
    let runtime_state = conversation_state_from_session_messages(&[test_message(
        &format!("{source_session_id}-runtime-user"),
        SessionMessageKind::User,
        SessionMessageRecordKind::Transcript,
        "runtime prompt",
    )])
    .expect("valid replayed conversation identities");
    sync_runtime_log_from_conversation_state(&source_dir, &runtime_state, "t1")
        .expect("sync source runtime log");
    // source_metadata identifies the exact active segment retained after metadata deletion.
    // source_metadata 标识删除元数据后保留的精确活动分段。
    let source_metadata = read_runtime_metadata(&source_dir).expect("read runtime metadata");
    // source_segment_path contains authoritative runtime bytes that the failed fork must preserve.
    // source_segment_path 包含失败分叉必须保留的权威运行时字节。
    let source_segment_path = source_dir.join(&source_metadata.active_segment);
    // source_index_path contains the rebuildable but contradictory remaining index.
    // source_index_path 包含可重建但形成矛盾残留的索引。
    let source_index_path = runtime_index_path(&source_dir);
    // original_segment captures source evidence before the failed fork attempt.
    // original_segment 捕获失败分叉尝试前的源证据。
    let original_segment = fs::read(&source_segment_path).expect("read source runtime segment");
    // original_index captures the copied sidecar before the failed fork attempt.
    // original_index 捕获失败分叉尝试前的已复制附属索引。
    let original_index = fs::read(&source_index_path).expect("read source runtime index");
    fs::remove_file(runtime_metadata_path(&source_dir)).expect("remove source runtime metadata");
    // fork_request selects the no-anchor path that rewrites copied runtime identity in place.
    // fork_request 选择原地改写已复制运行时身份的无锚点路径。
    let fork_request = SegmentedSessionForkRequest {
        source_archived: false,
        source_project_tag: String::from("Project Alpha"),
        source_session_id: source_session_id.clone(),
        target_project_tag: String::from("Project Alpha"),
        target_session_id: target_session_id.clone(),
        target_created_date: String::from("2026-06-07"),
        target_created_at: String::from("2026-06-07T10:00:00Z"),
        target_title: String::from("Rejected Fork"),
        target_title_source: String::from("user"),
        source_anchor_message_id: None,
    };
    // target_dir is derived before execution so cleanup can be asserted after the error.
    // target_dir 在执行前推导，以便错误后断言清理结果。
    let target_dir = segmented_session_directory(
        &root,
        false,
        &fork_request.target_project_tag,
        &fork_request.target_created_date,
        &fork_request.target_session_id,
    );

    // error proves orphan classification aborts before the target session is published.
    // error 证明孤儿分类会在目标会话发布前中止。
    let error = fork_segmented_session_directory_from_source(&source_dir, &root, &fork_request)
        .expect_err("orphaned runtime artifacts must fail fork");
    // target_absence distinguishes completed cleanup from target metadata inspection failure.
    // target_absence 区分已完成清理与目标元数据检查失败。
    let target_absence = fs::symlink_metadata(&target_dir);
    // source_metadata_absence verifies the source interruption fixture remains unchanged.
    // source_metadata_absence 验证源中断夹具保持不变。
    let source_metadata_absence = fs::symlink_metadata(runtime_metadata_path(&source_dir));

    assert!(error.contains("Runtime metadata is missing while persisted runtime artifacts remain"));
    assert!(error.contains(&target_dir.display().to_string()));
    assert!(matches!(
        target_absence,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound
    ));
    assert_eq!(
        fs::read(&source_segment_path).expect("read preserved source runtime segment"),
        original_segment
    );
    assert_eq!(
        fs::read(&source_index_path).expect("read preserved source runtime index"),
        original_index
    );
    assert!(matches!(
        source_metadata_absence,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound
    ));
}

/// Verifies fork display conversion stops while streaming the source segment containing the anchor.
/// 验证分叉展示转换会在流式处理源分段时停在锚点所在位置。
#[test]
fn fork_session_directory_stops_display_at_source_anchor() {
    let root = test_root("fork_session_directory_stops_display_at_source_anchor");
    let source_session_id = Uuid::new_v4().to_string();
    let target_session_id = Uuid::new_v4().to_string();
    let mut request = test_session_request(SegmentPolicy::default());
    request.session_id = source_session_id.clone();
    let source_dir = create_segmented_session_in_location(&root, false, request)
        .expect("create segmented session");
    let anchor_message_id = format!("{source_session_id}-keep-anchor");
    let dropped_message_id = format!("{source_session_id}-drop-after-anchor");
    append_segmented_session_message_update(
        &source_dir,
        &[
            test_message(
                &anchor_message_id,
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::AssistantOutput,
                "keep this answer",
            ),
            test_message(
                &dropped_message_id,
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::AssistantOutput,
                "drop this answer",
            ),
        ],
    )
    .expect("append source transcript");

    let target_dir = fork_segmented_session_directory_from_source(
        &source_dir,
        &root,
        &SegmentedSessionForkRequest {
            source_archived: false,
            source_project_tag: "Project Alpha".to_string(),
            source_session_id: source_session_id.clone(),
            target_project_tag: "Project Alpha".to_string(),
            target_session_id: target_session_id.clone(),
            target_created_date: "2026-06-07".to_string(),
            target_created_at: "2026-06-07T10:00:00Z".to_string(),
            target_title: "Alpha Fork".to_string(),
            target_title_source: "user".to_string(),
            source_anchor_message_id: Some(anchor_message_id.clone()),
        },
    )
    .expect("fork segmented session");

    let target_metadata = read_session_metadata(&target_dir).expect("read target metadata");
    let target_text = target_metadata
        .segments
        .iter()
        .map(|segment| {
            fs::read_to_string(target_dir.join(&segment.file)).expect("read target segment")
        })
        .collect::<String>();
    assert!(
        target_text.contains(&anchor_message_id.replace(&source_session_id, &target_session_id))
    );
    assert!(!target_text.contains(&source_session_id));
    assert!(
        !target_text.contains(&dropped_message_id.replace(&source_session_id, &target_session_id))
    );
    assert_eq!(target_metadata.session_id, target_session_id);
}
