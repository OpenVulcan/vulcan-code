//! Direct Anthropic HTTP and SSE transport owned by the host runtime.
//! 宿主运行时自有的 Anthropic HTTP 与 SSE 直连传输模块。

use crate::agent_runtime::provider_protocol_anthropic_client::normalize_anthropic_base_url;
use crate::agent_runtime::provider_protocol_anthropic_decoders::sse::iter_sse_messages;
use crate::agent_runtime::provider_protocol_bridge::{
    build_openai_compatible_turn_response, core_provider_turn_delta_to_runtime_delta,
};
use crate::agent_runtime::provider_protocol_content::{
    ProviderAssistantContent, ProviderReasoningPart,
};
use crate::agent_runtime::provider_protocol_tool_definition::ProviderToolDefinition;
use crate::agent_runtime::provider_transport_http_support::{
    build_json_http_client, ensure_json_content_type, ensure_static_header, ensure_string_header,
    extract_standard_error_message,
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

/// Latest Anthropic API version used by the host-owned direct transport.
/// 宿主自有直连传输使用的最新 Anthropic API 版本。
const ANTHROPIC_API_VERSION_LATEST: &str = "2023-06-01";

/// Minimal Anthropic streaming event used by the host-owned direct transport.
/// 宿主自有直连传输使用的最小 Anthropic 流式事件。
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AnthropicStreamingEvent {
    /// Message-start event carrying response identity and initial usage.
    /// 携带响应标识与初始用量的消息开始事件。
    MessageStart { message: AnthropicMessageStart },
    /// Content-block-start event carrying one initial content block.
    /// 携带初始内容块的内容块开始事件。
    ContentBlockStart {
        #[serde(rename = "index")]
        _index: usize,
        content_block: AnthropicContentBlock,
    },
    /// Content-block-delta event carrying one streamed content fragment.
    /// 携带单条流式内容碎片的内容块增量事件。
    ContentBlockDelta {
        #[serde(rename = "index")]
        _index: usize,
        delta: AnthropicContentDelta,
    },
    /// Content-block-stop event closing the current content block.
    /// 用于关闭当前内容块的内容块结束事件。
    ContentBlockStop {
        #[serde(rename = "index")]
        _index: usize,
    },
    /// Message-delta event carrying incremental usage and stop reason.
    /// 携带增量用量与停止原因的消息增量事件。
    MessageDelta {
        delta: AnthropicMessageDelta,
        #[serde(rename = "usage")]
        _usage: AnthropicPartialUsage,
    },
    /// Message-stop event marking the natural end of the stream.
    /// 标记流自然结束的消息停止事件。
    MessageStop,
    /// Ping event emitted by the provider to keep the stream alive.
    /// Provider 用于保活流的 Ping 事件。
    Ping,
    /// Unknown or unsupported event variant.
    /// 未知或当前不支持的事件变体。
    #[serde(other)]
    Unknown,
}

/// Minimal Anthropic message-start payload.
/// 最小 Anthropic 消息开始载荷。
#[derive(Debug, Deserialize)]
struct AnthropicMessageStart {
    /// Upstream response identifier.
    /// 上游响应标识。
    id: String,
    /// Upstream model identifier.
    /// 上游模型标识。
    model: String,
    /// Initial usage snapshot.
    /// 初始用量快照。
    #[serde(rename = "usage")]
    _usage: AnthropicUsage,
}

/// Minimal Anthropic content block used by the host-owned direct transport.
/// 宿主自有直连传输使用的最小 Anthropic 内容块。
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AnthropicContentBlock {
    /// Plain text content block.
    /// 纯文本内容块。
    Text { text: String },
    /// Tool-use content block.
    /// 工具使用内容块。
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    /// Thinking content block.
    /// 思考内容块。
    Thinking {
        thinking: String,
        signature: Option<String>,
    },
    /// Redacted thinking content block.
    /// 脱敏思考内容块。
    RedactedThinking {
        #[serde(rename = "data")]
        _data: String,
    },
}

/// Minimal Anthropic content delta used by the host-owned direct transport.
/// 宿主自有直连传输使用的最小 Anthropic 内容增量。
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AnthropicContentDelta {
    /// Streamed text delta.
    /// 流式文本增量。
    TextDelta { text: String },
    /// Streamed partial JSON for tool input.
    /// 工具输入的流式部分 JSON。
    InputJsonDelta { partial_json: String },
    /// Streamed visible thinking delta.
    /// 流式可见思考增量。
    ThinkingDelta { thinking: String },
    /// Streamed thinking signature delta.
    /// 流式思考签名增量。
    SignatureDelta { signature: String },
}

/// Minimal Anthropic message-delta payload.
/// 最小 Anthropic 消息增量载荷。
#[derive(Debug, Deserialize)]
struct AnthropicMessageDelta {
    /// Optional provider stop reason.
    /// 可选的 Provider 停止原因。
    stop_reason: Option<String>,
}

/// Minimal Anthropic full usage payload.
/// 最小 Anthropic 完整用量载荷。
#[derive(Debug, Clone, Default, Deserialize)]
struct AnthropicUsage {
    /// Input token count.
    /// 输入 token 数量。
    #[serde(rename = "input_tokens")]
    _input_tokens: u64,
    /// Output token count.
    /// 输出 token 数量。
    #[serde(rename = "output_tokens")]
    _output_tokens: u64,
}

/// Minimal Anthropic partial usage payload.
/// 最小 Anthropic 增量用量载荷。
#[derive(Debug, Clone, Default, Deserialize)]
struct AnthropicPartialUsage {
    /// Output token count accumulated so far.
    /// 当前累计的输出 token 数量。
    #[serde(rename = "output_tokens")]
    _output_tokens: usize,
}

/// Pending tool-use block assembled across multiple Anthropic stream events.
/// 跨多条 Anthropic 流事件累计组装的待定工具使用块。
#[derive(Debug, Default)]
struct PendingAnthropicToolUse {
    /// Stable tool-use identifier emitted by the provider.
    /// Provider 发出的稳定工具使用标识。
    id: String,
    /// Stable tool name emitted by the provider.
    /// Provider 发出的稳定工具名称。
    name: String,
    /// Accumulated partial JSON payload for the tool input.
    /// 为工具输入累计的部分 JSON 载荷。
    input_json: String,
}

/// Pending thinking block assembled across multiple Anthropic stream events.
/// 跨多条 Anthropic 流事件累计组装的待定思考块。
#[derive(Debug, Default)]
struct PendingAnthropicThinking {
    /// Accumulated visible thinking text.
    /// 累计的可见思考文本。
    thinking: String,
    /// Accumulated thinking signature text.
    /// 累计的思考签名文本。
    signature: String,
}

/// Executes one direct Anthropic streaming turn without going through any third-party completion-model abstraction.
/// 在不经过任何第三方 completion model 抽象的前提下执行一次直连 Anthropic 流式回合。
pub(crate) async fn execute_anthropic_http_streaming_turn<S>(
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
        build_anthropic_http_client(provider_id, provider_label, api_key, request_headers)?;
    let endpoint = build_anthropic_messages_endpoint(base_url);
    let payload = build_anthropic_request_payload(model_id, request, provider_tool_definitions)?;

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

    let mut response_id = None;
    let mut response_model = None;
    let mut assistant_text = String::new();
    let mut assistant_reasoning_blocks = Vec::new();
    let mut pending_tool_use = None::<PendingAnthropicToolUse>;
    let mut pending_thinking = None::<PendingAnthropicThinking>;
    let mut finalized_tool_calls = Vec::new();

    while let Some(event_result) = events.next().await {
        let event = event_result.map_err(|error| VulcanCoreError::ProviderFailure {
            provider_id: provider_id.to_string(),
            message: format!("{provider_label} SSE 解码失败：{error}"),
        })?;

        if event.data.trim().is_empty() {
            continue;
        }

        let event =
            serde_json::from_str::<AnthropicStreamingEvent>(&event.data).map_err(|error| {
                VulcanCoreError::ProviderFailure {
                    provider_id: provider_id.to_string(),
                    message: format!("{provider_label} 流式事件解析失败：{error}"),
                }
            })?;

        match event {
            AnthropicStreamingEvent::MessageStart { message } => {
                response_id = Some(message.id);
                response_model = Some(message.model);
            }
            AnthropicStreamingEvent::ContentBlockStart { content_block, .. } => match content_block
            {
                AnthropicContentBlock::ToolUse { id, name, input } => {
                    pending_tool_use = Some(PendingAnthropicToolUse {
                        id,
                        name,
                        input_json: if input.is_null() || input == json!({}) {
                            String::new()
                        } else {
                            serde_json::to_string(&input).unwrap_or_else(|_| input.to_string())
                        },
                    });
                }
                AnthropicContentBlock::Thinking {
                    thinking,
                    signature,
                } => {
                    pending_thinking = Some(PendingAnthropicThinking {
                        thinking,
                        signature: signature.unwrap_or_default(),
                    });
                }
                AnthropicContentBlock::RedactedThinking { .. } => {}
                AnthropicContentBlock::Text { text } => {
                    if !text.is_empty() {
                        assistant_text.push_str(&text);
                        forward_anthropic_delta(
                            live_sender,
                            sink,
                            CoreProviderTurnDelta::Output { delta: text },
                        );
                    }
                }
            },
            AnthropicStreamingEvent::ContentBlockDelta { delta, .. } => match delta {
                AnthropicContentDelta::TextDelta { text } => {
                    assistant_text.push_str(&text);
                    forward_anthropic_delta(
                        live_sender,
                        sink,
                        CoreProviderTurnDelta::Output { delta: text },
                    );
                }
                AnthropicContentDelta::InputJsonDelta { partial_json } => {
                    pending_tool_use
                        .get_or_insert_with(PendingAnthropicToolUse::default)
                        .input_json
                        .push_str(&partial_json);
                }
                AnthropicContentDelta::ThinkingDelta { thinking } => {
                    pending_thinking
                        .get_or_insert_with(PendingAnthropicThinking::default)
                        .thinking
                        .push_str(&thinking);
                    forward_anthropic_delta(
                        live_sender,
                        sink,
                        CoreProviderTurnDelta::Thinking { delta: thinking },
                    );
                }
                AnthropicContentDelta::SignatureDelta { signature } => {
                    pending_thinking
                        .get_or_insert_with(PendingAnthropicThinking::default)
                        .signature
                        .push_str(&signature);
                }
            },
            AnthropicStreamingEvent::ContentBlockStop { .. } => {
                if let Some(tool_use) = pending_tool_use.take() {
                    finalized_tool_calls.push(finalize_pending_anthropic_tool_use(tool_use)?);
                }
                if let Some(thinking) = pending_thinking.take()
                    && !thinking.thinking.is_empty()
                {
                    assistant_reasoning_blocks.push(ProviderReasoningPart::Text(thinking.thinking));
                }
            }
            AnthropicStreamingEvent::MessageDelta { delta, .. } => {
                if delta.stop_reason.as_deref() == Some("tool_use") {
                    if let Some(tool_use) = pending_tool_use.take() {
                        finalized_tool_calls.push(finalize_pending_anthropic_tool_use(tool_use)?);
                    }
                }
            }
            AnthropicStreamingEvent::MessageStop
            | AnthropicStreamingEvent::Ping
            | AnthropicStreamingEvent::Unknown => {}
        }
    }

    if let Some(tool_use) = pending_tool_use.take() {
        finalized_tool_calls.push(finalize_pending_anthropic_tool_use(tool_use)?);
    }
    if let Some(thinking) = pending_thinking.take()
        && !thinking.thinking.is_empty()
    {
        assistant_reasoning_blocks.push(ProviderReasoningPart::Text(thinking.thinking));
    }

    let mut assistant_content = Vec::new();
    if !assistant_text.is_empty() {
        assistant_content.push(ProviderAssistantContent::Text(assistant_text));
    }
    if !assistant_reasoning_blocks.is_empty() {
        assistant_content.push(ProviderAssistantContent::Reasoning(
            assistant_reasoning_blocks,
        ));
    }
    assistant_content.extend(
        finalized_tool_calls
            .into_iter()
            .map(ProviderAssistantContent::ToolCall),
    );

    Ok(build_openai_compatible_turn_response(
        provider_id,
        response_model.as_deref().unwrap_or(model_id),
        response_id,
        &assistant_content,
    ))
}

/// Builds one direct Anthropic HTTP client with merged API key, version, and caller-supplied request headers.
/// 构建一个直连 Anthropic HTTP 客户端，并合并 API Key、版本头与调用方提供的请求头。
fn build_anthropic_http_client(
    provider_id: &str,
    provider_label: &str,
    api_key: &str,
    mut request_headers: HeaderMap,
) -> Result<reqwest::Client, VulcanCoreError> {
    ensure_string_header(
        provider_id,
        provider_label,
        &mut request_headers,
        "x-api-key",
        api_key,
        "API Key ",
    )?;
    ensure_static_header(
        &mut request_headers,
        "anthropic-version",
        ANTHROPIC_API_VERSION_LATEST,
    );
    ensure_json_content_type(&mut request_headers);
    build_json_http_client(provider_id, provider_label, request_headers)
}

/// Builds the final Anthropic `/v1/messages` endpoint from one optional base URL override.
/// 根据可选基础地址覆盖值构建最终 Anthropic `/v1/messages` 端点。
fn build_anthropic_messages_endpoint(base_url: Option<&str>) -> String {
    let normalized_base = base_url
        .filter(|value| !value.trim().is_empty())
        .map(normalize_anthropic_base_url)
        .unwrap_or_else(|| String::from("https://api.anthropic.com"));
    format!("{}/v1/messages", normalized_base.trim_end_matches('/'))
}

/// Builds the final Anthropic JSON payload from one host-controlled turn request and tool schema set.
/// 根据一份宿主控制回合请求与工具 schema 集合构建最终 Anthropic JSON 载荷。
fn build_anthropic_request_payload(
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

    payload.insert(String::from("model"), Value::String(model_id.to_string()));
    let max_tokens = extract_anthropic_max_tokens(model_id, &mut payload);
    payload.insert(String::from("max_tokens"), json!(max_tokens));
    payload.insert(String::from("stream"), Value::Bool(true));

    if !snapshot.system_prompt.trim().is_empty() {
        payload.insert(
            String::from("system"),
            Value::Array(vec![json!({
                "type": "text",
                "text": snapshot.system_prompt
            })]),
        );
    }

    if let Some(temperature) = snapshot.temperature {
        payload.insert(String::from("temperature"), json!(temperature));
    }

    payload.insert(
        String::from("messages"),
        Value::Array(
            current_history
                .iter()
                .map(core_message_to_anthropic_message)
                .chain(std::iter::once(core_message_to_anthropic_message(
                    &current_prompt,
                )))
                .collect::<Result<Vec<_>, _>>()?,
        ),
    );

    if !provider_tool_definitions.is_empty() {
        payload.insert(
            String::from("tools"),
            Value::Array(
                provider_tool_definitions
                    .iter()
                    .map(provider_tool_definition_to_anthropic_tool)
                    .collect(),
            ),
        );
        payload.entry(String::from("tool_choice")).or_insert(json!({
            "type": "auto"
        }));
    }

    Ok(Value::Object(payload))
}

/// Extracts one Anthropic-compatible `max_tokens` value from the payload or falls back to a model-aware default.
/// 从载荷中提取一个兼容 Anthropic 的 `max_tokens`，或回退到按模型感知的默认值。
fn extract_anthropic_max_tokens(model_id: &str, payload: &mut JsonMap<String, Value>) -> u64 {
    if let Some(max_tokens) = payload.remove("max_tokens")
        && let Some(value) = max_tokens.as_u64()
    {
        return value;
    }

    if model_id.starts_with("claude-opus-4-7") || model_id.starts_with("claude-opus-4-6") {
        128_000
    } else if model_id.starts_with("claude-opus-4")
        || model_id.starts_with("claude-sonnet-4")
        || model_id.starts_with("claude-haiku-4-5")
    {
        64_000
    } else {
        2_048
    }
}

/// Converts one canonical core message into one Anthropic request message while intentionally dropping local-only reasoning replay.
/// 将一条规范 core 消息转换为一条 Anthropic 请求消息，同时刻意丢弃仅限本地使用的思考回放。
fn core_message_to_anthropic_message(
    message: &CoreConversationMessage,
) -> Result<Value, VulcanCoreError> {
    match &message.payload {
        CoreConversationPayload::UserText { text } => Ok(json!({
            "role": "user",
            "content": [
                {
                    "type": "text",
                    "text": text
                }
            ]
        })),
        CoreConversationPayload::Assistant {
            text, tool_calls, ..
        } => {
            let mut content = Vec::new();
            if !text.is_empty() {
                content.push(json!({
                    "type": "text",
                    "text": text
                }));
            }
            content.extend(
                tool_calls
                    .iter()
                    .map(core_tool_call_to_anthropic_tool_use)
                    .collect::<Vec<_>>(),
            );
            Ok(json!({
                "role": "assistant",
                "content": content
            }))
        }
        CoreConversationPayload::ToolResult {
            tool_call_id, text, ..
        } => {
            let Some(tool_use_id) = tool_call_id
                .clone()
                .filter(|value| !value.trim().is_empty())
            else {
                return Err(VulcanCoreError::InvalidState {
                    message: String::from(
                        "Anthropic 工具结果消息缺少 tool_call_id，无法构造 provider 请求。",
                    ),
                });
            };
            Ok(json!({
                "role": "user",
                "content": [
                    {
                        "type": "tool_result",
                        "tool_use_id": tool_use_id,
                        "content": [
                            {
                                "type": "text",
                                "text": text
                            }
                        ]
                    }
                ]
            }))
        }
    }
}

/// Converts one canonical core tool call into one Anthropic `tool_use` block.
/// 将一条规范 core 工具调用转换为一条 Anthropic `tool_use` 内容块。
fn core_tool_call_to_anthropic_tool_use(tool_call: &CoreConversationToolCall) -> Value {
    let replayable_tool_call_id = tool_call
        .tool_call_id
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| tool_call.id.clone());
    json!({
        "type": "tool_use",
        "id": replayable_tool_call_id,
        "name": tool_call.tool_name,
        "input": tool_call.arguments
    })
}

