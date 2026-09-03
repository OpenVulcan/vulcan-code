//! Physical API request admission, semantic streaming updates, and terminal transitions.
//! 物理 API 请求准入、语义流更新及终态转换。

use super::compaction::validate_provider_projection_proof;
use super::input::validate_user_input;
use super::persistence::DirtyFile;
use super::provider_source::{
    build_provider_source, ensure_provider_source_matches, provider_source_at_seq,
    validate_provider_source_input,
};
use super::selection::{
    active_request_from_snapshot, validate_api_request_snapshot,
    validate_request_against_active_config,
};
#[cfg(test)]
use super::test_provider_request_budget_evidence;
use super::tools::tool_status_name;
use super::{
    ActiveApiEventKind, ActiveApiState, IdempotentMutation, SessionDocument,
    ensure_api_event_owner, ensure_api_request_field, validate_timestamp,
};
use crate::v1::budget::ProviderRequestBudgetEvidence;
use crate::v1::config::ActiveRequestConfig;
use crate::v1::context::{LockedBaselineProjection, RuntimeInitializationState};
use crate::v1::errors::DomainError;
use crate::v1::event::{
    ApiCallStatus, ApiEndReason, ApiErrorCategory, ApiPurposeKind, ApiRequestSnapshot,
    ApiStartedEvent, ApiStatsEvent, ApiTimings, AssistantReasoningEvent, AssistantSemanticStatus,
    AssistantTextEvent, ContinuationEligibility, ConversationEvent, DisplayNoteEvent,
    DisplayNoticeKind, EventBody, EventVisibility, ExecutionControlEvent, ExecutionControlKind,
    NoticeSeverity, ProviderProjectionProof, ProviderSource, ProviderSourceInput, RedactedApiError,
    ToolCallStatus, ToolPayload, ToolTerminalErrorKind, UserInputEvent, UserInputSource,
};
use crate::v1::ids::{ApiCallId, ClientActionId, JS_SAFE_INTEGER_MAX, Seq, TurnId};
use crate::v1::stats::UsageSnapshot;
use std::collections::{BTreeMap, BTreeSet};

/// Complete terminal fact for one physical Provider API call.
/// 一次物理 Provider API 调用的完整终态事实。
#[derive(Debug, Clone, PartialEq)]
pub struct ApiCompletion {
    /// Resolved request identity that must equal the durable start snapshot.
    /// 必须等于持久化起始快照的已解析请求身份。
    pub request: ApiRequestSnapshot,
    /// Official Provider usage snapshot.
    /// Provider 官方用量快照。
    pub usage: UsageSnapshot,
    /// Terminal API status.
    /// API 终态。
    pub status: ApiCallStatus,
    /// Optional stable end-reason classification.
    /// 可选稳定结束原因分类。
    pub end_reason: Option<ApiEndReason>,
    /// Optional redacted API error.
    /// 可选脱敏 API 错误。
    pub error: Option<RedactedApiError>,
    /// Frozen transport-policy decision for later explicit continuation.
    /// 为后续显式继续冻结的 transport 策略判定。
    pub continuation_eligibility: ContinuationEligibility,
    /// API timing checkpoints.
    /// API 时间点。
    pub timing: ApiTimings,
    /// Optional Provider-native continuation source attached to the group.
    /// 附着到该组的可选 Provider 原生 continuation 来源。
    pub provider_source: Option<ProviderSourceInput>,
    /// Completion time in Unix milliseconds.
    /// 完成时间（Unix 毫秒）。
    pub completed_at_ms: i64,
}

/// Atomic receipt of one physical API terminal plus any in-turn inputs promoted behind it.
/// 一次物理 API 终态及其后提升的过程内输入的原子 receipt。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiGroupCompletionReceipt {
    /// Completed physical API identity.
    /// 已完成的物理 API 身份。
    pub api_call_id: ApiCallId,
    /// Owning logical Turn that remains open until an explicit Turn completion transition.
    /// 所属逻辑回合；在显式回合完成转换前保持开放。
    pub turn_id: TurnId,
    /// Seq of the terminal ApiStats fact.
    /// 终态 ApiStats 事实的 seq。
    pub stats_seq: Seq,
    /// Ordered Provider-visible input seq values promoted after the terminal fact.
    /// 在终态事实之后提升的有序 Provider 可见输入 seq。
    pub promoted_input_seqs: Vec<Seq>,
}

/// Strongly typed input for atomically freezing a Provider request and starting its API group.
/// 用于原子冻结 Provider 请求并启动其 API 组的强类型输入。
#[derive(Debug, Clone, PartialEq)]
pub struct PrepareAndStartApiInput {
    /// Execution purpose of the physical request.
    /// 物理请求的执行用途。
    pub purpose: ApiPurposeKind,
    /// Fully resolved provider-agnostic routing configuration.
    /// 已完整解析且与具体 Provider DTO 解耦的路由配置。
    pub request: ApiRequestSnapshot,
    /// Proof computed from the exact immutable request projection consumed by transport.
    /// 从 transport 实际消费的精确不可变请求投影计算得到的证明。
    pub projection: ProviderProjectionProof,
    /// Non-sensitive budget evidence computed from the exact request released to transport.
    /// 从实际放行给 transport 的精确请求计算得到的非敏感预算证据。
    pub request_budget_evidence: ProviderRequestBudgetEvidence,
    /// Exact baseline used when the runtime is fresh; initialized sessions must match it.
    /// runtime 为 fresh 时使用的精确基线；已初始化会话必须与其完全一致。
    pub locked_baseline: LockedBaselineProjection,
    /// Continue action dispatched by this API; required only for Continue purpose.
    /// 本次 API 派发的继续动作；仅 Continue 用途必须提供。
    pub continuation_action_id: Option<ClientActionId>,
    /// Mutation time in Unix milliseconds.
    /// 领域修改时间（Unix 毫秒）。
    pub now_ms: i64,
}

/// Durable Core result returned before the immutable network request may be dispatched.
/// 不可变网络请求获准派发前返回的 Core 持久化结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedApiStart {
    /// Allocated physical API identity.
    /// 已分配的物理 API 身份。
    pub api_call_id: ApiCallId,
    /// Seq of the durable ApiStarted fact.
    /// 持久化 ApiStarted 事实的 seq。
    pub started_seq: Seq,
    /// Exact projection proof persisted on ApiStarted.
    /// 持久化在 ApiStarted 上的精确投影证明。
    pub projection: ProviderProjectionProof,
    /// Continue action durably bound to this API, when applicable.
    /// 适用时持久化绑定到该 API 的继续动作。
    pub continuation_action_id: Option<ClientActionId>,
}

/// Durable receipt proving that transport observed one physical API request.
/// 证明 transport 已观察到一次物理 API 请求的持久化 receipt。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiTransportObservedReceipt {
    /// Physical API identity accepted by transport.
    /// transport 接受的物理 API 身份。
    pub api_call_id: ApiCallId,
    /// Seq of the immutable ApiStarted fact.
    /// 不可变 ApiStarted 事实的 seq。
    pub started_seq: Seq,
    /// Seq of the durable transport-observed fact.
    /// 持久化 transport-observed 事实的 seq。
    pub observed_seq: Seq,
    /// Continue action owning the API, when applicable.
    /// 适用时拥有该 API 的继续动作。
    pub continuation_action_id: Option<ClientActionId>,
}

/// Durable UI projection state associated with one terminal Continue dispatch.
/// 与一次 Continue 分派终态关联的可靠 UI 投影状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContinuationTerminalProjection {
    /// A user-visible terminal notice is durably recorded at the exact sequence.
    /// 用户可见的终态提示已经可靠记录在精确序号上。
    InlineNotice {
        /// Seq of the durable DisplayNote event.
        /// 可靠 DisplayNote 事件的 seq。
        notice_seq: Seq,
    },
    /// The terminal is expected and requires no error notice.
    /// 该终态属于预期结果，不需要错误提示。
    NoNoticeRequired,
    /// A failed terminal has no durable user-visible notice.
    /// 失败终态缺少可靠的用户可见提示。
    MissingNotice,
}

/// Durable dispatch state reconstructed for one Continue action.
/// 为一次 Continue 动作重建的持久化分派状态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContinuationDispatchResolution {
    /// The Continue-owned Turn terminated before any physical API was prepared.
    /// Continue 所属回合在任何物理 API 准备前已经终止。
    TerminalBeforeApi {
        /// Seq of the durable Continue request control fact.
        /// 可靠 Continue 请求控制事实的 seq。
        requested_seq: Seq,
        /// Seq of the durable Turn resume control fact.
        /// 可靠回合恢复控制事实的 seq。
        resumed_seq: Seq,
        /// Seq of the terminal Turn boundary.
        /// 终态回合边界的 seq。
        terminal_seq: Seq,
        /// Durable UI projection associated with this terminal boundary.
        /// 与该终态回合边界关联的可靠 UI 投影。
        projection: ContinuationTerminalProjection,
    },
    /// One physical API is prepared but transport has not been observed or terminated.
    /// 一个物理 API 已准备，但尚未观察到传输，也尚未终止。
    AwaitingTransport {
        /// Physical API identity bound to the Continue action.
        /// 绑定到 Continue 动作的物理 API 身份。
        api_call_id: ApiCallId,
        /// Seq of the immutable ApiStarted event.
        /// 不可变 ApiStarted 事件的 seq。
        started_seq: Seq,
    },
    /// Transport observation is durably recorded.
    /// 传输观察已经可靠记录。
    TransportObserved(ApiTransportObservedReceipt),
    /// The physical API terminated before transport observation became durable.
    /// 物理 API 在传输观察可靠落盘前已经终止。
    TerminalBeforeTransport {
        /// Physical API identity bound to the Continue action.
        /// 绑定到 Continue 动作的物理 API 身份。
        api_call_id: ApiCallId,
        /// Seq of the immutable ApiStarted event.
        /// 不可变 ApiStarted 事件的 seq。
        started_seq: Seq,
        /// Seq of the terminal ApiStats event.
        /// 终态 ApiStats 事件的 seq。
        terminal_seq: Seq,
        /// Durable UI projection associated with this API terminal.
        /// 与该 API 终态关联的可靠 UI 投影。
        projection: ContinuationTerminalProjection,
    },
}

