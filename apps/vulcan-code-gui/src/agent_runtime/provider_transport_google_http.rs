//! Direct Google Gemini HTTP and SSE transport owned by the host runtime.
//! 宿主运行时自有的 Google Gemini HTTP 与 SSE 直连传输模块。

use crate::agent_runtime::provider_attachment::{
    build_missing_attachment_replay_text, read_provider_attachment,
};
use crate::agent_runtime::provider_prompt_layers::provider_fallback_system_prompt;
use crate::agent_runtime::provider_protocol_anthropic_decoders::sse::iter_sse_messages;
use crate::agent_runtime::provider_protocol_bridge::core_provider_turn_delta_to_runtime_delta;
use crate::agent_runtime::provider_protocol_content::{
    ProviderAssistantContent, ProviderReasoningPart,
};
use crate::agent_runtime::provider_protocol_gemini::{
    GeminiCandidate, GeminiContent, GeminiFunctionCall, GeminiFunctionCallingConfig,
    GeminiFunctionDeclaration, GeminiFunctionResponse, GeminiPart, GeminiStreamingChunk,
    GeminiTool, GeminiToolConfig, build_gemini_provider_message_state,
    parse_gemini_provider_message_state,
};
use crate::agent_runtime::provider_protocol_tool_definition::ProviderToolDefinition;
use crate::agent_runtime::provider_transport_http_support::{
    build_json_http_client, build_rest_endpoint, ensure_json_content_type, ensure_string_header,
    format_standard_error_response,
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
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};
use std::time::{SystemTime, UNIX_EPOCH};
use vulcan_core::provider::VulcanProviderDeltaSink;
use vulcan_core::types::{
    ConversationMessage as CoreConversationMessage, ConversationPayload as CoreConversationPayload,
    ConversationToolCall as CoreConversationToolCall, CoreError as VulcanCoreError,
    ProviderContinuation as CoreProviderContinuation, ProviderTurnDelta as CoreProviderTurnDelta,
    ProviderTurnResponse as CoreProviderTurnResponse,
    ProviderUsageSnapshot as CoreProviderUsageSnapshot,
};
use vulcan_tools_manager::{VulcanToolSchemaProjectionProfile, project_tool_parameters_schema};

/// Default Google Gemini REST base URL used by the host-owned direct transport.
/// 宿主自有直连传输使用的默认 Google Gemini REST 基础地址。
const GOOGLE_GEMINI_DEFAULT_BASE_URL: &str = "https://generativelanguage.googleapis.com";

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
        let body = response.text().await.unwrap_or_default();
        let message = format_standard_error_response(status, &body);
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

    let provider_message_state = (!finalized_parts.is_empty())
        .then(|| build_gemini_provider_message_state(&GeminiContent::model(finalized_parts)));

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

/// Builds one direct Google Gemini HTTP client with merged API key and caller-supplied request headers.
/// 构建一个直连 Google Gemini HTTP 客户端，并合并 API Key 与调用方提供的请求头。
fn build_google_gemini_http_client(
    provider_id: &str,
    api_key: &str,
    mut request_headers: HeaderMap,
) -> Result<reqwest::Client, VulcanCoreError> {
    ensure_string_header(
        provider_id,
        "Google Gemini",
        &mut request_headers,
        "x-goog-api-key",
        api_key,
        "API Key ",
    )?;
    ensure_json_content_type(&mut request_headers);
    build_json_http_client(provider_id, "Google Gemini", request_headers)
}

/// Builds the Google Gemini native streaming endpoint from one optional base URL and one concrete model identifier.
/// 根据可选基础地址与具体模型标识构建 Google Gemini 原生流式端点。
fn build_google_gemini_streaming_endpoint(base_url: Option<&str>, model_id: &str) -> String {
    build_rest_endpoint(
        base_url,
        GOOGLE_GEMINI_DEFAULT_BASE_URL,
        format!("v1beta/models/{model_id}:streamGenerateContent?alt=sse").as_str(),
    )
}

