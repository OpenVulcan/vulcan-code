//! Direct OpenAI Responses HTTP and SSE transport owned by the host runtime.
//! 宿主运行时自有的 OpenAI Responses HTTP 与 SSE 直连传输模块。

use crate::agent_runtime::provider_protocol_anthropic_decoders::sse::iter_sse_messages;
use crate::agent_runtime::provider_protocol_bridge::{
    build_openai_compatible_turn_response, core_provider_turn_delta_to_runtime_delta,
};
use crate::agent_runtime::provider_protocol_content::{
    ProviderAssistantContent, ProviderReasoningPart,
};
use crate::agent_runtime::provider_protocol_tool_definition::ProviderToolDefinition;
use crate::agent_runtime::provider_transport_http_support::{
    build_json_http_client, build_rest_endpoint, ensure_bearer_authorization_header,
    ensure_json_content_type, extract_standard_error_message,
};
use crate::agent_runtime::provider_transport_live_delta::ProviderLiveDeltaSender;
use crate::agent_runtime::provider_transport_turn_request::OpenAiCompatibleCoreTurnRequest;
use futures_util::StreamExt;
use reqwest::header::HeaderMap;
use serde::Deserialize;
use serde_json::{Map as JsonMap, Value, json};
use vulcan_core::provider::VulcanProviderDeltaSink;
use vulcan_core::types::{
    ConversationMessage as CoreConversationMessage, ConversationPayload as CoreConversationPayload,
    ConversationToolCall as CoreConversationToolCall, CoreError as VulcanCoreError,
    ProviderTurnDelta as CoreProviderTurnDelta, ProviderTurnResponse as CoreProviderTurnResponse,
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
    let client =
        build_openai_responses_http_client(provider_id, provider_label, api_key, request_headers)?;
    let endpoint = build_openai_responses_endpoint(base_url);
    let payload =
        build_openai_responses_request_payload(model_id, request, provider_tool_definitions)?;
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
        let message = extract_standard_error_message(&body)
            .unwrap_or_else(|| format!("HTTP {status}: {}", body.trim()));
        return Err(VulcanCoreError::ProviderFailure {
            provider_id: provider_id.to_string(),
            message: format!("{provider_label} 请求失败：{message}"),
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
            Some("response.output_text.delta") | Some("response.refusal.delta") => {
                if let Some(delta) = value.get("delta").and_then(Value::as_str)
                    && !delta.is_empty()
                {
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
                    forward_openai_responses_delta(
                        live_sender,
                        sink,
                        CoreProviderTurnDelta::Thinking {
                            delta: delta.to_string(),
                        },
                    );
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
    } = request;

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

    if !snapshot.system_prompt.trim().is_empty() {
        payload.insert(
            String::from("instructions"),
            Value::String(snapshot.system_prompt.clone()),
        );
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
        CoreConversationPayload::UserText { text } => Ok(vec![json!({
            "role": "user",
            "content": [
                {
                    "type": "input_text",
                    "text": text
                }
            ]
        })]),
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
    ))
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
        sanitize_openai_responses_schema,
    };
    use crate::agent_runtime::provider_protocol_tool_definition::ProviderToolDefinition;
    use crate::agent_runtime::provider_transport_turn_request::OpenAiCompatibleCoreTurnRequest;
    use serde_json::json;
    use vulcan_core::types::{
        ConversationMessage as CoreConversationMessage,
        ConversationPayload as CoreConversationPayload,
        ConversationToolCall as CoreConversationToolCall,
        ProviderContinuation as CoreProviderContinuation,
        ProviderRequestSnapshot as CoreProviderRequestSnapshot,
    };

    /// Verifies the direct OpenAI Responses request payload strips reasoning replay, injects chained continuation, and preserves function-call output shapes.
    /// 验证直连 OpenAI Responses 请求载荷会剥离思考回放、注入串联 continuation，并保留 function-call output 形状。
    #[test]
    fn request_payload_strips_reasoning_replay_and_preserves_responses_shapes() {
        let request = OpenAiCompatibleCoreTurnRequest {
            snapshot: CoreProviderRequestSnapshot {
                provider_id: String::from("openai"),
                model_id: String::from("gpt-5"),
                system_prompt: String::from("system prompt"),
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
