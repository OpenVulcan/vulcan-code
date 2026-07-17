//! Streaming turn execution for the direct Google Gemini transport.
//! Google Gemini 直连传输的流式回合执行逻辑。

use super::client::{build_google_gemini_http_client, build_google_gemini_streaming_endpoint};
use super::payload::build_google_gemini_request_payload;
use super::response::process_gemini_candidate;
use super::tool_conversion::gemini_function_call_to_core_tool_call;
use super::util::current_unix_millis_text;
use crate::agent_runtime::provider_protocol_anthropic_decoders::sse::iter_sse_messages;
use crate::agent_runtime::provider_protocol_content::{
    ProviderAssistantContent, ProviderReasoningPart,
};
use crate::agent_runtime::provider_protocol_gemini::{
    GeminiContent, GeminiPart, GeminiStreamingChunk, build_scoped_gemini_provider_message_state,
};
use crate::agent_runtime::provider_protocol_tool_definition::ProviderToolDefinition;
use crate::agent_runtime::provider_transport_http_support::{
    ensure_json_content_type, ensure_string_header, format_standard_error_response_with_headers,
};
use crate::agent_runtime::provider_transport_live_delta::ProviderLiveDeltaSender;
use crate::agent_runtime::provider_transport_turn_request::OpenAiCompatibleCoreTurnRequest;
use crate::agent_runtime::provider_transport_wire_capture::{
    emit_provider_wire_request, provider_request_dry_run_active, provider_request_dry_run_response,
};
use futures_util::StreamExt;
use reqwest::header::HeaderMap;
use std::collections::HashMap;
use vulcan_core::provider::VulcanProviderDeltaSink;
use vulcan_core::types::{
    CoreError as VulcanCoreError, ProviderContinuation as CoreProviderContinuation,
    ProviderTurnResponse as CoreProviderTurnResponse,
    ProviderUsageSnapshot as CoreProviderUsageSnapshot,
};

