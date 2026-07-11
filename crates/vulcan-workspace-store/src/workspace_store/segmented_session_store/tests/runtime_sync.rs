use super::*;

/// Builds one deterministic runtime system-update event for persistence tests.
/// 为持久化测试构建一个确定性的运行时系统更新事件。
///
/// # Parameters
/// 参数
/// `update_id` is the unique system-update identifier asserted by the test.
/// `update_id` 是测试断言使用的唯一系统更新标识。
/// `text` is the update body stored in the runtime event.
/// `text` 是存储在运行时事件中的更新正文。
///
/// # Returns
/// 返回值
/// A complete trusted-host system-update runtime event.
/// 返回一条完整的可信宿主系统更新运行时事件。
fn test_runtime_system_update_event(update_id: &str, text: &str) -> RuntimeLogEvent {
    RuntimeLogEvent::SystemUpdate(RuntimeSystemUpdateEvent {
        turn_id: Some(format!("turn-{update_id}")),
        update_id: update_id.to_string(),
        effective_after_event_id: None,
        text: text.to_string(),
        trust: RuntimeSystemUpdateTrust::TrustedHost,
        lowering_hint: RuntimeSystemUpdateLoweringHint::Auto,
        fingerprint: Some(format!("fingerprint-{update_id}")),
        source: Some(String::from("runtime-sync-test")),
    })
}

/// Reads the persisted runtime index used by transaction assertions.
/// 读取事务断言使用的已持久化运行时索引。
///
/// # Parameters
/// 参数
/// `session_dir` is the segmented session directory that owns the runtime index.
/// `session_dir` 是拥有运行时索引的分段会话目录。
///
/// # Returns
/// 返回值
/// The decoded runtime index, panicking only when the test fixture is invalid.
/// 返回已解码运行时索引；仅在测试夹具无效时触发 panic。
fn read_test_runtime_index(session_dir: &Path) -> RuntimeLogIndex {
    read_required_json::<RuntimeLogIndex>(&runtime_index_path(session_dir))
        .expect("read runtime index")
}

/// Verifies compaction runtime backups copy active files and keep only the newest five backups.
/// 验证压缩运行时备份会复制活动文件，并且仅保留最新五份备份。
#[test]
fn runtime_log_backup_before_compaction_copies_sources_and_keeps_latest_five() {
    let root =
        test_root("runtime_log_backup_before_compaction_copies_sources_and_keeps_latest_five");
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        test_session_request(SegmentPolicy::default()),
    )
    .expect("create session");
    let messages = vec![test_message(
        "m-user-1",
        SessionMessageKind::User,
        SessionMessageRecordKind::Transcript,
        "first prompt",
    )];
    let conversation_state = conversation_state_from_session_messages(&messages)
        .expect("valid replayed conversation identities");
    rebuild_runtime_log_from_conversation_state(
        &session_dir,
        &conversation_state,
        "2026-06-09T10:00:00Z",
    )
    .expect("rebuild runtime log");
    // unrelated_runtime_file proves backup copies the implemented contract instead of recursing.
    // unrelated_runtime_file 证明备份只复制已实现契约，而不会递归复制无关文件。
    let unrelated_runtime_file = session_dir
        .join(SEGMENTED_RUNTIME_DIR)
        .join("unrelated.tmp");
    fs::write(&unrelated_runtime_file, b"must not be backed up")
        .expect("write unrelated runtime file");

    for index in 0..6 {
        // metadata_before captures the authoritative metadata bytes before the audit append.
        // metadata_before 捕获审计追加前的权威元数据字节。
        let metadata_before = fs::read(runtime_metadata_path(&session_dir))
            .expect("read runtime metadata before backup");
        // index_before captures the optional index bytes before the audit append rebuilds it.
        // index_before 捕获审计追加重建索引前的可选索引字节。
        let index_before =
            fs::read(runtime_index_path(&session_dir)).expect("read runtime index before backup");
        // segment_before captures the exact immutable source bytes protected by the append lock.
        // segment_before 捕获由追加锁保护的精确不可变源字节。
        let segment_before = fs::read(session_dir.join(runtime_segment_file_name()))
            .expect("read runtime segment before backup");
        let copied_files = backup_runtime_log_before_compaction(
            &session_dir,
            &format!("compaction-{index}"),
            &format!("20260609{index:02}"),
        )
        .expect("backup runtime log");
        assert!(
            copied_files
                .iter()
                .any(|path| path.ends_with("runtime/r001.jsonl"))
        );
        // copied_metadata locates the exact metadata artifact reported by the transaction.
        // copied_metadata 定位事务报告的精确元数据工件。
        let copied_metadata = copied_files
            .iter()
            .find(|path| path.ends_with(SEGMENTED_RUNTIME_METADATA_FILE))
            .expect("copied metadata path");
        // copied_index locates the exact optional index artifact reported by the transaction.
        // copied_index 定位事务报告的精确可选索引工件。
        let copied_index = copied_files
            .iter()
            .find(|path| path.ends_with(SEGMENTED_RUNTIME_INDEX_FILE))
            .expect("copied index path");
        // copied_segment locates the fixed runtime segment artifact reported by the transaction.
        // copied_segment 定位事务报告的固定运行时分段工件。
        let copied_segment = copied_files
            .iter()
            .find(|path| path.ends_with("runtime/r001.jsonl"))
            .expect("copied runtime segment path");
        assert_eq!(
            fs::read(session_dir.join(copied_metadata)).expect("read backed-up metadata"),
            metadata_before
        );
        assert_eq!(
            fs::read(session_dir.join(copied_index)).expect("read backed-up index"),
            index_before
        );
        assert_eq!(
            fs::read(session_dir.join(copied_segment)).expect("read backed-up segment"),
            segment_before
        );
        assert!(
            !copied_files
                .iter()
                .any(|path| path.ends_with("unrelated.tmp"))
        );
    }
    let mut backup_names = fs::read_dir(session_dir.join(SEGMENTED_RUNTIME_BACKUP_DIR))
        .expect("read backup dir")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .collect::<Vec<_>>();
    backup_names.sort();

    assert_eq!(backup_names.len(), 5);
    assert!(
        !backup_names
            .iter()
            .any(|name| name.contains("compaction-0"))
    );
    assert!(
        backup_names
            .iter()
            .any(|name| name.contains("compaction-5"))
    );
    assert!(
        backup_names
            .iter()
            .all(|name| name.contains("segment-r001"))
    );
    assert!(backup_names.iter().all(|name| name.contains("seq1-")));
    assert!(backup_names.iter().any(|name| name.contains("seq1-6")));

    let runtime_lines = read_runtime_log_lines(&session_dir)
        .expect("read runtime log")
        .expect("runtime log should exist");
    let latest_compaction = runtime_lines
        .iter()
        .rev()
        .find_map(|line| match &line.event {
            RuntimeLogEvent::ContextCompactionApplied(event) => Some(event),
            _ => None,
        })
        .expect("context compaction audit event should exist");
    assert!(
        latest_compaction
            .backup_files
            .iter()
            .any(|path| path.ends_with("runtime/r001.jsonl"))
    );
    assert!(
        latest_compaction
            .compaction_fingerprint
            .as_deref()
            .is_some_and(|fingerprint| !fingerprint.is_empty())
    );
}

