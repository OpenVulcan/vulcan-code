//! Phase-2 memory-state-machine invariant tests for `SessionDocument`.
//! `SessionDocument` 的阶段 2 内存状态机不变量测试。
//!
//! The primary invariant under test is: persistence order equals actual time
//! order, meaning the seq allocated by each mutation equals the order in
//! which the semantic fact actually occurred, and updates never allocate new
//! seq values.
//! 被测的首要不变量是：持久化顺序等于实际时间顺序，即每次 mutation 分配的
//! seq 等于该语义事实实际发生的顺序，且更新绝不分配新的 seq。

use super::*;
use crate::v1::config::{
    ActiveRequestConfig, GoalState, PendingRequestConfig, ProviderExtensionTargetSetConfig,
    ProviderMediaUnderstandingMode, ProviderMediaUnderstandingSelection,
    ProviderToolCapabilitySelection, ProviderToolSelectionConfig, ProviderToolTargetConfig,
    QueuedInputIdentity,
};
use crate::v1::context::{
    LockedBaselineSegmentInput, LockedPromptTarget, LockedSource, LockedSourceKind,
};
use crate::v1::ids::JS_SAFE_INTEGER_MAX;
use crate::v1::stats::UsageSnapshot;
use vulcan_ai_contract::ProviderReplayEnvelope;
use vulcan_change_set::{ChangeSetFileChangeKind, ChangeSetMode, PersistentChangeSetFile};

/// Fixed semantic timestamp used by mutation fixtures.
/// mutation 夹具使用的固定语义时间戳。
const T0: i64 = 1_700_000_000_000;

/// Creates one fresh empty session document for tests.
/// 为测试创建一个全新空会话文档。
fn new_document() -> SessionDocument {
    SessionDocument::new("sess-1", "project-1", "title", T0)
}

/// Builds an explicitly initialized empty runtime lock for loader fixtures.
/// 为加载器夹具构建一份明确初始化的空运行时锁。
fn initialized_runtime_lock() -> RuntimeLock {
    RuntimeLock {
        initialization_state: RuntimeInitializationState::Initialized,
        ..RuntimeLock::default()
    }
}

/// Builds one official usage snapshot.
/// 构建一个官方用量快照。
fn usage(input: u64, output: u64) -> UsageSnapshot {
    UsageSnapshot {
        input_tokens: Some(input),
        uncached_input_tokens: Some(input),
        output_tokens: Some(output),
        total_tokens: Some(input + output),
        ..UsageSnapshot::default()
    }
}

/// Builds one terminal or checkpoint tool update without optional artifacts.
/// 构建一条不带可选产物的工具终态或检查点更新。
fn tool_completion(
    output: Option<ToolPayload>,
    status: ToolCallStatus,
    finished_at_ms: i64,
) -> ToolCompletion {
    ToolCompletion {
        output,
        changeset: None,
        status,
        finished_at_ms,
        external_refs: Vec::new(),
        generated_resources: Vec::new(),
    }
}

/// Completes one API group successfully with an optional provider source.
/// 以可选 provider_source 成功闭合一个 API 组。
fn complete_ok(
    doc: &mut SessionDocument,
    api_call_id: ApiCallId,
    provider_source: Option<ProviderSourceInput>,
    now_ms: i64,
) -> Result<Seq, DomainError> {
    doc.complete_api_group(
        api_call_id,
        ApiCompletion {
            request: ApiRequestSnapshot::new("openai", "gpt-5"),
            usage: usage(10, 20),
            status: ApiCallStatus::Completed,
            end_reason: Some(ApiEndReason::CompletedSuccessfully),
            error: None,
            continuation_eligibility: ContinuationEligibility::Forbidden,
            timing: ApiTimings::default(),
            provider_source,
            completed_at_ms: now_ms,
        },
    )
    .map(|receipt| receipt.stats_seq)
}

/// Completes one physical API as an explicit user interruption eligible for Continue.
/// 将一个物理 API 以允许“继续”的明确用户中止终态闭合。
fn complete_user_interrupted(
    doc: &mut SessionDocument,
    api_call_id: ApiCallId,
    now_ms: i64,
) -> Result<Seq, DomainError> {
    doc.complete_api_group(
        api_call_id,
        ApiCompletion {
            request: ApiRequestSnapshot::new("openai", "gpt-5"),
            usage: UsageSnapshot::default(),
            status: ApiCallStatus::Interrupted,
            end_reason: Some(ApiEndReason::UserInterrupted),
            error: None,
            continuation_eligibility: ContinuationEligibility::Allowed,
            timing: ApiTimings::default(),
            provider_source: None,
            completed_at_ms: now_ms,
        },
    )
    .map(|receipt| receipt.stats_seq)
}

/// Builds one redacted terminal error whose category matches the stable end reason.
/// 构建一条错误类别与稳定结束原因匹配的脱敏终态错误。
fn terminal_error(reason: ApiEndReason) -> RedactedApiError {
    let category = match reason {
        ApiEndReason::ProviderError => ApiErrorCategory::Provider,
        ApiEndReason::NetworkError => ApiErrorCategory::Network,
        ApiEndReason::BalanceInsufficient => ApiErrorCategory::Balance,
        ApiEndReason::RetryExhausted => ApiErrorCategory::Unknown,
        ApiEndReason::Unknown => ApiErrorCategory::Unknown,
        ApiEndReason::CompletedSuccessfully
        | ApiEndReason::UserInterrupted
        | ApiEndReason::ProcessInterrupted
        | ApiEndReason::Cancelled => ApiErrorCategory::Unknown,
    };
    RedactedApiError {
        category,
        error_code: None,
        message: String::from("redacted test failure"),
    }
}

/// Returns the explicit continuation policy used by one terminal test fixture.
/// 返回一条终态测试夹具使用的显式继续策略。
fn continuation_eligibility_for_test(reason: ApiEndReason) -> ContinuationEligibility {
    match reason {
        ApiEndReason::UserInterrupted
        | ApiEndReason::ProcessInterrupted
        | ApiEndReason::NetworkError
        | ApiEndReason::BalanceInsufficient
        | ApiEndReason::RetryExhausted
        | ApiEndReason::ProviderError => ContinuationEligibility::Allowed,
        ApiEndReason::CompletedSuccessfully | ApiEndReason::Cancelled | ApiEndReason::Unknown => {
            ContinuationEligibility::Forbidden
        }
    }
}

/// Builds the exact request snapshot used by replay-source lifecycle tests.
/// 构建重放来源生命周期测试使用的精确请求快照。
fn replay_request() -> ApiRequestSnapshot {
    ApiRequestSnapshot {
        provider: String::from("openai"),
        profile: Some(String::from("responses")),
        model: String::from("gpt-5"),
        connection_id: Some(String::from("connection-1")),
        thinking_level: Some(String::from("high")),
        temperature: Some(0.2),
    }
}

/// Builds one deterministic typed replay source for an exact message identity.
/// 为精确消息身份构建一份确定性的强类型重放来源。
fn replay_source(message_id: &str) -> ProviderSourceInput {
    ProviderSourceInput {
        provider: String::from("openai"),
        profile: Some(String::from("responses")),
        model: String::from("gpt-5"),
        connection_id: Some(String::from("connection-1")),
        message_id: Some(message_id.to_string()),
        response_id: Some(String::from("response-1")),
        replay: Some(ProviderReplayEnvelope::new(
            "openai",
            "connection-1",
            "https://provider.invalid/v1/responses",
            "responses",
            "gpt-5",
            Vec::new(),
            Vec::new(),
        )),
    }
}

/// Builds one Prepared tool input for transactional Provider-batch tests.
/// 为事务化 Provider 批次测试构建一份 Prepared 工具输入。
fn prepared_tool(name: &str, provider_tool_call_id: &str) -> ToolPreparation {
    ToolPreparation {
        request_stream_id: RequestStreamId::try_new(format!("stream-{provider_tool_call_id}"))
            .unwrap(),
        tool_name: name.to_string(),
        input: ToolPayload::try_new(serde_json::json!({"value": name}))
            .expect("valid tool payload"),
        provider_tool_call_id: Some(provider_tool_call_id.to_string()),
    }
}

/// Completes one API group with the exact supplied usage facts.
/// 使用精确提供的用量事实闭合一个 API 组。
fn complete_with_usage(
    document: &mut SessionDocument,
    api_call_id: ApiCallId,
    usage_snapshot: UsageSnapshot,
    now_ms: i64,
) -> Result<Seq, DomainError> {
    document
        .complete_api_group(
            api_call_id,
            ApiCompletion {
                request: ApiRequestSnapshot::new("openai", "gpt-5"),
                usage: usage_snapshot,
                status: ApiCallStatus::Completed,
                end_reason: Some(ApiEndReason::CompletedSuccessfully),
                error: None,
                continuation_eligibility: ContinuationEligibility::Forbidden,
                timing: ApiTimings::default(),
                provider_source: None,
                completed_at_ms: now_ms,
            },
        )
        .map(|receipt| receipt.stats_seq)
}

/// Marks the current in-memory revision as durably published for domain fixtures.
/// 为领域夹具把当前内存修订标记为已经可靠发布。
fn advance_fixture_durable(document: &mut SessionDocument) -> Result<(), DomainError> {
    let revision = document
        .prepare_flush_candidates()?
        .ok_or(DomainError::InvalidStatistics {
            field: "fixture_flush_candidate",
        })?
        .revision;
    document.advance_durable(revision)
}

/// Builds a durable child with one unknown-usage tool API and one final text API.
/// 构建一个包含未知用量工具 API 与最终正文 API 的可靠子会话。
fn durable_child_return_fixture() -> Result<(SessionDocument, TurnId, Seq), DomainError> {
    let mut child = SessionDocument::new("child-session", "project-1", "child", T0);
    let turn_id = child.submit_root_input_fixture("delegate", Vec::new(), T0)?;

    let tool_api = child.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 1,
    )?;
    let tool_call_id = child.start_tool(
        tool_api,
        "file",
        ToolPayload::try_new(serde_json::json!({"path": "child.txt"}))?,
        Some("provider-child-tool".to_string()),
        T0 + 2,
    )?;
    child.finish_tool(
        tool_api,
        tool_call_id,
        tool_completion(
            Some(ToolPayload::try_new(serde_json::json!({"ok": true}))?),
            ToolCallStatus::Completed,
            T0 + 3,
        ),
    )?;
    complete_with_usage(&mut child, tool_api, UsageSnapshot::default(), T0 + 4)?;

    let final_api = child.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 5,
    )?;
    let source_assistant_text_seq = child
        .append_assistant_text(
            final_api,
            "final child result",
            AssistantSemanticStatus::Completed,
            T0 + 6,
        )?
        .into_value();
    complete_with_usage(&mut child, final_api, usage(3, 4), T0 + 7)?;
    child.close_turn(TurnOutcome::Completed, 8, T0 + 8)?;
    advance_fixture_durable(&mut child)?;
    Ok((child, turn_id, source_assistant_text_seq))
}

/// 20,000 mixed events keep a fully contiguous seq stream.
/// 20,000 条混合事件保持完全连续的 seq 流。
#[test]
fn ten_thousand_mixed_events_stay_contiguous() {
    let mut doc = new_document();
    for index in 0..2_500u64 {
        doc.submit_root_input_fixture(format!("prompt {index}"), Vec::new(), T0 + index as i64)
            .unwrap();
        let api = doc
            .start_api(
                ApiPurposeKind::MainGeneration,
                crate::v1::event::ApiRequestSnapshot::new("openai", "gpt-5"),
                0,
            )
            .unwrap();
        doc.append_reasoning(
            api,
            format!("think {index}"),
            AssistantSemanticStatus::Completed,
            T0 + index as i64,
        )
        .unwrap()
        .into_value();
        doc.append_assistant_text(
            api,
            format!("answer {index}"),
            AssistantSemanticStatus::Completed,
            T0 + index as i64,
        )
        .unwrap()
        .into_value();
        let tool = doc
            .start_tool(
                api,
                "file_edit",
                ToolPayload::try_new(serde_json::json!({"a": 1})).unwrap(),
                Some("pt1".to_string()),
                T0 + index as i64,
            )
            .unwrap();
        doc.finish_tool(
            api,
            tool,
            tool_completion(
                Some(ToolPayload::try_new(serde_json::json!({"ok": true})).unwrap()),
                ToolCallStatus::Completed,
                T0 + index as i64,
            ),
        )
        .unwrap()
        .into_value();
        complete_ok(&mut doc, api, None, T0 + index as i64).unwrap();
        doc.close_turn(TurnOutcome::Completed, 5, T0 + index as i64)
            .unwrap();
    }
    assert_eq!(doc.events().len(), 2_500 * 8);
    let mut expected = 1u64;
    for event in doc.events() {
        assert_eq!(event.seq.value(), expected, "seq must stay contiguous");
        expected += 1;
    }
    assert_eq!(doc.id_frontier().next_seq, expected);
}

/// 1,000 status updates of the same tool never allocate a new seq.
/// 同一个工具的 1,000 次状态更新绝不分配新 seq。
#[test]
fn one_thousand_tool_updates_keep_one_seq() {
    let mut doc = new_document();
    doc.submit_root_input_fixture("do it", Vec::new(), T0)
        .unwrap();
    let api = doc
        .start_api(
            ApiPurposeKind::MainGeneration,
            crate::v1::event::ApiRequestSnapshot::new("openai", "gpt-5"),
            0,
        )
        .unwrap();
    let tool = doc
        .start_tool(
            api,
            "shell",
            ToolPayload::try_new(serde_json::json!({})).unwrap(),
            None,
            T0,
        )
        .unwrap();
    let seq = doc
        .tool_seq_by_id
        .get(&tool)
        .copied()
        .expect("tool must be indexed");
    for index in 0..1_000u64 {
        doc.update_running_tool_output(
            api,
            tool,
            Some(ToolPayload::try_new(serde_json::json!({"progress": index})).unwrap()),
            T0 + index as i64,
        )
        .unwrap();
    }
    assert_eq!(
        doc.tool_seq_by_id.get(&tool).copied(),
        Some(seq),
        "tool seq must never change"
    );
    let tool_event_count = doc
        .events()
        .iter()
        .filter(|event| matches!(&event.event, EventBody::ToolCall(t) if t.tool_call_id == tool))
        .count();
    assert_eq!(tool_event_count, 1, "tool must exist as exactly one event");
}

/// Parallel tool completions never reorder cards because each completion
/// updates the tool's original event in place.
/// 并行工具完成绝不重排卡片，因为每次完成都原位更新工具原事件。
#[test]
fn parallel_tool_finishes_keep_card_order() {
    let mut doc = new_document();
    doc.submit_root_input_fixture("run tools", Vec::new(), T0)
        .unwrap();
    let api = doc
        .start_api(
            ApiPurposeKind::MainGeneration,
            crate::v1::event::ApiRequestSnapshot::new("openai", "gpt-5"),
            0,
        )
        .unwrap();
    let first = doc
        .start_tool(
            api,
            "shell",
            ToolPayload::try_new(serde_json::json!({"n": 1})).unwrap(),
            None,
            T0,
        )
        .unwrap();
    let second = doc
        .start_tool(
            api,
            "file_read",
            ToolPayload::try_new(serde_json::json!({"n": 2})).unwrap(),
            None,
            T0,
        )
        .unwrap();
    let third = doc
        .start_tool(
            api,
            "web",
            ToolPayload::try_new(serde_json::json!({"n": 3})).unwrap(),
            None,
            T0,
        )
        .unwrap();
    doc.finish_tool(
        api,
        third,
        tool_completion(
            Some(ToolPayload::try_new(serde_json::json!({"ok": true})).unwrap()),
            ToolCallStatus::Completed,
            T0 + 1,
        ),
    )
    .unwrap();
    doc.finish_tool(
        api,
        first,
        tool_completion(
            Some(ToolPayload::try_new(serde_json::json!({"ok": true})).unwrap()),
            ToolCallStatus::Completed,
            T0 + 2,
        ),
    )
    .unwrap();
    doc.finish_tool(
        api,
        second,
        tool_completion(
            Some(ToolPayload::try_new(serde_json::json!({"ok": true})).unwrap()),
            ToolCallStatus::Completed,
            T0 + 3,
        ),
    )
    .unwrap();
    let tool_ids_in_stream: Vec<u64> = doc
        .events()
        .iter()
        .filter_map(|event| match &event.event {
            EventBody::ToolCall(tool) => Some(tool.tool_call_id.value()),
            _ => None,
        })
        .collect();
    assert_eq!(
        tool_ids_in_stream,
        vec![first.value(), second.value(), third.value()]
    );
}

