//! Standalone Responses compaction requests and validated replay checkpoints.
//! 独立 Responses 压缩请求与已验证的重放检查点。

use super::failure::{
    ProviderExecutionFailure, compact_protocol_failure, compact_transport_failure, http_failure,
    http_send_transport_failure, llm_core_protocol_failure, pre_dispatch_failure, protocol_failure,
    sanitize_provider_execution_failure, transport_failure,
};
use super::projection_profiles::{
    apply_response_projection_profiles, project_response_record_text,
};
use super::request_encoding::{
    project_credential_request, project_provider_request, request_with_codex_tool_input_contract,
    validate_request_policy, validate_request_projection_guards_before_encoding,
};
use super::request_headers::execution_headers;
use super::response_body::read_bounded_response_text_with_capture;
use super::response_metadata::{
    allowlisted_rate_limit_headers, parse_retry_after_headers, provider_request_id_from_headers,
};
use super::transport_diagnostics::{
    emit_provider_transport_failure_trace, emit_provider_transport_http_status_trace,
    emit_provider_transport_trace, emit_responses_compact_response_shape_trace,
    provider_transport_unix_millis,
};
use super::wire_capture::{
    ProviderWireRequestCaptureTarget, ProviderWireRequestTraceDispatch,
    ProviderWireResponseCapture, ProviderWireResponseCaptureGuard, provider_wire_request_capture,
    publish_wire_request_capture,
};
use super::{ProviderExecutionClient, ProviderExecutionRoute};
use crate::CredentialExecutionSecret;
use async_stream::try_stream;
use futures_util::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::pin::Pin;
use std::time::{Duration, Instant, SystemTime};
use tokio::sync::oneshot;
use vulcan_ai_contract::{
    AiErrorCategory, AiExecutionRequest, DispatchOutcome, RetryAfterHint,
    canonicalize_codex_responses_replay_items, is_replayable_codex_responses_item,
};
use vulcan_llm_core::OPENAI_CODEX_RESPONSES_PROFILE_ID;
use vulcan_llm_core::encode_codex_responses_request;

/// Official Codex multiplier from stream idle timeout to the complete unary compact timeout.
/// 官方 Codex 从流空闲超时换算完整一元压缩超时所使用的倍数。
const RESPONSES_COMPACT_TIMEOUT_IDLE_MULTIPLIER: u32 = 4;

/// Carries one validated non-streaming Responses compaction result.
/// 承载一份已验证的非流式 Responses 远端压缩结果。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponsesCompactResult {
    /// Ordered provider-native output items preserved for the next Responses request.
    /// 为下一次 Responses 请求保留的有序供应商原生输出项。
    pub output_items: Vec<Value>,
    /// Optional provider-reported usage object retained without lossy projection.
    /// 可选的供应商用量对象，以无损形式保留。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<Value>,
    /// Optional upstream response identifier.
    /// 可选的上游响应标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_id: Option<String>,
}

/// Separates physical HTTP observations from the opaque unary compaction result.
/// 将物理 HTTP 观测与不透明的一元压缩结果分开。
#[derive(Debug, Clone, PartialEq)]
pub enum ResponsesCompactEvent {
    /// Response headers were observed before consuming either a success or failure body.
    /// 在消费成功或失败正文前已观察到响应头。
    TransportObserved {
        /// Host timestamp immediately before sending this one HTTP request.
        /// 发送本次唯一 HTTP 请求前的宿主时间戳。
        dispatched_at_ms: i64,
        /// Host timestamp when response headers became available.
        /// 响应头可用时的宿主时间戳。
        headers_at_ms: i64,
        /// Actual response status, including unauthorized and other failures.
        /// 实际响应状态，包括未授权及其他失败。
        http_status: u16,
        /// Upstream request identity selected by the frozen route allowlist.
        /// 由冻结路由允许列表选出的上游请求身份。
        provider_request_id: Option<String>,
        /// Allowlisted quota headers shared with ordinary HTTP execution.
        /// 与普通 HTTP 执行共用的允许列表配额响应头。
        rate_limit_headers: BTreeMap<String, String>,
    },
    /// Complete validated native output; this is not assistant text or a continuation token.
    /// 完整且已验证的原生输出；它不是助手正文或延续令牌。
    Completed(ResponsesCompactResult),
}