/// Verifies a missing rebuildable index does not block a snapshot backup or audit append.
/// 验证缺失的可重建索引不会阻塞快照备份或审计追加。
#[test]
fn runtime_log_backup_allows_missing_rebuildable_index() {
    // root owns the isolated backup fixture.
    // root 拥有隔离的备份测试夹具。
    let root = test_root("runtime_log_backup_allows_missing_rebuildable_index");
    // session_dir owns the valid metadata and fixed runtime segment.
    // session_dir 拥有有效元数据与固定运行时分段。
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        test_session_request(SegmentPolicy::default()),
    )
    .expect("create session");
    // state supplies one non-empty runtime event so the compaction audit must append.
    // state 提供一个非空运行时事件，从而压缩审计必须追加。
    let state = conversation_state_from_session_messages(&[test_message(
        "m-user-1",
        SessionMessageKind::User,
        SessionMessageRecordKind::Transcript,
        "first prompt",
    )])
    .expect("valid replayed conversation identities");
    rebuild_runtime_log_from_conversation_state(&session_dir, &state, "t1")
        .expect("rebuild runtime log");
    fs::remove_file(runtime_index_path(&session_dir)).expect("remove rebuildable index");

    // copied_files records only artifacts that actually existed at backup time.
    // copied_files 只记录备份时真实存在的工件。
    let copied_files = backup_runtime_log_before_compaction(&session_dir, "compaction-a", "t2")
        .expect("backup without runtime index");
    // runtime_lines proves the same transaction appended the compaction audit.
    // runtime_lines 证明同一个事务追加了压缩审计。
    let runtime_lines = read_runtime_log_lines(&session_dir)
        .expect("read runtime log")
        .expect("runtime log exists");

    assert!(
        !copied_files
            .iter()
            .any(|path| path.ends_with(SEGMENTED_RUNTIME_INDEX_FILE))
    );
    assert!(
        copied_files
            .iter()
            .any(|path| path.ends_with("runtime/r001.jsonl"))
    );
    assert!(runtime_index_path(&session_dir).is_file());
    assert!(runtime_lines.iter().any(|line| {
        matches!(
            &line.event,
            RuntimeLogEvent::ContextCompactionApplied(event)
                if event.compaction_id == "compaction-a"
        )
    }));
}

/// Verifies corrupt runtime metadata fails before creating any backup directory or changing data.
/// 验证损坏的运行时元数据会在创建备份目录或修改数据前失败。
#[test]
fn runtime_log_backup_rejects_corrupt_metadata_without_side_effects() {
    // root owns the isolated corruption fixture.
    // root 拥有隔离的损坏测试夹具。
    let root = test_root("runtime_log_backup_rejects_corrupt_metadata_without_side_effects");
    // session_dir owns the runtime artifacts being protected from partial backup writes.
    // session_dir 拥有需要防止部分备份写入的运行时工件。
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        test_session_request(SegmentPolicy::default()),
    )
    .expect("create session");
    // state establishes a complete runtime log before metadata corruption.
    // state 在元数据损坏前建立完整运行时日志。
    let state = conversation_state_from_session_messages(&[test_message(
        "m-user-1",
        SessionMessageKind::User,
        SessionMessageRecordKind::Transcript,
        "first prompt",
    )])
    .expect("valid replayed conversation identities");
    rebuild_runtime_log_from_conversation_state(&session_dir, &state, "t1")
        .expect("rebuild runtime log");
    // segment_before proves the failed validation path does not rewrite source data.
    // segment_before 证明验证失败路径不会重写源数据。
    let segment_before = fs::read(session_dir.join(runtime_segment_file_name()))
        .expect("read runtime segment before corrupt backup");
    fs::write(runtime_metadata_path(&session_dir), b"{ broken metadata\n")
        .expect("corrupt runtime metadata");

    // error captures the strict metadata parse failure.
    // error 捕获严格的元数据解析失败。
    let error = backup_runtime_log_before_compaction(&session_dir, "compaction-a", "t2")
        .expect_err("corrupt runtime metadata must reject backup");

    assert!(
        error.contains("解析 JSON 文件失败"),
        "unexpected error: {error}"
    );
    assert!(!session_dir.join(SEGMENTED_RUNTIME_BACKUP_DIR).exists());
    assert_eq!(
        fs::read(session_dir.join(runtime_segment_file_name()))
            .expect("read runtime segment after rejected backup"),
        segment_before
    );
}

/// Verifies orphan runtime artifacts fail before creating a backup directory.
/// 验证孤儿运行时工件会在创建备份目录前失败。
#[test]
fn runtime_log_backup_rejects_orphan_artifacts_without_side_effects() {
    // root owns the isolated orphan-artifact fixture.
    // root 拥有隔离的孤儿工件测试夹具。
    let root = test_root("runtime_log_backup_rejects_orphan_artifacts_without_side_effects");
    // session_dir owns the runtime artifacts whose metadata will be removed.
    // session_dir 拥有即将移除元数据的运行时工件。
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        test_session_request(SegmentPolicy::default()),
    )
    .expect("create session");
    // state establishes the segment and index that become orphans.
    // state 建立随后会成为孤儿的分段与索引。
    let state = conversation_state_from_session_messages(&[test_message(
        "m-user-1",
        SessionMessageKind::User,
        SessionMessageRecordKind::Transcript,
        "first prompt",
    )])
    .expect("valid replayed conversation identities");
    rebuild_runtime_log_from_conversation_state(&session_dir, &state, "t1")
        .expect("rebuild runtime log");
    // segment_before proves orphan diagnosis leaves the remaining source untouched.
    // segment_before 证明孤儿诊断不会修改剩余源文件。
    let segment_before = fs::read(session_dir.join(runtime_segment_file_name()))
        .expect("read runtime segment before orphan backup");
    fs::remove_file(runtime_metadata_path(&session_dir)).expect("remove runtime metadata");

    // error captures the explicit inconsistent-absence diagnosis.
    // error 捕获明确的不一致缺失诊断。
    let error = backup_runtime_log_before_compaction(&session_dir, "compaction-a", "t2")
        .expect_err("orphan runtime artifacts must reject backup");

    assert!(
        error.contains("Runtime metadata is missing"),
        "unexpected error: {error}"
    );
    assert!(!session_dir.join(SEGMENTED_RUNTIME_BACKUP_DIR).exists());
    assert_eq!(
        fs::read(session_dir.join(runtime_segment_file_name()))
            .expect("read runtime segment after rejected orphan backup"),
        segment_before
    );
}

/// Verifies runtime sync appends only new tail lines when the existing log is a prefix.
/// 验证现有日志是前缀时 runtime 同步只追加新的尾部行。
#[test]
fn runtime_log_sync_appends_tail_when_prefix_matches() {
    let root = test_root("runtime_log_sync_appends_tail_when_prefix_matches");
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        test_session_request(SegmentPolicy::default()),
    )
    .expect("create session");
    let first_messages = vec![test_message(
        "m-user-1",
        SessionMessageKind::User,
        SessionMessageRecordKind::Transcript,
        "first prompt",
    )];
    let first_state = conversation_state_from_session_messages(&first_messages)
        .expect("valid replayed conversation identities");
    let first_metadata = sync_runtime_log_from_conversation_state(&session_dir, &first_state, "t1")
        .expect("sync first runtime log");
    let second_messages = vec![
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
    let second_state = conversation_state_from_session_messages(&second_messages)
        .expect("valid replayed conversation identities");

    let second_metadata =
        sync_runtime_log_from_conversation_state(&session_dir, &second_state, "t2")
            .expect("append runtime tail");
    let lines = read_runtime_log_lines(&session_dir)
        .expect("read runtime lines")
        .expect("runtime lines exist");

    assert_eq!(first_metadata.created_at, "t1");
    assert_eq!(second_metadata.created_at, "t1");
    assert_eq!(second_metadata.updated_at, "t2");
    assert_eq!(lines.len(), 2);
}