/// Abort then continue keeps the same turn id and never creates a fake user
/// prompt.
/// 中止后继续保持同一 turn id，且绝不创建假的用户提示词。
#[test]
fn abort_continue_keeps_turn_id() {
    let mut doc = new_document();
    let turn_id = doc
        .submit_root_input_fixture("first", Vec::new(), T0)
        .unwrap();
    let api = doc
        .start_api(
            ApiPurposeKind::MainGeneration,
            crate::v1::event::ApiRequestSnapshot::new("openai", "gpt-5"),
            0,
        )
        .unwrap();
    doc.append_assistant_text(api, "partial", AssistantSemanticStatus::Interrupted, T0)
        .unwrap();
    complete_user_interrupted(&mut doc, api, T0 + 1).unwrap();
    doc.continue_turn_fixture(T0 + 2).unwrap();
    let api2 = doc
        .start_api(
            ApiPurposeKind::Continue,
            crate::v1::event::ApiRequestSnapshot::new("openai", "gpt-5"),
            0,
        )
        .unwrap();
    doc.append_assistant_text(api2, "resumed", AssistantSemanticStatus::Completed, T0 + 2)
        .unwrap();
    complete_ok(&mut doc, api2, None, T0 + 3).unwrap();
    doc.close_turn(TurnOutcome::Completed, 10, T0 + 3).unwrap();
    assert_eq!(doc.active_turn().map(|turn| turn.turn_id), Some(turn_id));
    let user_inputs: Vec<&str> = doc
        .events()
        .iter()
        .filter_map(|event| match &event.event {
            EventBody::UserInput(input) => Some(input.text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        user_inputs,
        vec!["first"],
        "continue must not fake a prompt"
    );
    let resume_controls = doc
        .events()
        .iter()
        .filter(|event| {
            matches!(
                &event.event,
                EventBody::ExecutionControl(control)
                    if control.control_kind == ExecutionControlKind::TurnResumed
            )
        })
        .count();
    assert_eq!(resume_controls, 1);
}

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

/// The allocator rejects ids beyond the JavaScript-safe maximum.
/// 分配器拒绝超过 JavaScript 安全上限的 id。
#[test]
fn js_safe_overflow_is_rejected() {
    let mut allocator = IdAllocator::from_frontier(2, JS_SAFE_INTEGER_MAX);
    // The frontier value itself is still allocatable; the next call must fail.
    // frontier 值本身仍然可分配；下一次调用必须失败。
    assert!(allocator.next_seq().is_ok());
    assert!(allocator.next_seq().is_err());
    let mut local = IdAllocator::from_frontier(JS_SAFE_INTEGER_MAX, 1);
    assert!(local.next_local_id().is_ok());
    assert!(local.next_local_id().is_err());
}

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

/// Goal and Plan public transitions reject invalid states without changing any document field.
/// Goal 与 Plan 公开转换在无效状态下会拒绝，且不修改任何文档字段。
#[test]
fn goal_and_plan_transition_errors_are_failure_atomic() {
    let mut document = new_document();
    assert!(
        document
            .set_plan_mode(
                PlanModeTransitionInput::try_new(true, "enter plan").unwrap(),
                T0,
            )
            .expect("enable plan")
    );
    let before_create = document.clone();
    assert!(matches!(
        document.create_goal("goal", Some(10), "goal context", T0 + 1),
        Err(DomainError::ModeConflict { .. })
    ));
    assert_eq!(document, before_create);

    assert!(
        document
            .set_plan_mode(
                PlanModeTransitionInput::try_new(false, "leave plan").unwrap(),
                T0 + 2,
            )
            .expect("disable plan")
    );
    for transition in ["update", "clear", "pause", "resume"] {
        let before = document.clone();
        let result = match transition {
            "update" => document
                .update_goal_definition("goal", Some(10), "goal context", T0 + 3)
                .map(|_| ()),
            "clear" => document.clear_goal("goal removed", T0 + 3).map(|_| ()),
            "pause" => document.pause_goal("goal paused", T0 + 3).map(|_| ()),
            "resume" => document
                .resume_goal("goal", Some(10), "goal context", T0 + 3)
                .map(|_| ()),
            _ => unreachable!("transition table is closed"),
        };
        assert!(matches!(
            result,
            Err(DomainError::GoalEnablementMismatch { .. })
        ));
        assert_eq!(document, before);
    }
}

/// Goal create, update, pause, resume, and clear publish complete atomic fact groups.
/// Goal 的创建、更新、暂停、恢复与清除都发布完整的原子事实组。
#[test]
fn goal_lifecycle_transitions_publish_complete_fact_groups() -> Result<(), DomainError> {
    let mut document = new_document();
    let baseline_revision = document.memory_revision();
    let created = document.create_goal("goal one", Some(100), "goal one context", T0)?;
    assert!(created.continuation_turn_id.is_some());
    assert_eq!(document.metadata.mode.goal.state, GoalState::Active);
    assert_eq!(document.events().len(), 2);
    assert_eq!(document.memory_revision(), baseline_revision + 1);
    assert_eq!(
        document.dirty_files().collect::<BTreeSet<_>>(),
        BTreeSet::from([DirtyFile::Metadata, DirtyFile::ActiveTail])
    );

    let unchanged_revision = document.memory_revision();
    assert!(
        document
            .update_goal_definition("goal one", Some(100), "ignored no-op context", T0 + 1)?
            .is_none()
    );
    assert_eq!(document.memory_revision(), unchanged_revision);

    let updated = document
        .update_goal_definition("goal two", Some(200), "goal two context", T0 + 2)?
        .expect("changed goal definition");
    assert!(updated.context_seq.is_some());
    assert_eq!(document.metadata.mode.goal.objective, "goal two");

    let before_empty_pause = document.clone();
    assert!(document.pause_goal("", T0 + 3).is_err());
    assert_eq!(document, before_empty_pause);
    document.pause_goal("goal paused", T0 + 3)?;
    assert_eq!(document.metadata.mode.goal.state, GoalState::Paused);
    document.resume_goal("goal two", Some(200), "goal resumed", T0 + 4)?;
    assert_eq!(document.metadata.mode.goal.state, GoalState::Active);
    document.clear_goal("goal removed", T0 + 5)?;
    assert_eq!(document.metadata.mode.goal, GoalConfig::default());
    Ok(())
}

/// Saving over a completed Goal creates a fresh active identity and resets its accounting origin.
/// 在已完成 Goal 上保存会创建新的活动身份，并重置其记账起点。
#[test]
fn save_goal_replaces_completed_generation_atomically() -> Result<(), DomainError> {
    let mut document = new_document();
    let first = document
        .save_goal("goal one", Some(100), "goal one context", T0)?
        .expect("first save must create a Goal");
    let first_origin_seq = document
        .metadata
        .mode
        .goal
        .origin_seq
        .expect("created Goal must own an origin seq");
    let first_turn_id = document
        .metadata
        .mode
        .goal
        .origin_turn_id
        .expect("created Goal must own an origin turn");
    assert_eq!(first.context_seq, Some(first_origin_seq));

    document.metadata.mode.goal.state = GoalState::Completed;
    document.metadata.mode.goal.budget_cache = Some(crate::v1::config::GoalBudgetCache {
        covered_until_seq: first.notice_seq,
        direct_total_tokens: 40,
        subagent_total_tokens: 2,
        total_tokens: 42,
        updated_at_ms: T0 + 1,
    });
    document
        .active_turn
        .as_mut()
        .expect("created Goal must own an active turn")
        .closed = true;
    document.metadata.open_turn_start_seq = None;

    let baseline_event_count = document.events().len();
    let baseline_revision = document.memory_revision();
    let replacement = document
        .save_goal("goal two", Some(200), "goal two context", T0 + 2)?
        .expect("completed Goal must be replaced even when save is otherwise valid");
    let replacement_origin_seq = document
        .metadata
        .mode
        .goal
        .origin_seq
        .expect("replacement Goal must own a new origin seq");
    let replacement_turn_id = document
        .metadata
        .mode
        .goal
        .origin_turn_id
        .expect("replacement Goal must own a new origin turn");

    assert_ne!(replacement_origin_seq, first_origin_seq);
    assert_ne!(replacement_turn_id, first_turn_id);
    assert_eq!(replacement.context_seq, Some(replacement_origin_seq));
    assert_eq!(replacement.continuation_turn_id, Some(replacement_turn_id));
    assert_eq!(document.metadata.mode.goal.objective, "goal two");
    assert_eq!(document.metadata.mode.goal.budget_tokens, Some(200));
    assert_eq!(document.metadata.mode.goal.state, GoalState::Active);
    assert!(document.metadata.mode.goal.budget_cache.is_none());
    assert_eq!(document.events().len(), baseline_event_count + 2);
    assert_eq!(document.memory_revision(), baseline_revision + 1);
    assert!(matches!(
        &document.events()[baseline_event_count].event,
        EventBody::SystemContext(SystemContextEvent {
            operation: SystemContextOperation::Upsert,
            content,
            ..
        }) if content == "goal two context"
    ));
    assert!(matches!(
        &document.events()[baseline_event_count + 1].event,
        EventBody::DisplayNote(DisplayNoteEvent { reason_code, .. }) if reason_code == "goal_saved"
    ));
    Ok(())
}

/// Every completed-Goal replacement error leaves the complete document unchanged.
/// 已完成 Goal 替换的任何错误都会保持完整文档不变。
#[test]
fn save_goal_completed_replacement_errors_are_failure_atomic() -> Result<(), DomainError> {
    let mut document = new_document();
    document
        .save_goal("goal one", Some(100), "goal one context", T0)?
        .expect("first save must create a Goal");
    document.metadata.mode.goal.state = GoalState::Completed;
    document
        .active_turn
        .as_mut()
        .expect("created Goal must own an active turn")
        .closed = true;
    document.metadata.open_turn_start_seq = None;

    let before_empty = document.clone();
    assert!(
        document
            .save_goal("   ", Some(200), "goal two context", T0 + 1)
            .is_err()
    );
    assert_eq!(document, before_empty);

    let (_, next_seq) = document.id_allocator.frontier();
    document.id_allocator = IdAllocator::from_frontier(JS_SAFE_INTEGER_MAX + 1, next_seq);
    let before_allocator_failure = document.clone();
    assert!(
        document
            .save_goal("goal two", Some(200), "goal two context", T0 + 2)
            .is_err()
    );
    assert_eq!(document, before_allocator_failure);
    Ok(())
}

/// Plan mode atomically couples metadata with its system semantic and remains idempotent.
/// Plan 模式原子绑定 metadata 与系统语义，同时保持幂等。
#[test]
fn set_plan_mode_is_atomic_idempotent_and_goal_exclusive() -> Result<(), DomainError> {
    let mut document = new_document();
    let baseline_revision = document.memory_revision();
    let before_empty = document.clone();
    assert!(PlanModeTransitionInput::try_new(true, "   ").is_err());
    assert_eq!(document, before_empty);
    assert!(document.set_plan_mode(PlanModeTransitionInput::try_new(true, "enter plan")?, T0)?);
    assert!(matches!(
        &document.events()[0].event,
        EventBody::SystemContext(SystemContextEvent {
            context_key,
            operation: SystemContextOperation::Upsert,
            scope: SystemContextScope::Session,
            provider_role_hint: Some(SystemRoleHint::Developer),
            content,
            ..
        }) if context_key == "plan_mode" && content == "enter plan"
    ));
    assert_eq!(document.memory_revision(), baseline_revision + 1);
    assert_eq!(
        document.dirty_files().collect::<BTreeSet<_>>(),
        BTreeSet::from([DirtyFile::Metadata, DirtyFile::ActiveTail])
    );
    let unchanged_revision = document.memory_revision();
    assert!(!document.set_plan_mode(
        PlanModeTransitionInput::try_new(true, "duplicate enter")?,
        T0 + 1,
    )?);
    assert_eq!(document.memory_revision(), unchanged_revision);

    assert!(document.set_plan_mode(
        PlanModeTransitionInput::try_new(false, "leave plan")?,
        T0 + 2,
    )?);
    assert!(matches!(
        &document.events()[1].event,
        EventBody::SystemContext(SystemContextEvent {
            context_key,
            operation: SystemContextOperation::Remove,
            content,
            ..
        }) if context_key == "plan_mode" && content == "leave plan"
    ));
    document.create_goal("goal", None, "goal context", T0 + 3)?;
    let before_conflict = document.clone();
    assert!(matches!(
        document.set_plan_mode(
            PlanModeTransitionInput::try_new(true, "enter plan").unwrap(),
            T0 + 4,
        ),
        Err(DomainError::ModeConflict { .. })
    ));
    assert_eq!(document, before_conflict);

    let mut exhausted = new_document();
    exhausted.id_allocator = IdAllocator::from_frontier(1, JS_SAFE_INTEGER_MAX + 1);
    let before_capacity_error = exhausted.clone();
    assert!(
        exhausted
            .set_plan_mode(
                PlanModeTransitionInput::try_new(true, "enter plan").unwrap(),
                T0,
            )
            .is_err()
    );
    assert_eq!(exhausted, before_capacity_error);
    Ok(())
}

/// A completed provider-sourced group carries exactly one provider_source and
/// never allows a second one.
/// 已完成的 Provider 来源组恰好携带一个 provider_source，绝不允许第二个。
#[test]
fn provider_source_exactly_once() {
    let mut doc = new_document();
    doc.submit_root_input_fixture("go", Vec::new(), T0).unwrap();
    let api = doc
        .start_api(
            ApiPurposeKind::MainGeneration,
            crate::v1::event::ApiRequestSnapshot::new("openai", "gpt-5"),
            0,
        )
        .unwrap();
    let _text_seq = doc
        .append_assistant_text(api, "t", AssistantSemanticStatus::Completed, T0)
        .unwrap();
    let source = ProviderSourceInput {
        provider: "openai".to_string(),
        profile: None,
        model: "gpt-5".to_string(),
        connection_id: None,
        message_id: Some("m1".to_string()),
        response_id: None,
        replay: None,
    };
    complete_ok(&mut doc, api, Some(source), T0 + 1).unwrap();
    let placed = doc
        .events()
        .iter()
        .filter(|event| matches!(&event.event, EventBody::AssistantText(t) if t.provider_source.is_some()))
        .count();
    assert_eq!(placed, 1);
    // A completed group closes; the next API group starts on fresh facts.
    // 已完成的组合闭；下一个 API 组在全新事实上启动。
    assert!(
        doc.start_api(
            ApiPurposeKind::MainGeneration,
            crate::v1::event::ApiRequestSnapshot::new("openai", "gpt-5"),
            0
        )
        .is_ok()
    );
}

/// Provider source on a group without Provider-sourced events is rejected.
/// 无 Provider 来源事件的组被拒绝放置 provider_source。
#[test]
fn provider_source_requires_provider_events() {
    let mut doc = new_document();
    doc.submit_root_input_fixture("fail", Vec::new(), T0)
        .unwrap();
    let api = doc
        .start_api(
            ApiPurposeKind::MainGeneration,
            crate::v1::event::ApiRequestSnapshot::new("openai", "gpt-5"),
            0,
        )
        .unwrap();
    let source = ProviderSourceInput {
        provider: "openai".to_string(),
        profile: None,
        model: "gpt-5".to_string(),
        connection_id: None,
        message_id: None,
        response_id: None,
        replay: None,
    };
    let result = doc.complete_api_group(
        api,
        ApiCompletion {
            request: ApiRequestSnapshot::new("openai", "gpt-5"),
            usage: UsageSnapshot::default(),
            status: ApiCallStatus::Failed,
            end_reason: Some(ApiEndReason::ProviderError),
            error: Some(terminal_error(ApiEndReason::ProviderError)),
            continuation_eligibility: ContinuationEligibility::Forbidden,
            timing: ApiTimings::default(),
            provider_source: Some(source),
            completed_at_ms: T0,
        },
    );
    assert!(matches!(
        result,
        Err(DomainError::ReferenceViolation { .. })
    ));
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
        events: doc.events().to_vec(),
        tool_seq_by_id: HashMap::new(),
        active_turn: None,
        active_api: None,
        id_allocator: IdAllocator::new(),
        memory_revision: 0,
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
        events,
        tool_seq_by_id: HashMap::new(),
        active_turn: None,
        active_api: None,
        id_allocator: IdAllocator::new(),
        memory_revision: 0,
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

/// Turn statistics aggregate direct APIs and final subagent returns.
/// 回合统计聚合 direct API 与最终子代理回流。
#[test]
fn turn_stats_aggregate_direct_and_subagent() -> Result<(), DomainError> {
    let mut doc = new_document();
    let turn_id = doc.submit_root_input_fixture("multi", Vec::new(), T0)?;
    let api = doc.start_api(
        ApiPurposeKind::MainGeneration,
        crate::v1::event::ApiRequestSnapshot::new("openai", "gpt-5"),
        0,
    )?;
    doc.append_assistant_text(api, "answer", AssistantSemanticStatus::Completed, T0)?;
    complete_ok(&mut doc, api, None, T0 + 1)?;
    let (child, child_turn_id, source_assistant_text_seq) = durable_child_return_fixture()?;
    let input = child.build_subagent_return_input(SubagentReturnBuildRequest {
        final_turn_id: child_turn_id,
        child_task_id: SubagentTaskId::try_new("task-1")?,
        source_assistant_text_seq,
    })?;
    assert!(matches!(
        doc.append_subagent_return_with_stats(input, T0 + 2)?,
        IdempotentMutation::Committed(_)
    ));
    doc.close_turn(TurnOutcome::Completed, 5, T0 + 3)?;
    let (stats_turn_id, stats) = doc
        .events()
        .iter()
        .find_map(|event| match &event.event {
            EventBody::TurnStats(stats) => Some((event.turn_id, stats)),
            _ => None,
        })
        .expect("turn stats must exist");
    assert_eq!(stats.buckets.direct.api_call_count, 1);
    assert_eq!(stats.buckets.subagent.api_call_count, 2);
    assert_eq!(stats.buckets.total.api_call_count, 3);
    assert_eq!(stats.buckets.subagent.unknown_usage_api_call_count, 1);
    assert_eq!(stats.buckets.total.unknown_usage_api_call_count, 1);
    assert_eq!(stats.buckets.subagent.tool_call_count, 1);
    assert_eq!(stats.buckets.total.tool_call_count, 1);
    assert_eq!(stats.buckets.total.usage.input_tokens, None);
    assert_eq!(stats.buckets.total.usage.output_tokens, None);
    assert_eq!(stats.buckets.total.subagent_return_count, 1);
    assert_eq!(stats_turn_id, Some(turn_id));
    Ok(())
}

/// Turn statistics count Provider and tool work physically owned by a child TaskSession.
/// 回合统计会计入子 TaskSession 物理持有的 Provider 与工具工作。
#[test]
fn child_turn_stats_include_local_subagent_purpose_work() -> Result<(), DomainError> {
    let mut child = SessionDocument::new("child-local-session", "project-1", "child", T0);
    child.submit_root_input_fixture("run locally", Vec::new(), T0)?;
    let tool_api = child.start_api(
        ApiPurposeKind::Subagent,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 1,
    )?;
    let tool_call_id = child.start_tool(
        tool_api,
        "file",
        ToolPayload::try_new(serde_json::json!({"path": "child.txt"}))?,
        Some("provider-child-local-tool".to_string()),
        T0 + 2,
    )?;
    child.finish_tool(
        tool_api,
        tool_call_id,
        tool_completion(
            Some(ToolPayload::try_new(serde_json::json!({"ok": true}))?),
            ToolCallStatus::Completed,
            T0 + 3,
        ),
    )?;
    complete_ok(&mut child, tool_api, None, T0 + 4)?;
    let final_api = child.start_api(
        ApiPurposeKind::Subagent,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 5,
    )?;
    child.append_assistant_text(
        final_api,
        "done",
        AssistantSemanticStatus::Completed,
        T0 + 6,
    )?;
    complete_ok(&mut child, final_api, None, T0 + 7)?;
    child.close_turn(TurnOutcome::Completed, 8, T0 + 8)?;

    let stats = child
        .events()
        .iter()
        .find_map(|event| match &event.event {
            EventBody::TurnStats(stats) => Some(stats),
            _ => None,
        })
        .expect("child turn stats must exist");
    assert_eq!(stats.buckets.direct.api_call_count, 2);
    assert_eq!(stats.buckets.direct.tool_call_count, 1);
    assert_eq!(stats.buckets.total.api_call_count, 2);
    assert_eq!(stats.buckets.total.tool_call_count, 1);
    assert_eq!(stats.buckets.total.usage.input_tokens, Some(20));
    assert_eq!(stats.buckets.total.usage.output_tokens, Some(40));
    Ok(())
}

/// Truncating a historical tail keeps the retained seqs contiguous.
/// 截断历史尾部后保留的 seq 仍然连续。
#[test]
fn truncate_after_keeps_contiguity() {
    let mut doc = new_document();
    doc.submit_root_input_fixture("a", Vec::new(), T0).unwrap();
    let api_a = doc
        .start_api(
            ApiPurposeKind::MainGeneration,
            crate::v1::event::ApiRequestSnapshot::new("openai", "gpt-5"),
            0,
        )
        .unwrap();
    doc.append_assistant_text(api_a, "a1", AssistantSemanticStatus::Completed, T0)
        .unwrap();
    complete_ok(&mut doc, api_a, None, T0 + 1).unwrap();
    doc.close_turn(TurnOutcome::Completed, 1, T0 + 1).unwrap();
    doc.submit_root_input_fixture("b", Vec::new(), T0 + 2)
        .unwrap();
    let kept = doc.events().len();
    doc.truncate_after(Seq::try_new(kept as u64).unwrap(), T0 + 10)
        .unwrap();
    let mut expected = 1u64;
    for event in doc.events() {
        assert_eq!(event.seq.value(), expected);
        expected += 1;
    }
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

/// Dirty files and revisions track the writer contract exactly.
/// dirty 文件与修订号精确跟踪写入器契约。
#[test]
fn revision_and_dirty_tracking() {
    let mut doc = new_document();
    let initial = doc.memory_revision();
    doc.submit_root_input_fixture("a", Vec::new(), T0).unwrap();
    assert!(doc.memory_revision() > initial);
    assert!(doc.dirty_files().any(|file| file == DirtyFile::ActiveTail));
    let snapshot_revision = doc.memory_revision();
    let candidate = doc
        .prepare_flush_candidates()
        .unwrap()
        .expect("a dirty tail must yield a candidate");
    assert_eq!(candidate.revision, snapshot_revision);
    doc.advance_durable(candidate.revision).unwrap();
    assert_eq!(doc.durable_revision(), snapshot_revision);
    assert!(!doc.dirty_files().any(|file| file == DirtyFile::ActiveTail));
    doc.add_pending_in_turn_input_fixture("later", Vec::new(), T0 + 1)
        .unwrap();
    assert!(doc.dirty_files().any(|file| file == DirtyFile::Metadata));
    assert!(
        !doc.dirty_files().any(|file| file == DirtyFile::ActiveTail),
        "an input-state update alone must never mark the tail dirty"
    );
}

/// Completing a turn and creating its active Goal successor is atomic and
/// receipt-free exactly-once by the completed turn id.
/// 完成回合并创建其活动 Goal 后继回合，以已完成 turn id 实现无 receipt 的原子
/// exactly-once。
#[test]
fn goal_continuation_exactly_once() -> Result<(), DomainError> {
    let mut doc = new_document();
    doc.metadata.mode.goal.enabled = true;
    doc.metadata.mode.goal.state = GoalState::Active;
    doc.metadata.mode.goal.objective = "objective".to_string();
    let completed_turn = doc.submit_root_input_fixture("work", Vec::new(), T0)?;
    let api = doc.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0,
    )?;
    doc.append_assistant_text(api, "done", AssistantSemanticStatus::Completed, T0)?;
    complete_ok(&mut doc, api, None, T0 + 1)?;

    let transition = doc.complete_turn_and_select_successor(completed_turn, T0 + 2)?;
    let TurnSuccessor::GoalContinuation {
        turn_id: goal_turn, ..
    } = transition.successor
    else {
        panic!("active Goal must open one successor turn");
    };
    assert_eq!(
        doc.active_turn()
            .map(|turn| (turn.turn_id, turn.origin, turn.closed)),
        Some((goal_turn, TurnOrigin::GoalContinuation, false))
    );
    assert_eq!(
        doc.metadata.open_turn_start_seq,
        doc.active_turn().map(|turn| turn.start_seq)
    );
    assert!(matches!(
        doc.events()
            .iter()
            .find(|event| event.seq() == transition.completed_turn_final_seq)
            .map(|event| &event.event),
        Some(EventBody::TurnStats(_))
    ));

    let event_count = doc.events().len();
    let duplicate = doc.complete_turn_and_select_successor(completed_turn, T0 + 3);
    assert!(matches!(
        duplicate,
        Err(DomainError::TurnIdentityMismatch { expected, actual })
            if expected == completed_turn.value() && actual == goal_turn.value()
    ));
    assert_eq!(doc.events().len(), event_count);
    assert_eq!(doc.active_turn().map(|turn| turn.turn_id), Some(goal_turn));
    Ok(())
}

/// Completing a turn with Goal inactive closes it without creating a successor.
/// Goal 未活动时完成回合，只闭合该回合而不创建后继回合。
#[test]
fn complete_turn_without_active_goal_stays_closed() -> Result<(), DomainError> {
    let mut doc = new_document();
    let turn_id = doc.submit_root_input_fixture("work", Vec::new(), T0)?;
    let api = doc.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 1,
    )?;
    doc.append_assistant_text(api, "done", AssistantSemanticStatus::Completed, T0 + 2)?;
    complete_ok(&mut doc, api, None, T0 + 3)?;
    let transition = doc.complete_turn_and_select_successor(turn_id, T0 + 4)?;
    assert_eq!(transition.successor, TurnSuccessor::None);
    assert_eq!(doc.active_turn().map(|turn| turn.closed), Some(true));
    assert!(doc.metadata.open_turn_start_seq.is_none());
    Ok(())
}

/// Goal completion rejects an in-flight API before appending any closure fact.
/// Goal 完成在追加任何闭合事实前拒绝仍在途的 API。
#[test]
fn goal_completion_with_active_api_is_non_mutating() -> Result<(), DomainError> {
    let mut doc = new_document();
    doc.metadata.mode.goal.enabled = true;
    doc.metadata.mode.goal.state = GoalState::Active;
    doc.metadata.mode.goal.objective = "objective".to_string();
    let turn_id = doc.submit_root_input_fixture("work", Vec::new(), T0)?;
    doc.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0,
    )?;
    let before = doc.clone();

    assert!(matches!(
        doc.complete_turn_and_select_successor(turn_id, T0 + 1),
        Err(DomainError::TurnCompletionWhileApiActive {
            turn_id: actual_turn_id,
            ..
        }) if actual_turn_id == turn_id.value()
    ));
    assert_eq!(doc, before);
    Ok(())
}

/// Root input is rejected while a turn is still open.
/// 回合仍然开放时根输入被拒绝。
#[test]
fn root_input_rejected_while_turn_open() {
    let mut doc = new_document();
    doc.submit_root_input_fixture("a", Vec::new(), T0).unwrap();
    let result = doc.submit_root_input_fixture("b", Vec::new(), T0 + 1);
    assert!(matches!(
        result,
        Err(DomainError::RootInputWhileTurnOpen { .. })
    ));
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
        events: doc.events().to_vec(),
        tool_seq_by_id: HashMap::new(),
        active_turn: None,
        active_api: None,
        id_allocator: IdAllocator::new(),
        memory_revision: 0,
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
/// Provider tool events and replay source become durable together before any tool can wait.
/// Provider 工具事件与重放来源在任何工具进入等待前一起可靠形成。
#[test]
fn prepared_tool_source_survives_open_api_reload_and_closes_once() -> Result<(), DomainError> {
    let mut doc = new_document();
    doc.submit_root_input_fixture("run tools", Vec::new(), T0)?;
    let api = doc.start_api(ApiPurposeKind::MainGeneration, replay_request(), T0 + 1)?;

    let event_count_before_failure = doc.events().len();
    let revision_before_failure = doc.memory_revision();
    let frontier_before_failure = doc.id_frontier();
    let mut mismatched_source = replay_source("message-1");
    mismatched_source.model = String::from("wrong-model");
    let mismatch = doc.prepare_tool_batch_with_provider_source(
        api,
        vec![prepared_tool("ask", "provider-tool-1")],
        mismatched_source,
        T0 + 2,
    );
    assert!(matches!(
        mismatch,
        Err(DomainError::ApiRequestMismatch { .. })
    ));
    assert_eq!(doc.events().len(), event_count_before_failure);
    assert_eq!(doc.memory_revision(), revision_before_failure);
    assert_eq!(doc.id_frontier(), frontier_before_failure);

    let source = replay_source("message-1");
    let tools = doc
        .prepare_tool_batch_with_provider_source(
            api,
            vec![
                prepared_tool("ask", "provider-tool-1"),
                prepared_tool("shell_session", "provider-tool-2"),
            ],
            source.clone(),
            T0 + 3,
        )?
        .into_value();
    assert_eq!(tools.tools.len(), 2);
    let tool_ids = tools
        .tools
        .iter()
        .map(|receipt| receipt.tool_call_id)
        .collect::<Vec<_>>();
    let provider_tools = doc
        .events()
        .iter()
        .filter_map(|event| match &event.event {
            EventBody::ToolCall(tool) if tool.api_call_id == api => Some(tool),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(provider_tools.len(), 2);
    assert!(provider_tools[0].provider_source.is_none());
    assert_eq!(
        provider_tools[1]
            .provider_source
            .as_ref()
            .and_then(|placed| placed.message_id.as_deref()),
        Some("message-1")
    );
    assert!(
        doc.events()
            .iter()
            .all(|event| !matches!(event.event, EventBody::ApiStats(_)))
    );

    let event_count_before_duplicate = doc.events().len();
    assert!(matches!(
        doc.prepare_tool_batch_with_provider_source(
            api,
            vec![prepared_tool("file", "provider-tool-3")],
            source.clone(),
            T0 + 4,
        ),
        Err(DomainError::ProviderSourceAlreadySet { .. })
    ));
    assert_eq!(doc.events().len(), event_count_before_duplicate);

    doc.start_prepared_tool(api, tool_ids[0], T0 + 5)?;
    doc.mark_tool_waiting_user(api, tool_ids[0], T0 + 6)?;

    let persisted_metadata = serde_json::from_str(
        &serde_json::to_string(&doc.metadata).expect("metadata must serialize"),
    )
    .expect("metadata must strictly reload");
    let persisted_events =
        serde_json::from_str(&serde_json::to_string(doc.events()).expect("events must serialize"))
            .expect("events must strictly reload");
    let mut restored = SessionDocument::from_parts(
        persisted_metadata,
        doc.runtime_lock.clone(),
        doc.compactions.clone(),
        doc.segment_catalog().to_vec(),
        persisted_events,
    )?;
    assert_eq!(
        restored
            .active_api()
            .map(|active| active.provider_source_placed),
        Some(true)
    );
    assert!(
        restored
            .events()
            .iter()
            .all(|event| !matches!(event.event, EventBody::ApiStats(_)))
    );

    restored.answer_waiting_tool(
        api,
        tool_ids[0],
        tool_completion(
            Some(ToolPayload::try_new(serde_json::json!({"answer": "yes"}))?),
            ToolCallStatus::Completed,
            T0 + 7,
        ),
    )?;
    restored.start_prepared_tool(api, tool_ids[1], T0 + 8)?;
    restored.finish_tool(
        api,
        tool_ids[1],
        tool_completion(
            Some(ToolPayload::try_new(serde_json::json!({"exit_code": 0}))?),
            ToolCallStatus::Completed,
            T0 + 9,
        ),
    )?;
    let event_count_before_duplicate_finish = restored.events().len();
    assert!(matches!(
        restored.finish_tool(
            api,
            tool_ids[1],
            tool_completion(
                Some(ToolPayload::try_new(serde_json::json!({"ok": true}))?),
                ToolCallStatus::Completed,
                T0 + 10,
            ),
        ),
        Err(DomainError::ImmutableFieldMutation {
            entity: "tool_call_terminal",
            field: "output",
        })
    ));
    assert_eq!(restored.events().len(), event_count_before_duplicate_finish);

    let stats_seq = restored
        .complete_api_group(
            api,
            ApiCompletion {
                request: replay_request(),
                usage: usage(10, 20),
                status: ApiCallStatus::Completed,
                end_reason: Some(ApiEndReason::CompletedSuccessfully),
                error: None,
                continuation_eligibility: ContinuationEligibility::Forbidden,
                timing: ApiTimings::default(),
                provider_source: Some(source.clone()),
                completed_at_ms: T0 + 11,
            },
        )?
        .stats_seq;
    assert!(matches!(
        restored
            .events()
            .iter()
            .find(|event| event.seq == stats_seq)
            .map(|event| &event.event),
        Some(EventBody::ApiStats(_))
    ));
    assert_eq!(
        restored
            .events()
            .iter()
            .filter(|event| matches!(event.event, EventBody::ApiStats(_)))
            .count(),
        1
    );
    assert!(matches!(
        restored.complete_api_group(
            api,
            ApiCompletion {
                request: replay_request(),
                usage: usage(10, 20),
                status: ApiCallStatus::Completed,
                end_reason: Some(ApiEndReason::CompletedSuccessfully),
                error: None,
                continuation_eligibility: ContinuationEligibility::Forbidden,
                timing: ApiTimings::default(),
                provider_source: Some(source),
                completed_at_ms: T0 + 12,
            },
        ),
        Err(DomainError::CrossApiGroupUpdate { .. })
    ));
    assert_eq!(
        restored
            .events()
            .iter()
            .filter(|event| matches!(event.event, EventBody::ApiStats(_)))
            .count(),
        1
    );
    Ok(())
}

/// A changed terminal replay identity cannot close a group whose source is already durable.
/// 变更后的终态重放身份不能闭合来源已经可靠落盘的 API 组。
#[test]
fn durable_tool_source_rejects_terminal_identity_drift() -> Result<(), DomainError> {
    let mut doc = new_document();
    doc.submit_root_input_fixture("run tool", Vec::new(), T0)?;
    let api = doc.start_api(ApiPurposeKind::MainGeneration, replay_request(), T0 + 1)?;
    let source = replay_source("message-1");
    let tools = doc
        .prepare_tool_batch_with_provider_source(
            api,
            vec![prepared_tool("file", "provider-tool-1")],
            source,
            T0 + 2,
        )?
        .into_value();
    let tool_id = tools.tools[0].tool_call_id;
    doc.start_prepared_tool(api, tool_id, T0 + 3)?;
    doc.finish_tool(
        api,
        tool_id,
        tool_completion(
            Some(ToolPayload::try_new(serde_json::json!({"ok": true}))?),
            ToolCallStatus::Completed,
            T0 + 4,
        ),
    )?;
    let mut changed_source = replay_source("message-2");
    changed_source.response_id = Some(String::from("response-2"));
    let event_count = doc.events().len();
    assert!(matches!(
        doc.complete_api_group(
            api,
            ApiCompletion {
                request: replay_request(),
                usage: usage(10, 20),
                status: ApiCallStatus::Completed,
                end_reason: Some(ApiEndReason::CompletedSuccessfully),
                error: None,
                continuation_eligibility: ContinuationEligibility::Forbidden,
                timing: ApiTimings::default(),
                provider_source: Some(changed_source),
                completed_at_ms: T0 + 5,
            },
        ),
        Err(DomainError::ApiRequestMismatch { .. })
    ));
    assert_eq!(doc.events().len(), event_count);
    assert!(doc.active_api().is_some());
    Ok(())
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

/// A tools-only group (no reasoning/text) carries exactly one provider_source
/// placed on its last tool event, per plan 5.5.5.
/// 仅工具组(无推理/正文)按计划 5.5.5 在最后一个工具事件上恰好携带一个
/// provider_source。
#[test]
fn provider_source_on_tools_only_group() {
    let mut doc = new_document();
    doc.submit_root_input_fixture("run", Vec::new(), T0)
        .unwrap();
    let api = doc
        .start_api(
            ApiPurposeKind::MainGeneration,
            crate::v1::event::ApiRequestSnapshot::new("openai", "gpt-5"),
            0,
        )
        .unwrap();
    let second_tool = doc
        .start_tool(
            api,
            "shell",
            ToolPayload::try_new(serde_json::json!({"cmd": "ls"})).unwrap(),
            Some("pt-1".to_string()),
            T0,
        )
        .unwrap();
    let last_tool = doc
        .start_tool(
            api,
            "file_read",
            ToolPayload::try_new(serde_json::json!({"path": "a"})).unwrap(),
            Some("pt-2".to_string()),
            T0,
        )
        .unwrap();
    doc.finish_tool(
        api,
        second_tool,
        tool_completion(
            Some(ToolPayload::try_new(serde_json::json!({"ok": true})).unwrap()),
            ToolCallStatus::Completed,
            T0 + 1,
        ),
    )
    .unwrap();
    doc.finish_tool(
        api,
        last_tool,
        tool_completion(
            Some(ToolPayload::try_new(serde_json::json!({"ok": true})).unwrap()),
            ToolCallStatus::Completed,
            T0 + 1,
        ),
    )
    .unwrap();
    let last_tool_seq = doc
        .events()
        .iter()
        .find_map(|event| match &event.event {
            EventBody::ToolCall(tool) if tool.tool_call_id == last_tool => Some(event.seq()),
            _ => None,
        })
        .unwrap();
    let source = ProviderSourceInput {
        provider: "openai".to_string(),
        profile: None,
        model: "gpt-5".to_string(),
        connection_id: None,
        message_id: Some("m1".to_string()),
        response_id: None,
        replay: None,
    };
    complete_ok(&mut doc, api, Some(source), T0 + 2).unwrap();
    let placed: Vec<(u64, &ToolCallEvent)> = doc
        .events()
        .iter()
        .filter_map(|event| match &event.event {
            EventBody::ToolCall(tool) if tool.provider_source.is_some() => {
                Some((tool.tool_call_id.value(), tool))
            }
            _ => None,
        })
        .collect();
    assert_eq!(placed.len(), 1, "exactly one provider_source in the group");
    assert_eq!(
        placed[0].0,
        last_tool.value(),
        "must sit on the last tool event"
    );
    assert_eq!(
        placed[0]
            .1
            .provider_source
            .as_ref()
            .map(|source| source.covered_until_seq),
        Some(last_tool_seq)
    );
}

/// `advance_beyond` rejects overflow instead of panicking.
/// `advance_beyond` 拒绝溢出而不是 panic。
#[test]
fn advance_beyond_rejects_overflow() {
    let mut allocator = IdAllocator::new();
    let result = allocator.advance_beyond(u64::MAX, 1);
    assert!(result.is_err());
    let result = allocator.advance_beyond(1, JS_SAFE_INTEGER_MAX);
    assert!(result.is_err());
    assert!(allocator.advance_beyond(10, 20).is_ok());
    assert_eq!(allocator.frontier(), (21, 11));
}

/// The document derives provider-source coverage and fingerprint internally.
/// 文档在内部推导 provider-source 覆盖位置与指纹。
#[test]
fn document_derives_provider_source_coverage_and_fingerprint() {
    let mut doc = new_document();
    doc.submit_root_input_fixture("go", Vec::new(), T0).unwrap();
    let api = doc
        .start_api(
            ApiPurposeKind::MainGeneration,
            crate::v1::event::ApiRequestSnapshot::new("openai", "gpt-5"),
            0,
        )
        .unwrap();
    let text_seq = doc
        .append_assistant_text(api, "t", AssistantSemanticStatus::Completed, T0)
        .unwrap()
        .into_value();
    let source = ProviderSourceInput {
        provider: "openai".to_string(),
        profile: None,
        model: "gpt-5".to_string(),
        connection_id: None,
        message_id: None,
        response_id: None,
        replay: None,
    };
    complete_ok(&mut doc, api, Some(source), T0 + 1).unwrap();
    let placed = doc.events().iter().find_map(|event| match &event.event {
        EventBody::AssistantText(text) => text.provider_source.as_ref(),
        _ => None,
    });
    let placed = placed.expect("provider source must be attached");
    assert_eq!(placed.covered_until_seq, text_seq);
    assert_eq!(
        placed.fingerprint_sha256,
        crate::v1::projection::provider_visible_fingerprint(doc.events(), text_seq).unwrap()
    );
    assert_eq!(placed.fingerprint_sha256.len(), 64);
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

/// Tools after the api stats are rejected at write time.
/// 在 api stats 之后写工具会被写时拒绝。
#[test]
fn tool_after_stats_is_rejected() {
    let mut doc = new_document();
    doc.submit_root_input_fixture("a", Vec::new(), T0).unwrap();
    let api = doc
        .start_api(
            ApiPurposeKind::MainGeneration,
            crate::v1::event::ApiRequestSnapshot::new("openai", "gpt-5"),
            0,
        )
        .unwrap();
    complete_ok(&mut doc, api, None, T0 + 1).unwrap();
    let result = doc.start_tool(
        api,
        "shell",
        ToolPayload::try_new(serde_json::json!({})).unwrap(),
        None,
        T0 + 2,
    );
    // A closed group has no active api; the late tool is rejected at write
    // time as a cross-group update.
    // 已闭合的组没有活动 api；迟到工具在写时作为跨组更新被拒绝。
    assert!(matches!(
        result,
        Err(DomainError::CrossApiGroupUpdate { .. })
    ));
}

/// The terminal turn statistic covers its own seq as the end.
/// 终态回合统计以自身 seq 作为覆盖结束。
#[test]
fn turn_stats_end_seq_includes_itself() {
    let mut doc = new_document();
    doc.submit_root_input_fixture("a", Vec::new(), T0).unwrap();
    let api = doc
        .start_api(
            ApiPurposeKind::MainGeneration,
            crate::v1::event::ApiRequestSnapshot::new("openai", "gpt-5"),
            0,
        )
        .unwrap();
    doc.append_assistant_text(api, "x", AssistantSemanticStatus::Completed, T0)
        .unwrap();
    complete_ok(&mut doc, api, None, T0 + 1).unwrap();
    let stats_seq = doc.close_turn(TurnOutcome::Completed, 1, T0 + 2).unwrap();
    let stats = doc
        .events()
        .iter()
        .find_map(|event| match &event.event {
            EventBody::TurnStats(stats) => Some(stats),
            _ => None,
        })
        .expect("turn stats must exist");
    assert_eq!(
        stats.end_seq.value(),
        stats_seq.value(),
        "turn stats must cover its own seq"
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
        events: doc.events().to_vec(),
        tool_seq_by_id: HashMap::new(),
        active_turn: None,
        active_api: None,
        id_allocator: IdAllocator::new(),
        memory_revision: 0,
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

/// Builds one raw envelope for strict-rebuild rejection fixtures.
/// 为严格重建拒绝夹具构建一条原始信封。
fn raw_event(
    seq: u64,
    turn: u64,
    visibility: EventVisibility,
    body: EventBody,
) -> ConversationEvent {
    ConversationEvent {
        seq: Seq::try_new(seq).unwrap(),
        turn_id: Some(TurnId::try_new(turn).unwrap()),
        created_at_ms: T0,
        updated_at_ms: T0,
        event_revision: crate::v1::event::EVENT_REVISION_INITIAL,
        visibility,
        event: body,
    }
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
        input_id: PendingInputId::try_new(1).unwrap(),
        client_action_id: ClientActionId::try_new("turn-closure-root").unwrap(),
        input_source: UserInputSource::Root,
        text: "a".to_string(),
        resource_ids: Vec::new(),
    });
    let boundary = EventBody::TurnBoundary(TurnBoundaryEvent {
        outcome: TurnOutcome::Completed,
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

fn group_reasoning(seq: u64, turn: u64, api: u64) -> ConversationEvent {
    raw_event(
        seq,
        turn,
        EventVisibility::Provider,
        EventBody::AssistantReasoning(AssistantReasoningEvent {
            api_call_id: ApiCallId::try_new(api).unwrap(),
            text: "r".to_string(),
            status: AssistantSemanticStatus::Completed,
            provider_source: None,
        }),
    )
}

/// Builds one raw immutable API-start fact for structural-validation tests.
/// 为结构校验测试构建一条原始不可变 API 起始事实。
fn group_started(seq: u64, turn: u64, api: u64) -> ConversationEvent {
    raw_event(
        seq,
        turn,
        EventVisibility::HostOnly,
        EventBody::ApiStarted(ApiStartedEvent {
            api_call_id: ApiCallId::try_new(api).unwrap(),
            purpose: ApiPurposeKind::MainGeneration,
            request: ApiRequestSnapshot::new("openai", "gpt-5"),
            request_budget_evidence: test_provider_request_budget_evidence(),
            projection: ProviderProjectionProof {
                segment_generation: 1,
                document_revision: 1,
                retained_from_seq: Seq::try_new(1).unwrap(),
                through_seq: Seq::try_new(seq - 1).unwrap(),
                fingerprint_sha256: "0".repeat(64),
            },
            continuation_action_id: None,
        }),
    )
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

/// The next compaction record trims unpublished candidates left over from a
/// failed runtime write, so an in-process failure can never make the next
/// compaction append on top of them (audit P0#7, plan 6.2/5.8.3).
/// 下一次压缩记录裁剪 runtime 写失败残留的未发布候选，同进程失败绝不可能使
/// 下一次压缩基于它们继续追加(审计 P0#7、计划 6.2/5.8.3)。
#[test]
fn next_record_trims_unpublished_candidates_after_runtime_failure() {
    let mut doc = new_document();
    doc.submit_root_input_fixture("a", Vec::new(), T0).unwrap();
    let api = doc
        .start_api(
            ApiPurposeKind::Compaction,
            crate::v1::event::ApiRequestSnapshot::new("openai", "gpt-5"),
            0,
        )
        .unwrap();
    let occurred1 = doc
        .append_assistant_text(api, "c1", AssistantSemanticStatus::Completed, T0 + 1)
        .unwrap()
        .into_value();
    complete_ok(&mut doc, api, None, T0 + 2).unwrap();
    // First compaction publishes durably (confirmed by a full flush cycle).
    // 第一次压缩持久化发布(经完整 flush 周期确认)。
    let node1 = doc
        .record_compaction(
            CompactionCommitInput::new(
                api,
                occurred1,
                Seq::try_new(1).unwrap(),
                "c1",
                LockedBaselineProjection::empty(),
            ),
            T0 + 3,
        )
        .unwrap()
        .into_value();
    let candidate = doc.prepare_flush_candidates().unwrap().unwrap();
    doc.advance_durable(candidate.revision).unwrap();
    assert_eq!(doc.compactions.len(), 1);
    // Second compaction becomes an unpublished candidate: its runtime write
    // never confirms (simulated by not advancing).
    // 第二次压缩成为未发布候选：它的 runtime 写入从未确认(以不 advance 模拟)。
    let api2 = doc
        .start_api(
            ApiPurposeKind::Compaction,
            crate::v1::event::ApiRequestSnapshot::new("openai", "gpt-5"),
            T0 + 4,
        )
        .unwrap();
    let occurred2 = doc
        .append_assistant_text(api2, "c2", AssistantSemanticStatus::Completed, T0 + 5)
        .unwrap()
        .into_value();
    complete_ok(&mut doc, api2, None, T0 + 6).unwrap();
    let node2 = doc
        .record_compaction(
            CompactionCommitInput::new(
                api2,
                occurred2,
                occurred1,
                "c2",
                LockedBaselineProjection::empty(),
            ),
            T0 + 7,
        )
        .unwrap()
        .into_value();
    assert_eq!(doc.compactions.len(), 2);
    assert_eq!(doc.runtime_lock.current_compaction_id, Some(node2));
    // The next record trims the unpublished candidate before appending.
    // 下一次记录先裁剪未发布候选再追加。
    let api3 = doc
        .start_api(
            ApiPurposeKind::Compaction,
            crate::v1::event::ApiRequestSnapshot::new("openai", "gpt-5"),
            T0 + 8,
        )
        .unwrap();
    let occurred3 = doc
        .append_assistant_text(api3, "c3", AssistantSemanticStatus::Completed, T0 + 9)
        .unwrap()
        .into_value();
    complete_ok(&mut doc, api3, None, T0 + 10).unwrap();
    let node3 = doc
        .record_compaction(
            CompactionCommitInput::new(
                api3,
                occurred3,
                occurred2,
                "c3",
                LockedBaselineProjection::empty(),
            ),
            T0 + 11,
        )
        .unwrap()
        .into_value();
    let ids: Vec<u64> = doc
        .compactions
        .iter()
        .map(|node| node.compaction_id.value())
        .collect();
    assert_eq!(ids, vec![node1.value(), node3.value()]);
    assert_eq!(doc.runtime_lock.current_compaction_id, Some(node3));
}

/// An exact compaction commit retry returns the original node without changing
/// the document, while any retained-boundary drift is rejected atomically.
/// 精确压缩提交重试返回原节点且不修改文档；任何保留边界漂移都会被原子拒绝。
#[test]
fn compaction_commit_is_idempotent_and_rejects_drift() {
    let mut doc = new_document();
    doc.submit_root_input_fixture("root", Vec::new(), T0)
        .unwrap();
    let api = doc
        .start_api(
            ApiPurposeKind::Compaction,
            crate::v1::event::ApiRequestSnapshot::new("openai", "gpt-5"),
            T0 + 1,
        )
        .unwrap();
    let occurred_at_seq = doc
        .append_assistant_text(api, "summary", AssistantSemanticStatus::Completed, T0 + 2)
        .unwrap()
        .into_value();
    complete_ok(&mut doc, api, None, T0 + 3).unwrap();
    let input = CompactionCommitInput::new(
        api,
        occurred_at_seq,
        Seq::try_new(1).unwrap(),
        "summary",
        LockedBaselineProjection::empty(),
    );

    let committed = doc.record_compaction(input.clone(), T0 + 4).unwrap();
    assert!(matches!(committed, IdempotentMutation::Committed(_)));
    let committed_id = committed.into_value();
    let after_commit = doc.clone();
    let replayed = doc.record_compaction(input, T0 + 5).unwrap();
    assert_eq!(replayed, IdempotentMutation::Idempotent(committed_id));
    assert_eq!(doc, after_commit);

    let drifted = CompactionCommitInput::new(
        api,
        occurred_at_seq,
        Seq::try_new(2).unwrap(),
        "summary",
        LockedBaselineProjection::empty(),
    );
    assert!(matches!(
        doc.record_compaction(drifted, T0 + 6),
        Err(DomainError::ImmutableFieldMutation {
            entity: "compaction",
            field: "existing_node"
        })
    ));
    assert_eq!(doc, after_commit);
}

/// Non-object tool payloads are rejected at construction, keeping raw
/// unvalidated JSON out of the main event stream (audit P0#9).
/// 非对象工具载荷在构造时被拒绝，使未经验证的原始 JSON 无法进入主事件流
/// (审计 P0#9)。
#[test]
fn tool_payload_rejects_non_object_containers() {
    assert!(ToolPayload::try_new(serde_json::json!(1)).is_err());
    assert!(ToolPayload::try_new(serde_json::json!("x")).is_err());
    assert!(ToolPayload::try_new(serde_json::json!([1, 2])).is_err());
    assert!(ToolPayload::try_new(serde_json::json!(null)).is_err());
    assert!(ToolPayload::try_new(serde_json::json!({"ok": true})).is_ok());
}

/// A document whose resident stream starts at the active tail's seq (tail-only
/// load) rebuilds and appends against that offset; a mutation that targets a
/// sealed (non-resident) api is rejected (audit P0#2).
/// 常驻流从活动尾段起始 seq 开始的文档(tail-only 加载)按该偏移重建与追加；
/// 针对封口(非驻留)API 的 mutation 被拒绝(审计 P0#2)。
#[test]
fn tail_only_document_operates_on_resident_offset() {
    let mut doc = new_document();
    doc.submit_root_input_fixture("a", Vec::new(), T0).unwrap();
    let api = doc
        .start_api(
            ApiPurposeKind::MainGeneration,
            crate::v1::event::ApiRequestSnapshot::new("openai", "gpt-5"),
            0,
        )
        .unwrap();
    let _text_seq = doc
        .append_assistant_text(api, "x", AssistantSemanticStatus::Completed, T0 + 1)
        .unwrap();
    complete_ok(&mut doc, api, None, T0 + 2).unwrap();
    let sealed_end = doc.events().last().unwrap().seq().value();
    // Simulate a tail-only load: the catalog carries the sealed prefix, the
    // resident stream starts after it.
    // 模拟 tail-only 加载：catalog 携带封口前缀，常驻流从其之后开始。
    let tail_events: Vec<ConversationEvent> = doc
        .events()
        .iter()
        .filter(|event| event.seq().value() > sealed_end)
        .cloned()
        .collect();
    let sealed_next_event_local_id =
        crate::v1::event::next_event_local_id(&doc.events()[..sealed_end as usize]);
    let stream_next_event_local_id = crate::v1::event::next_event_local_id(doc.events());
    let catalog = vec![
        crate::v1::config::SegmentInfo {
            segment_number: 1,
            file_name: "s0000001.jsonl".to_string(),
            start_seq: Seq::try_new(1).unwrap(),
            end_seq: Some(Seq::try_new(sealed_end).unwrap()),
            event_count: sealed_end,
            byte_len: 64,
            next_event_local_id: sealed_next_event_local_id,
        },
        crate::v1::config::SegmentInfo {
            segment_number: 2,
            file_name: "s0000002.jsonl".to_string(),
            start_seq: Seq::try_new(sealed_end + 1).unwrap(),
            end_seq: None,
            event_count: tail_events.len() as u64,
            byte_len: 64,
            next_event_local_id: stream_next_event_local_id,
        },
    ];
    let mut restored = SessionDocument::from_parts(
        doc.metadata.clone(),
        doc.runtime_lock.clone(),
        doc.compactions.clone(),
        catalog,
        tail_events,
    )
    .unwrap();
    assert_eq!(
        restored.segment_catalog().len(),
        2,
        "the catalog keeps every sealed segment addressable"
    );
    assert!(
        restored.active_api().is_none(),
        "the sealed group is closed; the tail-only document still rebuilds"
    );
    let appended = restored
        .append_display_note(
            None,
            DisplayNoticeKind::SystemError,
            NoticeSeverity::Info,
            "z",
            None,
            None,
            None,
            None,
            T0 + 3,
        )
        .unwrap();
    assert_eq!(
        appended.value(),
        sealed_end + restored.events().len() as u64,
        "appends continue from the resident offset"
    );
    // Mutations targeting the sealed (non-resident) closed group are rejected.
    // 针对封口(非驻留)已闭合组的 mutation 被拒绝。
    assert!(matches!(
        restored.append_assistant_text(api, "late", AssistantSemanticStatus::Completed, T0 + 4,),
        Err(DomainError::CrossApiGroupUpdate { .. })
    ));
}

/// The context-compaction node mutation dirties only metadata: the timeline is
/// untouched, a running API keeps working, and clearing the override restores
/// model/default selection (audit P0#10).
/// 上下文压缩节点 mutation 只弄脏 metadata：时间线与运行中的 API 不受影响，
/// 清除覆盖后恢复模型或默认选择(审计 P0#10)。
#[test]
fn set_context_compaction_window_node_dirts_metadata_only() {
    let mut doc = new_document();
    doc.submit_root_input_fixture("a", Vec::new(), T0).unwrap();
    let api = doc
        .start_api(
            ApiPurposeKind::MainGeneration,
            crate::v1::event::ApiRequestSnapshot::new("openai", "gpt-5"),
            0,
        )
        .unwrap();
    doc.append_assistant_text(api, "x", AssistantSemanticStatus::Streaming, T0 + 1)
        .unwrap();
    let dirty_before: Vec<DirtyFile> = doc.dirty_files().collect();
    doc.set_context_compaction_window_node_id(
        Some(vulcan_ai_contract::ContextCompactionWindowNodeId::CompressionWindow2),
        T0,
    );
    assert_eq!(
        doc.metadata.config.context_compaction_window_node_id,
        Some(vulcan_ai_contract::ContextCompactionWindowNodeId::CompressionWindow2)
    );
    assert!(doc.dirty_files().any(|file| file == DirtyFile::Metadata));
    assert!(
        doc.dirty_files()
            .filter(|file| *file == DirtyFile::ActiveTail)
            .count()
            <= dirty_before
                .iter()
                .filter(|file| **file == DirtyFile::ActiveTail)
                .count(),
        "the mutation must not add new tail dirtiness"
    );
    assert!(
        doc.active_api().is_some(),
        "the running API must keep working"
    );
    doc.set_context_compaction_window_node_id(None, T0 + 1);
    assert_eq!(doc.metadata.config.context_compaction_window_node_id, None);
}

/// The flush candidate captures the memory revision inside the document and
/// copies only the active tail (plan 6.2.1 flush contract).
/// flush 候选在文档内部捕获 memory 修订号，且只复制活动尾段(计划 6.2.1
/// flush 契约)。
#[test]
fn flush_candidate_snapshot_copies_dirty_files() {
    let mut doc = new_document();
    doc.submit_root_input_fixture("a", Vec::new(), T0).unwrap();
    let revision = doc.memory_revision();
    let candidate = doc
        .prepare_flush_candidates()
        .unwrap()
        .expect("a dirty tail must yield a candidate");
    assert!(candidate.dirty.contains(&DirtyFile::ActiveTail));
    assert_eq!(candidate.revision, revision);
    assert_eq!(candidate.tail_events.len(), doc.events().len());
    // The document keeps mutating after the snapshot; the candidate is owned
    // and unaffected, and only the tail participates.
    // 快照之后文档继续变更；候选是自有的、不受影响，且只有尾段参与。
    doc.append_display_note(
        None,
        DisplayNoticeKind::SystemError,
        NoticeSeverity::Info,
        "x",
        None,
        None,
        None,
        None,
        T0 + 1,
    )
    .unwrap();
    assert_eq!(candidate.tail_events.len() + 1, doc.events().len());
    assert!(doc.prepare_flush_candidates().unwrap().is_some());
    doc.advance_durable(candidate.revision).unwrap();
    // The display note dirtied the tail again after the snapshot, so the
    // first confirmation must NOT clear it; the next candidate captures the
    // new revision.
    // display note 在快照之后再次弄脏尾段，因此第一次确认不得清除它；下一个
    // 候选捕获新的修订号。
    assert!(
        doc.dirty_files().any(|file| file == DirtyFile::ActiveTail),
        "tail re-dirtied after the snapshot must survive the first confirmation"
    );
    let second = doc.prepare_flush_candidates().unwrap().unwrap();
    assert!(second.revision > candidate.revision);
    doc.advance_durable(second.revision).unwrap();
    assert_eq!(doc.durable_revision(), doc.memory_revision());
    assert!(doc.prepare_flush_candidates().unwrap().is_none());
}

/// A candidate issued before a later mutation cannot clear that later dirty
/// file, and consecutive flushes advance the durable revision accurately.
/// 在后续 mutation 之前发出的候选不能清除该后续 dirty 文件；连续两次 flush
/// 准确推进 durable 修订号。
#[test]
fn mutation_after_candidate_preserves_new_dirty() {
    let mut doc = new_document();
    doc.submit_root_input_fixture("a", Vec::new(), T0).unwrap();
    let first = doc.prepare_flush_candidates().unwrap().unwrap();
    assert!(first.dirty.contains(&DirtyFile::ActiveTail));
    // An input-state update marks only metadata dirty at a higher revision.
    // 输入区更新只以更高修订号标记 metadata dirty。
    doc.add_pending_in_turn_input_fixture("mid", Vec::new(), T0 + 1)
        .unwrap();
    doc.advance_durable(first.revision).unwrap();
    assert_eq!(doc.durable_revision(), first.revision);
    assert!(
        !doc.dirty_files().any(|file| file == DirtyFile::ActiveTail),
        "the tail was covered by the first candidate"
    );
    assert!(
        doc.dirty_files().any(|file| file == DirtyFile::Metadata),
        "the metadata dirtied after the snapshot must stay dirty"
    );
    let second = doc.prepare_flush_candidates().unwrap().unwrap();
    assert_eq!(second.revision, doc.memory_revision());
    assert!(second.dirty.contains(&DirtyFile::Metadata));
    doc.advance_durable(second.revision).unwrap();
    assert_eq!(doc.durable_revision(), doc.memory_revision());
    assert!(
        doc.prepare_flush_candidates().unwrap().is_none(),
        "no dirty files remain after both candidates are confirmed"
    );
}

/// Future or forged revisions are rejected by `advance_durable`; only issued
/// candidate revisions confirm, and out-of-order confirmation is a no-op.
/// `advance_durable` 拒绝未来或被伪造的修订号；只有已发出的候选修订号可被
/// 确认，乱序确认是无操作。
#[test]
fn advance_durable_rejects_unissued_and_future_revisions() {
    let mut doc = new_document();
    assert!(
        matches!(
            doc.advance_durable(doc.memory_revision() + 100),
            Err(DomainError::RevisionNeverIssued { .. })
        ),
        "a future revision must never confirm"
    );
    doc.submit_root_input_fixture("a", Vec::new(), T0).unwrap();
    let first = doc.prepare_flush_candidates().unwrap().unwrap();
    // Another mutation moves memory beyond the issued revision.
    // 另一次 mutation 使 memory 越过已发出的修订号。
    doc.add_pending_in_turn_input_fixture("mid", Vec::new(), T0 + 1)
        .unwrap();
    doc.advance_durable(first.revision).unwrap();
    let unissued = doc.memory_revision();
    assert!(
        unissued > first.revision,
        "fixture: memory moved beyond the issued revision"
    );
    assert!(
        matches!(
            doc.advance_durable(unissued),
            Err(DomainError::RevisionNeverIssued { .. })
        ),
        "an unissued revision between memory and durable must be rejected"
    );
    // A second candidate then confirms; the earlier confirmation stays valid.
    // 随后第二个候选被确认；此前的确认仍然有效。
    let second = doc.prepare_flush_candidates().unwrap().unwrap();
    let out_of_order = first.revision;
    assert!(second.revision > out_of_order);
    assert!(
        doc.advance_durable(out_of_order).is_ok(),
        "confirming an already-covered revision is a no-op, not an error"
    );
    doc.advance_durable(second.revision).unwrap();
    assert_eq!(doc.durable_revision(), doc.memory_revision());
    assert!(doc.dirty_files().next().is_none());
}

/// The event candidate copies only the active tail, never the sealed prefix
/// (plan 5.2.15: sealed segments are immutable and never rewritten).
/// 事件候选只复制活动尾段，绝不复制封口前缀(计划 5.2.15：封口分片不可变、
/// 永不重写)。
#[test]
fn event_candidate_copies_only_active_tail() {
    let mut doc = new_document();
    doc.submit_root_input_fixture("a", Vec::new(), T0).unwrap();
    let api = doc
        .start_api(
            ApiPurposeKind::MainGeneration,
            crate::v1::event::ApiRequestSnapshot::new("openai", "gpt-5"),
            0,
        )
        .unwrap();
    doc.append_reasoning(api, "r", AssistantSemanticStatus::Completed, T0 + 1)
        .unwrap();
    let text_seq = doc
        .append_assistant_text(api, "t", AssistantSemanticStatus::Completed, T0 + 2)
        .unwrap()
        .into_value();
    complete_ok(&mut doc, api, None, T0 + 3).unwrap();
    let first_tail_seq = text_seq.value() - 1;
    // Fabricate a split map where everything before the last two events is a
    // sealed segment; the doc-level split map is storage metadata and its
    // shape is authoritative for the flush slice.
    // 构造一个分片映射：最后两条事件之前的全部内容属于封口分片；文档级的
    // 分片映射是存储元数据，其形状对 flush 切片权威。
    let sealed_next_event_local_id =
        crate::v1::event::next_event_local_id(&doc.events()[..first_tail_seq as usize]);
    let stream_next_event_local_id = crate::v1::event::next_event_local_id(doc.events());
    doc.metadata.split_list = vec![
        crate::v1::config::SegmentInfo {
            segment_number: 1,
            file_name: "000001.jsonl".to_string(),
            start_seq: crate::v1::ids::Seq::try_new(1).unwrap(),
            end_seq: Some(crate::v1::ids::Seq::try_new(first_tail_seq).unwrap()),
            event_count: first_tail_seq,
            byte_len: 64,
            next_event_local_id: sealed_next_event_local_id,
        },
        crate::v1::config::SegmentInfo {
            segment_number: 2,
            file_name: "000002.jsonl".to_string(),
            start_seq: crate::v1::ids::Seq::try_new(first_tail_seq + 1).unwrap(),
            end_seq: None,
            event_count: 2,
            byte_len: 64,
            next_event_local_id: stream_next_event_local_id,
        },
    ];
    doc.append_display_note(
        None,
        DisplayNoticeKind::SystemError,
        NoticeSeverity::Info,
        "y",
        None,
        None,
        None,
        None,
        T0 + 4,
    )
    .unwrap();
    let candidate = doc.prepare_flush_candidates().unwrap().unwrap();
    assert_eq!(
        candidate.tail_events.len(),
        doc.events().len() - first_tail_seq as usize,
        "tail starts after the sealed prefix"
    );
    assert_eq!(candidate.tail_events[0].seq().value(), first_tail_seq + 1);
    assert!(
        candidate
            .tail_events
            .iter()
            .all(|event| event.seq().value() >= first_tail_seq + 1)
    );
    doc.advance_durable(candidate.revision).unwrap();
    assert_eq!(doc.durable_revision(), doc.memory_revision());
}

/// Pending request promotion and Goal terminal updates mutate metadata and
/// append structured display notes.
/// 待生效请求提升与 Goal 终态更新修改 metadata 并追加结构化 display-note。
#[test]
fn request_promotion_and_goal_state_mutations() {
    let mut doc = new_document();
    doc.set_pending_request(
        PendingRequestConfig {
            provider: Some("anthropic".to_string()),
            connection_id: None,
            model: Some("claude-4".to_string()),
            thinking_level: None,
            temperature: None,
            set_at_ms: T0,
        },
        T0,
    )
    .unwrap();
    doc.promote_pending_request(None, T0).unwrap();
    assert_eq!(
        doc.metadata.config.active_request.provider.as_deref(),
        Some("anthropic")
    );
    assert!(doc.metadata.config.pending_request.is_none());
    assert!(doc.dirty_files().any(|file| file == DirtyFile::Metadata));

    doc.metadata.mode.goal.enabled = true;
    doc.metadata.mode.goal.state = GoalState::Active;
    let note_seq = doc.block_goal("goal_blocked", T0 + 1).unwrap();
    assert_eq!(doc.metadata.mode.goal.state, GoalState::Blocked);
    let note = doc.events().iter().find(|e| e.seq() == note_seq).unwrap();
    match &note.event {
        EventBody::DisplayNote(display) => {
            assert_eq!(display.notice_kind, DisplayNoticeKind::GoalStatus);
            assert_eq!(display.reason_code, "goal_blocked");
        }
        _ => unreachable!(),
    }
}

/// Setting a pending request persists the exact typed payload and dirties only
/// metadata without appending a timeline event.
/// 设置待生效请求会持久化精确强类型载荷，并且只标记 metadata dirty，不追加
/// 时间线事件。
#[test]
fn pending_request_setter_dirties_only_metadata_and_serializes() -> Result<(), serde_json::Error> {
    let mut doc = new_document();
    doc.set_pending_request(
        PendingRequestConfig {
            provider: Some("anthropic".to_string()),
            connection_id: None,
            model: Some("claude-4".to_string()),
            thinking_level: Some("high".to_string()),
            temperature: Some(0.2),
            set_at_ms: T0,
        },
        T0,
    )
    .unwrap();

    assert_eq!(
        doc.dirty_files().collect::<Vec<_>>(),
        vec![DirtyFile::Metadata]
    );
    assert!(doc.events().is_empty());
    let Some(stored) = doc.metadata.config.pending_request.as_ref() else {
        panic!("the setter must store one pending request");
    };
    assert_eq!(
        serde_json::to_string(stored)?,
        r#"{"provider":"anthropic","model":"claude-4","thinking_level":"high","temperature":0.2,"set_at_ms":1700000000000}"#
    );
    Ok(())
}

/// Promotion without a pending request fails before changing any document fact.
/// 没有待生效请求时，promotion 在修改任何文档事实前失败。
#[test]
fn pending_request_promotion_rejects_missing_without_mutation() {
    let mut doc = new_document();
    let revision = doc.memory_revision();

    assert!(matches!(
        doc.promote_pending_request(None, T0),
        Err(DomainError::ReferenceViolation {
            kind: "pending_request",
            value: 0
        })
    ));
    assert_eq!(doc.memory_revision(), revision);
    assert!(doc.events().is_empty());
}

/// Promotion rejects an in-flight API and preserves both active and pending
/// request configs without appending a display fact.
/// promotion 拒绝在途 API，并保留 active 与 pending 请求配置，不追加显示事实。
#[test]
fn pending_request_promotion_rejects_unsafe_api_boundary() -> Result<(), DomainError> {
    let mut doc = new_document();
    let turn_id = doc.submit_root_input_fixture("work", Vec::new(), T0)?;
    doc.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0,
    )?;
    doc.set_pending_request(
        PendingRequestConfig {
            provider: Some("anthropic".to_string()),
            connection_id: None,
            model: Some("claude-4".to_string()),
            thinking_level: None,
            temperature: None,
            set_at_ms: T0 + 1,
        },
        T0 + 1,
    )
    .unwrap();
    let active_before = doc.metadata.config.active_request.clone();
    let pending_before = doc.metadata.config.pending_request.clone();
    let event_count = doc.events().len();
    let revision = doc.memory_revision();

    assert!(matches!(
        doc.promote_pending_request(Some(turn_id), T0 + 2),
        Err(DomainError::PromoteWhileApiOpen)
    ));
    assert_eq!(doc.metadata.config.active_request, active_before);
    assert_eq!(doc.metadata.config.pending_request, pending_before);
    assert_eq!(doc.events().len(), event_count);
    assert_eq!(doc.memory_revision(), revision);
    Ok(())
}

/// Promoting an identical request clears pending metadata without manufacturing
/// a model-switch timeline event.
/// 提升相同请求时只清除 pending metadata，不制造模型切换时间线事件。
#[test]
fn identical_pending_request_clears_without_switch_notice() -> Result<(), DomainError> {
    let mut doc = new_document();
    let active = ActiveRequestConfig {
        provider: Some("openai".to_string()),
        connection_id: Some("conn-1".to_string()),
        model: Some("gpt-5".to_string()),
        thinking_level: Some("high".to_string()),
        temperature: Some(0.3),
    };
    doc.metadata.config.active_request = active.clone();
    doc.set_pending_request(
        PendingRequestConfig {
            provider: active.provider.clone(),
            connection_id: active.connection_id.clone(),
            model: active.model.clone(),
            thinking_level: active.thinking_level.clone(),
            temperature: active.temperature,
            set_at_ms: T0,
        },
        T0,
    )
    .unwrap();

    assert_eq!(doc.promote_pending_request(None, T0 + 1)?, None);
    assert_eq!(doc.metadata.config.active_request, active);
    assert!(doc.metadata.config.pending_request.is_none());
    assert!(doc.events().is_empty());
    assert_eq!(
        doc.dirty_files().collect::<Vec<_>>(),
        vec![DirtyFile::Metadata]
    );
    Ok(())
}

/// A real safe-boundary request switch atomically updates metadata and appends
/// the exact permanent before/after request-config event.
/// 真实安全边界请求切换会原子更新 metadata，并追加精确的永久请求配置前后值事件。
#[test]
fn real_pending_request_switch_appends_serialized_notice() -> Result<(), Box<dyn std::error::Error>>
{
    let mut doc = new_document();
    doc.metadata.config.active_request = ActiveRequestConfig {
        provider: Some("openai".to_string()),
        connection_id: Some("conn-1".to_string()),
        model: Some("gpt-5".to_string()),
        thinking_level: Some("high".to_string()),
        temperature: Some(0.3),
    };
    let turn_id = doc.submit_root_input_fixture("switch", Vec::new(), T0)?;
    doc.set_pending_request(
        PendingRequestConfig {
            provider: Some("anthropic".to_string()),
            connection_id: Some("conn-2".to_string()),
            model: Some("claude-4".to_string()),
            thinking_level: Some("max".to_string()),
            temperature: Some(0.2),
            set_at_ms: T0 + 1,
        },
        T0 + 1,
    )
    .unwrap();

    let Some(notice_seq) = doc.promote_pending_request(Some(turn_id), T0 + 2)? else {
        panic!("a real request switch must append a notice");
    };
    assert_eq!(
        doc.metadata.config.active_request,
        ActiveRequestConfig {
            provider: Some("anthropic".to_string()),
            connection_id: Some("conn-2".to_string()),
            model: Some("claude-4".to_string()),
            thinking_level: Some("max".to_string()),
            temperature: Some(0.2),
        }
    );
    assert!(doc.metadata.config.pending_request.is_none());
    let Some(notice) = doc.events().iter().find(|event| event.seq() == notice_seq) else {
        panic!("the promoted switch must exist in the timeline");
    };
    assert_eq!(
        serde_json::to_string(notice)?,
        r#"{"seq":2,"turn_id":2,"created_at_ms":1700000000002,"updated_at_ms":1700000000002,"event_revision":1,"visibility":"ui_only","event":{"kind":"request_config_changed","previous":{"provider":"openai","connection_id":"conn-1","model":"gpt-5","thinking_level":"high","temperature":0.3},"current":{"provider":"anthropic","connection_id":"conn-2","model":"claude-4","thinking_level":"max","temperature":0.2}}}"#
    );
    assert!(doc.dirty_files().any(|file| file == DirtyFile::Metadata));
    assert!(doc.dirty_files().any(|file| file == DirtyFile::ActiveTail));
    Ok(())
}

/// Batch preparation preserves Provider order, while an exact repeated start is idempotent.
/// 批量准备保留 Provider 原始顺序，而精确重复启动是幂等操作。
#[test]
fn prepared_tool_batch_preserves_order_and_idempotent_start() -> Result<(), DomainError> {
    let mut doc = new_document();
    doc.submit_root_input_fixture("tools", Vec::new(), T0)?;
    let api = doc.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0,
    )?;
    let ids = doc
        .prepare_tool_batch(
            api,
            vec![
                ToolPreparation {
                    request_stream_id: RequestStreamId::try_new("stream-first").unwrap(),
                    tool_name: "first".to_string(),
                    input: ToolPayload::try_new(serde_json::json!({"order": 1}))?,
                    provider_tool_call_id: Some("provider-1".to_string()),
                },
                ToolPreparation {
                    request_stream_id: RequestStreamId::try_new("stream-second").unwrap(),
                    tool_name: "second".to_string(),
                    input: ToolPayload::try_new(serde_json::json!({"order": 2}))?,
                    provider_tool_call_id: Some("provider-2".to_string()),
                },
                ToolPreparation {
                    request_stream_id: RequestStreamId::try_new("stream-third").unwrap(),
                    tool_name: "third".to_string(),
                    input: ToolPayload::try_new(serde_json::json!({"order": 3}))?,
                    provider_tool_call_id: Some("provider-3".to_string()),
                },
            ],
            T0 + 1,
        )?
        .into_value();
    let ids = ids
        .tools
        .iter()
        .map(|receipt| receipt.tool_call_id)
        .collect::<Vec<_>>();
    let prepared = doc
        .events()
        .iter()
        .filter_map(|event| match &event.event {
            EventBody::ToolCall(tool) => Some((event.seq(), tool.clone())),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(prepared.len(), 3);
    assert_eq!(
        prepared
            .iter()
            .map(|(_, tool)| tool.tool_name.as_str())
            .collect::<Vec<_>>(),
        vec!["first", "second", "third"]
    );
    assert!(prepared.windows(2).all(|pair| {
        pair[1].0.value() == pair[0].0.value() + 1
            && pair[0].1.status == ToolCallStatus::Prepared
            && pair[0].1.started_at_ms.is_none()
    }));
    assert_eq!(prepared[2].1.status, ToolCallStatus::Prepared);
    let second_seq = prepared[1].0;

    assert!(doc.start_prepared_tool(api, ids[1], T0 + 2)?);
    let started = doc
        .events()
        .iter()
        .find(|event| event.seq() == second_seq)
        .expect("prepared tool seq must remain present");
    assert!(matches!(
        &started.event,
        EventBody::ToolCall(tool)
            if tool.status == ToolCallStatus::Running
                && tool.started_at_ms == Some(T0 + 2)
    ));
    let revision_before_replay = doc.memory_revision();
    assert!(!doc.start_prepared_tool(api, ids[1], T0 + 3)?);
    assert_eq!(doc.memory_revision(), revision_before_replay);
    assert!(matches!(
        doc.events()
            .iter()
            .find(|event| event.seq() == second_seq)
            .map(|event| &event.event),
        Some(EventBody::ToolCall(tool)) if tool.started_at_ms == Some(T0 + 2)
    ));
    let unknown = ToolCallId::try_new(9_000_000)?;
    assert!(matches!(
        doc.start_prepared_tool(api, unknown, T0 + 3),
        Err(DomainError::ReferenceViolation {
            kind: "tool_call_id",
            ..
        })
    ));
    Ok(())
}

/// Tool progress transitions commit once and exact replays do not create revisions.
/// 工具进度转换仅提交一次，精确重放不会制造修订号。
#[test]
fn tool_progress_transitions_are_idempotent() -> Result<(), DomainError> {
    let mut doc = new_document();
    doc.submit_root_input_fixture("tools", Vec::new(), T0)?;
    let api = doc.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 1,
    )?;
    let tool = doc.prepare_tool(
        api,
        RequestStreamId::try_new("stream-progress-idempotent")?,
        "ask",
        ToolPayload::try_new(serde_json::json!({"question": "continue?"}))?,
        None,
        T0 + 2,
    )?;

    assert!(doc.start_prepared_tool(api, tool, T0 + 3)?);
    let started_revision = doc.memory_revision();
    assert!(!doc.start_prepared_tool(api, tool, T0 + 30)?);
    assert_eq!(doc.memory_revision(), started_revision);

    let progress = Some(ToolPayload::try_new(serde_json::json!({"progress": 50}))?);
    assert!(doc.update_running_tool_output(api, tool, progress.clone(), T0 + 4)?);
    let progress_revision = doc.memory_revision();
    assert!(!doc.update_running_tool_output(api, tool, progress, T0 + 40)?);
    assert_eq!(doc.memory_revision(), progress_revision);

    assert!(doc.mark_tool_waiting_user(api, tool, T0 + 5)?);
    let waiting_revision = doc.memory_revision();
    assert!(!doc.mark_tool_waiting_user(api, tool, T0 + 50)?);
    assert_eq!(doc.memory_revision(), waiting_revision);
    Ok(())
}

/// Every same-seq tool lifecycle failure leaves the complete document unchanged.
/// 任一同 seq 工具生命周期失败都会保持完整文档不变。
#[test]
fn tool_lifecycle_revision_failures_are_atomic() -> Result<(), DomainError> {
    let mut doc = new_document();
    doc.submit_root_input_fixture("tools", Vec::new(), T0)?;
    let api = doc.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 1,
    )?;
    let tool = doc.prepare_tool(
        api,
        RequestStreamId::try_new("stream-lifecycle-overflow")?,
        "ask",
        ToolPayload::try_new(serde_json::json!({"question": "continue?"}))?,
        None,
        T0 + 2,
    )?;
    let tool_seq = doc.tool_seq_by_id[&tool];
    let index =
        usize::try_from(tool_seq.value() - doc.resident_start_seq).expect("resident fixture index");
    doc.events[index].event_revision = JS_SAFE_INTEGER_MAX;
    let before_start = doc.clone();
    assert!(matches!(
        doc.start_prepared_tool(api, tool, T0 + 3),
        Err(DomainError::InvalidId {
            entity: "event_revision",
            ..
        })
    ));
    assert_eq!(doc, before_start);

    doc.events[index].event_revision = 1;
    assert!(doc.start_prepared_tool(api, tool, T0 + 3)?);
    assert!(doc.mark_tool_waiting_user(api, tool, T0 + 4)?);
    doc.events[index].event_revision = JS_SAFE_INTEGER_MAX;
    let before_answer = doc.clone();
    assert!(matches!(
        doc.answer_waiting_tool(
            api,
            tool,
            tool_completion(
                Some(ToolPayload::try_new(serde_json::json!({"answer": "yes"}))?),
                ToolCallStatus::Completed,
                T0 + 5,
            ),
        ),
        Err(DomainError::InvalidId {
            entity: "event_revision",
            ..
        })
    ));
    assert_eq!(doc, before_answer);
    Ok(())
}

/// A crash interrupts a durably Prepared tool because no exactly-once tool
/// process recovery scheduler exists; the same seq receives an explicit error
/// output that the next Provider request can replay.
/// 崩溃会中断已可靠 Prepared 的工具，因为不存在 exactly-once 工具进程恢复调度器；
/// 同一 seq 会得到明确错误输出，供下一次 Provider 请求重放。
#[test]
fn crash_interrupts_prepared_tool_with_replayable_error_output() -> Result<(), DomainError> {
    let mut doc = new_document();
    let turn_id = doc.submit_root_input_fixture("tools", Vec::new(), T0)?;
    let api = doc.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0,
    )?;
    let tool = doc.prepare_tool(
        api,
        RequestStreamId::try_new("stream-provider-tool")?,
        "shell",
        ToolPayload::try_new(serde_json::json!({"command": "echo ready"}))?,
        Some("provider-tool".to_string()),
        T0 + 1,
    )?;
    let tool_seq = doc.tool_seq_by_id[&tool];
    let mut restored = SessionDocument::from_parts(
        doc.metadata.clone(),
        initialized_runtime_lock(),
        Vec::new(),
        Vec::new(),
        doc.events().to_vec(),
    )?;

    assert_eq!(restored.project_persisted_interruptions()?, 2);
    assert!(restored.active_api().is_none());
    assert_eq!(
        restored.active_turn().map(|turn| turn.turn_id),
        Some(turn_id)
    );
    assert!(matches!(
        restored
            .events()
            .iter()
            .find(|event| event.seq() == tool_seq),
        Some(ConversationEvent {
            event: EventBody::ToolCall(ToolCallEvent {
                status: ToolCallStatus::Interrupted,
                started_at_ms: None,
                finished_at_ms: Some(_),
                output: Some(_),
                ..
            }),
            ..
        })
    ));
    let interrupted_output = restored
        .events()
        .iter()
        .find_map(|event| match &event.event {
            EventBody::ToolCall(tool_event) if tool_event.tool_call_id == tool => {
                tool_event.output.clone()
            }
            _ => None,
        })
        .expect("interrupted tool must persist an explicit output");
    assert_eq!(
        serde_json::to_value(&interrupted_output).unwrap(),
        serde_json::json!({
            "error": {
                "kind": "interrupted",
                "message": "Tool execution was interrupted before completion."
            },
            "ok": false
        })
    );
    let items = crate::v1::projection::project_provider_items(restored.events(), None, &[])?;
    assert!(items.iter().any(|item| matches!(
        item,
        crate::v1::projection::ProjectedItem::ToolResult { output, .. }
            if output == &interrupted_output
    )));
    Ok(())
}

/// Tool completion commits once, survives reload, and treats an exact replay as a no-op.
/// 工具终态仅提交一次、可跨重载保留，并将精确重放视为无操作。
#[test]
fn tool_completion_is_exactly_idempotent_across_reload() -> Result<(), DomainError> {
    let mut doc = new_document();
    doc.submit_root_input_fixture("run", Vec::new(), T0)?;
    let api = doc.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 1,
    )?;
    let tool = doc.prepare_tool(
        api,
        RequestStreamId::try_new("stream-idempotent-tool")?,
        "shell",
        ToolPayload::try_new(serde_json::json!({"command": "echo ok"}))?,
        None,
        T0 + 2,
    )?;
    doc.start_prepared_tool(api, tool, T0 + 3)?;
    let output = ToolPayload::try_new(serde_json::json!({"exit_code": 0}))?;
    let before_finish_revision = doc.memory_revision();
    let committed = doc.finish_tool(
        api,
        tool,
        tool_completion(Some(output.clone()), ToolCallStatus::Completed, T0 + 4),
    )?;
    let receipt = match committed {
        IdempotentMutation::Committed(receipt) => receipt,
        IdempotentMutation::Idempotent(_) => panic!("first completion must commit"),
    };
    assert_eq!(receipt.api_call_id, api);
    assert_eq!(receipt.tool_call_id, tool);
    assert_eq!(receipt.status, ToolCallStatus::Completed);
    assert_eq!(receipt.finished_at_ms, T0 + 4);
    assert_eq!(doc.memory_revision(), before_finish_revision + 1);

    let mut restored = SessionDocument::from_parts(
        doc.metadata.clone(),
        doc.runtime_lock.clone(),
        doc.compactions.clone(),
        Vec::new(),
        doc.events().to_vec(),
    )?;
    let revision_before_replay = restored.memory_revision();
    let replayed = restored.finish_tool(
        api,
        tool,
        tool_completion(Some(output), ToolCallStatus::Completed, T0 + 99),
    )?;
    assert_eq!(replayed, IdempotentMutation::Idempotent(receipt.clone()));
    assert_eq!(restored.memory_revision(), revision_before_replay);

    let before_drift = restored.clone();
    assert!(matches!(
        restored.finish_tool(
            api,
            tool,
            tool_completion(
                Some(ToolPayload::try_new(serde_json::json!({"exit_code": 1}))?),
                ToolCallStatus::Completed,
                T0 + 100,
            ),
        ),
        Err(DomainError::ImmutableFieldMutation {
            entity: "tool_call_terminal",
            field: "output",
        })
    ));
    assert_eq!(restored, before_drift);

    let before_changeset_drift = restored.clone();
    let drift_changeset: PersistentChangeSet = serde_json::from_value(serde_json::json!({
        "mode": "applied",
        "summary": null,
        "files": [],
        "diagnostics": []
    }))
    .expect("empty canonical changeset fixture");
    assert!(matches!(
        restored.finish_tool(
            api,
            tool,
            ToolCompletion {
                output: Some(ToolPayload::try_new(serde_json::json!({"exit_code": 0}))?),
                changeset: Some(drift_changeset),
                status: ToolCallStatus::Completed,
                finished_at_ms: T0 + 101,
                external_refs: Vec::new(),
                generated_resources: Vec::new(),
            },
        ),
        Err(DomainError::ImmutableFieldMutation {
            entity: "tool_call_terminal",
            field: "changeset",
        })
    ));
    assert_eq!(restored, before_changeset_drift);

    let before_ref_drift = restored.clone();
    assert!(matches!(
        restored.finish_tool(
            api,
            tool,
            ToolCompletion {
                output: Some(ToolPayload::try_new(serde_json::json!({"exit_code": 0}))?),
                changeset: None,
                status: ToolCallStatus::Completed,
                finished_at_ms: T0 + 102,
                external_refs: vec![ExternalResourceRef {
                    kind: ExternalResourceKind::Terminal,
                    id: "terminal-1".to_string(),
                }],
                generated_resources: Vec::new(),
            },
        ),
        Err(DomainError::ImmutableFieldMutation {
            entity: "tool_call_terminal",
            field: "external_refs",
        })
    ));
    assert_eq!(restored, before_ref_drift);
    Ok(())
}

/// WaitingUser rejects direct completion, while a durable answer or cancel can be confirmed once.
/// WaitingUser 拒绝直接终结，而已持久化回答或取消可被精确确认一次。
#[test]
fn waiting_tool_requires_durable_answer_or_cancel_before_finish_confirmation()
-> Result<(), DomainError> {
    let mut doc = new_document();
    doc.submit_root_input_fixture("ask", Vec::new(), T0)?;
    let api = doc.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 1,
    )?;
    let ask = doc.prepare_tool(
        api,
        RequestStreamId::try_new("stream-waiting-answer")?,
        "ask",
        ToolPayload::try_new(serde_json::json!({"question": "continue?"}))?,
        None,
        T0 + 2,
    )?;
    doc.start_prepared_tool(api, ask, T0 + 3)?;
    doc.mark_tool_waiting_user(api, ask, T0 + 4)?;
    let answer = ToolPayload::try_new(serde_json::json!({"answer": "yes"}))?;
    let before_direct_finish = doc.clone();
    assert!(matches!(
        doc.finish_tool(
            api,
            ask,
            tool_completion(Some(answer.clone()), ToolCallStatus::Completed, T0 + 5,),
        ),
        Err(DomainError::ToolStatusMismatch {
            expected: "running_or_terminal",
            actual: "waiting_user",
            ..
        })
    ));
    assert_eq!(doc, before_direct_finish);

    let committed_answer = doc.answer_waiting_tool(
        api,
        ask,
        tool_completion(Some(answer.clone()), ToolCallStatus::Completed, T0 + 6),
    )?;
    assert!(matches!(committed_answer, IdempotentMutation::Committed(_)));
    let answer_revision = doc.memory_revision();
    let confirmed = doc.answer_waiting_tool(
        api,
        ask,
        tool_completion(Some(answer), ToolCallStatus::Completed, T0 + 60),
    )?;
    assert!(matches!(confirmed, IdempotentMutation::Idempotent(_)));
    assert_eq!(confirmed.into_value().finished_at_ms, T0 + 6);
    assert_eq!(doc.memory_revision(), answer_revision);

    let ask_cancel = doc.prepare_tool(
        api,
        RequestStreamId::try_new("stream-waiting-cancel")?,
        "ask",
        ToolPayload::try_new(serde_json::json!({"question": "cancel?"}))?,
        None,
        T0 + 7,
    )?;
    doc.start_prepared_tool(api, ask_cancel, T0 + 8)?;
    doc.mark_tool_waiting_user(api, ask_cancel, T0 + 9)?;
    let committed_cancel = doc.cancel_waiting_tool(api, ask_cancel, None, T0 + 10)?;
    assert!(matches!(committed_cancel, IdempotentMutation::Committed(_)));
    let cancel_revision = doc.memory_revision();
    let confirmed_cancel = doc.cancel_waiting_tool(api, ask_cancel, None, T0 + 100)?;
    assert!(matches!(
        confirmed_cancel,
        IdempotentMutation::Idempotent(_)
    ));
    assert_eq!(confirmed_cancel.into_value().finished_at_ms, T0 + 10);
    assert_eq!(doc.memory_revision(), cancel_revision);
    Ok(())
}

/// A same-seq revision validation failure leaves the entire document unchanged.
/// 同 seq 修订号校验失败时整个文档保持不变。
#[test]
fn tool_completion_revision_failure_is_atomic() -> Result<(), DomainError> {
    let mut doc = new_document();
    doc.submit_root_input_fixture("run", Vec::new(), T0)?;
    let api = doc.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 1,
    )?;
    let tool = doc.prepare_tool(
        api,
        RequestStreamId::try_new("stream-revision-overflow")?,
        "shell",
        ToolPayload::try_new(serde_json::json!({"command": "echo ok"}))?,
        None,
        T0 + 2,
    )?;
    doc.start_prepared_tool(api, tool, T0 + 3)?;
    let tool_seq = doc.tool_seq_by_id[&tool];
    let index =
        usize::try_from(tool_seq.value() - doc.resident_start_seq).expect("resident fixture index");
    doc.events[index].event_revision = JS_SAFE_INTEGER_MAX;
    let before = doc.clone();
    assert!(matches!(
        doc.finish_tool(
            api,
            tool,
            tool_completion(
                Some(ToolPayload::try_new(serde_json::json!({"exit_code": 0}))?),
                ToolCallStatus::Completed,
                T0 + 4,
            ),
        ),
        Err(DomainError::InvalidId {
            entity: "event_revision",
            ..
        })
    ));
    assert_eq!(doc, before);
    Ok(())
}

/// Tool completion rejects an in-memory malformed change-set before any terminal mutation.
/// 工具完成会在任何终态修改前拒绝内存中构造的非法 change-set。
#[test]
fn tool_completion_validates_changeset_before_mutation() -> Result<(), DomainError> {
    let mut doc = new_document();
    doc.submit_root_input_fixture("run", Vec::new(), T0)?;
    let api = doc.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 1,
    )?;
    let tool = doc.prepare_tool(
        api,
        RequestStreamId::try_new("stream-invalid-changeset")?,
        "file_edit",
        ToolPayload::try_new(serde_json::json!({"path": "a.txt"}))?,
        None,
        T0 + 2,
    )?;
    doc.start_prepared_tool(api, tool, T0 + 3)?;
    let invalid = PersistentChangeSet {
        mode: ChangeSetMode::Applied,
        summary: None,
        files: vec![PersistentChangeSetFile {
            resource_id: None,
            change: ChangeSetFileChangeKind::Create,
            path: Some("a.txt".to_string()),
            old_path: None,
            new_path: None,
            hunks: Vec::new(),
            lifecycle_content: None,
            before_text_format: None,
            after_text_format: None,
        }],
        diagnostics: Vec::new(),
    };
    let before = doc.clone();
    assert!(matches!(
        doc.finish_tool(
            api,
            tool,
            ToolCompletion {
                output: Some(ToolPayload::try_new(serde_json::json!({"ok": true}))?),
                changeset: Some(invalid),
                status: ToolCallStatus::Completed,
                finished_at_ms: T0 + 4,
                external_refs: Vec::new(),
                generated_resources: Vec::new(),
            },
        ),
        Err(DomainError::InvalidPersistentChangeSet { .. })
    ));
    assert_eq!(doc, before);
    Ok(())
}

/// WaitingUser Ask survives a raw reload while its API remains open; answers
/// and cancellation stay on the original tool seq, and Continue never
/// manufactures another user input.
/// WaitingUser Ask 在 API 仍开放的原始重载中保持等待；回答与取消都留在原工具
/// seq，Continue 绝不制造另一条用户输入。
#[test]
fn waiting_ask_answer_cancel_and_continue_keep_one_turn() -> Result<(), DomainError> {
    let mut doc = new_document();
    let turn_id = doc.submit_root_input_fixture("ask", Vec::new(), T0)?;
    let api = doc.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0,
    )?;
    let ask = doc.prepare_tool(
        api,
        RequestStreamId::try_new("stream-ask-1")?,
        "ask",
        ToolPayload::try_new(serde_json::json!({"question": "continue?"}))?,
        None,
        T0 + 1,
    )?;
    doc.start_prepared_tool(api, ask, T0 + 1)?;
    doc.mark_tool_waiting_user(api, ask, T0 + 2)?;
    let ask_seq = doc.tool_seq_by_id[&ask];
    let mut restored = SessionDocument::from_parts(
        doc.metadata.clone(),
        initialized_runtime_lock(),
        Vec::new(),
        Vec::new(),
        doc.events().to_vec(),
    )?;
    restored.answer_waiting_tool(
        api,
        ask,
        tool_completion(
            Some(ToolPayload::try_new(serde_json::json!({"answer": "yes"}))?),
            ToolCallStatus::Completed,
            T0 + 3,
        ),
    )?;
    complete_user_interrupted(&mut restored, api, T0 + 4)?;
    restored.continue_turn_fixture(T0 + 4)?;
    assert!(matches!(
        restored.events().iter().find(|event| event.seq() == ask_seq),
        Some(ConversationEvent {
            turn_id: Some(value),
            event: EventBody::ToolCall(ToolCallEvent {
                status: ToolCallStatus::Completed,
                ..
            }),
            ..
        }) if *value == turn_id
    ));

    let api2 = restored.start_api(
        ApiPurposeKind::Continue,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 5,
    )?;
    let ask2 = restored.prepare_tool(
        api2,
        RequestStreamId::try_new("stream-ask-2")?,
        "ask",
        ToolPayload::try_new(serde_json::json!({"question": "cancel?"}))?,
        None,
        T0 + 6,
    )?;
    restored.start_prepared_tool(api2, ask2, T0 + 6)?;
    restored.mark_tool_waiting_user(api2, ask2, T0 + 6)?;
    let ask2_seq = restored.tool_seq_by_id[&ask2];
    restored.cancel_waiting_tool(api2, ask2, None, T0 + 8)?;
    complete_ok(&mut restored, api2, None, T0 + 9)?;
    assert!(matches!(
        restored.events().iter().find(|event| event.seq() == ask2_seq),
        Some(ConversationEvent {
            event: EventBody::ToolCall(ToolCallEvent {
                status: ToolCallStatus::Cancelled,
                finished_at_ms: Some(value),
                ..
            }),
            ..
        }) if *value == T0 + 8
    ));
    assert_eq!(
        restored
            .events()
            .iter()
            .filter(|event| matches!(event.event, EventBody::UserInput(_)))
            .count(),
        1
    );
    assert_eq!(
        restored.active_turn().map(|turn| turn.turn_id),
        Some(turn_id)
    );
    Ok(())
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

/// Turn completion selects queued users before Goal continuation and preserves FIFO on restart.
/// 回合完成在 Goal 续跑前选择排队用户，并在重启后保持 FIFO。
#[test]
fn turn_completion_selects_one_queued_successor_before_goal() -> Result<(), DomainError> {
    let mut doc = new_document();
    let first_turn = doc.submit_root_input_fixture("root", Vec::new(), T0)?;
    doc.create_goal("finish", None, "Keep working.", T0 + 1)?;
    let queued_first_action = ClientActionId::try_new("queued-first")?;
    let queued_second_action = ClientActionId::try_new("queued-second")?;
    let queued_first = doc
        .enqueue_input(
            ClientInputSubmission::new(queued_first_action.clone(), "first queued", Vec::new()),
            T0 + 2,
        )?
        .into_value()
        .input_id;
    let queued_second = doc
        .enqueue_input(
            ClientInputSubmission::new(queued_second_action.clone(), "second queued", Vec::new()),
            T0 + 3,
        )?
        .into_value()
        .input_id;
    let api = doc.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 4,
    )?;
    doc.append_assistant_text(api, "done", AssistantSemanticStatus::Completed, T0 + 5)?;
    complete_ok(&mut doc, api, None, T0 + 6)?;
    let transition = doc.complete_turn_and_select_successor(first_turn, T0 + 7)?;
    let TurnSuccessor::QueuedUser {
        turn_id: first_queued_turn,
        input_id,
        client_action_id,
        input_seq,
    } = transition.successor
    else {
        panic!("queued user must outrank active Goal");
    };
    assert_eq!(input_id, queued_first);
    assert_eq!(client_action_id, queued_first_action);
    assert_eq!(doc.metadata.input_state.queued_inputs.len(), 1);
    assert_eq!(
        doc.metadata.input_state.queued_inputs[0].input_id,
        queued_second
    );
    assert_eq!(doc.metadata.mode.goal.state, GoalState::Active);
    assert!(matches!(
        doc.events().iter().find(|event| event.seq == input_seq),
        Some(ConversationEvent {
            turn_id: Some(value),
            event: EventBody::UserInput(UserInputEvent {
                input_source: UserInputSource::QueuedScheduled,
                ..
            }),
            ..
        }) if *value == first_queued_turn
    ));

    let mut restored = SessionDocument::from_parts(
        doc.metadata.clone(),
        doc.runtime_lock.clone(),
        doc.compactions.clone(),
        doc.segment_catalog().to_vec(),
        doc.events().to_vec(),
    )?;
    assert_eq!(
        restored.active_turn().map(|turn| turn.turn_id),
        Some(first_queued_turn)
    );
    assert_eq!(restored.metadata.input_state.queued_inputs.len(), 1);
    let api = restored.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 8,
    )?;
    restored.append_assistant_text(api, "done", AssistantSemanticStatus::Completed, T0 + 9)?;
    complete_ok(&mut restored, api, None, T0 + 10)?;
    let second = restored.complete_turn_and_select_successor(first_queued_turn, T0 + 11)?;
    let TurnSuccessor::QueuedUser {
        turn_id: second_queued_turn,
        input_id,
        client_action_id,
        ..
    } = second.successor
    else {
        panic!("second FIFO item must become the next queued successor");
    };
    assert_eq!(input_id, queued_second);
    assert_eq!(client_action_id, queued_second_action);
    assert!(restored.metadata.input_state.queued_inputs.is_empty());

    let api = restored.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 12,
    )?;
    restored.append_assistant_text(api, "done", AssistantSemanticStatus::Completed, T0 + 13)?;
    complete_ok(&mut restored, api, None, T0 + 14)?;
    let after_queue = restored.complete_turn_and_select_successor(second_queued_turn, T0 + 15)?;
    assert!(matches!(
        after_queue.successor,
        TurnSuccessor::GoalContinuation { .. }
    ));
    Ok(())
}

