//! Direct Cohere HTTP and SSE transport owned by the host runtime.
//! 宿主运行时自有的 Cohere HTTP 与 SSE 直连传输模块。

use crate::agent_runtime::provider_prompt_layers::provider_fallback_system_prompt;
use crate::agent_runtime::provider_protocol_anthropic_decoders::sse::iter_sse_messages;
use crate::agent_runtime::provider_protocol_bridge::{
    build_openai_compatible_turn_response, core_provider_turn_delta_to_runtime_delta,
};
use crate::agent_runtime::provider_protocol_cohere::{
    CohereAssistantContent, CohereChatMessage, CohereFunctionDefinition,
    CohereMessageEndDeltaEnvelope, CohereStreamingContentDelta, CohereStreamingEvent,
    CohereStreamingToolCallDelta, CohereStreamingToolCallFunctionDelta, CohereToolCall,
    CohereToolCallFunction, CohereToolCallKind, CohereToolDefinition, CohereToolResultContent,
    CohereUsage, CohereUserContent,
};
use crate::agent_runtime::provider_protocol_content::{
    ProviderAssistantContent, ProviderReasoningPart,
};
use crate::agent_runtime::provider_protocol_tool_definition::ProviderToolDefinition;
use crate::agent_runtime::provider_transport_http_support::{
    build_json_http_client, build_rest_endpoint, ensure_bearer_authorization_header,
    ensure_json_content_type, format_standard_error_response_with_headers,
};
use crate::agent_runtime::provider_transport_live_delta::ProviderLiveDeltaSender;
use crate::agent_runtime::provider_transport_turn_request::OpenAiCompatibleCoreTurnRequest;
use crate::agent_runtime::provider_transport_wire_capture::{
    emit_provider_wire_request, provider_request_dry_run_active, provider_request_dry_run_response,
};
use crate::provider_config::ProviderApiFamily;
use crate::provider_input_normalizer::normalize_provider_user_input;
use futures_util::StreamExt;
use reqwest::header::HeaderMap;
use serde_json::{Map as JsonMap, Value, json};
use vulcan_core::provider::VulcanProviderDeltaSink;
use vulcan_core::types::{
    ConversationMessage as CoreConversationMessage, ConversationPayload as CoreConversationPayload,
    ConversationToolCall as CoreConversationToolCall, CoreError as VulcanCoreError,
    ProviderTurnDelta as CoreProviderTurnDelta, ProviderTurnResponse as CoreProviderTurnResponse,
    ProviderUsageSnapshot as CoreProviderUsageSnapshot,
};

/// Default Cohere REST base URL used by the host-owned direct transport.
/// 宿主自有直连传输使用的默认 Cohere REST 基础地址。
const COHERE_DEFAULT_BASE_URL: &str = "https://api.cohere.com";

/// Current active Cohere content block kind carried by the streaming state machine.
/// 由流式状态机维护的当前活动 Cohere 内容块类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActiveCohereContentKind {
    /// Visible assistant output text.
    /// 可见助手输出文本。
    Text,
    /// Hidden or local-only reasoning text.
    /// 隐藏或仅本地使用的思考文本。
    Thinking,
    /// Any unsupported content kind.
    /// 任意当前不支持的内容类型。
    Other,
}

/// Pending Cohere tool call assembled across multiple streaming events.
/// 跨多条流式事件累计组装的待定 Cohere 工具调用。
#[derive(Debug, Default)]
struct PendingCohereToolCall {
    /// Provider-native tool call identifier.
    /// Provider 原生工具调用标识。
    id: String,
    /// Stable function name.
    /// 稳定函数名称。
    name: String,
    /// Accumulated JSON-stringified arguments payload.
    /// 累积的 JSON 字符串化参数载荷。
    arguments_json: String,
}

