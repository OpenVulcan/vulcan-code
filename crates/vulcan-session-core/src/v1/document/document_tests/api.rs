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

/// A fatal tool durability failure closes the logical Turn and forbids Continue.
/// 致命工具持久化失败会闭合逻辑回合并禁止继续。
#[test]
fn execution_boundary_failure_closes_turn_and_forbids_continue() -> Result<(), DomainError> {
    // document is the authoritative event graph mutated by the fatal boundary transition.
    // document 是由致命边界转换修改的权威事件图。
    let mut document = new_document();
    // turn_id identifies the logical owner that must be closed atomically.
    // turn_id 标识必须原子闭合的逻辑 owner。
    let turn_id = document.submit_root_input_fixture("root", Vec::new(), T0)?;
    // api_call_id identifies the active physical request that must become failed.
    // api_call_id 标识必须进入失败状态的活动物理请求。
    let api_call_id = document.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("deepseek", "deepseek-v4.1-flash"),
        T0,
    )?;
    // tool_call_id identifies the in-flight tool that must become interrupted.
    // tool_call_id 标识必须进入中断状态的进行中工具。
    let tool_call_id = document.start_tool(
        api_call_id,
        "file",
        ToolPayload::try_new(serde_json::json!({"action": "write", "path": "new.txt"}))?,
        None,
        T0,
    )?;
    // revision_before_failure proves the composite transition publishes exactly one revision.
    // revision_before_failure 证明复合转换只发布一个修订。
    let revision_before_failure = document.memory_revision();

    assert!(document.fail_execution_boundary_and_close_turn(
        "tool_boundary_durability_failed",
        Some("Invalid change-set field at payload.files[0].hunks"),
        T0 + 1,
    )?);
    assert_eq!(document.memory_revision(), revision_before_failure + 1);
    // failure_receipt must contain every API, tool, notice, and Turn terminal mutation.
    // failure_receipt 必须包含全部 API、工具、提示与回合终态变更。
    let failure_receipt = document.memory_revision_receipt(revision_before_failure + 1)?;
    assert_eq!(failure_receipt.revision, revision_before_failure + 1);
    assert!(failure_receipt.changed_events.iter().any(|event| {
        matches!(
            &event.event,
            EventBody::ToolCall(tool) if tool.tool_call_id == tool_call_id
        )
    }));
    assert!(failure_receipt.changed_events.iter().any(|event| {
        matches!(
            &event.event,
            EventBody::ApiStats(stats) if stats.api_call_id == api_call_id
        )
    }));
    assert!(
        failure_receipt
            .changed_events
            .iter()
            .any(|event| { matches!(&event.event, EventBody::DisplayNote(_)) })
    );
    assert!(
        failure_receipt
            .changed_events
            .iter()
            .any(|event| { matches!(&event.event, EventBody::TurnBoundary(_)) })
    );
    assert!(
        failure_receipt
            .changed_events
            .iter()
            .any(|event| { matches!(&event.event, EventBody::TurnStats(_)) })
    );
    assert!(matches!(
        document.memory_revision_receipt(revision_before_failure + 2),
        Err(DomainError::ReferenceViolation {
            kind: "memory_revision_receipt",
            ..
        })
    ));
    assert!(document.active_api().is_none());
    assert_eq!(
        document
            .active_turn()
            .map(|turn| (turn.turn_id, turn.closed)),
        Some((turn_id, true))
    );
    assert!(document.events().iter().any(|event| {
        matches!(
            &event.event,
            EventBody::ToolCall(tool)
                if tool.tool_call_id == tool_call_id
                    && tool.status == ToolCallStatus::Interrupted
                    && tool.finished_at_ms == Some(T0 + 1)
        )
    }));
    assert!(document.events().iter().any(|event| {
        matches!(
            &event.event,
            EventBody::ApiStats(stats)
                if stats.api_call_id == api_call_id
                    && stats.status == ApiCallStatus::Failed
                    && stats.end_reason == Some(ApiEndReason::Unknown)
                    && stats.continuation_eligibility == ContinuationEligibility::Forbidden
                    && stats.error.as_ref().is_some_and(|error|
                        error.category == ApiErrorCategory::Unknown
                            && error.error_code.as_deref()
                                == Some("tool_boundary_durability_failed")
                            && error.message
                                == "Invalid change-set field at payload.files[0].hunks")
        )
    }));
    assert!(document.events().iter().any(|event| {
        matches!(
            &event.event,
            EventBody::DisplayNote(note)
                if note.notice_kind == DisplayNoticeKind::SystemError
                    && note.severity == NoticeSeverity::Error
                    && note.reason_code == "tool_boundary_durability_failed"
                    && note.api_call_id == Some(api_call_id)
                    && note.redacted_detail.as_deref()
                        == Some("Invalid change-set field at payload.files[0].hunks")
        )
    }));
    assert!(!document.fail_execution_boundary_and_close_turn(
        "tool_boundary_durability_failed",
        Some("Invalid change-set field at payload.files[0].hunks"),
        T0 + 2,
    )?);
    document.rebuild_derived_state()?;
    document.submit_root_input_fixture("new root", Vec::new(), T0 + 3)?;
    Ok(())
}

