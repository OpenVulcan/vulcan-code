//! Direct OpenAI Responses HTTP and SSE transport owned by the host runtime.
//! 宿主运行时自有的 OpenAI Responses HTTP 与 SSE 直连传输模块。

use crate::agent_runtime::provider_attachment::{
    build_missing_attachment_replay_text, read_provider_attachment,
};
use crate::agent_runtime::provider_prompt_layers::provider_fallback_system_prompt;
use crate::agent_runtime::provider_protocol_anthropic_decoders::sse::iter_sse_messages;
use crate::agent_runtime::provider_protocol_bridge::{
    build_openai_compatible_turn_response, core_provider_turn_delta_to_runtime_delta,
};
use crate::agent_runtime::provider_protocol_content::{
    ProviderAssistantContent, ProviderReasoningPart,
};
use crate::agent_runtime::provider_protocol_tool_definition::{
    ProviderToolDefinition, normalize_provider_tool_parameters_schema,
};
use crate::agent_runtime::provider_transport_http_support::{
    build_json_http_client, build_rest_endpoint, ensure_bearer_authorization_header,
    ensure_json_content_type, format_standard_error_response,
};
use crate::agent_runtime::provider_transport_live_delta::ProviderLiveDeltaSender;
use crate::agent_runtime::provider_transport_turn_request::OpenAiCompatibleCoreTurnRequest;
use crate::agent_runtime::provider_transport_wire_capture::{
    emit_provider_wire_request, provider_request_dry_run_response,
};
use crate::provider_config::ProviderApiFamily;
use crate::provider_input_normalizer::normalize_provider_user_input;
use futures_util::StreamExt;
use reqwest::header::HeaderMap;
use serde::Deserialize;
use serde_json::{Map as JsonMap, Value, json};
use vulcan_core::provider::VulcanProviderDeltaSink;
use vulcan_core::types::{
    ConversationMessage as CoreConversationMessage, ConversationPayload as CoreConversationPayload,
    ConversationToolCall as CoreConversationToolCall, CoreError as VulcanCoreError,
    ProviderTurnDelta as CoreProviderTurnDelta, ProviderTurnResponse as CoreProviderTurnResponse,
    ProviderUsageSnapshot as CoreProviderUsageSnapshot,
};

/// Minimal OpenAI Responses top-level error payload decoded from one failed HTTP response or SSE error event.
/// 从单个失败 HTTP 响应或 SSE 错误事件解码出的最小 OpenAI Responses 顶层错误载荷。
#[derive(Debug, Clone, Deserialize)]
struct OpenAiResponsesErrorPayload {
    /// Optional provider error code.
    /// 可选的 Provider 错误码。
    code: Option<String>,
    /// Human-readable provider error message.
    /// 人类可读的 Provider 错误消息。
    message: String,
}

/// Minimal OpenAI Responses incomplete-details payload preserved for terminal error reporting.
/// 为终态错误报告保留的最小 OpenAI Responses incomplete-details 载荷。
#[derive(Debug, Clone, Deserialize)]
struct OpenAiResponsesIncompleteDetails {
    /// Human-readable incomplete reason returned by the provider.
    /// Provider 返回的人类可读不完整原因。
    reason: String,
}

/// Minimal OpenAI Responses reasoning summary item decoded from the completed response body.
/// 从完成响应正文中解码出的最小 OpenAI Responses 思考摘要项。
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum OpenAiResponsesReasoningSummary {
    /// One plain-text reasoning summary line.
    /// 一条纯文本思考摘要行。
    SummaryText { text: String },
}

/// Minimal OpenAI Responses assistant message content item decoded from the completed response body.
/// 从完成响应正文中解码出的最小 OpenAI Responses 助手消息内容项。
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum OpenAiResponsesAssistantContentItem {
    /// One visible assistant output-text fragment.
    /// 一条可见的助手输出文本片段。
    OutputText { text: String },
    /// One visible refusal fragment that should still surface as assistant text.
    /// 一条仍应作为助手文本展示的可见拒绝片段。
    Refusal { refusal: String },
}

/// Minimal OpenAI Responses assistant message output item decoded from the completed response body.
/// 从完成响应正文中解码出的最小 OpenAI Responses 助手消息输出项。
#[derive(Debug, Clone, Deserialize)]
struct OpenAiResponsesOutputMessage {
    /// Message content fragments.
    /// 消息内容片段集合。
    content: Vec<OpenAiResponsesAssistantContentItem>,
}

/// Minimal OpenAI Responses function-call output item decoded from the completed response body.
/// 从完成响应正文中解码出的最小 OpenAI Responses 函数调用输出项。
#[derive(Debug, Clone, Deserialize)]
struct OpenAiResponsesOutputFunctionCall {
    /// Stable provider item identifier.
    /// 稳定的 Provider 项标识。
    id: String,
    /// Stable provider-visible call identifier used by tool-result messages.
    /// 被工具结果消息使用的稳定 Provider 可见调用标识。
    call_id: String,
    /// Tool function name emitted by the provider.
    /// Provider 发出的工具函数名称。
    name: String,
    /// Structured JSON arguments decoded from the Responses API stringified payload.
    /// 从 Responses API 字符串化载荷解码出的结构化 JSON 参数。
    #[serde(deserialize_with = "deserialize_stringified_json")]
    arguments: Value,
}

/// Minimal OpenAI Responses completed-response output item decoded from the terminal response body.
/// 从终态响应正文中解码出的最小 OpenAI Responses 完成输出项。
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum OpenAiResponsesOutputItem {
    /// One assistant message output item.
    /// 一条助手消息输出项。
    Message(OpenAiResponsesOutputMessage),
    /// One function-call output item.
    /// 一条函数调用输出项。
    #[serde(alias = "function_call")]
    FunctionCall(OpenAiResponsesOutputFunctionCall),
    /// One reasoning output item.
    /// 一条思考输出项。
    Reasoning {
        /// Summary fragments retained for local-only thinking projection.
        /// 为仅本地思考投影保留的摘要片段集合。
        summary: Vec<OpenAiResponsesReasoningSummary>,
        /// Optional encrypted reasoning payload that should remain opaque locally.
        /// 应在本地保持不透明的可选加密思考载荷。
        #[serde(default)]
        encrypted_content: Option<String>,
    },
    /// Any unknown provider-native output item that should not break terminal response decoding.
    /// 任何不应打断终态响应解码的未知 Provider 原生输出项。
    #[serde(other)]
    Unknown,
}

