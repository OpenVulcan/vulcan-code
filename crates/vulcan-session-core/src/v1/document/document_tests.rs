//! Shared fixtures and responsibility-oriented regression suites for `SessionDocument`.
//! `SessionDocument` 的共享夹具及按职责组织的回归测试套件。
//!
//! The primary invariant under test is: persistence order equals actual time
//! order, meaning the seq allocated by each mutation equals the order in
//! which the semantic fact actually occurred, and updates never allocate new
//! seq values.
//! 被测的首要不变量是：持久化顺序等于实际时间顺序，即每次 mutation 分配的
//! seq 等于该语义事实实际发生的顺序，且更新绝不分配新的 seq。

mod api;
mod business_api_boundaries;
mod compaction;
mod hydration;
mod inactive_admission;
mod input;
mod input_handoff;
mod invariants;
mod modes;
mod persistence;
mod provider_source;
mod remote_compaction;
mod selection;
mod subagent;
mod terminal_settlement;
mod tools;
mod turns;
mod user_prompt;

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
    complete_ok_for_request(
        doc,
        api_call_id,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        provider_source,
        now_ms,
    )
}

/// Completes one physical API successfully with its exact frozen request snapshot.
/// 使用物理 API 精确冻结的请求快照成功完成该调用。
fn complete_ok_for_request(
    doc: &mut SessionDocument,
    api_call_id: ApiCallId,
    request: ApiRequestSnapshot,
    provider_source: Option<ProviderSourceInput>,
    now_ms: i64,
) -> Result<Seq, DomainError> {
    doc.complete_api_group(
        api_call_id,
        ApiCompletion {
            request,
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
