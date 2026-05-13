//! Direct OpenAI-compatible HTTP and SSE transport owned by the host runtime.
//! 宿主运行时自有的 OpenAI 兼容 HTTP 与 SSE 直连传输模块。

use crate::agent_runtime::provider_protocol_anthropic_decoders::sse::iter_sse_messages;
use crate::agent_runtime::provider_protocol_bridge::{
    build_openai_compatible_turn_response, core_provider_turn_delta_to_runtime_delta,
};
use crate::agent_runtime::provider_protocol_content::{
    ProviderAssistantContent, ProviderReasoningPart,
};
use crate::agent_runtime::provider_protocol_openai_compatible::{
    CompatibleAccumulatedToolCall, CompatibleChoiceData, CompatibleFinishReason,
    CompatibleToolCallChunk, append_tool_call_arguments, finalize_completed_streaming_tool_call,
    finalize_pending_tool_call, normalize_first_choice_chunk,
    should_evict_distinct_named_tool_call, tool_call_chunks,
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
use std::collections::HashMap;
use vulcan_core::provider::VulcanProviderDeltaSink;
use vulcan_core::types::{
    ConversationMessage as CoreConversationMessage, ConversationPayload as CoreConversationPayload,
    ConversationToolCall as CoreConversationToolCall, CoreError as VulcanCoreError,
    ProviderTurnDelta as CoreProviderTurnDelta, ProviderTurnResponse as CoreProviderTurnResponse,
};

/// Minimal OpenAI-compatible streaming function payload decoded from one delta chunk.
/// 从单条增量块解码出的最小 OpenAI 兼容流式函数载荷。
#[derive(Debug, Clone, Deserialize)]
struct OpenAiCompatibleStreamingFunction {
    /// Optional streamed function name fragment.
    /// 可选的流式函数名称碎片。
    name: Option<String>,
    /// Optional streamed function-arguments fragment.
    /// 可选的流式函数参数碎片。
    arguments: Option<String>,
}

/// Minimal OpenAI-compatible streaming tool-call payload decoded from one delta chunk.
/// 从单条增量块解码出的最小 OpenAI 兼容流式工具调用载荷。
#[derive(Debug, Clone, Deserialize)]
struct OpenAiCompatibleStreamingToolCall {
    /// Stable positional index used to merge multiple chunks for the same logical tool call.
    /// 用于合并同一逻辑工具调用多段碎片的稳定位置索引。
    index: usize,
    /// Optional upstream tool-call identifier.
    /// 可选的上游工具调用标识。
    id: Option<String>,
    /// Streamed function payload.
    /// 流式函数载荷。
    function: OpenAiCompatibleStreamingFunction,
}

impl From<&OpenAiCompatibleStreamingToolCall> for CompatibleToolCallChunk {
    /// Converts one streamed OpenAI-compatible tool-call fragment into the shared compatibility chunk shape.
    /// 将单条流式 OpenAI 兼容工具调用碎片转换为共享兼容层工具调用碎片形状。
    fn from(value: &OpenAiCompatibleStreamingToolCall) -> Self {
        Self {
            index: value.index,
            id: value.id.clone(),
            name: value.function.name.clone(),
            arguments: value.function.arguments.clone(),
        }
    }
}

/// Minimal OpenAI-compatible delta payload decoded from one streamed choice.
/// 从单条流式 choice 解码出的最小 OpenAI 兼容增量载荷。
#[derive(Debug, Clone, Deserialize)]
struct OpenAiCompatibleStreamingDelta {
    /// Optional assistant text delta.
    /// 可选的助手文本增量。
    #[serde(default)]
    content: Option<String>,
    /// Optional hidden reasoning delta carried by some OpenAI-compatible providers.
    /// 某些 OpenAI 兼容 Provider 携带的可选隐藏思考增量。
    #[serde(default)]
    reasoning_content: Option<String>,
    /// Streamed tool-call fragments.
    /// 流式工具调用碎片集合。
    #[serde(default)]
    tool_calls: Vec<OpenAiCompatibleStreamingToolCall>,
}

/// Minimal OpenAI-compatible finish reason used by streamed choices.
/// 流式 choice 使用的最小 OpenAI 兼容结束原因。
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum OpenAiCompatibleFinishReason {
    /// Tool calls have completed and should be flushed immediately.
    /// 工具调用已经结束并应立即冲刷。
    ToolCalls,
    /// Assistant output stopped normally.
    /// 助手输出正常结束。
    Stop,
    /// Provider filtered the content.
    /// Provider 过滤了内容。
    ContentFilter,
    /// Provider reached the length limit.
    /// Provider 达到了长度限制。
    Length,
    /// Any provider-specific finish reason not covered above.
    /// 上述枚举之外的任意 Provider 专用结束原因。
    #[serde(untagged)]
    Other(String),
}

/// Minimal OpenAI-compatible streamed choice payload.
/// 最小 OpenAI 兼容流式 choice 载荷。
#[derive(Debug, Clone, Deserialize)]
struct OpenAiCompatibleStreamingChoice {
    /// Streamed delta payload.
    /// 流式增量载荷。
    delta: OpenAiCompatibleStreamingDelta,
    /// Optional finish reason emitted by the provider.
    /// Provider 发出的可选结束原因。
    finish_reason: Option<OpenAiCompatibleFinishReason>,
}

/// Minimal OpenAI-compatible streaming chunk decoded from one SSE `data:` payload.
/// 从单条 SSE `data:` 载荷解码出的最小 OpenAI 兼容流式块。
#[derive(Debug, Clone, Deserialize)]
struct OpenAiCompatibleStreamingChunk {
    /// Optional upstream response identifier.
    /// 可选的上游响应标识。
    id: Option<String>,
    /// Streamed choice list.
    /// 流式 choice 列表。
    #[serde(default)]
    choices: Vec<OpenAiCompatibleStreamingChoice>,
}

/// Executes one direct OpenAI-compatible streaming turn without going through any third-party completion-model abstraction.
/// 在不经过任何第三方 completion model 抽象的前提下执行一次直连 OpenAI 兼容流式回合。
pub(crate) async fn execute_openai_compatible_http_streaming_turn<S>(
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
        build_openai_compatible_http_client(provider_id, provider_label, api_key, request_headers)?;
    let endpoint = build_openai_compatible_chat_completions_endpoint(base_url);
    let payload =
        build_openai_compatible_request_payload(model_id, request, provider_tool_definitions)?;
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
    let mut assistant_text = String::new();
    let mut assistant_reasoning = String::new();
    let mut pending_tool_calls: HashMap<usize, CompatibleAccumulatedToolCall> = HashMap::new();
    let mut finalized_tool_calls = Vec::new();

    while let Some(event_result) = events.next().await {
        let event = event_result.map_err(|error| VulcanCoreError::ProviderFailure {
            provider_id: provider_id.to_string(),
            message: format!("{provider_label} SSE 解码失败：{error}"),
        })?;

        if event.data.trim().is_empty() || event.data.trim() == "[DONE]" {
            continue;
        }

        let chunk = normalize_openai_compatible_chunk(provider_id, provider_label, &event.data)?;
        response_id = chunk.response_id.or(response_id);
        let Some(choice) = chunk.choice else {
            continue;
        };

        process_compatible_tool_call_chunks(
            &mut pending_tool_calls,
            &mut finalized_tool_calls,
            choice.tool_calls,
        );

        if let Some(reasoning) = choice.reasoning
            && !reasoning.is_empty()
        {
            assistant_reasoning.push_str(&reasoning);
            forward_openai_compatible_delta(
                live_sender,
                sink,
                CoreProviderTurnDelta::Thinking { delta: reasoning },
            );
        }

        if let Some(text) = choice.text
            && !text.is_empty()
        {
            assistant_text.push_str(&text);
            forward_openai_compatible_delta(
                live_sender,
                sink,
                CoreProviderTurnDelta::Output { delta: text },
            );
        }

        if choice.finish_reason == CompatibleFinishReason::ToolCalls {
            flush_pending_compatible_tool_calls(&mut pending_tool_calls, &mut finalized_tool_calls);
        }
    }

    flush_pending_compatible_tool_calls(&mut pending_tool_calls, &mut finalized_tool_calls);

    let mut assistant_content = Vec::new();
    if !assistant_text.is_empty() {
        assistant_content.push(ProviderAssistantContent::Text(assistant_text));
    }
    if !assistant_reasoning.is_empty() {
        assistant_content.push(ProviderAssistantContent::Reasoning(vec![
            ProviderReasoningPart::Text(assistant_reasoning),
        ]));
    }
    assistant_content.extend(
        finalized_tool_calls
            .into_iter()
            .map(ProviderAssistantContent::ToolCall),
    );

    Ok(build_openai_compatible_turn_response(
        provider_id,
        model_id,
        response_id,
        &assistant_content,
    ))
}

/// Builds one direct OpenAI-compatible HTTP client with merged authorization and caller-supplied request headers.
/// 构建一个直连 OpenAI 兼容 HTTP 客户端，并合并鉴权头与调用方提供的请求头。
fn build_openai_compatible_http_client(
    provider_id: &str,
    provider_label: &str,
    api_key: &str,
    mut request_headers: HeaderMap,
) -> Result<reqwest::Client, VulcanCoreError> {
    ensure_bearer_authorization_header(provider_id, provider_label, api_key, &mut request_headers)?;
    ensure_json_content_type(&mut request_headers);
    build_json_http_client(provider_id, provider_label, request_headers)
}

/// Builds the final OpenAI-compatible `/chat/completions` endpoint from one optional base URL override.
/// 根据可选基础地址覆盖值构建最终 OpenAI 兼容 `/chat/completions` 端点。
fn build_openai_compatible_chat_completions_endpoint(base_url: Option<&str>) -> String {
    build_rest_endpoint(base_url, "https://api.openai.com/v1", "chat/completions")
}

/// Builds the final OpenAI-compatible JSON payload from one host-controlled turn request and tool schema set.
/// 根据一份宿主控制回合请求与工具 schema 集合构建最终 OpenAI 兼容 JSON 载荷。
fn build_openai_compatible_request_payload(
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

    let mut messages = Vec::new();
    if !snapshot.system_prompt.trim().is_empty() {
        messages.push(json!({
            "role": "system",
            "content": snapshot.system_prompt
        }));
    }
    messages.extend(
        current_history
            .iter()
            .map(core_message_to_openai_compatible_message)
            .collect::<Result<Vec<_>, _>>()?,
    );
    messages.push(core_message_to_openai_compatible_message(&current_prompt)?);

    payload.insert(String::from("model"), Value::String(model_id.to_string()));
    payload.insert(String::from("messages"), Value::Array(messages));
    payload.insert(String::from("stream"), Value::Bool(true));

    if let Some(temperature) = snapshot.temperature {
        payload.insert(String::from("temperature"), json!(temperature));
    }

    if !provider_tool_definitions.is_empty() {
        payload.insert(
            String::from("tools"),
            Value::Array(
                provider_tool_definitions
                    .iter()
                    .map(provider_tool_definition_to_openai_compatible_tool)
                    .collect(),
            ),
        );
    }

    Ok(Value::Object(payload))
}

/// Converts one canonical core message into one OpenAI-compatible request message while intentionally dropping local-only reasoning replay.
/// 将一条规范 core 消息转换为一条 OpenAI 兼容请求消息，同时刻意丢弃仅限本地使用的思考回放。
fn core_message_to_openai_compatible_message(
    message: &CoreConversationMessage,
) -> Result<Value, VulcanCoreError> {
    match &message.payload {
        CoreConversationPayload::UserText { text } => Ok(json!({
            "role": "user",
            "content": text
        })),
        CoreConversationPayload::Assistant {
            text, tool_calls, ..
        } => {
            let mut object = JsonMap::new();
            object.insert(
                String::from("role"),
                Value::String(String::from("assistant")),
            );
            if !text.is_empty() {
                object.insert(String::from("content"), Value::String(text.clone()));
            }
            if !tool_calls.is_empty() {
                object.insert(
                    String::from("tool_calls"),
                    Value::Array(
                        tool_calls
                            .iter()
                            .map(core_tool_call_to_openai_compatible_tool_call)
                            .collect(),
                    ),
                );
            }
            Ok(Value::Object(object))
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
                        "OpenAI 兼容工具结果消息缺少 tool_call_id，无法构造 provider 请求。",
                    ),
                });
            };
            Ok(json!({
                "role": "tool",
                "tool_call_id": tool_call_id,
                "content": text
            }))
        }
    }
}