/// One lazy HTTP attempt with ordered observations and a single terminal result or failure.
/// 一次惰性 HTTP 尝试，包含有序观测及唯一终态结果或失败。
pub type ResponsesCompactStream =
    Pin<Box<dyn Stream<Item = Result<ResponsesCompactEvent, ProviderExecutionFailure>> + Send>>;

impl ProviderExecutionClient {
    /// Executes the standalone Codex Responses compaction endpoint without streaming.
    /// 执行独立 Codex Responses 远端压缩端点，且不使用流式传输。
    ///
    /// # Parameters
    /// 参数
    /// `route` and `request` are the exact immutable provider route and conversation projection.
    /// `route` 与 `request` 是精确不可变供应商路由和会话投影。
    /// `secret` supplies only the credential-bound authentication projection.
    /// `secret` 仅提供凭据绑定的鉴权投影。
    ///
    /// # Returns
    /// 返回值
    /// A bounded result whose ordered output items can be atomically persisted as a route-bound checkpoint.
    /// 一份有界结果；其有序输出项可作为路由绑定检查点原子持久化。
    pub async fn execute_responses_compact(
        &self,
        route: ProviderExecutionRoute,
        request: AiExecutionRequest,
        secret: CredentialExecutionSecret,
    ) -> Result<ResponsesCompactResult, ProviderExecutionFailure> {
        // stream preserves the same one-attempt transport contract for non-session callers.
        // stream 为非会话调用方保留同一单次尝试传输契约。
        let mut stream = self.stream_responses_compact(route, request, secret, None, None);
        while let Some(event) = stream.next().await {
            if let ResponsesCompactEvent::Completed(result) = event? {
                return Ok(result);
            }
        }
        Err(compact_protocol_failure(
            "Remote Responses compaction ended without a terminal result.",
        ))
    }