/// Builds one Google Gemini native request payload from the canonical host-owned turn request and mirrored tool definitions.
/// 根据规范宿主单回合请求与镜像工具定义构建一份 Google Gemini 原生请求载荷。
fn build_google_gemini_request_payload(
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
        Some(Value::Object(object)) => Value::Object(object),
        Some(other) => json!({ "hostAdditionalParams": other }),
        None => json!({}),
    };

    let mut all_messages = current_history;
    all_messages.push(current_prompt);

    let mut contents = Vec::new();
    for index in 0..all_messages.len() {
        let content = core_message_to_gemini_content(&all_messages[index], &all_messages[..index])?;
        if !content.parts.is_empty() {
            contents.push(serde_json::to_value(content).map_err(|error| {
                VulcanCoreError::InvalidState {
                    message: format!("Google Gemini 历史消息序列化失败：{error}"),
                }
            })?);
        }
    }

    payload = merge_json_values(payload, json!({ "contents": contents }));

    if !system_prompt.trim().is_empty() {
        payload = merge_json_values(
            payload,
            json!({
                "systemInstruction": {
                    "parts": [
                        {
                            "text": system_prompt
                        }
                    ]
                }
            }),
        );
    }

    if let Some(temperature) = snapshot.temperature {
        payload = merge_json_values(
            payload,
            json!({
                "generationConfig": {
                    "temperature": temperature
                }
            }),
        );
    }

    append_gemini_function_tools(&mut payload, provider_tool_definitions)?;

    Ok(payload)
}

/// Appends Gemini function declarations without replacing provider-native AI tools already present in the payload.
/// 追加 Gemini 函数声明，同时不替换载荷中已经存在的 Provider 原生 AI 工具。
fn append_gemini_function_tools(
    payload: &mut Value,
    provider_tool_definitions: &[ProviderToolDefinition],
) -> Result<(), VulcanCoreError> {
    if provider_tool_definitions.is_empty() {
        return Ok(());
    }

    let Value::Object(object) = payload else {
        return Err(VulcanCoreError::InvalidState {
            message: String::from("Google Gemini 请求载荷不是对象，无法追加工具声明。"),
        });
    };
    let function_tool = serde_json::to_value(GeminiTool {
        function_declarations: provider_tool_definitions
            .iter()
            .map(provider_tool_definition_to_gemini_function_declaration)
            .collect::<Vec<_>>(),
    })
    .map_err(|error| VulcanCoreError::InvalidState {
        message: format!("Google Gemini 工具声明序列化失败：{error}"),
    })?;

    let tools = object
        .entry(String::from("tools"))
        .or_insert_with(|| Value::Array(Vec::new()));
    let Some(tools) = tools.as_array_mut() else {
        return Err(VulcanCoreError::InvalidState {
            message: String::from("Google Gemini 请求载荷中的 tools 字段不是数组。"),
        });
    };
    tools.push(function_tool);

    object.entry(String::from("toolConfig")).or_insert_with(|| {
        json!(GeminiToolConfig {
            function_calling_config: GeminiFunctionCallingConfig {
                mode: String::from("AUTO"),
            }
        })
    });

    Ok(())
}

