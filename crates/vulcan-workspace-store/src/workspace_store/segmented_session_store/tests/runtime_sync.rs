use super::*;

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
    let conversation_state = conversation_state_from_session_messages(&messages);
    rebuild_runtime_log_from_conversation_state(
        &session_dir,
        &conversation_state,
        "2026-06-09T10:00:00Z",
    )
    .expect("rebuild runtime log");

    for index in 0..6 {
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
    let first_state = conversation_state_from_session_messages(&first_messages);
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
    let second_state = conversation_state_from_session_messages(&second_messages);

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
    )]);
    sync_runtime_log_from_conversation_state(&session_dir, &first_state, "t1")
        .expect("sync first runtime log");
    let rewritten_state = conversation_state_from_session_messages(&[test_message(
        "m-user-1",
        SessionMessageKind::User,
        SessionMessageRecordKind::Transcript,
        "rewritten prompt",
    )]);

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

/// Verifies runtime audit append skips cleanly when metadata points at a missing active segment.
/// 验证运行时审计追加在元数据指向缺失活动分段时会干净跳过。
#[test]
fn runtime_audit_append_skips_missing_active_segment() {
    let root = test_root("runtime_audit_append_skips_missing_active_segment");
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
    append_segmented_session_message_tail(&session_dir, &messages).expect("append display row");
    let state = conversation_state_from_session_messages(&messages);
    sync_runtime_log_from_conversation_state(&session_dir, &state, "t1").expect("sync runtime log");
    let metadata = read_runtime_metadata(&session_dir).expect("read runtime metadata");
    fs::remove_file(session_dir.join(&metadata.active_segment)).expect("remove active segment");

    let appended = record_runtime_audit_event(
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
    .expect("append audit after active segment loss");

    assert!(!appended);
    assert!(!session_dir.join(&metadata.active_segment).exists());
}

/// Verifies runtime audit append skips fully damaged runtime logs without display fallback.
/// 验证运行时审计追加在 runtime 全损坏时会跳过且不回退到展示记录。
#[test]
fn runtime_audit_append_skips_fully_damaged_runtime_log() {
    let root = test_root("runtime_audit_append_skips_fully_damaged_runtime_log");
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
    let state = conversation_state_from_session_messages(&messages);
    sync_runtime_log_from_conversation_state(&session_dir, &state, "t1").expect("sync runtime log");
    fs::write(session_dir.join("runtime").join("r001.jsonl"), "{ broken\n")
        .expect("damage runtime segment");

    let appended = record_runtime_audit_event(
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
    .expect("append audit after full damage");
    let lines = read_runtime_log_lines(&session_dir)
        .expect("read runtime lines")
        .expect("runtime lines exist");

    assert!(!appended);
    assert!(lines.is_empty());
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
    )]);
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
    ]);

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
    )]);
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
    )]);

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