/// Minimal OpenAI Responses completed terminal response decoded from SSE `response.completed`, `response.failed`, or `response.incomplete` events.
/// 从 SSE `response.completed`、`response.failed` 或 `response.incomplete` 事件解码出的最小 OpenAI Responses 终态响应。
#[derive(Debug, Clone, Deserialize)]
struct OpenAiResponsesCompletedResponse {
    /// Stable OpenAI Responses response identifier used for continuation chaining.
    /// 用于 continuation 串联的稳定 OpenAI Responses 响应标识。
    id: String,
    /// Terminal output items returned by the provider.
    /// Provider 返回的终态输出项集合。
    #[serde(default)]
    output: Vec<OpenAiResponsesOutputItem>,
    /// Optional provider error payload carried by failed terminal responses.
    /// 失败终态响应携带的可选 Provider 错误载荷。
    #[serde(default)]
    error: Option<OpenAiResponsesErrorPayload>,
    /// Optional incomplete details carried by incomplete terminal responses.
    /// 不完整终态响应携带的可选 incomplete-details 载荷。
    #[serde(default)]
    incomplete_details: Option<OpenAiResponsesIncompleteDetails>,
    /// Optional terminal usage payload.
    /// 可选的终态用量载荷。
    #[serde(default)]
    usage: Option<OpenAiResponsesUsage>,
}

/// Minimal OpenAI Responses terminal usage payload.
/// 最小 OpenAI Responses 终态用量载荷。
#[derive(Debug, Clone, Default, Deserialize)]
struct OpenAiResponsesUsage {
    /// Optional input token count.
    /// 可选的输入 Token 数量。
    #[serde(default)]
    input_tokens: Option<usize>,
    /// Optional output token count.
    /// 可选的输出 Token 数量。
    #[serde(default)]
    output_tokens: Option<usize>,
    /// Optional total token count.
    /// 可选的总 Token 数量。
    #[serde(default)]
    total_tokens: Option<usize>,
    /// Optional output-token detail payload.
    /// 可选的输出 Token 细分载荷。
    #[serde(default)]
    output_tokens_details: Option<OpenAiResponsesOutputTokenDetails>,
    /// Optional input-token detail payload.
    /// 可选的输入 Token 细分载荷。
    #[serde(default)]
    input_tokens_details: Option<OpenAiResponsesInputTokenDetails>,
}

/// Minimal OpenAI Responses output-token detail payload.
/// 最小 OpenAI Responses 输出 Token 细分载荷。
#[derive(Debug, Clone, Default, Deserialize)]
struct OpenAiResponsesOutputTokenDetails {
    /// Optional reasoning token count.
    /// 可选的推理 Token 数量。
    #[serde(default)]
    reasoning_tokens: Option<usize>,
}

/// Minimal OpenAI Responses input-token detail payload.
/// 最小 OpenAI Responses 输入 Token 细分载荷。
#[derive(Debug, Clone, Default, Deserialize)]
struct OpenAiResponsesInputTokenDetails {
    /// Optional cached-token count reported for prompt reuse.
    /// 为提示词复用报告的可选缓存命中 Token 数量。
    #[serde(default)]
    cached_tokens: Option<usize>,
}