/// Queued successor completion is failure-atomic without an active Goal.
/// 不存在活动 Goal 时，排队后继完成仍保持失败原子性。
#[test]
fn queued_successor_completion_is_atomic_without_goal() -> Result<(), DomainError> {
    let mut doc = new_document();
    let turn_id = doc.submit_root_input_fixture("root", Vec::new(), T0)?;
    let queued_id = doc
        .enqueue_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("queued-no-goal")?,
                "queued",
                Vec::new(),
            ),
            T0 + 1,
        )?
        .into_value()
        .input_id;
    let api = doc.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 2,
    )?;
    doc.append_assistant_text(api, "done", AssistantSemanticStatus::Completed, T0 + 3)?;
    complete_ok(&mut doc, api, None, T0 + 4)?;

    let (_, next_seq) = doc.id_allocator.frontier();
    let mut local_exhausted = doc.clone();
    local_exhausted.id_allocator = IdAllocator::from_frontier(JS_SAFE_INTEGER_MAX + 1, next_seq);
    let before_local_failure = local_exhausted.clone();
    assert!(
        local_exhausted
            .complete_turn_and_select_successor(turn_id, T0 + 5)
            .is_err()
    );
    assert_eq!(local_exhausted, before_local_failure);

    let (next_local_id, _) = doc.id_allocator.frontier();
    let mut seq_exhausted = doc.clone();
    seq_exhausted.id_allocator = IdAllocator::from_frontier(next_local_id, JS_SAFE_INTEGER_MAX + 1);
    let before_seq_failure = seq_exhausted.clone();
    assert!(
        seq_exhausted
            .complete_turn_and_select_successor(turn_id, T0 + 5)
            .is_err()
    );
    assert_eq!(seq_exhausted, before_seq_failure);

    let completed = doc.complete_turn_and_select_successor(turn_id, T0 + 5)?;
    assert!(matches!(
        completed.successor,
        TurnSuccessor::QueuedUser { input_id, .. } if input_id == queued_id
    ));
    Ok(())
}