/// Converts one canonical core message into one Gemini-native content item, preserving stored provider state whenever native replay fidelity is required.
/// 将一条规范 core 消息转换为一条 Gemini 原生内容项，并在需要原生回放精度时优先保留已存储的 Provider 状态。
fn core_message_to_gemini_content(
    message: &CoreConversationMessage,
    previous_messages: &[CoreConversationMessage],
) -> Result<GeminiContent, VulcanCoreError> {
    match &message.payload {
        CoreConversationPayload::UserText { text, attachments } => {
            let normalized = normalize_provider_user_input(
                ProviderApiFamily::GoogleGeminiGenerateContent,
                text,
                attachments,
            );
            let mut parts = Vec::new();
            if !normalized.text.is_empty() {
                parts.push(GeminiPart::text(normalized.text));
            }
            for attachment in &normalized.attachments {
                let attachment_payload = match read_provider_attachment(attachment)? {
                    Some(payload) => payload,
                    None => {
                        parts.push(GeminiPart::text(build_missing_attachment_replay_text(
                            attachment,
                        )));
                        continue;
                    }
                };
                let mut additional_fields = BTreeMap::new();
                additional_fields.insert(
                    String::from("inlineData"),
                    json!({
                        "mimeType": attachment_payload.mime_type,
                        "data": attachment_payload.base64_data
                    }),
                );
                parts.push(GeminiPart {
                    thought: Some(false),
                    additional_fields,
                    ..GeminiPart::default()
                });
            }
            Ok(GeminiContent::user(parts))
        }
        CoreConversationPayload::Assistant {
            text,
            provider_message_state,
            tool_calls,
            ..
        } => {
            if let Some(content) = provider_message_state
                .as_ref()
                .and_then(parse_gemini_provider_message_state)
            {
                return Ok(if content.role.is_some() {
                    content
                } else {
                    GeminiContent::model(content.parts)
                });
            }

            let mut parts = Vec::new();
            if !text.trim().is_empty() {
                parts.push(GeminiPart::text(text.clone()));
            }
            parts.extend(
                tool_calls
                    .iter()
                    .cloned()
                    .map(core_tool_call_to_gemini_function_call)
                    .map(GeminiPart::function_call),
            );
            Ok(GeminiContent::model(parts))
        }
        CoreConversationPayload::ToolResult {
            tool_call_id, text, ..
        } => {
            let Some(request_key) = tool_call_id
                .clone()
                .filter(|value| !value.trim().is_empty())
            else {
                return Err(VulcanCoreError::InvalidState {
                    message: String::from(
                        "Google Gemini 工具结果消息缺少 tool_call_id，无法构造原生 provider 请求。",
                    ),
                });
            };
            let Some((tool_name, provider_visible_call_id)) =
                find_matching_gemini_tool_call(previous_messages, request_key.as_str())
            else {
                return Err(VulcanCoreError::InvalidState {
                    message: format!(
                        "Google Gemini 工具结果无法在历史中找到对应调用 `{request_key}`。"
                    ),
                });
            };

            Ok(GeminiContent::user(vec![GeminiPart::function_response(
                GeminiFunctionResponse {
                    id: provider_visible_call_id,
                    name: tool_name,
                    response: Some(parse_tool_result_text_as_json(text)),
                },
            )]))
        }
    }
}

/// Resolves the matching Gemini tool call for one tool-result message by scanning the already-emitted history in reverse order.
/// 通过逆序扫描已发出的历史，为一条工具结果消息解析对应的 Gemini 工具调用。
fn find_matching_gemini_tool_call(
    previous_messages: &[CoreConversationMessage],
    request_key: &str,
) -> Option<(String, Option<String>)> {
    previous_messages.iter().rev().find_map(|message| {
        let CoreConversationPayload::Assistant { tool_calls, .. } = &message.payload else {
            return None;
        };

        tool_calls.iter().rev().find_map(|tool_call| {
            (resolved_core_tool_call_key(tool_call) == request_key).then(|| {
                (
                    tool_call.tool_name.clone(),
                    tool_call
                        .tool_call_id
                        .clone()
                        .filter(|value| !value.trim().is_empty()),
                )
            })
        })
    })
}

/// Returns the stable lookup key for one canonical core tool call, preferring provider-visible identifiers and falling back to the local item identifier.
/// 返回一条规范 core 工具调用的稳定查找键，优先使用 Provider 可见标识，并在缺失时回退到本地项标识。
fn resolved_core_tool_call_key(tool_call: &CoreConversationToolCall) -> &str {
    tool_call
        .tool_call_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(tool_call.id.as_str())
}

/// Converts one canonical core tool call into one Gemini-native function-call payload while preserving the provider-visible call identifier when available.
/// 将一条规范 core 工具调用转换为一条 Gemini 原生函数调用载荷，并在可用时保留 Provider 可见调用标识。
fn core_tool_call_to_gemini_function_call(
    tool_call: CoreConversationToolCall,
) -> GeminiFunctionCall {
    GeminiFunctionCall {
        id: tool_call
            .tool_call_id
            .clone()
            .filter(|value| !value.trim().is_empty()),
        name: tool_call.tool_name,
        args: tool_call.arguments,
    }
}

/// Converts one host-owned provider tool definition into one Gemini-native function declaration.
/// 将一条宿主自有 Provider 工具定义转换为一条 Gemini 原生函数声明。
fn provider_tool_definition_to_gemini_function_declaration(
    definition: &ProviderToolDefinition,
) -> GeminiFunctionDeclaration {
    let parameters = project_tool_parameters_schema(
        &definition.parameters,
        VulcanToolSchemaProjectionProfile::GeminiSlim,
    );

    GeminiFunctionDeclaration {
        name: definition.name.clone(),
        description: definition.description.clone(),
        parameters: Some(parameters),
    }
}

