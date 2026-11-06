//! Session-bound Responses WebSocket execution, reuse, and recovery.
//! 会话绑定的 Responses WebSocket 执行、复用与恢复。

use super::failure::{
    ProviderExecutionFailure, llm_core_protocol_failure, pre_dispatch_failure,
    stream_completion_failure_for_transport, stream_protocol_failure_for_transport,
    stream_transport_failure_with_websocket_close_code, websocket_stream_loss_failure,
};
use super::projection_profiles::apply_response_projection_profiles;
use super::request_encoding::{
    request_with_codex_tool_input_contract, request_without_nonpersistent_responses_continuation,
};
use super::request_headers::{execution_headers, websocket_headers};
use super::route::upstream_protocol_for_route;
use super::stream_state::{update_semantic_output_observation, validate_stream_event};
use super::transport_diagnostics::{
    emit_provider_transport_trace, provider_transport_unix_millis, trace_provider_transport_failure,
};
use super::usage::{ProfiledUsageEventGate, extract_profiled_usage_value_events};
use super::wire_capture::{
    ProviderWireCaptureTargets, ProviderWireResponseCaptureGuard, provider_wire_request_capture,
    publish_wire_request_capture,
};
use super::{ProviderExecutionClient, ProviderExecutionRoute, ProviderExecutionStream};
use crate::CredentialExecutionSecret;
use async_stream::try_stream;
use reqwest::header::{HeaderName, HeaderValue};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex as TokioMutex;
use vulcan_ai_contract::{
    AiExecutionRequest, AiStreamEvent, AiStreamStateMachine, AiTransportKind,
};
use vulcan_llm_core::ProtocolConversionError;
use vulcan_llm_core::{
    DEFAULT_RESPONSES_WEBSOCKET_HEARTBEAT_INTERVAL, DEFAULT_RESPONSES_WEBSOCKET_MAX_AGE,
    EncodedWireRequest, LlmCoreError, RESPONSES_WEBSOCKET_CONNECTION_LIMIT_CODE,
    ResponsesWebsocketConfig, ResponsesWebsocketSession, UpstreamProtocol, UsageAccumulator,
    UsageRecordKind,
};

/// Codex Responses WebSocket protocol revision frozen from the adopted source baseline.
/// 从已采纳源码基线冻结的 Codex Responses WebSocket 协议修订值。
const CODEX_RESPONSES_WEBSOCKET_BETA: &str = "responses_websockets=2026-02-06";

/// Official control error indicating that the active socket no longer owns the referenced chain.
/// 表示活动 Socket 已不再持有被引用链的官方控制错误。
const RESPONSES_WEBSOCKET_PREVIOUS_RESPONSE_NOT_FOUND_CODE: &str = "previous_response_not_found";

/// Maximum process-local Responses WebSocket sessions retained across all roots.
/// 全部根会话合计允许保留的最大进程内 Responses WebSocket 会话数。
const RESPONSES_WEBSOCKET_SESSION_MAX_ENTRIES: usize = 256;

/// Maximum Responses WebSocket health records retained across all roots.
/// 全部根会话合计允许保留的最大 Responses WebSocket 健康记录数量。
const RESPONSES_WEBSOCKET_HEALTH_MAX_ENTRIES: usize = 4_096;

/// Consecutive WebSocket failures after which a session stays on HTTP, matching Codex's
/// default `stream_max_retries`.
/// 连续多少次 WebSocket 失败后会话固定改走 HTTP，与 Codex 默认 `stream_max_retries` 一致。
const RESPONSES_WEBSOCKET_MAX_FAILURES: u32 = 5;

/// Handshake status with which a server states it does not accept WebSocket at all.
/// 服务端声明完全不接受 WebSocket 时使用的握手状态码。
const HTTP_UPGRADE_REQUIRED_STATUS: u16 = 426;

/// Transport health of one session-scoped Responses WebSocket key.
/// 一个会话作用域 Responses WebSocket 键的传输健康状态。
#[derive(Debug, Clone, Copy)]
pub(super) struct ResponsesWebsocketHealth {
    /// Connect or socket failures since the last response that completed over WebSocket.
    /// 自上一次经 WebSocket 完成的响应以来的连接或 Socket 失败次数。
    consecutive_failures: u32,
    /// Whether this key stays on HTTP for the rest of the process lifetime.
    /// 该键是否在进程剩余生命周期内固定使用 HTTP。
    http_fallback: bool,
    /// Monotonic timestamp used for deterministic least-recently-used eviction.
    /// 用于确定性最久未使用淘汰的单调时间戳。
    last_used_at: Instant,
}

impl ResponsesWebsocketHealth {
    /// Creates a healthy record observed at one instant.
    /// 创建一条在某一时刻观察到的健康记录。
    fn new(observed_at: Instant) -> Self {
        Self {
            consecutive_failures: 0,
            http_fallback: false,
            last_used_at: observed_at,
        }
    }
}