/// Verifies a strict projection failure propagates before any persisted runtime artifact changes.
/// 验证严格投影失败会在任何持久化运行时工件变化前传播。
#[test]
fn runtime_log_sync_rejects_projection_failure_without_writes() {
    // root owns the isolated projection-failure fixture.
    // root 拥有隔离的投影失败测试夹具。
    let root = test_root("runtime_log_sync_rejects_projection_failure_without_writes");
    // session_dir owns the valid runtime snapshot protected by this regression test.
    // session_dir 拥有本回归测试保护的有效运行时快照。
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        test_session_request(SegmentPolicy::default()),
    )
    .expect("create session");
    // valid_state establishes a complete persisted runtime baseline.
    // valid_state 建立完整的已持久化运行时基线。
    let valid_state = conversation_state_from_session_messages(&[test_message(
        "m-user-1",
        SessionMessageKind::User,
        SessionMessageRecordKind::Transcript,
        "first prompt",
    )])
    .expect("valid replayed conversation identities");
    sync_runtime_log_from_conversation_state(&session_dir, &valid_state, "t1")
        .expect("sync valid runtime baseline");
    // metadata_before captures the authoritative metadata before the rejected projection.
    // metadata_before 捕获投影被拒绝前的权威元数据。
    let metadata_before =
        fs::read(runtime_metadata_path(&session_dir)).expect("read baseline metadata");
    // index_before captures the rebuildable index before the rejected projection.
    // index_before 捕获投影被拒绝前的可重建索引。
    let index_before = fs::read(runtime_index_path(&session_dir)).expect("read baseline index");
    // segment_before captures the provider-facing bytes before the rejected projection.
    // segment_before 捕获投影被拒绝前的 Provider 可见字节。
    let segment_before = fs::read(session_dir.join(runtime_segment_file_name()))
        .expect("read baseline runtime segment");
    // duplicate_call is cloned so both calls resolve to the same runtime-owned identifier.
    // duplicate_call 会被克隆，使两个调用解析为相同运行时自有标识。
    let duplicate_call = vulcan_context::protocol::SessionConversationToolCall {
        id: String::from("call-a"),
        tool_call_id: Some(String::from("call-a")),
        tool_name: String::from("demo"),
        arguments: json!({}),
    };
    // invalid_state contains the identity conflict that must stop projection before persistence.
    // invalid_state 包含必须在持久化前停止投影的身份冲突。
    let invalid_state = vulcan_context::protocol::SessionConversationState {
        messages: vec![vulcan_context::protocol::SessionConversationMessage {
            id: String::from("assistant-1"),
            payload: vulcan_context::protocol::SessionConversationPayload::Assistant {
                text: String::new(),
                reasoning: String::new(),
                provider_message_id: None,
                provider_message_state: None,
                usage: None,
                tool_calls: vec![duplicate_call.clone(), duplicate_call],
            },
        }],
        records: Vec::new(),
        continuation: Default::default(),
    };

    // error retains the typed projection diagnosis through the string-based store boundary.
    // error 通过字符串存储边界保留类型化投影诊断。
    let error = sync_runtime_log_from_conversation_state(&session_dir, &invalid_state, "t2")
        .expect_err("duplicate tool-call identity must reject runtime synchronization");

    assert!(
        error.contains("duplicate tool-call identifier `call-a`"),
        "unexpected error: {error}"
    );
    assert_eq!(
        fs::read(runtime_metadata_path(&session_dir)).expect("read metadata after rejection"),
        metadata_before
    );
    assert_eq!(
        fs::read(runtime_index_path(&session_dir)).expect("read index after rejection"),
        index_before
    );
    assert_eq!(
        fs::read(session_dir.join(runtime_segment_file_name()))
            .expect("read segment after rejection"),
        segment_before
    );
}

/// Verifies runtime sync rebuilds when the desired history is not prefixed by the existing log.
/// 验证目标历史不再以前缀匹配现有日志时 runtime 同步会重建。
#[test]
fn runtime_log_sync_rebuilds_when_prefix_mismatches() {
    let root = test_root("runtime_log_sync_rebuilds_when_prefix_mismatches");
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        test_session_request(SegmentPolicy::default()),
    )
    .expect("create session");
    let first_state = conversation_state_from_session_messages(&[test_message(
        "m-user-1",
        SessionMessageKind::User,
        SessionMessageRecordKind::Transcript,
        "first prompt",
    )])
    .expect("valid replayed conversation identities");
    sync_runtime_log_from_conversation_state(&session_dir, &first_state, "t1")
        .expect("sync first runtime log");
    let rewritten_state = conversation_state_from_session_messages(&[test_message(
        "m-user-1",
        SessionMessageKind::User,
        SessionMessageRecordKind::Transcript,
        "rewritten prompt",
    )])
    .expect("valid replayed conversation identities");

    let rebuilt_metadata =
        sync_runtime_log_from_conversation_state(&session_dir, &rewritten_state, "t3")
            .expect("rebuild runtime log");
    let restored = read_runtime_conversation_state(&session_dir)
        .expect("read runtime conversation")
        .expect("runtime conversation exists");

    assert_eq!(rebuilt_metadata.created_at, "t3");
    let vulcan_context::SessionConversationPayload::UserText { text, .. } =
        &restored.messages[0].payload
    else {
        panic!("expected user text payload");
    };
    assert_eq!(text, "rewritten prompt");
}