/// A queued user input atomically supersedes an undispatched Goal continuation and is restart-idempotent.
/// 一条排队用户输入会原子取代尚未派发的 Goal 续跑，并在重启后保持幂等。
#[test]
fn enqueue_preempts_undispatched_goal_and_replays_exact_receipt() -> Result<(), DomainError> {
    let mut doc = new_document();
    let goal = doc.create_goal("finish", None, "Keep working.", T0 + 1)?;
    let goal_turn = goal
        .continuation_turn_id
        .expect("creating a Goal without an open Turn must start its continuation");
    let action_id = ClientActionId::try_new("goal-preempting-user")?;
    let submission = ClientInputSubmission::new(action_id.clone(), "user priority", Vec::new());
    let before_revision = doc.memory_revision();

    let committed = doc.enqueue_input(submission.clone(), T0 + 2)?;
    assert!(committed.changed());
    let receipt = committed.into_value();
    let EnqueueInputDisposition::SupersededGoalAndScheduled {
        stopped_turn_id,
        stopped_turn_stats_seq,
        scheduled_turn_id,
        input_seq,
    } = receipt.disposition.clone()
    else {
        panic!("an undispatched Goal continuation must be superseded immediately");
    };
    assert_eq!(stopped_turn_id, goal_turn);
    assert_eq!(doc.memory_revision(), before_revision + 1);
    assert!(doc.metadata.input_state.queued_inputs.is_empty());
    assert_eq!(
        doc.active_turn()
            .map(|turn| (turn.turn_id, turn.origin, turn.start_seq, turn.closed)),
        Some((scheduled_turn_id, TurnOrigin::UserRoot, input_seq, false))
    );
    assert_eq!(
        doc.events()
            .iter()
            .filter(|event| {
                event.turn_id == Some(goal_turn) && matches!(event.event, EventBody::TurnStats(_))
            })
            .count(),
        1
    );
    assert!(matches!(
        doc.events()
            .iter()
            .find(|event| event.seq == stopped_turn_stats_seq)
            .map(|event| &event.event),
        Some(EventBody::TurnStats(TurnStatsEvent {
            outcome: TurnOutcome::Stopped,
            ..
        }))
    ));

    let mut restored = SessionDocument::from_parts(
        doc.metadata.clone(),
        doc.runtime_lock.clone(),
        doc.compactions.clone(),
        doc.segment_catalog().to_vec(),
        doc.events().to_vec(),
    )?;
    let restored_revision = restored.memory_revision();
    let event_count = restored.events().len();
    let replay = restored.enqueue_input(submission.clone(), T0 + 3)?;
    assert!(!replay.changed());
    assert_eq!(replay.into_value(), receipt);
    assert_eq!(restored.memory_revision(), restored_revision);
    assert_eq!(restored.events().len(), event_count);

    let before_conflict = restored.clone();
    assert!(matches!(
        restored.enqueue_input(
            ClientInputSubmission::new(action_id, "different", Vec::new()),
            T0 + 4,
        ),
        Err(DomainError::ClientActionConflict { .. })
    ));
    assert_eq!(restored, before_conflict);
    Ok(())
}

