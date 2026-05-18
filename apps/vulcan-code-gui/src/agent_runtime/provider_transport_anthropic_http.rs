//! Direct Anthropic HTTP and SSE transport owned by the host runtime.
//! 宿主运行时自有的 Anthropic HTTP 与 SSE 直连传输模块。

use crate::agent_runtime::provider_attachment::{
    build_attachment_transport_error_text, build_missing_attachment_replay_text,
    read_provider_attachment,
};
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
use crate::provider_config::ProviderApiFamily;
use crate::provider_input_normalizer::normalize_provider_user_input;
use crate::tooling::build_direct_public_tool_name;
use futures_util::StreamExt;
use reqwest::header::HeaderMap;
use serde::{Deserialize, Serialize};
use serde_json::{Map as JsonMap, Value, json};
use vulcan_core::provider::VulcanProviderDeltaSink;
use vulcan_core::types::{
    ConversationMessage as CoreConversationMessage, ConversationPayload as CoreConversationPayload,
    ConversationToolCall as CoreConversationToolCall, CoreError as VulcanCoreError,
    ProviderTurnDelta as CoreProviderTurnDelta, ProviderTurnResponse as CoreProviderTurnResponse,
    ProviderUsageSnapshot as CoreProviderUsageSnapshot,
};

/// Latest Anthropic API version used by the host-owned direct transport.
/// 宿主自有直连传输使用的最新 Anthropic API 版本。
const ANTHROPIC_API_VERSION_LATEST: &str = "2023-06-01";