/// Stores one shared Responses WebSocket connection and its last cache access.
/// 保存一个共享 Responses WebSocket 连接及其最近缓存访问时间。
#[derive(Clone)]
pub(super) struct ResponsesWebsocketSessionCacheEntry {
    /// Shared non-multiplexed connection handle.
    /// 共享的非多路复用连接句柄。
    session: Arc<TokioMutex<ResponsesWebsocketSession>>,
    /// Monotonic timestamp used for deterministic least-recently-used eviction.
    /// 用于确定性最久未使用淘汰的单调时间戳。
    last_used_at: Instant,
}

impl ProviderExecutionClient {
    /// Encodes the exact credential-projected wire body selected for this route and session.
    /// 编码由当前路由与会话选中的精确凭据投影 Wire 正文。
    ///
    /// # Parameters
    /// 参数
    /// `route` is the frozen provider route and `request` is the immutable canonical request.
    /// `route` 是冻结的供应商路由，`request` 是不可变规范请求。
    /// `secret` supplies the same credential-bound body projection used by real dispatch.
    /// `secret` 提供与真实分派相同的凭据绑定正文投影。
    ///
    /// # Returns
    /// 返回值
    /// The exact credential-projected request body selected by current transport state.
    /// 返回由当前传输状态选中的精确凭据投影请求体。
    pub async fn encode_effective_request_with_credential(
        &self,
        route: &ProviderExecutionRoute,
        request: &AiExecutionRequest,
        secret: &CredentialExecutionSecret,
    ) -> Result<EncodedWireRequest, ProviderExecutionFailure> {
        let websocket_key = responses_websocket_cache_key(route, request, secret);
        let websocket_allowed = websocket_key.as_ref().is_some_and(|key| {
            route.protocol_route.responses_websocket_enabled()
                && upstream_protocol_for_route(route) == Some(UpstreamProtocol::OpenAiResponses)
                && !self.responses_websocket_disabled(key)
        });
        let effective_request = if websocket_allowed {
            let existing = {
                let mut sessions = self.responses_websocket_sessions.lock().await;
                sessions
                    .get_mut(
                        websocket_key
                            .as_deref()
                            .expect("allowed route has a cache key"),
                    )
                    .map(|entry| {
                        entry.last_used_at = Instant::now();
                        Arc::clone(&entry.session)
                    })
            };
            if let Some(existing) = existing {
                let session = existing.lock().await;
                if session.replacement_required() {
                    request_without_nonpersistent_responses_continuation(route, request)
                } else {
                    responses_websocket_request_for_active_session(
                        request,
                        session.previous_response_id(),
                    )
                }
            } else {
                request_without_nonpersistent_responses_continuation(route, request)
            }
        } else {
            request_without_nonpersistent_responses_continuation(route, request)
        };
        let effective_request = request_with_codex_tool_input_contract(route, &effective_request);
        self.encode_request_with_credential(route, &effective_request, secret)
    }

