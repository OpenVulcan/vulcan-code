//! Physical API admission, streaming semantics, transport observations, and terminal regression tests.
//! 物理 API 准入、流式语义、传输观察及终态回归测试。

use super::*;

/// Explicit API interruption closes only the physical API, interrupts every
/// non-terminal tool, and lets Continue inherit the same logical turn.
/// 显式 API 中断只闭合物理 API、中断每个非终态工具，并让 Continue 继承同一
/// 逻辑回合。
#[test]
fn explicit_interrupt_finishes_waiting_ask_and_preserves_open_turn() -> Result<(), DomainError> {
    /// Returns the current revision of one resident event.
    /// 返回一条常驻事件的当前修订号。
    fn revision_at(document: &SessionDocument, seq: Seq) -> u64 {
        document
            .events()
            .iter()
            .find(|event| event.seq() == seq)
            .map(|event| event.event_revision)
            .expect("event revision")
    }

    let mut doc = new_document();
    let turn_id = doc.submit_root_input_fixture("first", Vec::new(), T0)?;
    let api = doc.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0,
    )?;
    doc.append_reasoning(api, "think", AssistantSemanticStatus::Streaming, T0)?;
    doc.append_assistant_text(api, "partial", AssistantSemanticStatus::Streaming, T0)?;
    let prepared_tool = doc
        .prepare_tool_batch(
            api,
            vec![ToolPreparation {
                request_stream_id: RequestStreamId::try_new("stream-file").unwrap(),
                tool_name: String::from("file"),
                input: ToolPayload::try_new(serde_json::json!({"path": "a.txt"}))?,
                provider_tool_call_id: Some(String::from("provider-prepared")),
            }],
            T0,
        )?
        .into_value()
        .tools[0]
        .tool_call_id;
    let running_tool = doc.start_tool(
        api,
        "shell",
        ToolPayload::try_new(serde_json::json!({"cmd": "ls"}))?,
        None,
        T0,
    )?;
    let waiting_ask = doc.start_tool(
        api,
        "ask",
        ToolPayload::try_new(serde_json::json!({"question": "continue?"}))?,
        None,
        T0,
    )?;
    doc.mark_tool_waiting_user(api, waiting_ask, T0)?;

    let reasoning_seq = doc
        .events()
        .iter()
        .find(|event| matches!(event.event, EventBody::AssistantReasoning(_)))
        .map(ConversationEvent::seq)
        .expect("reasoning seq");
    let text_seq = doc
        .events()
        .iter()
        .find(|event| matches!(event.event, EventBody::AssistantText(_)))
        .map(ConversationEvent::seq)
        .expect("assistant text seq");
    let prepared_seq = doc.tool_seq_by_id[&prepared_tool];
    let running_seq = doc.tool_seq_by_id[&running_tool];
    let waiting_seq = doc.tool_seq_by_id[&waiting_ask];

    let stats_seq = doc.interrupt_active_api(T0 + 1)?;
    assert!(doc.active_api().is_none());
    assert_eq!(
        doc.active_turn().map(|turn| (turn.turn_id, turn.closed)),
        Some((turn_id, false))
    );
    assert!(doc.events().iter().all(|event| match &event.event {
        EventBody::AssistantReasoning(reasoning) => {
            reasoning.status == AssistantSemanticStatus::Interrupted
        }
        EventBody::AssistantText(text) => {
            text.status == AssistantSemanticStatus::Interrupted
        }
        _ => true,
    }));
    let tool_status = |tool_call_id| {
        doc.events().iter().find_map(|event| match &event.event {
            EventBody::ToolCall(tool) if tool.tool_call_id == tool_call_id => {
                Some((tool.status, tool.finished_at_ms))
            }
            _ => None,
        })
    };
    assert_eq!(
        tool_status(prepared_tool),
        Some((ToolCallStatus::Interrupted, Some(T0 + 1)))
    );
    assert_eq!(
        tool_status(running_tool),
        Some((ToolCallStatus::Interrupted, Some(T0 + 1)))
    );
    assert_eq!(
        tool_status(waiting_ask),
        Some((ToolCallStatus::Interrupted, Some(T0 + 1)))
    );
    assert_eq!(revision_at(&doc, reasoning_seq), 2);
    assert_eq!(revision_at(&doc, text_seq), 2);
    assert_eq!(revision_at(&doc, prepared_seq), 2);
    assert_eq!(revision_at(&doc, running_seq), 3);
    assert_eq!(revision_at(&doc, waiting_seq), 4);
    let Some(interruption_control) = doc
        .events()
        .iter()
        .find(|event| event.seq().value() + 1 == stats_seq.value())
    else {
        panic!("the execution-control fact must precede api_stats");
    };
    assert!(matches!(
        &interruption_control.event,
        EventBody::ExecutionControl(control)
            if control.control_kind == ExecutionControlKind::ExecutionInterrupted
                && control.api_call_id == Some(api)
    ));
    let Some(stats) = doc.events().iter().find(|event| event.seq() == stats_seq) else {
        panic!("the interruption must append terminal api_stats");
    };
    assert!(matches!(
        &stats.event,
        EventBody::ApiStats(stats)
            if stats.status == ApiCallStatus::Interrupted
                && stats.end_reason == Some(ApiEndReason::UserInterrupted)
                && stats.error.is_none()
                && stats.timing.completed_at_ms == Some(T0 + 1)
    ));
    let user_stopped_reload = SessionDocument::from_parts(
        doc.metadata.clone(),
        doc.runtime_lock.clone(),
        doc.compactions.clone(),
        doc.segment_catalog().to_vec(),
        doc.events().to_vec(),
    )?;
    assert!(user_stopped_reload.active_api().is_none());

    let event_count = doc.events().len();
    assert!(matches!(
        doc.interrupt_active_api(T0 + 2),
        Err(DomainError::NoActiveApi)
    ));
    assert_eq!(doc.events().len(), event_count);
    for seq in [
        reasoning_seq,
        text_seq,
        prepared_seq,
        running_seq,
        waiting_seq,
    ] {
        let revision_before_noop = revision_at(&doc, seq);
        let before_noop = doc.clone();
        let mut staged_event_updates = BTreeMap::new();
        assert!(!doc.stage_active_event_interruption(
            &mut staged_event_updates,
            seq,
            api,
            T0 + 2,
        )?);
        assert!(staged_event_updates.is_empty());
        assert_eq!(doc, before_noop);
        assert_eq!(revision_at(&doc, seq), revision_before_noop);
    }
    doc.continue_turn_fixture(T0 + 3)?;
    assert!(doc.events().iter().rev().take(2).all(|event| {
        event.turn_id == Some(turn_id) && matches!(event.event, EventBody::ExecutionControl(_))
    }));
    Ok(())
}