/// Converts one canonical core tool call into one OpenAI-compatible tool-call object with stringified JSON arguments.
/// 将一条规范 core 工具调用转换为一条 OpenAI 兼容工具调用对象，并把 JSON 参数字符串化。
fn core_tool_call_to_openai_compatible_tool_call(tool_call: &CoreConversationToolCall) -> Value {
    json!({
        "id": tool_call.id,
        "type": "function",
        "function": {
            "name": tool_call.tool_name,
            "arguments": stringify_openai_compatible_tool_arguments(&tool_call.arguments)
        }
    })
}

/// Converts one host-owned provider tool definition into one OpenAI-compatible tool schema object.
/// 将一条宿主自有 Provider 工具定义转换为一条 OpenAI 兼容工具 schema 对象。
fn provider_tool_definition_to_openai_compatible_tool(
    definition: &ProviderToolDefinition,
) -> Value {
    json!({
        "type": "function",
        "function": {
            "name": definition.name,
            "description": definition.description,
            "parameters": definition.parameters
        }
    })
}

/// Serializes one tool-arguments JSON value into the OpenAI-compatible string form expected inside assistant tool calls.
/// 将单个工具参数 JSON 值序列化为助手工具调用中期望的 OpenAI 兼容字符串形式。
fn stringify_openai_compatible_tool_arguments(arguments: &Value) -> String {
    match arguments {
        Value::String(value) => value.clone(),
        other => serde_json::to_string(other).unwrap_or_else(|_| other.to_string()),
    }
}