    /// Executes one explicitly enabled Responses route over a non-multiplexed WebSocket.
    /// 通过不复用并发响应的 WebSocket 执行一条显式启用的 Responses 路由。
    pub(super) fn execute_responses_websocket(
        &self,
        route: ProviderExecutionRoute,
        request: AiExecutionRequest,
        secret: CredentialExecutionSecret,
        execution_id: String,
        websocket_key: String,
        wire_captures: ProviderWireCaptureTargets,
    ) -> ProviderExecutionStream {
        let client = self.clone();
        Box::pin(try_stream! {
            let ProviderWireCaptureTargets {
                request: wire_request_capture,
                response: wire_response_capture,
            } = wire_captures;
            // wireRequestCapture remains available until the active WebSocket request is encoded.
            // wireRequestCapture 保留到活动 WebSocket 请求完成编码时使用。
            let mut wire_request_capture = wire_request_capture;
            // wireResponseCaptureGuard publishes every retained frame even when local decoding fails.
            // wireResponseCaptureGuard 即使本地解码失败也会发布所有已经保留的帧。
            let mut wire_response_capture = ProviderWireResponseCaptureGuard::new(
                "responses_websocket",
                wire_response_capture,
            );
            let transport_started_at = Instant::now();
            let dispatched_at_ms = provider_transport_unix_millis()
                .inspect_err(|failure| wire_response_capture.observe_failure("dispatch", failure))?;
            emit_provider_transport_trace(
                execution_id.as_str(),
                "responses_websocket",
                "execution_started",
                &transport_started_at,
            );
            // Handshake projection must never depend on a non-persistent continuation cursor.
            // 握手投影绝不能依赖非持久化的延续游标。
            let handshake_request = request_without_nonpersistent_responses_continuation(
                &route,
                &request,
            );
            let handshake_request =
                request_with_codex_tool_input_contract(&route, &handshake_request);
            let handshake_encoded = client
                .encode_request_with_credential(&route, &handshake_request, &secret)
                .inspect_err(|failure| wire_response_capture.observe_failure("dispatch", failure))?;
            let transport = client
                .protocols
                .transport_profile(route.protocol_route.protocol_profile_id.as_str())
                .map_err(llm_core_protocol_failure)
                .inspect_err(|failure| wire_response_capture.observe_failure("dispatch", failure))?;
            let url = responses_websocket_url(&route, handshake_encoded.path.as_str())
                .inspect_err(|failure| wire_response_capture.observe_failure("dispatch", failure))?;
            let mut headers = execution_headers(
                &route,
                &handshake_encoded.headers,
                &secret,
                handshake_request.cache_scope_id.as_deref(),
            )
                .inspect_err(|failure| wire_response_capture.observe_failure("dispatch", failure))?;
            headers.insert(
                HeaderName::from_static("openai-beta"),
                HeaderValue::from_static(CODEX_RESPONSES_WEBSOCKET_BETA),
            );
            let config = ResponsesWebsocketConfig {
                url: url.clone(),
                headers: websocket_headers(&headers)
                    .inspect_err(|failure| wire_response_capture.observe_failure("dispatch", failure))?,
                connect_timeout: Duration::from_millis(transport.response_headers_timeout_ms),
                idle_timeout: Duration::from_millis(transport.stream_idle_timeout_ms),
                max_connection_age: DEFAULT_RESPONSES_WEBSOCKET_MAX_AGE,
                heartbeat_interval: DEFAULT_RESPONSES_WEBSOCKET_HEARTBEAT_INTERVAL,
                max_frame_bytes: transport.max_frame_bytes,
            };
            let session_handle = match client
                .responses_websocket_session(websocket_key.as_str(), config.clone())
                .await
            {
                Ok(session) => session,
                Err(error) => {
                    client.record_responses_websocket_failure(
                        websocket_key.as_str(),
                        error.websocket_handshake_status,
                    );
                    let failure = trace_provider_transport_failure(
                        execution_id.as_str(),
                        "responses_websocket",
                        stream_transport_failure_with_websocket_close_code(
                            error.message,
                            false,
                            error.websocket_close_code,
                        ),
                        &transport_started_at,
                    );
                    wire_response_capture.observe_failure("dispatch", &failure);
                    Err(failure)?
                }
            };
            let mut session = session_handle.lock().await;
            if session.replacement_required() {
                let _ = session.close().await;
                *session = match ResponsesWebsocketSession::connect(config).await {
                    Ok(session) => session,
                    Err(error) => {
                        client.record_responses_websocket_failure(
                        websocket_key.as_str(),
                        error.websocket_handshake_status,
                    );
                        let failure = trace_provider_transport_failure(
                            execution_id.as_str(),
                            "responses_websocket",
                            stream_transport_failure_with_websocket_close_code(
                                error.message,
                                false,
                                error.websocket_close_code,
                            ),
                            &transport_started_at,
                        );
                        wire_response_capture.observe_failure("dispatch", &failure);
                        Err(failure)?
                    }
                };
            }
            // A store=false cursor is valid only when this exact active socket proves ownership.
            // store=false 游标仅在当前精确活动 Socket 证明其归属时才有效。
            let wire_request = responses_websocket_request_for_active_session(
                &request,
                session.previous_response_id(),
            );
            // wireRequest is the exact encoded and decoded request snapshot for this response.
            // wireRequest 是当前响应用于编码与解码的精确请求快照。
            let wire_request = request_with_codex_tool_input_contract(&route, &wire_request);
            let encoded = client
                .encode_request_with_credential(&route, &wire_request, &secret)
                .inspect_err(|failure| wire_response_capture.observe_failure("dispatch", failure))?;
            // The capture contains the exact frame payload plus the final secret-safe handshake headers.
            // 捕获内容包含精确帧载荷以及最终且不含秘密值的握手请求头。
            if let Some(capture) = wire_request_capture.take() {
                publish_wire_request_capture(
                    capture,
                    provider_wire_request_capture(
                        "responses_websocket",
                        "WEBSOCKET_FRAME",
                        url.as_str(),
                        &encoded,
                        &headers,
                    ),
                )
                .await
                .inspect_err(|failure| wire_response_capture.observe_failure("dispatch", failure))?;
            }
            if let Err(error) = session.begin_response(encoded.body, None).await {
                retire_failed_responses_websocket(
                    &client,
                    &mut session,
                    websocket_key.as_str(),
                );
                let failure = trace_provider_transport_failure(
                    execution_id.as_str(),
                    "responses_websocket",
                    stream_transport_failure_with_websocket_close_code(
                        error.message,
                        false,
                        error.websocket_close_code,
                    ),
                    &transport_started_at,
                );
                wire_response_capture.observe_failure("dispatch", &failure);
                Err(failure)?;
            }
            yield AiStreamEvent::TransportObserved {
                transport: AiTransportKind::ResponsesWebsocket,
                dispatched_at_ms,
                headers_at_ms: None,
                http_status: None,
                provider_request_id: None,
                rate_limit_headers: BTreeMap::new(),
            };
            emit_provider_transport_trace(
                execution_id.as_str(),
                "responses_websocket",
                "request_dispatched",
                &transport_started_at,
            );
            let mut decoder = client
                .protocols
                .create_stream_decoder(
                    route.protocol_route.protocol_profile_id.as_str(),
                    execution_id.clone(),
                    &wire_request,
                )
                .map_err(|error| {
                    let failure = llm_core_protocol_failure(error);
                    wire_response_capture.observe_failure("protocol_decoder", &failure);
                    failure
                })?;
            let usage_profiles = route.protocol_route.usage_extraction_profiles.clone();
            let response_profiles = route.response_projection_profiles.clone();
            let profiled_usage_enabled = !usage_profiles.is_empty();
            let mut usage_accumulator = UsageAccumulator::default();
            let mut profiled_usage_gate = ProfiledUsageEventGate::default();
            let mut semantic_output_observed = false;
            let mut stream_state = AiStreamStateMachine::default();
            let mut terminal_observed = false;
            loop {
                let mut frame = match session.next_frame().await {
                    Ok(Some(frame)) => {
                        wire_response_capture.observe_websocket_frame(frame.raw.clone());
                        frame
                    }
                    Ok(None) => {
                        retire_failed_responses_websocket(
                            &client,
                            &mut session,
                            websocket_key.as_str(),
                        );
                        let failure = trace_provider_transport_failure(
                            execution_id.as_str(),
                            "responses_websocket",
                            websocket_stream_loss_failure(
                                "Responses WebSocket closed without a terminal event.",
                                semantic_output_observed,
                                None,
                            ),
                            &transport_started_at,
                        );
                        wire_response_capture.observe_failure("body_transport", &failure);
                        Err(failure)?
                    }
                    Err(error) => {
                        if let Some(raw_frame) = session.take_last_raw_frame() {
                            wire_response_capture.observe_websocket_frame(raw_frame);
                        }
                        retire_failed_responses_websocket(
                            &client,
                            &mut session,
                            websocket_key.as_str(),
                        );
                        let failure = trace_provider_transport_failure(
                            execution_id.as_str(),
                            "responses_websocket",
                            websocket_stream_loss_failure(
                                error.message,
                                semantic_output_observed,
                                error.websocket_close_code,
                            ),
                            &transport_started_at,
                        );
                        wire_response_capture.observe_failure("body_transport", &failure);
                        Err(failure)?
                    }
                };
                apply_response_projection_profiles(&response_profiles, &mut frame.value).map_err(
                    |error| {
                        let failure = abandon_responses_websocket_attempt(
                            &mut session,
                            stream_protocol_failure_for_transport(
                                error,
                                semantic_output_observed,
                                AiTransportKind::ResponsesWebsocket,
                            ),
                        );
                        wire_response_capture.observe_failure("response_projection", &failure);
                        failure
                    },
                )?;
                if let Some((code, message)) = responses_websocket_retry_control(&frame.value) {
                    let failure = responses_websocket_control_failure(
                        code,
                        message,
                        semantic_output_observed,
                    );
                    wire_response_capture.observe_failure("protocol_decoder", &failure);
                    Err(failure)?;
                }
                let profiled_usage_events = extract_profiled_usage_value_events(
                    &usage_profiles,
                    UsageRecordKind::WebSocketFrame,
                    &frame.value,
                    &mut usage_accumulator,
                )
                .map_err(|error| {
                    let failure = abandon_responses_websocket_attempt(
                        &mut session,
                        stream_protocol_failure_for_transport(
                            error,
                            semantic_output_observed,
                            AiTransportKind::ResponsesWebsocket,
                        ),
                    );
                    wire_response_capture.observe_failure("response_projection", &failure);
                    failure
                })?;
                for mut usage_event in profiled_usage_gate
                    .stage(profiled_usage_events)
                    .map_err(|error| {
                        let failure = abandon_responses_websocket_attempt(
                            &mut session,
                            stream_protocol_failure_for_transport(
                                error,
                                semantic_output_observed,
                                AiTransportKind::ResponsesWebsocket,
                            ),
                        );
                        wire_response_capture.observe_failure("response_projection", &failure);
                        failure
                    })?
                {
                    validate_stream_event(
                        &mut stream_state,
                        &usage_event,
                        semantic_output_observed,
                    )
                    .map_err(|failure| {
                        let failure = abandon_responses_websocket_attempt(&mut session, failure);
                        wire_response_capture.observe_failure("protocol_decoder", &failure);
                        failure
                    })?;
                    update_semantic_output_observation(
                        &mut semantic_output_observed,
                        &mut usage_event,
                    );
                    yield usage_event;
                }
                for mut event in decoder
                    .push_json(&frame.value)
                    .map_err(|error| {
                        let failure = abandon_responses_websocket_attempt(
                            &mut session,
                            stream_protocol_failure_for_transport(
                                error,
                                semantic_output_observed,
                                AiTransportKind::ResponsesWebsocket,
                            ),
                        );
                        wire_response_capture.observe_failure("protocol_decoder", &failure);
                        failure
                    })?
                {
                    if profiled_usage_enabled
                        && matches!(event, AiStreamEvent::UsageUpdated { .. })
                    {
                        continue;
                    }
                    let was_semantic_output_observed = semantic_output_observed;
                    update_semantic_output_observation(
                        &mut semantic_output_observed,
                        &mut event,
                    );
                    if semantic_output_observed && !was_semantic_output_observed {
                        emit_provider_transport_trace(
                            execution_id.as_str(),
                            "responses_websocket",
                            "first_semantic_event",
                            &transport_started_at,
                        );
                    }
                    terminal_observed |= matches!(
                        event,
                        AiStreamEvent::ResponseCompleted | AiStreamEvent::ResponseFailed { .. }
                    );
                    validate_stream_event(&mut stream_state, &event, semantic_output_observed)
                        .map_err(|failure| {
                            let failure = abandon_responses_websocket_attempt(&mut session, failure);
                            wire_response_capture.observe_failure("protocol_decoder", &failure);
                            failure
                        })?;
                    record_terminal_websocket_success(&client, websocket_key.as_str(), &event);
                    wire_response_capture.observe_normalized_event(&event);
                    let released_usage = profiled_usage_gate.observe(&event);
                    yield event;
                    for mut usage_event in released_usage {
                        validate_stream_event(
                            &mut stream_state,
                            &usage_event,
                            semantic_output_observed,
                        )
                        .map_err(|failure| {
                            let failure = abandon_responses_websocket_attempt(&mut session, failure);
                            wire_response_capture.observe_failure("protocol_decoder", &failure);
                            failure
                        })?;
                        update_semantic_output_observation(
                            &mut semantic_output_observed,
                            &mut usage_event,
                        );
                        yield usage_event;
                    }
                }
                if frame.completed || frame.failed {
                    break;
                }
            }
            for mut event in decoder
                .finish()
                .map_err(|error| {
                    let failure = abandon_responses_websocket_attempt(
                        &mut session,
                        stream_completion_failure_for_transport(
                            error,
                            semantic_output_observed,
                            AiTransportKind::ResponsesWebsocket,
                        ),
                    );
                    wire_response_capture.observe_failure("protocol_decoder", &failure);
                    failure
                })?
            {
                if profiled_usage_enabled && matches!(event, AiStreamEvent::UsageUpdated { .. }) {
                    continue;
                }
                update_semantic_output_observation(&mut semantic_output_observed, &mut event);
                terminal_observed |= matches!(
                    event,
                    AiStreamEvent::ResponseCompleted | AiStreamEvent::ResponseFailed { .. }
                );
                validate_stream_event(&mut stream_state, &event, semantic_output_observed)
                    .map_err(|failure| {
                        let failure = abandon_responses_websocket_attempt(&mut session, failure);
                        wire_response_capture.observe_failure("protocol_decoder", &failure);
                        failure
                    })?;
                record_terminal_websocket_success(&client, websocket_key.as_str(), &event);
                wire_response_capture.observe_normalized_event(&event);
                let released_usage = profiled_usage_gate.observe(&event);
                yield event;
                for mut usage_event in released_usage {
                    validate_stream_event(
                        &mut stream_state,
                        &usage_event,
                        semantic_output_observed,
                    )
                    .map_err(|failure| {
                        let failure = abandon_responses_websocket_attempt(&mut session, failure);
                        wire_response_capture.observe_failure("protocol_decoder", &failure);
                        failure
                    })?;
                    update_semantic_output_observation(
                        &mut semantic_output_observed,
                        &mut usage_event,
                    );
                    yield usage_event;
                }
            }
            profiled_usage_gate
                .ensure_released()
                .map_err(|error| {
                    let failure = abandon_responses_websocket_attempt(
                        &mut session,
                        stream_completion_failure_for_transport(
                            error,
                            semantic_output_observed,
                            AiTransportKind::ResponsesWebsocket,
                        ),
                    );
                    wire_response_capture.observe_failure("protocol_decoder", &failure);
                    failure
                })?;
            if !terminal_observed || !stream_state.is_terminal() {
                let failure = abandon_responses_websocket_attempt(
                    &mut session,
                    stream_completion_failure_for_transport(
                        ProtocolConversionError::new(
                            "Responses WebSocket ended without a normalized terminal event.",
                        ),
                        semantic_output_observed,
                        AiTransportKind::ResponsesWebsocket,
                    ),
                );
                wire_response_capture.observe_failure("protocol_decoder", &failure);
                Err(failure)?;
            }
            emit_provider_transport_trace(
                execution_id.as_str(),
                "responses_websocket",
                "execution_completed",
                &transport_started_at,
            );
            wire_response_capture.set_outcome_if_unresolved("completed");
        })
    }