/// Verifies concurrent audit writers both commit unique contiguous sequences.
/// 验证并发审计写入方都会提交唯一且连续的序号。
#[test]
fn concurrent_runtime_audit_appends_commit_without_event_loss() {
    // root isolates the concurrent append transaction fixture.
    // root 隔离并发追加事务夹具。
    let root = test_root("concurrent_runtime_audit_appends_commit_without_event_loss");
    // session_dir owns the runtime log shared by both writer threads.
    // session_dir 持有两个写入线程共享的运行时日志。
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        test_session_request(SegmentPolicy::default()),
    )
    .expect("create session");
    // initial_state creates the non-empty runtime log required by audit append semantics.
    // initial_state 创建审计追加语义所要求的非空运行时日志。
    let initial_state = conversation_state_from_session_messages(&[test_message(
        "m-user-1",
        SessionMessageKind::User,
        SessionMessageRecordKind::Transcript,
        "first prompt",
    )])
    .expect("valid replayed conversation identities");
    sync_runtime_log_from_conversation_state(&session_dir, &initial_state, "t1")
        .expect("sync initial runtime log");
    // barrier releases both writers only after every thread is ready.
    // barrier 仅在所有线程准备完成后同时释放两个写入方。
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
    // writer_handles retain both concurrent append results for deterministic joining.
    // writer_handles 保留两个并发追加结果，以便确定性等待。
    let writer_handles = (0..2)
        .map(|writer_index| {
            // writer_session_dir points each thread at the same persisted runtime log.
            // writer_session_dir 使每个线程指向同一份持久化运行时日志。
            let writer_session_dir = session_dir.clone();
            // writer_barrier synchronizes the start of the persistence calls.
            // writer_barrier 同步持久化调用的开始时刻。
            let writer_barrier = std::sync::Arc::clone(&barrier);
            std::thread::spawn(move || {
                writer_barrier.wait();
                // update_id identifies the event owned by this writer.
                // update_id 标识当前写入方拥有的事件。
                let update_id = format!("concurrent-audit-{writer_index}");
                record_runtime_audit_event(
                    &writer_session_dir,
                    &format!("t-audit-{writer_index}"),
                    test_runtime_system_update_event(&update_id, "concurrent audit"),
                )
            })
        })
        .collect::<Vec<_>>();
    barrier.wait();
    // append_results proves both transactions returned successful writes.
    // append_results 证明两个事务都返回了成功写入。
    let append_results = writer_handles
        .into_iter()
        .map(|handle| {
            handle
                .join()
                .expect("join audit writer")
                .expect("append concurrent audit")
        })
        .collect::<Vec<_>>();
    // lines captures the final serialized transaction order.
    // lines 捕获最终序列化事务顺序。
    let lines = read_runtime_log_lines(&session_dir)
        .expect("read runtime lines")
        .expect("runtime lines exist");
    // sequences exposes exact persisted ordering for the contiguity assertion.
    // sequences 暴露精确持久化顺序，用于连续性断言。
    let sequences = lines.iter().map(|line| line.seq).collect::<Vec<_>>();
    // index captures the rebuildable sidecar after both serialized append transactions.
    // index 捕获两个串行追加事务后的可重建附属索引。
    let index = read_test_runtime_index(&session_dir);
    // update_ids collects both unique audit identifiers independent of lock acquisition order.
    // update_ids 收集两个唯一审计标识，且不依赖锁获取顺序。
    let mut update_ids = lines
        .iter()
        .filter_map(|line| match &line.event {
            RuntimeLogEvent::SystemUpdate(event) => Some(event.update_id.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    update_ids.sort();

    assert_eq!(append_results, vec![true, true]);
    assert_eq!(sequences, vec![1, 2, 3]);
    assert_eq!(index.last_seq, Some(3));
    assert_eq!(index.line_count, 3);
    assert_eq!(
        update_ids,
        vec![
            String::from("concurrent-audit-0"),
            String::from("concurrent-audit-1")
        ]
    );
    for update_id in &update_ids {
        // event_seq is the authoritative JSONL sequence mirrored by the index entry.
        // event_seq 是索引条目所镜像的权威 JSONL 序号。
        let event_seq = lines
            .iter()
            .find_map(|line| match &line.event {
                RuntimeLogEvent::SystemUpdate(event) if &event.update_id == update_id => {
                    Some(line.seq)
                }
                _ => None,
            })
            .expect("find persisted concurrent audit event");
        assert_eq!(index.system_update_offsets.get(update_id), Some(&event_seq));
    }
}

/// Verifies a rebuilding sync and audit append preserve the audit event in either lock order.
/// 验证重建同步与审计追加无论锁顺序如何都会保留审计事件。
#[test]
fn concurrent_runtime_rebuild_and_audit_append_preserve_both_results() {
    // root isolates the concurrent rebuild-and-append fixture.
    // root 隔离并发重建与追加夹具。
    let root = test_root("concurrent_runtime_rebuild_and_audit_append_preserve_both_results");
    // session_dir owns the runtime state both operations mutate.
    // session_dir 持有两个操作共同修改的运行时状态。
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        test_session_request(SegmentPolicy::default()),
    )
    .expect("create session");
    // initial_state establishes history that the concurrent sync must replace.
    // initial_state 建立并发同步必须替换的历史。
    let initial_state = conversation_state_from_session_messages(&[test_message(
        "m-user-1",
        SessionMessageKind::User,
        SessionMessageRecordKind::Transcript,
        "initial prompt",
    )])
    .expect("valid replayed conversation identities");
    sync_runtime_log_from_conversation_state(&session_dir, &initial_state, "t1")
        .expect("sync initial runtime log");
    // rewritten_state forces the sync path to rebuild rather than append a prefix tail.
    // rewritten_state 强制同步路径执行重建，而不是追加前缀尾部。
    let rewritten_state = conversation_state_from_session_messages(&[test_message(
        "m-user-1",
        SessionMessageKind::User,
        SessionMessageRecordKind::Transcript,
        "rewritten concurrent prompt",
    )])
    .expect("valid replayed conversation identities");
    // barrier aligns both operations immediately before lock acquisition.
    // barrier 使两个操作在获取锁之前对齐。
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
    // sync_session_dir is the path owned by the rebuild thread.
    // sync_session_dir 是重建线程拥有的路径。
    let sync_session_dir = session_dir.clone();
    // sync_barrier releases the rebuild thread with the audit thread.
    // sync_barrier 与审计线程一起释放重建线程。
    let sync_barrier = std::sync::Arc::clone(&barrier);
    // sync_handle retains the rebuild result until both operations finish.
    // sync_handle 保留重建结果，直到两个操作都结束。
    let sync_handle = std::thread::spawn(move || {
        sync_barrier.wait();
        sync_runtime_log_from_conversation_state(&sync_session_dir, &rewritten_state, "t3")
    });
    // audit_session_dir is the path owned by the audit thread.
    // audit_session_dir 是审计线程拥有的路径。
    let audit_session_dir = session_dir.clone();
    // audit_barrier releases the audit thread with the rebuild thread.
    // audit_barrier 与重建线程一起释放审计线程。
    let audit_barrier = std::sync::Arc::clone(&barrier);
    // audit_handle retains the append result until both operations finish.
    // audit_handle 保留追加结果，直到两个操作都结束。
    let audit_handle = std::thread::spawn(move || {
        audit_barrier.wait();
        record_runtime_audit_event(
            &audit_session_dir,
            "t2",
            test_runtime_system_update_event("concurrent-rebuild-audit", "must survive rebuild"),
        )
    });
    barrier.wait();
    sync_handle
        .join()
        .expect("join runtime sync")
        .expect("rebuild runtime log");
    assert!(
        audit_handle
            .join()
            .expect("join runtime audit")
            .expect("append runtime audit")
    );
    // restored_state verifies the canonical replacement won the intended sync semantics.
    // restored_state 验证规范替换实现了预期同步语义。
    let restored_state = read_runtime_conversation_state(&session_dir)
        .expect("read runtime conversation")
        .expect("runtime conversation exists");
    // final_lines verifies the audit-only event survived either transaction order.
    // final_lines 验证仅审计事件在任一事务顺序下都得到保留。
    let final_lines = read_runtime_log_lines(&session_dir)
        .expect("read final runtime lines")
        .expect("final runtime lines exist");
    // final_index must mirror the same post-race log rather than an earlier transaction.
    // final_index 必须镜像同一份竞态后日志，而不是更早的事务。
    let final_index = read_test_runtime_index(&session_dir);
    let vulcan_context::SessionConversationPayload::UserText { text, .. } =
        &restored_state.messages[0].payload
    else {
        panic!("expected rewritten user text payload");
    };

    assert_eq!(text, "rewritten concurrent prompt");
    assert!(final_lines.iter().any(|line| {
        matches!(
            &line.event,
            RuntimeLogEvent::SystemUpdate(event)
                if event.update_id == "concurrent-rebuild-audit"
        )
    }));
    // audit_seq identifies the surviving event sequence for the index assertion.
    // audit_seq 标识用于索引断言的已保留事件序号。
    let audit_seq = final_lines
        .iter()
        .find_map(|line| match &line.event {
            RuntimeLogEvent::SystemUpdate(event)
                if event.update_id == "concurrent-rebuild-audit" =>
            {
                Some(line.seq)
            }
            _ => None,
        })
        .expect("find concurrent rebuild audit event");
    assert_eq!(
        final_index
            .system_update_offsets
            .get("concurrent-rebuild-audit"),
        Some(&audit_seq)
    );
    assert_eq!(
        final_index.line_count,
        u64::try_from(final_lines.len()).expect("small test line count")
    );
    assert_eq!(
        final_index.last_seq,
        final_lines.last().map(|line| line.seq)
    );
}

/// Verifies stale next-sequence metadata is rejected before an audit append writes bytes.
/// 验证陈旧的下一序号元数据会在审计追加写入字节前被拒绝。
#[test]
fn runtime_audit_append_rejects_stale_next_sequence_before_write() {
    // root isolates the stale sequence metadata fixture.
    // root 隔离陈旧序号元数据夹具。
    let root = test_root("runtime_audit_append_rejects_stale_next_sequence_before_write");
    // session_dir owns the valid runtime log whose metadata is corrupted.
    // session_dir 持有元数据将被破坏的有效运行时日志。
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        test_session_request(SegmentPolicy::default()),
    )
    .expect("create session");
    // state materializes exactly one runtime line and therefore next sequence two.
    // state 精确物化一条运行时日志行，因此下一序号为二。
    let state = conversation_state_from_session_messages(&[test_message(
        "m-user-1",
        SessionMessageKind::User,
        SessionMessageRecordKind::Transcript,
        "first prompt",
    )])
    .expect("valid replayed conversation identities");
    sync_runtime_log_from_conversation_state(&session_dir, &state, "t1").expect("sync runtime log");
    // metadata is the valid source cloned for both stale-low and stale-high cases.
    // metadata 是有效源元数据，用于克隆出过低与过高两种陈旧情况。
    let metadata = read_runtime_metadata(&session_dir).expect("read runtime metadata");
    // metadata_path receives intentionally invalid JSON metadata through the raw atomic writer.
    // metadata_path 通过原始原子写入器接收有意构造的无效 JSON 元数据。
    let metadata_path = runtime_metadata_path(&session_dir);
    // segment_path identifies the authoritative bytes that every failed append must preserve.
    // segment_path 标识每次失败追加都必须保留的权威字节。
    let segment_path = session_dir.join(&metadata.active_segment);
    // original_segment captures the exact precondition bytes.
    // original_segment 捕获精确前置条件字节。
    let original_segment = fs::read(&segment_path).expect("read runtime segment");
    // original_index captures the sidecar bytes that failed appends must not refresh.
    // original_index 捕获失败追加不得刷新的附属索引字节。
    let original_index = fs::read(runtime_index_path(&session_dir)).expect("read runtime index");
    // invalid_next_sequences covers both duplicate and skipped-sequence metadata.
    // invalid_next_sequences 覆盖重复序号与跳号两种元数据。
    let invalid_next_sequences = [1_u64, 3_u64];
    for invalid_next_seq in invalid_next_sequences {
        // invalid_metadata changes only the authoritative next sequence.
        // invalid_metadata 仅修改权威下一序号。
        let mut invalid_metadata = metadata.clone();
        invalid_metadata.next_seq = invalid_next_seq;
        write_json_file_atomic(&metadata_path, &invalid_metadata)
            .expect("write invalid runtime metadata");
        // invalid_metadata_bytes captures the exact rejected document for zero-write proof.
        // invalid_metadata_bytes 捕获被拒绝文档的精确字节，用于证明零写入。
        let invalid_metadata_bytes =
            fs::read(&metadata_path).expect("read invalid runtime metadata");
        // error proves each invalid sequence is rejected before segment append.
        // error 证明每个无效序号都会在分段追加前被拒绝。
        let error = record_runtime_audit_event(
            &session_dir,
            "t2",
            test_runtime_system_update_event("stale-next-sequence", "must not append"),
        )
        .expect_err("stale next sequence must fail audit append");
        assert!(error.contains("Runtime metadata next sequence mismatch"));
        assert_eq!(
            fs::read(&segment_path).expect("read preserved runtime segment"),
            original_segment
        );
        assert_eq!(
            fs::read(&metadata_path).expect("read preserved invalid metadata"),
            invalid_metadata_bytes
        );
        assert_eq!(
            fs::read(runtime_index_path(&session_dir)).expect("read preserved runtime index"),
            original_index
        );
    }
}