/// Validates one cumulative Provider semantic snapshot and returns whether it changed.
/// 校验一份累计 Provider 语义快照，并返回它是否发生了真实变化。
///
/// # Parameters
/// 参数
/// `current_text` and `current_status` are the durable semantic state, while
/// `next_text` and `next_status` are the full cumulative snapshot being applied.
/// Provider source identity is immutable once attached.
/// `current_text` 与 `current_status` 是持久化语义状态，`next_text` 与
/// `next_status` 是待应用的完整累计快照；Provider 来源身份一经附加即不可变。
///
/// # Returns
/// 返回值
/// Returns `true` only for a monotonic text/status evolution and `false` for an
/// exact replay. Regressions, terminal rewrites, and source drift are rejected.
/// 仅单调正文或状态演进返回 `true`，完全相同的重放返回 `false`；正文回退、
/// 终态改写与来源漂移都会被拒绝。
fn validate_provider_semantic_snapshot_transition(
    current_text: &str,
    current_status: AssistantSemanticStatus,
    current_provider_source: Option<&ProviderSource>,
    next_text: &str,
    next_status: AssistantSemanticStatus,
    next_provider_source: Option<&ProviderSource>,
) -> Result<bool, DomainError> {
    if current_provider_source != next_provider_source {
        return Err(DomainError::ImmutableFieldMutation {
            entity: "assistant_semantic",
            field: "provider_source",
        });
    }
    if current_text == next_text && current_status == next_status {
        return Ok(false);
    }
    if current_status != AssistantSemanticStatus::Streaming {
        return Err(DomainError::ImmutableFieldMutation {
            entity: "assistant_semantic",
            field: "terminal_snapshot",
        });
    }
    if !next_text.starts_with(current_text) {
        return Err(DomainError::ImmutableFieldMutation {
            entity: "assistant_semantic",
            field: "cumulative_text",
        });
    }
    Ok(true)
}

/// Validates the exact purpose-to-Continue-action shape of one API start.
/// 校验一次 API 启动的用途与 Continue 动作之间的精确结构。
fn validate_api_continuation_binding_shape(
    purpose: ApiPurposeKind,
    continuation_action_id: Option<&ClientActionId>,
) -> Result<(), DomainError> {
    match (
        purpose == ApiPurposeKind::Continue,
        continuation_action_id.is_some(),
    ) {
        (true, false) => Err(DomainError::InvalidContinuationHistory {
            field: "api_started.continuation_action_id_missing",
        }),
        (false, true) => Err(DomainError::InvalidContinuationHistory {
            field: "api_started.continuation_action_id_unexpected",
        }),
        _ => Ok(()),
    }
}

/// Validates durable Continue-to-API bindings and transport observations.
/// 校验持久化 Continue 到 API 的绑定及 transport 观察事实。
pub(super) fn validate_api_dispatch_history(
    events: &[ConversationEvent],
) -> Result<(), DomainError> {
    let mut starts = BTreeMap::new();
    let mut bound_actions = BTreeSet::new();
    for event in events {
        let EventBody::ApiStarted(started) = &event.event else {
            continue;
        };
        validate_api_continuation_binding_shape(
            started.purpose,
            started.continuation_action_id.as_ref(),
        )?;
        if starts
            .insert(started.api_call_id, (event, started))
            .is_some()
        {
            return Err(DomainError::InvalidContinuationHistory {
                field: "duplicate_api_started",
            });
        }
        let Some(action_id) = started.continuation_action_id.as_ref() else {
            continue;
        };
        if !bound_actions.insert(action_id.clone()) {
            return Err(DomainError::InvalidContinuationHistory {
                field: "duplicate_continue_api_binding",
            });
        }
        let mut requested = None;
        let mut resumed = None;
        for candidate in events
            .iter()
            .take_while(|candidate| candidate.seq < event.seq)
        {
            let EventBody::ExecutionControl(control) = &candidate.event else {
                continue;
            };
            if control.client_action_id.as_ref() != Some(action_id) {
                continue;
            }
            match control.control_kind {
                ExecutionControlKind::TurnContinueRequested => requested = Some(candidate),
                ExecutionControlKind::TurnResumed => resumed = Some(candidate),
                _ => {}
            }
        }
        let requested = requested.ok_or(DomainError::InvalidContinuationHistory {
            field: "continue_api.missing_requested",
        })?;
        let resumed = resumed.ok_or(DomainError::InvalidContinuationHistory {
            field: "continue_api.missing_resumed",
        })?;
        if requested.turn_id != event.turn_id
            || resumed.turn_id != event.turn_id
            || requested.seq.value() >= resumed.seq.value()
        {
            return Err(DomainError::InvalidContinuationHistory {
                field: "continue_api.turn_or_order",
            });
        }
    }

    let mut observed_api_ids = BTreeSet::new();
    for event in events {
        let EventBody::ExecutionControl(control) = &event.event else {
            continue;
        };
        if control.control_kind != ExecutionControlKind::ApiTransportObserved {
            continue;
        }
        let api_call_id = control
            .api_call_id
            .ok_or(DomainError::InvalidContinuationHistory {
                field: "api_transport_observed.api_call_id_missing",
            })?;
        if !observed_api_ids.insert(api_call_id) {
            return Err(DomainError::InvalidContinuationHistory {
                field: "duplicate_api_transport_observed",
            });
        }
        let (started_event, _) = starts.get(&api_call_id).ok_or_else(|| {
            DomainError::reference("api_transport_observed.api_started", api_call_id.value())
        })?;
        if event.turn_id != started_event.turn_id || event.seq <= started_event.seq {
            return Err(DomainError::InvalidContinuationHistory {
                field: "api_transport_observed.turn_or_order",
            });
        }
        if events.iter().any(|candidate| {
            candidate.seq < event.seq
                && matches!(
                    &candidate.event,
                    EventBody::ApiStats(stats) if stats.api_call_id == api_call_id
                )
        }) {
            return Err(DomainError::InvalidContinuationHistory {
                field: "api_transport_observed_after_terminal",
            });
        }
    }
    Ok(())
}

/// Validates the external identity shape of one execution-control event.
/// 校验一条执行控制事件的外部身份结构。
pub(super) fn validate_execution_control_event(
    control: &ExecutionControlEvent,
) -> Result<(), DomainError> {
    let is_continue = matches!(
        control.control_kind,
        ExecutionControlKind::TurnContinueRequested | ExecutionControlKind::TurnResumed
    );
    let is_transport = control.control_kind == ExecutionControlKind::ApiTransportObserved;
    match (is_continue, control.client_action_id.is_some()) {
        (true, false) => Err(DomainError::InvalidContinuationHistory {
            field: "execution_control.client_action_id_missing",
        }),
        (false, true) => Err(DomainError::InvalidContinuationHistory {
            field: "execution_control.client_action_id_unexpected",
        }),
        _ if is_continue && control.api_call_id.is_some() => {
            Err(DomainError::InvalidContinuationHistory {
                field: "execution_control.api_call_id",
            })
        }
        _ if is_transport && control.api_call_id.is_none() => {
            Err(DomainError::InvalidContinuationHistory {
                field: "api_transport_observed.api_call_id_missing",
            })
        }
        _ => Ok(()),
    }
}

/// Validates persisted request-budget evidence at both commit and strict reload boundaries.
/// 在提交与严格重载边界同时校验持久化请求预算证据。
pub(super) fn validate_provider_request_budget_evidence(
    evidence: &ProviderRequestBudgetEvidence,
) -> Result<(), DomainError> {
    validate_provider_budget_fingerprint(
        "request_assembly_fingerprint",
        &evidence.request_assembly_fingerprint,
    )?;
    validate_provider_budget_fingerprint(
        "model_route_fingerprint",
        &evidence.model_route_fingerprint,
    )?;
    validate_provider_budget_fingerprint("toolset_fingerprint", &evidence.toolset_fingerprint)?;
    validate_provider_budget_fingerprint(
        "system_prompt_fingerprint",
        &evidence.system_prompt_fingerprint,
    )?;
    let measurement = &evidence.request_text_measurement;
    for (field, value) in [
        ("character_count", measurement.character_count),
        ("utf8_byte_count", measurement.utf8_byte_count),
        ("structural_units", measurement.structural_units),
        ("history_message_count", measurement.history_message_count),
        (
            "context_breakdown.message_units",
            measurement.context_breakdown.message_units,
        ),
        (
            "context_breakdown.system_tool_units",
            measurement.context_breakdown.system_tool_units,
        ),
        (
            "context_breakdown.mcp_tool_units",
            measurement.context_breakdown.mcp_tool_units,
        ),
        (
            "context_breakdown.skill_units",
            measurement.context_breakdown.skill_units,
        ),
        (
            "context_breakdown.system_prompt_units",
            measurement.context_breakdown.system_prompt_units,
        ),
        (
            "context_breakdown.other_units",
            measurement.context_breakdown.other_units,
        ),
    ] {
        if value > JS_SAFE_INTEGER_MAX {
            return Err(DomainError::InvalidProviderBudgetEvidence { field });
        }
    }
    if measurement.utf8_byte_count < measurement.character_count {
        return Err(DomainError::InvalidProviderBudgetEvidence {
            field: "utf8_byte_count",
        });
    }
    validate_provider_budget_fingerprint("history_fingerprint", &measurement.history_fingerprint)?;
    validate_provider_budget_fingerprint(
        "prefix_invariant_fingerprint",
        &measurement.prefix_invariant_fingerprint,
    )
}

/// Validates one lowercase SHA-256 identity without accepting alternate encodings.
/// 校验一个小写 SHA-256 身份，不接受其他编码形式。
fn validate_provider_budget_fingerprint(
    field: &'static str,
    value: &str,
) -> Result<(), DomainError> {
    let Some(hex) = value.strip_prefix("sha256:") else {
        return Err(DomainError::InvalidProviderBudgetEvidence { field });
    };
    if hex.len() != 64
        || !hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(DomainError::InvalidProviderBudgetEvidence { field });
    }
    Ok(())
}