/// Executes one direct Cohere streaming turn without going through any compatibility transport layer.
/// 在不经过任何兼容传输层的前提下执行一次直连 Cohere 流式回合。
pub(crate) async fn execute_cohere_http_streaming_turn<S>(
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
    ensure_bearer_authorization_header(provider_id, "Cohere", api_key, &mut wire_headers)?;
    ensure_json_content_type(&mut wire_headers);
    let client = build_cohere_http_client(provider_id, api_key, request_headers)?;
    let endpoint = build_cohere_chat_endpoint(base_url);
    let payload = build_cohere_request_payload(model_id, request, provider_tool_definitions)?;
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
            message: format!("Cohere 请求发送失败：{error}"),
        })?;

    if !response.status().is_success() {
        let status = response.status();
        let headers = response.headers().clone();
        let body = response.text().await.unwrap_or_default();
        let message = format_standard_error_response_with_headers(status, &headers, &body);
        return Err(VulcanCoreError::ProviderFailure {
            provider_id: provider_id.to_string(),
            message: format!("Cohere request failed: {message}"),
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

    let mut response_id = None::<String>;
    let mut assistant_text = String::new();
    let mut assistant_reasoning = Vec::<ProviderReasoningPart>::new();
    let mut active_content_kind = None::<ActiveCohereContentKind>;
    let mut active_thinking_text = String::new();
    let mut tool_plan_text = String::new();
    let mut pending_tool_call = None::<PendingCohereToolCall>;
    let mut tool_calls = Vec::<CoreConversationToolCall>::new();
    let mut final_usage = None::<CoreProviderUsageSnapshot>;

    while let Some(event_result) = events.next().await {
        let event = event_result.map_err(|error| VulcanCoreError::ProviderFailure {
            provider_id: provider_id.to_string(),
            message: format!("Cohere SSE 解码失败：{error}"),
        })?;

        if event.data.trim().is_empty() {
            continue;
        }

        let streaming_event =
            serde_json::from_str::<CohereStreamingEvent>(&event.data).map_err(|error| {
                VulcanCoreError::ProviderFailure {
                    provider_id: provider_id.to_string(),
                    message: format!("Cohere 流式事件解析失败：{error}"),
                }
            })?;

        match streaming_event {
            CohereStreamingEvent::MessageStart { id, .. } => {
                response_id = id.filter(|value| !value.trim().is_empty());
            }
            CohereStreamingEvent::ContentStart { delta, .. } => {
                let content = delta
                    .and_then(|payload| payload.message)
                    .and_then(|message| message.content);
                handle_cohere_content_start(
                    live_sender,
                    sink,
                    content,
                    &mut active_content_kind,
                    &mut active_thinking_text,
                    &mut assistant_text,
                );
            }
            CohereStreamingEvent::ContentDelta { delta, .. } => {
                let content = delta
                    .and_then(|payload| payload.message)
                    .and_then(|message| message.content);
                handle_cohere_content_delta(
                    live_sender,
                    sink,
                    content,
                    active_content_kind,
                    &mut active_thinking_text,
                    &mut assistant_text,
                );
            }
            CohereStreamingEvent::ContentEnd { .. } => {
                if matches!(active_content_kind, Some(ActiveCohereContentKind::Thinking))
                    && !active_thinking_text.is_empty()
                {
                    assistant_reasoning.push(ProviderReasoningPart::Text(std::mem::take(
                        &mut active_thinking_text,
                    )));
                }
                active_content_kind = None;
            }
            CohereStreamingEvent::ToolPlanDelta { delta } => {
                if let Some(tool_plan_delta) = delta
                    .and_then(|payload| payload.message)
                    .and_then(|message| message.tool_plan)
                    .filter(|value| !value.is_empty())
                {
                    tool_plan_text.push_str(&tool_plan_delta);
                    forward_cohere_delta(
                        live_sender,
                        sink,
                        CoreProviderTurnDelta::Thinking {
                            delta: tool_plan_delta,
                        },
                    );
                }
            }
            CohereStreamingEvent::ToolCallStart { delta, .. } => {
                if let Some(tool_call_delta) = delta
                    .and_then(|payload| payload.message)
                    .and_then(|message| message.tool_calls)
                {
                    pending_tool_call = Some(start_pending_cohere_tool_call(tool_call_delta));
                }
            }
            CohereStreamingEvent::ToolCallDelta { delta, .. } => {
                if let (Some(pending), Some(arguments_delta)) = (
                    pending_tool_call.as_mut(),
                    delta
                        .and_then(|payload| payload.message)
                        .and_then(|message| message.tool_calls)
                        .and_then(extract_cohere_tool_call_arguments_delta),
                ) {
                    pending.arguments_json.push_str(&arguments_delta);
                }
            }
            CohereStreamingEvent::ToolCallEnd { .. } => {
                if let Some(pending) = pending_tool_call.take() {
                    tool_calls.push(finalize_pending_cohere_tool_call(provider_id, pending)?);
                }
            }
            CohereStreamingEvent::MessageEnd { id, delta } => {
                if response_id.is_none() {
                    response_id = id.filter(|value| !value.trim().is_empty());
                }
                if let Some(CohereMessageEndDeltaEnvelope { usage, .. }) = delta {
                    final_usage = usage.and_then(cohere_usage_to_snapshot);
                }
                break;
            }
            CohereStreamingEvent::CitationStart { .. }
            | CohereStreamingEvent::CitationEnd { .. }
            | CohereStreamingEvent::Debug
            | CohereStreamingEvent::Unknown => {}
        }
    }

    if let Some(pending) = pending_tool_call.take() {
        tool_calls.push(finalize_pending_cohere_tool_call(provider_id, pending)?);
    }
    if matches!(active_content_kind, Some(ActiveCohereContentKind::Thinking))
        && !active_thinking_text.is_empty()
    {
        assistant_reasoning.push(ProviderReasoningPart::Text(active_thinking_text));
    }
    if !tool_plan_text.is_empty() {
        assistant_reasoning.push(ProviderReasoningPart::Summary(tool_plan_text));
    }

    let mut assistant_content = Vec::new();
    if !assistant_text.is_empty() {
        assistant_content.push(ProviderAssistantContent::Text(assistant_text));
    }
    if !assistant_reasoning.is_empty() {
        assistant_content.push(ProviderAssistantContent::Reasoning(assistant_reasoning));
    }
    assistant_content.extend(
        tool_calls
            .into_iter()
            .map(ProviderAssistantContent::ToolCall),
    );

    Ok(build_openai_compatible_turn_response(
        provider_id,
        model_id,
        response_id,
        &assistant_content,
        final_usage,
    ))
}

/// Converts one raw Cohere usage payload into the shared normalized usage snapshot.
/// 将一份原始 Cohere 用量载荷转换为共享规范化用量快照。
fn cohere_usage_to_snapshot(usage: CohereUsage) -> Option<CoreProviderUsageSnapshot> {
    let input_tokens = usage
        .tokens
        .as_ref()
        .and_then(|tokens| tokens.input_tokens)
        .or_else(|| {
            usage
                .billed_units
                .as_ref()
                .and_then(|tokens| tokens.input_tokens)
        })
        .map(|value| value.max(0.0) as usize);
    let output_tokens = usage
        .tokens
        .as_ref()
        .and_then(|tokens| tokens.output_tokens)
        .or_else(|| {
            usage
                .billed_units
                .as_ref()
                .and_then(|tokens| tokens.output_tokens)
        })
        .map(|value| value.max(0.0) as usize);
    let snapshot = CoreProviderUsageSnapshot {
        input_tokens,
        output_tokens,
        reasoning_tokens: None,
        total_tokens: match (input_tokens, output_tokens) {
            (Some(input_tokens), Some(output_tokens)) => {
                Some(input_tokens.saturating_add(output_tokens))
            }
            _ => None,
        },
        cached_input_tokens: None,
        cache_creation_input_tokens: None,
    };
    snapshot.has_any_tokens().then_some(snapshot)
}

/// Builds one direct Cohere HTTP client with merged bearer authorization and caller-supplied request headers.
/// 构建一个直连 Cohere HTTP 客户端，并合并 Bearer 鉴权与调用方提供的请求头。
fn build_cohere_http_client(
    provider_id: &str,
    api_key: &str,
    mut request_headers: HeaderMap,
) -> Result<reqwest::Client, VulcanCoreError> {
    ensure_bearer_authorization_header(provider_id, "Cohere", api_key, &mut request_headers)?;
    ensure_json_content_type(&mut request_headers);
    build_json_http_client(provider_id, "Cohere", request_headers)
}

/// Builds the Cohere native chat endpoint from one optional base URL override.
/// 根据可选基础地址覆盖值构建 Cohere 原生聊天端点。
fn build_cohere_chat_endpoint(base_url: Option<&str>) -> String {
    build_rest_endpoint(base_url, COHERE_DEFAULT_BASE_URL, "v2/chat")
}

/// Builds one Cohere native request payload from the canonical host-owned turn request and mirrored tool definitions.
/// 根据规范宿主单回合请求与镜像工具定义构建一份 Cohere 原生请求载荷。
fn build_cohere_request_payload(
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

    payload.insert(String::from("model"), Value::String(model_id.to_string()));
    payload.insert(String::from("stream"), Value::Bool(true));

    if let Some(temperature) = snapshot.temperature {
        payload.insert(String::from("temperature"), json!(temperature));
    }

    let mut messages = Vec::new();
    if !system_prompt.trim().is_empty() {
        messages.push(
            serde_json::to_value(CohereChatMessage::System {
                content: system_prompt,
            })
            .map_err(|error| VulcanCoreError::InvalidState {
                message: format!("Cohere 系统消息序列化失败：{error}"),
            })?,
        );
    }

    messages.extend(
        current_history
            .iter()
            .map(core_message_to_cohere_message)
            .chain(std::iter::once(core_message_to_cohere_message(
                &current_prompt,
            )))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(|message| {
                serde_json::to_value(message).map_err(|error| VulcanCoreError::InvalidState {
                    message: format!("Cohere 请求消息序列化失败：{error}"),
                })
            })
            .collect::<Result<Vec<_>, _>>()?,
    );

    payload.insert(String::from("messages"), Value::Array(messages));

    if !provider_tool_definitions.is_empty() {
        payload.insert(
            String::from("tools"),
            Value::Array(
                provider_tool_definitions
                    .iter()
                    .map(provider_tool_definition_to_cohere_tool_definition)
                    .map(|tool| serde_json::to_value(tool).unwrap_or_else(|_| Value::Null))
                    .collect(),
            ),
        );
    }

    Ok(Value::Object(payload))
}

/// Converts one canonical core message into one Cohere native request message while intentionally dropping local-only reasoning replay.
/// 将一条规范 core 消息转换为一条 Cohere 原生请求消息，同时刻意丢弃仅限本地使用的思考回放。
fn core_message_to_cohere_message(
    message: &CoreConversationMessage,
) -> Result<CohereChatMessage, VulcanCoreError> {
    match &message.payload {
        CoreConversationPayload::UserText { text, attachments } => {
            let normalized =
                normalize_provider_user_input(ProviderApiFamily::CohereChat, text, attachments);

            Ok(CohereChatMessage::User {
                content: vec![CohereUserContent::Text {
                    text: normalized.text,
                }],
            })
        }
        CoreConversationPayload::Assistant {
            text, tool_calls, ..
        } => {
            let mut content = Vec::new();
            if !text.trim().is_empty() {
                content.push(CohereAssistantContent::Text { text: text.clone() });
            }

            Ok(CohereChatMessage::Assistant {
                content,
                tool_calls: tool_calls
                    .iter()
                    .map(core_tool_call_to_cohere_tool_call)
                    .collect(),
                tool_plan: None,
                citations: Vec::new(),
            })
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
                        "Cohere 工具结果消息缺少 tool_call_id，无法构造原生 provider 请求。",
                    ),
                });
            };

            Ok(CohereChatMessage::Tool {
                content: vec![CohereToolResultContent::Text { text: text.clone() }],
                tool_call_id,
            })
        }
    }
}