/// Converts one host-owned provider tool definition into one Anthropic tool schema object.
/// 将一条宿主自有 Provider 工具定义转换为一条 Anthropic 工具 schema 对象。
fn provider_tool_definition_to_anthropic_tool(definition: &ProviderToolDefinition) -> Value {
    json!({
        "name": definition.name,
        "description": definition.description,
        "input_schema": definition.parameters
    })
}

/// Finalizes one pending Anthropic tool-use block into the canonical core tool-call shape.
/// 将一条待定 Anthropic 工具使用块收尾为规范 core 工具调用形状。
fn finalize_pending_anthropic_tool_use(
    tool_use: PendingAnthropicToolUse,
) -> Result<CoreConversationToolCall, VulcanCoreError> {
    let input_json = if tool_use.input_json.trim().is_empty() {
        json!({})
    } else {
        serde_json::from_str::<Value>(&tool_use.input_json).map_err(|error| {
            VulcanCoreError::ProviderFailure {
                provider_id: String::from("anthropic"),
                message: format!("Anthropic 工具输入 JSON 解析失败：{error}"),
            }
        })?
    };

    let provider_visible_id = tool_use.id.trim().to_string();
    let provider_visible_id = (!provider_visible_id.is_empty()).then_some(provider_visible_id);
    let local_id = provider_visible_id.clone().unwrap_or_else(|| {
        format!(
            "{}:{}",
            tool_use.name,
            serde_json::to_string(&input_json).unwrap_or_else(|_| input_json.to_string())
        )
    });

    Ok(CoreConversationToolCall {
        id: local_id,
        tool_call_id: provider_visible_id,
        tool_name: tool_use.name,
        arguments: input_json,
    })
}