/// Parses one flattened tool-result text payload into the most structured JSON value available for native Gemini function responses.
/// 将一条已扁平化工具结果文本解析为尽可能结构化的 JSON 值，以供 Gemini 原生函数结果使用。
fn parse_tool_result_text_as_json(text: &str) -> Value {
    serde_json::from_str::<Value>(text).unwrap_or_else(|_| json!({ "result": text }))
}

/// Processes one Gemini candidate payload, forwards live deltas, and mutates the accumulated native part list used for the final provider-state replay.
/// 处理单个 Gemini 候选载荷，转发实时增量，并更新用于最终 Provider 状态回放的原生 Part 累积列表。
fn process_gemini_candidate<S>(
    provider_id: &str,
    candidate: GeminiCandidate,
    live_sender: &ProviderLiveDeltaSender,
    sink: &mut S,
    assistant_text: &mut String,
    finalized_parts: &mut Vec<GeminiPart>,
    pending_reasoning_index: &mut Option<usize>,
    function_part_indices: &mut HashMap<String, usize>,
) -> Result<(), VulcanCoreError>
where
    S: VulcanProviderDeltaSink,
{
    let Some(content) = candidate.content else {
        return Ok(());
    };

    for part in content.parts {
        if let Some(function_call) = part.function_call.clone() {
            let key = gemini_function_call_key(&function_call);
            if let Some(index) = function_part_indices.get(&key).copied() {
                finalized_parts[index] = part;
            } else {
                function_part_indices.insert(key, finalized_parts.len());
                finalized_parts.push(part);
            }
            continue;
        }

        if part.is_reasoning() {
            let text = part.text.clone().unwrap_or_default();
            if !text.is_empty() {
                forward_google_gemini_delta(
                    live_sender,
                    sink,
                    CoreProviderTurnDelta::Thinking {
                        delta: text.clone(),
                    },
                );
            }

            match *pending_reasoning_index {
                Some(index) => {
                    let existing_text = finalized_parts[index].text.clone().unwrap_or_default();
                    finalized_parts[index].text =
                        Some(reconcile_reasoning_text(&existing_text, &text));
                    if part.thought_signature.is_some() {
                        finalized_parts[index].thought_signature = part.thought_signature.clone();
                        *pending_reasoning_index = None;
                    }
                }
                None => {
                    let index = finalized_parts.len();
                    finalized_parts.push(part.clone());
                    if part.thought_signature.is_none() {
                        *pending_reasoning_index = Some(index);
                    }
                }
            }
            continue;
        }

        if let Some(text) = part.text.clone().filter(|value| !value.is_empty()) {
            assistant_text.push_str(&text);
            forward_google_gemini_delta(
                live_sender,
                sink,
                CoreProviderTurnDelta::Output { delta: text },
            );
            finalized_parts.push(part);
            continue;
        }

        finalized_parts.push(part);
    }

    if let Some(finish_reason) = candidate.finish_reason
        && finish_reason.eq_ignore_ascii_case("malformed_function_call")
    {
        return Err(VulcanCoreError::ProviderFailure {
            provider_id: provider_id.to_string(),
            message: String::from("Google Gemini 返回了无效函数调用，无法继续当前回合。"),
        });
    }

    Ok(())
}

/// Reconciles one finalized Gemini reasoning text with the latest streamed chunk while avoiding duplicated prefixes when the provider replays the full block on the signature chunk.
/// 在避免 Provider 在签名块中重放完整文本时产生重复前缀的前提下，协调一段 Gemini 最终思考文本与最新流式块。
fn reconcile_reasoning_text(existing_text: &str, latest_text: &str) -> String {
    if latest_text.is_empty() {
        return existing_text.to_string();
    }
    if existing_text.is_empty() {
        return latest_text.to_string();
    }
    if latest_text.starts_with(existing_text) {
        return latest_text.to_string();
    }
    if existing_text.starts_with(latest_text) {
        return existing_text.to_string();
    }
    format!("{existing_text}{latest_text}")
}

/// Builds the stable deduplication key for one Gemini function call, preferring the native call identifier when available.
/// 为一条 Gemini 函数调用构建稳定去重键，优先使用原生调用标识。
fn gemini_function_call_key(function_call: &GeminiFunctionCall) -> String {
    function_call
        .id
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| {
            format!(
                "{}:{}",
                function_call.name,
                serde_json::to_string(&function_call.args)
                    .unwrap_or_else(|_| function_call.args.to_string())
            )
        })
}