    /// Returns or establishes one session-isolated WebSocket connection.
    /// 返回或建立一个会话隔离的 WebSocket 连接。
    async fn responses_websocket_session(
        &self,
        key: &str,
        config: ResponsesWebsocketConfig,
    ) -> Result<Arc<TokioMutex<ResponsesWebsocketSession>>, vulcan_llm_core::LlmCoreError> {
        {
            let mut sessions = self.responses_websocket_sessions.lock().await;
            if let Some(existing) = sessions.get_mut(key) {
                existing.last_used_at = Instant::now();
                return Ok(Arc::clone(&existing.session));
            }
            if sessions.len() >= RESPONSES_WEBSOCKET_SESSION_MAX_ENTRIES
                && oldest_idle_responses_websocket_session_key(&sessions).is_none()
            {
                return Err(LlmCoreError::new(format!(
                    "Responses WebSocket session capacity {} is exhausted by active connections.",
                    RESPONSES_WEBSOCKET_SESSION_MAX_ENTRIES
                )));
            }
        }
        let connected = Arc::new(TokioMutex::new(
            ResponsesWebsocketSession::connect(config).await?,
        ));
        let mut sessions = self.responses_websocket_sessions.lock().await;
        if let Some(existing) = sessions.get_mut(key) {
            existing.last_used_at = Instant::now();
            return Ok(Arc::clone(&existing.session));
        }
        if sessions.len() >= RESPONSES_WEBSOCKET_SESSION_MAX_ENTRIES {
            let eviction_key = oldest_idle_responses_websocket_session_key(&sessions)
                .ok_or_else(|| {
                    LlmCoreError::new(format!(
                        "Responses WebSocket session capacity {} is exhausted by active connections.",
                        RESPONSES_WEBSOCKET_SESSION_MAX_ENTRIES
                    ))
                })?;
            sessions.remove(eviction_key.as_str());
        }
        sessions.insert(
            key.to_string(),
            ResponsesWebsocketSessionCacheEntry {
                session: Arc::clone(&connected),
                last_used_at: Instant::now(),
            },
        );
        Ok(connected)
    }