/// Converts one canonical core tool call into one Cohere native tool-call object.
/// 将一条规范 core 工具调用转换为一条 Cohere 原生工具调用对象。
fn core_tool_call_to_cohere_tool_call(tool_call: &CoreConversationToolCall) -> CohereToolCall {
    CohereToolCall {
        id: tool_call
            .tool_call_id
            .clone()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| Some(tool_call.id.clone())),
        kind: Some(CohereToolCallKind::Function),
        function: Some(CohereToolCallFunction {
            name: tool_call.tool_name.clone(),
            arguments: serde_json::to_string(&tool_call.arguments)
                .unwrap_or_else(|_| tool_call.arguments.to_string()),
        }),
    }
}

/// Converts one host-owned provider tool definition into one Cohere native tool definition.
/// 将一条宿主自有 Provider 工具定义转换为一条 Cohere 原生工具定义。
fn provider_tool_definition_to_cohere_tool_definition(
    definition: &ProviderToolDefinition,
) -> CohereToolDefinition {
    CohereToolDefinition {
        kind: CohereToolCallKind::Function,
        function: CohereFunctionDefinition {
            name: definition.name.clone(),
            description: definition.description.clone(),
            parameters: definition.parameters.clone(),
        },
    }
}

/// Handles one Cohere content-start payload and forwards any immediately available text or thinking fragments.
/// 处理一条 Cohere 内容开始载荷，并转发其中立即可见的文本或思考片段。
fn handle_cohere_content_start<S>(
    live_sender: &ProviderLiveDeltaSender,
    sink: &mut S,
    content: Option<CohereStreamingContentDelta>,
    active_content_kind: &mut Option<ActiveCohereContentKind>,
    active_thinking_text: &mut String,
    assistant_text: &mut String,
) where
    S: VulcanProviderDeltaSink,
{
    let Some(content) = content else {
        *active_content_kind = Some(ActiveCohereContentKind::Other);
        return;
    };

    let kind = match content.kind.as_deref() {
        Some("thinking") => ActiveCohereContentKind::Thinking,
        Some("text") | None => ActiveCohereContentKind::Text,
        _ => ActiveCohereContentKind::Other,
    };
    *active_content_kind = Some(kind);

    match kind {
        ActiveCohereContentKind::Thinking => {
            let delta = content.thinking.or(content.text).unwrap_or_default();
            if !delta.is_empty() {
                active_thinking_text.push_str(&delta);
                forward_cohere_delta(live_sender, sink, CoreProviderTurnDelta::Thinking { delta });
            }
        }
        ActiveCohereContentKind::Text => {
            let delta = content.text.unwrap_or_default();
            if !delta.is_empty() {
                assistant_text.push_str(&delta);
                forward_cohere_delta(live_sender, sink, CoreProviderTurnDelta::Output { delta });
            }
        }
        ActiveCohereContentKind::Other => {}
    }
}