/// An interruption atomically promotes pending supplements after ApiStats and keeps the Turn open.
/// 中断会在 ApiStats 之后原子晋升待处理补充，并保持逻辑回合开放。
#[test]
fn interruption_atomically_promotes_pending_in_turn_inputs() -> Result<(), DomainError> {
    let mut document = new_document();
    let turn_id = document.submit_root_input_fixture("root", Vec::new(), T0)?;
    let api_call_id = document.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0,
    )?;
    let pending_input_id = document
        .add_pending_in_turn_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("interrupt-pending-input")?,
                "supplement",
                Vec::new(),
            ),
            T0 + 1,
        )?
        .into_value();
    let revision_before_interruption = document.memory_revision();

    let stats_seq = document.interrupt_active_api(T0 + 2)?;

    assert_eq!(document.memory_revision(), revision_before_interruption + 1);
    assert!(document.active_api().is_none());
    assert_eq!(
        document
            .active_turn()
            .map(|turn| (turn.turn_id, turn.closed)),
        Some((turn_id, false))
    );
    assert!(
        document
            .metadata
            .input_state
            .pending_in_turn_inputs
            .is_empty()
    );
    let promoted_event = document
        .events()
        .iter()
        .find(|event| event.seq.value() == stats_seq.value() + 1)
        .expect("promoted input immediately after interrupted ApiStats");
    assert_eq!(promoted_event.turn_id, Some(turn_id));
    assert!(matches!(
        &promoted_event.event,
        EventBody::UserInput(input)
            if input.input_id == pending_input_id
                && input.input_source == UserInputSource::PendingInTurn
                && input.text == "supplement"
    ));
    let stats_event = document
        .events()
        .iter()
        .find(|event| event.seq == stats_seq)
        .expect("interrupted ApiStats");
    assert!(matches!(
        &stats_event.event,
        EventBody::ApiStats(stats)
            if stats.api_call_id == api_call_id
                && stats.status == ApiCallStatus::Interrupted
                && stats.end_reason == Some(ApiEndReason::UserInterrupted)
    ));
    let restored = SessionDocument::from_parts(
        document.metadata.clone(),
        document.runtime_lock.clone(),
        document.compactions.clone(),
        document.segment_catalog().to_vec(),
        document.events().to_vec(),
    )?;
    assert_eq!(restored.events(), document.events());
    assert!(
        restored
            .metadata
            .input_state
            .pending_in_turn_inputs
            .is_empty()
    );
    Ok(())
}

/// API completion atomically promotes pending in-turn inputs and deduplicates by input id.
/// API 完成会原子晋升过程内补充，并按 input id 去重。
#[test]
fn pending_inputs_promote_after_group_close() {
    let mut doc = new_document();
    doc.submit_root_input_fixture("a", Vec::new(), T0).unwrap();
    doc.add_pending_in_turn_input_fixture("mid", Vec::new(), T0 + 1)
        .unwrap();
    let api = doc
        .start_api(
            ApiPurposeKind::MainGeneration,
            crate::v1::event::ApiRequestSnapshot::new("openai", "gpt-5"),
            0,
        )
        .unwrap();
    assert_eq!(doc.metadata.input_state.pending_in_turn_inputs.len(), 1);
    doc.append_assistant_text(api, "a1", AssistantSemanticStatus::Completed, T0 + 2)
        .unwrap();
    let completion = doc
        .complete_api_group(
            api,
            ApiCompletion {
                request: ApiRequestSnapshot::new("openai", "gpt-5"),
                usage: usage(10, 20),
                status: ApiCallStatus::Completed,
                end_reason: Some(ApiEndReason::CompletedSuccessfully),
                error: None,
                continuation_eligibility: ContinuationEligibility::Forbidden,
                timing: ApiTimings::default(),
                provider_source: None,
                completed_at_ms: T0 + 3,
            },
        )
        .unwrap();
    assert_eq!(completion.promoted_input_seqs.len(), 1);
    assert!(doc.metadata.input_state.pending_in_turn_inputs.is_empty());
}