    /// Exposes exactly one compact HTTP attempt without OAuth refresh or retry.
    /// 暴露恰好一次 compact HTTP 尝试，不进行 OAuth 刷新或重试。
    ///
    /// `route`, `request`, and `secret` freeze the admitted request; `wire_request_capture`
    /// optionally records its final encoded body using the ordinary best-effort trace contract,
    /// and `wire_response_capture` optionally retains the same pre-parser response bytes the
    /// ordinary HTTP execution retains.
    /// `route`、`request` 与 `secret` 冻结已准入请求；`wire_request_capture` 可选地通过
    /// 普通尽力追踪契约记录其最终编码正文，`wire_response_capture` 可选地保留与普通 HTTP
    /// 执行相同的解析前响应字节。
    ///
    /// Returns a lazy stream. Session callers must obtain durable admission before polling it,
    /// persist each observation, and obtain a fresh admission before any subsequent attempt.
    /// 返回惰性流。会话调用方必须在轮询前取得可靠准入，持久化每次观测，并在后续尝试前重新准入。
    pub fn stream_responses_compact(
        &self,
        route: ProviderExecutionRoute,
        request: AiExecutionRequest,
        secret: CredentialExecutionSecret,
        wire_request_capture: Option<oneshot::Sender<ProviderWireRequestTraceDispatch>>,
        wire_response_capture: Option<oneshot::Sender<ProviderWireResponseCapture>>,
    ) -> ResponsesCompactStream {
        // client is immutable transport ownership retained until this attempt ends or is dropped.
        // client 是保留至本次尝试结束或析构的不可变传输所有权。
        let client = self.clone();
        // redactionSecret remains outside the stream so every yielded failure is sanitized.
        // redactionSecret 保留在流外，确保每个产出的失败都经过脱敏。
        let redaction_secret = secret.clone();
        // traceStartedAt measures the complete unary compact attempt, including pre-dispatch validation.
        // traceStartedAt 衡量完整的一元压缩尝试，包括分派前校验。
        let trace_started_at = Instant::now();
        // traceId reuses the non-secret cache scope so concurrent session compactions remain distinguishable.
        // traceId 复用不含密钥的缓存作用域，使并发会话压缩仍可区分。
        let trace_id = request
            .cache_scope_id
            .clone()
            .unwrap_or_else(|| String::from("responses-compact"));
        // diagnosticTraceId remains available after the inner stream consumes its own identity.
        // diagnosticTraceId 在内部流取得自身身份后仍可供诊断使用。
        let diagnostic_trace_id = trace_id.clone();
        // stream owns one physical request and suspends after headers for host durability.
        // stream 持有一次物理请求，并在响应头后暂停以等待宿主持久化。
        let stream = try_stream! {
            // wireResponseCapture publishes every raw byte already received on early return,
            // cancellation, and transport failure, exactly like ordinary HTTP execution.
            // wireResponseCapture 与普通 HTTP 执行一样，在提前返回、取消与传输失败时发布已收到的原始字节。
            let mut wire_response_capture = ProviderWireResponseCaptureGuard::new(
                "responses_compact",
                wire_response_capture,
            );
            emit_provider_transport_trace(
                trace_id.as_str(), "responses_compact", "execution_started", &trace_started_at,
            );
            if route.protocol_route.protocol_profile_id != OPENAI_CODEX_RESPONSES_PROFILE_ID {
                let failure = pre_dispatch_failure(
                    "Standalone remote compaction requires the Codex Responses protocol.",
                );
                wire_response_capture.observe_failure("dispatch", &failure);
                Err(failure)?;
            }
            // request freezes the same route-owned tool input contract used by ordinary HTTP and WebSocket dispatch.
            // request 冻结与普通 HTTP 和 WebSocket 分派相同的路由自有工具输入契约。
            let request = request_with_codex_tool_input_contract(&route, &request);
            emit_provider_transport_trace(
                trace_id.as_str(),
                "responses_compact",
                "route_validated",
                &trace_started_at,
            );
            // profile is the exact registered Codex Responses implementation used for both encoding and transport limits.
            // profile 是同时用于编码与传输限制的精确已注册 Codex Responses 实现。
            let transport = client
                .protocols
                .transport_profile(route.protocol_route.protocol_profile_id.as_str())
                .map_err(llm_core_protocol_failure)
                .inspect_err(|failure| wire_response_capture.observe_failure("dispatch", failure))?;
            emit_provider_transport_trace(
                trace_id.as_str(),
                "responses_compact",
                "protocol_ready",
                &trace_started_at,
            );
            validate_request_policy(&route, &request)
                .inspect_err(|failure| wire_response_capture.observe_failure("request_policy", failure))?;
            validate_request_projection_guards_before_encoding(&route, &request)
                .inspect_err(|failure| wire_response_capture.observe_failure("request_policy", failure))?;
            emit_provider_transport_trace(
                trace_id.as_str(),
                "responses_compact",
                "request_policy_validated",
                &trace_started_at,
            );
            // encoded preserves the normal request projection order before the compact-only field filter runs.
            // encoded 在压缩专用字段过滤执行前保留普通请求的投影顺序。
            let encoded = encode_codex_responses_request(&request).map_err(|error| {
                pre_dispatch_failure(format!("LLM core request encoding failed: {error}"))
            })
            .inspect_err(|failure| wire_response_capture.observe_failure("request_encoding", failure))?;
            emit_provider_transport_trace(
                trace_id.as_str(),
                "responses_compact",
                "protocol_encoded",
                &trace_started_at,
            );
            // encoded applies provider-owned model and request overrides without credential material.
            // encoded 应用供应商持有的模型与请求覆盖，但不包含凭据材料。
            let encoded = project_provider_request(&route, &request, encoded)
                .inspect_err(|failure| wire_response_capture.observe_failure("request_projection", failure))?;
            emit_provider_transport_trace(
                trace_id.as_str(),
                "responses_compact",
                "provider_projected",
                &trace_started_at,
            );
            // encoded adds the exact credential projection only after all public request checks pass.
            // encoded 仅在全部公共请求检查通过后加入精确凭据投影。
            let mut encoded = project_credential_request(&route, &secret, encoded)
                .inspect_err(|failure| wire_response_capture.observe_failure("request_projection", failure))?;
            emit_provider_transport_trace(
                trace_id.as_str(),
                "responses_compact",
                "credential_projected",
                &trace_started_at,
            );
            if encoded.path != "/responses" {
                let failure = pre_dispatch_failure(format!(
                    "Codex Responses compaction requires the `/responses` endpoint, got `{}`.",
                    encoded.path
                ));
                wire_response_capture.observe_failure("request_projection", &failure);
                Err(failure)?;
            }
            emit_provider_transport_trace(
                trace_id.as_str(),
                "responses_compact",
                "endpoint_validated",
                &trace_started_at,
            );
            prepare_responses_compact_body(&mut encoded.body)
                .inspect_err(|failure| wire_response_capture.observe_failure("request_projection", failure))?;
            emit_provider_transport_trace(
                trace_id.as_str(),
                "responses_compact",
                "body_prepared",
                &trace_started_at,
            );
            encoded.path = String::from("/responses/compact");
            let url = route.request_url(encoded.path.as_str());
            let headers = execution_headers(
                &route,
                &encoded.headers,
                &secret,
                request.cache_scope_id.as_deref(),
            )
            .inspect_err(|failure| wire_response_capture.observe_failure("request_headers", failure))?;
            emit_provider_transport_trace(
                trace_id.as_str(),
                "responses_compact",
                "headers_ready",
                &trace_started_at,
            );
            // Trace acknowledgement is observational; it does not grant session dispatch permission.
            // 追踪确认仅用于观测，不授予会话分派权限。
            if let Some(capture) = wire_request_capture {
                publish_wire_request_capture(
                    ProviderWireRequestCaptureTarget::Trace(capture),
                    provider_wire_request_capture(
                        "responses_compact", "POST", url.as_str(), &encoded, &headers,
                    ),
                ).await
                .inspect_err(|failure| wire_response_capture.observe_failure("request_trace", failure))?;
            }
            // transport freezes the same profile's response limits for the unary HTTP call.
            // transport 为一元 HTTP 调用冻结同一协议实现的响应限制。
            // compactRequestStartedAt and compactRequestTimeout bound both headers and the complete body.
            // compactRequestStartedAt 与 compactRequestTimeout 同时约束响应头和完整正文。
            let compact_request_started_at = Instant::now();
            let compact_request_timeout = Duration::from_millis(
                transport
                    .stream_idle_timeout_ms
                    .saturating_mul(u64::from(RESPONSES_COMPACT_TIMEOUT_IDLE_MULTIPLIER)),
            );
            emit_provider_transport_trace(
                trace_id.as_str(),
                "responses_compact",
                "transport_ready",
                &trace_started_at,
            );
            emit_provider_transport_trace(
                trace_id.as_str(),
                "responses_compact",
                "request_dispatched",
                &trace_started_at,
            );
            // dispatchedAtMs records this exact attempt rather than a surrounding retry operation.
            // dispatchedAtMs 记录本次精确尝试，而不是外围重试操作。
            let dispatched_at_ms = provider_transport_unix_millis()?;
            let response = tokio::time::timeout(
                Duration::from_millis(transport.response_headers_timeout_ms),
                client.http
                    .post(url.as_str())
                    .headers(headers)
                    .json(&encoded.body)
                    .send(),
            )
            .await
            .map_err(|_| {
                transport_failure(format!(
                    "Provider compaction request to `{url}` exceeded the registered {} ms response-header timeout.",
                    transport.response_headers_timeout_ms
                ))
            })
            .inspect_err(|failure| wire_response_capture.observe_failure("dispatch", failure))?
            .map_err(|error| {
                http_send_transport_failure(
                    &error,
                    format!("Provider compaction request to `{url}` failed: {error}"),
                )
            })
            .inspect_err(|failure| wire_response_capture.observe_failure("dispatch", failure))?;
            emit_provider_transport_trace(
                trace_id.as_str(),
                "responses_compact",
                "response_headers_received",
                &trace_started_at,
            );
            let status = response.status();
            wire_response_capture.observe_http_headers(status.as_u16(), response.headers());
            // Headers are emitted even for 401 so retrying cannot erase a rejected physical request.
            // 即使是 401 也产出响应头，避免重试抹去被拒绝的物理请求。
            yield ResponsesCompactEvent::TransportObserved {
                dispatched_at_ms,
                headers_at_ms: provider_transport_unix_millis()?,
                http_status: status.as_u16(),
                provider_request_id: provider_request_id_from_headers(
                    response.headers(),
                    route.protocol_route.provider_request_id_headers.as_slice(),
                )?,
                rate_limit_headers: allowlisted_rate_limit_headers(response.headers()),
            };
            emit_provider_transport_http_status_trace(
                trace_id.as_str(),
                "responses_compact",
                status.as_u16(),
                &trace_started_at,
            );
            if !status.is_success() {
                let retry_after = parse_retry_after_headers(response.headers(), SystemTime::now());
                let body = read_responses_compact_body_with_timeout(
                    response,
                    transport.max_error_body_bytes,
                    url.as_str(),
                    status.as_u16(),
                    retry_after.clone(),
                    compact_request_started_at,
                    compact_request_timeout,
                    Some(&mut wire_response_capture),
                )
                .await
                .inspect_err(|failure| wire_response_capture.observe_failure("error_response_body", failure))?;
                let body = project_response_record_text(
                    &route.response_projection_profiles,
                    body.as_str(),
                )
                .map_err(protocol_failure)
                .inspect_err(|failure| wire_response_capture.observe_failure("error_response_body", failure))?;
                wire_response_capture.set_outcome("responses_compact_http_error");
                let failure = http_failure(status.as_u16(), retry_after, body.as_str());
                wire_response_capture.observe_failure("headers", &failure);
                Err(failure)?;
            } else {
            let content_type = response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .unwrap_or_default();
            if !content_type
                .split(';')
                .next()
                .is_some_and(|value| value.trim().eq_ignore_ascii_case("application/json"))
            {
                let failure = compact_protocol_failure(format!(
                    "Provider compaction response from `{url}` must use application/json, got `{content_type}`."
                ));
                wire_response_capture.observe_failure("headers", &failure);
                Err(failure)?;
            }
            let body = read_responses_compact_body_with_timeout(
                response,
                transport.max_frame_bytes,
                url.as_str(),
                status.as_u16(),
                None,
                compact_request_started_at,
                compact_request_timeout,
                Some(&mut wire_response_capture),
            )
            .await
            .inspect_err(|failure| wire_response_capture.observe_failure("body_transport", failure))?;
            let mut value: Value = serde_json::from_str(body.as_str()).map_err(|error| {
                compact_protocol_failure(format!(
                    "Provider compaction response from `{url}` is invalid JSON: {error}"
                ))
            })
            .inspect_err(|failure| wire_response_capture.observe_failure("response_projection", failure))?;
            crate::provider_manifest::validate_runtime_json_bounds(
                &value,
                "Provider compaction response",
            )
            .map_err(|error| compact_protocol_failure(error.to_string()))
            .inspect_err(|failure| wire_response_capture.observe_failure("response_projection", failure))?;
            apply_response_projection_profiles(&route.response_projection_profiles, &mut value)
                .map_err(protocol_failure)
                .inspect_err(|failure| wire_response_capture.observe_failure("response_projection", failure))?;
            emit_responses_compact_response_shape_trace(
                trace_id.as_str(),
                &value,
                &trace_started_at,
            );
            let result = decode_responses_compact_result(&value, url.as_str())
                .inspect_err(|failure| wire_response_capture.observe_failure("response_projection", failure))?;
            wire_response_capture.set_outcome_if_unresolved("responses_compact_completed");
            yield ResponsesCompactEvent::Completed(result);
            }
        };
        Box::pin(stream.map(move |result| {
            // result crosses the exact-secret boundary before any caller sees a failure.
            // result 在调用方看到任何失败前跨越精确秘密脱敏边界。
            let result = result
                .map_err(|failure| sanitize_provider_execution_failure(failure, &redaction_secret));
            match &result {
                Ok(ResponsesCompactEvent::Completed(_)) => emit_provider_transport_trace(
                    diagnostic_trace_id.as_str(),
                    "responses_compact",
                    "execution_finished",
                    &trace_started_at,
                ),
                Err(failure) => emit_provider_transport_failure_trace(
                    diagnostic_trace_id.as_str(),
                    "responses_compact",
                    failure,
                    &trace_started_at,
                ),
                Ok(ResponsesCompactEvent::TransportObserved { .. }) => {}
            }
            result
        }))
    }
}