/// Handles one Cohere content-delta payload and forwards it according to the current active content kind.
/// 根据当前活动内容类型处理一条 Cohere 内容增量载荷并转发。
fn handle_cohere_content_delta<S>(
    live_sender: &ProviderLiveDeltaSender,
    sink: &mut S,
    content: Option<CohereStreamingContentDelta>,
    active_content_kind: Option<ActiveCohereContentKind>,
    active_thinking_text: &mut String,
    assistant_text: &mut String,
) where
    S: VulcanProviderDeltaSink,
{
    let Some(content) = content else {
        return;
    };

    match active_content_kind.unwrap_or(ActiveCohereContentKind::Text) {
        ActiveCohereContentKind::Thinking => {
            let delta = content.thinking.or(content.text).unwrap_or_default();
            if !delta.is_empty() {
                active_thinking_text.push_str(&delta);
                forward_cohere_delta(live_sender, sink, CoreProviderTurnDelta::Thinking { delta });
            }
        }
        ActiveCohereContentKind::Text => {
            let delta = content.text.unwrap_or_default();
            if !delta.is_empty() {
                assistant_text.push_str(&delta);
                forward_cohere_delta(live_sender, sink, CoreProviderTurnDelta::Output { delta });
            }
        }
        ActiveCohereContentKind::Other => {}
    }
}