/// Verifies an active segment outside the metadata segment set is rejected before append.
/// 验证不属于元数据分段集合的活动分段会在追加前被拒绝。
#[test]
fn runtime_audit_append_rejects_unindexed_active_segment_before_write() {
    // root isolates the unindexed active-segment metadata fixture.
    // root 隔离未索引活动分段元数据夹具。
    let root = test_root("runtime_audit_append_rejects_unindexed_active_segment_before_write");
    // session_dir owns the valid runtime log whose active pointer is corrupted.
    // session_dir 持有活动指针将被破坏的有效运行时日志。
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        test_session_request(SegmentPolicy::default()),
    )
    .expect("create session");
    // state materializes the runtime log required by audit append semantics.
    // state 物化审计追加语义所要求的运行时日志。
    let state = conversation_state_from_session_messages(&[test_message(
        "m-user-1",
        SessionMessageKind::User,
        SessionMessageRecordKind::Transcript,
        "first prompt",
    )])
    .expect("valid replayed conversation identities");
    sync_runtime_log_from_conversation_state(&session_dir, &state, "t1").expect("sync runtime log");
    // invalid_metadata points active_segment away from its unique segment entry.
    // invalid_metadata 使 active_segment 偏离其唯一分段条目。
    let mut invalid_metadata = read_runtime_metadata(&session_dir).expect("read runtime metadata");
    // segment_path identifies the authoritative bytes that must remain unchanged.
    // segment_path 标识必须保持不变的权威字节。
    let segment_path = session_dir.join(&invalid_metadata.segments[0].file);
    // original_segment captures bytes before invalid metadata publication.
    // original_segment 捕获无效元数据发布前的字节。
    let original_segment = fs::read(&segment_path).expect("read runtime segment");
    // original_index captures the sidecar bytes before invalid metadata publication.
    // original_index 捕获无效元数据发布前的附属索引字节。
    let original_index = fs::read(runtime_index_path(&session_dir)).expect("read runtime index");
    invalid_metadata.active_segment = String::from("runtime/unindexed.jsonl");
    write_json_file_atomic(&runtime_metadata_path(&session_dir), &invalid_metadata)
        .expect("write invalid runtime metadata");
    // invalid_metadata_bytes captures the exact rejected active pointer document.
    // invalid_metadata_bytes 捕获被拒绝活动指针文档的精确字节。
    let invalid_metadata_bytes =
        fs::read(runtime_metadata_path(&session_dir)).expect("read invalid runtime metadata");

    // error proves the unindexed pointer is rejected before file creation or append.
    // error 证明未索引指针会在文件创建或追加前被拒绝。
    let error = record_runtime_audit_event(
        &session_dir,
        "t2",
        test_runtime_system_update_event("unindexed-active-segment", "must not append"),
    )
    .expect_err("unindexed active segment must fail audit append");

    assert!(error.contains("does not match the unique metadata segment"));
    assert_eq!(
        fs::read(&segment_path).expect("read preserved runtime segment"),
        original_segment
    );
    assert_eq!(
        fs::read(runtime_metadata_path(&session_dir)).expect("read preserved invalid metadata"),
        invalid_metadata_bytes
    );
    assert_eq!(
        fs::read(runtime_index_path(&session_dir)).expect("read preserved runtime index"),
        original_index
    );
    assert!(!session_dir.join("runtime/unindexed.jsonl").exists());
}