/// Normalizes one OpenAI-compatible SSE `data:` payload into the shared compatibility chunk shape.
/// 将单条 OpenAI 兼容 SSE `data:` 载荷规范化为共享兼容块形状。
fn normalize_openai_compatible_chunk(
    provider_id: &str,
    provider_label: &str,
    data: &str,
) -> Result<
    crate::agent_runtime::provider_protocol_openai_compatible::CompatibleChunk,
    VulcanCoreError,
> {
    let chunk = serde_json::from_str::<OpenAiCompatibleStreamingChunk>(data).map_err(|error| {
        VulcanCoreError::ProviderFailure {
            provider_id: provider_id.to_string(),
            message: format!("{provider_label} 流式数据解析失败：{error}"),
        }
    })?;

    Ok(normalize_first_choice_chunk(
        chunk.id,
        &chunk.choices,
        |choice| CompatibleChoiceData {
            finish_reason: if choice.finish_reason == Some(OpenAiCompatibleFinishReason::ToolCalls)
            {
                CompatibleFinishReason::ToolCalls
            } else {
                CompatibleFinishReason::Other
            },
            text: choice.delta.content.clone(),
            reasoning: choice.delta.reasoning_content.clone(),
            tool_calls: tool_call_chunks(&choice.delta.tool_calls),
        },
    ))
}