    /// Reports whether a session-scoped WebSocket breaker is active and refreshes its LRU age.
    /// 报告会话作用域 WebSocket 熔断器是否生效，并刷新其 LRU 时间。
    ///
    /// # Parameters
    /// 参数
    /// `key` is the opaque credential-, route-, model-, and root-session-bound cache key.
    /// `key` 是绑定凭据、路由、模型与根会话的不透明缓存键。
    ///
    /// # Returns
    /// 返回值
    /// `true` only when this exact cache key has an active process-local breaker.
    /// 仅当该精确缓存键存在活动的进程内熔断器时返回 `true`。
    pub(super) fn responses_websocket_disabled(&self, key: &str) -> bool {
        let mut health = self
            .responses_websocket_health
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(record) = health.get_mut(key).filter(|record| record.http_fallback) else {
            return false;
        };
        record.last_used_at = Instant::now();
        true
    }

    /// Records one connect or socket failure using Codex's fallback rule.
    /// 按 Codex 的回退规则记录一次连接或 Socket 失败。
    ///
    /// A handshake answered with `426 Upgrade Required` pins HTTP at once; every other failure
    /// only counts, so the retry reconnects over WebSocket until consecutive failures reach the
    /// limit.
    /// 握手被 `426 Upgrade Required` 应答时立即固定为 HTTP；其他失败只计数，重试会重新建立
    /// WebSocket 连接，直到连续失败达到上限。
    ///
    /// # Parameters
    /// 参数
    /// `key` is the opaque session key and `handshake_status` is the HTTP status of a refused
    /// handshake, when one was received.
    /// `key` 是不透明会话键，`handshake_status` 是被拒绝握手的 HTTP 状态码（若收到）。
    fn record_responses_websocket_failure(&self, key: &str, handshake_status: Option<u16>) {
        let mut health = self
            .responses_websocket_health
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let record = bounded_responses_websocket_health(&mut health, key, Instant::now());
        record.consecutive_failures = record.consecutive_failures.saturating_add(1);
        record.http_fallback |= handshake_status == Some(HTTP_UPGRADE_REQUIRED_STATUS)
            || record.consecutive_failures >= RESPONSES_WEBSOCKET_MAX_FAILURES;
    }