/// Goal preemption stages all ids and events before publishing, while dispatched Goal work only queues.
/// Goal 抢占会在发布前暂存全部 id 与事件，而已经派发的 Goal 工作只会进入队列。
#[test]
fn enqueue_goal_preemption_is_failure_atomic_and_never_crosses_active_api()
-> Result<(), DomainError> {
    let mut doc = new_document();
    doc.create_goal("finish", None, "Keep working.", T0 + 1)?;
    let (_, next_seq) = doc.id_allocator.frontier();
    let mut exhausted = doc.clone();
    exhausted.id_allocator = IdAllocator::from_frontier(JS_SAFE_INTEGER_MAX + 1, next_seq);
    let before_failure = exhausted.clone();
    assert!(
        exhausted
            .enqueue_input(
                ClientInputSubmission::new(
                    ClientActionId::try_new("goal-preempt-allocation-failure")?,
                    "user priority",
                    Vec::new(),
                ),
                T0 + 2,
            )
            .is_err()
    );
    assert_eq!(exhausted, before_failure);

    let api = doc.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 2,
    )?;
    let goal_turn = doc.require_open_turn()?.turn_id;
    let outcome = doc.enqueue_input(
        ClientInputSubmission::new(
            ClientActionId::try_new("goal-active-api-queued")?,
            "wait behind active Goal API",
            Vec::new(),
        ),
        T0 + 3,
    )?;
    assert!(matches!(
        outcome.into_value().disposition,
        EnqueueInputDisposition::Queued
    ));
    assert_eq!(doc.active_api().map(|active| active.api_call_id), Some(api));
    assert_eq!(doc.active_turn().map(|turn| turn.turn_id), Some(goal_turn));
    assert_eq!(doc.metadata.input_state.queued_inputs.len(), 1);
    Ok(())
}

