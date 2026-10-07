//! Session-owned physical-attempt diagnostics and raw Provider trace persistence.
//! 会话持有的物理尝试诊断与 Provider 原始追踪持久化。

use super::{VulcanPublicContractAdapter, provider_failure};
use base64::Engine;
use futures_util::StreamExt;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};
use vulcan_ai_contract::AiStreamEvent;
use vulcan_core::provider::VulcanProviderAdapter;
use vulcan_core::types::CoreError as VulcanCoreError;
use vulcan_provider_adapters::{
    ProviderExecutionRoute, ProviderExecutionStream, ProviderWireRequestCapture,
    ProviderWireRequestTraceDispatch, ProviderWireResponseCapture,
};
use vulcan_session_core::v1::ApiCallId;
use vulcan_workspace_store::workspace_store::{
    ProviderApiTraceAttemptReservation, ProviderApiTracePayload,
    ProviderApiTraceReservationOutcome, ProviderApiTraceStore, ProviderApiTraceWriteOutcome,
};

/// Selects the response evidence persisted for one trace-attempt snapshot.
/// 选择一次追踪尝试快照要持久化的响应证据。
pub(super) enum ProviderApiTraceResponseState {
    /// The response capture channel closed without publishing a capture.
    /// 响应捕获通道关闭且没有发布捕获结果。
    CaptureChannelClosed,
    /// A partial or complete raw response capture is available.
    /// 已取得部分或完整的原始响应捕获。
    Captured(ProviderWireResponseCapture),
}

/// Describes the first host-side processing failure observed after raw Provider transport capture.
/// 描述 Provider 原始传输捕获之后观察到的首个宿主侧处理失败。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ProviderApiTraceProcessingFailure {
    /// Stable pipeline boundary used by `outcome.json` diagnostics.
    /// `outcome.json` 诊断使用的稳定流水线边界。
    stage: String,
    /// Controlled error text that excludes credentials and raw request payloads.
    /// 不包含凭据与原始请求载荷的受控错误文本。
    message: String,
}

/// Publishes at most one post-transport processing failure to the detached trace writer.
/// 向独立追踪写入任务至多发布一次传输后处理失败。
pub(super) struct ProviderApiTraceProcessingFailureSender {
    /// One-shot channel retained until successful completion or the first processing failure.
    /// 保留到成功完成或首次处理失败的一次性通道。
    sender: Option<tokio::sync::oneshot::Sender<ProviderApiTraceProcessingFailure>>,
}

impl ProviderApiTraceProcessingFailureSender {
    /// Creates a failure publisher for one traced physical Provider attempt.
    /// 为一次已追踪的真实 Provider 尝试创建失败发布器。
    ///
    /// # Parameters
    /// 参数
    /// `sender` is absent when persisted Provider tracing is disabled.
    /// 关闭 Provider 持久化追踪时 `sender` 为空。
    ///
    /// # Returns
    /// 返回值
    /// A publisher that silently drops diagnostics when tracing is disabled or already finalized.
    /// 返回一个在追踪关闭或已结束时静默丢弃诊断的发布器。
    pub(super) fn new(
        sender: Option<tokio::sync::oneshot::Sender<ProviderApiTraceProcessingFailure>>,
    ) -> Self {
        Self { sender }
    }

    /// Records the first exact post-transport failure without changing business error semantics.
    /// 记录首个精确的传输后失败，且不改变业务错误语义。
    ///
    /// # Parameters
    /// 参数
    /// `stage` is the stable processing boundary and `error` is the original controlled failure.
    /// `stage` 是稳定处理边界，`error` 是原始受控失败。
    pub(super) fn observe_failure(&mut self, stage: &str, error: &VulcanCoreError) {
        let Some(sender) = self.sender.take() else {
            return;
        };
        let _ = sender.send(ProviderApiTraceProcessingFailure {
            stage: stage.to_string(),
            message: error.to_string(),
        });
    }
}