/// Forwards one normalized provider delta into both the controller sink and the GUI live sender.
/// 将一条规范 Provider 增量同时转发给控制器汇聚器与 GUI 实时发送器。
fn forward_anthropic_delta<S>(
    live_sender: &ProviderLiveDeltaSender,
    sink: &mut S,
    delta: CoreProviderTurnDelta,
) where
    S: VulcanProviderDeltaSink,
{
    sink.push_delta(delta.clone());
    let _ = live_sender.send(Ok(core_provider_turn_delta_to_runtime_delta(delta)));
}

#[cfg(test)]
mod tests {
    use super::{
        PendingAnthropicToolUse, build_anthropic_request_payload,
        finalize_pending_anthropic_tool_use,
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

    /// Verifies the direct Anthropic request payload strips reasoning replay while preserving tool-use and tool-result message shapes.
    /// 验证直连 Anthropic 请求载荷会剥离思考回放，同时保留 tool_use 与 tool_result 消息形状。
    #[test]
    fn request_payload_strips_reasoning_replay_and_preserves_anthropic_tool_shapes() {
        let request = OpenAiCompatibleCoreTurnRequest {
            snapshot: CoreProviderRequestSnapshot {
                provider_id: String::from("anthropic"),
                model_id: String::from("claude-sonnet-4-6"),
                system_prompt: String::from("system prompt"),
                prompt: String::from("tool result body"),
                history: Vec::new(),
                temperature: Some(0.2),
                max_turns: 8,
                tool_schemas: Vec::new(),
                additional_params: Some(json!({})),
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
                        id: String::from("tool-1"),
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

        let payload = build_anthropic_request_payload(
            "claude-sonnet-4-6",
            request,
            &[ProviderToolDefinition {
                name: String::from("read_file"),
                description: String::from("Read one file"),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" }
                    },
                    "required": ["path"]
                }),
            }],
        )
        .expect("payload should build");

        let messages = payload["messages"]
            .as_array()
            .expect("messages should be an array");
        assert_eq!(payload["max_tokens"], 64000);
        assert_eq!(messages[0]["role"], "assistant");
        assert_eq!(messages[0]["content"][0]["type"], "text");
        assert_eq!(messages[0]["content"][1]["type"], "tool_use");
        assert_eq!(messages[0]["content"][1]["id"], "call-1");
        assert_eq!(messages[0]["content"][1]["name"], "read_file");
        assert!(messages[0].get("reasoning").is_none());
        assert_eq!(messages[1]["role"], "user");
        assert_eq!(messages[1]["content"][0]["type"], "tool_result");
        assert_eq!(messages[1]["content"][0]["tool_use_id"], "call-1");
        assert_eq!(
            messages[1]["content"][0]["content"][0]["text"],
            "tool result body"
        );
        assert_eq!(payload["tools"][0]["name"], "read_file");
        assert_eq!(payload["tool_choice"]["type"], "auto");
    }