/// Draft and queued-input CRUD remain metadata-authoritative, idempotent, and strict after reload.
/// 草稿与排队输入 CRUD 在重载后仍保持元数据权威、幂等且严格。
#[test]
fn draft_and_queued_crud_are_atomic_idempotent_and_reload_strict() -> Result<(), DomainError> {
    let mut doc = new_document();
    let first = doc
        .enqueue_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("queue-crud-first")?,
                "first",
                vec![ResourceId::try_new(11)?],
            ),
            T0 + 1,
        )?
        .into_value();
    let second = doc
        .enqueue_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("queue-crud-second")?,
                "second",
                vec![ResourceId::try_new(12)?],
            ),
            T0 + 2,
        )?
        .into_value();
    let first_identity = QueuedInputIdentity {
        client_action_id: first.client_action_id,
        input_id: first.input_id,
    };
    let second_identity = QueuedInputIdentity {
        client_action_id: second.client_action_id,
        input_id: second.input_id,
    };
    assert_eq!(
        doc.queued_input_identity_for_action(&first_identity.client_action_id)?,
        Some(first_identity.clone())
    );
    assert_eq!(
        doc.queued_input_identity_for_action(&second_identity.client_action_id)?,
        Some(second_identity.clone())
    );

    let reverse = vec![second_identity.clone(), first_identity.clone()];
    assert!(
        doc.reorder_queued_inputs(reverse.clone(), T0 + 3)?
            .changed()
    );
    let revision = doc.memory_revision();
    assert!(
        !doc.reorder_queued_inputs(reverse.clone(), T0 + 4)?
            .changed()
    );
    assert_eq!(doc.memory_revision(), revision);
    let before_bad_order = doc.clone();
    assert!(matches!(
        doc.reorder_queued_inputs(vec![first_identity.clone()], T0 + 5),
        Err(DomainError::InvalidQueuedInputOrder {
            field: "member_count"
        })
    ));
    assert_eq!(doc, before_bad_order);

    let moved = doc.move_queued_input_to_draft(second_identity.clone(), T0 + 6)?;
    assert!(moved.changed());
    assert_eq!(
        moved.into_value().draft.restored_from_queue,
        Some(second_identity.clone())
    );
    assert_eq!(doc.metadata.input_state.queued_inputs.len(), 1);
    assert_eq!(
        doc.queued_input_identity_for_action(&second_identity.client_action_id)?,
        Some(second_identity.clone())
    );
    let mut restored = SessionDocument::from_parts(
        doc.metadata.clone(),
        doc.runtime_lock.clone(),
        doc.compactions.clone(),
        doc.segment_catalog().to_vec(),
        doc.events().to_vec(),
    )?;
    let revision = restored.memory_revision();
    assert!(
        !restored
            .move_queued_input_to_draft(second_identity.clone(), T0 + 7)?
            .changed()
    );
    assert_eq!(restored.memory_revision(), revision);

    assert!(
        restored
            .set_draft("manual replacement", Vec::new(), T0 + 8)?
            .changed()
    );
    assert!(
        restored
            .metadata
            .input_state
            .draft
            .restored_from_queue
            .is_none()
    );
    let revision = restored.memory_revision();
    let replayed_move = restored.move_queued_input_to_draft(second_identity, T0 + 9)?;
    assert!(!replayed_move.changed());
    assert_eq!(restored.memory_revision(), revision);
    assert_eq!(replayed_move.into_value().draft.text, "second");
    let revision = restored.memory_revision();
    assert!(
        !restored
            .set_draft("manual replacement", Vec::new(), T0 + 10)?
            .changed()
    );
    assert_eq!(restored.memory_revision(), revision);
    assert!(restored.clear_draft(T0 + 11)?.changed());
    let revision = restored.memory_revision();
    assert!(!restored.clear_draft(T0 + 12)?.changed());
    assert_eq!(restored.memory_revision(), revision);

    assert!(
        restored
            .remove_queued_input(first_identity.clone(), T0 + 13)?
            .changed()
    );
    let persisted_metadata = serde_json::from_str(
        &serde_json::to_string(&restored.metadata).expect("metadata must serialize"),
    )
    .expect("metadata must strictly reload");
    let persisted_events = serde_json::from_str(
        &serde_json::to_string(restored.events()).expect("events must serialize"),
    )
    .expect("events must strictly reload");
    let mut restored = SessionDocument::from_parts(
        persisted_metadata,
        restored.runtime_lock.clone(),
        restored.compactions.clone(),
        restored.segment_catalog().to_vec(),
        persisted_events,
    )?;
    let revision = restored.memory_revision();
    let replayed_admission = restored.enqueue_input(
        ClientInputSubmission::new(
            first_identity.client_action_id.clone(),
            "first",
            vec![ResourceId::try_new(11)?],
        ),
        T0 + 14,
    )?;
    assert!(!replayed_admission.changed());
    assert_eq!(
        replayed_admission.into_value().input_id,
        first_identity.input_id
    );
    assert_eq!(restored.memory_revision(), revision);
    let later = restored
        .enqueue_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("queue-crud-later")?,
                "later",
                Vec::new(),
            ),
            T0 + 15,
        )?
        .into_value();
    let revision = restored.memory_revision();
    assert!(
        !restored
            .remove_queued_input(first_identity.clone(), T0 + 16)?
            .changed()
    );
    assert_eq!(restored.memory_revision(), revision);
    assert_eq!(restored.metadata.input_state.queued_inputs.len(), 1);
    assert_eq!(
        restored.metadata.input_state.queued_inputs[0].input_id,
        later.input_id
    );
    assert_eq!(
        restored.queued_input_identity_for_action(&first_identity.client_action_id)?,
        Some(first_identity.clone())
    );
    assert_eq!(
        restored.queued_input_identity_for_action(&ClientActionId::try_new(
            "queue-crud-unknown-action"
        )?)?,
        None
    );

    let wrong_local_id = QueuedInputIdentity {
        client_action_id: first_identity.client_action_id.clone(),
        input_id: PendingInputId::try_new(first_identity.input_id.value() + 100)?,
    };
    let before_conflict = restored.clone();
    assert!(matches!(
        restored.remove_queued_input(wrong_local_id, T0 + 17),
        Err(DomainError::ClientActionConflict { .. })
    ));
    assert_eq!(restored, before_conflict);
    let unknown = QueuedInputIdentity {
        client_action_id: ClientActionId::try_new("queue-crud-unknown")?,
        input_id: PendingInputId::try_new(first_identity.input_id.value() + 101)?,
    };
    let before_missing = restored.clone();
    assert!(matches!(
        restored.remove_queued_input(unknown, T0 + 18),
        Err(DomainError::QueuedInputNotFound { .. })
    ));
    assert_eq!(restored, before_missing);
    Ok(())
}

/// User-input admission consumes only the exact durable draft and preserves later edits.
/// 用户输入准入只消费完全匹配的持久化草稿，并保留随后产生的新编辑。
#[test]
fn input_admission_consumes_only_the_matching_draft_atomically() -> Result<(), DomainError> {
    let root_resources = vec![ResourceId::try_new(101)?, ResourceId::try_new(102)?];
    let root_action = ClientActionId::try_new("draft-root-admission")?;
    let root_submission =
        ClientInputSubmission::new(root_action, "root draft", root_resources.clone());
    let mut root = new_document();
    root.set_draft("root draft", root_resources, T0 + 1)?;
    root.submit_root_input(root_submission.clone(), T0 + 2)?;
    assert_eq!(root.metadata.input_state.draft, DraftState::default());
    root.set_draft("newer root draft", Vec::new(), T0 + 3)?;
    let revision = root.memory_revision();
    assert!(!root.submit_root_input(root_submission, T0 + 4)?.changed());
    assert_eq!(root.memory_revision(), revision);
    assert_eq!(root.metadata.input_state.draft.text, "newer root draft");

    let queued_resources = vec![ResourceId::try_new(201)?];
    let queued_submission = ClientInputSubmission::new(
        ClientActionId::try_new("draft-queued-admission")?,
        "queued draft",
        queued_resources.clone(),
    );
    let mut queued = new_document();
    queued.set_draft("queued draft", queued_resources, T0 + 5)?;
    queued.enqueue_input(queued_submission.clone(), T0 + 6)?;
    assert_eq!(queued.metadata.input_state.draft, DraftState::default());
    queued.set_draft("newer queued draft", Vec::new(), T0 + 7)?;
    let revision = queued.memory_revision();
    assert!(!queued.enqueue_input(queued_submission, T0 + 8)?.changed());
    assert_eq!(queued.memory_revision(), revision);
    assert_eq!(queued.metadata.input_state.draft.text, "newer queued draft");

    let mut pending = new_document();
    pending.submit_root_input_fixture("open", Vec::new(), T0 + 9)?;
    let pending_resources = vec![ResourceId::try_new(301)?];
    let pending_submission = ClientInputSubmission::new(
        ClientActionId::try_new("draft-pending-admission")?,
        "pending draft",
        pending_resources.clone(),
    );
    pending.set_draft("pending draft", pending_resources, T0 + 10)?;
    pending.add_pending_in_turn_input(pending_submission.clone(), T0 + 11)?;
    assert_eq!(pending.metadata.input_state.draft, DraftState::default());
    pending.set_draft("newer pending draft", Vec::new(), T0 + 12)?;
    let revision = pending.memory_revision();
    assert!(
        !pending
            .add_pending_in_turn_input(pending_submission, T0 + 13)?
            .changed()
    );
    assert_eq!(pending.memory_revision(), revision);
    assert_eq!(
        pending.metadata.input_state.draft.text,
        "newer pending draft"
    );

    let mut continued = new_document();
    continued.submit_root_input_fixture("continue", Vec::new(), T0 + 14)?;
    let api = continued.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 15,
    )?;
    complete_user_interrupted(&mut continued, api, T0 + 16)?;
    let continue_resources = vec![ResourceId::try_new(401)?];
    let continue_request = ContinuationRequest::new(
        ClientActionId::try_new("draft-continue-admission")?,
        "continue draft",
        continue_resources.clone(),
    );
    continued.set_draft("continue draft", continue_resources, T0 + 17)?;
    continued.continue_open_turn(continue_request.clone(), T0 + 18)?;
    assert_eq!(continued.metadata.input_state.draft, DraftState::default());
    continued.set_draft("newer continue draft", Vec::new(), T0 + 19)?;
    let revision = continued.memory_revision();
    assert!(
        !continued
            .continue_open_turn(continue_request, T0 + 20)?
            .changed()
    );
    assert_eq!(continued.memory_revision(), revision);
    assert_eq!(
        continued.metadata.input_state.draft.text,
        "newer continue draft"
    );

    let mut rejected = new_document();
    rejected.submit_root_input_fixture("already open", Vec::new(), T0 + 21)?;
    rejected.set_draft("must survive", Vec::new(), T0 + 22)?;
    let before_rejection = rejected.clone();
    assert!(matches!(
        rejected.submit_root_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("draft-rejected-root")?,
                "must survive",
                Vec::new(),
            ),
            T0 + 23,
        ),
        Err(DomainError::RootInputWhileTurnOpen { .. })
    ));
    assert_eq!(rejected, before_rejection);
    Ok(())
}

/// Guided queue input moves into the open Turn once and survives promotion plus strict reload.
/// 引导的排队输入只进入开放回合一次，并在提升及严格重载后保持幂等。
#[test]
fn guided_queued_input_is_durable_idempotent_and_same_turn() -> Result<(), DomainError> {
    let mut document = new_document();
    let root_turn_id = document.submit_root_input_fixture("root", Vec::new(), T0 + 1)?;
    let api_call_id = document.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 2,
    )?;
    let queued = document
        .enqueue_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("guided-queued-input")?,
                "supplement",
                vec![ResourceId::try_new(501)?],
            ),
            T0 + 3,
        )?
        .into_value();
    let identity = QueuedInputIdentity {
        client_action_id: queued.client_action_id,
        input_id: queued.input_id,
    };

    let first = document.promote_queued_input_to_pending_turn(identity.clone(), T0 + 4)?;
    assert!(first.changed());
    let first_receipt = first.into_value();
    assert_eq!(first_receipt.turn_id, root_turn_id);
    assert!(document.metadata.input_state.queued_inputs.is_empty());
    assert_eq!(
        document.metadata.input_state.pending_in_turn_inputs.len(),
        1
    );
    assert_eq!(
        document.metadata.input_state.pending_in_turn_inputs[0].input_id,
        identity.input_id
    );

    let revision = document.memory_revision();
    let retry = document.promote_queued_input_to_pending_turn(identity.clone(), T0 + 5)?;
    assert!(!retry.changed());
    assert_eq!(retry.into_value(), first_receipt);
    assert_eq!(document.memory_revision(), revision);

    document.append_assistant_text(
        api_call_id,
        "continue",
        AssistantSemanticStatus::Completed,
        T0 + 6,
    )?;
    let promoted = document
        .complete_api_group(
            api_call_id,
            ApiCompletion {
                request: ApiRequestSnapshot::new("openai", "gpt-5"),
                usage: usage(10, 20),
                status: ApiCallStatus::Completed,
                end_reason: Some(ApiEndReason::CompletedSuccessfully),
                error: None,
                continuation_eligibility: ContinuationEligibility::Forbidden,
                timing: ApiTimings::default(),
                provider_source: None,
                completed_at_ms: T0 + 7,
            },
        )?
        .promoted_input_seqs;
    assert_eq!(promoted.len(), 1);
    let promoted_event = document
        .events()
        .iter()
        .find(|event| event.seq == promoted[0])
        .expect("promoted user input must exist");
    assert_eq!(promoted_event.turn_id, Some(root_turn_id));
    assert!(matches!(
        &promoted_event.event,
        EventBody::UserInput(input)
            if input.input_source == UserInputSource::PendingInTurn
                && input.input_id == identity.input_id
                && input.client_action_id == identity.client_action_id
    ));

    let mut restored = SessionDocument::from_parts(
        document.metadata.clone(),
        document.runtime_lock.clone(),
        document.compactions.clone(),
        document.segment_catalog().to_vec(),
        document.events().to_vec(),
    )?;
    let restored_revision = restored.memory_revision();
    let restored_retry = restored.promote_queued_input_to_pending_turn(identity, T0 + 8)?;
    assert!(!restored_retry.changed());
    assert_eq!(restored_retry.into_value(), first_receipt);
    assert_eq!(restored.memory_revision(), restored_revision);
    Ok(())
}

/// Guided queue input cannot leave the future queue when no logical Turn is open.
/// 没有开放逻辑回合时，引导操作不能让输入离开未来队列。
#[test]
fn guided_queued_input_without_open_turn_is_failure_atomic() -> Result<(), DomainError> {
    let mut document = new_document();
    let queued = document
        .enqueue_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("guided-without-turn")?,
                "queued",
                Vec::new(),
            ),
            T0 + 1,
        )?
        .into_value();
    let identity = QueuedInputIdentity {
        client_action_id: queued.client_action_id,
        input_id: queued.input_id,
    };
    let before = document.clone();
    assert!(
        document
            .promote_queued_input_to_pending_turn(identity, T0 + 2)
            .is_err()
    );
    assert_eq!(document, before);
    Ok(())
}

/// Queued terminal resolutions publish no partial metadata when seq validation fails.
/// 排队输入终结处置在 seq 校验失败时不发布任何部分元数据。
#[test]
fn queued_terminal_resolutions_are_failure_atomic() -> Result<(), DomainError> {
    let mut removed = new_document();
    let remove_receipt = removed
        .enqueue_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("queue-remove-seq-failure")?,
                "remove",
                Vec::new(),
            ),
            T0 + 1,
        )?
        .into_value();
    let remove_identity = QueuedInputIdentity {
        client_action_id: remove_receipt.client_action_id,
        input_id: remove_receipt.input_id,
    };
    let (next_local_id, next_seq) = removed.id_allocator.frontier();
    removed.id_allocator = IdAllocator::from_frontier(next_local_id, next_seq + 1);
    let before_remove = removed.clone();
    assert!(matches!(
        removed.remove_queued_input(remove_identity, T0 + 2),
        Err(DomainError::SequenceViolation { .. })
    ));
    assert_eq!(removed, before_remove);

    let mut moved = new_document();
    let move_receipt = moved
        .enqueue_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("queue-move-seq-failure")?,
                "move",
                Vec::new(),
            ),
            T0 + 3,
        )?
        .into_value();
    let move_identity = QueuedInputIdentity {
        client_action_id: move_receipt.client_action_id,
        input_id: move_receipt.input_id,
    };
    let (next_local_id, next_seq) = moved.id_allocator.frontier();
    moved.id_allocator = IdAllocator::from_frontier(next_local_id, next_seq + 1);
    let before_move = moved.clone();
    assert!(matches!(
        moved.move_queued_input_to_draft(move_identity, T0 + 4),
        Err(DomainError::SequenceViolation { .. })
    ));
    assert_eq!(moved, before_move);
    Ok(())
}

/// Strict reload rejects a queue-restored draft whose local input id is still reused elsewhere.
/// 严格重载会拒绝本地输入 id 仍在其他位置复用的队列恢复草稿。
#[test]
fn reload_rejects_reused_queue_restored_draft_identity() -> Result<(), DomainError> {
    let mut doc = new_document();
    let receipt = doc
        .enqueue_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("queue-corrupt-draft")?,
                "queued",
                Vec::new(),
            ),
            T0 + 1,
        )?
        .into_value();
    let identity = QueuedInputIdentity {
        client_action_id: ClientActionId::try_new("different-action")?,
        input_id: receipt.input_id,
    };
    doc.metadata.input_state.draft = DraftState {
        text: "draft".to_string(),
        resource_ids: Vec::new(),
        restored_from_queue: Some(identity),
    };
    assert!(matches!(
        SessionDocument::from_parts(
            doc.metadata.clone(),
            doc.runtime_lock.clone(),
            doc.compactions.clone(),
            doc.segment_catalog().to_vec(),
            doc.events().to_vec(),
        ),
        Err(DomainError::InvalidQueuedInputOrder {
            field: "draft_input_id_reused"
        })
    ));
    Ok(())
}

/// Goal budget exhaustion is computed from persisted statistics and recovery
/// cannot manufacture a successor or replay the completed transition.
/// Goal 预算耗尽只从持久化统计计算，恢复后既不能制造后继回合，也不能重放已完成转换。
#[test]
fn goal_budget_exhaustion_and_successor_recovery_are_exactly_once() -> Result<(), DomainError> {
    let mut exhausted = new_document();
    exhausted.metadata.mode.goal.enabled = true;
    exhausted.metadata.mode.goal.state = GoalState::Active;
    exhausted.metadata.mode.goal.objective = "finish".to_string();
    exhausted.metadata.mode.goal.budget_tokens = Some(30);
    let turn = exhausted.submit_root_input_fixture("work", Vec::new(), T0)?;
    exhausted.metadata.mode.goal.origin_turn_id = Some(turn);
    exhausted.metadata.mode.goal.origin_seq = exhausted.active_turn().map(|value| value.start_seq);
    let api = exhausted.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 1,
    )?;
    exhausted.append_assistant_text(api, "done", AssistantSemanticStatus::Completed, T0 + 2)?;
    complete_ok(&mut exhausted, api, None, T0 + 3)?;
    let transition = exhausted.complete_turn_and_select_successor(turn, T0 + 4)?;
    assert_eq!(transition.successor, TurnSuccessor::None);
    assert_eq!(
        exhausted.metadata.mode.goal.state,
        GoalState::BudgetExhausted
    );
    let cache = exhausted
        .metadata
        .mode
        .goal
        .budget_cache
        .as_ref()
        .expect("known persisted usage must build a cache");
    assert_eq!(cache.total_tokens, 30);
    assert_eq!(cache.covered_until_seq, transition.completed_turn_final_seq);

    let restored = SessionDocument::from_parts(
        exhausted.metadata.clone(),
        initialized_runtime_lock(),
        Vec::new(),
        Vec::new(),
        exhausted.events().to_vec(),
    )?;
    assert!(restored.active_turn().is_none());
    assert!(!restored.events().iter().any(|event| {
        matches!(
            &event.event,
            EventBody::SystemContext(context) if context.context_key == "goal_continuation"
        )
    }));

    let mut continuing = new_document();
    continuing.metadata.mode.goal.enabled = true;
    continuing.metadata.mode.goal.state = GoalState::Active;
    continuing.metadata.mode.goal.objective = "continue".to_string();
    let first = continuing.submit_root_input_fixture("work", Vec::new(), T0)?;
    continuing.metadata.mode.goal.origin_turn_id = Some(first);
    continuing.metadata.mode.goal.origin_seq =
        continuing.active_turn().map(|value| value.start_seq);
    let api = continuing.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 1,
    )?;
    continuing.append_assistant_text(api, "done", AssistantSemanticStatus::Completed, T0 + 2)?;
    complete_ok(&mut continuing, api, None, T0 + 3)?;
    let completed = continuing.complete_turn_and_select_successor(first, T0 + 4)?;
    let TurnSuccessor::GoalContinuation {
        turn_id: successor, ..
    } = completed.successor
    else {
        panic!("active unbudgeted Goal must continue");
    };
    let mut recovered = SessionDocument::from_parts(
        continuing.metadata.clone(),
        initialized_runtime_lock(),
        Vec::new(),
        Vec::new(),
        continuing.events().to_vec(),
    )?;
    assert_eq!(
        recovered.active_turn().map(|value| value.turn_id),
        Some(successor)
    );
    assert!(matches!(
        recovered.complete_turn_and_select_successor(first, T0 + 5),
        Err(DomainError::TurnIdentityMismatch { expected, actual })
            if expected == first.value() && actual == successor.value()
    ));
    Ok(())
}

/// A durable child return round-trips and exact delivery retry is revision-idempotent.
/// 可靠子代理回流可以严格往返，且精确重复投递不会制造新修订。
#[test]
fn subagent_return_round_trips_and_exact_retry_is_idempotent() -> Result<(), DomainError> {
    let (child, child_turn_id, source_assistant_text_seq) = durable_child_return_fixture()?;
    let input = child.build_subagent_return_input(SubagentReturnBuildRequest {
        final_turn_id: child_turn_id,
        child_task_id: SubagentTaskId::try_new("task-final")?,
        source_assistant_text_seq,
    })?;
    assert_eq!(input.child_session_id, "child-session");
    assert_eq!(input.content, "final child result");
    assert_eq!(input.totals.api_call_count, 2);
    assert_eq!(input.totals.unknown_usage_api_call_count, 1);
    assert_eq!(input.totals.tool_call_count, 1);

    let mut parent = new_document();
    let parent_turn = parent.submit_root_input_fixture("delegate", Vec::new(), T0)?;
    let committed = parent.append_subagent_return_with_stats(input.clone(), T0 + 1)?;
    assert!(committed.changed());
    let receipt = committed.into_value();
    assert_eq!(receipt.parent_turn_id, parent_turn);
    assert_eq!(receipt.source_assistant_text_seq, source_assistant_text_seq);
    assert_eq!(
        parent.pending_subagent_return_dispatch()?,
        Some(receipt.clone())
    );
    let mut covered = parent.clone();
    covered.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 2,
    )?;
    assert_eq!(covered.pending_subagent_return_dispatch()?, None);
    let revision_after_commit = parent.memory_revision();
    let repeated = parent.append_subagent_return_with_stats(input.clone(), T0 + 2)?;
    assert!(!repeated.changed());
    assert_eq!(repeated.into_value(), receipt);
    assert_eq!(parent.memory_revision(), revision_after_commit);

    advance_fixture_durable(&mut parent)?;
    let mut restored = SessionDocument::from_parts(
        parent.metadata.clone(),
        parent.runtime_lock.clone(),
        parent.compactions.clone(),
        parent.segment_catalog.clone(),
        parent.events.clone(),
    )?;
    let restored_revision = restored.memory_revision();
    let replayed = restored.append_subagent_return_with_stats(input, T0 + 3)?;
    assert!(!replayed.changed());
    assert_eq!(replayed.into_value(), receipt);
    assert_eq!(restored.memory_revision(), restored_revision);

    restored.close_turn(TurnOutcome::Stopped, 4, T0 + 4)?;
    let turn_stats = restored
        .events()
        .iter()
        .find_map(|event| match &event.event {
            EventBody::TurnStats(value) => Some(value),
            _ => None,
        })
        .expect("turn stats must exist");
    assert_eq!(turn_stats.buckets.subagent.api_call_count, 2);
    assert_eq!(turn_stats.buckets.subagent.unknown_usage_api_call_count, 1);
    assert_eq!(turn_stats.buckets.subagent.tool_call_count, 1);
    assert_eq!(turn_stats.buckets.subagent.subagent_return_count, 1);
    assert_eq!(turn_stats.buckets.total.usage.input_tokens, None);
    assert_eq!(turn_stats.buckets.total.usage.output_tokens, None);
    Ok(())
}