/// Starts one pending Cohere tool call from the current tool-call-start payload.
/// 根据当前工具调用开始载荷启动一条待定 Cohere 工具调用。
fn start_pending_cohere_tool_call(
    tool_call_delta: CohereStreamingToolCallDelta,
) -> PendingCohereToolCall {
    let function = tool_call_delta
        .function
        .unwrap_or(CohereStreamingToolCallFunctionDelta {
            name: None,
            arguments: None,
        });
    PendingCohereToolCall {
        id: tool_call_delta.id.unwrap_or_default(),
        name: function.name.unwrap_or_default(),
        arguments_json: function.arguments.unwrap_or_default(),
    }
}

/// Extracts the JSON-arguments delta fragment from one Cohere tool-call delta payload.
/// 从一条 Cohere 工具调用增量载荷中提取 JSON 参数碎片。
fn extract_cohere_tool_call_arguments_delta(
    tool_call_delta: CohereStreamingToolCallDelta,
) -> Option<String> {
    tool_call_delta
        .function
        .and_then(|function| function.arguments)
}

/// Finalizes one pending Cohere tool call into the canonical core tool-call shape.
/// 将一条待定 Cohere 工具调用收尾为规范 core 工具调用形状。
fn finalize_pending_cohere_tool_call(
    provider_id: &str,
    pending: PendingCohereToolCall,
) -> Result<CoreConversationToolCall, VulcanCoreError> {
    let arguments = if pending.arguments_json.trim().is_empty() {
        json!({})
    } else {
        serde_json::from_str::<Value>(&pending.arguments_json).map_err(|error| {
            VulcanCoreError::ProviderFailure {
                provider_id: provider_id.to_string(),
                message: format!("Cohere 工具参数 JSON 解析失败：{error}"),
            }
        })?
    };

    let provider_visible_id = if pending.id.trim().is_empty() {
        None
    } else {
        Some(pending.id.clone())
    };
    let local_id = provider_visible_id.clone().unwrap_or_else(|| {
        format!(
            "{}:{}",
            pending.name,
            serde_json::to_string(&arguments).unwrap_or_else(|_| arguments.to_string())
        )
    });

    Ok(CoreConversationToolCall {
        id: local_id,
        tool_call_id: provider_visible_id,
        tool_name: pending.name,
        arguments,
    })
}