/// Validates the single legal terminal status/reason/error/timing contract of one API.
/// 校验一次 API 唯一合法的终态状态、原因、错误与时间契约。
///
/// # Parameters
/// 参数
/// `status`, `end_reason`, and `error` describe the terminal classification; `timing`
/// contains the ordered physical checkpoints persisted in `ApiStats`.
/// `status`、`end_reason` 与 `error` 描述终态分类；`timing` 包含持久化在
/// `ApiStats` 中的有序物理时间点。
///
/// # Returns
/// 返回值
/// Returns the required terminal completion timestamp after every cross-field invariant
/// succeeds.
/// 所有跨字段不变量通过后返回必需的终态完成时间。
pub(super) fn validate_api_terminal_facts(
    status: ApiCallStatus,
    end_reason: Option<ApiEndReason>,
    error: Option<&RedactedApiError>,
    continuation_eligibility: ContinuationEligibility,
    timing: &ApiTimings,
) -> Result<i64, DomainError> {
    let reason = end_reason.ok_or(DomainError::InvalidApiTerminalFacts {
        field: "end_reason_missing",
    })?;
    match (status, reason, error) {
        (ApiCallStatus::Completed, ApiEndReason::CompletedSuccessfully, None)
        | (ApiCallStatus::Interrupted, ApiEndReason::UserInterrupted, None)
        | (ApiCallStatus::Interrupted, ApiEndReason::ProcessInterrupted, None)
        | (ApiCallStatus::Cancelled, ApiEndReason::Cancelled, None) => {}
        (ApiCallStatus::Failed, ApiEndReason::ProviderError, Some(error))
            if matches!(
                error.category,
                ApiErrorCategory::Provider | ApiErrorCategory::Auth | ApiErrorCategory::Protocol
            ) => {}
        (ApiCallStatus::Failed, ApiEndReason::NetworkError, Some(error))
            if matches!(
                error.category,
                ApiErrorCategory::Network | ApiErrorCategory::Timeout
            ) => {}
        (ApiCallStatus::Failed, ApiEndReason::BalanceInsufficient, Some(error))
            if error.category == ApiErrorCategory::Balance => {}
        (ApiCallStatus::Failed, ApiEndReason::RetryExhausted, Some(_)) => {}
        (ApiCallStatus::Failed, ApiEndReason::Unknown, Some(error))
            if error.category == ApiErrorCategory::Unknown => {}
        (ApiCallStatus::Prepared | ApiCallStatus::Dispatched | ApiCallStatus::InProgress, _, _) => {
            return Err(DomainError::InvalidApiTerminalFacts {
                field: "status_not_terminal",
            });
        }
        _ => {
            return Err(DomainError::InvalidApiTerminalFacts {
                field: "status_reason_error",
            });
        }
    }
    let required_eligibility = match reason {
        ApiEndReason::UserInterrupted
        | ApiEndReason::ProcessInterrupted
        | ApiEndReason::NetworkError
        | ApiEndReason::BalanceInsufficient
        | ApiEndReason::RetryExhausted => Some(ContinuationEligibility::Allowed),
        ApiEndReason::CompletedSuccessfully | ApiEndReason::Cancelled | ApiEndReason::Unknown => {
            Some(ContinuationEligibility::Forbidden)
        }
        ApiEndReason::ProviderError => None,
    };
    if required_eligibility.is_some_and(|required| required != continuation_eligibility) {
        return Err(DomainError::InvalidApiTerminalFacts {
            field: "continuation_eligibility",
        });
    }
    if let Some(error) = error
        && error.message.trim().is_empty()
    {
        return Err(DomainError::InvalidApiTerminalFacts {
            field: "error_message",
        });
    }

    let completed_at_ms = timing
        .completed_at_ms
        .ok_or(DomainError::InvalidApiTerminalFacts {
            field: "timing.completed_at_ms",
        })?;
    let ordered = [
        ("timing.prepared_at_ms", timing.prepared_at_ms),
        ("timing.dispatched_at_ms", timing.dispatched_at_ms),
        ("timing.first_event_at_ms", timing.first_event_at_ms),
        ("timing.first_token_at_ms", timing.first_token_at_ms),
        ("timing.completed_at_ms", Some(completed_at_ms)),
    ];
    let mut previous = None;
    for (field, value) in ordered {
        let Some(value) = value else {
            continue;
        };
        if let Some(previous) = previous
            && value < previous
        {
            return Err(DomainError::InvalidApiTerminalFacts { field });
        }
        previous = Some(value);
    }
    Ok(completed_at_ms)
}

/// Maps one physical API failure reason to its permanent display notice.
/// 把一次物理 API 失败原因映射为永久显示通知。
fn api_failure_notice(
    reason: ApiEndReason,
) -> Option<(DisplayNoticeKind, NoticeSeverity, &'static str)> {
    match reason {
        ApiEndReason::ProviderError => Some((
            DisplayNoticeKind::ProviderError,
            NoticeSeverity::Error,
            "provider_error",
        )),
        ApiEndReason::NetworkError => Some((
            DisplayNoticeKind::NetworkDisconnected,
            NoticeSeverity::Warning,
            "network_error",
        )),
        ApiEndReason::BalanceInsufficient => Some((
            DisplayNoticeKind::BalanceInsufficient,
            NoticeSeverity::Warning,
            "balance_insufficient",
        )),
        ApiEndReason::RetryExhausted => Some((
            DisplayNoticeKind::RetryExhausted,
            NoticeSeverity::Error,
            "retry_exhausted",
        )),
        ApiEndReason::Unknown => Some((
            DisplayNoticeKind::SystemError,
            NoticeSeverity::Error,
            "system_error",
        )),
        ApiEndReason::CompletedSuccessfully
        | ApiEndReason::UserInterrupted
        | ApiEndReason::ProcessInterrupted
        | ApiEndReason::Cancelled => None,
    }
}

// Keep all mutations on the single authoritative document instance.
// 所有修改仍作用于唯一权威文档实例。
impl SessionDocument {
    /// Starts one physical Provider API group inside the open turn.
    /// 在开放回合内启动一次物理 Provider API 组。
    pub fn prepare_and_start_api(
        &mut self,
        input: PrepareAndStartApiInput,
    ) -> Result<IdempotentMutation<PreparedApiStart>, DomainError> {
        self.prepare_and_start_api_with_user_prompt_media(input, Vec::new())
    }

    /// Atomically freezes `prompt_media` on their user events and starts the exact physical request in `input`.
    /// 将`prompt_media`原子冻结到各自用户事件，并启动`input`中的精确物理请求。
    /// Returns one shared revision boundary; failed API validation cannot leave a partially frozen prompt.
    /// 返回同一修订边界；API校验失败不能留下部分冻结的提示词。
    pub fn prepare_and_start_api_with_user_prompt_media(
        &mut self,
        input: PrepareAndStartApiInput,
        prompt_media: Vec<crate::v1::UserPromptMediaBinding>,
    ) -> Result<IdempotentMutation<PreparedApiStart>, DomainError> {
        let PrepareAndStartApiInput {
            purpose,
            request,
            projection,
            request_budget_evidence,
            locked_baseline,
            continuation_action_id,
            now_ms,
        } = input;
        // A newly frozen prompt must use the exact captured request route, never a later global selection.
        // 新冻结提示词必须使用精确捕获请求路线，绝不能使用后来的全局选择。
        for binding in &prompt_media {
            let route = &binding.origin.route;
            if route.provider_id != request.provider
                || route.model_id != request.model
                || Some(route.connection_id.as_str()) != request.connection_id.as_deref()
                || Some(route.protocol_profile_id.as_str()) != request.profile.as_deref()
            {
                return Err(crate::v1::user_prompt::invalid_prompt_media(
                    "captured_request_route_mismatch",
                ));
            }
        }
        validate_api_continuation_binding_shape(purpose, continuation_action_id.as_ref())?;
        if let Some(action_id) = continuation_action_id.as_ref()
            && let Some(receipt) = self.existing_api_start_for_action(action_id)?
        {
            // Repeating an old API cannot use its idempotency key to freeze any later prompt.
            // 重复旧API不能借助其幂等键冻结任何较晚提示词。
            if !self
                .stage_user_prompt_media(prompt_media, now_ms)?
                .is_empty()
            {
                return Err(crate::v1::user_prompt::invalid_prompt_media(
                    "idempotent_request_has_new_prompt_media",
                ));
            }
            return Ok(IdempotentMutation::Idempotent(receipt));
        }
        validate_timestamp("api_started.now_ms", now_ms)?;
        validate_api_request_snapshot(&request)?;
        validate_provider_projection_proof(self, &projection)?;
        validate_provider_request_budget_evidence(&request_budget_evidence)?;
        let requested_active = active_request_from_snapshot(&request);
        let active_is_empty = self.metadata.config.active_request == ActiveRequestConfig::default();
        // A rewritten history generation owns a fresh request boundary even though it retains the
        // previous generation's active selection for display and fallback. Its first physical API
        // must establish the exact current request together with the refreshed prompt baseline.
        // 重写历史代际拥有新的请求边界，即使它为显示和回退保留了上一代际的活动选择。它的首次
        // 物理 API 必须与刷新的提示词基线一起建立当前精确请求。
        let request_rebase_allowed = self.runtime_lock.initialization_state
            == RuntimeInitializationState::BaselineRefreshPending;
        if !active_is_empty && !request_rebase_allowed {
            validate_request_against_active_config(
                &self.metadata.config.active_request,
                &requested_active,
            )?;
        } else if self.runtime_lock.initialization_state == RuntimeInitializationState::Initialized
        {
            return Err(DomainError::InvalidRequestConfiguration {
                field: "active_request_uninitialized",
            });
        }
        let turn_id = self.require_open_turn()?.turn_id;
        if let Some(action_id) = continuation_action_id.as_ref() {
            self.validate_continuation_action_for_api(action_id, turn_id)?;
        }
        if self.active_api.is_some() {
            return Err(DomainError::NoActiveApi);
        }
        self.ensure_transition_capacity(1, 1)?;

        // Prompt origin belongs to its chronological user event, never to ApiStarted or a cache's latest value.
        // 提示词原身份属于其时序用户事件，绝不属于ApiStarted或缓存的最近值。
        let staged_prompt_events = self.stage_user_prompt_media(prompt_media, now_ms)?;

        let mut staged_runtime_lock = self.runtime_lock.clone();
        let locked_sources = locked_baseline.into_locked_sources();
        let runtime_changed = match staged_runtime_lock.initialization_state {
            RuntimeInitializationState::FreshUninitialized
            | RuntimeInitializationState::BaselineRefreshPending => {
                staged_runtime_lock.initialization_state = RuntimeInitializationState::Initialized;
                staged_runtime_lock.locked_sources = locked_sources;
                true
            }
            RuntimeInitializationState::Initialized => {
                if staged_runtime_lock.locked_sources != locked_sources {
                    return Err(DomainError::RuntimeBaselineAlreadyInitialized);
                }
                false
            }
        };
        let mut staged_metadata = self.metadata.clone();
        if active_is_empty || request_rebase_allowed {
            staged_metadata.config.active_request = requested_active;
        }

        let mut staged_allocator = self.id_allocator;
        let api_call_id = ApiCallId::try_new(staged_allocator.next_local_id()?)?;
        let mut staged_events = Vec::with_capacity(1);
        let started_seq = self.stage_event(
            &mut staged_allocator,
            &mut staged_events,
            Some(turn_id),
            EventVisibility::HostOnly,
            EventBody::ApiStarted(ApiStartedEvent {
                api_call_id,
                purpose,
                request: request.clone(),
                projection: projection.clone(),
                request_budget_evidence,
                continuation_action_id: continuation_action_id.clone(),
            }),
            now_ms,
        )?;
        // All fallible checks are complete; update prompts and append the API in one failure-atomic revision.
        // 所有可能失败的检查均已完成；在一个失败原子修订内更新提示词并追加API。
        let mut changed_seqs = staged_prompt_events
            .iter()
            .map(|(_, event)| event.seq())
            .collect::<Vec<_>>();
        for (index, event) in staged_prompt_events {
            self.events[index] = event;
        }
        changed_seqs.push(started_seq);
        self.runtime_lock = staged_runtime_lock;
        self.metadata = staged_metadata;
        self.id_allocator = staged_allocator;
        self.events.extend(staged_events);
        self.active_api = Some(ActiveApiState {
            api_call_id,
            turn_id,
            purpose,
            started_seq,
            request,
            projection: projection.clone(),
            reasoning_seq: None,
            text_seq: None,
            tool_seqs: Vec::new(),
            stats_seq: None,
            provider_source_placed: false,
        });
        let dirty = if runtime_changed {
            vec![DirtyFile::ActiveTail, DirtyFile::Runtime]
        } else {
            vec![DirtyFile::ActiveTail]
        };
        self.bump_revision(&dirty, now_ms, &changed_seqs);
        Ok(IdempotentMutation::Committed(PreparedApiStart {
            api_call_id,
            started_seq,
            projection,
            continuation_action_id,
        }))
    }