    /// Verifies Anthropic replay falls back to the local tool-call identifier when the upstream provider omits a stable visible id.
    /// 验证当上游 Provider 缺失稳定可见 id 时，Anthropic 回放会回退到本地工具调用标识。
    #[test]
    fn request_payload_falls_back_to_local_tool_id_when_provider_visible_id_is_missing() {
        let request = OpenAiCompatibleCoreTurnRequest {
            snapshot: CoreProviderRequestSnapshot {
                provider_id: String::from("anthropic"),
                model_id: String::from("claude-sonnet-4-6"),
                system_prompt: String::from("system prompt"),
                prompt: String::from("tool result body"),
                history: Vec::new(),
                temperature: Some(0.2),
                max_turns: 8,
                tool_schemas: Vec::new(),
                additional_params: Some(json!({})),
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
                        id: String::from("local-call-1"),
                        tool_call_id: Some(String::from("")),
                        tool_name: String::from("read_file"),
                        arguments: json!({ "path": "src/main.rs" }),
                    }],
                },
            }],
            current_prompt: CoreConversationMessage {
                id: String::from("tool-result-1"),
                payload: CoreConversationPayload::ToolResult {
                    tool_result_id: String::from("result-1"),
                    tool_call_id: Some(String::from("local-call-1")),
                    text: String::from("tool result body"),
                },
            },
        };

        let payload = build_anthropic_request_payload(
            "claude-sonnet-4-6",
            request,
            &[ProviderToolDefinition {
                name: String::from("read_file"),
                description: String::from("Read one file"),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" }
                    },
                    "required": ["path"]
                }),
            }],
        )
        .expect("payload should build");

        let messages = payload["messages"]
            .as_array()
            .expect("messages should be an array");
        assert_eq!(messages[0]["content"][1]["id"], "local-call-1");
        assert_eq!(messages[1]["content"][0]["tool_use_id"], "local-call-1");
    }

    /// Verifies Anthropic tool-use finalization synthesizes one stable local identifier when the provider omits the upstream id.
    /// 验证当 Provider 缺失上游 id 时，Anthropic 工具使用收尾会合成一个稳定的本地标识。
    #[test]
    fn finalize_pending_anthropic_tool_use_synthesizes_local_identifier() {
        let tool_call = finalize_pending_anthropic_tool_use(PendingAnthropicToolUse {
            id: String::new(),
            name: String::from("read_file"),
            input_json: String::from("{\"path\":\"src/main.rs\"}"),
        })
        .expect("tool call should finalize");

        assert_eq!(tool_call.id, "read_file:{\"path\":\"src/main.rs\"}");
        assert_eq!(tool_call.tool_call_id, None);
        assert_eq!(tool_call.tool_name, "read_file");
        assert_eq!(tool_call.arguments, json!({ "path": "src/main.rs" }));
    }
}