/// Owns physical-attempt numbering for the current durable V1 API identity.
/// 持有当前可靠 V1 API 身份的物理尝试编号。
#[derive(Debug, Default)]
pub(crate) struct ProviderApiTraceAttemptState {
    /// Durable API identity that owns the next attempt index.
    /// 拥有下一尝试下标的可靠 API 身份。
    api_call_id: Option<ApiCallId>,
    /// Index assigned to the next physical request.
    /// 分配给下一次真实请求的索引。
    next_attempt_index: u32,
}

/// Returns the current Unix timestamp in milliseconds for an ordered trace identity.
/// 返回用于有序追踪标识的当前 Unix 毫秒时间戳。
///
/// # Returns
/// 返回值
/// The current timestamp, or an explicit clock error when the host clock predates Unix epoch.
/// 返回当前时间戳；当宿主时钟早于 Unix 纪元时返回明确时钟错误。
fn provider_request_trace_timestamp_ms() -> Result<u128, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .map_err(|error| format!("Provider request trace clock failed: {error}"))
}

/// Redacts credential-shaped header values before a trace reaches disk.
/// 在追踪写入磁盘前脱敏具有凭据形态的请求头值。
///
/// # Parameters
/// 参数
/// `headers` are the final protocol-projected non-transport headers.
/// `headers` 是协议最终投影的非传输请求头。
///
/// # Returns
/// 返回值
/// A complete header map with sensitive values replaced by a fixed marker.
/// 返回完整请求头映射，其中敏感值被固定标记替换。
fn provider_request_trace_headers(headers: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    headers
        .iter()
        .map(|(name, value)| {
            let normalized = name.trim().to_ascii_lowercase();
            let sensitive = matches!(
                normalized.as_str(),
                "authorization"
                    | "proxy-authorization"
                    | "x-api-key"
                    | "api-key"
                    | "x-goog-api-key"
                    | "anthropic-api-key"
                    | "cookie"
                    | "set-cookie"
            ) || normalized.contains("token")
                || normalized.contains("secret")
                || normalized.ends_with("-key");
            (
                name.clone(),
                if sensitive {
                    String::from("<redacted>")
                } else {
                    value.clone()
                },
            )
        })
        .collect()
}

/// Selects the authoritative failure stage for one persisted Provider trace attempt.
/// 为一次持久化 Provider 追踪尝试选择权威失败阶段。
///
/// # Parameters
/// 参数
/// `transport_stage` and `transport_message` come from pre-parser wire capture;
/// `processing_failure` comes from the host-side public response accumulator.
/// `transport_stage` 与 `transport_message` 来自解析前 Wire 捕获；`processing_failure`
/// 来自宿主侧公共响应累积器。
///
/// # Returns
/// 返回值
/// The effective stage, effective message, and whether only local post-transport processing failed.
/// 返回有效阶段、有效消息，以及是否仅本地传输后处理失败。
fn select_provider_api_trace_failure(
    transport_stage: Option<&str>,
    transport_message: Option<&str>,
    processing_failure: Option<&ProviderApiTraceProcessingFailure>,
) -> (Option<String>, Option<String>, bool) {
    let local_processing_failed = transport_stage.is_none() && processing_failure.is_some();
    let stage = transport_stage
        .map(str::to_string)
        .or_else(|| processing_failure.map(|failure| failure.stage.clone()));
    let message = transport_message
        .map(str::to_string)
        .or_else(|| processing_failure.map(|failure| failure.message.clone()));
    (stage, message, local_processing_failed)
}

/// Owned Provider identity rendered into one persisted session trace artifact.
/// 渲染进一份持久化会话追踪制品的 Provider 所有权身份。
#[derive(Debug, Clone)]
pub(crate) struct ProviderApiTraceRenderIdentity {
    /// Stable provider identifier.
    /// 稳定供应商标识。
    pub(crate) provider_id: String,
    /// Session cache scope shared by every physical request of the owning run.
    /// 所属运行中每个真实请求共享的会话缓存作用域。
    pub(crate) session_scope_id: String,
    /// Exact model identifier fixed for the owning execution.
    /// 所属执行固定的精确模型标识。
    pub(crate) model_id: String,
    /// Immutable provider execution route.
    /// 不可变供应商执行路由。
    pub(crate) route: ProviderExecutionRoute,
}