    /// Starts one API with an explicit deterministic projection proof for unit fixtures only.
    /// 仅供单元夹具使用显式确定性投影证明启动一次 API。
    #[cfg(test)]
    pub(crate) fn start_api(
        &mut self,
        purpose: ApiPurposeKind,
        request: ApiRequestSnapshot,
        now_ms: i64,
    ) -> Result<ApiCallId, DomainError> {
        let continuation_action_id = if purpose == ApiPurposeKind::Continue {
            self.events
                .iter()
                .rev()
                .find_map(|event| match &event.event {
                    EventBody::ExecutionControl(control)
                        if control.control_kind == ExecutionControlKind::TurnResumed =>
                    {
                        control.client_action_id.clone()
                    }
                    _ => None,
                })
        } else {
            None
        };
        let retained_from_seq = match self.runtime_lock.current_compaction_id {
            Some(compaction_id) => self
                .compactions
                .iter()
                .find(|node| node.compaction_id == compaction_id)
                .map(|node| node.retained_from_seq)
                .ok_or_else(|| {
                    DomainError::reference("current_compaction", compaction_id.value())
                })?,
            None => Seq::try_new(1)?,
        };
        let prepared = self.prepare_and_start_api(PrepareAndStartApiInput {
            purpose,
            request,
            request_budget_evidence: test_provider_request_budget_evidence(),
            projection: ProviderProjectionProof {
                segment_generation: self.metadata.segment_generation,
                document_revision: self.memory_revision,
                retained_from_seq,
                through_seq: self.last_seq()?,
                fingerprint_sha256: "0".repeat(64),
            },
            locked_baseline: LockedBaselineProjection::from_locked_sources(
                self.runtime_lock.locked_sources.clone(),
            )?,
            continuation_action_id,
            now_ms,
        })?;
        Ok(prepared.into_value().api_call_id)
    }

    /// Records that transport accepted one immutable physical API request.
    /// 记录 transport 已接受一份不可变物理 API 请求。
    ///
    /// # Parameters / 参数
    /// `api_call_id` identifies the already durable ApiStarted fact and
    /// `now_ms` is the explicit observation boundary time.
    /// `api_call_id` 标识已经持久化的 ApiStarted 事实，`now_ms` 是显式观察边界时间。
    ///
    /// # Returns / 返回值
    /// Returns a committed receipt or the original idempotent receipt.
    /// 返回已提交 receipt 或原始幂等 receipt。
    pub fn mark_api_transport_observed(
        &mut self,
        api_call_id: ApiCallId,
        now_ms: i64,
    ) -> Result<IdempotentMutation<ApiTransportObservedReceipt>, DomainError> {
        let started_event = self
            .events
            .iter()
            .find(|event| {
                matches!(
                    &event.event,
                    EventBody::ApiStarted(started) if started.api_call_id == api_call_id
                )
            })
            .ok_or_else(|| DomainError::reference("api_started", api_call_id.value()))?;
        let EventBody::ApiStarted(started) = &started_event.event else {
            unreachable!("ApiStarted selected above")
        };
        let started_seq = started_event.seq;
        let started_turn_id = started_event.turn_id;
        let continuation_action_id = started.continuation_action_id.clone();
        let mut existing_observed_seq = None;
        for event in &self.events {
            if let EventBody::ExecutionControl(control) = &event.event
                && control.control_kind == ExecutionControlKind::ApiTransportObserved
                && control.api_call_id == Some(api_call_id)
                && existing_observed_seq.replace(event.seq).is_some()
            {
                return Err(DomainError::InvalidContinuationHistory {
                    field: "duplicate_api_transport_observed",
                });
            }
        }
        let receipt = |observed_seq| ApiTransportObservedReceipt {
            api_call_id,
            started_seq,
            observed_seq,
            continuation_action_id: continuation_action_id.clone(),
        };
        if let Some(observed_seq) = existing_observed_seq {
            return Ok(IdempotentMutation::Idempotent(receipt(observed_seq)));
        }
        validate_timestamp("api_transport_observed.now_ms", now_ms)?;
        if self.events.iter().any(|event| {
            matches!(
                &event.event,
                EventBody::ApiStats(stats) if stats.api_call_id == api_call_id
            )
        }) {
            return Err(DomainError::InvalidContinuationHistory {
                field: "api_transport_observed_after_terminal",
            });
        }
        match &self.active_api {
            Some(active) if active.api_call_id == api_call_id => {}
            _ => return Err(DomainError::NoActiveApi),
        }
        self.ensure_transition_capacity(1, 0)?;
        let turn_id = started_turn_id
            .ok_or_else(|| DomainError::reference("api_started.turn_id", api_call_id.value()))?;
        let mut staged_allocator = self.id_allocator;
        let mut staged_events = Vec::with_capacity(1);
        let observed_seq = self.stage_event(
            &mut staged_allocator,
            &mut staged_events,
            Some(turn_id),
            EventVisibility::HostOnly,
            EventBody::ExecutionControl(ExecutionControlEvent {
                control_kind: ExecutionControlKind::ApiTransportObserved,
                api_call_id: Some(api_call_id),
                client_action_id: None,
            }),
            now_ms,
        )?;
        self.id_allocator = staged_allocator;
        self.events.extend(staged_events);
        self.bump_revision(&[DirtyFile::ActiveTail], now_ms, &[observed_seq]);
        Ok(IdempotentMutation::Committed(receipt(observed_seq)))
    }

    /// Reads the durable transport-observed receipt owned by one Continue action.
    /// 读取由一次 Continue 动作拥有的持久化 transport-observed receipt。
    pub fn api_transport_observed_for_action(
        &self,
        action_id: &ClientActionId,
    ) -> Result<Option<ApiTransportObservedReceipt>, DomainError> {
        let Some(prepared) = self.existing_api_start_for_action(action_id)? else {
            return Ok(None);
        };
        let mut observed_seq = None;
        for event in &self.events {
            let EventBody::ExecutionControl(control) = &event.event else {
                continue;
            };
            if control.control_kind != ExecutionControlKind::ApiTransportObserved
                || control.api_call_id != Some(prepared.api_call_id)
            {
                continue;
            }
            if observed_seq.replace(event.seq).is_some() {
                return Err(DomainError::InvalidContinuationHistory {
                    field: "duplicate_api_transport_observed",
                });
            }
        }
        Ok(
            observed_seq.map(|observed_seq| ApiTransportObservedReceipt {
                api_call_id: prepared.api_call_id,
                started_seq: prepared.started_seq,
                observed_seq,
                continuation_action_id: prepared.continuation_action_id,
            }),
        )
    }

    /// Reconstructs the exact durable dispatch resolution for one Continue action.
    /// 重建一次 Continue 动作的精确持久化分派结果。
    ///
    /// # Parameters / 参数
    /// `action_id` is the browser idempotency identity already bound to Continue events.
    /// `action_id` 是已经绑定到 Continue 事件的浏览器幂等身份。
    ///
    /// # Returns / 返回值
    /// `None` before ApiStarted, otherwise the prepared, observed, or terminal-before-observed
    /// state. Duplicate terminal facts are rejected instead of being guessed.
    /// ApiStarted 前返回 `None`；否则返回已准备、已观察或观察前终止状态。重复终态事实会被
    /// 明确拒绝，而不是进行猜测。
    pub fn continuation_dispatch_resolution(
        &self,
        action_id: &ClientActionId,
    ) -> Result<Option<ContinuationDispatchResolution>, DomainError> {
        let Some(through_seq) = self.events.last().map(|event| event.seq) else {
            return Ok(None);
        };
        self.continuation_dispatch_resolution_through(action_id, through_seq)
    }