/// A pre-API error remains visible while both original and subsequent user prompts reach Provider projection.
/// API 前错误保持可见，同时原始与后续用户提示词都进入 Provider 投影。
/// No external resources are used; returns any domain error instead of erasing the accepted input.
/// 不使用外部资源；直接返回领域错误，不删除已接纳输入。
#[test]
fn pre_api_error_preserves_two_consecutive_provider_user_prompts() -> Result<(), DomainError> {
    use crate::v1::projection::{ProjectedItem, project_document};
    // No ApiStarted event exists when preparation rejects the first admitted input.
    // 准备阶段拒绝首次已接纳输入时，尚不存在 ApiStarted 事件。
    let mut document = new_document();
    document.submit_root_input_fixture("first prompt", Vec::new(), T0)?;
    document.fail_execution_boundary_and_close_turn(
        "submission_preparation_failed",
        Some("exact preparation error"),
        T0 + 1,
    )?;
    assert!(document.active_turn().unwrap().closed);
    assert!(!document.events().iter().any(|event| matches!(
        event.event,
        EventBody::ApiStarted(_) | EventBody::ApiStats(_)
    )));
    assert_eq!(
        document
            .events()
            .iter()
            .filter(|event| matches!(&event.event,
                EventBody::DisplayNote(note) if note.reason_code == "submission_preparation_failed"
                    && note.redacted_detail.as_deref() == Some("exact preparation error")
                    && event.visibility == EventVisibility::UiOnly
            ))
            .count(),
        1
    );
    document.rebuild_derived_state()?;
    document.submit_root_input_fixture("second prompt", Vec::new(), T0 + 2)?;
    assert_eq!(
        project_document(&document, None, &[])?,
        vec![
            ProjectedItem::UserInput {
                text: "first prompt".into(),
                resource_ids: vec![]
            },
            ProjectedItem::UserInput {
                text: "second prompt".into(),
                resource_ids: vec![]
            },
        ]
    );
    Ok(())
}