/// Reserves one physical trace identity from the session-owned V1 trace store.
/// 从会话所属 V1 追踪存储中预留一个物理追踪身份。
///
/// # Parameters
/// 参数
/// `provider_request_trace_enabled` is the frozen opt-in decision; `attempt_state` owns the
/// per-API attempt numbering; `api_call_id` is the durable V1 identity.
/// `provider_request_trace_enabled` 是冻结的可选开关决策；`attempt_state` 持有按 API 的尝试
/// 编号；`api_call_id` 是可靠 V1 身份。
///
/// # Returns
/// 返回值
/// A durable sequence bound to the V1 API and physical attempt when tracing is enabled.
/// 启用追踪时返回绑定 V1 API 与物理尝试的可靠序号。
pub(crate) fn claim_provider_api_trace_attempt(
    provider_request_trace_enabled: bool,
    attempt_state: &std::sync::Mutex<ProviderApiTraceAttemptState>,
    api_call_id: ApiCallId,
    trace_store: &ProviderApiTraceStore,
) -> Result<Option<ProviderApiTraceAttemptReservation>, String> {
    if !provider_request_trace_enabled {
        return Ok(None);
    }
    let mut state = attempt_state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if state.api_call_id != Some(api_call_id) {
        state.api_call_id = Some(api_call_id);
        state.next_attempt_index = 0;
    }
    let attempt_index = state.next_attempt_index;
    state.next_attempt_index = state.next_attempt_index.checked_add(1).ok_or_else(|| {
        format!(
            "Provider API trace attempt index exhausted for V1 API {}.",
            api_call_id.value()
        )
    })?;
    drop(state);
    match trace_store.reserve_attempt(api_call_id, attempt_index) {
        ProviderApiTraceReservationOutcome::Reserved(reservation) => Ok(Some(reservation)),
        ProviderApiTraceReservationOutcome::Disabled => Ok(None),
        ProviderApiTraceReservationOutcome::Diagnostic(diagnostic) => Err(format!(
            "{}: {}",
            diagnostic.code.as_str(),
            diagnostic.detail
        )),
    }
}

/// Owns the reused session trace capture pair for one physical Provider attempt.
/// 为一次真实 Provider 尝试持有复用的会话追踪捕获对。
///
/// The caller hands both endpoints to its physical transport; the detached writer publishes
/// exactly one complete redacted artifact after the response capture scope ends.
/// 调用方把两个端点交给自己的真实传输；独立写入器在响应捕获作用域结束后恰好发布一份完整脱敏制品。
pub(crate) struct ProviderApiTraceAttemptCapture {
    /// Request endpoint handed to the physical transport.
    /// 交给真实传输的请求捕获端点。
    pub(crate) request: tokio::sync::oneshot::Sender<ProviderWireRequestTraceDispatch>,
    /// Raw response endpoint handed to the physical transport.
    /// 交给真实传输的原始响应捕获端点。
    pub(crate) response: tokio::sync::oneshot::Sender<ProviderWireResponseCapture>,
}