/// Provider-message-state key used to persist replayable Anthropic-native assistant content.
/// 用于持久化可回放 Anthropic 原生助手内容的 provider_message_state 键名。
const ANTHROPIC_PROVIDER_STATE_KEY: &str = "anthropic_native";

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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// Provider-native server-side tool use such as Anthropic web search.
    /// Provider 原生服务端工具使用，例如 Anthropic 网页搜索。
    ServerToolUse {
        id: String,
        name: String,
        #[serde(default)]
        input: Value,
    },
    /// Provider-native web-search result block returned by Anthropic.
    /// Anthropic 返回的 Provider 原生网页搜索结果块。
    WebSearchToolResult {
        tool_use_id: String,
        #[serde(default)]
        content: Value,
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

/// Converts one full Anthropic usage payload into the shared normalized usage snapshot.
/// 将一份完整 Anthropic 用量载荷转换为共享规范化用量快照。
fn anthropic_usage_to_snapshot(usage: &AnthropicUsage) -> Option<CoreProviderUsageSnapshot> {
    let snapshot = CoreProviderUsageSnapshot {
        input_tokens: Some(usage._input_tokens as usize),
        output_tokens: Some(usage._output_tokens as usize),
        reasoning_tokens: None,
        total_tokens: Some((usage._input_tokens.saturating_add(usage._output_tokens)) as usize),
    };
    snapshot.has_any_tokens().then_some(snapshot)
}

/// Merges one Anthropic partial usage payload into the current normalized usage snapshot.
/// 将一份 Anthropic 增量用量载荷合并进当前规范化用量快照。
fn merge_anthropic_partial_usage(
    current: Option<CoreProviderUsageSnapshot>,
    partial: &AnthropicPartialUsage,
) -> Option<CoreProviderUsageSnapshot> {
    let mut snapshot = current.unwrap_or_default();
    if partial._output_tokens > 0 {
        snapshot.output_tokens = Some(partial._output_tokens);
    }
    if let (Some(input_tokens), Some(output_tokens)) =
        (snapshot.input_tokens, snapshot.output_tokens)
    {
        snapshot.total_tokens = Some(input_tokens.saturating_add(output_tokens));
    }
    snapshot.has_any_tokens().then_some(snapshot)
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

/// Pending provider-native server-side tool-use block assembled across stream events.
/// 跨流事件累计组装的待定 Provider 原生服务端工具使用块。
#[derive(Debug, Default)]
struct PendingAnthropicServerToolUse {
    /// Stable server-tool-use identifier emitted by the provider.
    /// Provider 发出的稳定服务端工具使用标识。
    id: String,
    /// Stable server-tool name emitted by the provider.
    /// Provider 发出的稳定服务端工具名称。
    name: String,
    /// Accumulated partial JSON payload for server-tool input.
    /// 为服务端工具输入累计的部分 JSON 载荷。
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
    let mut usage_snapshot = None::<CoreProviderUsageSnapshot>;
    let mut assistant_text = String::new();
    let mut assistant_reasoning_blocks = Vec::new();
    let mut finalized_provider_content_blocks = Vec::new();
    let mut pending_tool_use = None::<PendingAnthropicToolUse>;
    let mut pending_server_tool_use = None::<PendingAnthropicServerToolUse>;
    let mut pending_thinking = None::<PendingAnthropicThinking>;
    let mut pending_text = None::<String>;
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
                usage_snapshot = anthropic_usage_to_snapshot(&message._usage);
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
                AnthropicContentBlock::ServerToolUse { id, name, input } => {
                    pending_server_tool_use = Some(PendingAnthropicServerToolUse {
                        id,
                        name,
                        input_json: if input.is_null() || input == json!({}) {
                            String::new()
                        } else {
                            serde_json::to_string(&input).unwrap_or_else(|_| input.to_string())
                        },
                    });
                }
                AnthropicContentBlock::WebSearchToolResult {
                    tool_use_id,
                    content,
                } => {
                    finalized_provider_content_blocks.push(
                        AnthropicContentBlock::WebSearchToolResult {
                            tool_use_id,
                            content,
                        },
                    );
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
                AnthropicContentBlock::RedactedThinking { _data } => {
                    assistant_reasoning_blocks.push(ProviderReasoningPart::Opaque);
                    finalized_provider_content_blocks
                        .push(AnthropicContentBlock::RedactedThinking { _data });
                }
                AnthropicContentBlock::Text { text } => {
                    pending_text = Some(text.clone());
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
                    pending_text.get_or_insert_with(String::new).push_str(&text);
                    assistant_text.push_str(&text);
                    forward_anthropic_delta(
                        live_sender,
                        sink,
                        CoreProviderTurnDelta::Output { delta: text },
                    );
                }
                AnthropicContentDelta::InputJsonDelta { partial_json } => {
                    if let Some(server_tool_use) = pending_server_tool_use.as_mut() {
                        server_tool_use.input_json.push_str(&partial_json);
                    } else {
                        pending_tool_use
                            .get_or_insert_with(PendingAnthropicToolUse::default)
                            .input_json
                            .push_str(&partial_json);
                    }
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
                if let Some(text) = pending_text.take()
                    && !text.is_empty()
                {
                    finalized_provider_content_blocks.push(AnthropicContentBlock::Text { text });
                }
                if let Some(tool_use) = pending_tool_use.take() {
                    finalized_provider_content_blocks
                        .push(anthropic_content_block_from_pending_tool_use(&tool_use)?);
                    finalized_tool_calls.push(finalize_pending_anthropic_tool_use(
                        tool_use,
                        provider_tool_definitions,
                    )?);
                }
                if let Some(server_tool_use) = pending_server_tool_use.take() {
                    finalized_provider_content_blocks.push(
                        anthropic_content_block_from_pending_server_tool_use(&server_tool_use),
                    );
                }
                if let Some(thinking) = pending_thinking.take()
                    && !thinking.thinking.is_empty()
                {
                    finalized_provider_content_blocks.push(AnthropicContentBlock::Thinking {
                        thinking: thinking.thinking.clone(),
                        signature: (!thinking.signature.is_empty()).then_some(thinking.signature),
                    });
                    assistant_reasoning_blocks.push(ProviderReasoningPart::Text(thinking.thinking));
                }
            }
            AnthropicStreamingEvent::MessageDelta { delta, _usage } => {
                usage_snapshot = merge_anthropic_partial_usage(usage_snapshot.take(), &_usage);
                if delta.stop_reason.as_deref() == Some("tool_use") {
                    if let Some(tool_use) = pending_tool_use.take() {
                        finalized_provider_content_blocks
                            .push(anthropic_content_block_from_pending_tool_use(&tool_use)?);
                        finalized_tool_calls.push(finalize_pending_anthropic_tool_use(
                            tool_use,
                            provider_tool_definitions,
                        )?);
                    }
                    if let Some(server_tool_use) = pending_server_tool_use.take() {
                        finalized_provider_content_blocks.push(
                            anthropic_content_block_from_pending_server_tool_use(&server_tool_use),
                        );
                    }
                }
            }
            AnthropicStreamingEvent::MessageStop
            | AnthropicStreamingEvent::Ping
            | AnthropicStreamingEvent::Unknown => {}
        }
    }

    if let Some(tool_use) = pending_tool_use.take() {
        finalized_provider_content_blocks
            .push(anthropic_content_block_from_pending_tool_use(&tool_use)?);
        finalized_tool_calls.push(finalize_pending_anthropic_tool_use(
            tool_use,
            provider_tool_definitions,
        )?);
    }
    if let Some(server_tool_use) = pending_server_tool_use.take() {
        finalized_provider_content_blocks.push(
            anthropic_content_block_from_pending_server_tool_use(&server_tool_use),
        );
    }
    if let Some(text) = pending_text.take()
        && !text.is_empty()
    {
        finalized_provider_content_blocks.push(AnthropicContentBlock::Text { text });
    }
    if let Some(thinking) = pending_thinking.take()
        && !thinking.thinking.is_empty()
    {
        finalized_provider_content_blocks.push(AnthropicContentBlock::Thinking {
            thinking: thinking.thinking.clone(),
            signature: (!thinking.signature.is_empty()).then_some(thinking.signature),
        });
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

    let mut response = build_openai_compatible_turn_response(
        provider_id,
        response_model.as_deref().unwrap_or(model_id),
        response_id,
        &assistant_content,
        usage_snapshot,
    );
    response.provider_message_state = (!finalized_provider_content_blocks.is_empty())
        .then(|| build_anthropic_provider_message_state(&finalized_provider_content_blocks));
    Ok(response)
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

/// Wraps replayable Anthropic-native assistant content into the generic provider-message-state payload stored in canonical assistant messages.
/// 将可回放的 Anthropic 原生助手内容包装成存入规范助手消息中的通用 provider_message_state 载荷。
fn build_anthropic_provider_message_state(content: &[AnthropicContentBlock]) -> Value {
    json!({
        ANTHROPIC_PROVIDER_STATE_KEY: {
            "content": content
        }
    })
}

/// Restores replayable Anthropic-native assistant content from the generic provider-message-state payload when future history replay needs provider-native fidelity.
/// 当后续历史回放需要 Provider 原生精度时，从通用 provider_message_state 载荷中恢复可回放的 Anthropic 原生助手内容。
fn parse_anthropic_provider_message_state(state: &Value) -> Option<Vec<AnthropicContentBlock>> {
    state
        .get(ANTHROPIC_PROVIDER_STATE_KEY)?
        .get("content")
        .cloned()
        .and_then(|value| serde_json::from_value::<Vec<AnthropicContentBlock>>(value).ok())
}

/// Coalesces consecutive Anthropic user messages into one content array so one assistant tool-use turn can replay multiple tool results as a single legal follow-up message.
/// 将连续的 Anthropic user 消息合并为同一个内容数组，确保同一条助手工具调用回合产生的多个工具结果会以一条合法的后续消息回放。
///
/// This regrouping is intentionally Anthropic-specific and must not be generalized to other provider transports, because their native replay rules still accept one result message per tool call.
/// 这段重组逻辑被刻意限制为 Anthropic 专用，不能泛化到其他 Provider 传输链路，因为其他 Provider 的原生回放规则仍然接受每次工具调用对应一条结果消息。
fn coalesce_consecutive_anthropic_user_messages(messages: Vec<Value>) -> Vec<Value> {
    let mut merged_messages: Vec<Value> = Vec::with_capacity(messages.len());

    for message in messages {
        let Some(message_tool_results) = anthropic_user_tool_result_blocks(&message) else {
            merged_messages.push(message);
            continue;
        };

        let Some(previous_message) = merged_messages.last_mut() else {
            merged_messages.push(message);
            continue;
        };

        let previous_is_tool_result_only =
            anthropic_user_tool_result_blocks(previous_message).is_some();
        if previous_is_tool_result_only
            && let Some(previous_content) = previous_message
                .get_mut("content")
                .and_then(Value::as_array_mut)
        {
            // Merge only adjacent tool-result turns because Anthropic binds tool_result blocks to the immediately preceding assistant tool_use turn.
            // 仅合并相邻的工具结果回合，因为 Anthropic 会把 tool_result 内容块绑定到紧邻的上一条 assistant tool_use 回合。
            previous_content.extend(message_tool_results);
            continue;
        }

        merged_messages.push(message);
    }

    merged_messages
}

/// Extracts one Anthropic user message content array only when every block is a `tool_result`, so asynchronous task acknowledgements can be regrouped without mixing in ordinary user text.
/// 仅当一条 Anthropic user 消息的所有内容块都是 `tool_result` 时提取其内容数组，以便在不混入普通用户文本的前提下重组异步任务确认消息。
fn anthropic_user_tool_result_blocks(message: &Value) -> Option<Vec<Value>> {
    let role = message.get("role").and_then(Value::as_str)?;
    if role != "user" {
        return None;
    }

    let content = message.get("content").and_then(Value::as_array)?;
    if content
        .iter()
        .all(|block| block.get("type").and_then(Value::as_str) == Some("tool_result"))
    {
        Some(content.clone())
    } else {
        None
    }
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
        ..
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
        Value::Array(coalesce_consecutive_anthropic_user_messages(
            current_history
                .iter()
                .map(|message| {
                    core_message_to_anthropic_message(message, provider_tool_definitions)
                })
                .chain(std::iter::once(core_message_to_anthropic_message(
                    &current_prompt,
                    provider_tool_definitions,
                )))
                .collect::<Result<Vec<_>, _>>()?,
        )),
    );

    append_anthropic_function_tools(&mut payload, provider_tool_definitions)?;

    Ok(Value::Object(payload))
}

/// Appends Anthropic function tools without replacing provider-native server tools already present in the payload.
/// 追加 Anthropic 函数工具，同时不替换载荷中已经存在的 Provider 原生服务端工具。
fn append_anthropic_function_tools(
    payload: &mut JsonMap<String, Value>,
    provider_tool_definitions: &[ProviderToolDefinition],
) -> Result<(), VulcanCoreError> {
    if provider_tool_definitions.is_empty() {
        return Ok(());
    }

    let tools = payload
        .entry(String::from("tools"))
        .or_insert_with(|| Value::Array(Vec::new()));
    let Some(tools) = tools.as_array_mut() else {
        return Err(VulcanCoreError::InvalidState {
            message: String::from("Anthropic 请求载荷中的 tools 字段不是数组。"),
        });
    };
    tools.extend(
        provider_tool_definitions
            .iter()
            .map(provider_tool_definition_to_anthropic_tool),
    );
    payload.entry(String::from("tool_choice")).or_insert(json!({
        "type": "auto"
    }));
    Ok(())
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

/// Converts one canonical core message into one Anthropic request message while preserving stored provider-native replay state for tool turns.
/// 将一条规范 core 消息转换为一条 Anthropic 请求消息，并在工具回合中优先保留已存储的 Provider 原生回放状态。
fn core_message_to_anthropic_message(
    message: &CoreConversationMessage,
    provider_tool_definitions: &[ProviderToolDefinition],
) -> Result<Value, VulcanCoreError> {
    match &message.payload {
        CoreConversationPayload::UserText { text, attachments } => {
            let normalized = normalize_provider_user_input(
                ProviderApiFamily::AnthropicMessages,
                text,
                attachments,
            );
            let mut content = Vec::new();
            if !normalized.text.is_empty() {
                content.push(json!({
                    "type": "text",
                    "text": normalized.text
                }));
            }
            for attachment in &normalized.attachments {
                let attachment_payload = match read_provider_attachment(attachment)? {
                    Some(payload) => payload,
                    None => {
                        content.push(json!({
                            "type": "text",
                            "text": build_missing_attachment_replay_text(attachment)
                        }));
                        continue;
                    }
                };
                match attachment_payload.kind {
                    vulcan_core::types::ConversationAttachmentKind::Image => {
                        content.push(json!({
                            "type": "image",
                            "source": {
                                "type": "base64",
                                "media_type": attachment_payload.mime_type,
                                "data": attachment_payload.base64_data
                            }
                        }));
                    }
                    vulcan_core::types::ConversationAttachmentKind::Pdf
                        if attachment_payload.is_pdf_document() =>
                    {
                        content.push(json!({
                            "type": "document",
                            "source": {
                                "type": "base64",
                                "media_type": attachment_payload.mime_type,
                                "data": attachment_payload.base64_data
                            }
                        }));
                    }
                    vulcan_core::types::ConversationAttachmentKind::Audio
                    | vulcan_core::types::ConversationAttachmentKind::Video
                    | vulcan_core::types::ConversationAttachmentKind::Pdf => {
                        content.push(json!({
                            "type": "text",
                            "text": build_attachment_transport_error_text(
                                attachment,
                                "this API path"
                            )
                        }));
                    }
                }
            }
            Ok(json!({
                "role": "user",
                "content": content
            }))
        }
        CoreConversationPayload::Assistant {
            text,
            reasoning,
            provider_message_state,
            tool_calls,
            ..
        } => {
            if let Some(content) = provider_message_state
                .as_ref()
                .and_then(parse_anthropic_provider_message_state)
            {
                let replayable_content =
                    repair_replayable_anthropic_provider_content(content, tool_calls);
                return Ok(json!({
                    "role": "assistant",
                    "content": replayable_content
                }));
            }

            let mut content = Vec::new();
            if !tool_calls.is_empty() && !reasoning.trim().is_empty() {
                content.push(json!({
                    "type": "thinking",
                    "thinking": reasoning
                }));
            }
            if !text.is_empty() {
                content.push(json!({
                    "type": "text",
                    "text": text
                }));
            }
            content.extend(
                tool_calls
                    .iter()
                    .map(|tool_call| {
                        core_tool_call_to_anthropic_tool_use(tool_call, provider_tool_definitions)
                    })
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

/// Parses one pending Anthropic tool-use input JSON payload into a structured value used by both transcript storage and provider-state replay.
/// 将一条待定 Anthropic 工具使用输入 JSON 载荷解析为结构化值，供转录存储与 provider-state 回放共同使用。
fn parse_pending_anthropic_tool_use_input_json(
    tool_use: &PendingAnthropicToolUse,
) -> Result<Value, VulcanCoreError> {
    if tool_use.input_json.trim().is_empty() {
        Ok(json!({}))
    } else {
        serde_json::from_str::<Value>(&tool_use.input_json).map_err(|error| {
            VulcanCoreError::ProviderFailure {
                provider_id: String::from("anthropic"),
                message: format!("Anthropic 工具输入 JSON 解析失败：{error}"),
            }
        })
    }
}

/// Converts one pending Anthropic tool-use assembly into one replayable Anthropic-native content block so future history replays preserve provider-visible identifiers and names.
/// 将一条待定 Anthropic 工具使用累计结果转换为一条可回放的 Anthropic 原生内容块，以便后续历史回放保留 Provider 可见标识与名称。
fn anthropic_content_block_from_pending_tool_use(
    tool_use: &PendingAnthropicToolUse,
) -> Result<AnthropicContentBlock, VulcanCoreError> {
    let input_json = parse_pending_anthropic_tool_use_input_json(tool_use)?;
    Ok(AnthropicContentBlock::ToolUse {
        id: replayable_anthropic_tool_use_id(&tool_use.id, &tool_use.name, &input_json),
        name: tool_use.name.clone(),
        input: input_json,
    })
}

/// Converts one pending Anthropic server-side tool-use assembly into a replayable provider-native content block.
/// 将一条待定 Anthropic 服务端工具使用累计结果转换为可回放的 Provider 原生内容块。
fn anthropic_content_block_from_pending_server_tool_use(
    tool_use: &PendingAnthropicServerToolUse,
) -> AnthropicContentBlock {
    let input = if tool_use.input_json.trim().is_empty() {
        json!({})
    } else {
        serde_json::from_str::<Value>(&tool_use.input_json).unwrap_or_else(|_| json!({}))
    };
    AnthropicContentBlock::ServerToolUse {
        id: replayable_anthropic_server_tool_use_id(&tool_use.id, &tool_use.name, &input),
        name: tool_use.name.clone(),
        input,
    }
}

/// Builds one replayable Anthropic server-tool identifier with a deterministic local fallback.
/// 构建一条可回放的 Anthropic 服务端工具标识，并提供确定性的本地回退值。
fn replayable_anthropic_server_tool_use_id(
    provider_visible_id: &str,
    tool_name: &str,
    input_json: &Value,
) -> String {
    let provider_visible_id = provider_visible_id.trim();
    if !provider_visible_id.is_empty() {
        return provider_visible_id.to_string();
    }

    format!(
        "server:{}:{}",
        tool_name,
        serde_json::to_string(input_json).unwrap_or_else(|_| input_json.to_string())
    )
}

/// Builds one replayable Anthropic tool-use identifier, preferring the upstream visible id and falling back to a deterministic local surrogate when the provider emitted blanks.
/// 构建一条可回放的 Anthropic tool-use 标识；优先使用上游可见 id，若 Provider 发来空白值则回退到确定性的本地代理标识。
fn replayable_anthropic_tool_use_id(
    provider_visible_id: &str,
    tool_name: &str,
    input_json: &Value,
) -> String {
    let provider_visible_id = provider_visible_id.trim();
    if !provider_visible_id.is_empty() {
        return provider_visible_id.to_string();
    }

    format!(
        "{}:{}",
        tool_name,
        serde_json::to_string(input_json).unwrap_or_else(|_| input_json.to_string())
    )
}

/// Resolves one replay-safe host-visible tool-call identifier from the canonical core tool-call entry.
/// 从规范 core 工具调用条目中解析一条可安全回放的宿主可见工具调用标识。
fn replayable_core_tool_call_id(tool_call: &CoreConversationToolCall) -> String {
    tool_call
        .tool_call_id
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| tool_call.id.clone())
}

/// Repairs replayable Anthropic-native assistant content blocks using the canonical tool-call list so older broken sessions with blank stored tool-use ids can still be resent safely.
/// 使用规范工具调用列表修复可回放的 Anthropic 原生助手内容块，使得历史上已存入空白 tool-use id 的损坏会话仍能被安全重放。
fn repair_replayable_anthropic_provider_content(
    content: Vec<AnthropicContentBlock>,
    tool_calls: &[CoreConversationToolCall],
) -> Vec<AnthropicContentBlock> {
    let mut tool_use_index = 0usize;

    content
        .into_iter()
        .map(|block| match block {
            AnthropicContentBlock::ToolUse { id, name, input } => {
                let repaired_id = if id.trim().is_empty() {
                    tool_calls
                        .get(tool_use_index)
                        .map(replayable_core_tool_call_id)
                        .unwrap_or_else(|| replayable_anthropic_tool_use_id("", &name, &input))
                } else {
                    id.trim().to_string()
                };
                tool_use_index = tool_use_index.saturating_add(1);
                AnthropicContentBlock::ToolUse {
                    id: repaired_id,
                    name,
                    input,
                }
            }
            other => other,
        })
        .collect()
}

/// Converts one canonical core tool call into one Anthropic `tool_use` block.
/// 将一条规范 core 工具调用转换为一条 Anthropic `tool_use` 内容块。
fn core_tool_call_to_anthropic_tool_use(
    tool_call: &CoreConversationToolCall,
    provider_tool_definitions: &[ProviderToolDefinition],
) -> Value {
    let replayable_tool_call_id = replayable_core_tool_call_id(tool_call);
    let provider_visible_tool_name = resolve_provider_visible_anthropic_tool_name(
        &tool_call.tool_name,
        provider_tool_definitions,
    );
    json!({
        "type": "tool_use",
        "id": replayable_tool_call_id,
        "name": provider_visible_tool_name,
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
    provider_tool_definitions: &[ProviderToolDefinition],
) -> Result<CoreConversationToolCall, VulcanCoreError> {
    let input_json = parse_pending_anthropic_tool_use_input_json(&tool_use)?;

    let provider_visible_id = tool_use.id.trim().to_string();
    let provider_visible_id = (!provider_visible_id.is_empty()).then_some(provider_visible_id);
    let local_id = replayable_anthropic_tool_use_id(&tool_use.id, &tool_use.name, &input_json);
    let host_tool_name =
        resolve_host_owned_anthropic_tool_name(&tool_use.name, provider_tool_definitions);

    Ok(CoreConversationToolCall {
        id: local_id,
        tool_call_id: provider_visible_id,
        tool_name: host_tool_name,
        arguments: input_json,
    })
}

/// Resolves one provider-visible Anthropic tool name from the host-owned tool identifier, mainly to keep historical invalid tool ids replayable after the unified dispatch layer started emitting provider-safe names directly.
/// 根据宿主自有工具标识解析一条面向 Anthropic 的 Provider 可见工具名称，主要用于在统一调度层已经直接产出安全工具名后继续兼容历史非法标识的回放。
fn resolve_provider_visible_anthropic_tool_name(
    host_tool_name: &str,
    provider_tool_definitions: &[ProviderToolDefinition],
) -> String {
    provider_tool_definitions
        .iter()
        .find(|definition| definition.name == host_tool_name)
        .map(|definition| definition.name.clone())
        .unwrap_or_else(|| build_anthropic_provider_tool_name(host_tool_name))
}

/// Resolves one host-owned tool identifier from the provider-visible Anthropic tool name, keeping a legacy fallback for replayed sessions that were stored before the unified safe-name registry landed.
/// 根据面向 Anthropic 的 Provider 可见工具名称反向解析一条宿主自有工具标识，并为统一安全名注册表落地前写入的历史回放会话保留旧格式回退。
fn resolve_host_owned_anthropic_tool_name(
    provider_tool_name: &str,
    provider_tool_definitions: &[ProviderToolDefinition],
) -> String {
    provider_tool_definitions
        .iter()
        .find(|definition| definition.name == provider_tool_name)
        .or_else(|| {
            provider_tool_definitions.iter().find(|definition| {
                build_anthropic_provider_tool_name(&definition.name) == provider_tool_name
            })
        })
        .map(|definition| definition.name.clone())
        .unwrap_or_else(|| provider_tool_name.to_string())
}

/// Builds one Anthropic provider-visible tool name only for replay compatibility of historical invalid host identifiers.
/// 仅为历史非法宿主标识的回放兼容构建一条 Anthropic Provider 可见工具名称。
fn build_anthropic_provider_tool_name(host_tool_name: &str) -> String {
    build_direct_public_tool_name(host_tool_name)
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
        AnthropicContentBlock, PendingAnthropicServerToolUse, PendingAnthropicToolUse,
        anthropic_content_block_from_pending_server_tool_use,
        anthropic_content_block_from_pending_tool_use, build_anthropic_provider_message_state,
        build_anthropic_provider_tool_name, build_anthropic_request_payload,
        finalize_pending_anthropic_tool_use,
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

    /// Builds one temporary attachment used by Anthropic multimodal request tests.
    /// 构建一条供 Anthropic 多模态请求测试使用的临时附件。
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
            std::env::temp_dir().join(format!("vulcan-anthropic-{unique_id}.{extension}"));
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

    /// Verifies the direct Anthropic request payload replays legacy tool-turn reasoning blocks while preserving tool-use and tool-result message shapes.
    /// 验证直连 Anthropic 请求载荷会为旧版工具回合回放思考块，同时保留 tool_use 与 tool_result 消息形状。
    #[test]
    fn request_payload_replays_legacy_reasoning_for_tool_turns_and_preserves_anthropic_tool_shapes()
    {
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
        assert_eq!(messages[0]["content"][0]["type"], "thinking");
        assert_eq!(messages[0]["content"][0]["thinking"], "这段思考不应回放。");
        assert_eq!(messages[0]["content"][1]["type"], "text");
        assert_eq!(messages[0]["content"][2]["type"], "tool_use");
        assert_eq!(messages[0]["content"][2]["id"], "call-1");
        assert_eq!(messages[0]["content"][2]["name"], "read_file");
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

    /// Verifies Anthropic user messages encode staged image attachments as base64 image blocks.
    /// 验证 Anthropic 用户消息会把已暂存图片附件编码为 Base64 图片块。
    #[test]
    fn request_payload_embeds_user_image_attachments_as_image_blocks() {
        let attachment = build_test_attachment(
            "png",
            ConversationAttachmentKind::Image,
            "image/png",
            "diagram.png",
        );
        let request = OpenAiCompatibleCoreTurnRequest {
            snapshot: CoreProviderRequestSnapshot {
                provider_id: String::from("anthropic"),
                model_id: String::from("claude-sonnet-4-6"),
                system_prompt: String::from("system prompt"),
                prompt: String::from("分析图片"),
                history: Vec::new(),
                temperature: None,
                max_turns: 8,
                tool_schemas: Vec::new(),
                additional_params: Some(json!({})),
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

        let payload = build_anthropic_request_payload("claude-sonnet-4-6", request, &[])
            .expect("payload should build");
        let messages = payload["messages"]
            .as_array()
            .expect("messages should be an array");
        let content = messages[0]["content"]
            .as_array()
            .expect("user content should be one block array");

        assert_eq!(messages[0]["role"], "user");
        assert_eq!(content[0]["type"], "text");
        assert_eq!(content[1]["type"], "image");
        assert_eq!(content[1]["source"]["type"], "base64");
        assert_eq!(content[1]["source"]["media_type"], "image/png");
        assert_eq!(content[1]["source"]["data"], "AAECAw==");

        let _ = std::fs::remove_file(&attachment.local_path);
    }

    /// Verifies Anthropic user messages encode staged PDF attachments as document blocks and degrade unsupported video attachments into explicit text errors.
    /// 验证 Anthropic 用户消息会把已暂存 PDF 附件编码为文档块，并将不受支持的视频附件降级为显式文本错误。
    #[test]
    fn request_payload_embeds_pdf_and_degrades_video_attachments() {
        let pdf_attachment = build_test_attachment(
            "pdf",
            ConversationAttachmentKind::Pdf,
            "application/pdf",
            "guide.pdf",
        );
        let video_attachment = build_test_attachment(
            "mp4",
            ConversationAttachmentKind::Video,
            "video/mp4",
            "clip.mp4",
        );
        let request = OpenAiCompatibleCoreTurnRequest {
            snapshot: CoreProviderRequestSnapshot {
                provider_id: String::from("anthropic"),
                model_id: String::from("claude-sonnet-4-6"),
                system_prompt: String::from("system prompt"),
                prompt: String::from("分析附件"),
                history: Vec::new(),
                temperature: None,
                max_turns: 8,
                tool_schemas: Vec::new(),
                additional_params: Some(json!({})),
                request_headers: None,
                continuation: CoreProviderContinuation::default(),
            },
            current_history: Vec::new(),
            current_prompt: CoreConversationMessage {
                id: String::from("user-1"),
                payload: CoreConversationPayload::UserText {
                    text: String::from("分析附件"),
                    attachments: vec![pdf_attachment.clone(), video_attachment.clone()],
                },
            },
            interleaved_reasoning: None,
        };

        let payload = build_anthropic_request_payload("claude-sonnet-4-6", request, &[])
            .expect("payload should build");
        let messages = payload["messages"]
            .as_array()
            .expect("messages should be an array");
        let content = messages[0]["content"]
            .as_array()
            .expect("user content should be one block array");

        assert_eq!(content[1]["type"], "document");
        assert_eq!(content[1]["source"]["media_type"], "application/pdf");
        assert_eq!(content[1]["source"]["data"], "AAECAw==");
        assert_eq!(
            content[0]["text"],
            json!(
                "分析附件\n\nERROR: Cannot read \"clip.mp4\" (this API path does not support video input). Inform the user."
            )
        );

        let _ = std::fs::remove_file(&pdf_attachment.local_path);
        let _ = std::fs::remove_file(&video_attachment.local_path);
    }

    /// Verifies a pure attachment-only Anthropic user message remains provider-visible when its managed replay file is gone.
    /// 验证纯附件 Anthropic 用户消息在托管回放文件缺失时仍会以文本提示形式传给 Provider。
    #[test]
    fn request_payload_replaces_missing_attachment_only_message_with_text_notice() {
        let attachment = build_test_attachment(
            "png",
            ConversationAttachmentKind::Image,
            "image/png",
            "lost-diagram.png",
        );
        std::fs::remove_file(&attachment.local_path).expect("managed attachment should remove");
        let request = OpenAiCompatibleCoreTurnRequest {
            snapshot: CoreProviderRequestSnapshot {
                provider_id: String::from("anthropic"),
                model_id: String::from("claude-sonnet-4-6"),
                system_prompt: String::new(),
                prompt: String::new(),
                history: Vec::new(),
                temperature: None,
                max_turns: 8,
                tool_schemas: Vec::new(),
                additional_params: Some(json!({})),
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

        let payload = build_anthropic_request_payload("claude-sonnet-4-6", request, &[])
            .expect("payload should build");
        let messages = payload["messages"]
            .as_array()
            .expect("messages should be an array");
        let content = messages[0]["content"]
            .as_array()
            .expect("user content should be one block array");

        assert_eq!(messages[0]["role"], "user");
        assert_eq!(content[0]["type"], "text");
        assert_eq!(
            content[0]["text"],
            json!("ERROR: Attachment \"lost-diagram.png\" no longer exists. Please ignore it.")
        );
    }

    /// Verifies one stored Anthropic provider-message-state round-trips native thinking signatures and provider-visible tool metadata during future history replay.
    /// 验证一份已存储的 Anthropic provider_message_state 能在后续历史回放中保留原生 thinking 签名与 Provider 可见工具元数据。
    #[test]
    fn request_payload_replays_stored_anthropic_provider_message_state() {
        let assistant_state = build_anthropic_provider_message_state(&[
            AnthropicContentBlock::Thinking {
                thinking: String::from("先检查目录。"),
                signature: Some(String::from("sig-1")),
            },
            AnthropicContentBlock::Text {
                text: String::from("我先读取文件。"),
            },
            AnthropicContentBlock::ToolUse {
                id: String::from("call-1"),
                name: String::from("functions_shell_command"),
                input: json!({ "command": "Get-ChildItem" }),
            },
        ]);
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
                    reasoning: String::from("先检查目录。"),
                    provider_message_id: Some(String::from("resp-1")),
                    provider_message_state: Some(assistant_state),
                    tool_calls: vec![CoreConversationToolCall {
                        id: String::from("tool-1"),
                        tool_call_id: Some(String::from("call-1")),
                        tool_name: String::from("functions.shell_command"),
                        arguments: json!({ "command": "Get-ChildItem" }),
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

        let payload = build_anthropic_request_payload(
            "claude-sonnet-4-6",
            request,
            &[ProviderToolDefinition {
                name: String::from("functions.shell_command"),
                description: String::from("Run one shell command"),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "command": { "type": "string" }
                    },
                    "required": ["command"]
                }),
            }],
        )
        .expect("payload should build");

        let messages = payload["messages"]
            .as_array()
            .expect("messages should be an array");
        assert_eq!(messages[0]["content"][0]["type"], "thinking");
        assert_eq!(messages[0]["content"][0]["thinking"], "先检查目录。");
        assert_eq!(messages[0]["content"][0]["signature"], "sig-1");
        assert_eq!(messages[0]["content"][2]["type"], "tool_use");
        assert_eq!(messages[0]["content"][2]["name"], "functions_shell_command");
    }

    /// Verifies Anthropic replay ignores foreign provider-message-state payloads and rebuilds the assistant tool turn from canonical text/tool-call fields when a session switches from another protocol family.
    /// 验证当会话从其他协议家族切换过来时，Anthropic 回放会忽略外部 provider-message-state，并改为基于规范文本与工具调用字段重建助手工具回合。
    #[test]
    fn request_payload_ignores_foreign_provider_message_state_after_protocol_switch() {
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
                    text: String::from("我先继续调用工具。"),
                    reasoning: String::new(),
                    provider_message_id: Some(String::from("foreign-resp-1")),
                    provider_message_state: Some(json!({
                        "gemini_native": {
                            "role": "model",
                            "parts": [{ "text": "这是一段外部协议状态" }]
                        }
                    })),
                    tool_calls: vec![CoreConversationToolCall {
                        id: String::from("local-call-1"),
                        tool_call_id: Some(String::from("call-1")),
                        tool_name: String::from("spawn_agent"),
                        arguments: json!({ "task": "delegate-1" }),
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

        let payload = build_anthropic_request_payload(
            "claude-sonnet-4-6",
            request,
            &[ProviderToolDefinition {
                name: String::from("spawn_agent"),
                description: String::from("Spawn one delegate agent"),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "task": { "type": "string" }
                    },
                    "required": ["task"]
                }),
            }],
        )
        .expect("payload should build");

        let messages = payload["messages"]
            .as_array()
            .expect("messages should be an array");
        assert_eq!(messages[0]["role"], "assistant");
        assert_eq!(messages[0]["content"][0]["type"], "text");
        assert_eq!(messages[0]["content"][0]["text"], "我先继续调用工具。");
        assert_eq!(messages[0]["content"][1]["type"], "tool_use");
        assert_eq!(messages[0]["content"][1]["id"], "call-1");
        assert_eq!(messages[1]["content"][0]["tool_use_id"], "call-1");
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
                    usage: None,
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
            interleaved_reasoning: None,
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
        assert_eq!(messages[0]["content"][2]["id"], "local-call-1");
        assert_eq!(messages[1]["content"][0]["tool_use_id"], "local-call-1");
    }

    /// Verifies newly stored Anthropic provider-message-state tool-use blocks already carry the replayable local fallback identifier when the upstream provider omitted its visible id.
    /// 验证当上游 Provider 缺失可见 id 时，新写入的 Anthropic provider-message-state 工具块会直接携带可回放的本地回退标识。
    #[test]
    fn anthropic_provider_state_stores_replayable_local_tool_id_when_upstream_id_is_blank() {
        let block = anthropic_content_block_from_pending_tool_use(&PendingAnthropicToolUse {
            id: String::new(),
            name: String::from("spawn_agent"),
            input_json: String::from("{\"task\":\"delegate-1\"}"),
        })
        .expect("tool-use block should build");

        match block {
            AnthropicContentBlock::ToolUse { id, name, input } => {
                assert_eq!(id, "spawn_agent:{\"task\":\"delegate-1\"}");
                assert_eq!(name, "spawn_agent");
                assert_eq!(input, json!({ "task": "delegate-1" }));
            }
            other => panic!("unexpected content block: {other:?}"),
        }
    }

    /// Verifies Anthropic request replay heals older stored provider-message-state blocks whose tool-use ids were blank, so legacy broken sessions can still resend parallel tool results.
    /// 验证 Anthropic 请求回放会修复历史上已存入空白 tool-use id 的 provider-message-state，从而让旧的损坏会话仍能重放并行工具结果。
    #[test]
    fn request_payload_repairs_stored_blank_tool_use_ids_for_parallel_tool_results() {
        let assistant_state = build_anthropic_provider_message_state(&[
            AnthropicContentBlock::ToolUse {
                id: String::new(),
                name: String::from("spawn_agent"),
                input: json!({ "task": "delegate-1" }),
            },
            AnthropicContentBlock::ToolUse {
                id: String::new(),
                name: String::from("spawn_agent"),
                input: json!({ "task": "delegate-2" }),
            },
            AnthropicContentBlock::ToolUse {
                id: String::new(),
                name: String::from("spawn_agent"),
                input: json!({ "task": "delegate-3" }),
            },
            AnthropicContentBlock::ToolUse {
                id: String::new(),
                name: String::from("spawn_agent"),
                input: json!({ "task": "delegate-4" }),
            },
        ]);
        let request = OpenAiCompatibleCoreTurnRequest {
            snapshot: CoreProviderRequestSnapshot {
                provider_id: String::from("anthropic"),
                model_id: String::from("claude-sonnet-4-6"),
                system_prompt: String::from("system prompt"),
                prompt: String::from("tool result body 4"),
                history: Vec::new(),
                temperature: Some(0.2),
                max_turns: 8,
                tool_schemas: Vec::new(),
                additional_params: Some(json!({})),
                request_headers: None,
                continuation: CoreProviderContinuation::default(),
            },
            current_history: vec![
                CoreConversationMessage {
                    id: String::from("assistant-1"),
                    payload: CoreConversationPayload::Assistant {
                        text: String::new(),
                        reasoning: String::new(),
                        provider_message_id: Some(String::from("resp-1")),
                        provider_message_state: Some(assistant_state),
                        tool_calls: vec![
                            CoreConversationToolCall {
                                id: String::from("local-call-1"),
                                tool_call_id: None,
                                tool_name: String::from("spawn_agent"),
                                arguments: json!({ "task": "delegate-1" }),
                            },
                            CoreConversationToolCall {
                                id: String::from("local-call-2"),
                                tool_call_id: None,
                                tool_name: String::from("spawn_agent"),
                                arguments: json!({ "task": "delegate-2" }),
                            },
                            CoreConversationToolCall {
                                id: String::from("local-call-3"),
                                tool_call_id: None,
                                tool_name: String::from("spawn_agent"),
                                arguments: json!({ "task": "delegate-3" }),
                            },
                            CoreConversationToolCall {
                                id: String::from("local-call-4"),
                                tool_call_id: None,
                                tool_name: String::from("spawn_agent"),
                                arguments: json!({ "task": "delegate-4" }),
                            },
                        ],
                        usage: None,
                    },
                },
                CoreConversationMessage {
                    id: String::from("tool-result-1"),
                    payload: CoreConversationPayload::ToolResult {
                        tool_result_id: String::from("result-1"),
                        tool_call_id: Some(String::from("local-call-1")),
                        text: String::from("tool result body 1"),
                    },
                },
                CoreConversationMessage {
                    id: String::from("tool-result-2"),
                    payload: CoreConversationPayload::ToolResult {
                        tool_result_id: String::from("result-2"),
                        tool_call_id: Some(String::from("local-call-2")),
                        text: String::from("tool result body 2"),
                    },
                },
                CoreConversationMessage {
                    id: String::from("tool-result-3"),
                    payload: CoreConversationPayload::ToolResult {
                        tool_result_id: String::from("result-3"),
                        tool_call_id: Some(String::from("local-call-3")),
                        text: String::from("tool result body 3"),
                    },
                },
            ],
            current_prompt: CoreConversationMessage {
                id: String::from("tool-result-4"),
                payload: CoreConversationPayload::ToolResult {
                    tool_result_id: String::from("result-4"),
                    tool_call_id: Some(String::from("local-call-4")),
                    text: String::from("tool result body 4"),
                },
            },
            interleaved_reasoning: None,
        };

        let payload = build_anthropic_request_payload(
            "claude-sonnet-4-6",
            request,
            &[ProviderToolDefinition {
                name: String::from("spawn_agent"),
                description: String::from("Spawn one delegate agent"),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "task": { "type": "string" }
                    },
                    "required": ["task"]
                }),
            }],
        )
        .expect("payload should build");

        let messages = payload["messages"]
            .as_array()
            .expect("messages should be an array");
        assert_eq!(messages[0]["content"][0]["id"], "local-call-1");
        assert_eq!(messages[0]["content"][1]["id"], "local-call-2");
        assert_eq!(messages[0]["content"][2]["id"], "local-call-3");
        assert_eq!(messages[0]["content"][3]["id"], "local-call-4");
        let tool_results = messages[1]["content"]
            .as_array()
            .expect("tool result content should be an array");
        assert_eq!(tool_results[0]["tool_use_id"], "local-call-1");
        assert_eq!(tool_results[1]["tool_use_id"], "local-call-2");
        assert_eq!(tool_results[2]["tool_use_id"], "local-call-3");
        assert_eq!(tool_results[3]["tool_use_id"], "local-call-4");
    }

    /// Verifies Anthropic request payloads coalesce multiple consecutive tool results into one user message so parallel delegate completions replay against the same assistant tool-use turn.
    /// 验证 Anthropic 请求载荷会把多个连续工具结果合并为一条 user 消息，确保并行委托完成结果会回放到同一条助手工具调用回合。
    #[test]
    fn request_payload_coalesces_consecutive_tool_results_into_one_user_message() {
        let request = OpenAiCompatibleCoreTurnRequest {
            snapshot: CoreProviderRequestSnapshot {
                provider_id: String::from("anthropic"),
                model_id: String::from("claude-sonnet-4-6"),
                system_prompt: String::from("system prompt"),
                prompt: String::from("tool result body 4"),
                history: Vec::new(),
                temperature: Some(0.2),
                max_turns: 8,
                tool_schemas: Vec::new(),
                additional_params: Some(json!({})),
                request_headers: None,
                continuation: CoreProviderContinuation::default(),
            },
            current_history: vec![
                CoreConversationMessage {
                    id: String::from("assistant-1"),
                    payload: CoreConversationPayload::Assistant {
                        text: String::from("并行等待四个子代理完成。"),
                        reasoning: String::new(),
                        provider_message_id: Some(String::from("resp-1")),
                        provider_message_state: None,
                        tool_calls: vec![
                            CoreConversationToolCall {
                                id: String::from("local-call-1"),
                                tool_call_id: Some(String::from("call-1")),
                                tool_name: String::from("spawn_agent"),
                                arguments: json!({ "task": "delegate-1" }),
                            },
                            CoreConversationToolCall {
                                id: String::from("local-call-2"),
                                tool_call_id: Some(String::from("call-2")),
                                tool_name: String::from("spawn_agent"),
                                arguments: json!({ "task": "delegate-2" }),
                            },
                            CoreConversationToolCall {
                                id: String::from("local-call-3"),
                                tool_call_id: Some(String::from("call-3")),
                                tool_name: String::from("spawn_agent"),
                                arguments: json!({ "task": "delegate-3" }),
                            },
                            CoreConversationToolCall {
                                id: String::from("local-call-4"),
                                tool_call_id: Some(String::from("call-4")),
                                tool_name: String::from("spawn_agent"),
                                arguments: json!({ "task": "delegate-4" }),
                            },
                        ],
                        usage: None,
                    },
                },
                CoreConversationMessage {
                    id: String::from("tool-result-1"),
                    payload: CoreConversationPayload::ToolResult {
                        tool_result_id: String::from("result-1"),
                        tool_call_id: Some(String::from("call-1")),
                        text: String::from("tool result body 1"),
                    },
                },
                CoreConversationMessage {
                    id: String::from("tool-result-2"),
                    payload: CoreConversationPayload::ToolResult {
                        tool_result_id: String::from("result-2"),
                        tool_call_id: Some(String::from("call-2")),
                        text: String::from("tool result body 2"),
                    },
                },
                CoreConversationMessage {
                    id: String::from("tool-result-3"),
                    payload: CoreConversationPayload::ToolResult {
                        tool_result_id: String::from("result-3"),
                        tool_call_id: Some(String::from("call-3")),
                        text: String::from("tool result body 3"),
                    },
                },
            ],
            current_prompt: CoreConversationMessage {
                id: String::from("tool-result-4"),
                payload: CoreConversationPayload::ToolResult {
                    tool_result_id: String::from("result-4"),
                    tool_call_id: Some(String::from("call-4")),
                    text: String::from("tool result body 4"),
                },
            },
            interleaved_reasoning: None,
        };

        let payload = build_anthropic_request_payload(
            "claude-sonnet-4-6",
            request,
            &[ProviderToolDefinition {
                name: String::from("spawn_agent"),
                description: String::from("Spawn one delegate agent"),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "task": { "type": "string" }
                    },
                    "required": ["task"]
                }),
            }],
        )
        .expect("payload should build");

        let messages = payload["messages"]
            .as_array()
            .expect("messages should be an array");
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0]["role"], "assistant");
        assert_eq!(messages[1]["role"], "user");
        let tool_results = messages[1]["content"]
            .as_array()
            .expect("tool result content should be an array");
        assert_eq!(tool_results.len(), 4);
        assert_eq!(tool_results[0]["type"], "tool_result");
        assert_eq!(tool_results[0]["tool_use_id"], "call-1");
        assert_eq!(tool_results[1]["tool_use_id"], "call-2");
        assert_eq!(tool_results[2]["tool_use_id"], "call-3");
        assert_eq!(tool_results[3]["tool_use_id"], "call-4");
    }

    /// Verifies Anthropic tool-use finalization synthesizes one stable local identifier when the provider omits the upstream id.
    /// 验证当 Provider 缺失上游 id 时，Anthropic 工具使用收尾会合成一个稳定的本地标识。
    #[test]
    fn finalize_pending_anthropic_tool_use_synthesizes_local_identifier() {
        let tool_call = finalize_pending_anthropic_tool_use(
            PendingAnthropicToolUse {
                id: String::new(),
                name: String::from("read_file"),
                input_json: String::from("{\"path\":\"src/main.rs\"}"),
            },
            &[],
        )
        .expect("tool call should finalize");

        assert_eq!(tool_call.id, "read_file:{\"path\":\"src/main.rs\"}");
        assert_eq!(tool_call.tool_call_id, None);
        assert_eq!(tool_call.tool_name, "read_file");
        assert_eq!(tool_call.arguments, json!({ "path": "src/main.rs" }));
    }

    /// Verifies Anthropic request replay still sanitizes one historical invalid host tool identifier while the current provider tool definition already carries the unified safe public name.
    /// 验证当当前 Provider 工具定义已经使用统一安全公开名时，Anthropic 请求回放仍会清洗历史遗留的非法宿主工具标识。
    #[test]
    fn request_payload_sanitizes_invalid_anthropic_tool_names() {
        let host_tool_name = "functions.shell_command";
        let provider_tool_name = build_anthropic_provider_tool_name(host_tool_name);
        assert_eq!(provider_tool_name, "functions_shell_command");
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
                    text: String::from("先执行命令。"),
                    reasoning: String::new(),
                    provider_message_id: Some(String::from("resp-1")),
                    provider_message_state: None,
                    tool_calls: vec![CoreConversationToolCall {
                        id: String::from("tool-1"),
                        tool_call_id: Some(String::from("call-1")),
                        tool_name: String::from(host_tool_name),
                        arguments: json!({ "command": "pwd" }),
                    }],
                    usage: None,
                },
            }],
            current_prompt: CoreConversationMessage {
                id: String::from("tool-result-1"),
                payload: CoreConversationPayload::ToolResult {
                    tool_result_id: String::from("result-1"),
                    tool_call_id: Some(String::from("call-1")),
                    text: String::from("D:/openvulcan/vulcan-code"),
                },
            },
            interleaved_reasoning: None,
        };

        let payload = build_anthropic_request_payload(
            "claude-sonnet-4-6",
            request,
            &[ProviderToolDefinition {
                name: provider_tool_name.clone(),
                description: String::from("Run one shell command"),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "command": { "type": "string" }
                    },
                    "required": ["command"]
                }),
            }],
        )
        .expect("payload should build");

        let messages = payload["messages"]
            .as_array()
            .expect("messages should be an array");
        assert_eq!(messages[0]["content"][1]["name"], provider_tool_name);
        assert_eq!(payload["tools"][0]["name"], provider_tool_name);
    }

    /// Verifies Anthropic request payloads preserve provider-native server tools while appending host function tools.
    /// 验证 Anthropic 请求载荷会保留 Provider 原生服务端工具，同时追加宿主函数工具。
    #[test]
    fn request_payload_preserves_native_web_search_and_appends_function_tools() {
        let request = OpenAiCompatibleCoreTurnRequest {
            snapshot: CoreProviderRequestSnapshot {
                provider_id: String::from("anthropic"),
                model_id: String::from("claude-sonnet-4-6"),
                system_prompt: String::from("system prompt"),
                prompt: String::from("search and read"),
                history: Vec::new(),
                temperature: Some(0.2),
                max_turns: 8,
                tool_schemas: Vec::new(),
                additional_params: Some(json!({
                    "tools": [{
                        "type": "web_search_20250305",
                        "name": "web_search"
                    }]
                })),
                request_headers: None,
                continuation: CoreProviderContinuation::default(),
            },
            current_history: Vec::new(),
            current_prompt: CoreConversationMessage {
                id: String::from("user-1"),
                payload: CoreConversationPayload::UserText {
                    text: String::from("search and read"),
                    attachments: Vec::new(),
                },
            },
            interleaved_reasoning: None,
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

        let tools = payload["tools"]
            .as_array()
            .expect("tools should be an array");
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0]["type"], "web_search_20250305");
        assert_eq!(tools[0]["name"], "web_search");
        assert_eq!(tools[1]["name"], "read_file");
        assert_eq!(tools[1]["description"], "Read one file");
        assert_eq!(payload["tool_choice"]["type"], "auto");
    }

    /// Verifies Anthropic server-side tool-use state stores replayable native content without becoming a host tool call.
    /// 验证 Anthropic 服务端工具使用状态会保存为可回放原生内容，而不会变成宿主工具调用。
    #[test]
    fn anthropic_provider_state_stores_replayable_server_tool_use() {
        let block =
            anthropic_content_block_from_pending_server_tool_use(&PendingAnthropicServerToolUse {
                id: String::new(),
                name: String::from("web_search"),
                input_json: String::from("{\"query\":\"vulcan\"}"),
            });

        match block {
            AnthropicContentBlock::ServerToolUse { id, name, input } => {
                assert_eq!(id, "server:web_search:{\"query\":\"vulcan\"}");
                assert_eq!(name, "web_search");
                assert_eq!(input, json!({ "query": "vulcan" }));
            }
            other => panic!("unexpected content block: {other:?}"),
        }
    }

    /// Verifies Anthropic tool-use finalization restores the original host tool identifier after the provider uses one sanitized tool name.
    /// 验证当 Provider 使用已清洗的工具名称时，Anthropic 工具使用收尾会恢复原始宿主工具标识。
    #[test]
    fn finalize_pending_anthropic_tool_use_restores_host_tool_name() {
        let host_tool_name = "functions.shell_command";
        let tool_call = finalize_pending_anthropic_tool_use(
            PendingAnthropicToolUse {
                id: String::from("call-1"),
                name: build_anthropic_provider_tool_name(host_tool_name),
                input_json: String::from("{\"command\":\"pwd\"}"),
            },
            &[ProviderToolDefinition {
                name: String::from(host_tool_name),
                description: String::from("Run one shell command"),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "command": { "type": "string" }
                    }
                }),
            }],
        )
        .expect("tool call should finalize");

        assert_eq!(tool_call.tool_name, host_tool_name);
        assert_eq!(tool_call.tool_call_id.as_deref(), Some("call-1"));
        assert_eq!(tool_call.arguments, json!({ "command": "pwd" }));
    }
}