    /// Clears the consecutive-loss count after one response completes over WebSocket.
    /// 在一次响应经 WebSocket 完成后清除连续丢失计数。
    fn record_responses_websocket_success(&self, key: &str) {
        let mut health = self
            .responses_websocket_health
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if health.get(key).is_some_and(|record| !record.http_fallback) {
            health.remove(key);
        }
    }
}

/// Retires a socket after a transport failure and records the loss against its session.
/// 在传输失败后让 Socket 退役，并为其会话记录这次丢失。
///
/// Any close code, 1013 included, is an ordinary retryable loss as in Codex.
/// 与 Codex 一致，任何关闭码（包括 1013）都是普通的可重试丢失。
fn retire_failed_responses_websocket(
    client: &ProviderExecutionClient,
    session: &mut ResponsesWebsocketSession,
    websocket_key: &str,
) {
    session.abandon_response();
    client.record_responses_websocket_failure(websocket_key, None);
}

/// Clears the session's consecutive-loss count once a validated terminal event arrives.
/// 在收到已校验的终态事件时清除会话的连续丢失计数。
///
/// It runs before the event is yielded, because a consumer may drop the stream as soon as it
/// reads the terminal event and code after that yield would never run.
/// 它在事件交出之前执行，因为消费方读到终态事件后可能立即丢弃流，交出之后的代码不会再执行。
fn record_terminal_websocket_success(
    client: &ProviderExecutionClient,
    websocket_key: &str,
    event: &AiStreamEvent,
) {
    if matches!(
        event,
        AiStreamEvent::ResponseCompleted | AiStreamEvent::ResponseFailed { .. }
    ) {
        client.record_responses_websocket_success(websocket_key);
    }
}