    /// Resolves one Continue dispatch from facts no later than an exact durable event frontier.
    /// 仅使用不晚于精确可靠事件前沿的事实解析一次 Continue 分派。
    ///
    /// # Parameters / 参数
    ///
    /// `action_id` is the persisted client idempotency identity and `through_seq` is the last event
    /// known to be durable by the unique storage owner.
    /// `action_id` 是持久化客户端幂等身份；`through_seq` 是唯一存储 owner 已确认可靠的最后事件。
    ///
    /// # Returns / 返回值
    ///
    /// Returns only the dispatch state derivable from the bounded durable prefix. Later in-memory
    /// facts are deliberately invisible to this query.
    /// 仅返回可由有界可靠前缀推导的分派状态；更晚的内存事实会被该查询明确忽略。
    pub fn continuation_dispatch_resolution_through(
        &self,
        action_id: &ClientActionId,
        through_seq: Seq,
    ) -> Result<Option<ContinuationDispatchResolution>, DomainError> {
        // continuationTurn binds the browser action to one exact logical Turn before any API exists.
        // continuationTurn 在任何 API 存在前把浏览器动作绑定到一个精确逻辑回合。
        let mut continuation_turn = None;
        for event in self
            .events
            .iter()
            .take_while(|event| event.seq <= through_seq)
        {
            let EventBody::ExecutionControl(control) = &event.event else {
                continue;
            };
            if control.control_kind != ExecutionControlKind::TurnContinueRequested
                || control.client_action_id.as_ref() != Some(action_id)
            {
                continue;
            }
            // turn_id is the exact logical owner recorded on the Continue request control fact.
            // turn_id 是 Continue 请求控制事实记录的精确逻辑所有者。
            let turn_id = event
                .turn_id
                .ok_or(DomainError::InvalidContinuationHistory {
                    field: "continue_request_turn_id",
                })?;
            if continuation_turn.replace((turn_id, event.seq)).is_some() {
                return Err(DomainError::InvalidContinuationHistory {
                    field: "duplicate_continue_request",
                });
            }
        }
        let mut prepared = None;
        for event in self
            .events
            .iter()
            .take_while(|event| event.seq <= through_seq)
        {
            let EventBody::ApiStarted(started) = &event.event else {
                continue;
            };
            if started.continuation_action_id.as_ref() != Some(action_id) {
                continue;
            }
            let receipt = PreparedApiStart {
                api_call_id: started.api_call_id,
                started_seq: event.seq,
                projection: started.projection.clone(),
                continuation_action_id: Some(action_id.clone()),
            };
            if prepared.replace(receipt).is_some() {
                return Err(DomainError::InvalidContinuationHistory {
                    field: "duplicate_continue_api_binding",
                });
            }
        }
        let Some(prepared) = prepared else {
            let Some((continuation_turn_id, requested_seq)) = continuation_turn else {
                return Ok(None);
            };
            // resumedSeq proves preparation actually began; a request-only action may still be awaiting admission.
            // resumedSeq 证明准备工作确已开始；只有请求事实的动作仍可能正在等待准入。
            let mut resumed_seq = None;
            for event in self
                .events
                .iter()
                .take_while(|event| event.seq <= through_seq)
            {
                let EventBody::ExecutionControl(control) = &event.event else {
                    continue;
                };
                if control.control_kind != ExecutionControlKind::TurnResumed
                    || control.client_action_id.as_ref() != Some(action_id)
                    || event.turn_id != Some(continuation_turn_id)
                {
                    continue;
                }
                if resumed_seq.replace(event.seq).is_some() {
                    return Err(DomainError::InvalidContinuationHistory {
                        field: "duplicate_continue_resume",
                    });
                }
            }
            let Some(resumed_seq) = resumed_seq else {
                return Ok(None);
            };
            // terminalSeq is action-scoped by the same Turn identity and cannot be confused with a later user Turn.
            // terminalSeq 通过同一回合身份限定到该动作，不会与后续用户回合混淆。
            let terminal_seq = self
                .events
                .iter()
                .take_while(|event| event.seq <= through_seq)
                .find(|event| {
                    event.seq > resumed_seq
                        && event.turn_id == Some(continuation_turn_id)
                        && matches!(event.event, EventBody::TurnBoundary(_))
                })
                .map(|event| event.seq);
            return Ok(terminal_seq.map(|terminal_seq| {
                // noticeSeq proves that the pre-API failure already has a durable inline error.
                // noticeSeq 证明 API 前失败已经拥有可靠的行内错误。
                let notice_seq = self
                    .events
                    .iter()
                    .take_while(|event| event.seq < terminal_seq)
                    .last()
                    .filter(|event| {
                        event.seq > resumed_seq
                            && event.turn_id == Some(continuation_turn_id)
                            && matches!(
                                &event.event,
                                EventBody::DisplayNote(note)
                                    if note.notice_kind == DisplayNoticeKind::SystemError
                                        && note.severity == NoticeSeverity::Error
                                        && note.api_call_id.is_none()
                            )
                    })
                    .map(|event| event.seq);
                // A pre-API boundary without a system error is an expected stop/completion, not a missing failure projection.
                // 不含系统错误的 API 前边界属于预期停止或完成，而不是缺失失败投影。
                let projection = notice_seq.map_or(
                    ContinuationTerminalProjection::NoNoticeRequired,
                    |notice_seq| ContinuationTerminalProjection::InlineNotice { notice_seq },
                );
                ContinuationDispatchResolution::TerminalBeforeApi {
                    requested_seq,
                    resumed_seq,
                    terminal_seq,
                    projection,
                }
            }));
        };
        let mut observed_seq = None;
        for event in self
            .events
            .iter()
            .take_while(|event| event.seq <= through_seq)
        {
            let EventBody::ExecutionControl(control) = &event.event else {
                continue;
            };
            if control.control_kind != ExecutionControlKind::ApiTransportObserved
                || control.api_call_id != Some(prepared.api_call_id)
            {
                continue;
            }
            if observed_seq.replace(event.seq).is_some() {
                return Err(DomainError::InvalidContinuationHistory {
                    field: "duplicate_api_transport_observed",
                });
            }
        }
        if let Some(observed_seq) = observed_seq {
            let observed = ApiTransportObservedReceipt {
                api_call_id: prepared.api_call_id,
                started_seq: prepared.started_seq,
                observed_seq,
                continuation_action_id: prepared.continuation_action_id,
            };
            return Ok(Some(ContinuationDispatchResolution::TransportObserved(
                observed,
            )));
        }
        let mut terminal = None;
        for event in self
            .events
            .iter()
            .take_while(|event| event.seq <= through_seq)
        {
            let EventBody::ApiStats(stats) = &event.event else {
                continue;
            };
            if stats.api_call_id != prepared.api_call_id {
                continue;
            }
            if terminal.replace((event.seq, stats.status)).is_some() {
                return Err(DomainError::InvalidContinuationHistory {
                    field: "duplicate_continue_api_terminal",
                });
            }
        }
        Ok(Some(match terminal {
            Some((terminal_seq, terminal_status)) => {
                // noticeSeq is the exact event immediately following ApiStats in the atomic terminal transaction.
                // noticeSeq 是原子终态事务中紧随 ApiStats 的精确事件。
                let notice_seq = self
                    .events
                    .iter()
                    .take_while(|event| event.seq <= through_seq)
                    .find(|event| event.seq > terminal_seq)
                    .filter(|event| {
                        matches!(
                            &event.event,
                            EventBody::DisplayNote(note)
                                if note.api_call_id == Some(prepared.api_call_id)
                        )
                    })
                    .map(|event| event.seq);
                // Failed terminals require a durable inline notice; expected terminal states remain silent.
                // 失败终态必须拥有可靠行内提示；预期终态保持静默。
                let projection = match notice_seq {
                    Some(notice_seq) => ContinuationTerminalProjection::InlineNotice { notice_seq },
                    None if terminal_status == ApiCallStatus::Failed => {
                        ContinuationTerminalProjection::MissingNotice
                    }
                    None => ContinuationTerminalProjection::NoNoticeRequired,
                };
                ContinuationDispatchResolution::TerminalBeforeTransport {
                    api_call_id: prepared.api_call_id,
                    started_seq: prepared.started_seq,
                    terminal_seq,
                    projection,
                }
            }
            None => ContinuationDispatchResolution::AwaitingTransport {
                api_call_id: prepared.api_call_id,
                started_seq: prepared.started_seq,
            },
        }))
    }

    /// Finds the durable API binding of one Continue action.
    /// 查找一次 Continue 动作的持久化 API 绑定。
    fn existing_api_start_for_action(
        &self,
        action_id: &ClientActionId,
    ) -> Result<Option<PreparedApiStart>, DomainError> {
        let mut found = None;
        for event in &self.events {
            let EventBody::ApiStarted(started) = &event.event else {
                continue;
            };
            if started.continuation_action_id.as_ref() != Some(action_id) {
                continue;
            }
            let receipt = PreparedApiStart {
                api_call_id: started.api_call_id,
                started_seq: event.seq,
                projection: started.projection.clone(),
                continuation_action_id: Some(action_id.clone()),
            };
            if found.replace(receipt).is_some() {
                return Err(DomainError::InvalidContinuationHistory {
                    field: "duplicate_continue_api_binding",
                });
            }
        }
        Ok(found)
    }

    /// Validates that one unbound Continue action belongs to the open Turn.
    /// 校验一条尚未绑定的 Continue 动作属于当前开放回合。
    fn validate_continuation_action_for_api(
        &self,
        action_id: &ClientActionId,
        turn_id: TurnId,
    ) -> Result<(), DomainError> {
        let mut requested = None;
        let mut resumed = None;
        for event in &self.events {
            let EventBody::ExecutionControl(control) = &event.event else {
                continue;
            };
            if control.client_action_id.as_ref() != Some(action_id) {
                continue;
            }
            match control.control_kind {
                ExecutionControlKind::TurnContinueRequested => {
                    if requested.replace(event).is_some() {
                        return Err(DomainError::InvalidContinuationHistory {
                            field: "duplicate_continue_requested",
                        });
                    }
                }
                ExecutionControlKind::TurnResumed if resumed.replace(event).is_some() => {
                    return Err(DomainError::InvalidContinuationHistory {
                        field: "duplicate_turn_resumed",
                    });
                }
                ExecutionControlKind::TurnResumed => {}
                _ => {}
            }
        }
        let requested = requested.ok_or(DomainError::InvalidContinuationHistory {
            field: "continue_api.missing_requested",
        })?;
        let resumed = resumed.ok_or(DomainError::InvalidContinuationHistory {
            field: "continue_api.missing_resumed",
        })?;
        if requested.turn_id != Some(turn_id)
            || resumed.turn_id != Some(turn_id)
            || requested.seq.value() >= resumed.seq.value()
        {
            return Err(DomainError::InvalidContinuationHistory {
                field: "continue_api.turn_or_order",
            });
        }
        Ok(())
    }

