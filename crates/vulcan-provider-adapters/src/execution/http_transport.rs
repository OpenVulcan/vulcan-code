//! Immutable HTTP streaming execution and pre-parser wire diagnostics.
//! 不可变 HTTP 流式执行与解析前线路诊断。

use super::failure::{
    http_failure, http_send_transport_failure, llm_core_protocol_failure, protocol_failure,
    redact_exact_credential, stream_completion_failure, stream_idle_timeout_failure,
    stream_protocol_failure, stream_transport_failure, transport_failure,
};
use super::framing::IncrementalFrameParser;
use super::projection_profiles::{project_response_record, project_response_record_text};
use super::request_encoding::request_with_codex_tool_input_contract;
use super::request_encoding::request_without_nonpersistent_responses_continuation;
use super::request_headers::execution_headers;
use super::response_body::read_bounded_response_text_with_capture;
use super::response_metadata::{
    allowlisted_rate_limit_headers, parse_retry_after_headers, provider_request_id_from_headers,
    validate_response_content_type,
};
use super::stream_state::{update_semantic_output_observation, validate_stream_event};
use super::transport_diagnostics::{emit_provider_transport_trace, provider_transport_unix_millis};
use super::usage::{
    ProfiledUsageEventGate, extract_profiled_usage_events, extract_profiled_usage_record_events,
};
use super::wire_capture::{
    ProviderWireCaptureTargets, ProviderWireResponseCaptureGuard, provider_wire_request_capture,
    publish_wire_request_capture,
};
use super::{ProviderExecutionClient, ProviderExecutionRoute, ProviderExecutionStream};
use crate::CredentialExecutionSecret;
use async_stream::try_stream;
use futures_util::StreamExt;
use std::time::{Duration, Instant, SystemTime};
use vulcan_ai_contract::{
    AiExecutionRequest, AiStreamEvent, AiStreamStateMachine, AiTransportKind,
};
use vulcan_llm_core::{UsageAccumulator, UsageRecordKind};