/// A host failure after successful API persistence preserves that terminal and closes the Turn separately.
/// 成功 API 已持久化后的宿主失败会保留该终态，并单独闭合逻辑回合。
#[test]
fn post_terminal_host_failure_preserves_successful_api_and_closes_turn() -> Result<(), DomainError>
{
    // document owns one open logical Turn whose physical API has already completed durably.
    // document 持有一个物理 API 已可靠完成但逻辑回合仍开放的会话。
    let mut document = new_document();
    // turn_id identifies the logical owner that the later host failure must close.
    // turn_id 标识后续宿主失败必须闭合的逻辑 owner。
    let turn_id = document.submit_root_input_fixture("root", Vec::new(), T0)?;
    // api_call_id identifies the successful physical API that must not be rewritten.
    // api_call_id 标识不得被改写的成功物理 API。
    let api_call_id = document.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0,
    )?;
    // stats_seq locates the exact completed terminal retained across the host failure.
    // stats_seq 定位跨宿主失败仍需保留的精确已完成终态。
    let stats_seq = complete_ok(&mut document, api_call_id, None, T0 + 1)?;
    // terminal_before is the byte-for-byte event value that the failure boundary must preserve.
    // terminal_before 是失败边界必须原样保留的事件值。
    let terminal_before = document
        .events()
        .iter()
        .find(|event| event.seq == stats_seq)
        .cloned()
        .expect("completed API terminal");
    // revision_before_failure proves the later host failure publishes one independent revision.
    // revision_before_failure 证明后续宿主失败发布一个独立修订。
    let revision_before_failure = document.memory_revision();

    assert!(document.fail_execution_boundary_and_close_turn(
        "tool_lifecycle_projection_failed",
        Some("live lifecycle projection rejected after durable tool completion"),
        T0 + 2,
    )?);
    assert_eq!(document.memory_revision(), revision_before_failure + 1);
    assert_eq!(
        document
            .events()
            .iter()
            .find(|event| event.seq == stats_seq),
        Some(&terminal_before)
    );
    assert!(matches!(
        &terminal_before.event,
        EventBody::ApiStats(stats)
            if stats.api_call_id == api_call_id
                && stats.status == ApiCallStatus::Completed
                && stats.error.is_none()
    ));
    assert!(document.events().iter().any(|event| {
        matches!(
            &event.event,
            EventBody::DisplayNote(note)
                if note.notice_kind == DisplayNoticeKind::SystemError
                    && note.reason_code == "tool_lifecycle_projection_failed"
                    && note.redacted_detail.as_deref()
                        == Some("live lifecycle projection rejected after durable tool completion")
        )
    }));
    assert_eq!(
        document
            .active_turn()
            .map(|turn| (turn.turn_id, turn.closed)),
        Some((turn_id, true))
    );
    document.rebuild_derived_state()?;
    document.submit_root_input_fixture("new root", Vec::new(), T0 + 3)?;
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
    let bound_request_config = ActiveRequestConfig {
        provider: Some(String::from("openai")),
        connection_id: None,
        model: Some(String::from("gpt-5.1")),
        thinking_level: Some(String::from("low")),
        temperature: None,
    };
    let pending_input_id = document
        .add_pending_in_turn_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("interrupt-pending-input")?,
                "supplement",
                Vec::new(),
            )
            .with_bound_request_config(bound_request_config.clone()),
            T0 + 1,
        )?
        .into_value();
    assert_eq!(
        document.metadata.config.active_request.model.as_deref(),
        Some("gpt-5")
    );
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
    assert_eq!(
        document.metadata.config.active_request,
        bound_request_config
    );
    let promoted_event = document
        .events()
        .iter()
        .find(|event| {
            matches!(
                &event.event,
                EventBody::UserInput(input) if input.input_id == pending_input_id
            )
        })
        .expect("promoted input after interrupted ApiStats");
    assert!(promoted_event.seq.value() > stats_seq.value());
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

/// `ui-code` snapshots attach only with the sealing revision and stay immutable afterwards.
/// `ui-code` 快照只随封口修订附着，之后保持不可变。
#[test]
fn answer_code_snapshots_attach_once_when_text_is_sealed() -> Result<(), DomainError> {
    use crate::v1::answer_components::{AnswerCodeSnapshot, AnswerCodeSnapshotResult};
    let mut doc = new_document();
    doc.submit_root_input_fixture("root", Vec::new(), T0)?;
    let api = doc.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 1,
    )?;
    let body = "```ui-code\nsrc/a.rs:1-2\n```";
    let snapshot = |text: &str| AnswerCodeSnapshot {
        location: "src/a.rs:1-2".into(),
        result: AnswerCodeSnapshotResult::Captured { text: text.into() },
    };
    doc.append_assistant_text(api, body, AssistantSemanticStatus::Streaming, T0 + 2)?;

    // Streaming text cannot carry snapshots.
    // 流式正文不能携带快照。
    assert!(matches!(
        doc.append_assistant_text_with_code_snapshots(
            api,
            body,
            AssistantSemanticStatus::Streaming,
            vec![snapshot("a\nb")],
            T0 + 3,
        ),
        Err(DomainError::InvalidAnswerCodeSnapshots { field: "status" })
    ));

    // The sealing revision attaches them; an exact replay is idempotent.
    // 封口修订附着快照；完全相同的重放保持幂等。
    let sealed = doc.append_assistant_text_with_code_snapshots(
        api,
        body,
        AssistantSemanticStatus::Completed,
        vec![snapshot("a\nb")],
        T0 + 4,
    )?;
    assert!(sealed.changed());
    let seq = sealed.into_value();
    let replay = doc.append_assistant_text_with_code_snapshots(
        api,
        body,
        AssistantSemanticStatus::Completed,
        vec![snapshot("a\nb")],
        T0 + 5,
    )?;
    assert!(!replay.changed());
    let stored = doc
        .events()
        .iter()
        .find(|event| event.seq() == seq)
        .expect("assistant text event");
    let EventBody::AssistantText(text) = &stored.event else {
        panic!("expected assistant text");
    };
    assert_eq!(text.code_snapshots, vec![snapshot("a\nb")]);

    // A sealed reply never recaptures.
    // 已封口的回答绝不重新抓取。
    let before_rewrite = doc.clone();
    assert!(matches!(
        doc.append_assistant_text_with_code_snapshots(
            api,
            body,
            AssistantSemanticStatus::Completed,
            vec![snapshot("changed")],
            T0 + 6,
        ),
        Err(DomainError::ImmutableFieldMutation {
            entity: "assistant_semantic",
            field: "code_snapshots",
        })
    ));
    assert_eq!(doc, before_rewrite);
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

