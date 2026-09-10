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
use super::response_body::read_bounded_response_text;
use super::response_metadata::parse_retry_after_headers;
use super::transport_diagnostics::{
    emit_provider_transport_failure_trace, emit_provider_transport_http_status_trace,
    emit_provider_transport_trace, emit_responses_compact_response_shape_trace,
};
use super::{ProviderExecutionClient, ProviderExecutionRoute};
use crate::CredentialExecutionSecret;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::{Duration, Instant, SystemTime};
use vulcan_ai_contract::{
    AiErrorCategory, AiExecutionRequest, DispatchOutcome, RetryAfterHint,
    canonicalize_codex_responses_replay_items, is_replayable_codex_responses_item,
};
use vulcan_ai_protocols::OPENAI_CODEX_RESPONSES_PROFILE_ID;
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
        // traceStartedAt measures the complete unary compact attempt, including pre-dispatch validation.
        // traceStartedAt 衡量完整的一元压缩尝试，包括分派前校验。
        let trace_started_at = Instant::now();
        // traceId reuses the non-secret cache scope so concurrent session compactions remain distinguishable.
        // traceId 复用不含密钥的缓存作用域，使并发会话压缩仍可区分。
        let trace_id = request
            .cache_scope_id
            .clone()
            .unwrap_or_else(|| String::from("responses-compact"));
        emit_provider_transport_trace(
            trace_id.as_str(),
            "responses_compact",
            "execution_started",
            &trace_started_at,
        );
        // result keeps every early failure inside the same secret-safe diagnostic boundary.
        // result 将所有提前失败保留在同一个不含密钥的诊断边界内。
        let result = async {
            if route.protocol_route.protocol_profile_id != OPENAI_CODEX_RESPONSES_PROFILE_ID {
                return Err(pre_dispatch_failure(
                    "Standalone remote compaction requires the Codex Responses protocol.",
                ));
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
            let transport = self
                .protocols
                .transport_profile(route.protocol_route.protocol_profile_id.as_str())
                .map_err(llm_core_protocol_failure)?;
            emit_provider_transport_trace(
                trace_id.as_str(),
                "responses_compact",
                "protocol_ready",
                &trace_started_at,
            );
            validate_request_policy(&route, &request)?;
            validate_request_projection_guards_before_encoding(&route, &request)?;
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
            })?;
            emit_provider_transport_trace(
                trace_id.as_str(),
                "responses_compact",
                "protocol_encoded",
                &trace_started_at,
            );
            // encoded applies provider-owned model and request overrides without credential material.
            // encoded 应用供应商持有的模型与请求覆盖，但不包含凭据材料。
            let encoded = project_provider_request(&route, &request, encoded)?;
            emit_provider_transport_trace(
                trace_id.as_str(),
                "responses_compact",
                "provider_projected",
                &trace_started_at,
            );
            // encoded adds the exact credential projection only after all public request checks pass.
            // encoded 仅在全部公共请求检查通过后加入精确凭据投影。
            let mut encoded = project_credential_request(&route, &secret, encoded)?;
            emit_provider_transport_trace(
                trace_id.as_str(),
                "responses_compact",
                "credential_projected",
                &trace_started_at,
            );
            if encoded.path != "/responses" {
                return Err(pre_dispatch_failure(format!(
                    "Codex Responses compaction requires the `/responses` endpoint, got `{}`.",
                    encoded.path
                )));
            }
            emit_provider_transport_trace(
                trace_id.as_str(),
                "responses_compact",
                "endpoint_validated",
                &trace_started_at,
            );
            prepare_responses_compact_body(&mut encoded.body)?;
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
            )?;
            emit_provider_transport_trace(
                trace_id.as_str(),
                "responses_compact",
                "headers_ready",
                &trace_started_at,
            );
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
            let response = tokio::time::timeout(
                Duration::from_millis(transport.response_headers_timeout_ms),
                self.http
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
            })?
            .map_err(|error| {
                http_send_transport_failure(
                    &error,
                    format!("Provider compaction request to `{url}` failed: {error}"),
                )
            })?;
            emit_provider_transport_trace(
                trace_id.as_str(),
                "responses_compact",
                "response_headers_received",
                &trace_started_at,
            );
            let status = response.status();
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
                )
                .await?;
                let body = project_response_record_text(
                    &route.response_projection_profiles,
                    body.as_str(),
                )
                .map_err(protocol_failure)?;
                return Err(http_failure(status.as_u16(), retry_after, body.as_str()));
            }
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
                return Err(compact_protocol_failure(format!(
                    "Provider compaction response from `{url}` must use application/json, got `{content_type}`."
                )));
            }
            let body = read_responses_compact_body_with_timeout(
                response,
                transport.max_frame_bytes,
                url.as_str(),
                status.as_u16(),
                None,
                compact_request_started_at,
                compact_request_timeout,
            )
            .await?;
            let mut value: Value = serde_json::from_str(body.as_str()).map_err(|error| {
                compact_protocol_failure(format!(
                    "Provider compaction response from `{url}` is invalid JSON: {error}"
                ))
            })?;
            crate::provider_manifest::validate_runtime_json_bounds(
                &value,
                "Provider compaction response",
            )
            .map_err(|error| compact_protocol_failure(error.to_string()))?;
            apply_response_projection_profiles(&route.response_projection_profiles, &mut value)
                .map_err(protocol_failure)?;
            emit_responses_compact_response_shape_trace(
                trace_id.as_str(),
                &value,
                &trace_started_at,
            );
            decode_responses_compact_result(&value, url.as_str())
        }
        .await;
        // result crosses the same exact-secret redaction boundary as streaming execution errors.
        // result 与流式执行错误经过同一条精确秘密脱敏边界。
        let result =
            result.map_err(|failure| sanitize_provider_execution_failure(failure, &secret));
        match &result {
            Ok(_) => emit_provider_transport_trace(
                trace_id.as_str(),
                "responses_compact",
                "execution_finished",
                &trace_started_at,
            ),
            Err(failure) => emit_provider_transport_failure_trace(
                trace_id.as_str(),
                "responses_compact",
                failure,
                &trace_started_at,
            ),
        }
        result
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
/// contract; `request_started_at` and `request_timeout` cover the entire dispatched unary request.
/// `response`、`max_bytes`、`url`、`status` 与 `retry_after` 保留常规有界正文契约；
/// `request_started_at` 与 `request_timeout` 覆盖完整的已分派一元请求。
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
) -> Result<String, ProviderExecutionFailure> {
    // remaining prevents a successful header response from resetting the official unary deadline.
    // remaining 避免成功收到响应头后重新计算官方一元请求截止时间。
    let remaining = request_timeout.saturating_sub(request_started_at.elapsed());
    // result separates a complete body, a bounded body-read failure, and the complete deadline.
    // result 区分完整正文、有界正文读取失败与完整请求截止超时。
    let result = tokio::time::timeout(
        remaining,
        read_bounded_response_text(
            response,
            max_bytes,
            url,
            status,
            retry_after,
            request_timeout,
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
