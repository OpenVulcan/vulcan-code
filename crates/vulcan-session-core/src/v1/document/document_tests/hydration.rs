//! Strict event-stream reload and derived-state reconstruction regression tests.
//! 严格事件流重载及派生状态重建回归测试。

use super::*;

/// Rebuild rejects event revisions outside the JavaScript-safe monotonic range.
/// 重建拒绝超出 JavaScript 安全单调范围的事件修订号。
#[test]
fn rebuild_rejects_invalid_event_revision() {
    for invalid_revision in [0, JS_SAFE_INTEGER_MAX + 1] {
        let mut document = new_document();
        document
            .submit_root_input_fixture("revision", Vec::new(), T0)
            .expect("root input");
        document.events[0].event_revision = invalid_revision;

        assert!(matches!(
            document.rebuild_derived_state(),
            Err(DomainError::InvalidId {
                entity: "event_revision",
                ..
            })
        ));
    }
}

/// The rebuild derives indexes, active turn/API, and the allocator frontier
/// from the event stream alone (loader simulation).
/// rebuild 仅凭事件流派生索引、活动回合/API 与分配器 frontier(加载器模拟)。
#[test]
fn rebuild_derives_state_after_serde_round_trip() {
    let mut doc = new_document();
    doc.submit_root_input_fixture("hi", Vec::new(), T0).unwrap();
    let api = doc
        .start_api(
            ApiPurposeKind::MainGeneration,
            crate::v1::event::ApiRequestSnapshot::new("openai", "gpt-5"),
            0,
        )
        .unwrap();
    doc.append_reasoning(api, "think", AssistantSemanticStatus::Completed, T0)
        .unwrap();
    let tool = doc
        .start_tool(
            api,
            "shell",
            ToolPayload::try_new(serde_json::json!({"cmd": "ls"})).unwrap(),
            None,
            T0,
        )
        .unwrap();
    doc.finish_tool(
        api,
        tool,
        tool_completion(
            Some(ToolPayload::try_new(serde_json::json!({"ok": true})).unwrap()),
            ToolCallStatus::Completed,
            T0 + 1,
        ),
    )
    .unwrap();
    complete_ok(&mut doc, api, None, T0 + 2).unwrap();
    doc.close_turn(TurnOutcome::Completed, 3, T0 + 2).unwrap();
    let event_count = doc.events().len();

    let mut restored = SessionDocument {
        metadata: doc.metadata.clone(),
        runtime_lock: doc.runtime_lock.clone(),
        compactions: doc.compactions.clone(),
        events: ResidentEvents::new(doc.events().to_vec()),
        tool_seq_by_id: HashMap::new(),
        active_turn: None,
        active_api: None,
        id_allocator: IdAllocator::new(),
        memory_revision: 0,
        staged_commit_revision: None,
        durable_revision: 0,
        dirty_since: BTreeMap::new(),
        changed_event_seqs_by_revision: BTreeMap::new(),
        issued_revisions: BTreeSet::new(),
        published_compaction_id: None,
        pending_compaction_publish: BTreeMap::new(),
        segment_catalog: Vec::new(),
        resident_start_seq: 1,
    };
    restored.rebuild_derived_state().unwrap();
    assert_eq!(restored.events().len(), event_count);
    assert!(
        restored.active_turn().is_none(),
        "closed turn must not be active"
    );
    assert!(restored.active_api().is_none());
    assert_eq!(restored.id_frontier().next_seq, event_count as u64 + 1);
}

/// Rebuild rejects a non-contiguous stream.
/// rebuild 拒绝不连续的流。
#[test]
fn rebuild_rejects_sequence_gap() {
    let mut doc = new_document();
    doc.submit_root_input_fixture("a", Vec::new(), T0).unwrap();
    let mut events = doc.events().to_vec();
    let mut broken = events.remove(0);
    broken.seq = Seq::try_new(9).unwrap();
    events.insert(0, broken);
    let mut restored = SessionDocument {
        metadata: doc.metadata.clone(),
        runtime_lock: initialized_runtime_lock(),
        compactions: Vec::new(),
        events: ResidentEvents::new(events),
        tool_seq_by_id: HashMap::new(),
        active_turn: None,
        active_api: None,
        id_allocator: IdAllocator::new(),
        memory_revision: 0,
        staged_commit_revision: None,
        durable_revision: 0,
        dirty_since: BTreeMap::new(),
        changed_event_seqs_by_revision: BTreeMap::new(),
        issued_revisions: BTreeSet::new(),
        published_compaction_id: None,
        pending_compaction_publish: BTreeMap::new(),
        segment_catalog: Vec::new(),
        resident_start_seq: 1,
    };
    let result = restored.rebuild_derived_state();
    assert!(matches!(result, Err(DomainError::SequenceViolation { .. })));
}