/// A successful API terminal cannot overtake Prepared, Running, or WaitingUser tools.
/// 成功 API 终态不能越过 Prepared、Running 或 WaitingUser 工具。
#[test]
fn completed_api_rejects_every_non_terminal_tool_state() -> Result<(), DomainError> {
    let mut doc = new_document();
    doc.submit_root_input_fixture("run tool", Vec::new(), T0)?;
    let api = doc.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 1,
    )?;
    let tool = doc.prepare_tool(
        api,
        RequestStreamId::try_new("stream-provider-tool-1")?,
        "ask",
        ToolPayload::try_new(serde_json::json!({"question": "continue?"}))?,
        Some(String::from("provider-tool-1")),
        T0 + 2,
    )?;

    for expected_status in [
        ToolCallStatus::Prepared,
        ToolCallStatus::Running,
        ToolCallStatus::WaitingUser,
    ] {
        let event_count = doc.events().len();
        let revision = doc.memory_revision();
        assert!(matches!(
            complete_ok(&mut doc, api, None, T0 + 3),
            Err(DomainError::ToolStatusMismatch {
                actual,
                expected: "terminal",
                ..
            }) if actual == tool_status_name(expected_status)
        ));
        assert_eq!(doc.events().len(), event_count);
        assert_eq!(doc.memory_revision(), revision);
        assert!(doc.active_api().is_some());
        if expected_status == ToolCallStatus::Prepared {
            doc.start_prepared_tool(api, tool, T0 + 4)?;
        } else if expected_status == ToolCallStatus::Running {
            doc.mark_tool_waiting_user(api, tool, T0 + 5)?;
        }
    }

    doc.cancel_waiting_tool(api, tool, None, T0 + 6)?;
    complete_ok(&mut doc, api, None, T0 + 7)?;
    assert!(doc.active_api().is_none());
    assert_eq!(
        doc.events()
            .iter()
            .filter(|event| matches!(event.event, EventBody::ApiStats(_)))
            .count(),
        1
    );
    Ok(())
}

/// A failed API explicitly interrupts every non-terminal tool before publishing stats.
/// 失败 API 会在发布统计前显式中断每个非终态工具。
#[test]
fn failed_api_interrupts_tools_before_terminal_stats() -> Result<(), DomainError> {
    let mut doc = new_document();
    doc.submit_root_input_fixture("run tools", Vec::new(), T0)?;
    let api = doc.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 1,
    )?;
    let prepared = doc.prepare_tool(
        api,
        RequestStreamId::try_new("stream-provider-tool-1")?,
        "file",
        ToolPayload::try_new(serde_json::json!({"path": "a"}))?,
        Some(String::from("provider-tool-1")),
        T0 + 2,
    )?;
    let waiting = doc.prepare_tool(
        api,
        RequestStreamId::try_new("stream-provider-tool-2")?,
        "ask",
        ToolPayload::try_new(serde_json::json!({"question": "continue?"}))?,
        Some(String::from("provider-tool-2")),
        T0 + 3,
    )?;
    doc.start_prepared_tool(api, waiting, T0 + 4)?;
    doc.mark_tool_waiting_user(api, waiting, T0 + 5)?;

    let stats_seq = doc
        .complete_api_group(
            api,
            ApiCompletion {
                request: ApiRequestSnapshot::new("openai", "gpt-5"),
                usage: UsageSnapshot::default(),
                status: ApiCallStatus::Failed,
                end_reason: Some(ApiEndReason::ProviderError),
                error: Some(terminal_error(ApiEndReason::ProviderError)),
                continuation_eligibility: ContinuationEligibility::Forbidden,
                timing: ApiTimings::default(),
                provider_source: None,
                completed_at_ms: T0 + 6,
            },
        )?
        .stats_seq;

    for tool_call_id in [prepared, waiting] {
        assert!(matches!(
            doc.events().iter().find_map(|event| match &event.event {
                EventBody::ToolCall(tool) if tool.tool_call_id == tool_call_id => Some(tool),
                _ => None,
            }),
            Some(ToolCallEvent {
                status: ToolCallStatus::Interrupted,
                finished_at_ms: Some(value),
                ..
            }) if *value == T0 + 6
        ));
    }
    assert!(doc.active_api().is_none());
    assert!(matches!(
        doc.events().iter().find(|event| event.seq() == stats_seq),
        Some(ConversationEvent {
            event: EventBody::ApiStats(ApiStatsEvent {
                status: ApiCallStatus::Failed,
                ..
            }),
            ..
        })
    ));
    Ok(())
}

/// Reasoning written after text is rejected at write time.
/// 在正文之后写 reasoning 会被写时拒绝。
#[test]
fn reasoning_after_text_is_rejected() {
    let mut doc = new_document();
    doc.submit_root_input_fixture("a", Vec::new(), T0).unwrap();
    let api = doc
        .start_api(
            ApiPurposeKind::MainGeneration,
            crate::v1::event::ApiRequestSnapshot::new("openai", "gpt-5"),
            0,
        )
        .unwrap();
    doc.append_assistant_text(api, "t", AssistantSemanticStatus::Completed, T0)
        .unwrap();
    let result = doc.append_reasoning(api, "r", AssistantSemanticStatus::Streaming, T0);
    assert!(matches!(result, Err(DomainError::SequenceViolation { .. })));
}