/// Forwards one normalized Cohere delta into both the controller sink and the host live sender.
/// 将一条规范 Cohere 增量同时转发给控制器汇聚器与宿主实时发送器。
fn forward_cohere_delta<S>(
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
    use super::build_cohere_request_payload;
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

    /// Builds one temporary attachment used by Cohere transport tests.
    /// 构建一条供 Cohere 传输测试使用的临时附件。
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
        let file_path = std::env::temp_dir().join(format!("vulcan-cohere-{unique_id}.{extension}"));
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

    /// Verifies the direct Cohere request payload strips reasoning replay while preserving assistant tool calls and tool-result message shapes.
    /// 验证直连 Cohere 请求载荷会剥离思考回放，同时保留助手工具调用与工具结果消息形状。
    #[test]
    fn request_payload_strips_reasoning_replay_and_preserves_tool_shapes() {
        let request = OpenAiCompatibleCoreTurnRequest {
            snapshot: CoreProviderRequestSnapshot {
                provider_id: String::from("cohere"),
                model_id: String::from("command-a-reasoning-08-2025"),
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
                    "thinking": {
                        "type": "enabled",
                        "token_budget": 8192
                    }
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

        let payload = build_cohere_request_payload(
            "command-a-reasoning-08-2025",
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
        assert_eq!(messages[1]["content"][0]["type"], "text");
        assert_eq!(messages[1]["tool_calls"][0]["id"], "call-1");
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
        assert_eq!(messages[2]["content"][0]["text"], "tool result body");
        assert_eq!(payload["stream"], true);
        assert_eq!(payload["thinking"]["token_budget"], 8192);
        assert_eq!(payload["tools"][0]["function"]["name"], "read_file");
    }

    /// Verifies Cohere request building degrades staged non-text attachments with one explicit modality-specific error.
    /// 验证 Cohere 请求构建会用一条显式模态错误降级已暂存的非文本附件。
    #[test]
    fn request_payload_rejects_non_text_attachments() {
        let attachment = build_test_attachment(
            "mp3",
            ConversationAttachmentKind::Audio,
            "audio/mpeg",
            "sample.mp3",
        );
        let request = OpenAiCompatibleCoreTurnRequest {
            snapshot: CoreProviderRequestSnapshot {
                provider_id: String::from("cohere"),
                model_id: String::from("command-a-reasoning-08-2025"),
                system_prompt: String::from("system prompt"),
                prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                    String::from("system prompt").clone(),
                ),
                prompt: String::from("请转写音频"),
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
                    text: String::from("请转写音频"),
                    attachments: vec![attachment.clone()],
                },
            },
            interleaved_reasoning: None,
        };

        let payload = build_cohere_request_payload("command-a-reasoning-08-2025", request, &[])
            .expect("cohere should degrade non-text attachments into text");
        let messages = payload["messages"]
            .as_array()
            .expect("messages should be an array");
        let rendered = messages[1]["content"][0]["text"]
            .as_str()
            .expect("degraded attachment should be visible as text");

        assert!(rendered.contains("audio input"));
        assert!(rendered.contains("sample.mp3"));

        let _ = std::fs::remove_file(&attachment.local_path);
    }
}