/// An open API group with no stats survives the rebuild as the active API.
/// 没有统计的开放 API 组在 rebuild 后仍然是活动 API。
#[test]
fn rebuild_keeps_open_api_group_active() {
    let mut doc = new_document();
    doc.submit_root_input_fixture("open", Vec::new(), T0)
        .unwrap();
    let api = doc
        .start_api(
            ApiPurposeKind::MainGeneration,
            crate::v1::event::ApiRequestSnapshot::new("openai", "gpt-5"),
            0,
        )
        .unwrap();
    doc.append_reasoning(api, "partial", AssistantSemanticStatus::Streaming, T0)
        .unwrap();
    let mut restored = SessionDocument {
        metadata: doc.metadata.clone(),
        runtime_lock: doc.runtime_lock.clone(),
        compactions: doc.compactions.clone(),
        events: ResidentEvents::new(doc.events().to_vec()),
        tool_seq_by_id: HashMap::new(),
        active_turn: None,
        active_api: None,
        id_allocator: IdAllocator::new(),
        memory_revision: 0,
        staged_commit_revision: None,
        durable_revision: 0,
        dirty_since: BTreeMap::new(),
        changed_event_seqs_by_revision: BTreeMap::new(),
        issued_revisions: BTreeSet::new(),
        published_compaction_id: None,
        pending_compaction_publish: BTreeMap::new(),
        segment_catalog: Vec::new(),
        resident_start_seq: 1,
    };
    restored.rebuild_derived_state().unwrap();
    assert_eq!(
        restored.active_api().map(|state| state.api_call_id),
        Some(api)
    );
    assert_eq!(
        restored.active_turn().map(|turn| turn.origin),
        Some(TurnOrigin::UserRoot)
    );
}