impl ProviderApiTraceAttemptCapture {
    /// Reserves one durable attempt and starts the detached artifact writer.
    /// 预留一次可靠尝试并启动独立制品写入任务。
    ///
    /// # Parameters
    /// 参数
    /// `provider_request_trace_enabled` is the frozen opt-in decision; `trace_store` is the
    /// session-owned owner carried by the physical permit; `turn_anchor_message_id` preserves
    /// the same turn anchor used by ordinary requests.
    /// `provider_request_trace_enabled` 是冻结的可选开关决策；`trace_store` 是物理许可携带的
    /// 会话所属 owner；`turn_anchor_message_id` 保留与普通请求相同的回合锚点。
    ///
    /// # Returns
    /// 返回值
    /// Capture endpoints only when tracing is enabled and one sequence was durably reserved.
    /// 仅在启用追踪且一个序号已可靠预留时返回捕获端点。
    pub(crate) fn start(
        provider_request_trace_enabled: bool,
        attempt_state: &std::sync::Mutex<ProviderApiTraceAttemptState>,
        trace_store: Option<ProviderApiTraceStore>,
        api_call_id: ApiCallId,
        identity: ProviderApiTraceRenderIdentity,
        turn_anchor_message_id: Option<String>,
    ) -> Option<Self> {
        let Some(trace_store) = trace_store else {
            if provider_request_trace_enabled {
                eprintln!(
                    "[provider-api-trace] session={} skipped=no_session_v1_trace_owner",
                    identity.session_scope_id
                );
            }
            return None;
        };
        let reservation = match claim_provider_api_trace_attempt(
            provider_request_trace_enabled,
            attempt_state,
            api_call_id,
            &trace_store,
        ) {
            Ok(Some(reservation)) => reservation,
            Ok(None) => return None,
            Err(error) => {
                eprintln!(
                    "[provider-api-trace] session={} reservation_failed={error}",
                    identity.session_scope_id
                );
                return None;
            }
        };
        let (request_sender, request_receiver) = tokio::sync::oneshot::channel();
        let (response_sender, response_receiver) = tokio::sync::oneshot::channel();
        let execution_id = uuid::Uuid::new_v4().to_string();
        let trace_execution_id = execution_id;
        let trace_session_scope_id = identity.session_scope_id.clone();
        tokio::spawn(async move {
            let Ok(ProviderWireRequestTraceDispatch {
                request: captured_request,
                persistence_result,
            }) = request_receiver.await
            else {
                // A closed capture channel means encoding failed before an API request existed.
                // 捕获通道关闭表示编码在 API 请求形成前失败，因此没有请求可保存。
                return;
            };
            // The session-owned store already durably reserved trace_sequence before wire
            // dispatch; complete input/output artifacts publish only after capture finishes.
            // 会话级存储已在线路分派前可靠预留 trace_sequence；完整 input/output 制品只在捕获
            // 结束后发布。
            if persistence_result.send(Ok(())).is_err() {
                return;
            }
            // A closed response channel remains explicit in the trace instead of discarding the request.
            // 响应通道关闭会在追踪中显式记录，而不是丢弃请求。
            let response_state = response_receiver.await.map_or(
                ProviderApiTraceResponseState::CaptureChannelClosed,
                ProviderApiTraceResponseState::Captured,
            );
            if let Err(error) = persist_provider_api_trace_snapshot(
                identity,
                trace_store,
                reservation,
                trace_execution_id.as_str(),
                turn_anchor_message_id.as_deref(),
                captured_request,
                response_state,
                None,
            )
            .await
            {
                eprintln!(
                    "[provider-api-trace] session={} execution={} failed={error}",
                    trace_session_scope_id, trace_execution_id
                );
            }
        });
        Some(Self {
            request: request_sender,
            response: response_sender,
        })
    }
}

impl VulcanPublicContractAdapter {
    /// Reserves one physical trace identity from the session-owned V1 trace store.
    /// 从会话所属 V1 追踪存储中预留一个物理追踪身份。
    ///
    /// # Returns
    /// 返回值
    /// A durable sequence bound to the V1 API and physical attempt when tracing is enabled.
    /// 启用追踪时返回绑定 V1 API 与物理尝试的可靠序号。
    pub(super) fn claim_provider_api_trace_attempt(
        &self,
        api_call_id: ApiCallId,
        trace_store: &ProviderApiTraceStore,
    ) -> Result<Option<ProviderApiTraceAttemptReservation>, String> {
        claim_provider_api_trace_attempt(
            self.provider_request_trace_enabled,
            &self.provider_api_trace_attempt_state,
            api_call_id,
            trace_store,
        )
    }