/// Processes one batch of OpenAI-compatible tool-call chunks, merging fragments and emitting finalized calls when boundaries are reached.
/// 处理一批 OpenAI 兼容工具调用碎片，在边界到达时合并片段并产出已完成调用。
fn process_compatible_tool_call_chunks(
    pending_tool_calls: &mut HashMap<usize, CompatibleAccumulatedToolCall>,
    finalized_tool_calls: &mut Vec<CoreConversationToolCall>,
    chunks: Vec<CompatibleToolCallChunk>,
) {
    for chunk in chunks {
        if let Some(existing) = pending_tool_calls.get(&chunk.index)
            && should_evict_distinct_named_tool_call(existing, &chunk)
            && let Some(evicted) = pending_tool_calls.remove(&chunk.index)
            && let Some(finalized) = finalize_pending_tool_call(evicted)
        {
            finalized_tool_calls.push(compatible_tool_call_to_core_tool_call(finalized));
        }

        let pending = pending_tool_calls
            .entry(chunk.index)
            .or_insert_with(CompatibleAccumulatedToolCall::empty);

        if let Some(id) = chunk.id.as_ref()
            && !id.is_empty()
        {
            pending.id = id.clone();
        }

        if let Some(name) = chunk.name.as_ref()
            && !name.is_empty()
        {
            pending.name = name.clone();
        }

        if let Some(arguments) = chunk.arguments.as_ref()
            && !arguments.is_empty()
        {
            append_tool_call_arguments(pending, arguments);
        }
    }
}