    /// Appends or updates the displayable reasoning semantic of one API.
    /// 追加或更新一次 API 的可显示推理语义。
    ///
    /// The first call allocates a new seq; later calls update the same event
    /// in place and never allocate a new seq.
    /// 首次调用分配新 seq；后续调用原位更新同一事件，绝不分配新 seq。
    pub fn append_reasoning(
        &mut self,
        api_call_id: ApiCallId,
        text: impl Into<String>,
        status: AssistantSemanticStatus,
        now_ms: i64,
    ) -> Result<IdempotentMutation<Seq>, DomainError> {
        let mut api = self.require_active_api(api_call_id)?;
        if api.text_seq.is_some() || !api.tool_seqs.is_empty() || api.stats_seq.is_some() {
            return Err(DomainError::SequenceViolation {
                expected: 0,
                actual: api_call_id.value(),
            });
        }
        let text = text.into();
        let mutation = match api.reasoning_seq {
            Some(seq) => self.update_provider_semantic(
                seq,
                now_ms,
                EventBody::AssistantReasoning(AssistantReasoningEvent {
                    api_call_id,
                    text,
                    status,
                    provider_source: None,
                }),
            )?,
            None => {
                let seq = self.append_event(
                    Some(api.turn_id),
                    EventVisibility::Provider,
                    EventBody::AssistantReasoning(AssistantReasoningEvent {
                        api_call_id,
                        text,
                        status,
                        provider_source: None,
                    }),
                    now_ms,
                )?;
                api.reasoning_seq = Some(seq);
                IdempotentMutation::Committed(seq)
            }
        };
        self.save_active_api(api);
        Ok(mutation)
    }

    /// Appends or updates the assistant text semantic of one API.
    /// 追加或更新一次 API 的 assistant 正文语义。
    pub fn append_assistant_text(
        &mut self,
        api_call_id: ApiCallId,
        text: impl Into<String>,
        status: AssistantSemanticStatus,
        now_ms: i64,
    ) -> Result<IdempotentMutation<Seq>, DomainError> {
        let mut api = self.require_active_api(api_call_id)?;
        if !api.tool_seqs.is_empty() || api.stats_seq.is_some() {
            return Err(DomainError::SequenceViolation {
                expected: 0,
                actual: api_call_id.value(),
            });
        }
        let text = text.into();
        let mutation = match api.text_seq {
            Some(seq) => self.update_provider_semantic(
                seq,
                now_ms,
                EventBody::AssistantText(AssistantTextEvent {
                    api_call_id,
                    text,
                    status,
                    provider_source: None,
                }),
            )?,
            None => {
                let seq = self.append_event(
                    Some(api.turn_id),
                    EventVisibility::Provider,
                    EventBody::AssistantText(AssistantTextEvent {
                        api_call_id,
                        text,
                        status,
                        provider_source: None,
                    }),
                    now_ms,
                )?;
                api.text_seq = Some(seq);
                IdempotentMutation::Committed(seq)
            }
        };
        self.save_active_api(api);
        Ok(mutation)
    }

    /// Completes one physical API group with its terminal statistic.
    /// 用终态统计闭合一次物理 API 组。
    ///
    /// `provider_source` may only be attached to a group that produced at
    /// least one Provider-sourced event; it is placed exactly once on the last
    /// Provider-sourced event of the group.
    /// `provider_source` 只允许附着到至少产生过一个 Provider 来源事件的组，
    /// 并且恰好放置一次在组内最后一个 Provider 来源事件上。
    pub fn complete_api_group(
        &mut self,
        api_call_id: ApiCallId,
        completion: ApiCompletion,
    ) -> Result<ApiGroupCompletionReceipt, DomainError> {
        let mut terminal_timing = completion.timing.clone();
        if terminal_timing.completed_at_ms.is_none() {
            terminal_timing.completed_at_ms = Some(completion.completed_at_ms);
        }
        let terminal_completed_at_ms = validate_api_terminal_facts(
            completion.status,
            completion.end_reason,
            completion.error.as_ref(),
            completion.continuation_eligibility,
            &terminal_timing,
        )?;
        if terminal_completed_at_ms != completion.completed_at_ms {
            return Err(DomainError::InvalidApiTerminalFacts {
                field: "completed_at_mismatch",
            });
        }
        let mut api = self.require_active_api(api_call_id)?;
        let failure_notice = completion.end_reason.and_then(api_failure_notice);
        let redacted_detail = completion.error.as_ref().map(|error| error.message.clone());
        let pending_inputs = self.metadata.input_state.pending_in_turn_inputs.clone();
        let had_pending_inputs = !pending_inputs.is_empty();
        let bound_request_config = pending_inputs
            .iter()
            .rev()
            .find_map(|pending| pending.bound_request_config.clone());
        for pending in &pending_inputs {
            validate_user_input(pending.text.clone(), &pending.resource_ids)?;
            if let Some(parameters) = &pending.parameters {
                parameters.validate_pending(&pending.resource_ids)?;
            }
            if pending.turn_id != api.turn_id {
                return Err(DomainError::InvalidQueuedInputOrder {
                    field: "pending_input_turn_mismatch",
                });
            }
        }
        let promotable_count = pending_inputs
            .iter()
            .filter(|pending| !self.input_id_already_streamed(pending.input_id))
            .count();
        let promotable_count = u64::try_from(promotable_count)
            .map_err(|_| DomainError::invalid_id("pending_input_count", u64::MAX))?;
        self.ensure_transition_capacity(
            1 + u64::from(failure_notice.is_some())
                + promotable_count
                + u64::from(bound_request_config.is_some()),
            0,
        )?;
        self.validate_active_api_events(&api)?;
        ensure_api_request_field(
            "provider",
            &api.request.provider,
            &completion.request.provider,
        )?;
        ensure_api_request_field("profile", &api.request.profile, &completion.request.profile)?;
        ensure_api_request_field("model", &api.request.model, &completion.request.model)?;
        ensure_api_request_field(
            "connection_id",
            &api.request.connection_id,
            &completion.request.connection_id,
        )?;
        ensure_api_request_field(
            "thinking_level",
            &api.request.thinking_level,
            &completion.request.thinking_level,
        )?;
        ensure_api_request_field(
            "temperature",
            &api.request.temperature,
            &completion.request.temperature,
        )?;
        let source_to_attach = if let Some(source_input) = &completion.provider_source {
            validate_provider_source_input(&api, source_input)?;
            let last_provider_seq =
                api.last_provider_sourced_seq()
                    .ok_or_else(|| DomainError::ReferenceViolation {
                        kind: "provider_source",
                        value: api_call_id.value(),
                    })?;
            let expected_source =
                build_provider_source(&self.events, last_provider_seq, source_input)?;
            if api.provider_source_placed {
                let actual_source = provider_source_at_seq(
                    &self.events,
                    self.resident_start_seq,
                    last_provider_seq,
                )?;
                ensure_provider_source_matches(&expected_source, &actual_source)?;
                None
            } else {
                Some((last_provider_seq, expected_source))
            }
        } else if api.provider_source_placed {
            return Err(DomainError::ReferenceViolation {
                kind: "provider_source_terminal",
                value: api_call_id.value(),
            });
        } else {
            None
        };
        let mut staged_event_updates = BTreeMap::new();
        if completion.status != ApiCallStatus::Completed {
            for seq in api.reasoning_seq.iter().copied() {
                self.stage_active_event_interruption(
                    &mut staged_event_updates,
                    seq,
                    api_call_id,
                    completion.completed_at_ms,
                )?;
            }
            for seq in api.text_seq.iter().copied() {
                self.stage_active_event_interruption(
                    &mut staged_event_updates,
                    seq,
                    api_call_id,
                    completion.completed_at_ms,
                )?;
            }
            for seq in api.tool_seqs.iter().copied() {
                self.stage_active_event_interruption(
                    &mut staged_event_updates,
                    seq,
                    api_call_id,
                    completion.completed_at_ms,
                )?;
            }
        }
        self.ensure_staged_active_api_tools_terminal(&api, &staged_event_updates)?;
        if let Some((last_provider_seq, source)) = source_to_attach {
            self.stage_provider_source_attachment(
                &mut staged_event_updates,
                last_provider_seq,
                source,
                completion.completed_at_ms,
            )?;
            api.provider_source_placed = true;
        }
        let mut staged_allocator = self.id_allocator;
        let mut staged_events = Vec::with_capacity(
            1 + usize::from(failure_notice.is_some()) + promotable_count as usize,
        );
        let mut staged_metadata = self.metadata.clone();
        let seq = self.stage_event(
            &mut staged_allocator,
            &mut staged_events,
            Some(api.turn_id),
            EventVisibility::UiOnly,
            EventBody::ApiStats(ApiStatsEvent {
                api_call_id,
                purpose: api.purpose,
                provider: completion.request.provider.clone(),
                profile: completion.request.profile.clone(),
                model: completion.request.model.clone(),
                connection_id: completion.request.connection_id.clone(),
                thinking_level: completion.request.thinking_level.clone(),
                temperature: completion.request.temperature,
                usage: completion.usage.clone(),
                status: completion.status,
                end_reason: completion.end_reason,
                error: completion.error.clone(),
                continuation_eligibility: completion.continuation_eligibility,
                timing: terminal_timing,
            }),
            completion.completed_at_ms,
        )?;
        api.stats_seq = Some(seq);
        if let Some((notice_kind, severity, reason_code)) = failure_notice {
            self.stage_event(
                &mut staged_allocator,
                &mut staged_events,
                Some(api.turn_id),
                EventVisibility::UiOnly,
                EventBody::DisplayNote(DisplayNoteEvent {
                    notice_kind,
                    severity,
                    reason_code: reason_code.to_string(),
                    redacted_detail,
                    api_call_id: Some(api_call_id),
                    retry_count: None,
                    retry_limit: None,
                }),
                completion.completed_at_ms,
            )?;
        }
        if let Some(request_config) = bound_request_config {
            self.stage_bound_request_activation(
                &mut staged_metadata,
                &mut staged_allocator,
                &mut staged_events,
                request_config,
                Some(api.turn_id),
                completion.completed_at_ms,
            )?;
        }
        let mut promoted_input_seqs = Vec::new();
        for pending in pending_inputs {
            if self.input_id_already_streamed(pending.input_id) {
                continue;
            }
            let promoted_seq = self.stage_event(
                &mut staged_allocator,
                &mut staged_events,
                Some(api.turn_id),
                EventVisibility::Provider,
                EventBody::UserInput(UserInputEvent {
                    parameters: pending.parameters,
                    client_action_id: pending.client_action_id,
                    input_id: pending.input_id,
                    input_source: UserInputSource::PendingInTurn,
                    text: pending.text,
                    resource_ids: pending.resource_ids,
                }),
                completion.completed_at_ms,
            )?;
            promoted_input_seqs.push(promoted_seq);
        }
        staged_metadata.input_state.pending_in_turn_inputs.clear();
        let mut changed_event_seqs: Vec<Seq> = staged_event_updates
            .values()
            .map(ConversationEvent::seq)
            .collect();
        changed_event_seqs.extend(staged_events.iter().map(ConversationEvent::seq));
        for (index, event) in staged_event_updates {
            self.events[index] = event;
        }
        self.id_allocator = staged_allocator;
        self.metadata = staged_metadata;
        self.events.extend(staged_events);
        self.active_api = None;
        let dirty_files: &[DirtyFile] = if had_pending_inputs {
            &[DirtyFile::Metadata, DirtyFile::ActiveTail]
        } else {
            &[DirtyFile::ActiveTail]
        };
        self.bump_revision(dirty_files, completion.completed_at_ms, &changed_event_seqs);
        Ok(ApiGroupCompletionReceipt {
            api_call_id,
            turn_id: api.turn_id,
            stats_seq: seq,
            promoted_input_seqs,
        })
    }