/// Executes one direct Google Gemini streaming turn without going through any compatibility transport layer.
/// 在不经过任何兼容传输层的前提下执行一次直连 Google Gemini 流式回合。
pub(crate) async fn execute_google_gemini_http_streaming_turn<S>(
    provider_id: &str,
    api_key: &str,
    base_url: Option<&str>,
    model_id: &str,
    request_headers: HeaderMap,
    request: OpenAiCompatibleCoreTurnRequest,
    provider_tool_definitions: &[ProviderToolDefinition],
    live_sender: &ProviderLiveDeltaSender,
    sink: &mut S,
) -> Result<CoreProviderTurnResponse, VulcanCoreError>
where
    S: VulcanProviderDeltaSink,
{
    let mut wire_headers = request_headers.clone();
    ensure_string_header(
        provider_id,
        "Google Gemini",
        &mut wire_headers,
        "x-goog-api-key",
        api_key,
        "API Key ",
    )?;
    ensure_json_content_type(&mut wire_headers);
    let client = build_google_gemini_http_client(provider_id, api_key, request_headers)?;
    let endpoint = build_google_gemini_streaming_endpoint(base_url, model_id);
    let payload = build_google_gemini_request_payload(request, provider_tool_definitions)?;
    let dry_run = provider_request_dry_run_active(api_key);
    let wire_request = emit_provider_wire_request(
        live_sender,
        provider_id,
        model_id,
        "POST",
        endpoint.as_str(),
        &wire_headers,
        &payload,
        dry_run,
    );
    if let Some(response) = provider_request_dry_run_response(&wire_request, dry_run) {
        return Ok(response);
    }

    let mut response = client
        .post(endpoint)
        .json(&payload)
        .send()
        .await
        .map_err(|error| VulcanCoreError::ProviderFailure {
            provider_id: provider_id.to_string(),
            message: format!("Google Gemini 请求发送失败：{error}"),
        })?;

    if !response.status().is_success() {
        let status = response.status();
        let headers = response.headers().clone();
        let body = response.text().await.unwrap_or_default();
        let message = format_standard_error_response_with_headers(status, &headers, &body);
        return Err(VulcanCoreError::ProviderFailure {
            provider_id: provider_id.to_string(),
            message: format!("Google Gemini request failed: {message}"),
        });
    }

    let byte_stream = Box::pin(async_stream::stream! {
        loop {
            match response.chunk().await {
                Ok(Some(chunk)) => yield Ok::<Vec<u8>, std::io::Error>(chunk.to_vec()),
                Ok(None) => break,
                Err(error) => {
                    yield Err(std::io::Error::other(error.to_string()));
                    break;
                }
            }
        }
    });

    let events = iter_sse_messages(byte_stream);
    futures_util::pin_mut!(events);

    let mut response_model = None::<String>;
    let mut final_usage = None::<CoreProviderUsageSnapshot>;
    let mut assistant_text = String::new();
    let mut finalized_parts = Vec::<GeminiPart>::new();
    let mut pending_reasoning_index = None::<usize>;
    let mut function_part_indices = HashMap::<String, usize>::new();

    while let Some(event_result) = events.next().await {
        let event = event_result.map_err(|error| VulcanCoreError::ProviderFailure {
            provider_id: provider_id.to_string(),
            message: format!("Google Gemini SSE 解码失败：{error}"),
        })?;

        if event.data.trim().is_empty() {
            continue;
        }

        let chunk = serde_json::from_str::<GeminiStreamingChunk>(&event.data).map_err(|error| {
            VulcanCoreError::ProviderFailure {
                provider_id: provider_id.to_string(),
                message: format!("Google Gemini 流式事件解析失败：{error}"),
            }
        })?;

        if let Some(model_version) = chunk.model_version {
            response_model = Some(model_version);
        }
        if let Some(usage) = chunk
            .usage_metadata
            .as_ref()
            .and_then(gemini_usage_to_snapshot)
        {
            final_usage = Some(usage);
        }
        let Some(candidate) = chunk.candidates.into_iter().next() else {
            continue;
        };

        process_gemini_candidate(
            provider_id,
            candidate,
            live_sender,
            sink,
            &mut assistant_text,
            &mut finalized_parts,
            &mut pending_reasoning_index,
            &mut function_part_indices,
            provider_tool_definitions,
        )?;
    }

    let assistant_reasoning = finalized_parts
        .iter()
        .filter_map(|part| {
            part.is_reasoning()
                .then(|| part.text.clone().unwrap_or_default())
                .filter(|text| !text.is_empty())
                .map(ProviderReasoningPart::Text)
        })
        .collect::<Vec<_>>();
    let tool_calls = finalized_parts
        .iter()
        .filter_map(|part| {
            part.function_call
                .as_ref()
                .map(gemini_function_call_to_core_tool_call)
        })
        .collect::<Vec<_>>();

    let mut assistant_content = Vec::new();
    if !assistant_text.is_empty() {
        assistant_content.push(ProviderAssistantContent::Text(assistant_text));
    }
    if !assistant_reasoning.is_empty() {
        assistant_content.push(ProviderAssistantContent::Reasoning(assistant_reasoning));
    }
    assistant_content.extend(
        tool_calls
            .iter()
            .cloned()
            .map(ProviderAssistantContent::ToolCall),
    );

    let provider_message_state = (!finalized_parts.is_empty()).then(|| {
        build_scoped_gemini_provider_message_state(
            provider_id,
            model_id,
            &GeminiContent::model(finalized_parts),
        )
    });

    Ok(CoreProviderTurnResponse {
        assistant_text: assistant_content
            .iter()
            .filter_map(|item| match item {
                ProviderAssistantContent::Text(text) => Some(text.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(""),
        assistant_reasoning: assistant_content
            .iter()
            .filter_map(|item| match item {
                ProviderAssistantContent::Reasoning(reasoning) => Some(
                    reasoning
                        .iter()
                        .filter_map(|part| match part {
                            ProviderReasoningPart::Text(text)
                            | ProviderReasoningPart::Summary(text) => Some(text.clone()),
                            ProviderReasoningPart::Opaque => None,
                        })
                        .collect::<Vec<_>>()
                        .join("\n"),
                ),
                _ => None,
            })
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        provider_message_id: None,
        provider_message_state,
        usage: final_usage,
        tool_calls,
        continuation: Some(CoreProviderContinuation {
            provider_id: Some(provider_id.to_string()),
            model_id: Some(response_model.unwrap_or_else(|| model_id.to_string())),
            last_provider_message_id: None,
            updated_at: Some(current_unix_millis_text()),
        }),
    })
}

/// Converts one Gemini-native usage payload into the shared normalized usage snapshot.
/// 将一份 Gemini 原生用量载荷转换为共享规范化用量快照。
fn gemini_usage_to_snapshot(
    usage: &crate::agent_runtime::provider_protocol_gemini::GeminiUsageMetadata,
) -> Option<CoreProviderUsageSnapshot> {
    let snapshot = CoreProviderUsageSnapshot {
        input_tokens: (usage.prompt_token_count >= 0).then_some(usage.prompt_token_count as usize),
        output_tokens: usage
            .candidates_token_count
            .filter(|value| *value >= 0)
            .map(|value| value as usize),
        reasoning_tokens: usage
            .thoughts_token_count
            .filter(|value| *value >= 0)
            .map(|value| value as usize),
        total_tokens: usage
            .total_token_count
            .filter(|value| *value >= 0)
            .map(|value| value as usize),
        cached_input_tokens: usage
            .cached_content_token_count
            .filter(|value| *value >= 0)
            .map(|value| value as usize),
        cache_creation_input_tokens: None,
    };
    snapshot.has_any_tokens().then_some(snapshot)
}