impl ProviderExecutionClient {
    /// Starts one immutable HTTP SSE execution without transport fallback.
    /// 启动一次不可变 HTTP SSE 执行，不进行传输回退。
    pub(super) fn execute_http(
        &self,
        route: ProviderExecutionRoute,
        request: AiExecutionRequest,
        secret: CredentialExecutionSecret,
        execution_id: String,
        wire_captures: ProviderWireCaptureTargets,
    ) -> ProviderExecutionStream {
        let client = self.clone();
        Box::pin(try_stream! {
            let ProviderWireCaptureTargets {
                request: wire_request_capture,
                response: wire_response_capture,
            } = wire_captures;
            // wireRequestCapture remains available until the HTTP request is ready for dispatch.
            // wireRequestCapture 保留到 HTTP 请求准备分派时使用。
            let mut wire_request_capture = wire_request_capture;
            // wireResponseCaptureGuard publishes partial bytes on every early-return or cancellation path.
            // wireResponseCaptureGuard 会在任意提前返回或取消路径上发布部分字节。
            let mut wire_response_capture = ProviderWireResponseCaptureGuard::new(
                "http_sse",
                wire_response_capture,
            );
            let transport_started_at = Instant::now();
            emit_provider_transport_trace(
                execution_id.as_str(),
                "http_sse",
                "execution_started",
                &transport_started_at,
            );
            let request = request_without_nonpersistent_responses_continuation(&route, &request);
            // request freezes the same model-owned tool input contract for encoding and decoding.
            // request 为编码与解码冻结同一份模型自有工具输入契约。
            let request = request_with_codex_tool_input_contract(&route, &request);
            let encoded = client
                .encode_request_with_credential(&route, &request, &secret)
                .inspect_err(|failure| wire_response_capture.observe_failure("dispatch", failure))?;
            let response_profiles = route.response_projection_profiles.clone();
            let transport = client
                .protocols
                .transport_profile(route.protocol_route.protocol_profile_id.as_str())
                .map_err(llm_core_protocol_failure)
                .inspect_err(|failure| wire_response_capture.observe_failure("dispatch", failure))?;
            let url = route.request_url(encoded.path.as_str());
            let headers = execution_headers(
                &route,
                &encoded.headers,
                &secret,
                request.cache_scope_id.as_deref(),
            )
                .inspect_err(|failure| wire_response_capture.observe_failure("dispatch", failure))?;
            // The capture contains the exact body and the final headers after transport projection.
            // 捕获内容包含精确正文以及完成传输投影后的最终请求头。
            if let Some(capture) = wire_request_capture.take() {
                publish_wire_request_capture(
                    capture,
                    provider_wire_request_capture(
                        "http_sse",
                        "POST",
                        url.as_str(),
                        &encoded,
                        &headers,
                    ),
                )
                .await
                .inspect_err(|failure| wire_response_capture.observe_failure("dispatch", failure))?;
            }
            emit_provider_transport_trace(
                execution_id.as_str(),
                "http_sse",
                "request_dispatched",
                &transport_started_at,
            );
            let dispatched_at_ms = provider_transport_unix_millis()
                .inspect_err(|failure| wire_response_capture.observe_failure("dispatch", failure))?;
            let response = tokio::time::timeout(
                Duration::from_millis(transport.response_headers_timeout_ms),
                client
                    .http
                    .post(url.as_str())
                    .headers(headers)
                    .json(&encoded.body)
                    .send(),
            )
            .await
            .map_err(|_| {
                let failure = transport_failure(format!(
                    "Provider request to `{url}` exceeded the registered {} ms response-header timeout.",
                    transport.response_headers_timeout_ms
                ));
                wire_response_capture.observe_failure("dispatch", &failure);
                failure
            })?
            .map_err(|error| {
                let failure = http_send_transport_failure(
                    &error,
                    format!("Provider request to `{url}` failed: {error}"),
                );
                wire_response_capture.observe_failure("dispatch", &failure);
                failure
            })?;
            emit_provider_transport_trace(
                execution_id.as_str(),
                "http_sse",
                "response_headers_received",
                &transport_started_at,
            );
            let status = response.status();
            wire_response_capture.observe_http_headers(status.as_u16(), response.headers());
            let headers_at_ms = provider_transport_unix_millis()
                .inspect_err(|failure| wire_response_capture.observe_failure("headers", failure))?;
            let rate_limit_headers = allowlisted_rate_limit_headers(response.headers());
            let provider_request_id = provider_request_id_from_headers(
                response.headers(),
                route.protocol_route.provider_request_id_headers.as_slice(),
            )
            .inspect_err(|failure| wire_response_capture.observe_failure("headers", failure))?;
            yield AiStreamEvent::TransportObserved {
                transport: AiTransportKind::Http,
                dispatched_at_ms,
                headers_at_ms: Some(headers_at_ms),
                http_status: Some(status.as_u16()),
                provider_request_id,
                rate_limit_headers,
            };
            // usage_profiles and accumulator are initialized before status handling so failed bodies can report billable usage.
            // usage_profiles 与累加器在状态处理前初始化，使失败正文也能报告已计费用量。
            let usage_profiles = route.protocol_route.usage_extraction_profiles.clone();
            let profiled_usage_enabled = !usage_profiles.is_empty();
            let mut usage_accumulator = UsageAccumulator::default();
            let response = if status.is_success() {
                response
            } else {
                let retry_after =
                    parse_retry_after_headers(response.headers(), SystemTime::now());
                let body = read_bounded_response_text_with_capture(
                    response,
                    transport.max_error_body_bytes,
                    url.as_str(),
                    status.as_u16(),
                    retry_after.clone(),
                    Duration::from_millis(transport.stream_idle_timeout_ms),
                    Some(&mut wire_response_capture),
                )
                .await
                .inspect_err(|failure| {
                    wire_response_capture.observe_failure("body_transport", failure);
                })?;
                for usage_event in extract_profiled_usage_events(
                    &usage_profiles,
                    UsageRecordKind::ErrorJsonBody,
                    body.as_str(),
                    &mut usage_accumulator,
                )
                .map_err(|error| {
                    let failure = protocol_failure(error);
                    wire_response_capture.observe_failure("response_projection", &failure);
                    failure
                })?
                {
                    yield usage_event;
                }
                let body = project_response_record_text(&response_profiles, body.as_str())
                    .map_err(|error| {
                        let failure = protocol_failure(error);
                        wire_response_capture.observe_failure("response_projection", &failure);
                        failure
                    })?;
                wire_response_capture.set_outcome("http_error_response");
                // The exact credential is removed before classification bounds any field, so a
                // truncated field can never keep a credential fragment.
                // 在分类限长任何字段之前移除精确凭据，使截断后的字段不会残留凭据片段。
                let failure = http_failure(
                    status.as_u16(),
                    retry_after,
                    redact_exact_credential(body.as_str(), &secret).as_str(),
                );
                wire_response_capture.observe_failure("headers", &failure);
                Err(failure)?
            };
            validate_response_content_type(
                response.headers(),
                &transport,
                url.as_str(),
            )
            .inspect_err(|failure| {
                wire_response_capture.observe_failure("headers", failure);
            })?;
            let mut decoder = client
                .protocols
                .create_stream_decoder(
                    route.protocol_route.protocol_profile_id.as_str(),
                    execution_id.clone(),
                    &request,
                )
                .map_err(|error| {
                    let failure = llm_core_protocol_failure(error);
                    wire_response_capture.observe_failure("protocol_decoder", &failure);
                    failure
                })?;
            let mut parser = IncrementalFrameParser::new(transport);
            let mut bytes = response.bytes_stream();
            let mut profiled_usage_gate = ProfiledUsageEventGate::default();
            let mut semantic_output_observed = false;
            let mut first_chunk_observed = false;
            let mut stream_state = AiStreamStateMachine::default();
            loop {
                let next_chunk = tokio::time::timeout(
                    Duration::from_millis(transport.stream_idle_timeout_ms),
                    bytes.next(),
                )
                .await
                .map_err(|_| {
                    let failure = stream_idle_timeout_failure(
                        format!(
                            "Provider stream from `{url}` exceeded the registered {} ms idle timeout.",
                            transport.stream_idle_timeout_ms
                        ),
                        semantic_output_observed,
                    );
                    wire_response_capture.observe_failure("body_transport", &failure);
                    failure
                })?;
                let Some(chunk) = next_chunk else {
                    break;
                };
                let chunk = chunk.map_err(|error| {
                    let failure = stream_transport_failure(
                        format!("Provider stream from `{url}` failed: {error}"),
                        semantic_output_observed,
                    );
                    wire_response_capture.observe_failure("body_transport", &failure);
                    failure
                })?;
                wire_response_capture.observe_chunk(chunk.as_ref());
                if !first_chunk_observed {
                    first_chunk_observed = true;
                    emit_provider_transport_trace(
                        execution_id.as_str(),
                        "http_sse",
                        "first_chunk_received",
                        &transport_started_at,
                    );
                }
                for frame in parser
                    .push(chunk.as_ref())
                    .map_err(|error| {
                        let failure = stream_protocol_failure(error, semantic_output_observed);
                        wire_response_capture.observe_failure("frame_parser", &failure);
                        failure
                    })?
                {
                    let projected = project_response_record(
                        &response_profiles,
                        frame.data.as_str(),
                    )
                    .map_err(|error| {
                        let failure = stream_protocol_failure(error, semantic_output_observed);
                        wire_response_capture.observe_failure("response_projection", &failure);
                        failure
                    })?;
                    let profiled_usage_events = extract_profiled_usage_record_events(
                        &usage_profiles,
                        UsageRecordKind::SseEvent,
                        &projected.text,
                        projected.value.as_ref(),
                        &mut usage_accumulator,
                    )
                    .map_err(|error| {
                        let failure = stream_protocol_failure(error, semantic_output_observed);
                        wire_response_capture.observe_failure("response_projection", &failure);
                        failure
                    })?;
                    for mut usage_event in profiled_usage_gate
                        .stage(profiled_usage_events)
                        .map_err(|error| {
                            stream_protocol_failure(error, semantic_output_observed)
                        })
                        .inspect_err(|failure| {
                            wire_response_capture.observe_failure("response_projection", failure);
                        })?
                    {
                        validate_stream_event(
                            &mut stream_state,
                            &usage_event,
                            semantic_output_observed,
                        )
                        .inspect_err(|failure| {
                            wire_response_capture.observe_failure("protocol_decoder", failure);
                        })?;
                        update_semantic_output_observation(
                            &mut semantic_output_observed,
                            &mut usage_event,
                        );
                        yield usage_event;
                    }
                    for mut event in decoder
                        .push_sse(frame.event_name.as_deref(), &projected.text)
                        .map_err(|error| {
                            let failure =
                                stream_protocol_failure(error, semantic_output_observed);
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
                                "http_sse",
                                "first_semantic_event",
                                &transport_started_at,
                            );
                        }
                        if matches!(
                            event,
                            AiStreamEvent::ResponseCompleted | AiStreamEvent::ResponseFailed { .. }
                        ) {
                            emit_provider_transport_trace(
                                execution_id.as_str(),
                                "http_sse",
                                "terminal_event",
                                &transport_started_at,
                            );
                        }
                        validate_stream_event(&mut stream_state, &event, semantic_output_observed)
                            .inspect_err(|failure| {
                                wire_response_capture.observe_failure("protocol_decoder", failure);
                            })?;
                        // terminalCapture must be fixed before yielding because the runtime stops polling at terminal events.
                        // terminalCapture 必须在交付前固定，因为运行时会在终态事件处停止拉流。
                        wire_response_capture.observe_normalized_event(&event);
                        let released_usage = profiled_usage_gate.observe(&event);
                        yield event;
                        for mut usage_event in released_usage {
                            validate_stream_event(
                                &mut stream_state,
                                &usage_event,
                                semantic_output_observed,
                            )
                            .inspect_err(|failure| {
                                wire_response_capture.observe_failure("protocol_decoder", failure);
                            })?;
                            update_semantic_output_observation(
                                &mut semantic_output_observed,
                                &mut usage_event,
                            );
                            yield usage_event;
                        }
                    }
                }
            }
            for frame in parser
                .finish()
                .map_err(|error| {
                    let failure = stream_protocol_failure(error, semantic_output_observed);
                    wire_response_capture.observe_failure("frame_parser", &failure);
                    failure
                })?
            {
                let projected = project_response_record(
                    &response_profiles,
                    frame.data.as_str(),
                )
                .map_err(|error| stream_protocol_failure(error, semantic_output_observed))
                .inspect_err(|failure| {
                    wire_response_capture.observe_failure("response_projection", failure);
                })?;
                let profiled_usage_events = extract_profiled_usage_record_events(
                    &usage_profiles,
                    UsageRecordKind::SseEvent,
                    &projected.text,
                    projected.value.as_ref(),
                    &mut usage_accumulator,
                )
                .map_err(|error| {
                    let failure = stream_protocol_failure(error, semantic_output_observed);
                    wire_response_capture.observe_failure("response_projection", &failure);
                    failure
                })?;
                for mut usage_event in profiled_usage_gate
                    .stage(profiled_usage_events)
                    .map_err(|error| stream_protocol_failure(error, semantic_output_observed))
                    .inspect_err(|failure| {
                        wire_response_capture.observe_failure("response_projection", failure);
                    })?
                {
                    validate_stream_event(
                        &mut stream_state,
                        &usage_event,
                        semantic_output_observed,
                    )
                    .inspect_err(|failure| {
                        wire_response_capture.observe_failure("protocol_decoder", failure);
                    })?;
                    update_semantic_output_observation(
                        &mut semantic_output_observed,
                        &mut usage_event,
                    );
                    yield usage_event;
                }
                for mut event in decoder
                    .push_sse(frame.event_name.as_deref(), &projected.text)
                    .map_err(|error| {
                        let failure = stream_protocol_failure(error, semantic_output_observed);
                        wire_response_capture.observe_failure("protocol_decoder", &failure);
                        failure
                    })?
                {
                    if profiled_usage_enabled
                        && matches!(event, AiStreamEvent::UsageUpdated { .. })
                    {
                        continue;
                    }
                    update_semantic_output_observation(
                        &mut semantic_output_observed,
                        &mut event,
                    );
                    validate_stream_event(&mut stream_state, &event, semantic_output_observed)
                        .inspect_err(|failure| {
                            wire_response_capture.observe_failure("protocol_decoder", failure);
                        })?;
                    // terminalCapture must be fixed before yielding because the runtime stops polling at terminal events.
                    // terminalCapture 必须在交付前固定，因为运行时会在终态事件处停止拉流。
                    wire_response_capture.observe_normalized_event(&event);
                    let released_usage = profiled_usage_gate.observe(&event);
                    yield event;
                    for mut usage_event in released_usage {
                        validate_stream_event(
                            &mut stream_state,
                            &usage_event,
                            semantic_output_observed,
                        )
                        .inspect_err(|failure| {
                            wire_response_capture.observe_failure("protocol_decoder", failure);
                        })?;
                        update_semantic_output_observation(
                            &mut semantic_output_observed,
                            &mut usage_event,
                        );
                        yield usage_event;
                    }
                }
            }
            for mut event in decoder
                .finish()
                .map_err(|error| {
                    let failure = stream_completion_failure(error, semantic_output_observed);
                    wire_response_capture.observe_failure("protocol_decoder", &failure);
                    failure
                })?
            {
                if profiled_usage_enabled && matches!(event, AiStreamEvent::UsageUpdated { .. }) {
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
                        "http_sse",
                        "first_semantic_event",
                        &transport_started_at,
                    );
                }
                if matches!(
                    event,
                    AiStreamEvent::ResponseCompleted | AiStreamEvent::ResponseFailed { .. }
                ) {
                    emit_provider_transport_trace(
                        execution_id.as_str(),
                        "http_sse",
                        "terminal_event",
                        &transport_started_at,
                    );
                }
                validate_stream_event(&mut stream_state, &event, semantic_output_observed)
                    .inspect_err(|failure| {
                        wire_response_capture.observe_failure("protocol_decoder", failure);
                    })?;
                // terminalCapture must be fixed before yielding because the runtime stops polling at terminal events.
                // terminalCapture 必须在交付前固定，因为运行时会在终态事件处停止拉流。
                wire_response_capture.observe_normalized_event(&event);
                let released_usage = profiled_usage_gate.observe(&event);
                yield event;
                for mut usage_event in released_usage {
                    validate_stream_event(
                        &mut stream_state,
                        &usage_event,
                        semantic_output_observed,
                    )
                    .inspect_err(|failure| {
                        wire_response_capture.observe_failure("protocol_decoder", failure);
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
                .map_err(|error| stream_completion_failure(error, semantic_output_observed))
                .inspect_err(|failure| {
                    wire_response_capture.observe_failure("protocol_decoder", failure);
                })?;
            emit_provider_transport_trace(
                execution_id.as_str(),
                "http_sse",
                "execution_finished",
                &transport_started_at,
            );
            wire_response_capture.set_outcome_if_unresolved("completed");
        })
    }
}

#[cfg(test)]
mod tests;