/// Verifies that cumulative Provider snapshots distinguish exact replay from real evolution.
/// 验证累计 Provider 快照能够区分完全重放与真实演进。
#[test]
fn provider_semantic_snapshots_are_monotonic_and_idempotent() -> Result<(), DomainError> {
    // Build one open physical API that can receive cumulative reasoning snapshots.
    // 构建一个可接收累计 reasoning 快照的开放物理 API。
    let mut doc = new_document();
    doc.submit_root_input_fixture("root", Vec::new(), T0)?;
    let api = doc.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 1,
    )?;

    // Persist the first snapshot and retain its stable seq and revisions.
    // 持久化首份快照，并保留其稳定 seq 与修订号。
    let first = doc.append_reasoning(api, "think", AssistantSemanticStatus::Streaming, T0 + 2)?;
    assert!(first.changed());
    let reasoning_seq = first.into_value();
    let first_document_revision = doc.memory_revision();
    let first_event_revision = doc
        .events()
        .iter()
        .find(|event| event.seq() == reasoning_seq)
        .expect("reasoning event")
        .event_revision;

    // Replay the exact full snapshot without creating a false revision.
    // 完全重放同一完整快照，不得制造伪修订。
    let replay = doc.append_reasoning(api, "think", AssistantSemanticStatus::Streaming, T0 + 3)?;
    assert!(!replay.changed());
    assert_eq!(replay.into_value(), reasoning_seq);
    assert_eq!(doc.memory_revision(), first_document_revision);
    assert_eq!(
        doc.events()
            .iter()
            .find(|event| event.seq() == reasoning_seq)
            .expect("reasoning event")
            .event_revision,
        first_event_revision
    );

    // Grow the cumulative text once and require exactly one new revision.
    // 累计正文增长一次时必须精确产生一个新修订。
    let growth = doc.append_reasoning(
        api,
        "think more",
        AssistantSemanticStatus::Streaming,
        T0 + 4,
    )?;
    assert!(growth.changed());
    assert_eq!(growth.into_value(), reasoning_seq);
    assert_eq!(doc.memory_revision(), first_document_revision + 1);

    // Reject a regressed cumulative snapshot without mutating any document fact.
    // 拒绝回退的累计快照，且不得修改任何文档事实。
    let before_regression = doc.clone();
    assert!(matches!(
        doc.append_reasoning(api, "think", AssistantSemanticStatus::Streaming, T0 + 5,),
        Err(DomainError::ImmutableFieldMutation {
            entity: "assistant_semantic",
            field: "cumulative_text",
        })
    ));
    assert_eq!(doc, before_regression);

    // Seal the semantic once, then accept only an exact terminal replay.
    // 语义只封口一次，之后仅接受完全相同的终态重放。
    let terminal = doc.append_reasoning(
        api,
        "think more",
        AssistantSemanticStatus::Completed,
        T0 + 6,
    )?;
    assert!(terminal.changed());
    let terminal_revision = doc.memory_revision();
    let terminal_replay = doc.append_reasoning(
        api,
        "think more",
        AssistantSemanticStatus::Completed,
        T0 + 7,
    )?;
    assert!(!terminal_replay.changed());
    assert_eq!(doc.memory_revision(), terminal_revision);
    let before_terminal_rewrite = doc.clone();
    assert!(matches!(
        doc.append_reasoning(
            api,
            "think more again",
            AssistantSemanticStatus::Completed,
            T0 + 8,
        ),
        Err(DomainError::ImmutableFieldMutation {
            entity: "assistant_semantic",
            field: "terminal_snapshot",
        })
    ));
    assert_eq!(doc, before_terminal_rewrite);
    Ok(())
}

/// A live process exit records its exact runtime timestamp and remains idempotent.
/// 实时进程退出记录精确的运行时边界时间戳，并保持幂等。
#[test]
fn live_process_interruption_uses_explicit_runtime_boundary() {
    let mut doc = new_document();
    doc.submit_root_input_fixture("a", Vec::new(), T0).unwrap();
    let api = doc
        .start_api(
            ApiPurposeKind::MainGeneration,
            crate::v1::event::ApiRequestSnapshot::new("openai", "gpt-5"),
            T0,
        )
        .unwrap();
    let text_seq = doc
        .append_assistant_text(api, "partial", AssistantSemanticStatus::Streaming, T0 + 1)
        .unwrap()
        .into_value();
    let interruption_at_ms = T0 + 25;

    let stats_seq = doc
        .interrupt_active_api_for_process(interruption_at_ms)
        .unwrap()
        .expect("an observed main-generation API must be interrupted");

    let text = doc
        .events()
        .iter()
        .find(|event| event.seq() == text_seq)
        .expect("streaming text event");
    assert_eq!(text.updated_at_ms, interruption_at_ms);
    assert!(matches!(
        &text.event,
        EventBody::AssistantText(value)
            if value.status == AssistantSemanticStatus::Interrupted
    ));
    let stats = doc
        .events()
        .iter()
        .find(|event| event.seq() == stats_seq)
        .expect("process interruption stats");
    assert!(matches!(
        &stats.event,
        EventBody::ApiStats(value)
            if value.status == ApiCallStatus::Interrupted
                && value.end_reason == Some(ApiEndReason::ProcessInterrupted)
                && value.timing.completed_at_ms == Some(interruption_at_ms)
    ));
    let revision_after_interruption = doc.memory_revision();
    assert_eq!(
        doc.interrupt_active_api_for_process(interruption_at_ms + 1)
            .unwrap(),
        None
    );
    assert_eq!(doc.memory_revision(), revision_after_interruption);
}