/// An unclassified failed API emits one durable SystemError notice without changing continuation policy.
/// 未分类 API 失败会产生一条可靠 SystemError 提示，并且不会改变继续策略。
#[test]
fn unclassified_api_failure_emits_system_error_notice() -> Result<(), DomainError> {
    // doc owns one isolated failed API whose policy remains forbidden.
    // doc 持有一个继续策略保持禁止的隔离失败 API。
    let mut doc = new_document();
    doc.submit_root_input_fixture("unknown failure", Vec::new(), T0)?;
    // api is the exact physical call closed by the unclassified terminal.
    // api 是由未分类终态闭合的精确物理调用。
    let api = doc.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 1,
    )?;
    doc.complete_api_group(
        api,
        ApiCompletion {
            request: ApiRequestSnapshot::new("openai", "gpt-5"),
            usage: UsageSnapshot::default(),
            status: ApiCallStatus::Failed,
            end_reason: Some(ApiEndReason::Unknown),
            error: Some(terminal_error(ApiEndReason::Unknown)),
            continuation_eligibility: continuation_eligibility_for_test(ApiEndReason::Unknown),
            timing: ApiTimings::default(),
            provider_source: None,
            completed_at_ms: T0 + 2,
        },
    )?;

    assert!(matches!(
        doc.events().last(),
        Some(ConversationEvent {
            event: EventBody::DisplayNote(DisplayNoteEvent {
                notice_kind: DisplayNoticeKind::SystemError,
                severity: NoticeSeverity::Error,
                reason_code,
                api_call_id: Some(actual_api_call_id),
                ..
            }),
            ..
        }) if reason_code == "system_error" && *actual_api_call_id == api
    ));
    // continue_error proves that adding the UI notice did not weaken the forbidden continuation policy.
    // continue_error 证明添加 UI 提示没有削弱禁止继续的策略。
    let continue_error = doc
        .continue_turn_fixture(T0 + 3)
        .expect_err("unclassified failure must remain non-continuable");
    assert!(matches!(
        continue_error,
        DomainError::ContinuationNotEligible {
            reason: "terminal_policy_forbidden"
        }
    ));
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

/// Initialized requests must match active config while pending picker intent remains deferred.
/// 已初始化请求必须匹配活动配置，而待生效的选择器意图继续延后。
#[test]
fn provider_prepare_uses_active_request_and_ignores_pending_picker_intent()
-> Result<(), DomainError> {
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
    let prepared_with_pending = PrepareAndStartApiInput {
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
    let prepared = doc.prepare_and_start_api(prepared_with_pending)?;
    assert!(prepared.changed());
    assert_eq!(
        doc.metadata
            .config
            .pending_request
            .as_ref()
            .and_then(|pending| pending.model.as_deref()),
        Some("gpt-5.1")
    );
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
    let continued_request_config = ActiveRequestConfig {
        provider: Some(String::from("openai")),
        connection_id: None,
        model: Some(String::from("gpt-5.1")),
        thinking_level: Some(String::from("max")),
        temperature: None,
    };
    let mut continuation_request = ContinuationRequest::new(action_id.clone(), "", Vec::new())
        .with_hidden_model_guidance(Some(String::from("Continue the active plan.")));
    continuation_request.submission = continuation_request
        .submission
        .with_bound_request_config(continued_request_config.clone());
    let continued = doc.continue_open_turn(continuation_request.clone(), T0 + 3)?;
    assert!(continued.changed());
    let continued = continued.into_value();
    assert_eq!(continued.turn_id, turn_id);
    assert!(continued.guidance_seq.is_some());
    assert_eq!(doc.metadata.config.active_request, continued_request_config);
    let revision_after_continue = doc.memory_revision();
    let repeated = doc.continue_open_turn(continuation_request, T0 + 3)?;
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

    let mut request = ApiRequestSnapshot::new("openai", "gpt-5.1");
    request.thinking_level = Some(String::from("max"));
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
    // Explicit new input supersedes the prepared request after the host proves it has no worker.
    // 宿主证明没有工作器后，显式新输入取代已准备的请求。
    let mut replaced = restored.clone();
    replaced.submit_root_input_after_execution_ended(
        ClientInputSubmission::new(
            ClientActionId::try_new("supersede-undispatched-continue")?,
            "new task",
            Vec::new(),
        ),
        T0 + 5,
    )?;
    assert!(replaced.active_api().is_none());
    replaced.rebuild_derived_state()?;
    assert_ne!(
        replaced.active_turn().unwrap().turn_id,
        restored.active_turn().unwrap().turn_id
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
            projection: ContinuationTerminalProjection::NoNoticeRequired,
        })
    );
    Ok(())
}