    /// Interrupts the active physical API while preserving its logical turn.
    /// 中断活动物理 API，同时保留其所属逻辑回合。
    ///
    /// Streaming semantics and every non-terminal tool become interrupted
    /// before the terminal API statistic is published. The method records the
    /// explicit execution-control fact and one terminal
    /// `ApiStats(Interrupted, UserInterrupted)` event, then clears only the
    /// active API. The open turn remains available to `continue_turn`.
    /// 在发布 API 终态统计前，流式语义与每个非终态工具都会转为 interrupted。
    /// 该方法记录显式执行控制事实与一条终态
    /// `ApiStats(Interrupted, UserInterrupted)` 事件，随后只清除活动 API；开放
    /// 回合仍可供 `continue_turn` 继承。
    pub fn interrupt_active_api(&mut self, now_ms: i64) -> Result<Seq, DomainError> {
        let (_, completion) =
            self.interrupt_active_api_state(now_ms, ApiEndReason::UserInterrupted, true)?;
        Ok(completion.stats_seq)
    }

    /// Interrupts an active physical API because its owning process stopped unexpectedly.
    /// 因持有该物理 API 的进程意外停止而中断活动 API。
    ///
    /// # Parameters
    /// 参数
    /// `now_ms` is the exact runtime boundary timestamp used for every interrupted semantic,
    /// tool terminal, and the resulting API statistics event.
    /// `now_ms` 是用于所有被中断语义、工具终态及最终 API 统计事件的精确运行时边界时间戳。
    ///
    /// # Returns
    /// 返回值
    /// Returns the terminal statistics seq when an active API was interrupted. A prepared
    /// Continue API that transport never observed remains dispatchable and returns `None`.
    /// 当活动 API 被中断时返回终态统计 seq；尚未被 transport 观察的已准备 Continue API
    /// 保持可分派状态并返回 `None`。
    pub fn interrupt_active_api_for_process(
        &mut self,
        now_ms: i64,
    ) -> Result<Option<Seq>, DomainError> {
        let Some(active_api) = self.active_api.as_ref() else {
            return Ok(None);
        };
        if active_api.purpose == ApiPurposeKind::Continue
            && !self.api_transport_was_observed(active_api.api_call_id)
        {
            return Ok(None);
        }
        let (_, completion) =
            self.interrupt_active_api_state(now_ms, ApiEndReason::ProcessInterrupted, false)?;
        Ok(Some(completion.stats_seq))
    }

    /// Returns whether transport durably observed one physical API request.
    /// 返回 transport 是否已可靠观察到指定物理 API 请求。
    ///
    /// # Parameters
    /// 参数
    /// `api_call_id` identifies the exact physical API request.
    /// `api_call_id` 标识精确的物理 API 请求。
    ///
    /// # Returns
    /// 返回值
    /// Returns `true` only when the matching durable execution-control fact exists.
    /// 仅当存在匹配的可靠执行控制事实时返回 `true`。
    pub(super) fn api_transport_was_observed(&self, api_call_id: ApiCallId) -> bool {
        self.events.iter().any(|event| {
            matches!(
                &event.event,
                EventBody::ExecutionControl(control)
                    if control.control_kind == ExecutionControlKind::ApiTransportObserved
                        && control.api_call_id == Some(api_call_id)
            )
        })
    }

    /// Applies the shared active-API interruption transition.
    /// 应用共享的活动 API 中断转换。
    pub(super) fn interrupt_active_api_state(
        &mut self,
        now_ms: i64,
        end_reason: ApiEndReason,
        record_execution_control: bool,
    ) -> Result<(u32, ApiGroupCompletionReceipt), DomainError> {
        let api = self.active_api.clone().ok_or(DomainError::NoActiveApi)?;
        let pending_inputs = self.metadata.input_state.pending_in_turn_inputs.clone();
        let bound_request_config = pending_inputs
            .iter()
            .rev()
            .find_map(|pending| pending.bound_request_config.clone());
        for pending in &pending_inputs {
            validate_user_input(pending.text.clone(), &pending.resource_ids)?;
            if let Some(parameters) = &pending.parameters {
                parameters.validate_pending(&pending.resource_ids)?;
            }
        }
        let promotable_count = pending_inputs
            .iter()
            .filter(|pending| !self.input_id_already_streamed(pending.input_id))
            .count();
        let promotable_count = u64::try_from(promotable_count)
            .map_err(|_| DomainError::invalid_id("pending_input_count", u64::MAX))?;
        let appended_event_count = u64::from(if record_execution_control { 2u8 } else { 1u8 })
            .checked_add(promotable_count)
            .and_then(|count| count.checked_add(u64::from(bound_request_config.is_some())))
            .ok_or_else(|| DomainError::invalid_id("event_count", u64::MAX))?;
        self.ensure_transition_capacity(appended_event_count, 0)?;
        self.validate_active_api_events(&api)?;

        let mut projected = 0u32;
        let mut staged_event_updates = BTreeMap::new();
        for seq in [api.reasoning_seq, api.text_seq].into_iter().flatten() {
            projected += u32::from(self.stage_active_event_interruption(
                &mut staged_event_updates,
                seq,
                api.api_call_id,
                now_ms,
            )?);
        }
        for seq in api.tool_seqs.iter().copied() {
            projected += u32::from(self.stage_active_event_interruption(
                &mut staged_event_updates,
                seq,
                api.api_call_id,
                now_ms,
            )?);
        }
        self.ensure_staged_active_api_tools_terminal(&api, &staged_event_updates)?;
        let mut staged_allocator = self.id_allocator;
        let staged_capacity = usize::try_from(appended_event_count)
            .map_err(|_| DomainError::invalid_id("event_count", appended_event_count))?;
        let mut staged_events = Vec::with_capacity(staged_capacity);
        let mut staged_metadata = self.metadata.clone();
        if record_execution_control {
            self.stage_event(
                &mut staged_allocator,
                &mut staged_events,
                Some(api.turn_id),
                EventVisibility::HostOnly,
                EventBody::ExecutionControl(ExecutionControlEvent {
                    control_kind: ExecutionControlKind::ExecutionInterrupted,
                    api_call_id: Some(api.api_call_id),
                    client_action_id: None,
                }),
                now_ms,
            )?;
        }
        let request = api.request;
        let stats_seq = self.stage_event(
            &mut staged_allocator,
            &mut staged_events,
            Some(api.turn_id),
            EventVisibility::UiOnly,
            EventBody::ApiStats(ApiStatsEvent {
                api_call_id: api.api_call_id,
                purpose: api.purpose,
                provider: request.provider,
                profile: request.profile,
                model: request.model,
                connection_id: request.connection_id,
                thinking_level: request.thinking_level,
                temperature: request.temperature,
                usage: UsageSnapshot::default(),
                status: ApiCallStatus::Interrupted,
                end_reason: Some(end_reason),
                error: None,
                continuation_eligibility: ContinuationEligibility::Allowed,
                timing: ApiTimings {
                    completed_at_ms: Some(now_ms),
                    ..ApiTimings::default()
                },
            }),
            now_ms,
        )?;
        if let Some(request_config) = bound_request_config {
            self.stage_bound_request_activation(
                &mut staged_metadata,
                &mut staged_allocator,
                &mut staged_events,
                request_config,
                Some(api.turn_id),
                now_ms,
            )?;
        }
        let mut promoted_input_seqs = Vec::with_capacity(promotable_count as usize);
        for pending in pending_inputs.iter().cloned() {
            if self.input_id_already_streamed(pending.input_id) {
                continue;
            }
            let promoted_seq = self.stage_event(
                &mut staged_allocator,
                &mut staged_events,
                Some(api.turn_id),
                EventVisibility::Provider,
                EventBody::UserInput(UserInputEvent {
                    parameters: pending.parameters,
                    client_action_id: pending.client_action_id,
                    input_id: pending.input_id,
                    input_source: UserInputSource::PendingInTurn,
                    text: pending.text,
                    resource_ids: pending.resource_ids,
                }),
                now_ms,
            )?;
            promoted_input_seqs.push(promoted_seq);
        }
        let had_pending_inputs = !pending_inputs.is_empty();
        staged_metadata.input_state.pending_in_turn_inputs.clear();
        let mut changed_event_seqs: Vec<Seq> = staged_event_updates
            .values()
            .map(ConversationEvent::seq)
            .collect();
        changed_event_seqs.extend(staged_events.iter().map(ConversationEvent::seq));
        for (index, event) in staged_event_updates {
            self.events[index] = event;
        }
        self.id_allocator = staged_allocator;
        self.metadata = staged_metadata;
        self.events.extend(staged_events);
        self.active_api = None;
        let dirty_files: &[DirtyFile] = if had_pending_inputs {
            &[DirtyFile::Metadata, DirtyFile::ActiveTail]
        } else {
            &[DirtyFile::ActiveTail]
        };
        self.bump_revision(dirty_files, now_ms, &changed_event_seqs);
        let projected_input_count = u32::try_from(promoted_input_seqs.len())
            .map_err(|_| DomainError::invalid_id("pending_input_count", u64::MAX))?;
        Ok((
            projected
                .checked_add(1)
                .and_then(|count| count.checked_add(projected_input_count))
                .ok_or_else(|| DomainError::invalid_id("projection_count", u64::MAX))?,
            ApiGroupCompletionReceipt {
                api_call_id: api.api_call_id,
                turn_id: api.turn_id,
                stats_seq,
                promoted_input_seqs,
            },
        ))
    }