/// Crash reload projects every persisted non-terminal fact to interrupted and
/// stays idempotent (plan 6.5.6).
/// 崩溃重载把每条已持久化的非终态事实原位投影为 interrupted，并且保持幂等
/// (计划 6.5.6)。
#[test]
fn crash_reload_projects_interruptions() {
    let mut doc = new_document();
    doc.submit_root_input_fixture("a", Vec::new(), T0).unwrap();
    let api = doc
        .start_api(
            ApiPurposeKind::MainGeneration,
            crate::v1::event::ApiRequestSnapshot::new("openai", "gpt-5"),
            0,
        )
        .unwrap();
    let reasoning_seq = doc
        .append_reasoning(api, "think", AssistantSemanticStatus::Streaming, T0)
        .unwrap()
        .into_value();
    let tool = doc
        .start_tool(
            api,
            "shell",
            ToolPayload::try_new(serde_json::json!({"cmd": "ls"})).unwrap(),
            None,
            T0,
        )
        .unwrap();
    let _ = doc.mark_tool_waiting_user(api, tool, T0);

    let projected = doc.project_persisted_interruptions().unwrap();
    assert_eq!(
        projected, 3,
        "one streaming reasoning, one waiting tool, and one statically closed API"
    );
    let reasoning = doc
        .events()
        .iter()
        .find(|e| e.seq() == reasoning_seq)
        .unwrap();
    match &reasoning.event {
        EventBody::AssistantReasoning(r) => {
            assert_eq!(r.status, AssistantSemanticStatus::Interrupted)
        }
        _ => unreachable!(),
    }
    let tool_event = doc
        .events()
        .iter()
        .find(|event| matches!(&event.event, EventBody::ToolCall(_)))
        .unwrap();
    match &tool_event.event {
        EventBody::ToolCall(t) => assert_eq!(t.status, ToolCallStatus::Interrupted),
        _ => unreachable!(),
    }
    // The crashed API is statically closed: a terminal UiOnly ApiStats with an
    // unknown usage lands last and `active_api` is cleared (plan 12.21.1.5).
    // 崩溃 API 被统计闭合：一条终态 UiOnly ApiStats(用量全未知)落在末尾，
    // `active_api` 被清空(计划 12.21.1.5)。
    let last_event = doc.events().last().unwrap();
    match &last_event.event {
        EventBody::ApiStats(stats) => {
            assert_eq!(stats.api_call_id.value(), api.value());
            assert_eq!(stats.status, ApiCallStatus::Interrupted);
            assert_eq!(stats.end_reason, Some(ApiEndReason::ProcessInterrupted));
            assert_eq!(stats.timing.completed_at_ms, Some(T0));
            assert_eq!(stats.provider, "openai");
            assert_eq!(stats.model, "gpt-5");
            assert!(stats.usage.input_tokens.is_none(), "usage stays unknown");
        }
        _ => panic!("the interrupted API must close with a terminal ApiStats"),
    }
    assert!(
        doc.active_api().is_none(),
        "the crashed API no longer occupies active_api"
    );
    assert!(doc.dirty_files().any(|file| file == DirtyFile::ActiveTail));
    let crash_reload = SessionDocument::from_parts(
        doc.metadata.clone(),
        doc.runtime_lock.clone(),
        doc.compactions.clone(),
        doc.segment_catalog().to_vec(),
        doc.events().to_vec(),
    )
    .expect("a process-interrupted API must survive strict reload");
    assert!(crash_reload.active_api().is_none());
    assert!(matches!(
        &crash_reload.events().last().expect("crash stats").event,
        EventBody::ApiStats(stats)
            if stats.status == ApiCallStatus::Interrupted
                && stats.end_reason == Some(ApiEndReason::ProcessInterrupted)
                && stats.error.is_none()
                && stats.timing.completed_at_ms == Some(T0)
    ));
    // Idempotent: nothing left to project, no further events appended.
    // 幂等：没有剩余可投影的内容，不再追加事件。
    let events_after_first = doc.events().len();
    assert_eq!(doc.project_persisted_interruptions().unwrap(), 0);
    assert_eq!(doc.events().len(), events_after_first);
    // The turn stays open, continue records the resume facts, and a fresh
    // physical API can start (a NEW api_call_id) to finish the turn.
    // 回合保持开放，继续记录恢复事实，且可启动全新物理 API(新 api_call_id)
    // 完成回合。
    assert!(doc.active_turn().is_some());
    doc.continue_turn_fixture(T0 + 1).unwrap();
    let resumed = doc
        .start_api(
            ApiPurposeKind::MainGeneration,
            crate::v1::event::ApiRequestSnapshot::new("openai", "gpt-5"),
            0,
        )
        .unwrap();
    assert_ne!(resumed.value(), api.value(), "the new API gets a fresh id");
    doc.append_assistant_text(resumed, "final", AssistantSemanticStatus::Completed, T0 + 2)
        .unwrap();
    complete_ok(&mut doc, resumed, None, T0 + 3).unwrap();
    doc.close_turn(TurnOutcome::Completed, 1, T0 + 4).unwrap();
    assert_eq!(
        doc.active_turn().map(|turn| turn.closed),
        Some(true),
        "the turn is closed; the resident slot persists for aggregation"
    );
    let interruption_stats = doc
        .events()
        .iter()
        .filter_map(|event| match &event.event {
            EventBody::ApiStats(stats) => Some(stats.api_call_id.value()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        interruption_stats,
        vec![api.value(), resumed.value()],
        "both the interrupted and the resumed API keep their permanent records"
    );
}

/// An API that crashes before producing any Provider semantic is recoverable
/// from its durable start fact and closes with the frozen request identity.
/// 在产生任何 Provider 语义前崩溃的 API 可从持久化起始事实恢复，并使用冻结
/// 请求身份闭合。
#[test]
fn zero_output_api_rebuilds_from_start_fact() {
    let mut doc = new_document();
    doc.submit_root_input_fixture("a", Vec::new(), T0).unwrap();
    let mut request = ApiRequestSnapshot::new("openai", "gpt-5");
    request.thinking_level = Some("high".to_string());
    let api = doc
        .start_api(ApiPurposeKind::MainGeneration, request.clone(), T0 + 1)
        .unwrap();
    assert!(matches!(
        &doc.events().last().unwrap().event,
        EventBody::ApiStarted(started)
            if started.api_call_id == api && started.request == request
    ));

    let mut restored = SessionDocument::from_parts(
        doc.metadata.clone(),
        initialized_runtime_lock(),
        Vec::new(),
        Vec::new(),
        doc.events().to_vec(),
    )
    .unwrap();
    let active = restored.active_api().unwrap();
    assert_eq!(active.api_call_id, api);
    assert_eq!(active.purpose, ApiPurposeKind::MainGeneration);
    assert_eq!(active.request, request);
    assert_eq!(restored.project_persisted_interruptions().unwrap(), 1);
    match &restored.events().last().unwrap().event {
        EventBody::ApiStats(stats) => {
            assert_eq!(stats.api_call_id, api);
            assert_eq!(stats.provider, "openai");
            assert_eq!(stats.model, "gpt-5");
            assert_eq!(stats.thinking_level.as_deref(), Some("high"));
            assert_eq!(stats.status, ApiCallStatus::Interrupted);
        }
        _ => panic!("zero-output interruption must append api_stats"),
    }
}

/// The real purpose of an open API survives a reload because the group's first
/// persisted event carries it; it must never default back to MainGeneration.
/// 开放 API 的真实 purpose 在重载后存活，因为组首个持久化事件携带它；绝不
/// 默认回退成 MainGeneration。
#[test]
fn open_api_purpose_survives_rebuild() {
    let mut doc = new_document();
    doc.submit_root_input_fixture("a", Vec::new(), T0).unwrap();
    let api = doc
        .start_api(
            ApiPurposeKind::Compaction,
            crate::v1::event::ApiRequestSnapshot::new("openai", "gpt-5"),
            0,
        )
        .unwrap();
    doc.append_assistant_text(api, "c", AssistantSemanticStatus::Completed, T0 + 1)
        .unwrap();
    let first_purpose = doc.events().iter().find_map(|event| match &event.event {
        EventBody::ApiStarted(started) if started.api_call_id == api => Some(started.purpose),
        _ => None,
    });
    assert_eq!(
        first_purpose,
        Some(ApiPurposeKind::Compaction),
        "the group's first persisted event must carry the purpose"
    );
    let mut restored = SessionDocument {
        metadata: doc.metadata.clone(),
        runtime_lock: initialized_runtime_lock(),
        compactions: Vec::new(),
        events: ResidentEvents::new(doc.events().to_vec()),
        tool_seq_by_id: HashMap::new(),
        active_turn: None,
        active_api: None,
        id_allocator: IdAllocator::new(),
        memory_revision: 0,
        staged_commit_revision: None,
        durable_revision: 0,
        dirty_since: BTreeMap::new(),
        changed_event_seqs_by_revision: BTreeMap::new(),
        issued_revisions: BTreeSet::new(),
        published_compaction_id: None,
        pending_compaction_publish: BTreeMap::new(),
        segment_catalog: Vec::new(),
        resident_start_seq: 1,
    };
    restored.rebuild_derived_state().unwrap();
    assert_eq!(
        restored
            .active_api()
            .expect("the open API must rebuild as active")
            .purpose,
        ApiPurposeKind::Compaction,
        "purpose must be recovered from the persisted fact, not defaulted"
    );
}

/// Rebuild rejects a stream with more than one open turn.
/// rebuild 拒绝存在多个开放回合的流。
#[test]
fn rebuild_rejects_multiple_open_turns() {
    let events = vec![
        raw_event(
            1,
            1,
            EventVisibility::Provider,
            EventBody::UserInput(UserInputEvent {
                parameters: None,
                input_id: PendingInputId::try_new(1).unwrap(),
                client_action_id: ClientActionId::try_new("multiple-open-turn-a").unwrap(),
                input_source: UserInputSource::Root,
                text: "a".to_string(),
                resource_ids: Vec::new(),
            }),
        ),
        raw_event(
            2,
            2,
            EventVisibility::Provider,
            EventBody::UserInput(UserInputEvent {
                parameters: None,
                input_id: PendingInputId::try_new(2).unwrap(),
                client_action_id: ClientActionId::try_new("multiple-open-turn-b").unwrap(),
                input_source: UserInputSource::Root,
                text: "b".to_string(),
                resource_ids: Vec::new(),
            }),
        ),
    ];
    let result = SessionDocument::from_parts(
        new_document().metadata,
        initialized_runtime_lock(),
        Vec::new(),
        Vec::new(),
        events,
    );
    assert!(matches!(result, Err(DomainError::MultipleOpenTurns)));
}

/// Rebuild rejects a turn whose statistics appear before its boundary, a
/// duplicated boundary, and any same-turn event after the terminal statistics.
/// rebuild 拒绝统计先于边界、重复边界、以及终态统计之后出现的同回合事件。
#[test]
fn rebuild_rejects_bad_turn_closure_sequences() {
    let user_input = EventBody::UserInput(UserInputEvent {
        parameters: None,
        input_id: PendingInputId::try_new(1).unwrap(),
        client_action_id: ClientActionId::try_new("turn-closure-root").unwrap(),
        input_source: UserInputSource::Root,
        text: "a".to_string(),
        resource_ids: Vec::new(),
    });
    let boundary = EventBody::TurnBoundary(TurnBoundaryEvent {
        reason: crate::v1::event::TurnClosureReason::Completed,
        outcome: TurnOutcome::Completed,
        purpose: crate::v1::event::TurnBoundaryPurpose::Business,
    });
    let stats = EventBody::TurnStats(TurnStatsEvent {
        start_seq: Seq::try_new(1).unwrap(),
        end_seq: Seq::try_new(4).unwrap(),
        outcome: TurnOutcome::Completed,
        duration_ms: None,
        buckets: TurnStatBuckets::default(),
    });
    // Statistics without a preceding boundary.
    // 统计没有前置边界。
    let stats_first = vec![
        raw_event(1, 1, EventVisibility::UiOnly, user_input.clone()),
        raw_event(2, 1, EventVisibility::UiOnly, stats.clone()),
    ];
    assert!(matches!(
        SessionDocument::from_parts(
            new_document().metadata,
            initialized_runtime_lock(),
            Vec::new(),
            Vec::new(),
            stats_first,
        ),
        Err(DomainError::TurnClosureMismatch { .. })
    ));
    // Two boundaries for one turn.
    // 一个回合两条边界。
    let duplicate_boundary = vec![
        raw_event(1, 1, EventVisibility::UiOnly, user_input.clone()),
        raw_event(2, 1, EventVisibility::UiOnly, boundary.clone()),
        raw_event(3, 1, EventVisibility::UiOnly, boundary.clone()),
    ];
    assert!(matches!(
        SessionDocument::from_parts(
            new_document().metadata,
            initialized_runtime_lock(),
            Vec::new(),
            Vec::new(),
            duplicate_boundary,
        ),
        Err(DomainError::TurnClosureMismatch { .. })
    ));
    // A same-turn event persists after the terminal statistics.
    // 终态统计之后仍然出现同回合事件。
    let event_after_stats = vec![
        raw_event(1, 1, EventVisibility::UiOnly, user_input.clone()),
        raw_event(2, 1, EventVisibility::UiOnly, boundary),
        raw_event(3, 1, EventVisibility::UiOnly, stats),
        raw_event(
            4,
            1,
            EventVisibility::UiOnly,
            EventBody::UserInput(UserInputEvent {
                parameters: None,
                input_id: PendingInputId::try_new(2).unwrap(),
                client_action_id: ClientActionId::try_new("turn-closure-after-stats").unwrap(),
                input_source: UserInputSource::PendingInTurn,
                text: "late".to_string(),
                resource_ids: Vec::new(),
            }),
        ),
    ];
    assert!(matches!(
        SessionDocument::from_parts(
            new_document().metadata,
            initialized_runtime_lock(),
            Vec::new(),
            Vec::new(),
            event_after_stats,
        ),
        Err(DomainError::TurnClosureMismatch { .. })
    ));
}

/// Rebuild rejects interleaved open API groups, groups spanning turns, and
/// group events after their terminal statistics.
/// rebuild 拒绝交错开放 API 组、横跨回合的组、以及终态统计后的组事件。
#[test]
fn rebuild_rejects_invalid_group_structures() {
    let new_doc = || SessionDocument::new("sess-1", "p", "t", T0).metadata;
    // Two open groups interleave: A, B, A again (B opens before A closes).
    // 两个开放组交错：A、B、再 A(B 在 A 闭合前打开)。
    let interleaved = vec![
        raw_event(
            1,
            1,
            EventVisibility::Provider,
            EventBody::UserInput(UserInputEvent {
                parameters: None,
                input_id: PendingInputId::try_new(1).unwrap(),
                client_action_id: ClientActionId::try_new("interleaved-root").unwrap(),
                input_source: UserInputSource::Root,
                text: "a".to_string(),
                resource_ids: Vec::new(),
            }),
        ),
        group_started(2, 1, 1),
        group_reasoning(3, 1, 1),
        group_started(4, 1, 2),
        group_reasoning(5, 1, 2),
        group_reasoning(6, 1, 1),
    ];
    assert!(matches!(
        SessionDocument::from_parts(
            new_doc(),
            initialized_runtime_lock(),
            Vec::new(),
            Vec::new(),
            interleaved
        ),
        Err(DomainError::GroupsInterleaved { .. })
    ));
    // One group spans two turns.
    // 一个组横跨两个回合。
    let spans_turns = vec![
        raw_event(
            1,
            1,
            EventVisibility::Provider,
            EventBody::UserInput(UserInputEvent {
                parameters: None,
                input_id: PendingInputId::try_new(1).unwrap(),
                client_action_id: ClientActionId::try_new("spans-turns-root-a").unwrap(),
                input_source: UserInputSource::Root,
                text: "a".to_string(),
                resource_ids: Vec::new(),
            }),
        ),
        group_started(2, 1, 1),
        group_reasoning(3, 1, 1),
        group_reasoning(4, 2, 1),
        raw_event(
            5,
            2,
            EventVisibility::Provider,
            EventBody::UserInput(UserInputEvent {
                parameters: None,
                input_id: PendingInputId::try_new(2).unwrap(),
                client_action_id: ClientActionId::try_new("spans-turns-root-b").unwrap(),
                input_source: UserInputSource::Root,
                text: "b".to_string(),
                resource_ids: Vec::new(),
            }),
        ),
    ];
    assert!(matches!(
        SessionDocument::from_parts(
            new_doc(),
            initialized_runtime_lock(),
            Vec::new(),
            Vec::new(),
            spans_turns
        ),
        Err(DomainError::GroupSpansTurns { .. })
    ));
    // A group event appears after its own terminal statistics.
    // 组统计之后再次出现该组事件。
    let reopened = vec![
        raw_event(
            1,
            1,
            EventVisibility::Provider,
            EventBody::UserInput(UserInputEvent {
                parameters: None,
                input_id: PendingInputId::try_new(1).unwrap(),
                client_action_id: ClientActionId::try_new("reopened-root").unwrap(),
                input_source: UserInputSource::Root,
                text: "a".to_string(),
                resource_ids: Vec::new(),
            }),
        ),
        group_started(2, 1, 1),
        group_reasoning(3, 1, 1),
        raw_event(
            4,
            1,
            EventVisibility::UiOnly,
            EventBody::ApiStats(ApiStatsEvent {
                api_call_id: ApiCallId::try_new(1).unwrap(),
                purpose: ApiPurposeKind::MainGeneration,
                provider: "openai".to_string(),
                profile: None,
                model: "gpt-5".to_string(),
                connection_id: None,
                thinking_level: None,
                temperature: None,
                usage: UsageSnapshot::default(),
                status: ApiCallStatus::Completed,
                end_reason: Some(ApiEndReason::CompletedSuccessfully),
                error: None,
                continuation_eligibility: ContinuationEligibility::Forbidden,
                timing: ApiTimings::default(),
            }),
        ),
        group_reasoning(5, 1, 1),
    ];
    let result = SessionDocument::from_parts(
        new_doc(),
        initialized_runtime_lock(),
        Vec::new(),
        Vec::new(),
        reopened,
    );
    assert!(
        matches!(
            result,
            Err(DomainError::ClosedGroupReopened { .. })
                | Err(DomainError::SequenceViolation { .. })
        ),
        "group events after their stats must be rejected"
    );
}

/// Loader validation rejects stale open-turn metadata and a runtime pointer
/// that does not name any persisted compaction node.
/// 加载校验拒绝过期的开放回合 metadata，以及未指向任何持久化压缩节点的 runtime 指针。
#[test]
fn loader_rejects_stale_metadata_and_unknown_compaction_pointer() -> Result<(), DomainError> {
    let mut doc = new_document();
    doc.submit_root_input_fixture("open", Vec::new(), T0)?;
    let mut stale_metadata = doc.metadata.clone();
    stale_metadata.open_turn_start_seq = None;
    assert!(matches!(
        SessionDocument::from_parts(
            stale_metadata,
            initialized_runtime_lock(),
            Vec::new(),
            Vec::new(),
            doc.events().to_vec(),
        ),
        Err(DomainError::ImmutableFieldMutation {
            entity: "metadata",
            field: "open_turn_start_seq",
        })
    ));

    assert!(matches!(
        SessionDocument::from_parts(
            doc.metadata.clone(),
            RuntimeLock {
                initialization_state: RuntimeInitializationState::Initialized,
                current_compaction_id: Some(CompactionId::try_new(99)?),
                locked_sources: Vec::new(),
            },
            Vec::new(),
            Vec::new(),
            doc.events().to_vec(),
        ),
        Err(DomainError::ReferenceViolation {
            kind: "runtime_compaction_id",
            value: 99,
        })
    ));
    Ok(())
}

/// Strict reload rejects malformed request-budget evidence from a persisted api_started event.
/// 严格重载会拒绝持久化 api_started 事件中损坏的请求预算证据。
#[test]
fn strict_reload_rejects_malformed_provider_request_budget_evidence() -> Result<(), DomainError> {
    let mut doc = new_document();
    doc.submit_root_input_fixture("budget evidence", Vec::new(), T0)?;
    doc.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 1,
    )?;
    let mut events = doc.events().to_vec();
    let started = events
        .iter_mut()
        .find_map(|event| match &mut event.event {
            EventBody::ApiStarted(started) => Some(started),
            _ => None,
        })
        .expect("fixture must contain api_started");
    started.request_budget_evidence.request_assembly_fingerprint = String::from("invalid");

    assert!(matches!(
        SessionDocument::from_parts(
            doc.metadata.clone(),
            doc.runtime_lock.clone(),
            doc.compactions.clone(),
            doc.segment_catalog().to_vec(),
            events,
        ),
        Err(DomainError::InvalidProviderBudgetEvidence {
            field: "request_assembly_fingerprint"
        })
    ));
    Ok(())
}

/// Fresh runtime loader validation rejects every initialized-only field without repair.
/// Fresh runtime 加载校验拒绝全部仅初始化后合法的字段，且绝不修复。
#[test]
fn loader_rejects_contradictory_fresh_runtime_facts() -> Result<(), DomainError> {
    let metadata = SessionDocument::new("fresh", "project", "title", T0).metadata;
    let locked = RuntimeLock {
        locked_sources: vec![LockedSource {
            node_id: "agents-project".to_string(),
            order: 0,
            target: LockedPromptTarget::Developer,
            kind: LockedSourceKind::AgentsMd,
            source_label: Some("AGENTS.md".to_string()),
            source_id: None,
            content: "rules".to_string(),
        }],
        ..RuntimeLock::default()
    };
    assert!(matches!(
        SessionDocument::from_parts(metadata.clone(), locked, Vec::new(), Vec::new(), Vec::new(),),
        Err(DomainError::InvalidRuntimeInitializationState {
            field: "locked_sources"
        })
    ));
    let pointed = RuntimeLock {
        current_compaction_id: Some(CompactionId::try_new(1)?),
        ..RuntimeLock::default()
    };
    assert!(matches!(
        SessionDocument::from_parts(metadata, pointed, Vec::new(), Vec::new(), Vec::new()),
        Err(DomainError::InvalidRuntimeInitializationState {
            field: "current_compaction_id"
        })
    ));
    Ok(())
}