/// Verifies multi-segment metadata is rejected before cross-segment corruption can be repaired.
/// 验证多分段元数据会在跨分段损坏可能被修复前遭到拒绝。
#[test]
fn runtime_log_rejects_unimplemented_multi_segment_layout_without_rewriting() {
    // root isolates the unsupported cross-segment corruption fixture.
    // root 隔离不受支持的跨分段损坏夹具。
    let root = test_root("runtime_log_rejects_multi_segment_layout_without_rewriting");
    // session_dir owns an initially valid single-segment runtime log.
    // session_dir 持有初始有效的单分段运行时日志。
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        test_session_request(SegmentPolicy::default()),
    )
    .expect("create session");
    // state materializes the valid first runtime event.
    // state 物化第一条有效运行时事件。
    let state = conversation_state_from_session_messages(&[test_message(
        "m-user-1",
        SessionMessageKind::User,
        SessionMessageRecordKind::Transcript,
        "first prompt",
    )])
    .expect("valid replayed conversation identities");
    sync_runtime_log_from_conversation_state(&session_dir, &state, "t1").expect("sync runtime log");
    // metadata becomes an unsupported two-segment document written through the raw JSON helper.
    // metadata 将成为通过原始 JSON 辅助器写入的不受支持双分段文档。
    let mut metadata = read_runtime_metadata(&session_dir).expect("read runtime metadata");
    // first_segment_path receives a corrupt suffix before a later valid segment.
    // first_segment_path 会在后续有效分段之前接收损坏后缀。
    let first_segment_path = session_dir.join(&metadata.active_segment);
    // damaged_first_segment retains its valid first line followed by invalid JSON.
    // damaged_first_segment 保留有效首行，随后追加无效 JSON。
    let damaged_first_segment = format!(
        "{}{{ broken\n",
        fs::read_to_string(&first_segment_path).expect("read first runtime segment")
    );
    fs::write(&first_segment_path, &damaged_first_segment)
        .expect("write corrupt first segment suffix");
    // second_segment_path stores valid later evidence that must never be stitched over the gap.
    // second_segment_path 存储绝不能跨缺口拼接的后续有效证据。
    let second_segment_path = session_dir.join("runtime/r002.jsonl");
    // later_line is a valid sequence-two event placed after the corrupt first-segment suffix.
    // later_line 是放在首分段损坏后缀之后的有效序号二事件。
    let later_line = RuntimeLogEventLine::new(
        2,
        String::from("t2"),
        test_runtime_system_update_event("later-segment-event", "must remain untouched"),
    );
    write_json_lines_file_atomic(&second_segment_path, std::slice::from_ref(&later_line))
        .expect("write second runtime segment");
    // second_segment describes the unsupported later active segment.
    // second_segment 描述不受支持的后续活动分段。
    let mut second_segment =
        RuntimeSegmentMetadata::empty(String::from("runtime/r002.jsonl"), 2, String::from("t2"));
    second_segment.line_count = 1;
    second_segment.byte_len = fs::metadata(&second_segment_path)
        .expect("inspect second runtime segment")
        .len();
    second_segment.end_seq = Some(2);
    second_segment.updated_at = Some(String::from("t2"));
    metadata.segments[0].sealed = true;
    metadata.segments.push(second_segment);
    metadata.active_segment = String::from("runtime/r002.jsonl");
    metadata.next_seq = 3;
    metadata.updated_at = String::from("t2");
    write_json_file_atomic(&runtime_metadata_path(&session_dir), &metadata)
        .expect("write unsupported multi-segment metadata");
    // invalid_metadata_bytes captures the unsupported document before the rejected read.
    // invalid_metadata_bytes 捕获被拒绝读取前的不受支持文档。
    let invalid_metadata_bytes =
        fs::read(runtime_metadata_path(&session_dir)).expect("read multi-segment metadata");
    // original_index captures the sidecar that rejection must not rebuild.
    // original_index 捕获拒绝过程不得重建的附属索引。
    let original_index = fs::read(runtime_index_path(&session_dir)).expect("read runtime index");
    // original_second_segment captures later evidence before the rejected read.
    // original_second_segment 捕获被拒绝读取前的后续证据。
    let original_second_segment =
        fs::read_to_string(&second_segment_path).expect("read second runtime segment");

    // error proves the format is rejected before any tolerant repair action.
    // error 证明该格式会在任何容错修复动作前被拒绝。
    let error = read_runtime_log_lines(&session_dir)
        .expect_err("multi-segment runtime metadata must be rejected");
    // corrupt_backups proves rejection happened before repair publication.
    // corrupt_backups 证明拒绝发生在修复发布之前。
    let corrupt_backups = fs::read_dir(session_dir.join("runtime"))
        .expect("read runtime directory")
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().contains(".corrupt-"))
        .count();

    assert!(error.contains("must contain exactly one segment"));
    assert_eq!(
        fs::read_to_string(&first_segment_path).expect("read retained first segment"),
        damaged_first_segment
    );
    assert_eq!(
        fs::read_to_string(&second_segment_path).expect("read retained second segment"),
        original_second_segment
    );
    assert_eq!(
        fs::read(runtime_metadata_path(&session_dir))
            .expect("read retained multi-segment metadata"),
        invalid_metadata_bytes
    );
    assert_eq!(
        fs::read(runtime_index_path(&session_dir)).expect("read retained runtime index"),
        original_index
    );
    assert_eq!(corrupt_backups, 0);
}

/// Verifies every tail line is validated before an owned-lock append writes bytes.
/// 验证持锁追加在写入字节前会验证每一条尾部日志行。
#[test]
fn runtime_tail_append_rejects_internal_sequence_gap_before_write() {
    // root isolates the invalid append-tail fixture.
    // root 隔离无效追加尾部夹具。
    let root = test_root("runtime_tail_append_rejects_internal_sequence_gap_before_write");
    // session_dir owns the valid runtime snapshot used by the direct owned-lock helper test.
    // session_dir 持有直接测试持锁辅助器所用的有效运行时快照。
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        test_session_request(SegmentPolicy::default()),
    )
    .expect("create session");
    // state materializes one line before the invalid two-line tail is attempted.
    // state 在尝试无效双行尾部前物化一条日志行。
    let state = conversation_state_from_session_messages(&[test_message(
        "m-user-1",
        SessionMessageKind::User,
        SessionMessageRecordKind::Transcript,
        "first prompt",
    )])
    .expect("valid replayed conversation identities");
    sync_runtime_log_from_conversation_state(&session_dir, &state, "t1").expect("sync runtime log");
    // segment_path identifies the bytes protected by validation-before-write.
    // segment_path 标识由写前验证保护的字节。
    let segment_path = session_dir.join("runtime/r001.jsonl");
    // original_segment captures the exact bytes before acquiring the append transaction.
    // original_segment 捕获获取追加事务前的精确字节。
    let original_segment = fs::read(&segment_path).expect("read runtime segment");
    // original_metadata captures the authoritative document before the rejected append.
    // original_metadata 捕获被拒绝追加前的权威文档。
    let original_metadata =
        fs::read(runtime_metadata_path(&session_dir)).expect("read runtime metadata");
    // original_index captures the rebuildable sidecar before the rejected append.
    // original_index 捕获被拒绝追加前的可重建附属索引。
    let original_index = fs::read(runtime_index_path(&session_dir)).expect("read runtime index");
    // append_guard keeps snapshot validation and the rejected append in one lock lifetime.
    // append_guard 使快照验证与被拒绝追加保持在同一个锁生命周期内。
    let append_guard = segmented_append_file_io_lock()
        .lock()
        .expect("lock runtime append transaction");
    // snapshot is the validated state consumed by the append helper.
    // snapshot 是追加辅助器消费的已验证状态。
    let snapshot = read_runtime_log_snapshot_with_owned_lock(&session_dir, &append_guard)
        .expect("read runtime snapshot")
        .expect("runtime snapshot exists");
    // first_tail_seq is the exact sequence accepted for the first new row.
    // first_tail_seq 是第一条新记录所接受的精确序号。
    let first_tail_seq = snapshot.metadata.next_seq;
    // skipped_tail_seq deliberately omits the required second contiguous sequence.
    // skipped_tail_seq 有意跳过所要求的第二个连续序号。
    let skipped_tail_seq = first_tail_seq
        .checked_add(2)
        .expect("small test sequence should not overflow");
    // invalid_tail contains a valid first row followed by a sequence gap.
    // invalid_tail 包含一条有效首行，随后出现序号缺口。
    let invalid_tail = vec![
        RuntimeLogEventLine::new(
            first_tail_seq,
            String::from("t2"),
            test_runtime_system_update_event("tail-first", "first tail event"),
        ),
        RuntimeLogEventLine::new(
            skipped_tail_seq,
            String::from("t3"),
            test_runtime_system_update_event("tail-gap", "must not append"),
        ),
    ];
    // error proves the second row is checked before the first row can be appended.
    // error 证明第二条记录会在第一条可能追加前接受检查。
    let error = append_runtime_log_tail_with_owned_lock(
        &session_dir,
        snapshot,
        &invalid_tail,
        &append_guard,
    )
    .expect_err("internal tail sequence gap must fail append");
    drop(append_guard);

    assert!(error.contains("tail index 1"));
    assert_eq!(
        fs::read(&segment_path).expect("read preserved runtime segment"),
        original_segment
    );
    assert_eq!(
        fs::read(runtime_metadata_path(&session_dir)).expect("read preserved runtime metadata"),
        original_metadata
    );
    assert_eq!(
        fs::read(runtime_index_path(&session_dir)).expect("read preserved runtime index"),
        original_index
    );
}