/// Selects incremental WebSocket input only when the active socket proves cursor ownership.
/// 仅在活动 Socket 证明游标归属时选择 WebSocket 增量输入。
///
/// # Parameters
/// 参数
/// `request` carries the proven local append boundary and `active_response_id` is owned by the
/// exact cached socket.
/// `request` 携带已证明的本地追加边界，`active_response_id` 由精确缓存 Socket 持有。
///
/// # Returns
/// 返回值
/// The incremental request on an exact identity match, otherwise a complete-history request.
/// 精确身份匹配时返回增量请求，否则返回完整历史请求。
fn responses_websocket_request_for_active_session(
    request: &AiExecutionRequest,
    active_response_id: Option<&str>,
) -> AiExecutionRequest {
    let continuation_matches = request.continuation.as_ref().is_some_and(|continuation| {
        active_response_id == Some(continuation.provider_message_id.as_str())
    });
    let mut request = request.clone();
    if !continuation_matches {
        request.continuation = None;
    }
    request
}

/// Extracts retryable Codex WebSocket control errors from untouched provider frames.
/// 从未经修改的供应商帧中提取可重试的 Codex WebSocket 控制错误。
///
/// # Parameters
/// 参数
/// `value` is the exact parsed WebSocket frame before protocol decoding.
/// `value` 是协议解码前的精确已解析 WebSocket 帧。
///
/// # Returns
/// 返回值
/// The stable code and controlled message for recoverable chain or connection loss.
/// 返回可恢复链或连接丢失对应的稳定代码与受控消息。
fn responses_websocket_retry_control(value: &Value) -> Option<(String, String)> {
    let code = value
        .pointer("/error/code")
        .or_else(|| value.pointer("/response/error/code"))
        .and_then(Value::as_str)?;
    if !matches!(
        code,
        RESPONSES_WEBSOCKET_CONNECTION_LIMIT_CODE
            | RESPONSES_WEBSOCKET_PREVIOUS_RESPONSE_NOT_FOUND_CODE
    ) {
        return None;
    }
    let message = value
        .pointer("/error/message")
        .or_else(|| value.pointer("/response/error/message"))
        .and_then(Value::as_str)
        .filter(|message| !message.trim().is_empty())
        .unwrap_or(match code {
            RESPONSES_WEBSOCKET_CONNECTION_LIMIT_CODE => {
                "Responses WebSocket connection lifetime was exhausted."
            }
            RESPONSES_WEBSOCKET_PREVIOUS_RESPONSE_NOT_FOUND_CODE => {
                "Responses WebSocket previous response was not found."
            }
            _ => unreachable!("retry control code was matched above"),
        });
    Some((code.to_string(), message.to_string()))
}

/// Converts a recoverable WebSocket control event into one replay-safe physical-attempt failure.
/// 将可恢复 WebSocket 控制事件转换为一次可安全重放的真实尝试失败。
///
/// # Parameters
/// 参数
/// `code` and `message` preserve bounded upstream diagnostics; `semantic_output_observed` protects
/// callers from replay after visible output.
/// `code` 与 `message` 保留有界上游诊断，`semantic_output_observed` 防止在可见输出后重放。
///
/// # Returns
/// 返回值
/// A structured retryable transport failure whose safety flag remains authoritative.
/// 返回一项安全标志仍具权威性的结构化可重试传输失败。
fn responses_websocket_control_failure(
    code: String,
    message: String,
    semantic_output_observed: bool,
) -> ProviderExecutionFailure {
    let mut failure = stream_transport_failure_with_websocket_close_code(
        message.clone(),
        semantic_output_observed,
        None,
    );
    failure.error.subcategory = Some(String::from("responses_websocket_control"));
    failure.error.upstream_code = Some(code);
    failure.error.upstream_message = Some(message);
    failure
}

/// Invalidates a socket after local decoding or lifecycle validation rejects its active response.
/// 在本地解码或生命周期校验拒绝活动响应后使 Socket 失效。
///
/// # Parameters
/// 参数
/// `session` owns the exact in-flight socket and `failure` preserves the original typed cause.
/// `session` 持有精确的处理中 Socket，`failure` 保留原始强类型原因。
///
/// # Returns
/// 返回值
/// The unchanged failure after synchronously clearing unsafe connection-local state.
/// 同步清除不安全的连接局部状态后返回未经修改的失败。
fn abandon_responses_websocket_attempt(
    session: &mut ResponsesWebsocketSession,
    failure: ProviderExecutionFailure,
) -> ProviderExecutionFailure {
    session.abandon_response();
    failure
}