/// Executes one direct OpenAI Responses streaming turn without going through any third-party completion-model abstraction.
/// 在不经过任何第三方 completion model 抽象的前提下执行一次直连 OpenAI Responses 流式回合。
pub(crate) async fn execute_openai_responses_http_streaming_turn<S>(
    provider_id: &str,
    provider_label: &str,
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
    ensure_bearer_authorization_header(provider_id, provider_label, api_key, &mut wire_headers)?;
    ensure_json_content_type(&mut wire_headers);
    let client =
        build_openai_responses_http_client(provider_id, provider_label, api_key, request_headers)?;
    let endpoint = build_openai_responses_endpoint(base_url);
    let payload =
        build_openai_responses_request_payload(model_id, request, provider_tool_definitions)?;
    let wire_request = emit_provider_wire_request(
        live_sender,
        provider_id,
        model_id,
        "POST",
        endpoint.as_str(),
        &wire_headers,
        &payload,
    );
    if let Some(response) = provider_request_dry_run_response(&wire_request) {
        return Ok(response);
    }
    let mut response = client
        .post(endpoint)
        .json(&payload)
        .send()
        .await
        .map_err(|error| VulcanCoreError::ProviderFailure {
            provider_id: provider_id.to_string(),
            message: format!("{provider_label} 请求发送失败：{error}"),
        })?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        let message = format_standard_error_response(status, &body);
        return Err(VulcanCoreError::ProviderFailure {
            provider_id: provider_id.to_string(),
            message: format!("{provider_label} request failed: {message}"),
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

    let mut completed_response = None;
    // Best-effort response identifier captured from non-terminal events for terminal fallback synthesis.
    // 从非终态事件捕获的一份尽力响应标识，用于终态兜底合成。
    let mut fallback_response_id = None;
    // Collected output items emitted by `response.output_item.done` for terminal fallback synthesis.
    // 从 `response.output_item.done` 收集的输出项集合，用于终态兜底合成。
    let mut fallback_output = Vec::new();
    // Collected visible assistant-text deltas used only when no structured message item arrives.
    // 仅在没有结构化消息项到达时使用的可见助手文本增量累计值。
    let mut fallback_assistant_text = String::new();
    // Collected reasoning-summary deltas used only when no structured reasoning item arrives.
    // 仅在没有结构化思考项到达时使用的思考摘要增量累计值。
    let mut fallback_reasoning_text = String::new();

    while let Some(event_result) = events.next().await {
        let event = event_result.map_err(|error| VulcanCoreError::ProviderFailure {
            provider_id: provider_id.to_string(),
            message: format!("{provider_label} SSE 解码失败：{error}"),
        })?;

        if event.data.trim().is_empty() || event.data.trim() == "[DONE]" {
            continue;
        }

        let value = serde_json::from_str::<Value>(&event.data).map_err(|error| {
            VulcanCoreError::ProviderFailure {
                provider_id: provider_id.to_string(),
                message: format!("{provider_label} 流式数据解析失败：{error}"),
            }
        })?;

        match value.get("type").and_then(Value::as_str) {
            Some("response.created") | Some("response.in_progress") => {
                if fallback_response_id.is_none() {
                    fallback_response_id = extract_openai_responses_stream_response_id(&value);
                }
            }
            Some("response.output_text.delta") | Some("response.refusal.delta") => {
                if let Some(delta) = value.get("delta").and_then(Value::as_str)
                    && !delta.is_empty()
                {
                    fallback_assistant_text.push_str(delta);
                    forward_openai_responses_delta(
                        live_sender,
                        sink,
                        CoreProviderTurnDelta::Output {
                            delta: delta.to_string(),
                        },
                    );
                }
            }
            Some("response.reasoning_summary_text.delta") => {
                if let Some(delta) = value.get("delta").and_then(Value::as_str)
                    && !delta.is_empty()
                {
                    fallback_reasoning_text.push_str(delta);
                    forward_openai_responses_delta(
                        live_sender,
                        sink,
                        CoreProviderTurnDelta::Thinking {
                            delta: delta.to_string(),
                        },
                    );
                }
            }
            Some("response.output_item.done") => {
                if let Some(output_item) = value
                    .get("item")
                    .cloned()
                    .and_then(|item| serde_json::from_value::<OpenAiResponsesOutputItem>(item).ok())
                {
                    fallback_output.push(output_item);
                }
            }
            Some("response.completed") => {
                let Some(response_value) = value.get("response").cloned() else {
                    return Err(VulcanCoreError::ProviderFailure {
                        provider_id: provider_id.to_string(),
                        message: String::from(
                            "OpenAI Responses completed 事件缺少 response 载荷。",
                        ),
                    });
                };
                completed_response = Some(
                    serde_json::from_value::<OpenAiResponsesCompletedResponse>(response_value)
                        .map_err(|error| VulcanCoreError::ProviderFailure {
                            provider_id: provider_id.to_string(),
                            message: format!("OpenAI Responses completed 响应解析失败：{error}"),
                        })?,
                );
                break;
            }
            Some("response.failed") | Some("response.incomplete") => {
                let Some(response_value) = value.get("response").cloned() else {
                    return Err(VulcanCoreError::ProviderFailure {
                        provider_id: provider_id.to_string(),
                        message: String::from("OpenAI Responses 终态错误事件缺少 response 载荷。"),
                    });
                };
                let response =
                    serde_json::from_value::<OpenAiResponsesCompletedResponse>(response_value)
                        .map_err(|error| VulcanCoreError::ProviderFailure {
                            provider_id: provider_id.to_string(),
                            message: format!("OpenAI Responses 终态错误响应解析失败：{error}"),
                        })?;
                return Err(VulcanCoreError::ProviderFailure {
                    provider_id: provider_id.to_string(),
                    message: format!(
                        "{provider_label} 请求失败：{}",
                        build_openai_responses_terminal_error_message(&response)
                    ),
                });
            }
            Some("error") => {
                let message = value
                    .get("error")
                    .cloned()
                    .and_then(|error| {
                        serde_json::from_value::<OpenAiResponsesErrorPayload>(error).ok()
                    })
                    .map(|error| format_openai_responses_error_payload(&error))
                    .or_else(|| {
                        value
                            .get("message")
                            .and_then(Value::as_str)
                            .map(ToString::to_string)
                    })
                    .unwrap_or_else(|| event.data.clone());
                return Err(VulcanCoreError::ProviderFailure {
                    provider_id: provider_id.to_string(),
                    message: format!("{provider_label} 流式执行失败：{message}"),
                });
            }
            _ => {}
        }
    }

    let completed_response = completed_response.or_else(|| {
        synthesize_openai_responses_fallback_response(
            fallback_response_id,
            fallback_output,
            fallback_assistant_text,
            fallback_reasoning_text,
        )
    });

    let Some(completed_response) = completed_response else {
        return Err(VulcanCoreError::ProviderFailure {
            provider_id: provider_id.to_string(),
            message: String::from(
                "OpenAI Responses 流式请求未返回 response.completed，无法构造最终回合响应。",
            ),
        });
    };

    build_openai_responses_turn_response(provider_id, model_id, completed_response)
}

/// Builds one direct OpenAI Responses HTTP client with merged authorization and caller-supplied request headers.
/// 构建一个直连 OpenAI Responses HTTP 客户端，并合并鉴权头与调用方提供的请求头。
fn build_openai_responses_http_client(
    provider_id: &str,
    provider_label: &str,
    api_key: &str,
    mut request_headers: HeaderMap,
) -> Result<reqwest::Client, VulcanCoreError> {
    ensure_bearer_authorization_header(provider_id, provider_label, api_key, &mut request_headers)?;
    ensure_json_content_type(&mut request_headers);
    build_json_http_client(provider_id, provider_label, request_headers)
}

/// Builds the final OpenAI Responses `/responses` endpoint from one optional base URL override.
/// 根据可选基础地址覆盖值构建最终 OpenAI Responses `/responses` 端点。
fn build_openai_responses_endpoint(base_url: Option<&str>) -> String {
    build_rest_endpoint(base_url, "https://api.openai.com/v1", "responses")
}

/// Builds the final OpenAI Responses JSON payload from one host-controlled turn request and tool schema set.
/// 根据一份宿主控制回合请求与工具 schema 集合构建最终 OpenAI Responses JSON 载荷。
fn build_openai_responses_request_payload(
    model_id: &str,
    request: OpenAiCompatibleCoreTurnRequest,
    provider_tool_definitions: &[ProviderToolDefinition],
) -> Result<Value, VulcanCoreError> {
    let OpenAiCompatibleCoreTurnRequest {
        snapshot,
        current_history,
        current_prompt,
        ..
    } = request;

    let system_prompt = provider_fallback_system_prompt(&snapshot);
    let mut payload = match snapshot.additional_params {
        Some(Value::Object(object)) => object,
        Some(other) => {
            let mut object = JsonMap::new();
            object.insert(String::from("host_additional_params"), other);
            object
        }
        None => JsonMap::new(),
    };

    let mut input = current_history
        .iter()
        .map(core_message_to_openai_responses_input_items)
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    input.extend(core_message_to_openai_responses_input_items(
        &current_prompt,
    )?);

    payload.insert(String::from("model"), Value::String(model_id.to_string()));
    payload.insert(String::from("input"), Value::Array(input));
    payload.insert(String::from("stream"), Value::Bool(true));

    if !system_prompt.trim().is_empty() {
        payload.insert(String::from("instructions"), Value::String(system_prompt));
    }

    if let Some(temperature) = snapshot.temperature {
        payload.insert(String::from("temperature"), json!(temperature));
    }

    normalize_openai_responses_reasoning_include(&mut payload);

    let mut tools = payload
        .remove("tools")
        .and_then(|value| value.as_array().cloned())
        .unwrap_or_default();
    tools.extend(
        provider_tool_definitions
            .iter()
            .map(provider_tool_definition_to_openai_responses_tool),
    );
    if !tools.is_empty() {
        payload.insert(String::from("tools"), Value::Array(tools));
    }

    Ok(Value::Object(payload))
}

/// Ensures OpenAI Responses requests carrying reasoning config also request encrypted reasoning content when the caller did not already opt in explicitly.
/// 确保携带 reasoning 配置的 OpenAI Responses 请求在调用方未显式选择时也会一并请求加密思考内容。
fn normalize_openai_responses_reasoning_include(payload: &mut JsonMap<String, Value>) {
    if !payload.contains_key("reasoning") {
        return;
    }

    let include = payload
        .entry(String::from("include"))
        .or_insert_with(|| Value::Array(Vec::new()));
    let Some(include_array) = include.as_array_mut() else {
        return;
    };

    if include_array
        .iter()
        .any(|value| value.as_str() == Some("reasoning.encrypted_content"))
    {
        return;
    }

    include_array.push(Value::String(String::from("reasoning.encrypted_content")));
}

/// Converts one canonical core message into one or more OpenAI Responses input items while intentionally dropping local-only reasoning replay.
/// 将一条规范 core 消息转换为一条或多条 OpenAI Responses 输入项，同时刻意丢弃仅限本地使用的思考回放。
fn core_message_to_openai_responses_input_items(
    message: &CoreConversationMessage,
) -> Result<Vec<Value>, VulcanCoreError> {
    match &message.payload {
        CoreConversationPayload::UserText { text, attachments } => {
            let normalized = normalize_provider_user_input(
                ProviderApiFamily::OpenAiResponses,
                text,
                attachments,
            );
            let mut content = Vec::new();
            if !normalized.text.is_empty() {
                content.push(json!({
                    "type": "input_text",
                    "text": normalized.text
                }));
            }
            for attachment in &normalized.attachments {
                let attachment_payload = match read_provider_attachment(attachment)? {
                    Some(payload) => payload,
                    None => {
                        content.push(json!({
                            "type": "input_text",
                            "text": build_missing_attachment_replay_text(attachment)
                        }));
                        continue;
                    }
                };
                match attachment_payload.kind {
                    vulcan_core::types::ConversationAttachmentKind::Image => {
                        content.push(json!({
                            "type": "input_image",
                            "image_url": attachment_payload.data_url
                        }));
                    }
                    vulcan_core::types::ConversationAttachmentKind::Audio
                    | vulcan_core::types::ConversationAttachmentKind::Video
                    | vulcan_core::types::ConversationAttachmentKind::Pdf => {
                        content.push(json!({
                            "type": "input_file",
                            "filename": attachment_payload.file_name,
                            "file_data": attachment_payload.data_url
                        }));
                    }
                }
            }
            Ok(vec![json!({
                "role": "user",
                "content": content
            })])
        }
        CoreConversationPayload::Assistant {
            text,
            provider_message_id,
            tool_calls,
            ..
        } => {
            let mut items = Vec::new();

            if !text.is_empty() {
                let mut object = JsonMap::new();
                object.insert(
                    String::from("role"),
                    Value::String(String::from("assistant")),
                );
                object.insert(
                    String::from("content"),
                    Value::Array(vec![json!({
                        "type": "output_text",
                        "text": text
                    })]),
                );
                object.insert(
                    String::from("status"),
                    Value::String(String::from("completed")),
                );
                if let Some(message_id) = provider_message_id
                    .as_ref()
                    .filter(|value| !value.trim().is_empty())
                {
                    object.insert(String::from("id"), Value::String(message_id.clone()));
                }
                items.push(Value::Object(object));
            }

            items.extend(
                tool_calls
                    .iter()
                    .map(core_tool_call_to_openai_responses_item),
            );
            Ok(items)
        }
        CoreConversationPayload::ToolResult {
            tool_call_id, text, ..
        } => {
            let Some(tool_call_id) = tool_call_id
                .clone()
                .filter(|value| !value.trim().is_empty())
            else {
                return Err(VulcanCoreError::InvalidState {
                    message: String::from(
                        "OpenAI Responses 工具结果消息缺少 tool_call_id，无法构造 provider 请求。",
                    ),
                });
            };
            Ok(vec![json!({
                "type": "function_call_output",
                "call_id": tool_call_id,
                "output": text,
                "status": "completed"
            })])
        }
    }
}

/// Converts one canonical core tool call into one OpenAI Responses function-call input item with stringified JSON arguments.
/// 将一条规范 core 工具调用转换为一条 OpenAI Responses 函数调用输入项，并把 JSON 参数字符串化。
fn core_tool_call_to_openai_responses_item(tool_call: &CoreConversationToolCall) -> Value {
    json!({
        "type": "function_call",
        "id": tool_call.id,
        "call_id": tool_call.tool_call_id.clone().unwrap_or_else(|| tool_call.id.clone()),
        "name": tool_call.tool_name,
        "arguments": stringify_openai_responses_tool_arguments(&tool_call.arguments),
        "status": "completed"
    })
}

/// Converts one host-owned provider tool definition into one OpenAI Responses function-tool schema object with strict schema normalization.
/// 将一条宿主自有 Provider 工具定义转换为一条启用严格 schema 规范化的 OpenAI Responses 函数工具对象。
fn provider_tool_definition_to_openai_responses_tool(definition: &ProviderToolDefinition) -> Value {
    let mut parameters = definition.parameters.clone();
    normalize_provider_tool_parameters_schema(&mut parameters);
    sanitize_openai_responses_schema(&mut parameters);

    json!({
        "type": "function",
        "name": definition.name,
        "description": definition.description,
        "parameters": parameters,
        "strict": true
    })
}

/// Recursively normalizes one OpenAI Responses JSON schema so strict tool mode satisfies OpenAI object restrictions.
/// 递归规范化一份 OpenAI Responses JSON Schema，使严格工具模式满足 OpenAI 的对象限制。
fn sanitize_openai_responses_schema(schema: &mut Value) {
    if let Value::Object(object) = schema {
        if object.contains_key("$ref") {
            object.retain(|key, _| key == "$ref");
            return;
        }

        let is_object_schema = object.get("type") == Some(&Value::String(String::from("object")))
            || object.contains_key("properties");

        if is_object_schema && !object.contains_key("additionalProperties") {
            object.insert(String::from("additionalProperties"), Value::Bool(false));
        }

        if let Some(Value::Object(properties)) = object.get("properties") {
            object.insert(
                String::from("required"),
                Value::Array(properties.keys().cloned().map(Value::String).collect()),
            );
        }

        if let Some(Value::Object(definitions)) = object.get_mut("$defs") {
            for value in definitions.values_mut() {
                sanitize_openai_responses_schema(value);
            }
        }

        if let Some(Value::Object(properties)) = object.get_mut("properties") {
            for value in properties.values_mut() {
                sanitize_openai_responses_schema(value);
            }
        }

        if let Some(items) = object.get_mut("items") {
            sanitize_openai_responses_schema(items);
        }

        if let Some(one_of) = object.remove("oneOf") {
            match object.get_mut("anyOf") {
                Some(Value::Array(existing)) => {
                    if let Value::Array(mut incoming) = one_of {
                        existing.append(&mut incoming);
                    }
                }
                _ => {
                    object.insert(String::from("anyOf"), one_of);
                }
            }
        }

        for key in ["anyOf", "oneOf", "allOf"] {
            if let Some(Value::Array(variants)) = object.get_mut(key) {
                for variant in variants {
                    sanitize_openai_responses_schema(variant);
                }
            }
        }
    }
}

/// Serializes one tool-arguments JSON value into the string form expected inside OpenAI Responses function-call items.
/// 将单个工具参数 JSON 值序列化为 OpenAI Responses 函数调用项中期望的字符串形式。
fn stringify_openai_responses_tool_arguments(arguments: &Value) -> String {
    match arguments {
        Value::String(value) => value.clone(),
        other => serde_json::to_string(other).unwrap_or_else(|_| other.to_string()),
    }
}

/// Builds one normalized `vulcan-core` turn response from the terminal OpenAI Responses response object.
/// 根据终态 OpenAI Responses 响应对象构建一份规范化的 `vulcan-core` 回合响应。
fn build_openai_responses_turn_response(
    provider_id: &str,
    model_id: &str,
    response: OpenAiResponsesCompletedResponse,
) -> Result<CoreProviderTurnResponse, VulcanCoreError> {
    let mut assistant_content = Vec::new();

    for output in response.output {
        match output {
            OpenAiResponsesOutputItem::Message(message) => {
                for content in message.content {
                    match content {
                        OpenAiResponsesAssistantContentItem::OutputText { text } => {
                            if !text.is_empty() {
                                assistant_content.push(ProviderAssistantContent::Text(text));
                            }
                        }
                        OpenAiResponsesAssistantContentItem::Refusal { refusal } => {
                            if !refusal.is_empty() {
                                assistant_content.push(ProviderAssistantContent::Text(refusal));
                            }
                        }
                    }
                }
            }
            OpenAiResponsesOutputItem::FunctionCall(function_call) => {
                assistant_content.push(ProviderAssistantContent::ToolCall(
                    CoreConversationToolCall {
                        id: function_call.id,
                        tool_call_id: Some(function_call.call_id),
                        tool_name: function_call.name,
                        arguments: function_call.arguments,
                    },
                ));
            }
            OpenAiResponsesOutputItem::Reasoning {
                summary,
                encrypted_content,
            } => {
                let mut parts = summary
                    .into_iter()
                    .map(|item| match item {
                        OpenAiResponsesReasoningSummary::SummaryText { text } => {
                            ProviderReasoningPart::Summary(text)
                        }
                    })
                    .collect::<Vec<_>>();
                if encrypted_content.is_some() {
                    parts.push(ProviderReasoningPart::Opaque);
                }
                if !parts.is_empty() {
                    assistant_content.push(ProviderAssistantContent::Reasoning(parts));
                }
            }
            OpenAiResponsesOutputItem::Unknown => {}
        }
    }

    Ok(build_openai_compatible_turn_response(
        provider_id,
        model_id,
        Some(response.id),
        &assistant_content,
        response.usage.and_then(openai_responses_usage_to_snapshot),
    ))
}

/// Extracts one best-effort response identifier from one non-terminal OpenAI Responses SSE event payload.
/// 从一份非终态 OpenAI Responses SSE 事件载荷中提取一条尽力响应标识。
fn extract_openai_responses_stream_response_id(value: &Value) -> Option<String> {
    value
        .get("response")
        .and_then(|response| response.get("id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|response_id| !response_id.is_empty())
        .map(ToString::to_string)
}

/// Synthesizes one terminal OpenAI Responses payload from collected stream items when `response.completed` never arrives.
/// 当 `response.completed` 始终未到达时，根据已收集的流式输出项合成一份终态 OpenAI Responses 载荷。
fn synthesize_openai_responses_fallback_response(
    response_id: Option<String>,
    mut output: Vec<OpenAiResponsesOutputItem>,
    assistant_text: String,
    reasoning_text: String,
) -> Option<OpenAiResponsesCompletedResponse> {
    let response_id = response_id?;

    let has_visible_message = output.iter().any(|item| match item {
        OpenAiResponsesOutputItem::Message(message) => message.content.iter().any(|content| {
            matches!(
                content,
                OpenAiResponsesAssistantContentItem::OutputText { text }
                    if !text.is_empty()
            ) || matches!(
                content,
                OpenAiResponsesAssistantContentItem::Refusal { refusal }
                    if !refusal.is_empty()
            )
        }),
        _ => false,
    });
    if !has_visible_message && !assistant_text.is_empty() {
        output.push(OpenAiResponsesOutputItem::Message(
            OpenAiResponsesOutputMessage {
                content: vec![OpenAiResponsesAssistantContentItem::OutputText {
                    text: assistant_text,
                }],
            },
        ));
    }

    let has_reasoning = output.iter().any(|item| match item {
        OpenAiResponsesOutputItem::Reasoning {
            summary,
            encrypted_content,
        } => !summary.is_empty() || encrypted_content.is_some(),
        _ => false,
    });
    if !has_reasoning && !reasoning_text.is_empty() {
        output.push(OpenAiResponsesOutputItem::Reasoning {
            summary: vec![OpenAiResponsesReasoningSummary::SummaryText {
                text: reasoning_text,
            }],
            encrypted_content: None,
        });
    }

    if output.is_empty() {
        return None;
    }

    Some(OpenAiResponsesCompletedResponse {
        id: response_id,
        output,
        error: None,
        incomplete_details: None,
        usage: None,
    })
}

/// Converts one raw OpenAI Responses usage payload into the shared normalized usage snapshot.
/// 将一份原始 OpenAI Responses 用量载荷转换为共享规范化用量快照。
fn openai_responses_usage_to_snapshot(
    usage: OpenAiResponsesUsage,
) -> Option<CoreProviderUsageSnapshot> {
    let snapshot = CoreProviderUsageSnapshot {
        input_tokens: usage.input_tokens,
        output_tokens: usage.output_tokens,
        reasoning_tokens: usage
            .output_tokens_details
            .and_then(|details| details.reasoning_tokens),
        total_tokens: usage.total_tokens,
        cached_input_tokens: usage
            .input_tokens_details
            .and_then(|details| details.cached_tokens),
        cache_creation_input_tokens: None,
    };
    snapshot.has_any_tokens().then_some(snapshot)
}

/// Formats one decoded OpenAI Responses error payload into one human-readable message.
/// 将一份已解码的 OpenAI Responses 错误载荷格式化为一条人类可读消息。
fn format_openai_responses_error_payload(error: &OpenAiResponsesErrorPayload) -> String {
    match error.code.as_deref() {
        Some(code) if !code.trim().is_empty() => format!("{code}: {}", error.message),
        _ => error.message.clone(),
    }
}

/// Builds one human-readable message for a failed or incomplete terminal OpenAI Responses event.
/// 为一次失败或不完整的终态 OpenAI Responses 事件构建一条人类可读消息。
fn build_openai_responses_terminal_error_message(
    response: &OpenAiResponsesCompletedResponse,
) -> String {
    if let Some(error) = response.error.as_ref() {
        return format_openai_responses_error_payload(error);
    }

    if let Some(details) = response.incomplete_details.as_ref() {
        return format!("响应未完成：{}", details.reason);
    }

    String::from("响应以失败或不完整状态结束，但未返回错误详情。")
}

/// Forwards one normalized provider delta into both the controller sink and the GUI live sender.
/// 将一条规范 Provider 增量同时转发给控制器汇聚器与 GUI 实时发送器。
fn forward_openai_responses_delta<S>(
    live_sender: &ProviderLiveDeltaSender,
    sink: &mut S,
    delta: CoreProviderTurnDelta,
) where
    S: VulcanProviderDeltaSink,
{
    sink.push_delta(delta.clone());
    let _ = live_sender.send(Ok(core_provider_turn_delta_to_runtime_delta(delta)));
}

/// Deserializes one JSON value that may already be structured JSON or may be the stringified JSON form used by OpenAI Responses function calls.
/// 反序列化一个可能已经是结构化 JSON、也可能是 OpenAI Responses 函数调用使用的字符串化 JSON 载荷。
fn deserialize_stringified_json<'de, D>(deserializer: D) -> Result<Value, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    match value {
        Value::String(text) => {
            serde_json::from_str::<Value>(&text).map_err(serde::de::Error::custom)
        }
        other => Ok(other),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_openai_responses_request_payload, build_openai_responses_turn_response,
        sanitize_openai_responses_schema, synthesize_openai_responses_fallback_response,
    };
    use crate::agent_runtime::provider_protocol_tool_definition::ProviderToolDefinition;
    use crate::agent_runtime::provider_transport_turn_request::OpenAiCompatibleCoreTurnRequest;
    use serde_json::json;
    use std::time::{SystemTime, UNIX_EPOCH};
    use vulcan_core::types::{
        ConversationAttachment, ConversationAttachmentKind,
        ConversationMessage as CoreConversationMessage,
        ConversationPayload as CoreConversationPayload,
        ConversationToolCall as CoreConversationToolCall,
        ProviderContinuation as CoreProviderContinuation,
        ProviderRequestSnapshot as CoreProviderRequestSnapshot,
    };

    /// Builds one temporary attachment used by OpenAI Responses multimodal tests.
    /// 构建一条供 OpenAI Responses 多模态测试使用的临时附件。
    fn build_test_attachment(
        extension: &str,
        kind: ConversationAttachmentKind,
        mime_type: &str,
        file_name: &str,
    ) -> ConversationAttachment {
        let unique_id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        let file_path =
            std::env::temp_dir().join(format!("vulcan-openai-responses-{unique_id}.{extension}"));
        std::fs::write(&file_path, [0_u8, 1, 2, 3]).expect("temporary attachment should write");

        ConversationAttachment {
            kind,
            file_name: String::from(file_name),
            local_path: file_path.to_string_lossy().into_owned(),
            source_path: None,
            asset_id: None,
            byte_len: Some(4),
            preview_local_path: None,
            mime_type: String::from(mime_type),
        }
    }

    /// Verifies the direct OpenAI Responses request payload strips reasoning replay, injects chained continuation, and preserves function-call output shapes.
    /// 验证直连 OpenAI Responses 请求载荷会剥离思考回放、注入串联 continuation，并保留 function-call output 形状。
    #[test]
    fn request_payload_strips_reasoning_replay_and_preserves_responses_shapes() {
        let request = OpenAiCompatibleCoreTurnRequest {
            snapshot: CoreProviderRequestSnapshot {
                provider_id: String::from("openai"),
                model_id: String::from("gpt-5"),
                system_prompt: String::from("system prompt"),
                prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                    String::from("system prompt").clone(),
                ),
                prompt: String::from("tool result body"),
                history: Vec::new(),
                temperature: Some(0.2),
                max_turns: 8,
                tool_schemas: Vec::new(),
                additional_params: Some(json!({
                    "reasoning": { "effort": "high" },
                    "previous_response_id": "resp_prev"
                })),
                request_headers: None,
                continuation: CoreProviderContinuation::default(),
            },
            current_history: vec![CoreConversationMessage {
                id: String::from("assistant-1"),
                payload: CoreConversationPayload::Assistant {
                    text: String::from("我先读取文件。"),
                    reasoning: String::from("这段思考不应回放。"),
                    provider_message_id: Some(String::from("resp-1")),
                    provider_message_state: None,
                    tool_calls: vec![CoreConversationToolCall {
                        id: String::from("fc_1"),
                        tool_call_id: Some(String::from("call-1")),
                        tool_name: String::from("read_file"),
                        arguments: json!({ "path": "src/main.rs" }),
                    }],
                    usage: None,
                },
            }],
            current_prompt: CoreConversationMessage {
                id: String::from("tool-result-1"),
                payload: CoreConversationPayload::ToolResult {
                    tool_result_id: String::from("result-1"),
                    tool_call_id: Some(String::from("call-1")),
                    text: String::from("tool result body"),
                },
            },
            interleaved_reasoning: None,
        };

        let payload = build_openai_responses_request_payload(
            "gpt-5",
            request,
            &[ProviderToolDefinition {
                name: String::from("read_file"),
                description: String::from("Read one file"),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" }
                    }
                }),
            }],
        )
        .expect("payload should build");

        let input = payload["input"]
            .as_array()
            .expect("input should be an array");
        assert_eq!(payload["instructions"], "system prompt");
        assert_eq!(payload["stream"], true);
        assert_eq!(payload["previous_response_id"], "resp_prev");
        assert_eq!(payload["include"][0], "reasoning.encrypted_content");
        assert_eq!(input[0]["role"], "assistant");
        assert_eq!(input[0]["content"][0]["type"], "output_text");
        assert!(input[0].get("reasoning").is_none());
        assert_eq!(input[1]["type"], "function_call");
        assert_eq!(input[1]["call_id"], "call-1");
        assert_eq!(input[2]["type"], "function_call_output");
        assert_eq!(input[2]["call_id"], "call-1");
        assert_eq!(input[2]["output"], "tool result body");
        assert_eq!(payload["tools"][0]["type"], "function");
        assert_eq!(payload["tools"][0]["strict"], true);
        assert_eq!(
            payload["tools"][0]["parameters"]["additionalProperties"],
            false
        );
        assert_eq!(payload["tools"][0]["parameters"]["required"][0], "path");
    }

    /// Verifies OpenAI Responses preserves provider-native tools while appending host function tools.
    /// 验证 OpenAI Responses 会保留 Provider 原生工具，并继续追加宿主函数工具。
    #[test]
    fn request_payload_preserves_native_tools_and_appends_function_tools() {
        let request = OpenAiCompatibleCoreTurnRequest {
            snapshot: CoreProviderRequestSnapshot {
                provider_id: String::from("openai"),
                model_id: String::from("gpt-5"),
                system_prompt: String::new(),
                prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                    String::new().clone(),
                ),
                prompt: String::from("搜索最新信息"),
                history: Vec::new(),
                temperature: None,
                max_turns: 8,
                tool_schemas: Vec::new(),
                additional_params: Some(json!({
                    "tools": [{ "type": "web_search_preview" }]
                })),
                request_headers: None,
                continuation: CoreProviderContinuation::default(),
            },
            current_history: Vec::new(),
            current_prompt: CoreConversationMessage {
                id: String::from("user-1"),
                payload: CoreConversationPayload::UserText {
                    text: String::from("搜索最新信息"),
                    attachments: Vec::new(),
                },
            },
            interleaved_reasoning: None,
        };

        let payload = build_openai_responses_request_payload(
            "gpt-5",
            request,
            &[ProviderToolDefinition {
                name: String::from("read_file"),
                description: String::from("Read one file"),
                parameters: json!({ "type": "object", "properties": {} }),
            }],
        )
        .expect("payload should build");

        assert_eq!(payload["tools"][0]["type"], "web_search_preview");
        assert_eq!(payload["tools"][1]["type"], "function");
        assert_eq!(payload["tools"][1]["name"], "read_file");
    }

    /// Verifies OpenAI Responses user input items carry staged image attachments as `input_image` parts.
    /// 验证 OpenAI Responses 用户输入项会把已暂存图片附件编码为 `input_image` Part。
    #[test]
    fn request_payload_embeds_user_image_attachments_as_input_image_parts() {
        let attachment = build_test_attachment(
            "png",
            ConversationAttachmentKind::Image,
            "image/png",
            "diagram.png",
        );
        let request = OpenAiCompatibleCoreTurnRequest {
            snapshot: CoreProviderRequestSnapshot {
                provider_id: String::from("openai"),
                model_id: String::from("gpt-5"),
                system_prompt: String::from("system prompt"),
                prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                    String::from("system prompt").clone(),
                ),
                prompt: String::from("分析图片"),
                history: Vec::new(),
                temperature: None,
                max_turns: 8,
                tool_schemas: Vec::new(),
                additional_params: None,
                request_headers: None,
                continuation: CoreProviderContinuation::default(),
            },
            current_history: Vec::new(),
            current_prompt: CoreConversationMessage {
                id: String::from("user-1"),
                payload: CoreConversationPayload::UserText {
                    text: String::from("分析图片"),
                    attachments: vec![attachment.clone()],
                },
            },
            interleaved_reasoning: None,
        };

        let payload = build_openai_responses_request_payload("gpt-5", request, &[])
            .expect("payload should build");
        let input = payload["input"]
            .as_array()
            .expect("input should be an array");
        let content = input[0]["content"]
            .as_array()
            .expect("user content should be one part array");

        assert_eq!(input[0]["role"], "user");
        assert_eq!(content[0]["type"], "input_text");
        assert_eq!(content[1]["type"], "input_image");
        assert_eq!(
            content[1]["image_url"],
            json!("data:image/png;base64,AAECAw==")
        );

        let _ = std::fs::remove_file(&attachment.local_path);
    }

    /// Verifies OpenAI Responses user input items encode staged PDF and audio attachments as `input_file` parts.
    /// 验证 OpenAI Responses 用户输入项会把已暂存 PDF 与音频附件编码为 `input_file` Part。
    #[test]
    fn request_payload_embeds_user_file_like_attachments_as_input_file_parts() {
        let pdf_attachment = build_test_attachment(
            "pdf",
            ConversationAttachmentKind::Pdf,
            "application/pdf",
            "guide.pdf",
        );
        let audio_attachment = build_test_attachment(
            "mp3",
            ConversationAttachmentKind::Audio,
            "audio/mpeg",
            "sample.mp3",
        );
        let request = OpenAiCompatibleCoreTurnRequest {
            snapshot: CoreProviderRequestSnapshot {
                provider_id: String::from("openai"),
                model_id: String::from("gpt-5"),
                system_prompt: String::from("system prompt"),
                prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                    String::from("system prompt").clone(),
                ),
                prompt: String::from("分析附件"),
                history: Vec::new(),
                temperature: None,
                max_turns: 8,
                tool_schemas: Vec::new(),
                additional_params: None,
                request_headers: None,
                continuation: CoreProviderContinuation::default(),
            },
            current_history: Vec::new(),
            current_prompt: CoreConversationMessage {
                id: String::from("user-1"),
                payload: CoreConversationPayload::UserText {
                    text: String::from("分析附件"),
                    attachments: vec![pdf_attachment.clone(), audio_attachment.clone()],
                },
            },
            interleaved_reasoning: None,
        };

        let payload = build_openai_responses_request_payload("gpt-5", request, &[])
            .expect("payload should build");
        let input = payload["input"]
            .as_array()
            .expect("input should be an array");
        let content = input[0]["content"]
            .as_array()
            .expect("user content should be one part array");

        assert_eq!(content[1]["type"], "input_file");
        assert_eq!(content[1]["filename"], "guide.pdf");
        assert_eq!(
            content[1]["file_data"],
            json!("data:application/pdf;base64,AAECAw==")
        );
        assert_eq!(content[2]["type"], "input_file");
        assert_eq!(content[2]["filename"], "sample.mp3");
        assert_eq!(
            content[2]["file_data"],
            json!("data:audio/mpeg;base64,AAECAw==")
        );

        let _ = std::fs::remove_file(&pdf_attachment.local_path);
        let _ = std::fs::remove_file(&audio_attachment.local_path);
    }

    /// Verifies a pure attachment-only OpenAI Responses user item remains provider-visible when its managed replay file is gone.
    /// 验证纯附件 OpenAI Responses 用户项在托管回放文件缺失时仍会以文本提示形式传给 Provider。
    #[test]
    fn request_payload_replaces_missing_attachment_only_input_with_text_notice() {
        let attachment = build_test_attachment(
            "png",
            ConversationAttachmentKind::Image,
            "image/png",
            "lost-diagram.png",
        );
        std::fs::remove_file(&attachment.local_path).expect("managed attachment should remove");
        let request = OpenAiCompatibleCoreTurnRequest {
            snapshot: CoreProviderRequestSnapshot {
                provider_id: String::from("openai"),
                model_id: String::from("gpt-5"),
                system_prompt: String::new(),
                prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                    String::new().clone(),
                ),
                prompt: String::new(),
                history: Vec::new(),
                temperature: None,
                max_turns: 8,
                tool_schemas: Vec::new(),
                additional_params: None,
                request_headers: None,
                continuation: CoreProviderContinuation::default(),
            },
            current_history: Vec::new(),
            current_prompt: CoreConversationMessage {
                id: String::from("user-1"),
                payload: CoreConversationPayload::UserText {
                    text: String::new(),
                    attachments: vec![attachment],
                },
            },
            interleaved_reasoning: None,
        };

        let payload = build_openai_responses_request_payload("gpt-5", request, &[])
            .expect("payload should build");
        let input = payload["input"]
            .as_array()
            .expect("input should be an array");
        let content = input[0]["content"]
            .as_array()
            .expect("user content should be one part array");

        assert_eq!(input[0]["role"], "user");
        assert_eq!(content[0]["type"], "input_text");
        assert_eq!(
            content[0]["text"],
            json!("ERROR: Attachment \"lost-diagram.png\" no longer exists. Please ignore it.")
        );
    }

    /// Verifies completed OpenAI Responses outputs become the canonical assistant text, reasoning, tool calls, and continuation response identifier.
    /// 验证已完成的 OpenAI Responses 输出会转成规范助手文本、思考、工具调用以及 continuation 响应标识。
    #[test]
    fn completed_response_builds_canonical_turn_response() {
        let response = serde_json::from_value(json!({
            "id": "resp_done",
            "output": [
                {
                    "type": "message",
                    "content": [
                        { "type": "output_text", "text": "已读取文件。" }
                    ]
                },
                {
                    "type": "reasoning",
                    "summary": [
                        { "type": "summary_text", "text": "接下来需要修改函数。" }
                    ],
                    "encrypted_content": "opaque"
                },
                {
                    "type": "function_call",
                    "id": "fc_1",
                    "call_id": "call_1",
                    "name": "read_file",
                    "arguments": "{\"path\":\"src/main.rs\"}"
                }
            ]
        }))
        .expect("response should deserialize");

        let turn_response = build_openai_responses_turn_response("openai", "gpt-5", response)
            .expect("turn response should build");

        assert_eq!(
            turn_response.provider_message_id.as_deref(),
            Some("resp_done")
        );
        assert_eq!(turn_response.assistant_text, "已读取文件。");
        assert_eq!(turn_response.assistant_reasoning, "接下来需要修改函数。");
        assert_eq!(turn_response.tool_calls.len(), 1);
        assert_eq!(turn_response.tool_calls[0].id, "fc_1");
        assert_eq!(
            turn_response.tool_calls[0].tool_call_id.as_deref(),
            Some("call_1")
        );
        assert_eq!(turn_response.tool_calls[0].tool_name, "read_file");
        assert_eq!(turn_response.tool_calls[0].arguments["path"], "src/main.rs");
        assert_eq!(
            turn_response
                .continuation
                .as_ref()
                .and_then(|continuation| continuation.last_provider_message_id.as_deref()),
            Some("resp_done")
        );
    }

    /// Verifies fallback stream synthesis preserves structured output items when the provider omits `response.completed`.
    /// 验证 Provider 缺失 `response.completed` 时，流式兜底合成会保留结构化输出项。
    #[test]
    fn fallback_response_preserves_structured_output_items() {
        let output = vec![
            serde_json::from_value(json!({
                "type": "message",
                "content": [
                    { "type": "output_text", "text": "已拿到搜索结果。" }
                ]
            }))
            .expect("message item should deserialize"),
            serde_json::from_value(json!({
                "type": "function_call",
                "id": "fc_search",
                "call_id": "call_search",
                "name": "web_lookup",
                "arguments": "{\"q\":\"AMD AI MAX 395\"}"
            }))
            .expect("function call item should deserialize"),
        ];
        let response = synthesize_openai_responses_fallback_response(
            Some(String::from("resp_stream")),
            output,
            String::from("会被结构化消息覆盖"),
            String::new(),
        )
        .expect("fallback response should synthesize");

        let turn_response = build_openai_responses_turn_response("openai", "gpt-5", response)
            .expect("turn response should build");

        assert_eq!(
            turn_response.provider_message_id.as_deref(),
            Some("resp_stream")
        );
        assert_eq!(turn_response.assistant_text, "已拿到搜索结果。");
        assert_eq!(turn_response.tool_calls.len(), 1);
        assert_eq!(turn_response.tool_calls[0].tool_name, "web_lookup");
    }

    /// Verifies fallback stream synthesis can rebuild text and reasoning from deltas when no structured output item arrives.
    /// 验证在没有结构化输出项到达时，流式兜底合成仍可依据增量重建文本与思考。
    #[test]
    fn fallback_response_rebuilds_delta_only_output() {
        let response = synthesize_openai_responses_fallback_response(
            Some(String::from("resp_delta_only")),
            Vec::new(),
            String::from("已完成联网搜索。"),
            String::from("先检索，再归纳。"),
        )
        .expect("fallback response should synthesize");

        let turn_response = build_openai_responses_turn_response("openai", "gpt-5", response)
            .expect("turn response should build");

        assert_eq!(
            turn_response.provider_message_id.as_deref(),
            Some("resp_delta_only")
        );
        assert_eq!(turn_response.assistant_text, "已完成联网搜索。");
        assert_eq!(turn_response.assistant_reasoning, "先检索，再归纳。");
        assert_eq!(
            turn_response
                .continuation
                .as_ref()
                .and_then(|continuation| continuation.last_provider_message_id.as_deref()),
            Some("resp_delta_only")
        );
    }

    /// Verifies schema sanitization adds OpenAI strict-tool requirements recursively while preserving `$ref` exclusivity.
    /// 验证 schema 规范化会递归补齐 OpenAI 严格工具要求，并保留 `$ref` 的独占性。
    #[test]
    fn sanitize_schema_adds_strict_openai_constraints() {
        let mut schema = json!({
            "type": "object",
            "properties": {
                "location": {
                    "$ref": "#/$defs/Location",
                    "description": "The user's location"
                }
            },
            "$defs": {
                "Location": {
                    "type": "object",
                    "properties": {
                        "city": { "type": "string" }
                    }
                }
            }
        });

        sanitize_openai_responses_schema(&mut schema);

        assert_eq!(schema["additionalProperties"], false);
        assert_eq!(schema["required"][0], "location");
        assert_eq!(
            schema["properties"]["location"],
            json!({ "$ref": "#/$defs/Location" })
        );
        assert_eq!(schema["$defs"]["Location"]["additionalProperties"], false);
        assert_eq!(schema["$defs"]["Location"]["required"][0], "city");
    }
}