/// Child return selection rejects non-durable state and an imprecise final text seq.
/// 子代理回流选择会拒绝未可靠状态与不精确的最终正文 seq。
#[test]
fn subagent_return_builder_requires_durable_exact_terminal_child() -> Result<(), DomainError> {
    let (child, child_turn_id, source_assistant_text_seq) = durable_child_return_fixture()?;
    let request = SubagentReturnBuildRequest {
        final_turn_id: child_turn_id,
        child_task_id: SubagentTaskId::try_new("task-final")?,
        source_assistant_text_seq,
    };

    let mut dirty = child.clone();
    dirty.rename_session("renamed child", T0 + 20)?;
    assert!(matches!(
        dirty.build_subagent_return_input(request.clone()),
        Err(DomainError::SubagentSourceNotDurable { .. })
    ));

    let mut wrong_source = request;
    wrong_source.source_assistant_text_seq = child
        .events()
        .iter()
        .find_map(|event| matches!(&event.event, EventBody::ToolCall(_)).then_some(event.seq))
        .expect("child tool event");
    assert!(matches!(
        child.build_subagent_return_input(wrong_source),
        Err(DomainError::InvalidStatistics {
            field: "source_assistant_text"
        })
    ));
    Ok(())
}

/// Identity drift, broken atomic pairs, and allocator-frontier failure never partially commit.
/// 身份漂移、破损原子事件对与分配器前沿失败绝不会产生部分提交。
#[test]
fn subagent_return_conflict_and_failure_paths_are_strict_and_atomic() -> Result<(), DomainError> {
    let (child, child_turn_id, source_assistant_text_seq) = durable_child_return_fixture()?;
    let input = child.build_subagent_return_input(SubagentReturnBuildRequest {
        final_turn_id: child_turn_id,
        child_task_id: SubagentTaskId::try_new("task-final")?,
        source_assistant_text_seq,
    })?;

    let mut parent = new_document();
    parent.submit_root_input_fixture("delegate", Vec::new(), T0)?;
    parent.append_subagent_return_with_stats(input.clone(), T0 + 1)?;
    let mut drifted = input.clone();
    drifted.content.push_str(" changed");
    let before_drift = parent.clone();
    assert!(matches!(
        parent.append_subagent_return_with_stats(drifted, T0 + 2),
        Err(DomainError::ImmutableFieldMutation {
            entity: "subagent_return",
            ..
        })
    ));
    assert_eq!(parent, before_drift);

    let mut broken_events = parent.events.clone();
    assert!(matches!(
        broken_events.pop().map(|event| event.event),
        Some(EventBody::SubagentReturnStats(_))
    ));
    assert!(matches!(
        SessionDocument::from_parts(
            parent.metadata.clone(),
            parent.runtime_lock.clone(),
            parent.compactions.clone(),
            parent.segment_catalog.clone(),
            broken_events,
        ),
        Err(DomainError::InvalidStatistics {
            field: "subagent_return_stats_pair"
        })
    ));

    let mut wrong_visibility = parent.events.clone();
    let stats_event = wrong_visibility
        .iter_mut()
        .find(|event| matches!(&event.event, EventBody::SubagentReturnStats(_)))
        .expect("subagent stats event");
    stats_event.visibility = EventVisibility::Provider;
    assert!(matches!(
        SessionDocument::from_parts(
            parent.metadata.clone(),
            parent.runtime_lock.clone(),
            parent.compactions.clone(),
            parent.segment_catalog.clone(),
            wrong_visibility,
        ),
        Err(DomainError::InvalidStatistics {
            field: "subagent_return.visibility"
        })
    ));

    let mut exhausted = new_document();
    exhausted.submit_root_input_fixture("delegate", Vec::new(), T0)?;
    let (next_local_id, next_seq) = exhausted.id_allocator.frontier();
    exhausted.id_allocator = IdAllocator::from_frontier(next_local_id, next_seq + 1);
    let before_exhausted = exhausted.clone();
    assert!(matches!(
        exhausted.append_subagent_return_with_stats(input, T0 + 3),
        Err(DomainError::SequenceViolation { .. })
    ));
    assert_eq!(exhausted, before_exhausted);
    Ok(())
}

/// Typed replay state round-trips on the final Provider event, while a replay
/// envelope from another route is rejected before any completion mutation.
/// 强类型 replay 状态在最后一条 Provider 事件上往返保持；来自另一执行路由的封套会在
/// 任何完成态修改前被拒绝。
#[test]
fn typed_provider_replay_round_trips_and_rejects_route_drift() -> Result<(), DomainError> {
    let mut request = ApiRequestSnapshot::new("openai", "gpt-5");
    request.profile = Some("responses.v1".to_string());
    request.connection_id = Some("connection-1".to_string());
    let replay = ProviderReplayEnvelope::new(
        "openai",
        "connection-1",
        "https://example.test/v1/responses",
        "responses.v1",
        "gpt-5",
        Vec::new(),
        vec![serde_json::json!({"type": "reasoning", "id": "r1"})],
    );
    let mut doc = new_document();
    doc.submit_root_input_fixture("replay", Vec::new(), T0)?;
    let api = doc.start_api(ApiPurposeKind::MainGeneration, request.clone(), T0)?;
    let text_seq = doc
        .append_assistant_text(api, "done", AssistantSemanticStatus::Completed, T0 + 1)?
        .into_value();
    doc.complete_api_group(
        api,
        ApiCompletion {
            request: request.clone(),
            usage: usage(1, 2),
            status: ApiCallStatus::Completed,
            end_reason: Some(ApiEndReason::CompletedSuccessfully),
            error: None,
            continuation_eligibility: ContinuationEligibility::Forbidden,
            timing: ApiTimings::default(),
            provider_source: Some(ProviderSourceInput {
                provider: "openai".to_string(),
                profile: Some("responses.v1".to_string()),
                model: "gpt-5".to_string(),
                connection_id: Some("connection-1".to_string()),
                message_id: Some("message-1".to_string()),
                response_id: Some("response-1".to_string()),
                replay: Some(replay.clone()),
            }),
            completed_at_ms: T0 + 2,
        },
    )?;
    let serialized =
        serde_json::to_string(doc.events()).expect("typed replay event stream must serialize");
    let events: Vec<ConversationEvent> =
        serde_json::from_str(&serialized).expect("typed replay event stream must deserialize");
    let restored = SessionDocument::from_parts(
        doc.metadata.clone(),
        initialized_runtime_lock(),
        Vec::new(),
        Vec::new(),
        events,
    )?;
    let placed = restored
        .events()
        .iter()
        .find_map(|event| match &event.event {
            EventBody::AssistantText(text) if event.seq() == text_seq => {
                text.provider_source.as_ref()
            }
            _ => None,
        });
    assert_eq!(
        placed.and_then(|source| source.replay.as_ref()),
        Some(&replay)
    );

    let mut rejected = new_document();
    rejected.submit_root_input_fixture("replay", Vec::new(), T0)?;
    let rejected_api = rejected.start_api(ApiPurposeKind::MainGeneration, request.clone(), T0)?;
    rejected.append_assistant_text(
        rejected_api,
        "done",
        AssistantSemanticStatus::Completed,
        T0 + 1,
    )?;
    let event_count = rejected.events().len();
    let wrong_replay = ProviderReplayEnvelope::new(
        "openai",
        "connection-1",
        "https://example.test/v1/responses",
        "responses.v1",
        "different-model",
        Vec::new(),
        Vec::new(),
    );
    let result = rejected.complete_api_group(
        rejected_api,
        ApiCompletion {
            request,
            usage: usage(1, 2),
            status: ApiCallStatus::Completed,
            end_reason: Some(ApiEndReason::CompletedSuccessfully),
            error: None,
            continuation_eligibility: ContinuationEligibility::Forbidden,
            timing: ApiTimings::default(),
            provider_source: Some(ProviderSourceInput {
                provider: "openai".to_string(),
                profile: Some("responses.v1".to_string()),
                model: "gpt-5".to_string(),
                connection_id: Some("connection-1".to_string()),
                message_id: None,
                response_id: None,
                replay: Some(wrong_replay),
            }),
            completed_at_ms: T0 + 2,
        },
    );
    assert!(matches!(
        result,
        Err(DomainError::ApiRequestMismatch {
            field: "provider_source.replay.model_id",
            ..
        })
    ));
    assert_eq!(rejected.events().len(), event_count);
    assert_eq!(
        rejected.active_api().map(|value| value.api_call_id),
        Some(rejected_api)
    );
    Ok(())
}

/// Rebuild rejects a provider source moved off the final Provider event even
/// when its coverage and fingerprint were recomputed to look locally valid.
/// 即使重新计算覆盖位置与指纹使其局部看似有效，重建仍拒绝被移出最后 Provider 事件的来源。
#[test]
fn rebuild_rejects_provider_source_before_final_provider_event() -> Result<(), DomainError> {
    let mut doc = new_document();
    doc.submit_root_input_fixture("source", Vec::new(), T0)?;
    let api = doc.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0,
    )?;
    let reasoning_seq = doc
        .append_reasoning(api, "reasoning", AssistantSemanticStatus::Completed, T0 + 1)?
        .into_value();
    doc.append_assistant_text(api, "final", AssistantSemanticStatus::Completed, T0 + 2)?;
    complete_ok(
        &mut doc,
        api,
        Some(ProviderSourceInput {
            provider: "openai".to_string(),
            profile: None,
            model: "gpt-5".to_string(),
            connection_id: None,
            message_id: Some("m1".to_string()),
            response_id: None,
            replay: None,
        }),
        T0 + 3,
    )?;
    let mut events = doc.events().to_vec();
    let mut source = events
        .iter_mut()
        .find_map(|event| match &mut event.event {
            EventBody::AssistantText(text) => text.provider_source.take(),
            _ => None,
        })
        .expect("valid group must place a source");
    source.covered_until_seq = reasoning_seq;
    source.fingerprint_sha256 =
        crate::v1::projection::provider_visible_fingerprint(&events, reasoning_seq)?;
    let reasoning = events
        .iter_mut()
        .find_map(|event| {
            if event.seq() != reasoning_seq {
                return None;
            }
            match &mut event.event {
                EventBody::AssistantReasoning(reasoning) => Some(reasoning),
                _ => None,
            }
        })
        .expect("reasoning event must exist");
    reasoning.provider_source = Some(source);
    let result = SessionDocument::from_parts(
        doc.metadata.clone(),
        initialized_runtime_lock(),
        Vec::new(),
        Vec::new(),
        events,
    );
    assert!(matches!(
        result,
        Err(DomainError::ProviderSourcePlacement {
            api_call_id,
            source_seq,
            api_closed: true,
            ..
        }) if api_call_id == api.value() && source_seq == reasoning_seq.value()
    ));
    Ok(())
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

/// A failed compaction API cannot publish a context node even when it returned
/// text; node statistics must come from a successfully completed compaction API.
/// 失败的压缩 API 即使返回了文本也不能发布上下文节点；节点统计必须来自成功完成的压缩 API。
#[test]
fn failed_compaction_api_cannot_publish_node() -> Result<(), DomainError> {
    let mut doc = new_document();
    doc.submit_root_input_fixture("compact", Vec::new(), T0)?;
    let api = doc.start_api(
        ApiPurposeKind::Compaction,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0,
    )?;
    let occurred = doc
        .append_assistant_text(api, "summary", AssistantSemanticStatus::Completed, T0 + 1)?
        .into_value();
    doc.complete_api_group(
        api,
        ApiCompletion {
            request: ApiRequestSnapshot::new("openai", "gpt-5"),
            usage: usage(1, 1),
            status: ApiCallStatus::Failed,
            end_reason: Some(ApiEndReason::ProviderError),
            error: Some(terminal_error(ApiEndReason::ProviderError)),
            continuation_eligibility: ContinuationEligibility::Forbidden,
            timing: ApiTimings::default(),
            provider_source: None,
            completed_at_ms: T0 + 2,
        },
    )?;
    assert!(matches!(
        doc.record_compaction(
            CompactionCommitInput::new(
                api,
                occurred,
                Seq::try_new(1)?,
                "summary",
                LockedBaselineProjection::empty(),
            ),
            T0 + 3,
        ),
        Err(DomainError::ApiRequestMismatch {
            field: "compaction.status",
            ..
        })
    ));
    assert!(doc.compactions.is_empty());
    Ok(())
}

/// Fresh runtime state is explicit, idempotently initialized, and rejects APIs beforehand.
/// Fresh runtime 状态是显式的、可幂等初始化，并在初始化前拒绝 API。
#[test]
fn runtime_baseline_initialization_is_explicit_and_failure_atomic() -> Result<(), DomainError> {
    let mut doc = SessionDocument::new("fresh", "project", "title", T0);
    assert_eq!(
        doc.runtime_lock.initialization_state,
        RuntimeInitializationState::FreshUninitialized
    );
    doc.submit_root_input_fixture("hello", Vec::new(), T0)?;
    let before_invalid = doc.clone();
    let invalid = PrepareAndStartApiInput {
        purpose: ApiPurposeKind::MainGeneration,
        request: ApiRequestSnapshot::new("openai", "gpt-5"),
        request_budget_evidence: test_provider_request_budget_evidence(),
        projection: ProviderProjectionProof {
            segment_generation: doc.segment_generation(),
            document_revision: doc.memory_revision(),
            retained_from_seq: Seq::try_new(1)?,
            through_seq: doc.events().last().expect("root input").seq,
            fingerprint_sha256: "invalid".to_string(),
        },
        locked_baseline: LockedBaselineProjection::empty(),
        continuation_action_id: None,
        now_ms: T0 + 1,
    };
    assert!(matches!(
        doc.prepare_and_start_api(invalid),
        Err(DomainError::InvalidProviderProjection {
            field: "fingerprint_sha256"
        })
    ));
    assert_eq!(doc, before_invalid);

    let prepared = doc.prepare_and_start_api(PrepareAndStartApiInput {
        purpose: ApiPurposeKind::MainGeneration,
        request: ApiRequestSnapshot::new("openai", "gpt-5"),
        request_budget_evidence: test_provider_request_budget_evidence(),
        projection: ProviderProjectionProof {
            segment_generation: doc.segment_generation(),
            document_revision: doc.memory_revision(),
            retained_from_seq: Seq::try_new(1)?,
            through_seq: doc.events().last().expect("root input").seq,
            fingerprint_sha256: "0".repeat(64),
        },
        locked_baseline: LockedBaselineProjection::empty(),
        continuation_action_id: None,
        now_ms: T0 + 2,
    })?;
    let prepared = prepared.into_value();
    assert_eq!(
        doc.runtime_lock.initialization_state,
        RuntimeInitializationState::Initialized
    );
    assert_eq!(
        doc.active_api().expect("active api").api_call_id,
        prepared.api_call_id
    );
    assert_eq!(
        doc.metadata.config.active_request,
        ActiveRequestConfig {
            provider: Some(String::from("openai")),
            connection_id: None,
            model: Some(String::from("gpt-5")),
            thinking_level: None,
            temperature: None,
        }
    );
    assert!(doc.dirty_files().any(|file| file == DirtyFile::Metadata));
    assert!(doc.dirty_files().any(|file| file == DirtyFile::Runtime));
    assert!(doc.dirty_files().any(|file| file == DirtyFile::ActiveTail));

    let before_conflict = doc.clone();
    assert!(matches!(
        doc.initialize_runtime_baseline(
            LockedBaselineProjection::project(vec![LockedBaselineSegmentInput {
                kind: LockedSourceKind::AgentsMd,
                node_id: "agents-project".to_string(),
                target: LockedPromptTarget::Developer,
                source_label: Some("AGENTS.md".to_string()),
                source_id: None,
                content: "rules".to_string(),
            }])?,
            T0 + 3,
        ),
        Err(DomainError::RuntimeBaselineAlreadyInitialized)
    ));
    assert_eq!(doc, before_conflict);
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

/// Tool batch receipts preserve local execution identity across retry and reload.
/// 工具批次 receipt 在重试与重载后保持本地执行身份。
#[test]
fn prepared_tool_batch_receipt_is_exact_and_restart_idempotent() -> Result<(), DomainError> {
    let mut doc = new_document();
    doc.submit_root_input_fixture("tools", Vec::new(), T0)?;
    let api = doc.start_api(ApiPurposeKind::MainGeneration, replay_request(), T0 + 1)?;
    let preparations = vec![
        prepared_tool("ask", "provider-tool-1"),
        prepared_tool("shell", "provider-tool-2"),
    ];
    let source = replay_source("message-1");
    let first = doc.prepare_tool_batch_with_provider_source(
        api,
        preparations.clone(),
        source.clone(),
        T0 + 2,
    )?;
    assert!(first.changed());
    let receipt = first.into_value();
    assert_eq!(receipt.api_call_id, api);
    assert_eq!(receipt.tools.len(), preparations.len());
    for (mapped, preparation) in receipt.tools.iter().zip(&preparations) {
        assert_eq!(mapped.api_call_id, api);
        assert_eq!(mapped.request_stream_id, preparation.request_stream_id);
        assert!(matches!(
            doc.events().iter().find(|event| event.seq == mapped.tool_seq),
            Some(ConversationEvent {
                event: EventBody::ToolCall(tool),
                ..
            }) if tool.tool_call_id == mapped.tool_call_id
                && tool.request_stream_id == mapped.request_stream_id
        ));
    }

    let revision_before_retry = doc.memory_revision();
    let repeated = doc.prepare_tool_batch_with_provider_source(
        api,
        preparations.clone(),
        source.clone(),
        T0 + 3,
    )?;
    assert!(!repeated.changed());
    assert_eq!(repeated.into_value(), receipt);
    assert_eq!(doc.memory_revision(), revision_before_retry);

    let mut restored = SessionDocument::from_parts(
        doc.metadata.clone(),
        doc.runtime_lock.clone(),
        doc.compactions.clone(),
        doc.segment_catalog().to_vec(),
        doc.events().to_vec(),
    )?;
    let restored_revision = restored.memory_revision();
    let repeated_after_reload = restored.prepare_tool_batch_with_provider_source(
        api,
        preparations.clone(),
        source,
        T0 + 4,
    )?;
    assert!(!repeated_after_reload.changed());
    assert_eq!(repeated_after_reload.into_value(), receipt);
    assert_eq!(restored.memory_revision(), restored_revision);

    let mut drifted = preparations;
    drifted[0].input = ToolPayload::try_new(serde_json::json!({"changed": true}))?;
    let before_drift = restored.clone();
    assert!(matches!(
        restored.prepare_tool_batch(api, drifted, T0 + 5),
        Err(DomainError::InvalidToolIdentity {
            field: "prepared_batch_semantic_drift"
        })
    ));
    assert_eq!(restored, before_drift);
    Ok(())
}

/// Goal completion is atomic inside a tool API and suppresses the next continuation turn.
/// Goal 完成可在工具 API 内原子发生，并阻止创建下一续跑回合。
#[test]
fn complete_goal_inside_active_api_preserves_the_turn_and_tool_lifecycle() -> Result<(), DomainError>
{
    let mut doc = new_document();
    let turn_id = doc.submit_root_input_fixture("finish", Vec::new(), T0)?;
    doc.create_goal(
        "finish",
        None,
        "Continue until the goal is complete.",
        T0 + 1,
    )?;
    let api = doc.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 2,
    )?;
    let tool = doc.prepare_tool(
        api,
        RequestStreamId::try_new("goal-complete-stream")?,
        "update_goal",
        ToolPayload::try_new(serde_json::json!({"status": "complete"}))?,
        Some(String::from("provider-goal-complete")),
        T0 + 3,
    )?;
    doc.start_prepared_tool(api, tool, T0 + 4)?;

    let transition = doc.complete_goal(T0 + 5)?.expect("active Goal changes");
    assert_eq!(doc.metadata.mode.goal.state, GoalState::Completed);
    assert_eq!(
        doc.active_api()
            .expect("tool API remains active")
            .api_call_id,
        api
    );
    assert_eq!(
        doc.active_turn().expect("turn remains open").turn_id,
        turn_id
    );
    assert!(!doc.active_turn().expect("turn remains open").closed);
    assert!(matches!(
        doc.events().iter().find(|event| event.seq() == transition.context_seq.unwrap()),
        Some(ConversationEvent {
            event: EventBody::SystemContext(SystemContextEvent {
                context_key,
                operation: SystemContextOperation::Remove,
                ..
            }),
            ..
        }) if context_key == "goal_state"
    ));

    doc.finish_tool(
        api,
        tool,
        tool_completion(
            Some(ToolPayload::try_new(
                serde_json::json!({"status": "complete"}),
            )?),
            ToolCallStatus::Completed,
            T0 + 6,
        ),
    )?;
    complete_ok(&mut doc, api, None, T0 + 7)?;
    let final_api = doc.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 8,
    )?;
    doc.append_assistant_text(
        final_api,
        "Goal completed.",
        AssistantSemanticStatus::Completed,
        T0 + 9,
    )?;
    complete_ok(&mut doc, final_api, None, T0 + 10)?;
    let closure = doc.complete_turn_and_select_successor(turn_id, T0 + 11)?;
    assert_eq!(closure.successor, TurnSuccessor::None);
    assert!(
        doc.active_turn()
            .expect("completed turn remains addressable")
            .closed
    );
    Ok(())
}

/// Completing a Goal keeps an already-dispatched continuation Turn open, but closes an
/// undispatched future continuation and remains idempotent.
/// 完成 Goal 时保留已派发的续跑回合，但关闭尚未派发的未来续跑，并保持幂等。
#[test]
fn complete_goal_distinguishes_running_and_undispatched_continuations() -> Result<(), DomainError> {
    let mut running = new_document();
    let created =
        running.create_goal("finish", None, "Continue until the goal is complete.", T0)?;
    let continuation = created.continuation_turn_id.expect("continuation turn");
    let api = running.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 1,
    )?;
    running.complete_goal(T0 + 2)?.expect("state changes");
    assert_eq!(
        running
            .active_api()
            .expect("API remains active")
            .api_call_id,
        api
    );
    assert_eq!(
        running.active_turn().expect("turn remains open").turn_id,
        continuation
    );
    assert!(!running.active_turn().expect("turn remains open").closed);
    let before_noop = running.clone();
    assert_eq!(running.complete_goal(T0 + 3)?, None);
    assert_eq!(running, before_noop);

    let mut undispatched = new_document();
    undispatched.create_goal("finish", None, "Continue until the goal is complete.", T0)?;
    undispatched.complete_goal(T0 + 1)?.expect("state changes");
    assert!(
        undispatched
            .active_turn()
            .expect("closed turn remains addressable")
            .closed
    );
    assert_eq!(undispatched.metadata.open_turn_start_seq, None);

    let mut exhausted = new_document();
    exhausted.submit_root_input_fixture("finish", Vec::new(), T0)?;
    exhausted.create_goal(
        "finish",
        None,
        "Continue until the goal is complete.",
        T0 + 1,
    )?;
    let (next_local_id, _) = exhausted.id_allocator.frontier();
    exhausted.id_allocator = IdAllocator::from_frontier(next_local_id, JS_SAFE_INTEGER_MAX + 1);
    let before_failure = exhausted.clone();
    assert!(exhausted.complete_goal(T0 + 2).is_err());
    assert_eq!(exhausted, before_failure);
    Ok(())
}