/// Converts one Gemini-native function call into one canonical core tool call.
/// 将一条 Gemini 原生函数调用转换为一条规范 core 工具调用。
fn gemini_function_call_to_core_tool_call(
    function_call: &GeminiFunctionCall,
) -> CoreConversationToolCall {
    let local_id = function_call
        .id
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| gemini_function_call_key(function_call));

    CoreConversationToolCall {
        id: local_id,
        tool_call_id: function_call
            .id
            .clone()
            .filter(|value| !value.trim().is_empty()),
        tool_name: function_call.name.clone(),
        arguments: function_call.args.clone(),
    }
}

/// Forwards one normalized Gemini delta to both the GUI live-delta channel and the `vulcan-core` streaming sink.
/// 将一条规范化 Gemini 增量同时转发给 GUI 实时增量通道与 `vulcan-core` 流式接收器。
fn forward_google_gemini_delta<S>(
    live_sender: &ProviderLiveDeltaSender,
    sink: &mut S,
    delta: CoreProviderTurnDelta,
) where
    S: VulcanProviderDeltaSink,
{
    let runtime_delta = core_provider_turn_delta_to_runtime_delta(delta.clone());
    let _ = live_sender.send(Ok(runtime_delta));
    sink.push_delta(delta);
}

/// Deep-merges two JSON values while allowing overlay branches to replace non-object leaves.
/// 深度合并两个 JSON 值，并允许覆盖分支替换非对象叶子节点。
fn merge_json_values(base: Value, overlay: Value) -> Value {
    match (base, overlay) {
        (Value::Object(mut base_object), Value::Object(overlay_object)) => {
            for (key, overlay_value) in overlay_object {
                let merged_value = match base_object.remove(&key) {
                    Some(base_value) => merge_json_values(base_value, overlay_value),
                    None => overlay_value,
                };
                base_object.insert(key, merged_value);
            }
            Value::Object(base_object)
        }
        (_, overlay) => overlay,
    }
}