/// A failed Continue terminal requires its correlated DisplayNote before RPC acknowledgement is safe.
/// 失败的 Continue 终态只有在关联 DisplayNote 已存在后才能安全返回 RPC 确认。
#[test]
fn continue_failed_terminal_requires_durable_inline_notice() -> Result<(), DomainError> {
    // doc owns the exact Continue API and its ordered ApiStats-plus-DisplayNote transaction.
    // doc 持有精确 Continue API 及其有序 ApiStats 与 DisplayNote 事务。
    let mut doc = new_document();
    doc.submit_root_input_fixture("continue failed terminal", Vec::new(), T0)?;
    // first_api creates the interrupted predecessor required by the Continue contract.
    // first_api 创建 Continue 契约所需的中断前序 API。
    let first_api = doc.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 1,
    )?;
    complete_user_interrupted(&mut doc, first_api, T0 + 2)?;
    // action_id is the exact browser identity bound to the failing Continue API.
    // action_id 是绑定失败 Continue API 的精确浏览器身份。
    let action_id = ClientActionId::try_new("continue-failed-inline-notice")?;
    doc.continue_open_turn(
        ContinuationRequest::new(action_id.clone(), "", Vec::new()),
        T0 + 3,
    )?;
    // projection freezes the complete durable prefix used to prepare the physical Continue request.
    // projection 冻结用于准备物理 Continue 请求的完整可靠前缀。
    let projection = ProviderProjectionProof {
        segment_generation: doc.segment_generation(),
        document_revision: doc.memory_revision(),
        retained_from_seq: Seq::try_new(1)?,
        through_seq: doc.events().last().expect("continue resumed").seq,
        fingerprint_sha256: "c".repeat(64),
    };
    // baseline is the exact locked system-source projection inherited by the Continue request.
    // baseline 是 Continue 请求继承的精确锁定系统来源投影。
    let baseline =
        LockedBaselineProjection::from_locked_sources(doc.runtime_lock.locked_sources.clone())?;
    // prepared identifies the immutable ApiStarted fact for this exact browser action.
    // prepared 标识该精确浏览器动作的不可变 ApiStarted 事实。
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
    // completion atomically appends the failed ApiStats and its user-visible network notice.
    // completion 原子追加失败 ApiStats 及其用户可见网络提示。
    let completion = doc.complete_api_group(
        prepared.api_call_id,
        ApiCompletion {
            request: ApiRequestSnapshot::new("openai", "gpt-5"),
            usage: UsageSnapshot::default(),
            status: ApiCallStatus::Failed,
            end_reason: Some(ApiEndReason::NetworkError),
            error: Some(terminal_error(ApiEndReason::NetworkError)),
            continuation_eligibility: ContinuationEligibility::Allowed,
            timing: ApiTimings::default(),
            provider_source: None,
            completed_at_ms: T0 + 5,
        },
    )?;
    // notice_seq is the exact DisplayNote correlated to the failed physical API.
    // notice_seq 是与失败物理 API 关联的精确 DisplayNote 序号。
    let notice_seq = doc
        .events()
        .iter()
        .find(|event| {
            matches!(
                &event.event,
                EventBody::DisplayNote(note)
                    if note.api_call_id == Some(prepared.api_call_id)
            )
        })
        .map(|event| event.seq)
        .expect("failed Continue API must persist an inline notice");

    assert_eq!(
        doc.continuation_dispatch_resolution_through(&action_id, completion.stats_seq)?,
        Some(ContinuationDispatchResolution::TerminalBeforeTransport {
            api_call_id: prepared.api_call_id,
            started_seq: prepared.started_seq,
            terminal_seq: completion.stats_seq,
            projection: ContinuationTerminalProjection::MissingNotice,
        }),
        "ApiStats alone cannot prove that the user-visible terminal error is durable"
    );
    assert_eq!(
        doc.continuation_dispatch_resolution(&action_id)?,
        Some(ContinuationDispatchResolution::TerminalBeforeTransport {
            api_call_id: prepared.api_call_id,
            started_seq: prepared.started_seq,
            terminal_seq: completion.stats_seq,
            projection: ContinuationTerminalProjection::InlineNotice { notice_seq },
        })
    );
    Ok(())
}