    /// Persists one bounded dispatched API request paired with its pre-parser response evidence.
    /// 为一次执行持久化一份有界的实际分派 API 请求及其解析前响应证据。
    ///
    /// # Parameters
    /// 参数
    /// `execution_id` is the physical call identity shared with the real execution client.
    /// `execution_id` 是与真实执行客户端共享的物理调用标识。
    /// `turn_anchor_message_id` is the latest canonical message that anchored this Provider turn.
    /// `turn_anchor_message_id` 是锚定当前 Provider 回合的最新规范消息。
    /// `captured_request` is the exact secret-safe request projected immediately before dispatch;
    /// `captured_response` retains bounded transport bytes before local protocol parsing.
    /// `captured_request` 是分派前立即投影的精确无秘密请求；`captured_response` 保留本地协议
    /// 解析前的有界传输字节。
    ///
    /// # Returns
    /// 返回值
    /// Success only after the trace has been atomically published in the owning session directory.
    /// 仅在追踪已原子发布到所属会话目录后返回成功。
    pub(super) async fn persist_provider_api_trace_snapshot(
        &self,
        trace_store: ProviderApiTraceStore,
        reservation: ProviderApiTraceAttemptReservation,
        execution_id: &str,
        turn_anchor_message_id: Option<&str>,
        captured_request: ProviderWireRequestCapture,
        response_state: ProviderApiTraceResponseState,
        processing_failure: Option<ProviderApiTraceProcessingFailure>,
    ) -> Result<(), VulcanCoreError> {
        persist_provider_api_trace_snapshot(
            ProviderApiTraceRenderIdentity {
                provider_id: self.provider_id().to_string(),
                session_scope_id: self.cache_scope_id.clone(),
                model_id: self.model_id.clone(),
                route: self.route.clone(),
            },
            trace_store,
            reservation,
            execution_id,
            turn_anchor_message_id,
            captured_request,
            response_state,
            processing_failure,
        )
        .await
    }
}