/// Builds the non-secret isolation key for one Responses WebSocket session.
/// 为一个 Responses WebSocket 会话构建不含密钥的隔离键。
pub(super) fn responses_websocket_cache_key(
    route: &ProviderExecutionRoute,
    request: &AiExecutionRequest,
    secret: &CredentialExecutionSecret,
) -> Option<String> {
    let root_session_id = request.cache_scope_id.as_deref()?;
    let material = serde_json::to_vec(&json!({
        "provider_id": route.provider.id,
        "entry": route.entry,
        "connection_id": route.connection.id,
        "credential_revision": secret.revision(),
        "protocol_route": route.protocol_route,
        "secondary_route": route.secondary_route,
        "custom_model_projection": route.custom_model_projection,
        "model_id": request.model_id,
        "root_session_id": root_session_id,
    }))
    .ok()?;
    Some(format!("sha256:{}", hex::encode(Sha256::digest(material))))
}

/// Selects the least-recently-used WebSocket session that has no external owner.
/// 选择没有外部持有者且最久未使用的 WebSocket 会话。
///
/// # Parameters
/// 参数
/// `sessions` is the complete bounded process-local WebSocket session table.
/// `sessions` 是完整的有界进程内 WebSocket 会话表。
///
/// # Returns
/// 返回值
/// The deterministic eviction key, or `None` when every cached session is currently referenced.
/// 返回确定性的淘汰键；当每个缓存会话当前都被引用时返回 `None`。
fn oldest_idle_responses_websocket_session_key(
    sessions: &BTreeMap<String, ResponsesWebsocketSessionCacheEntry>,
) -> Option<String> {
    sessions
        .iter()
        .filter(|(_, entry)| Arc::strong_count(&entry.session) == 1)
        .min_by(|(left_key, left), (right_key, right)| {
            left.last_used_at
                .cmp(&right.last_used_at)
                .then_with(|| left_key.cmp(right_key))
        })
        .map(|(key, _)| key.clone())
}

/// Returns one WebSocket health record while enforcing a deterministic LRU capacity bound.
/// 在强制确定性 LRU 容量上限的同时返回一条 WebSocket 健康记录。
///
/// # Parameters
/// 参数
/// `health` is the mutable process-local health table, `key` is the opaque session key, and
/// `observed_at` is the monotonic access timestamp.
/// `health` 是可变进程内健康表，`key` 是不透明会话键，`observed_at` 是单调访问时间。
///
/// # Returns
/// 返回值
/// The existing or newly created record with its access time refreshed; an older record is
/// evicted before a new key can exceed the hard limit.
/// 返回已刷新访问时间的既有或新建记录；新键超过硬上限前会先淘汰一条更旧的记录。
fn bounded_responses_websocket_health<'a>(
    health: &'a mut BTreeMap<String, ResponsesWebsocketHealth>,
    key: &str,
    observed_at: Instant,
) -> &'a mut ResponsesWebsocketHealth {
    if !health.contains_key(key)
        && health.len() >= RESPONSES_WEBSOCKET_HEALTH_MAX_ENTRIES
        && let Some(oldest_key) = health
            .iter()
            .min_by(|(left_key, left), (right_key, right)| {
                left.last_used_at
                    .cmp(&right.last_used_at)
                    .then_with(|| left_key.cmp(right_key))
            })
            .map(|(oldest_key, _)| oldest_key.clone())
    {
        health.remove(oldest_key.as_str());
    }
    let record = health
        .entry(key.to_string())
        .or_insert_with(|| ResponsesWebsocketHealth::new(observed_at));
    record.last_used_at = observed_at;
    record
}

/// Converts the exact HTTP Responses endpoint to its WebSocket scheme.
/// 将精确 HTTP Responses 端点转换为对应 WebSocket scheme。
fn responses_websocket_url(
    route: &ProviderExecutionRoute,
    encoded_path: &str,
) -> Result<String, ProviderExecutionFailure> {
    let http_url = route.request_url(encoded_path);
    let mut url = reqwest::Url::parse(http_url.as_str()).map_err(|error| {
        pre_dispatch_failure(format!("Responses WebSocket endpoint is invalid: {error}"))
    })?;
    let websocket_scheme = match url.scheme() {
        "https" => "wss",
        "http" => "ws",
        scheme => {
            return Err(pre_dispatch_failure(format!(
                "Responses WebSocket cannot derive from endpoint scheme `{scheme}`."
            )));
        }
    };
    url.set_scheme(websocket_scheme).map_err(|_| {
        pre_dispatch_failure("Responses WebSocket endpoint scheme could not be replaced.")
    })?;
    Ok(url.to_string())
}

#[cfg(test)]
mod tests;