/// All input entry points reject empty facts without creating false revisions.
/// 所有输入入口都会拒绝空事实且不会制造虚假修订。
#[test]
fn user_input_validation_does_not_create_false_revisions() -> Result<(), DomainError> {
    let mut doc = new_document();
    let fresh = doc.clone();
    assert!(matches!(
        doc.submit_root_input_fixture("   ", Vec::new(), T0),
        Err(DomainError::EmptyUserInput)
    ));
    assert_eq!(doc, fresh);

    let resource = ResourceId::try_new(1)?;
    doc.submit_root_input_fixture("", vec![resource], T0)?;

    let before_queue = doc.clone();
    assert!(matches!(
        doc.enqueue_input_fixture("\t", Vec::new(), T0 + 2),
        Err(DomainError::EmptyUserInput)
    ));
    assert_eq!(doc, before_queue);
    let before_pending = doc.clone();
    assert!(matches!(
        doc.add_pending_in_turn_input_fixture("\n", Vec::new(), T0 + 3),
        Err(DomainError::EmptyUserInput)
    ));
    assert_eq!(doc, before_pending);
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

/// Lifecycle metadata mutations are idempotent, monotonic, and metadata-only.
/// 生命周期 metadata 修改保持幂等、时间单调且只标记 metadata。
#[test]
fn lifecycle_metadata_mutations_are_idempotent_and_monotonic() -> Result<(), DomainError> {
    let mut doc = new_document();
    assert!(doc.rename_session("renamed", T0 + 10)?);
    assert_eq!(doc.metadata.title, "renamed");
    assert_eq!(doc.metadata.updated_at_ms, T0 + 10);
    assert_eq!(
        doc.dirty_files().collect::<BTreeSet<_>>(),
        BTreeSet::from([DirtyFile::Metadata])
    );
    let revision = doc.memory_revision();
    assert!(!doc.rename_session("renamed", T0 + 11)?);
    assert_eq!(doc.memory_revision(), revision);

    assert!(doc.set_archived_at(Some(T0 + 5), T0 + 5)?);
    assert_eq!(doc.metadata.archived_at_ms, Some(T0 + 5));
    assert_eq!(doc.metadata.updated_at_ms, T0 + 10);
    let before_invalid = doc.clone();
    assert!(doc.rename_session("   ", T0 + 12).is_err());
    assert_eq!(doc, before_invalid);
    assert!(doc.set_archived_at(Some(T0 - 1), T0 + 12).is_err());
    assert_eq!(doc, before_invalid);
    Ok(())
}

/// Output-style normalization is explicit, while request selections stay strict and idempotent.
/// 输出风格会显式规范化，而请求选择保持严格且幂等。
#[test]
fn selection_and_pending_request_setters_are_atomic_and_idempotent() -> Result<(), DomainError> {
    let mut doc = new_document();
    assert!(doc.set_output_style(Some("  concise  ".to_string()), T0 + 1)?);
    assert_eq!(
        doc.metadata.config.selection.output_style.as_deref(),
        Some("concise")
    );
    let style_revision = doc.memory_revision();
    assert!(!doc.set_output_style(Some("concise".to_string()), T0 + 2)?);
    assert_eq!(doc.memory_revision(), style_revision);
    assert!(doc.set_output_style(Some("   ".to_string()), T0 + 3)?);
    assert_eq!(doc.metadata.config.selection.output_style, None);
    let before_bad_style_time = doc.clone();
    assert!(
        doc.set_output_style(Some("concise".to_string()), -1)
            .is_err()
    );
    assert_eq!(doc, before_bad_style_time);

    let pending = PendingRequestConfig {
        provider: Some("openai".to_string()),
        connection_id: Some("connection".to_string()),
        model: Some("gpt-5".to_string()),
        thinking_level: Some("high".to_string()),
        temperature: Some(0.2),
        set_at_ms: T0 + 4,
    };
    assert!(doc.set_pending_request(pending, T0 + 4)?);
    let stored = doc
        .metadata
        .config
        .pending_request
        .as_ref()
        .expect("pending request must be stored");
    assert_eq!(stored.provider.as_deref(), Some("openai"));
    assert_eq!(stored.connection_id.as_deref(), Some("connection"));
    assert_eq!(stored.model.as_deref(), Some("gpt-5"));
    assert_eq!(stored.thinking_level.as_deref(), Some("high"));
    let pending_revision = doc.memory_revision();
    assert!(!doc.set_pending_request(
        PendingRequestConfig {
            provider: Some("openai".to_string()),
            connection_id: Some("connection".to_string()),
            model: Some("gpt-5".to_string()),
            thinking_level: Some("high".to_string()),
            temperature: Some(0.2),
            set_at_ms: T0 + 5,
        },
        T0 + 5,
    )?);
    assert_eq!(doc.memory_revision(), pending_revision);
    let before_invalid_pending = doc.clone();
    assert!(matches!(
        doc.set_pending_request(
            PendingRequestConfig {
                provider: Some(" openai".to_string()),
                connection_id: None,
                model: Some("gpt-5".to_string()),
                thinking_level: None,
                temperature: None,
                set_at_ms: T0 + 6,
            },
            T0 + 6,
        ),
        Err(DomainError::InvalidRequestConfiguration {
            field: "pending.provider"
        })
    ));
    assert_eq!(doc, before_invalid_pending);
    assert!(
        doc.set_pending_request(
            PendingRequestConfig {
                provider: Some("openai".to_string()),
                connection_id: None,
                model: Some("gpt-5".to_string()),
                thinking_level: None,
                temperature: Some(f64::NAN),
                set_at_ms: T0 + 7,
            },
            T0 + 7,
        )
        .is_err()
    );
    assert_eq!(doc, before_invalid_pending);
    Ok(())
}

/// Provider-tool selection is complete, canonical, idempotent, and reload-validated.
/// 供应商工具选择保持完整、规范、幂等，并在重载时校验。
#[test]
fn provider_tool_selection_is_atomic_idempotent_and_strict() -> Result<(), DomainError> {
    let mut doc = new_document();
    let selection = ProviderToolSelectionConfig {
        initialized: true,
        overridden: true,
        enabled_capabilities: vec!["image_generation".to_string(), "web_search".to_string()],
        selections: vec![ProviderToolCapabilitySelection {
            capability: "web_search".to_string(),
            target: ProviderToolTargetConfig {
                connection_id: "connection-a".to_string(),
                target_id: "search-a".to_string(),
            },
        }],
        media_understanding_modes: vec![ProviderMediaUnderstandingSelection {
            capability: "image_understanding".to_string(),
            mode: ProviderMediaUnderstandingMode::ProviderTool,
        }],
        extension_target_sets: vec![ProviderExtensionTargetSetConfig {
            capability: "image_generation".to_string(),
            targets: vec![ProviderToolTargetConfig {
                connection_id: "connection-b".to_string(),
                target_id: "image-a".to_string(),
            }],
        }],
    };

    assert!(doc.set_provider_tool_selection(selection.clone(), T0 + 1)?);
    assert_eq!(doc.metadata.config.selection.provider_tools, selection);
    let revision = doc.memory_revision();
    assert!(!doc.set_provider_tool_selection(selection.clone(), T0 + 2)?);
    assert_eq!(doc.memory_revision(), revision);

    let before_invalid = doc.clone();
    let mut invalid = selection.clone();
    invalid.enabled_capabilities.reverse();
    assert!(matches!(
        doc.set_provider_tool_selection(invalid, T0 + 3),
        Err(DomainError::InvalidSelectionConfiguration {
            field: "provider_tools.enabled_capabilities.order"
        })
    ));
    assert_eq!(doc, before_invalid);

    let mut invalid_metadata = doc.metadata.clone();
    invalid_metadata.config.selection.provider_tools.initialized = false;
    assert!(matches!(
        SessionDocument::from_parts(
            invalid_metadata,
            doc.runtime_lock.clone(),
            doc.compactions.clone(),
            doc.segment_catalog.clone(),
            doc.events().to_vec(),
        ),
        Err(DomainError::InvalidSelectionConfiguration {
            field: "provider_tools.initialized"
        })
    ));
    Ok(())
}

/// Knowledge and VMM session selections are strict, idempotent metadata facts.
/// 知识库与 VMM 会话选择是严格且幂等的 metadata 事实。
#[test]
fn knowledge_and_vmm_selections_are_atomic_idempotent_and_strict() -> Result<(), DomainError> {
    let mut doc = new_document();
    let knowledge = KnowledgeLibrarySelectionConfig {
        mode: KnowledgeLibrarySelectionMode::Explicit,
        library_ids: vec!["library-a".to_string(), "library-b".to_string()],
        retrieval_mode: Some(crate::v1::config::KnowledgeRetrievalMode::DisplayHybrid),
    };
    assert!(doc.set_knowledge_library_selection(knowledge.clone(), T0 + 1)?);
    assert_eq!(doc.metadata.config.selection.knowledge_libraries, knowledge);
    let knowledge_revision = doc.memory_revision();
    assert!(!doc.set_knowledge_library_selection(knowledge.clone(), T0 + 2)?);
    assert_eq!(doc.memory_revision(), knowledge_revision);

    let before_invalid = doc.clone();
    let mut invalid_order = knowledge.clone();
    invalid_order.library_ids.reverse();
    assert!(matches!(
        doc.set_knowledge_library_selection(invalid_order, T0 + 3),
        Err(DomainError::InvalidSelectionConfiguration {
            field: "knowledge_libraries.library_ids.order"
        })
    ));
    assert_eq!(doc, before_invalid);

    let mut invalid_metadata = doc.metadata.clone();
    invalid_metadata.config.selection.knowledge_libraries = KnowledgeLibrarySelectionConfig {
        mode: KnowledgeLibrarySelectionMode::Inherit,
        library_ids: vec!["library-a".to_string()],
        retrieval_mode: None,
    };
    assert!(matches!(
        SessionDocument::from_parts(
            invalid_metadata,
            doc.runtime_lock.clone(),
            doc.compactions.clone(),
            doc.segment_catalog.clone(),
            doc.events().to_vec(),
        ),
        Err(DomainError::InvalidSelectionConfiguration {
            field: "knowledge_libraries.inherit_ids"
        })
    ));

    let vmm = VmmSessionPolicyConfig {
        precheck_enabled: Some(false),
        memory_writeback_enabled: Some(true),
    };
    assert!(doc.set_vmm_session_policy(vmm.clone(), T0 + 4)?);
    assert_eq!(doc.metadata.config.selection.vmm, vmm);
    let vmm_revision = doc.memory_revision();
    assert!(!doc.set_vmm_session_policy(vmm, T0 + 5)?);
    assert_eq!(doc.memory_revision(), vmm_revision);
    Ok(())
}

/// The complete session-selection bundle commits once and rejects any invalid member atomically.
/// 完整会话选择配置只提交一次，并以原子方式拒绝任意无效成员。
#[test]
fn session_selection_bundle_is_single_revision_idempotent_and_failure_atomic()
-> Result<(), DomainError> {
    let mut doc = new_document();
    let input = SessionSelectionBundleInput {
        output_style: Some(" concise ".to_string()),
        provider_tools: ProviderToolSelectionConfig {
            initialized: true,
            overridden: true,
            enabled_capabilities: vec!["web_search".to_string()],
            selections: vec![ProviderToolCapabilitySelection {
                capability: "web_search".to_string(),
                target: ProviderToolTargetConfig {
                    connection_id: "connection-a".to_string(),
                    target_id: "search-a".to_string(),
                },
            }],
            media_understanding_modes: Vec::new(),
            extension_target_sets: Vec::new(),
        },
        knowledge_libraries: KnowledgeLibrarySelectionConfig {
            mode: KnowledgeLibrarySelectionMode::Explicit,
            library_ids: vec!["library-a".to_string()],
            retrieval_mode: Some(crate::v1::config::KnowledgeRetrievalMode::DisplayBm25),
        },
        vmm: VmmSessionPolicyConfig {
            precheck_enabled: Some(true),
            memory_writeback_enabled: Some(false),
        },
    };
    let before_revision = doc.memory_revision();
    assert!(doc.set_session_selection_bundle(input.clone(), T0 + 1)?);
    assert_eq!(doc.memory_revision(), before_revision + 1);
    assert_eq!(
        doc.metadata.config.selection.output_style.as_deref(),
        Some("concise")
    );
    let committed_revision = doc.memory_revision();
    assert!(!doc.set_session_selection_bundle(input.clone(), T0 + 2)?);
    assert_eq!(doc.memory_revision(), committed_revision);

    let mut invalid = input;
    invalid.knowledge_libraries.library_ids =
        vec!["library-b".to_string(), "library-a".to_string()];
    let before_invalid = doc.clone();
    assert!(matches!(
        doc.set_session_selection_bundle(invalid, T0 + 3),
        Err(DomainError::InvalidSelectionConfiguration {
            field: "knowledge_libraries.library_ids.order"
        })
    ));
    assert_eq!(doc, before_invalid);
    Ok(())
}

/// API request selection validation is identical at mutation and reload boundaries.
/// API 请求选择在修改边界与重载边界使用完全相同的校验契约。
#[test]
fn api_request_selection_validation_is_symmetric_and_atomic() -> Result<(), DomainError> {
    let mut doc = new_document();
    doc.submit_root_input_fixture("hello", Vec::new(), T0)?;
    let projection = ProviderProjectionProof {
        segment_generation: doc.segment_generation(),
        document_revision: doc.memory_revision(),
        retained_from_seq: Seq::try_new(1)?,
        through_seq: doc.events().last().expect("root input").seq,
        fingerprint_sha256: "0".repeat(64),
    };
    let invalid_requests = [
        (ApiRequestSnapshot::new("", "gpt-5"), "provider"),
        (ApiRequestSnapshot::new(" openai", "gpt-5"), "provider"),
        (ApiRequestSnapshot::new("openai", ""), "model"),
        (
            ApiRequestSnapshot {
                profile: Some(" ".to_string()),
                ..ApiRequestSnapshot::new("openai", "gpt-5")
            },
            "profile",
        ),
        (
            ApiRequestSnapshot {
                connection_id: Some("connection ".to_string()),
                ..ApiRequestSnapshot::new("openai", "gpt-5")
            },
            "connection_id",
        ),
        (
            ApiRequestSnapshot {
                thinking_level: Some(String::new()),
                ..ApiRequestSnapshot::new("openai", "gpt-5")
            },
            "thinking_level",
        ),
        (
            ApiRequestSnapshot {
                temperature: Some(f64::INFINITY),
                ..ApiRequestSnapshot::new("openai", "gpt-5")
            },
            "temperature",
        ),
    ];
    for (request, field) in invalid_requests {
        let before = doc.clone();
        assert!(matches!(
            doc.prepare_and_start_api(PrepareAndStartApiInput {
                purpose: ApiPurposeKind::MainGeneration,
                request,
                request_budget_evidence: test_provider_request_budget_evidence(),
                projection: projection.clone(),
                locked_baseline: LockedBaselineProjection::empty(),
                continuation_action_id: None,
                now_ms: T0 + 1,
            }),
            Err(DomainError::InvalidRequestConfiguration { field: actual })
                if actual == field
        ));
        assert_eq!(doc, before);
    }

    let api = doc.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 2,
    )?;
    let mut invalid_events = doc.events().to_vec();
    let started = invalid_events
        .iter_mut()
        .find_map(|event| match &mut event.event {
            EventBody::ApiStarted(started) if started.api_call_id == api => Some(started),
            _ => None,
        })
        .expect("api_started event");
    started.request.profile = Some(" responses".to_string());
    assert!(matches!(
        SessionDocument::from_parts(
            doc.metadata.clone(),
            doc.runtime_lock.clone(),
            doc.compactions.clone(),
            doc.segment_catalog().to_vec(),
            invalid_events,
        ),
        Err(DomainError::InvalidRequestConfiguration { field: "profile" })
    ));

    let mut invalid_metadata = new_document().metadata.clone();
    invalid_metadata.config.active_request.provider = Some("openai ".to_string());
    assert!(matches!(
        SessionDocument::from_parts(
            invalid_metadata,
            RuntimeLock::default(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        ),
        Err(DomainError::InvalidRequestConfiguration {
            field: "active.provider"
        })
    ));
    Ok(())
}

/// Logical live patches report only the exact seq values changed by each revision.
/// 逻辑实时补丁只报告每个修订真实变化的 seq。
#[test]
fn revision_receipts_separate_logical_patches_from_full_tail_flushes() -> Result<(), DomainError> {
    let mut doc = new_document();
    doc.submit_root_input_fixture("hello", Vec::new(), T0 + 1)?;
    let root_revision = doc.memory_revision();
    let root_receipt = doc.memory_revision_receipt(root_revision)?;
    assert_eq!(root_receipt.changed_event_seqs, vec![Seq::try_new(1)?]);
    assert_eq!(root_receipt.changed_events.len(), 1);
    let root_candidate = doc
        .prepare_flush_candidates()?
        .expect("root event must produce a flush candidate");
    assert_eq!(root_candidate.changed_event_seqs, vec![Seq::try_new(1)?]);
    assert_eq!(root_candidate.tail_events.len(), 1);
    doc.advance_durable(root_candidate.revision)?;

    let api = doc.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 2,
    )?;
    let started_candidate = doc
        .prepare_flush_candidates()?
        .expect("api start must produce a flush candidate");
    doc.advance_durable(started_candidate.revision)?;
    let reasoning_seq = doc
        .append_reasoning(api, "partial", AssistantSemanticStatus::Streaming, T0 + 3)?
        .into_value();
    let first_candidate = doc
        .prepare_flush_candidates()?
        .expect("reasoning append must produce a flush candidate");
    doc.advance_durable(first_candidate.revision)?;
    doc.append_reasoning(
        api,
        "partial complete",
        AssistantSemanticStatus::Completed,
        T0 + 4,
    )?;
    let update_receipt = doc.memory_revision_receipt(doc.memory_revision())?;
    assert_eq!(update_receipt.changed_event_seqs, vec![reasoning_seq]);
    assert_eq!(update_receipt.changed_events[0].event_revision, 2);
    let update_candidate = doc
        .prepare_flush_candidates()?
        .expect("reasoning update must produce a flush candidate");
    assert_eq!(update_candidate.changed_event_seqs, vec![reasoning_seq]);
    assert!(update_candidate.tail_events.len() > update_candidate.changed_event_seqs.len());
    Ok(())
}

/// Allocation and seq failures leave every public input/API transition byte-for-byte unchanged.
/// 分配与 seq 失败会让每个公开输入/API 转换保持完整文档逐字段不变。
#[test]
fn input_and_api_allocation_failures_are_document_atomic() -> Result<(), DomainError> {
    let mut root = new_document();
    root.id_allocator = IdAllocator::from_frontier(JS_SAFE_INTEGER_MAX, 1);
    let before_root = root.clone();
    assert!(root.submit_root_input_fixture("x", Vec::new(), T0).is_err());
    assert_eq!(root, before_root);

    let mut enqueue = new_document();
    let (_, enqueue_seq) = enqueue.id_allocator.frontier();
    enqueue.id_allocator = IdAllocator::from_frontier(JS_SAFE_INTEGER_MAX + 1, enqueue_seq);
    let before_enqueue = enqueue.clone();
    assert!(
        enqueue
            .enqueue_input_fixture("queued", Vec::new(), T0)
            .is_err()
    );
    assert_eq!(enqueue, before_enqueue);

    let mut pending = new_document();
    pending.submit_root_input_fixture("root", Vec::new(), T0)?;
    let (_, pending_seq) = pending.id_allocator.frontier();
    pending.id_allocator = IdAllocator::from_frontier(JS_SAFE_INTEGER_MAX + 1, pending_seq);
    let before_pending = pending.clone();
    assert!(
        pending
            .add_pending_in_turn_input_fixture("more", Vec::new(), T0 + 1)
            .is_err()
    );
    assert_eq!(pending, before_pending);

    let mut api = new_document();
    api.submit_root_input_fixture("root", Vec::new(), T0)?;
    let (_, api_seq) = api.id_allocator.frontier();
    api.id_allocator = IdAllocator::from_frontier(JS_SAFE_INTEGER_MAX + 1, api_seq);
    let before_api = api.clone();
    assert!(
        api.start_api(
            ApiPurposeKind::MainGeneration,
            ApiRequestSnapshot::new("openai", "gpt-5"),
            T0 + 1,
        )
        .is_err()
    );
    assert_eq!(api, before_api);

    let mut continuation = new_document();
    let (_, continuation_seq) = continuation.id_allocator.frontier();
    continuation.id_allocator =
        IdAllocator::from_frontier(JS_SAFE_INTEGER_MAX + 1, continuation_seq);
    let before_continuation = continuation.clone();
    assert!(
        continuation
            .start_goal_continuation_turn("continue", T0)
            .is_err()
    );
    assert_eq!(continuation, before_continuation);
    Ok(())
}

/// The common append primitive validates seq consistency before publishing allocator state.
/// 公共追加母层在发布分配器状态前校验 seq 一致性。
#[test]
fn append_event_sequence_failure_does_not_advance_allocator() -> Result<(), DomainError> {
    let mut doc = new_document();
    let (next_local_id, next_seq) = doc.id_allocator.frontier();
    doc.id_allocator = IdAllocator::from_frontier(next_local_id, next_seq + 1);
    let before = doc.clone();
    assert!(matches!(
        doc.append_event(
            None,
            EventVisibility::Provider,
            EventBody::SystemContext(SystemContextEvent {
                context_key: "broken".to_string(),
                operation: SystemContextOperation::Upsert,
                scope: SystemContextScope::Session,
                ttl_turns: None,
                provider_role_hint: None,
                content: "broken".to_string(),
            }),
            T0,
        ),
        Err(DomainError::SequenceViolation { .. })
    ));
    assert_eq!(doc, before);
    Ok(())
}

/// System-context TTL shape is strict, failure-atomic, and identical on reload.
/// 系统上下文 TTL 结构保持严格、失败原子，并在重载时使用相同契约。
#[test]
fn system_context_ttl_shape_is_symmetric_and_atomic() -> Result<(), DomainError> {
    let mut document = new_document();
    let invalid_cases = [
        (
            " turn",
            SystemContextOperation::Upsert,
            SystemContextScope::Turn,
            Some(1),
            "context_key",
        ),
        (
            "turn",
            SystemContextOperation::Upsert,
            SystemContextScope::Turn,
            None,
            "turn_scope_ttl_turns",
        ),
        (
            "session",
            SystemContextOperation::Upsert,
            SystemContextScope::Session,
            Some(0),
            "ttl_turns",
        ),
        (
            "session",
            SystemContextOperation::Remove,
            SystemContextScope::Session,
            Some(1),
            "remove_ttl_turns",
        ),
    ];
    for (context_key, operation, scope, ttl_turns, field) in invalid_cases {
        let before = document.clone();
        let result = document.append_system_context(
            None,
            context_key,
            operation,
            scope,
            ttl_turns,
            None,
            "semantic body",
            T0,
        );
        assert!(
            matches!(
                &result,
                Err(DomainError::ImmutableFieldMutation {
                    entity: "system_context",
                    field: actual,
                }) if *actual == field
            ),
            "unexpected validation result for {field}: {result:?}"
        );
        assert_eq!(document, before);
    }

    document.append_system_context(
        None,
        "session",
        SystemContextOperation::Upsert,
        SystemContextScope::Session,
        Some(2),
        None,
        "semantic body",
        T0,
    )?;
    let mut invalid_events = document.events().to_vec();
    let context = invalid_events
        .iter_mut()
        .find_map(|event| match &mut event.event {
            EventBody::SystemContext(context) => Some(context),
            _ => None,
        })
        .expect("system context event");
    context.ttl_turns = Some(0);
    assert!(matches!(
        SessionDocument::from_parts(
            document.metadata.clone(),
            document.runtime_lock.clone(),
            document.compactions.clone(),
            document.segment_catalog().to_vec(),
            invalid_events,
        ),
        Err(DomainError::ImmutableFieldMutation {
            entity: "system_context",
            field: "ttl_turns",
        })
    ));
    Ok(())
}

/// Compaction allocation and truncation rebuild failures never publish partial mutations.
/// 压缩分配与截断重建失败绝不发布部分修改。
#[test]
fn compaction_and_truncation_failures_are_document_atomic() -> Result<(), DomainError> {
    let mut compaction = new_document();
    compaction.submit_root_input_fixture("compact", Vec::new(), T0)?;
    let api = compaction.start_api(
        ApiPurposeKind::Compaction,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0,
    )?;
    let occurred = compaction
        .append_assistant_text(api, "summary", AssistantSemanticStatus::Completed, T0 + 1)?
        .into_value();
    compaction.complete_api_group(
        api,
        ApiCompletion {
            request: ApiRequestSnapshot::new("openai", "gpt-5"),
            usage: usage(1, 1),
            status: ApiCallStatus::Completed,
            end_reason: Some(ApiEndReason::CompletedSuccessfully),
            error: None,
            continuation_eligibility: ContinuationEligibility::Forbidden,
            timing: ApiTimings::default(),
            provider_source: None,
            completed_at_ms: T0 + 2,
        },
    )?;
    let (_, next_seq) = compaction.id_allocator.frontier();
    compaction.id_allocator = IdAllocator::from_frontier(JS_SAFE_INTEGER_MAX + 1, next_seq);
    let before_compaction = compaction.clone();
    assert!(
        compaction
            .record_compaction(
                CompactionCommitInput::new(
                    api,
                    occurred,
                    Seq::try_new(1)?,
                    "summary",
                    LockedBaselineProjection::empty(),
                ),
                T0 + 3,
            )
            .is_err()
    );
    assert_eq!(compaction, before_compaction);

    let mut truncation = new_document();
    truncation.submit_root_input_fixture("root", Vec::new(), T0)?;
    truncation.append_system_context(
        None,
        "context",
        SystemContextOperation::Upsert,
        SystemContextScope::Session,
        None,
        None,
        "context",
        T0 + 1,
    )?;
    truncation.events[0].seq = Seq::try_new(3)?;
    let before_truncation = truncation.clone();
    assert!(truncation.truncate_after(Seq::try_new(2)?, T0 + 2).is_err());
    assert_eq!(truncation, before_truncation);
    Ok(())
}