/// Restricts one encoded Codex compaction body to the official `CompactionInput` fields.
/// 将编码后的 Codex 压缩请求体限制为官方 `CompactionInput` 字段集合。
///
/// # Parameters
/// 参数
/// `body` is the protocol-encoded Responses request body before the compact endpoint rewrite.
/// `body` 是改写到 compact 端点前的协议编码 Responses 请求体。
///
/// # Returns
/// 返回值
/// Success after removing non-compact fields while preserving the encoded parallel-call policy, or
/// a pre-dispatch failure when the encoder did not produce an object body.
/// 成功时移除非压缩字段并保留已编码的并行调用策略；编码器未产生对象体时返回分派前失败。
fn prepare_responses_compact_body(body: &mut Value) -> Result<(), ProviderExecutionFailure> {
    let object = body.as_object_mut().ok_or_else(|| {
        pre_dispatch_failure("Codex Responses compaction request body must be a JSON object.")
    })?;
    object.retain(|key, _| {
        matches!(
            key.as_str(),
            "model"
                | "input"
                | "instructions"
                | "tools"
                | "parallel_tool_calls"
                | "reasoning"
                | "service_tier"
                | "prompt_cache_key"
                | "text"
        )
    });
    if object
        .get("instructions")
        .and_then(Value::as_str)
        .is_some_and(str::is_empty)
    {
        object.remove("instructions");
    }
    Ok(())
}