/// Completion metadata cannot contradict the immutable API-start snapshot.
/// 完成元数据不得与不可变 API 起始快照矛盾。
#[test]
fn api_completion_rejects_request_identity_drift() {
    let mut doc = new_document();
    doc.submit_root_input_fixture("a", Vec::new(), T0).unwrap();
    let api = doc
        .start_api(
            ApiPurposeKind::MainGeneration,
            ApiRequestSnapshot::new("openai", "gpt-5"),
            T0 + 1,
        )
        .unwrap();
    let result = doc.complete_api_group(
        api,
        ApiCompletion {
            request: ApiRequestSnapshot::new("openai", "different-model"),
            usage: UsageSnapshot::default(),
            status: ApiCallStatus::Failed,
            end_reason: Some(ApiEndReason::ProviderError),
            error: Some(terminal_error(ApiEndReason::ProviderError)),
            continuation_eligibility: ContinuationEligibility::Forbidden,
            timing: ApiTimings::default(),
            provider_source: None,
            completed_at_ms: T0 + 2,
        },
    );
    assert!(matches!(
        result,
        Err(DomainError::ApiRequestMismatch { field: "model", .. })
    ));
    assert_eq!(doc.active_api().map(|active| active.api_call_id), Some(api));
}

/// Provider, network, balance and retry-exhaustion failures close only their
/// physical API, interrupt active semantics, persist one notice, and preserve
/// the logical turn for Continue.
/// Provider、网络、余额与重试耗尽失败只闭合对应物理 API、中断活动语义、持久化一条通知，
/// 并保留逻辑回合供 Continue 使用。
#[test]
fn api_failures_preserve_turn_and_emit_stable_notices() -> Result<(), DomainError> {
    let mut doc = new_document();
    let turn_id = doc.submit_root_input_fixture("retry", Vec::new(), T0)?;
    let cases = [
        (
            ApiEndReason::ProviderError,
            DisplayNoticeKind::ProviderError,
        ),
        (
            ApiEndReason::NetworkError,
            DisplayNoticeKind::NetworkDisconnected,
        ),
        (
            ApiEndReason::BalanceInsufficient,
            DisplayNoticeKind::BalanceInsufficient,
        ),
        (
            ApiEndReason::RetryExhausted,
            DisplayNoticeKind::RetryExhausted,
        ),
    ];
    for (index, (reason, notice_kind)) in cases.into_iter().enumerate() {
        let now_ms = T0 + i64::try_from(index).expect("small index") * 10;
        let api = doc.start_api(
            ApiPurposeKind::RetryGeneration,
            ApiRequestSnapshot::new("openai", "gpt-5"),
            now_ms,
        )?;
        let reasoning_seq = doc
            .append_reasoning(
                api,
                "partial",
                AssistantSemanticStatus::Streaming,
                now_ms + 1,
            )?
            .into_value();
        let tool = doc.prepare_tool(
            api,
            RequestStreamId::try_new("stream-failure-tool")?,
            "shell",
            ToolPayload::try_new(serde_json::json!({"command": "work"}))?,
            None,
            now_ms + 2,
        )?;
        doc.start_prepared_tool(api, tool, now_ms + 2)?;
        let tool_seq = doc.tool_seq_by_id[&tool];
        let stats_seq = doc
            .complete_api_group(
                api,
                ApiCompletion {
                    request: ApiRequestSnapshot::new("openai", "gpt-5"),
                    usage: UsageSnapshot::default(),
                    status: ApiCallStatus::Failed,
                    end_reason: Some(reason),
                    error: Some(terminal_error(reason)),
                    continuation_eligibility: continuation_eligibility_for_test(reason),
                    timing: ApiTimings::default(),
                    provider_source: None,
                    completed_at_ms: now_ms + 3,
                },
            )?
            .stats_seq;
        assert!(matches!(
            doc.events()
                .iter()
                .find(|event| event.seq() == reasoning_seq),
            Some(ConversationEvent {
                event: EventBody::AssistantReasoning(AssistantReasoningEvent {
                    status: AssistantSemanticStatus::Interrupted,
                    ..
                }),
                ..
            })
        ));
        assert!(matches!(
            doc.events().iter().find(|event| event.seq() == tool_seq),
            Some(ConversationEvent {
                event: EventBody::ToolCall(ToolCallEvent {
                    status: ToolCallStatus::Interrupted,
                    ..
                }),
                ..
            })
        ));
        assert!(matches!(
            doc.events().iter().find(|event| event.seq() == stats_seq),
            Some(ConversationEvent {
                event: EventBody::ApiStats(ApiStatsEvent {
                    status: ApiCallStatus::Failed,
                    end_reason: Some(value),
                    ..
                }),
                ..
            }) if *value == reason
        ));
        assert!(matches!(
            doc.events().last(),
            Some(ConversationEvent {
                event: EventBody::DisplayNote(DisplayNoteEvent {
                    notice_kind: value,
                    ..
                }),
                ..
            }) if *value == notice_kind
        ));
        doc.continue_turn_fixture(now_ms + 4)?;
    }
    assert_eq!(doc.active_turn().map(|turn| turn.turn_id), Some(turn_id));
    assert_eq!(
        doc.events()
            .iter()
            .filter(|event| matches!(event.event, EventBody::UserInput(_)))
            .count(),
        1
    );
    Ok(())
}