/// Persists one bounded dispatched API request paired with its pre-parser response evidence.
/// 为一次执行持久化一份有界的实际分派 API 请求及其解析前响应证据。
///
/// # Parameters
/// 参数
/// `identity` carries the exact Provider, session, model, and route fields rendered into the
/// artifact; `execution_id` is the physical call identity shared with the real execution client;
/// `turn_anchor_message_id` is the latest canonical message that anchored this Provider turn;
/// `captured_request` is the exact secret-safe request projected immediately before dispatch;
/// `captured_response` retains bounded transport bytes before local protocol parsing.
/// `identity` 携带渲染进制品的精确 Provider、会话、模型与路由字段；`execution_id` 是与真实执行
/// 客户端共享的物理调用标识；`turn_anchor_message_id` 是锚定当前 Provider 回合的最新规范消息；
/// `captured_request` 是分派前立即投影的精确无秘密请求；`captured_response` 保留本地协议解析前的
/// 有界传输字节。
///
/// # Returns
/// 返回值
/// Success only after the trace has been atomically published in the owning session directory.
/// 仅在追踪已原子发布到所属会话目录后返回成功。
pub(crate) async fn persist_provider_api_trace_snapshot(
    identity: ProviderApiTraceRenderIdentity,
    trace_store: ProviderApiTraceStore,
    reservation: ProviderApiTraceAttemptReservation,
    execution_id: &str,
    turn_anchor_message_id: Option<&str>,
    captured_request: ProviderWireRequestCapture,
    response_state: ProviderApiTraceResponseState,
    processing_failure: Option<ProviderApiTraceProcessingFailure>,
) -> Result<(), VulcanCoreError> {
    let captured_at_unix_ms = provider_request_trace_timestamp_ms()
        .map_err(|error| provider_failure(identity.provider_id.as_str(), &error))?;
    let headers = provider_request_trace_headers(&captured_request.headers);
    let processing_failure_stage = processing_failure
        .as_ref()
        .map(|failure| failure.stage.clone());
    let processing_failure_message = processing_failure
        .as_ref()
        .map(|failure| failure.message.clone());
    let output = match response_state {
        ProviderApiTraceResponseState::CaptureChannelClosed => json!({
            "schema_version": 1,
            "trace_sequence": reservation.trace_sequence,
            "api_call_id": reservation.api_call_id,
            "attempt_index": reservation.attempt_index,
            "capture_point": "before_local_response_framing_and_protocol_decode",
            "capture_state": "response_capture_channel_closed",
            "response": Value::Null,
            "chunks": [],
            "frames": [],
            "outcome": {
                "classification": "response_capture_channel_closed",
                "stage": "response_capture",
                "complete": false,
                "processing_failure_stage": processing_failure_stage,
                "processing_failure_message": processing_failure_message,
            },
        }),
        ProviderApiTraceResponseState::Captured(captured) => {
            let transport_failure_stage = captured.failure_stage.clone();
            let transport_failure_message = captured.failure_message.clone();
            let (effective_failure_stage, effective_failure_message, local_processing_failed) =
                select_provider_api_trace_failure(
                    transport_failure_stage.as_deref(),
                    transport_failure_message.as_deref(),
                    processing_failure.as_ref(),
                );
            let capture_complete = captured.outcome != "stream_failed_or_cancelled"
                && !captured.body_truncated
                && !captured.chunk_index_truncated
                && !captured.frame_index_truncated
                && !local_processing_failed;
            let chunks = captured
                .chunks
                .iter()
                .enumerate()
                .map(|(index, chunk)| {
                    json!({
                        "index": index,
                        "offset": chunk.offset,
                        "length": chunk.length,
                        "observed_at_unix_ms": chunk.observed_at_unix_ms,
                        "sha256": chunk.sha256,
                    })
                })
                .collect::<Vec<_>>();
            let frames = captured
                .frames
                .iter()
                .enumerate()
                .map(|(index, frame)| {
                    json!({
                        "index": index,
                        "kind": frame.kind,
                        "offset": frame.offset,
                        "length": frame.length,
                        "observed_at_unix_ms": frame.observed_at_unix_ms,
                        "sha256": frame.sha256,
                    })
                })
                .collect::<Vec<_>>();
            json!({
                "schema_version": 1,
                "trace_sequence": reservation.trace_sequence,
                "api_call_id": reservation.api_call_id,
                "attempt_index": reservation.attempt_index,
                "capture_point": "before_local_response_framing_and_protocol_decode",
                "capture_state": "captured",
                "response": {
                    "transport": captured.transport,
                    "http_status": captured.http_status,
                    "content_type": captured.content_type,
                    "content_encoding": captured.content_encoding,
                    "received_bytes": captured.received_bytes,
                    "retained_bytes": captured.body.len(),
                    "body_base64": base64::engine::general_purpose::STANDARD.encode(captured.body),
                    "body_sha256": captured.body_sha256,
                },
                "chunks": chunks,
                "frames": frames,
                "outcome": {
                    "classification": if local_processing_failed {
                        "local_processing_failed"
                    } else {
                        captured.outcome.as_str()
                    },
                    "stage": effective_failure_stage
                        .as_deref()
                        .unwrap_or("response_capture_complete"),
                    "complete": capture_complete,
                    "body_truncated": captured.body_truncated,
                    "chunk_index_truncated": captured.chunk_index_truncated,
                    "frame_index_truncated": captured.frame_index_truncated,
                    "normalized_tool_call_abort": captured.normalized_tool_call_abort,
                    "failure_stage": effective_failure_stage,
                    "failure_message": effective_failure_message,
                    "transport_failure_stage": transport_failure_stage,
                    "transport_failure_message": transport_failure_message,
                    "processing_failure_stage": processing_failure_stage,
                    "processing_failure_message": processing_failure_message,
                },
            })
        }
    };
    let input = json!({
        "schema_version": 1,
        "trace_sequence": reservation.trace_sequence,
        "api_call_id": reservation.api_call_id,
        "attempt_index": reservation.attempt_index,
        "captured_at_unix_ms": captured_at_unix_ms,
        "execution_id": execution_id,
        "turn_anchor_message_id": turn_anchor_message_id,
        "session_id": identity.session_scope_id,
        "model_id": identity.model_id,
        "route": {
            "provider": &identity.route.provider,
            "service_entry": &identity.route.entry,
            "connection": &identity.route.connection,
            "protocol_route": &identity.route.protocol_route,
            "secondary_route": &identity.route.secondary_route,
            "custom_model_projection": &identity.route.custom_model_projection,
            "request_projection_profiles": &identity.route.request_projection_profiles,
            "response_projection_profiles": &identity.route.response_projection_profiles,
            "upload_channels": &identity.route.upload_channels,
            "manifest_revision": identity.route.manifest_revision,
            "manifest_generation": identity.route.manifest_generation,
            "route_revision": identity.route.route_revision,
        },
        "request": {
            "capture_point": "transport_dispatch",
            "transport": captured_request.transport,
            "method": captured_request.method,
            "path": captured_request.path,
            "resolved_url": captured_request.resolved_url,
            "headers": headers,
            "credential_values_redacted": true,
            "body": captured_request.body,
        },
    });
    let input = serde_json::to_string_pretty(&input).map_err(|error| {
        provider_failure(
            identity.provider_id.as_str(),
            &format!("Provider API trace input serialization failed: {error}"),
        )
    })?;
    let output = serde_json::to_string_pretty(&output).map_err(|error| {
        provider_failure(
            identity.provider_id.as_str(),
            &format!("Provider API trace output serialization failed: {error}"),
        )
    })?;
    let trace_sequence = reservation.trace_sequence;
    let api_call_id = reservation.api_call_id;
    let attempt_index = reservation.attempt_index;
    let write_outcome = tokio::task::spawn_blocking(move || {
        trace_store.publish_attempt(
            &reservation,
            ProviderApiTracePayload {
                input: input.as_str(),
                output: output.as_str(),
                secrets: &[],
            },
        )
    })
    .await
    .map_err(|error| {
        provider_failure(
            identity.provider_id.as_str(),
            &format!("Provider API trace writer task failed: {error}"),
        )
    })?;
    match write_outcome {
        ProviderApiTraceWriteOutcome::Disabled => Ok(()),
        ProviderApiTraceWriteOutcome::Written { slot } => {
            eprintln!(
                "[provider-api-trace] session={} api_call_id={} attempt={} trace_sequence={} slot={}",
                identity.session_scope_id,
                api_call_id.value(),
                attempt_index,
                trace_sequence,
                slot
            );
            Ok(())
        }
        ProviderApiTraceWriteOutcome::Diagnostic(diagnostic) => Err(provider_failure(
            identity.provider_id.as_str(),
            &format!("{}: {}", diagnostic.code.as_str(), diagnostic.detail),
        )),
    }
}