/// Decodes and validates one standalone Responses compact response.
/// 解码并校验一份独立 Responses compact 响应。
///
/// # Parameters
/// 参数
/// `value` is the bounded JSON response body and `url` identifies the exact endpoint in errors.
/// `value` 是已受大小限制的 JSON 响应体，`url` 用于错误中标识精确端点。
///
/// # Returns
/// 返回值
/// A result containing the ordered non-empty `compaction` output items.
/// 返回按原顺序保留的非空 `compaction` 输出项。
fn decode_responses_compact_result(
    value: &Value,
    url: &str,
) -> Result<ResponsesCompactResult, ProviderExecutionFailure> {
    let mut output_items = value
        .get("output")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            compact_protocol_failure(format!(
                "Provider compaction response from `{url}` does not contain an output array."
            ))
        })?
        .clone();
    canonicalize_codex_responses_replay_items(&mut output_items);
    let contains_compaction = output_items
        .iter()
        .any(|item| item.get("type").and_then(Value::as_str) == Some("compaction"));
    if output_items.is_empty()
        || !output_items.iter().all(is_replayable_codex_responses_item)
        || !contains_compaction
    {
        return Err(compact_protocol_failure(format!(
            "Provider compaction response from `{url}` must contain ordered replayable output items and a compaction item."
        )));
    }
    Ok(ResponsesCompactResult {
        output_items,
        usage: value.get("usage").cloned(),
        response_id: value.get("id").and_then(Value::as_str).map(str::to_string),
    })
}