    /// Stages interruption of one active API-owned semantic or running tool.
    /// 暂存一条属于活动 API 的语义或运行中工具中断。
    pub(super) fn stage_active_event_interruption(
        &self,
        staged_event_updates: &mut BTreeMap<usize, ConversationEvent>,
        seq: Seq,
        api_call_id: ApiCallId,
        now_ms: i64,
    ) -> Result<bool, DomainError> {
        let index = seq.value().checked_sub(self.resident_start_seq).ok_or(
            DomainError::ImmutableFieldMutation {
                entity: "events",
                field: "sealed_segment",
            },
        )? as usize;
        let mut event = staged_event_updates
            .get(&index)
            .cloned()
            .or_else(|| self.events.get(index).cloned())
            .ok_or_else(|| DomainError::reference("events", seq.value()))?;
        let changed = match &event.event {
            EventBody::AssistantReasoning(reasoning) => {
                ensure_api_event_owner(seq, api_call_id, reasoning.api_call_id)?;
                reasoning.status == AssistantSemanticStatus::Streaming
            }
            EventBody::AssistantText(text) => {
                ensure_api_event_owner(seq, api_call_id, text.api_call_id)?;
                text.status == AssistantSemanticStatus::Streaming
            }
            EventBody::ToolCall(tool) => {
                ensure_api_event_owner(seq, api_call_id, tool.api_call_id)?;
                matches!(
                    tool.status,
                    ToolCallStatus::Prepared
                        | ToolCallStatus::Running
                        | ToolCallStatus::WaitingUser
                )
            }
            _ => {
                return Err(DomainError::ImmutableFieldMutation {
                    entity: "events",
                    field: "active_api_event",
                });
            }
        };
        if changed {
            event.mutate_in_place(now_ms, |body| match body {
                EventBody::AssistantReasoning(reasoning) => {
                    reasoning.status = AssistantSemanticStatus::Interrupted;
                }
                EventBody::AssistantText(text) => {
                    text.status = AssistantSemanticStatus::Interrupted;
                }
                EventBody::ToolCall(tool) => {
                    tool.status = ToolCallStatus::Interrupted;
                    tool.output = Some(ToolPayload::terminal_failure(
                        ToolTerminalErrorKind::Interrupted,
                    ));
                    tool.finished_at_ms = Some(now_ms);
                }
                _ => unreachable!("active API event kind was validated before mutation"),
            })?;
            staged_event_updates.insert(index, event);
        }
        Ok(changed)
    }

    /// Stages one Provider source attachment without publishing the target event.
    /// 暂存一次 Provider 来源附着，不发布目标事件。
    fn stage_provider_source_attachment(
        &self,
        staged_event_updates: &mut BTreeMap<usize, ConversationEvent>,
        seq: Seq,
        source: ProviderSource,
        now_ms: i64,
    ) -> Result<(), DomainError> {
        let index = seq.value().checked_sub(self.resident_start_seq).ok_or(
            DomainError::ImmutableFieldMutation {
                entity: "events",
                field: "sealed_segment",
            },
        )? as usize;
        let mut event = staged_event_updates
            .get(&index)
            .cloned()
            .or_else(|| self.events.get(index).cloned())
            .ok_or_else(|| DomainError::reference("events", seq.value()))?;
        let target = match &event.event {
            EventBody::AssistantReasoning(reasoning) => &reasoning.provider_source,
            EventBody::AssistantText(text) => &text.provider_source,
            EventBody::ToolCall(tool) => &tool.provider_source,
            _ => {
                return Err(DomainError::ImmutableFieldMutation {
                    entity: "events",
                    field: "provider_source",
                });
            }
        };
        if target.is_some() {
            return Err(DomainError::ProviderSourceAlreadySet {
                api_call_id: source.covered_until_seq.value(),
            });
        }
        event.mutate_in_place(now_ms, |body| match body {
            EventBody::AssistantReasoning(reasoning) => {
                reasoning.provider_source = Some(source);
            }
            EventBody::AssistantText(text) => {
                text.provider_source = Some(source);
            }
            EventBody::ToolCall(tool) => {
                tool.provider_source = Some(source);
            }
            _ => unreachable!("provider source target was validated before mutation"),
        })?;
        staged_event_updates.insert(index, event);
        Ok(())
    }

    /// Requires every staged or persisted tool in one API group to be terminal.
    /// 要求一个 API 组中每个暂存或已持久化工具均为终态。
    fn ensure_staged_active_api_tools_terminal(
        &self,
        api: &ActiveApiState,
        staged_event_updates: &BTreeMap<usize, ConversationEvent>,
    ) -> Result<(), DomainError> {
        for seq in api.tool_seqs.iter().copied() {
            let index = seq.value().checked_sub(self.resident_start_seq).ok_or(
                DomainError::ImmutableFieldMutation {
                    entity: "events",
                    field: "sealed_segment",
                },
            )? as usize;
            let event = staged_event_updates
                .get(&index)
                .or_else(|| self.events.get(index))
                .ok_or_else(|| DomainError::reference("events", seq.value()))?;
            let EventBody::ToolCall(tool) = &event.event else {
                return Err(DomainError::ImmutableFieldMutation {
                    entity: "events",
                    field: "tool_call",
                });
            };
            if !matches!(
                tool.status,
                ToolCallStatus::Completed
                    | ToolCallStatus::Failed
                    | ToolCallStatus::Cancelled
                    | ToolCallStatus::Stopped
                    | ToolCallStatus::Interrupted
            ) {
                return Err(DomainError::ToolStatusMismatch {
                    tool_call_id: tool.tool_call_id.value(),
                    expected: "terminal",
                    actual: tool_status_name(tool.status),
                });
            }
            if tool.output.is_none() {
                return Err(DomainError::MissingTerminalToolOutput {
                    tool_call_id: tool.tool_call_id.value(),
                    status: tool_status_name(tool.status),
                });
            }
        }
        Ok(())
    }

    /// Validates every resident event referenced by the active API before an
    /// all-or-nothing interruption starts mutating the document.
    /// 在全有或全无中断开始修改文档前，校验活动 API 引用的每条常驻事件。
    pub(super) fn validate_active_api_events(
        &self,
        api: &ActiveApiState,
    ) -> Result<(), DomainError> {
        if let Some(seq) = api.reasoning_seq {
            self.validate_active_event(seq, api.api_call_id, ActiveApiEventKind::Reasoning)?;
        }
        if let Some(seq) = api.text_seq {
            self.validate_active_event(seq, api.api_call_id, ActiveApiEventKind::Text)?;
        }
        for seq in api.tool_seqs.iter().copied() {
            self.validate_active_event(seq, api.api_call_id, ActiveApiEventKind::Tool)?;
        }
        Ok(())
    }

    /// Validates the kind and API owner of one resident active event.
    /// 校验一条常驻活动事件的种类与 API 所有者。
    fn validate_active_event(
        &self,
        seq: Seq,
        api_call_id: ApiCallId,
        expected_kind: ActiveApiEventKind,
    ) -> Result<(), DomainError> {
        let index = seq.value().checked_sub(self.resident_start_seq).ok_or(
            DomainError::ImmutableFieldMutation {
                entity: "events",
                field: "sealed_segment",
            },
        )? as usize;
        let event = self
            .events
            .get(index)
            .ok_or_else(|| DomainError::reference("events", seq.value()))?;
        let actual_api_call_id = match (&event.event, expected_kind) {
            (EventBody::AssistantReasoning(reasoning), ActiveApiEventKind::Reasoning) => {
                reasoning.api_call_id
            }
            (EventBody::AssistantText(text), ActiveApiEventKind::Text) => text.api_call_id,
            (EventBody::ToolCall(tool), ActiveApiEventKind::Tool) => tool.api_call_id,
            _ => {
                return Err(DomainError::ImmutableFieldMutation {
                    entity: "events",
                    field: "active_api_event_kind",
                });
            }
        };
        ensure_api_event_owner(seq, api_call_id, actual_api_call_id)
    }

    /// Replaces the streaming text of an existing provider semantic in place.
    /// 原位替换既有 Provider 语义的流式正文。
    fn update_provider_semantic(
        &mut self,
        seq: Seq,
        now_ms: i64,
        replacement: EventBody,
    ) -> Result<IdempotentMutation<Seq>, DomainError> {
        // Resolve the resident event without mutating it so every validation failure is atomic.
        // 先只读解析常驻事件，确保任何校验失败都不会留下部分修改。
        let index = seq.value().checked_sub(self.resident_start_seq).ok_or(
            DomainError::ImmutableFieldMutation {
                entity: "events",
                field: "sealed_segment",
            },
        )? as usize;
        // Decide whether the cumulative snapshot is a real monotonic evolution or an exact replay.
        // 判定累计快照是真实单调演进还是完全相同的重放。
        let event = self
            .events
            .get(index)
            .ok_or_else(|| DomainError::reference("events", seq.value()))?;
        let changed = match (&event.event, &replacement) {
            (EventBody::AssistantReasoning(current), EventBody::AssistantReasoning(next)) => {
                ensure_api_event_owner(seq, next.api_call_id, current.api_call_id)?;
                validate_provider_semantic_snapshot_transition(
                    &current.text,
                    current.status,
                    current.provider_source.as_ref(),
                    &next.text,
                    next.status,
                    next.provider_source.as_ref(),
                )?
            }
            (EventBody::AssistantText(current), EventBody::AssistantText(next)) => {
                ensure_api_event_owner(seq, next.api_call_id, current.api_call_id)?;
                validate_provider_semantic_snapshot_transition(
                    &current.text,
                    current.status,
                    current.provider_source.as_ref(),
                    &next.text,
                    next.status,
                    next.provider_source.as_ref(),
                )?
            }
            _ => {
                return Err(DomainError::ImmutableFieldMutation {
                    entity: "events",
                    field: "streaming_kind",
                });
            }
        };
        if !changed {
            return Ok(IdempotentMutation::Idempotent(seq));
        }
        // Publish the already-validated replacement as one in-place event revision.
        // 将已经通过校验的替换值作为一次原位事件修订发布。
        let event = self
            .events
            .get_mut(index)
            .ok_or_else(|| DomainError::reference("events", seq.value()))?;
        event.mutate_in_place(now_ms, |body| *body = replacement)?;
        self.bump_revision(&[DirtyFile::ActiveTail], now_ms, &[seq]);
        Ok(IdempotentMutation::Committed(seq))
    }
}