/// Classifies the host-side accumulator boundary responsible for one normalized Provider event.
/// 对一项规范化 Provider 事件负责的宿主侧累积边界进行分类。
///
/// # Parameters
/// 参数
/// `event` is inspected before ownership moves into the public turn accumulator.
/// `event` 在所有权移入公共回合累积器之前接受检查。
///
/// # Returns
/// 返回值
/// The stable trace stage used when applying this event fails.
/// 返回应用该事件失败时使用的稳定追踪阶段。
pub(super) fn provider_trace_processing_stage_for_event(event: &AiStreamEvent) -> &'static str {
    if matches!(
        event,
        AiStreamEvent::ToolCallStarted { .. }
            | AiStreamEvent::ToolCallArgumentsDelta { .. }
            | AiStreamEvent::ToolCallCompleted { .. }
            | AiStreamEvent::ToolCallAborted { .. }
    ) {
        "tool_request_coordinator"
    } else {
        "response_projection"
    }
}

/// Drains a failed physical Provider stream only to complete opted-in response diagnostics.
/// 仅为完成显式启用的响应诊断而排空失败的真实 Provider 流。
///
/// # Parameters
/// 参数
/// `stream` is the remaining normalized stream whose underlying transport still owns uncaptured bytes.
/// `stream` 是剩余的规范化流，其底层传输仍可能持有尚未捕获的字节。
pub(super) fn drain_provider_stream_for_trace(mut stream: ProviderExecutionStream) {
    // traceDrain deliberately discards semantic events: failed turns must never resume projection or tools.
    // traceDrain 会刻意丢弃语义事件：失败回合绝不能恢复事件投影或工具执行。
    tokio::spawn(async move { while stream.next().await.is_some() {} });
}

#[cfg(test)]
mod tests;