/// Verifies runtime audit append rejects metadata that points at a missing active segment.
/// 验证运行时审计追加会拒绝指向缺失活动分段的元数据。
#[test]
fn runtime_audit_append_rejects_missing_active_segment() {
    let root = test_root("runtime_audit_append_rejects_missing_active_segment");
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        test_session_request(SegmentPolicy::default()),
    )
    .expect("create session");
    let messages = vec![test_message(
        "m-user-1",
        SessionMessageKind::User,
        SessionMessageRecordKind::Transcript,
        "first prompt",
    )];
    append_segmented_session_message_update(&session_dir, &messages).expect("append display row");
    let state = conversation_state_from_session_messages(&messages)
        .expect("valid replayed conversation identities");
    sync_runtime_log_from_conversation_state(&session_dir, &state, "t1").expect("sync runtime log");
    let metadata = read_runtime_metadata(&session_dir).expect("read runtime metadata");
    // active_segment_path is removed to verify metadata cannot silently recreate authoritative bytes.
    // active_segment_path 会被删除，用于验证元数据不能静默重建权威字节。
    let active_segment_path = session_dir.join(&metadata.active_segment);
    fs::remove_file(&active_segment_path).expect("remove active segment");

    // error captures the strict missing-segment diagnosis.
    // error 捕获严格的分段缺失诊断。
    let error = record_runtime_audit_event(
        &session_dir,
        "t2",
        RuntimeLogEvent::SystemUpdate(RuntimeSystemUpdateEvent {
            turn_id: Some(String::from("turn-a")),
            update_id: String::from("system-update-after-repair"),
            effective_after_event_id: None,
            text: String::from("repair test"),
            trust: RuntimeSystemUpdateTrust::TrustedHost,
            lowering_hint: RuntimeSystemUpdateLoweringHint::Auto,
            fingerprint: Some(String::from("system-update-after-repair-fp")),
            source: Some(String::from("test")),
        }),
    )
    .expect_err("missing active segment must fail audit append");
    // active_segment_absence distinguishes true deletion from filesystem inspection failure.
    // active_segment_absence 区分真正删除与文件系统检查失败。
    let active_segment_absence = fs::symlink_metadata(&active_segment_path);

    assert!(error.contains("Failed to open required runtime JSONL segment"));
    assert!(matches!(
        active_segment_absence,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound
    ));
}

/// Verifies runtime audit append reports fully damaged runtime logs without display fallback.
/// 验证运行时审计追加会上报 runtime 全损坏且不回退到展示记录。
#[test]
fn runtime_audit_append_reports_fully_damaged_runtime_log() {
    let root = test_root("runtime_audit_append_reports_fully_damaged_runtime_log");
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        test_session_request(SegmentPolicy::default()),
    )
    .expect("create session");
    let messages = vec![test_message(
        "m-user-1",
        SessionMessageKind::User,
        SessionMessageRecordKind::Transcript,
        "first prompt",
    )];
    let state = conversation_state_from_session_messages(&messages)
        .expect("valid replayed conversation identities");
    sync_runtime_log_from_conversation_state(&session_dir, &state, "t1").expect("sync runtime log");
    fs::write(session_dir.join("runtime").join("r001.jsonl"), "{ broken\n")
        .expect("damage runtime segment");

    // append_error captures the first strict full-damage diagnosis.
    // append_error 捕获第一次严格的全损坏诊断。
    let append_error = record_runtime_audit_event(
        &session_dir,
        "t2",
        RuntimeLogEvent::SystemUpdate(RuntimeSystemUpdateEvent {
            turn_id: Some(String::from("turn-a")),
            update_id: String::from("system-update-after-damage"),
            effective_after_event_id: None,
            text: String::from("repair test"),
            trust: RuntimeSystemUpdateTrust::TrustedHost,
            lowering_hint: RuntimeSystemUpdateLoweringHint::Auto,
            fingerprint: Some(String::from("system-update-after-damage-fp")),
            source: Some(String::from("test")),
        }),
    )
    .expect_err("fully damaged runtime log must fail audit append");
    // read_error proves the audit attempt did not erase the damaged source.
    // read_error 证明审计追加尝试没有擦除损坏源文件。
    let read_error = read_runtime_log_lines(&session_dir)
        .expect_err("fully damaged runtime log must fail every read");
    // persisted_segment retains the original bytes after both failed operations.
    // persisted_segment 在两次失败操作后保留原始字节。
    let persisted_segment = fs::read_to_string(session_dir.join("runtime").join("r001.jsonl"))
        .expect("read damaged runtime segment");

    assert!(append_error.contains("Runtime log is fully damaged"));
    assert!(read_error.contains("Runtime log is fully damaged"));
    assert_eq!(persisted_segment, "{ broken\n");
}