/// A projection proof is invalid after any same-seq document revision change.
/// 任意不增长 seq 的文档修订变化后，旧投影证明都必须失效。
#[test]
fn provider_prepare_rejects_stale_document_revision_without_seq_growth() -> Result<(), DomainError>
{
    let mut doc = SessionDocument::new("stale-proof", "project", "title", T0);
    doc.submit_root_input_fixture("hello", Vec::new(), T0)?;
    let through_seq = doc.events().last().expect("root input").seq;
    let stale_projection = ProviderProjectionProof {
        segment_generation: doc.segment_generation(),
        document_revision: doc.memory_revision(),
        retained_from_seq: Seq::try_new(1)?,
        through_seq,
        fingerprint_sha256: "0".repeat(64),
    };

    assert!(doc.set_output_style(Some("concise".to_string()), T0 + 1)?);
    assert_eq!(doc.events().last().expect("root input").seq, through_seq);
    let before_prepare = doc.clone();

    assert!(matches!(
        doc.prepare_and_start_api(PrepareAndStartApiInput {
            purpose: ApiPurposeKind::MainGeneration,
            request: ApiRequestSnapshot::new("openai", "gpt-5"),
            request_budget_evidence: test_provider_request_budget_evidence(),
            projection: stale_projection,
            locked_baseline: LockedBaselineProjection::empty(),
            continuation_action_id: None,
            now_ms: T0 + 2,
        }),
        Err(DomainError::InvalidProviderProjection {
            field: "document_revision"
        })
    ));
    assert_eq!(doc, before_prepare);
    Ok(())
}

/// The latest V1 usage anchor is selected only from a matching start and usable terminal pair.
/// 最近的 V1 用量锚点只从匹配的启动事件与可用终态事件组合中选择。
#[test]
fn latest_provider_usage_anchor_uses_matching_v1_api_facts() -> Result<(), DomainError> {
    let mut doc = new_document();
    doc.submit_root_input_fixture("budget anchor", Vec::new(), T0)?;
    let first_api = doc.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 1,
    )?;
    complete_ok(&mut doc, first_api, None, T0 + 2)?;

    let first_anchor = crate::v1::latest_provider_usage_budget_anchor(doc.events())
        .expect("completed usage must produce one V1 budget anchor");
    assert_eq!(first_anchor.api_call_id, first_api);
    assert_eq!(first_anchor.usage.input_tokens, Some(10));
    assert_eq!(
        first_anchor.evidence,
        test_provider_request_budget_evidence()
    );

    let second_api = doc.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 3,
    )?;
    complete_user_interrupted(&mut doc, second_api, T0 + 4)?;
    let retained_anchor = crate::v1::latest_provider_usage_budget_anchor(doc.events())
        .expect("an unusable later terminal must not hide the last usable anchor");
    assert_eq!(retained_anchor.api_call_id, first_api);
    Ok(())
}

/// Initialized requests must match active config and cannot bypass an unpromoted pending config.
/// 已初始化请求必须匹配活动配置，且不能绕过尚未提升的待生效配置。
#[test]
fn provider_prepare_requires_the_promoted_active_request() -> Result<(), DomainError> {
    let mut doc = new_document();
    doc.submit_root_input_fixture("hello", Vec::new(), T0)?;
    let api = doc.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 1,
    )?;
    complete_ok(&mut doc, api, None, T0 + 2)?;

    let mismatched = PrepareAndStartApiInput {
        purpose: ApiPurposeKind::MainGeneration,
        request: ApiRequestSnapshot::new("openai", "gpt-5.1"),
        request_budget_evidence: test_provider_request_budget_evidence(),
        projection: ProviderProjectionProof {
            segment_generation: doc.segment_generation(),
            document_revision: doc.memory_revision(),
            retained_from_seq: Seq::try_new(1)?,
            through_seq: doc.events().last().expect("api stats").seq,
            fingerprint_sha256: "1".repeat(64),
        },
        locked_baseline: LockedBaselineProjection::empty(),
        continuation_action_id: None,
        now_ms: T0 + 3,
    };
    let before_mismatch = doc.clone();
    assert!(matches!(
        doc.prepare_and_start_api(mismatched),
        Err(DomainError::ApiRequestMismatch {
            field: "active.model",
            ..
        })
    ));
    assert_eq!(doc, before_mismatch);

    doc.set_pending_request(
        PendingRequestConfig {
            provider: Some(String::from("openai")),
            connection_id: None,
            model: Some(String::from("gpt-5.1")),
            thinking_level: None,
            temperature: None,
            set_at_ms: T0 + 4,
        },
        T0 + 4,
    )?;
    let before_pending = doc.clone();
    let blocked = PrepareAndStartApiInput {
        purpose: ApiPurposeKind::MainGeneration,
        request: ApiRequestSnapshot::new("openai", "gpt-5"),
        request_budget_evidence: test_provider_request_budget_evidence(),
        projection: ProviderProjectionProof {
            segment_generation: doc.segment_generation(),
            document_revision: doc.memory_revision(),
            retained_from_seq: Seq::try_new(1)?,
            through_seq: doc.events().last().expect("api stats").seq,
            fingerprint_sha256: "2".repeat(64),
        },
        locked_baseline: LockedBaselineProjection::empty(),
        continuation_action_id: None,
        now_ms: T0 + 5,
    };
    assert!(matches!(
        doc.prepare_and_start_api(blocked),
        Err(DomainError::InvalidRequestConfiguration {
            field: "pending_request_not_promoted"
        })
    ));
    assert_eq!(doc, before_pending);
    Ok(())
}