/// Returns the current Unix timestamp in milliseconds as one text value for Gemini continuation refreshes.
/// 以文本形式返回当前 Unix 毫秒时间戳，供 Gemini continuation 刷新使用。
fn current_unix_millis_text() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::{
        build_google_gemini_request_payload, core_message_to_gemini_content,
        parse_gemini_provider_message_state,
        provider_tool_definition_to_gemini_function_declaration,
    };
    use crate::agent_runtime::provider_protocol_gemini::{
        GeminiContent, GeminiFunctionCall, GeminiPart, build_gemini_provider_message_state,
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

    /// Builds one temporary attachment used by Gemini multimodal request tests.
    /// 构建一条供 Gemini 多模态请求测试使用的临时附件。
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
        let file_path = std::env::temp_dir().join(format!("vulcan-gemini-{unique_id}.{extension}"));
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

    /// Verifies one stored Gemini provider-message-state round-trips the native content payload without losing thought signatures.
    /// 验证一份已存储 Gemini provider_message_state 可以在不丢失 thought signature 的前提下往返恢复原生内容载荷。
    #[test]
    fn gemini_provider_message_state_round_trips_native_content() {
        let content = GeminiContent::model(vec![GeminiPart::reasoning(
            "reasoning text",
            Some(String::from("sig-1")),
        )]);
        let state = build_gemini_provider_message_state(&content);

        let restored = parse_gemini_provider_message_state(&state)
            .expect("gemini provider state should parse");

        assert_eq!(restored, content);
    }

    /// Verifies Gemini request building replays stored native assistant parts and maps tool results into native function responses with the matching function name.
    /// 验证 Gemini 请求构建会回放已存储的原生助手 Part，并把工具结果映射成带匹配函数名称的原生 function response。
    #[test]
    fn gemini_request_payload_replays_native_parts_and_function_response() {
        let assistant_state = build_gemini_provider_message_state(&GeminiContent::model(vec![
            GeminiPart::reasoning("thinking text", Some(String::from("sig-1"))),
            GeminiPart::function_call(GeminiFunctionCall {
                id: Some(String::from("provider-call-1")),
                name: String::from("read_file"),
                args: json!({"path":"src/main.rs"}),
            }),
        ]));
        let assistant_message = CoreConversationMessage {
            id: String::from("assistant-1"),
            payload: CoreConversationPayload::Assistant {
                text: String::new(),
                reasoning: String::from("thinking text"),
                provider_message_id: None,
                provider_message_state: Some(assistant_state),
                tool_calls: vec![CoreConversationToolCall {
                    id: String::from("tool-item-1"),
                    tool_call_id: Some(String::from("provider-call-1")),
                    tool_name: String::from("read_file"),
                    arguments: json!({"path":"src/main.rs"}),
                }],
                usage: None,
            },
        };
        let tool_result_message = CoreConversationMessage {
            id: String::from("tool-result-1"),
            payload: CoreConversationPayload::ToolResult {
                tool_result_id: String::from("result-1"),
                tool_call_id: Some(String::from("provider-call-1")),
                text: String::from("{\"summary\":\"done\"}"),
            },
        };
        let request = OpenAiCompatibleCoreTurnRequest {
            snapshot: CoreProviderRequestSnapshot {
                provider_id: String::from("google"),
                model_id: String::from("gemini-2.5-flash"),
                system_prompt: String::from("You are helpful."),
                prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                    String::from("You are helpful.").clone(),
                ),
                prompt: String::from("继续"),
                history: Vec::new(),
                temperature: Some(0.2),
                max_turns: 8,
                tool_schemas: Vec::new(),
                additional_params: Some(json!({
                    "generationConfig": {
                        "thinkingConfig": {
                            "thinkingBudget": 2048
                        }
                    }
                })),
                request_headers: None,
                continuation: CoreProviderContinuation::default(),
            },
            current_history: vec![assistant_message, tool_result_message],
            current_prompt: CoreConversationMessage {
                id: String::from("user-1"),
                payload: CoreConversationPayload::UserText {
                    text: String::from("继续"),
                    attachments: Vec::new(),
                },
            },
            interleaved_reasoning: None,
        };

        let payload = build_google_gemini_request_payload(
            request,
            &[ProviderToolDefinition {
                name: String::from("read_file"),
                description: String::from("Read file"),
                parameters: json!({"type":"object"}),
            }],
        )
        .expect("gemini request payload should build");

        let contents = payload["contents"]
            .as_array()
            .expect("contents should be one array");
        assert_eq!(contents[0]["parts"][0]["thoughtSignature"], json!("sig-1"));
        assert_eq!(
            contents[1]["parts"][0]["functionResponse"]["name"],
            json!("read_file")
        );
        assert_eq!(
            contents[1]["parts"][0]["functionResponse"]["id"],
            json!("provider-call-1")
        );
        assert_eq!(
            payload["generationConfig"]["thinkingConfig"]["thinkingBudget"],
            json!(2048)
        );
    }

    /// Verifies Gemini preserves provider-native tools while appending function declarations.
    /// 验证 Gemini 会保留 Provider 原生工具，并继续追加函数声明。
    #[test]
    fn gemini_request_payload_preserves_google_search_and_appends_function_tools() {
        let request = OpenAiCompatibleCoreTurnRequest {
            snapshot: CoreProviderRequestSnapshot {
                provider_id: String::from("google"),
                model_id: String::from("gemini-2.5-flash"),
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
                    "tools": [{ "google_search": {} }]
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

        let payload = build_google_gemini_request_payload(
            request,
            &[ProviderToolDefinition {
                name: String::from("read_file"),
                description: String::from("Read file"),
                parameters: json!({"type":"object"}),
            }],
        )
        .expect("gemini request payload should build");

        assert!(payload["tools"][0].get("google_search").is_some());
        assert_eq!(
            payload["tools"][1]["functionDeclarations"][0]["name"],
            json!("read_file")
        );
        assert_eq!(
            payload["toolConfig"]["functionCallingConfig"]["mode"],
            json!("AUTO")
        );
    }

    /// Verifies Gemini tool-result conversion omits the native response `id` when the originating function call only had a local controller identifier.
    /// 验证当原始函数调用只有本地控制器标识时，Gemini 工具结果转换不会附带原生响应 `id`。
    #[test]
    fn gemini_tool_result_omits_provider_visible_id_when_unavailable() {
        let assistant_message = CoreConversationMessage {
            id: String::from("assistant-1"),
            payload: CoreConversationPayload::Assistant {
                text: String::new(),
                reasoning: String::new(),
                provider_message_id: None,
                provider_message_state: None,
                tool_calls: vec![CoreConversationToolCall {
                    id: String::from("local-tool-id"),
                    tool_call_id: None,
                    tool_name: String::from("read_file"),
                    arguments: json!({"path":"src/main.rs"}),
                }],
                usage: None,
            },
        };
        let tool_result_message = CoreConversationMessage {
            id: String::from("tool-result-1"),
            payload: CoreConversationPayload::ToolResult {
                tool_result_id: String::from("result-1"),
                tool_call_id: Some(String::from("local-tool-id")),
                text: String::from("done"),
            },
        };

        let content = core_message_to_gemini_content(&tool_result_message, &[assistant_message])
            .expect("tool result should convert into gemini content");

        assert!(content.parts[0].function_response.is_some());
        assert_eq!(
            content.parts[0]
                .function_response
                .as_ref()
                .and_then(|response| response.id.clone()),
            None
        );
    }

    /// Verifies Gemini user content includes inline image data parts when the prompt carries staged image attachments.
    /// 验证当提示词携带已暂存图片附件时，Gemini 用户内容会包含内联图片数据 Part。
    #[test]
    fn gemini_request_payload_embeds_user_image_attachments_as_inline_data() {
        let attachment = build_test_attachment(
            "png",
            ConversationAttachmentKind::Image,
            "image/png",
            "diagram.png",
        );
        let request = OpenAiCompatibleCoreTurnRequest {
            snapshot: CoreProviderRequestSnapshot {
                provider_id: String::from("google"),
                model_id: String::from("gemini-2.5-flash"),
                system_prompt: String::from("You are helpful."),
                prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                    String::from("You are helpful.").clone(),
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

        let payload = build_google_gemini_request_payload(request, &[])
            .expect("gemini request payload should build");
        let contents = payload["contents"]
            .as_array()
            .expect("contents should be one array");
        let parts = contents[0]["parts"]
            .as_array()
            .expect("user parts should be one array");

        assert_eq!(contents[0]["role"], "user");
        assert_eq!(parts[0]["text"], "分析图片");
        assert_eq!(parts[1]["inlineData"]["mimeType"], "image/png");
        assert_eq!(parts[1]["inlineData"]["data"], "AAECAw==");

        let _ = std::fs::remove_file(&attachment.local_path);
    }

    /// Verifies Gemini function declarations normalize nested container schemas before they are sent to the upstream GenerateContent API.
    /// 验证 Gemini 函数声明会在发送到上游 GenerateContent API 之前规范化嵌套容器 schema。
    #[test]
    fn gemini_function_declaration_normalizes_nested_container_types() {
        let declaration =
            provider_tool_definition_to_gemini_function_declaration(&ProviderToolDefinition {
                name: String::from("apply_patch_set"),
                description: String::from("Apply one patch batch"),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "patches": {
                            "items": {
                                "properties": {
                                    "file": { "type": "string" },
                                    "replacement": { "type": "string" }
                                }
                            }
                        }
                    }
                }),
            });

        let parameters = declaration.parameters.expect("parameters should exist");
        assert_eq!(parameters["properties"]["patches"]["type"], json!("array"));
        assert_eq!(
            parameters["properties"]["patches"]["items"]["type"],
            json!("object")
        );
    }

    /// Verifies Gemini function declarations hydrate `anyOf` required branches with their minimal property schemas.
    /// 验证 Gemini 函数声明会为 `anyOf` 必填分支补充最小属性 Schema。
    #[test]
    fn gemini_function_declaration_hydrates_required_anyof_branch_properties() {
        let declaration =
            provider_tool_definition_to_gemini_function_declaration(&ProviderToolDefinition {
                name: String::from("vulcan_curl_get"),
                description: String::from("Execute one simple GET request"),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "basic": {
                            "type": "object",
                            "properties": {
                                "username": { "type": "string" },
                                "password": { "type": "string" },
                                "user": { "type": "string" },
                                "pass": { "type": "string" }
                            },
                            "anyOf": [
                                { "required": ["username", "password"] },
                                { "required": ["user", "pass"] }
                            ]
                        }
                    }
                }),
            });

        let parameters = declaration.parameters.expect("parameters should exist");
        let basic = &parameters["properties"]["basic"];
        assert_eq!(basic["anyOf"][0]["type"], json!("object"));
        assert_eq!(
            basic["anyOf"][0]["properties"]["username"]["type"],
            json!("string")
        );
        assert_eq!(
            basic["anyOf"][0]["properties"]["password"]["type"],
            json!("string")
        );
        assert!(basic["anyOf"][0]["properties"]["user"].is_null());
        assert_eq!(basic["anyOf"][1]["type"], json!("object"));
        assert_eq!(
            basic["anyOf"][1]["properties"]["user"]["type"],
            json!("string")
        );
        assert_eq!(
            basic["anyOf"][1]["properties"]["pass"]["type"],
            json!("string")
        );
        assert!(basic["anyOf"][1]["properties"]["username"].is_null());
    }

    /// Verifies Gemini user content includes inline data parts for audio, video, and PDF attachments in addition to images.
    /// 验证 Gemini 用户内容除了图片外，也会为音频、视频与 PDF 附件生成内联数据 Part。
    #[test]
    fn gemini_request_payload_embeds_audio_video_and_pdf_attachments_as_inline_data() {
        let audio_attachment = build_test_attachment(
            "mp3",
            ConversationAttachmentKind::Audio,
            "audio/mpeg",
            "sample.mp3",
        );
        let video_attachment = build_test_attachment(
            "mp4",
            ConversationAttachmentKind::Video,
            "video/mp4",
            "clip.mp4",
        );
        let pdf_attachment = build_test_attachment(
            "pdf",
            ConversationAttachmentKind::Pdf,
            "application/pdf",
            "guide.pdf",
        );
        let request = OpenAiCompatibleCoreTurnRequest {
            snapshot: CoreProviderRequestSnapshot {
                provider_id: String::from("google"),
                model_id: String::from("gemini-2.5-flash"),
                system_prompt: String::from("You are helpful."),
                prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                    String::from("You are helpful.").clone(),
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
                    attachments: vec![
                        audio_attachment.clone(),
                        video_attachment.clone(),
                        pdf_attachment.clone(),
                    ],
                },
            },
            interleaved_reasoning: None,
        };

        let payload = build_google_gemini_request_payload(request, &[])
            .expect("gemini request payload should build");
        let contents = payload["contents"]
            .as_array()
            .expect("contents should be one array");
        let parts = contents[0]["parts"]
            .as_array()
            .expect("user parts should be one array");

        assert_eq!(parts[1]["inlineData"]["mimeType"], "audio/mpeg");
        assert_eq!(parts[2]["inlineData"]["mimeType"], "video/mp4");
        assert_eq!(parts[3]["inlineData"]["mimeType"], "application/pdf");

        let _ = std::fs::remove_file(&audio_attachment.local_path);
        let _ = std::fs::remove_file(&video_attachment.local_path);
        let _ = std::fs::remove_file(&pdf_attachment.local_path);
    }

    /// Verifies a pure attachment-only Gemini user content remains provider-visible when its managed replay file is gone.
    /// 验证纯附件 Gemini 用户内容在托管回放文件缺失时仍会以文本提示形式传给 Provider。
    #[test]
    fn gemini_request_payload_replaces_missing_attachment_only_content_with_text_notice() {
        let attachment = build_test_attachment(
            "png",
            ConversationAttachmentKind::Image,
            "image/png",
            "lost-diagram.png",
        );
        std::fs::remove_file(&attachment.local_path).expect("managed attachment should remove");
        let request = OpenAiCompatibleCoreTurnRequest {
            snapshot: CoreProviderRequestSnapshot {
                provider_id: String::from("google"),
                model_id: String::from("gemini-2.5-flash"),
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

        let payload = build_google_gemini_request_payload(request, &[])
            .expect("gemini request payload should build");
        let contents = payload["contents"]
            .as_array()
            .expect("contents should be one array");
        let parts = contents[0]["parts"]
            .as_array()
            .expect("user parts should be one array");

        assert_eq!(contents[0]["role"], "user");
        assert_eq!(
            parts[0]["text"],
            json!("ERROR: Attachment \"lost-diagram.png\" no longer exists. Please ignore it.")
        );
    }
}