/// A Continue Turn that fails before ApiStarted settles from its own durable Turn boundary.
/// 在 ApiStarted 前失败的 Continue 回合根据自身可靠回合边界完成结算。
#[test]
fn continue_dispatch_resolution_reports_terminal_before_api() -> Result<(), DomainError> {
    // doc owns the exact durable event sequence exercised by this resolution test.
    // doc 持有本次结算测试执行的精确可靠事件序列。
    let mut doc = new_document();
    doc.submit_root_input_fixture("continue pre-api terminal", Vec::new(), T0)?;
    // first_api creates the interrupted predecessor whose open Turn can be continued.
    // first_api 创建可继续其开放回合的中断前序 API。
    let first_api = doc.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 1,
    )?;
    complete_user_interrupted(&mut doc, first_api, T0 + 2)?;
    // action_id is the browser idempotency identity bound to this exact Continue attempt.
    // action_id 是绑定本次精确 Continue 尝试的浏览器幂等身份。
    let action_id = ClientActionId::try_new("continue-terminal-before-api")?;
    doc.continue_open_turn(
        ContinuationRequest::new(action_id.clone(), "", Vec::new()),
        T0 + 3,
    )?;
    // resumed_seq is the durable frontier before the terminal boundary exists.
    // resumed_seq 是终态边界出现前的可靠前沿。
    let resumed_seq = doc.events().last().expect("continue resumed").seq;

    assert_eq!(
        doc.continuation_dispatch_resolution_through(&action_id, resumed_seq)?,
        None,
        "a resumed Continue remains pending until its Turn has a durable terminal boundary"
    );
    assert!(doc.fail_turn_without_active_api(
        "stream_finished_without_v1_api_terminal",
        Some("controller seed was unavailable"),
        T0 + 4,
    )?);
    // terminal_seq selects the exact Turn boundary produced by the pre-API failure.
    // terminal_seq 选择 API 前失败产生的精确回合边界。
    let terminal_seq = doc
        .events()
        .iter()
        .find(|event| event.seq > resumed_seq && matches!(&event.event, EventBody::TurnBoundary(_)))
        .map(|event| event.seq)
        .expect("pre-api failure must persist a terminal Turn boundary");
    // notice_seq selects the atomic system-error note written before the Turn boundary.
    // notice_seq 选择在回合边界前原子写入的系统错误提示。
    let notice_seq = doc
        .events()
        .iter()
        .find(|event| {
            event.seq > resumed_seq
                && matches!(
                    &event.event,
                    EventBody::DisplayNote(DisplayNoteEvent {
                        notice_kind: DisplayNoticeKind::SystemError,
                        ..
                    })
                )
        })
        .map(|event| event.seq)
        .expect("pre-api failure must persist an inline system-error notice");
    // requested_seq is the immediately preceding Continue request fact in this validated pair.
    // requested_seq 是该已校验配对中紧邻在前的 Continue 请求事实。
    let requested_seq = Seq::try_new(resumed_seq.value() - 1)?;

    assert_eq!(
        doc.continuation_dispatch_resolution(&action_id)?,
        Some(ContinuationDispatchResolution::TerminalBeforeApi {
            requested_seq,
            resumed_seq,
            terminal_seq,
            projection: ContinuationTerminalProjection::InlineNotice { notice_seq },
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