/// Flushes every pending OpenAI-compatible tool call into finalized canonical tool-call entries.
/// 将所有待定 OpenAI 兼容工具调用冲刷为已完成的规范工具调用项。
fn flush_pending_compatible_tool_calls(
    pending_tool_calls: &mut HashMap<usize, CompatibleAccumulatedToolCall>,
    finalized_tool_calls: &mut Vec<CoreConversationToolCall>,
) {
    let mut indices = pending_tool_calls.keys().copied().collect::<Vec<_>>();
    indices.sort_unstable();

    for index in indices {
        let Some(pending) = pending_tool_calls.remove(&index) else {
            continue;
        };
        let finalized = finalize_pending_tool_call(finalize_completed_streaming_tool_call(pending));
        if let Some(tool_call) = finalized {
            finalized_tool_calls.push(compatible_tool_call_to_core_tool_call(tool_call));
        }
    }
}

/// Converts one accumulated compatible tool call into the canonical `vulcan-core` tool-call shape.
/// 将一条累计完成的兼容工具调用转换为规范 `vulcan-core` 工具调用形状。
fn compatible_tool_call_to_core_tool_call(
    tool_call: CompatibleAccumulatedToolCall,
) -> CoreConversationToolCall {
    let tool_call_id = (!tool_call.id.is_empty()).then_some(tool_call.id.clone());
    CoreConversationToolCall {
        id: tool_call.id,
        tool_call_id,
        tool_name: tool_call.name,
        arguments: tool_call.arguments,
    }
}

/// Forwards one normalized provider delta into both the controller sink and the GUI live sender.
/// 将一条规范 Provider 增量同时转发给控制器汇聚器与 GUI 实时发送器。
fn forward_openai_compatible_delta<S>(
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
    use super::build_openai_compatible_request_payload;
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

    /// Verifies the direct OpenAI-compatible request payload strips reasoning replay while preserving assistant tool calls and tool results.
    /// 验证直连 OpenAI 兼容请求载荷会剥离思考回放，同时保留助手工具调用与工具结果。
    #[test]
    fn request_payload_strips_reasoning_replay_and_preserves_tool_shapes() {
        let request = OpenAiCompatibleCoreTurnRequest {
            snapshot: CoreProviderRequestSnapshot {
                provider_id: String::from("openai"),
                model_id: String::from("gpt-test"),
                system_prompt: String::from("system prompt"),
                prompt: String::from("tool result body"),
                history: Vec::new(),
                temperature: Some(0.2),
                max_turns: 8,
                tool_schemas: Vec::new(),
                additional_params: Some(json!({
                    "reasoning": { "effort": "high" }
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

        let payload = build_openai_compatible_request_payload(
            "gpt-test",
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
        assert_eq!(messages[0]["role"], "system");
        assert_eq!(messages[1]["role"], "assistant");
        assert!(messages[1].get("reasoning_content").is_none());
        assert_eq!(messages[1]["tool_calls"][0]["id"], "tool-1");
        assert_eq!(
            messages[1]["tool_calls"][0]["function"]["name"],
            "read_file"
        );
        assert_eq!(
            messages[1]["tool_calls"][0]["function"]["arguments"],
            "{\"path\":\"src/main.rs\"}"
        );
        assert_eq!(messages[2]["role"], "tool");
        assert_eq!(messages[2]["tool_call_id"], "call-1");
        assert_eq!(messages[2]["content"], "tool result body");
        assert_eq!(payload["stream"], true);
        assert_eq!(payload["reasoning"]["effort"], "high");
        assert_eq!(payload["tools"][0]["function"]["name"], "read_file");
    }
}