/// A Continue action binds exactly one API and transport observation survives restart.
/// 一次 Continue 动作只绑定一个 API，且 transport 观察事实可跨重启恢复。
#[test]
fn continue_api_binding_and_transport_observation_are_restart_idempotent() -> Result<(), DomainError>
{
    let mut doc = new_document();
    let turn_id = doc.submit_root_input_fixture("continue", Vec::new(), T0)?;
    let first_api = doc.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 1,
    )?;
    complete_user_interrupted(&mut doc, first_api, T0 + 2)?;
    let action_id = ClientActionId::try_new("continue-action-1")?;
    let continuation_request = ContinuationRequest::new(action_id.clone(), "", Vec::new())
        .with_hidden_model_guidance(Some(String::from("Continue the active plan.")));
    let continued = doc.continue_open_turn(continuation_request.clone(), T0 + 3)?;
    assert!(continued.changed());
    let continued = continued.into_value();
    assert_eq!(continued.turn_id, turn_id);
    assert!(continued.guidance_seq.is_some());
    let revision_after_continue = doc.memory_revision();
    let repeated = doc.continue_open_turn(continuation_request.clone(), T0 + 3)?;
    assert!(!repeated.changed());
    assert_eq!(repeated.into_value(), continued);
    assert_eq!(doc.memory_revision(), revision_after_continue);
    let before_guidance_drift = doc.clone();
    assert!(matches!(
        doc.continue_open_turn(
            ContinuationRequest::new(action_id.clone(), "", Vec::new())
                .with_hidden_model_guidance(Some(String::from("Different guidance."))),
            T0 + 3,
        ),
        Err(DomainError::ClientActionConflict { .. })
    ));
    assert_eq!(doc, before_guidance_drift);

    let request = ApiRequestSnapshot::new("openai", "gpt-5");
    let projection = ProviderProjectionProof {
        segment_generation: doc.segment_generation(),
        document_revision: doc.memory_revision(),
        retained_from_seq: Seq::try_new(1)?,
        through_seq: doc.events().last().expect("continue resumed").seq,
        fingerprint_sha256: "a".repeat(64),
    };
    let baseline =
        LockedBaselineProjection::from_locked_sources(doc.runtime_lock.locked_sources.clone())?;
    let prepared = doc.prepare_and_start_api(PrepareAndStartApiInput {
        purpose: ApiPurposeKind::Continue,
        request: request.clone(),
        request_budget_evidence: test_provider_request_budget_evidence(),
        projection: projection.clone(),
        locked_baseline: baseline.clone(),
        continuation_action_id: Some(action_id.clone()),
        now_ms: T0 + 4,
    })?;
    assert!(prepared.changed());
    let prepared = prepared.into_value();
    assert_eq!(prepared.continuation_action_id.as_ref(), Some(&action_id));
    assert_eq!(doc.api_transport_observed_for_action(&action_id)?, None);
    assert_eq!(
        doc.continuation_dispatch_resolution(&action_id)?,
        Some(ContinuationDispatchResolution::AwaitingTransport {
            api_call_id: prepared.api_call_id,
            started_seq: prepared.started_seq,
        })
    );

    let mut restored = SessionDocument::from_parts(
        doc.metadata.clone(),
        doc.runtime_lock.clone(),
        doc.compactions.clone(),
        doc.segment_catalog().to_vec(),
        doc.events().to_vec(),
    )?;
    assert_eq!(
        restored.project_persisted_interruptions()?,
        0,
        "an unobserved Continue remains dispatchable after restart"
    );
    let revision_before_retry = restored.memory_revision();
    let retried = restored.prepare_and_start_api(PrepareAndStartApiInput {
        purpose: ApiPurposeKind::Continue,
        request,
        request_budget_evidence: test_provider_request_budget_evidence(),
        projection,
        locked_baseline: baseline,
        continuation_action_id: Some(action_id.clone()),
        now_ms: T0 + 5,
    })?;
    assert!(!retried.changed());
    assert_eq!(retried.into_value(), prepared);
    assert_eq!(restored.memory_revision(), revision_before_retry);

    let observed = restored.mark_api_transport_observed(prepared.api_call_id, T0 + 6)?;
    assert!(observed.changed());
    let observed = observed.into_value();
    assert_eq!(observed.continuation_action_id.as_ref(), Some(&action_id));
    assert_eq!(
        restored.api_transport_observed_for_action(&action_id)?,
        Some(observed.clone())
    );
    assert_eq!(
        restored.continuation_dispatch_resolution(&action_id)?,
        Some(ContinuationDispatchResolution::TransportObserved(
            observed.clone()
        ))
    );
    let revision_before_observation_retry = restored.memory_revision();
    let observed_again = restored.mark_api_transport_observed(prepared.api_call_id, T0 + 7)?;
    assert!(!observed_again.changed());
    assert_eq!(observed_again.into_value(), observed);
    assert_eq!(
        restored.memory_revision(),
        revision_before_observation_retry
    );

    let mut reloaded = SessionDocument::from_parts(
        restored.metadata.clone(),
        restored.runtime_lock.clone(),
        restored.compactions.clone(),
        restored.segment_catalog().to_vec(),
        restored.events().to_vec(),
    )?;
    assert_eq!(
        reloaded.api_transport_observed_for_action(&action_id)?,
        Some(observed)
    );
    assert!(reloaded.project_persisted_interruptions()? > 0);
    assert!(reloaded.events().iter().any(|event| {
        matches!(
            &event.event,
            EventBody::ApiStats(stats)
                if stats.api_call_id == prepared.api_call_id
                    && stats.end_reason == Some(ApiEndReason::ProcessInterrupted)
        )
    }));
    Ok(())
}