/// Verifies runtime-only audit events survive a later canonical conversation rebuild.
/// 验证仅审计 runtime 事件会在后续规范会话重建后继续保留。
#[test]
fn runtime_log_sync_preserves_audit_events_when_rebuilding() {
    let root = test_root("runtime_log_sync_preserves_audit_events_when_rebuilding");
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        test_session_request(SegmentPolicy::default()),
    )
    .expect("create session");
    let first_state = conversation_state_from_session_messages(&[test_message(
        "m-user-1",
        SessionMessageKind::User,
        SessionMessageRecordKind::Transcript,
        "first prompt",
    )])
    .expect("valid replayed conversation identities");
    sync_runtime_log_from_conversation_state(&session_dir, &first_state, "t1")
        .expect("sync first runtime log");
    backup_runtime_log_before_compaction(&session_dir, "compaction-a", "t2")
        .expect("record compaction backup");
    record_runtime_injection_applied(
        &session_dir,
        "t3",
        RuntimeInjectionAppliedEvent {
            turn_id: Some(String::from("turn-a")),
            combined_fingerprint: Some(String::from("combined-a")),
            persistent_fingerprint: Some(String::from("persistent-a")),
            transient_fingerprint: Some(String::from("transient-a")),
            has_transient: true,
            merged_baseline_fingerprint: None,
            source_node_ids: vec![String::from("persistent:0:persistent-a")],
        },
    )
    .expect("record injection audit");
    append_runtime_log_event_if_present(
        &session_dir,
        "t3",
        RuntimeLogEvent::SystemUpdate(RuntimeSystemUpdateEvent {
            turn_id: Some(String::from("turn-a")),
            update_id: String::from("system-update-a"),
            effective_after_event_id: None,
            text: String::from("Use concise answers."),
            trust: RuntimeSystemUpdateTrust::TrustedHost,
            lowering_hint: RuntimeSystemUpdateLoweringHint::Auto,
            fingerprint: Some(String::from("system-update-fp")),
            source: Some(String::from("test")),
        }),
    )
    .expect("record system update audit");
    append_runtime_log_event_if_present(
        &session_dir,
        "t3",
        RuntimeLogEvent::InjectionCancelled(RuntimeInjectionCancelledEvent {
            turn_id: Some(String::from("turn-a")),
            cancellation_id: String::from("cancel-a"),
            source_node_ids: vec![String::from("node-a")],
            previous_persistent_fingerprint: Some(String::from("persistent-a")),
            previous_transient_fingerprint: Some(String::from("transient-a")),
            reason: Some(String::from("test cancellation")),
        }),
    )
    .expect("record injection cancellation audit");
    append_runtime_log_event_if_present(
        &session_dir,
        "t3",
        RuntimeLogEvent::InjectionMerged(RuntimeInjectionMergedEvent {
            turn_id: Some(String::from("turn-a")),
            merge_id: String::from("merge-a"),
            compaction_id: Some(String::from("compaction-a")),
            previous_persistent_fingerprint: Some(String::from("persistent-a")),
            merged_baseline_fingerprint: Some(String::from("merged-a")),
            source_node_ids: vec![String::from("node-a")],
        }),
    )
    .expect("record injection merge audit");
    append_runtime_log_event_if_present(
        &session_dir,
        "t3",
        RuntimeLogEvent::ProviderRequestCheckpoint(RuntimeProviderRequestCheckpointEvent {
            turn_id: Some(String::from("turn-a")),
            checkpoint_id: String::from("request-a"),
            provider_id: String::from("anthropic"),
            model_id: String::from("claude"),
            provider_family: String::from("anthropic"),
            capability_fingerprint: Some(String::from("cap-a")),
            prompt_fingerprint: Some(String::from("prompt-a")),
            injection_fingerprint: Some(String::from("inject-a")),
            request_fingerprint: Some(String::from("request-fp-a")),
            cache_policy_fingerprint: Some(String::from("cache-a")),
            cache_policy_diagnostic: Some(String::from("cache_hints_unsupported")),
            converter_version: 1,
        }),
    )
    .expect("record provider request checkpoint");
    append_runtime_log_event_if_present(
        &session_dir,
        "t3",
        RuntimeLogEvent::ProviderResponseCheckpoint(RuntimeProviderResponseCheckpointEvent {
            turn_id: Some(String::from("turn-a")),
            checkpoint_id: String::from("response-a"),
            provider_id: String::from("anthropic"),
            model_id: String::from("claude"),
            provider_family: String::from("anthropic"),
            finish_reason: Some(String::from("stop")),
            native_state: Some(RuntimeProviderNativeStateEnvelope::new(
                "anthropic",
                "anthropic",
                "claude",
                json!({ "content": [] }),
            )),
            usage: None,
        }),
    )
    .expect("record provider response checkpoint");
    append_runtime_log_event_if_present(
        &session_dir,
        "t3",
        RuntimeLogEvent::ProviderLoweringRepair(RuntimeProviderLoweringRepairEvent {
            turn_id: Some(String::from("turn-a")),
            repair_id: String::from("lowering-repair-a"),
            provider_id: String::from("anthropic"),
            model_id: String::from("claude"),
            provider_family: String::from("anthropic"),
            action: String::from("fallback"),
            reason: String::from("unsupported native system update"),
            affected_items: 1,
        }),
    )
    .expect("record provider lowering repair");
    let rewritten_state = conversation_state_from_session_messages(&[
        test_message(
            "m-user-1",
            SessionMessageKind::User,
            SessionMessageRecordKind::Transcript,
            "rewritten prompt",
        ),
        test_message(
            "m-assistant-1",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::AssistantOutput,
            "rewritten answer",
        ),
    ])
    .expect("valid replayed conversation identities");

    sync_runtime_log_from_conversation_state(&session_dir, &rewritten_state, "t4")
        .expect("rebuild runtime log");
    let lines = read_runtime_log_lines(&session_dir)
        .expect("read runtime lines")
        .expect("runtime lines exist");
    let restored = read_runtime_conversation_state(&session_dir)
        .expect("read runtime conversation")
        .expect("runtime conversation exists");

    assert_eq!(restored.messages.len(), 2);
    assert!(
        lines
            .iter()
            .any(|line| { matches!(line.event, RuntimeLogEvent::ContextCompactionApplied(_)) })
    );
    assert!(
        lines
            .iter()
            .any(|line| { matches!(line.event, RuntimeLogEvent::InjectionApplied(_)) })
    );
    assert!(
        lines
            .iter()
            .any(|line| { matches!(line.event, RuntimeLogEvent::SystemUpdate(_)) })
    );
    assert!(
        lines
            .iter()
            .any(|line| { matches!(line.event, RuntimeLogEvent::InjectionCancelled(_)) })
    );
    assert!(
        lines
            .iter()
            .any(|line| { matches!(line.event, RuntimeLogEvent::InjectionMerged(_)) })
    );
    assert!(
        lines
            .iter()
            .any(|line| { matches!(line.event, RuntimeLogEvent::ProviderRequestCheckpoint(_)) })
    );
    assert!(
        lines
            .iter()
            .any(|line| { matches!(line.event, RuntimeLogEvent::ProviderResponseCheckpoint(_)) })
    );
    assert!(
        lines
            .iter()
            .any(|line| { matches!(line.event, RuntimeLogEvent::ProviderLoweringRepair(_)) })
    );
}

/// Verifies compacted runtime sync drops provider checkpoints that predate the compaction boundary.
/// 验证压缩后的运行时同步会丢弃早于压缩边界的 Provider 检查点。
#[test]
fn compacted_runtime_log_sync_prunes_pre_compaction_provider_checkpoints() {
    let root = test_root("compacted_runtime_log_sync_prunes_pre_compaction_provider_checkpoints");
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        test_session_request(SegmentPolicy::default()),
    )
    .expect("create session");
    let first_state = conversation_state_from_session_messages(&[test_message(
        "m-user-1",
        SessionMessageKind::User,
        SessionMessageRecordKind::Transcript,
        "first prompt",
    )])
    .expect("valid replayed conversation identities");
    sync_runtime_log_from_conversation_state(&session_dir, &first_state, "t1")
        .expect("sync first runtime log");
    append_runtime_log_event_if_present(
        &session_dir,
        "t2",
        RuntimeLogEvent::ProviderRequestCheckpoint(RuntimeProviderRequestCheckpointEvent {
            turn_id: Some(String::from("turn-before")),
            checkpoint_id: String::from("request-before"),
            provider_id: String::from("deepseek"),
            model_id: String::from("deepseek-v4-pro"),
            provider_family: String::from("deepseek"),
            capability_fingerprint: None,
            prompt_fingerprint: Some(String::from("prompt-before")),
            injection_fingerprint: None,
            request_fingerprint: Some(String::from("request-before-fp")),
            cache_policy_fingerprint: None,
            cache_policy_diagnostic: None,
            converter_version: 1,
        }),
    )
    .expect("record pre-compaction provider request checkpoint");
    backup_runtime_log_before_compaction(&session_dir, "compaction-a", "t3")
        .expect("record compaction backup");
    append_runtime_log_event_if_present(
        &session_dir,
        "t4",
        RuntimeLogEvent::ProviderRequestCheckpoint(RuntimeProviderRequestCheckpointEvent {
            turn_id: Some(String::from("turn-after")),
            checkpoint_id: String::from("request-after"),
            provider_id: String::from("deepseek"),
            model_id: String::from("deepseek-v4-pro"),
            provider_family: String::from("deepseek"),
            capability_fingerprint: None,
            prompt_fingerprint: Some(String::from("prompt-after")),
            injection_fingerprint: None,
            request_fingerprint: Some(String::from("request-after-fp")),
            cache_policy_fingerprint: None,
            cache_policy_diagnostic: None,
            converter_version: 1,
        }),
    )
    .expect("record post-compaction provider request checkpoint");
    let compacted_state = conversation_state_from_session_messages(&[test_message(
        "summary-message",
        SessionMessageKind::User,
        SessionMessageRecordKind::CompactionSummary,
        "summary prompt",
    )])
    .expect("valid replayed conversation identities");

    sync_runtime_log_from_compacted_conversation_state(
        &session_dir,
        &compacted_state,
        "t5",
        "compaction-a",
    )
    .expect("sync compacted runtime log");
    let lines = read_runtime_log_lines(&session_dir)
        .expect("read runtime lines")
        .expect("runtime lines exist");

    assert!(
        lines
            .iter()
            .any(|line| matches!(line.event, RuntimeLogEvent::ContextCompactionApplied(_)))
    );
    assert!(lines.iter().any(|line| {
        matches!(
            &line.event,
            RuntimeLogEvent::ProviderRequestCheckpoint(event)
                if event.checkpoint_id == "request-after"
        )
    }));
    assert!(!lines.iter().any(|line| {
        matches!(
            &line.event,
            RuntimeLogEvent::ProviderRequestCheckpoint(event)
                if event.checkpoint_id == "request-before"
        )
    }));
}