/// Reads one standalone Responses compact body within the official complete-request deadline.
/// 在官方完整请求截止时间内读取一份独立 Responses 压缩正文。
///
/// # Parameters
/// 参数
/// `response`, `max_bytes`, `url`, `status`, and `retry_after` retain the normal bounded body
/// contract; `request_started_at` and `request_timeout` cover the entire dispatched unary request;
/// `capture` optionally retains every raw transport chunk before this reader consumes it.
/// `response`、`max_bytes`、`url`、`status` 与 `retry_after` 保留常规有界正文契约；
/// `request_started_at` 与 `request_timeout` 覆盖完整的已分派一元请求；`capture` 可选地在读取器
/// 消费前保留每个原始传输 chunk。
///
/// # Returns
/// 返回值
/// The complete bounded body, or a structured transport timeout/failure.
/// 返回完整的有界正文，或结构化传输超时/失败。
async fn read_responses_compact_body_with_timeout(
    response: reqwest::Response,
    max_bytes: usize,
    url: &str,
    status: u16,
    retry_after: Option<RetryAfterHint>,
    request_started_at: Instant,
    request_timeout: Duration,
    capture: Option<&mut ProviderWireResponseCaptureGuard>,
) -> Result<String, ProviderExecutionFailure> {
    // remaining prevents a successful header response from resetting the official unary deadline.
    // remaining 避免成功收到响应头后重新计算官方一元请求截止时间。
    let remaining = request_timeout.saturating_sub(request_started_at.elapsed());
    // result separates a complete body, a bounded body-read failure, and the complete deadline.
    // result 区分完整正文、有界正文读取失败与完整请求截止超时。
    let result = tokio::time::timeout(
        remaining,
        read_bounded_response_text_with_capture(
            response,
            max_bytes,
            url,
            status,
            retry_after,
            request_timeout,
            capture,
        ),
    )
    .await;
    match result {
        Ok(Ok(body)) => Ok(body),
        Ok(Err(mut failure)) => {
            if failure.error.category == AiErrorCategory::ProviderUnavailable {
                failure.error.stage = String::from("response_compact");
                failure.error.dispatch_outcome = DispatchOutcome::Dispatched;
            }
            Err(failure)
        }
        Err(_) => Err(compact_transport_failure(format!(
            "Provider compaction request to `{url}` exceeded the registered {} ms complete-response timeout.",
            request_timeout.as_millis()
        ))),
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_negative_chain;