/// A Continue API terminal before transport observation is reconstructed explicitly.
/// Continue API 在传输观察前终止时会被明确重建。
#[test]
fn continue_dispatch_resolution_reports_terminal_before_transport() -> Result<(), DomainError> {
    let mut doc = new_document();
    doc.submit_root_input_fixture("continue terminal", Vec::new(), T0)?;
    let first_api = doc.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 1,
    )?;
    complete_user_interrupted(&mut doc, first_api, T0 + 2)?;
    let action_id = ClientActionId::try_new("continue-terminal-before-transport")?;
    doc.continue_open_turn(
        ContinuationRequest::new(action_id.clone(), "", Vec::new()),
        T0 + 3,
    )?;
    let projection = ProviderProjectionProof {
        segment_generation: doc.segment_generation(),
        document_revision: doc.memory_revision(),
        retained_from_seq: Seq::try_new(1)?,
        through_seq: doc.events().last().expect("continue resumed").seq,
        fingerprint_sha256: "b".repeat(64),
    };
    let baseline =
        LockedBaselineProjection::from_locked_sources(doc.runtime_lock.locked_sources.clone())?;
    let prepared = doc
        .prepare_and_start_api(PrepareAndStartApiInput {
            purpose: ApiPurposeKind::Continue,
            request: ApiRequestSnapshot::new("openai", "gpt-5"),
            request_budget_evidence: test_provider_request_budget_evidence(),
            projection,
            locked_baseline: baseline,
            continuation_action_id: Some(action_id.clone()),
            now_ms: T0 + 4,
        })?
        .into_value();
    let terminal_seq = complete_user_interrupted(&mut doc, prepared.api_call_id, T0 + 5)?;

    assert_eq!(
        doc.continuation_dispatch_resolution_through(&action_id, prepared.started_seq)?,
        Some(ContinuationDispatchResolution::AwaitingTransport {
            api_call_id: prepared.api_call_id,
            started_seq: prepared.started_seq,
        }),
        "the durable-prefix query must not observe a later in-memory terminal"
    );
    assert_eq!(
        doc.continuation_dispatch_resolution(&action_id)?,
        Some(ContinuationDispatchResolution::TerminalBeforeTransport {
            api_call_id: prepared.api_call_id,
            started_seq: prepared.started_seq,
            terminal_seq,
        })
    );
    Ok(())
}

/// API terminal validation rejects impossible classifications before mutation and on reload.
/// API 终态校验在修改前及重载时拒绝不可能的分类组合。
#[test]
fn api_terminal_contract_is_shared_by_mutation_and_loader() -> Result<(), DomainError> {
    let mut doc = new_document();
    doc.submit_root_input_fixture("hello", Vec::new(), T0)?;
    let api = doc.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 1,
    )?;
    let before_invalid = doc.clone();
    let invalid = doc.complete_api_group(
        api,
        ApiCompletion {
            request: ApiRequestSnapshot::new("openai", "gpt-5"),
            usage: UsageSnapshot::default(),
            status: ApiCallStatus::Completed,
            end_reason: Some(ApiEndReason::NetworkError),
            error: Some(RedactedApiError {
                category: ApiErrorCategory::Network,
                error_code: None,
                message: String::from("redacted"),
            }),
            continuation_eligibility: ContinuationEligibility::Allowed,
            timing: ApiTimings {
                prepared_at_ms: Some(T0 + 1),
                completed_at_ms: Some(T0 + 2),
                ..ApiTimings::default()
            },
            provider_source: None,
            completed_at_ms: T0 + 2,
        },
    );
    assert!(matches!(
        invalid,
        Err(DomainError::InvalidApiTerminalFacts {
            field: "status_reason_error"
        })
    ));
    assert_eq!(doc, before_invalid);

    complete_ok(&mut doc, api, None, T0 + 3)?;
    let mut corrupted_events = doc.events().to_vec();
    let stats = corrupted_events
        .iter_mut()
        .find_map(|event| match &mut event.event {
            EventBody::ApiStats(stats) => Some(stats),
            _ => None,
        })
        .expect("valid API stats");
    stats.end_reason = Some(ApiEndReason::NetworkError);
    stats.error = Some(RedactedApiError {
        category: ApiErrorCategory::Network,
        error_code: None,
        message: String::from("redacted"),
    });
    assert!(matches!(
        SessionDocument::from_parts(
            doc.metadata.clone(),
            doc.runtime_lock.clone(),
            doc.compactions.clone(),
            doc.segment_catalog.clone(),
            corrupted_events,
        ),
        Err(DomainError::InvalidApiTerminalFacts {
            field: "status_reason_error"
        })
    ));

    assert!(matches!(
        validate_api_terminal_facts(
            ApiCallStatus::Failed,
            Some(ApiEndReason::NetworkError),
            Some(&RedactedApiError {
                category: ApiErrorCategory::Network,
                error_code: None,
                message: String::from("redacted"),
            }),
            ContinuationEligibility::Allowed,
            &ApiTimings {
                prepared_at_ms: Some(T0 + 5),
                first_token_at_ms: Some(T0 + 4),
                completed_at_ms: Some(T0 + 6),
                ..ApiTimings::default()
            },
        ),
        Err(DomainError::InvalidApiTerminalFacts {
            field: "timing.first_token_at_ms"
        })
    ));
    Ok(())
}
