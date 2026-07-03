use super::*;

pub(super) fn build_openai_compatible_request_payload(
    model_id: &str,
    request: OpenAiCompatibleCoreTurnRequest,
    provider_tool_definitions: &[ProviderToolDefinition],
    include_model_field: bool,
) -> Result<Value, VulcanCoreError> {
    let OpenAiCompatibleCoreTurnRequest {
        snapshot,
        current_history,
        current_prompt,
        interleaved_reasoning,
        ..
    } = request;
    let usage_policy =
        resolve_openai_compatible_streaming_usage_policy(snapshot.provider_id.as_str());
    let schema_projection_profile =
        resolve_tool_schema_projection_profile(&VulcanToolSchemaProjectionContext {
            provider_id: snapshot.provider_id.as_str(),
            api_family_key: "openai-chat-compatible",
            model_id,
            additional_params: snapshot.additional_params.as_ref(),
        });

    let system_prompt = provider_fallback_system_prompt(&snapshot);
    let _unsupported_cache_lowering = lower_provider_cache_policy(
        &default_provider_cache_candidates(),
        ProviderCacheHintSupport::Unsupported,
    );
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
    if !system_prompt.trim().is_empty() {
        messages.push(json!({
            "role": "system",
            "content": system_prompt
        }));
    }
    messages.extend(
        current_history
            .iter()
            .map(|message| {
                core_message_to_openai_compatible_message(
                    message,
                    interleaved_reasoning.as_ref(),
                    snapshot.provider_id.as_str(),
                    snapshot.model_id.as_str(),
                )
            })
            .collect::<Result<Vec<_>, _>>()?,
    );
    messages.push(core_message_to_openai_compatible_message(
        &current_prompt,
        interleaved_reasoning.as_ref(),
        snapshot.provider_id.as_str(),
        snapshot.model_id.as_str(),
    )?);

    if include_model_field {
        payload.insert(String::from("model"), Value::String(model_id.to_string()));
    }
    payload.insert(String::from("messages"), Value::Array(messages));
    payload.insert(String::from("stream"), Value::Bool(true));
    ensure_openai_compatible_stream_usage_requested(&mut payload, &usage_policy);

    if let Some(temperature) = snapshot.temperature {
        payload.insert(String::from("temperature"), json!(temperature));
    }

    if !provider_tool_definitions.is_empty() {
        let mut tools = payload
            .remove("tools")
            .and_then(|value| value.as_array().cloned())
            .unwrap_or_default();
        tools.extend(provider_tool_definitions.iter().map(|definition| {
            provider_tool_definition_to_openai_compatible_tool(
                definition,
                schema_projection_profile,
            )
        }));
        payload.insert(String::from("tools"), Value::Array(tools));
    }

    Ok(Value::Object(payload))
}

/// Ensures OpenAI-compatible streaming requests ask the provider to emit the terminal usage chunk when the upstream protocol supports `stream_options.include_usage`.
/// 确保 OpenAI 兼容流式请求在上游协议支持 `stream_options.include_usage` 时明确请求返回终态 usage 块。
pub(super) fn ensure_openai_compatible_stream_usage_requested(
    payload: &mut JsonMap<String, Value>,
    usage_policy: &OpenAiCompatibleStreamingUsageConfig,
) {
    if !usage_policy.request_include_usage {
        return;
    }
    let mut stream_options = match payload.remove("stream_options") {
        Some(Value::Object(object)) => object,
        Some(_) | None => JsonMap::new(),
    };
    stream_options
        .entry(String::from("include_usage"))
        .or_insert(Value::Bool(true));
    payload.insert(
        String::from("stream_options"),
        Value::Object(stream_options),
    );
}

/// Converts one canonical core message into one OpenAI-compatible request message and replays provider-native interleaved state only when the active model explicitly declares compatible support.
/// 将一条规范 core 消息转换为一条 OpenAI 兼容请求消息，并且仅在当前模型显式声明兼容支持时回放 Provider 原生交错状态。
pub(super) fn core_message_to_openai_compatible_message(
    message: &CoreConversationMessage,
    interleaved_reasoning: Option<&ModelInterleavedCapability>,
    provider_id: &str,
    model_id: &str,
) -> Result<Value, VulcanCoreError> {
    match &message.payload {
        CoreConversationPayload::UserText { text, attachments } => {
            let normalized = normalize_provider_user_input(
                ProviderApiFamily::OpenAiChatCompatible,
                text,
                attachments,
            );
            if normalized.attachments.is_empty() {
                Ok(json!({
                    "role": "user",
                    "content": normalized.text
                }))
            } else {
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
                                "type": "image_url",
                                "image_url": {
                                    "url": attachment_payload.data_url
                                }
                            }));
                        }
                        // OpenAI-compatible Chat endpoints are not uniform beyond text/image parts.
                        // OpenAI-Compatible Chat 端点在文本/图片以外的 Part 形态并不统一。
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
        }
        CoreConversationPayload::Assistant {
            text,
            reasoning,
            provider_message_state,
            tool_calls,
            ..
        } => {
            let mut object =
                if openai_compatible_supports_native_interleaved_replay(interleaved_reasoning) {
                    provider_message_state
                        .as_ref()
                        .and_then(|state| {
                            parse_openai_compatible_provider_message_state(
                                state,
                                provider_id,
                                model_id,
                            )
                        })
                        .unwrap_or_default()
                } else {
                    JsonMap::new()
                };
            object.insert(
                String::from("role"),
                Value::String(String::from("assistant")),
            );
            if !text.is_empty() && !object.contains_key("content") {
                object.insert(String::from("content"), Value::String(text.clone()));
            }
            if tool_calls.is_empty() {
                // Canonical tool calls are the source of truth; stale provider state must not replay tool protocol.
                // 规范工具调用是真实来源；过期 Provider 状态不得回放工具协议。
                object.remove("tool_calls");
            } else {
                // Rebuild tool_calls from canonical calls so assistant ids match following tool messages.
                // 从规范调用重建 tool_calls，确保助手调用标识与后续工具消息一致。
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
            if let Some(reasoning_field) =
                openai_compatible_textual_interleaved_reasoning_field(interleaved_reasoning)
                && !object.contains_key(reasoning_field)
            {
                object.insert(
                    reasoning_field.to_string(),
                    Value::String(reasoning.clone()),
                );
            }
            ensure_openai_compatible_assistant_message_has_required_payload(&mut object);
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
        CoreConversationPayload::AgentMessage { .. } => Ok(json!({
            "role": "user",
            "content": message.payload.agent_message_model_text().unwrap_or_default()
        })),
    }
}

/// Converts one canonical core tool call into one OpenAI-compatible tool-call object with stringified JSON arguments.
/// 将一条规范 core 工具调用转换为一条 OpenAI 兼容工具调用对象，并把 JSON 参数字符串化。
pub(super) fn core_tool_call_to_openai_compatible_tool_call(
    tool_call: &CoreConversationToolCall,
) -> Value {
    let tool_call_id = openai_compatible_replay_tool_call_id(tool_call);
    json!({
        "id": tool_call_id,
        "type": "function",
        "function": {
            "name": tool_call.tool_name,
            "arguments": stringify_openai_compatible_tool_arguments(&tool_call.arguments)
        }
    })
}

/// Resolves the OpenAI-compatible replay id that must match subsequent `tool_call_id` fields.
/// 解析 OpenAI 兼容回放标识，该标识必须与后续 `tool_call_id` 字段一致。
pub(super) fn openai_compatible_replay_tool_call_id(
    tool_call: &CoreConversationToolCall,
) -> String {
    tool_call
        .tool_call_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| tool_call.id.trim())
        .to_string()
}

/// Converts one host-owned provider tool definition into one OpenAI-compatible tool schema object.
/// 将一条宿主自有 Provider 工具定义转换为一条 OpenAI 兼容工具 schema 对象。
pub(super) fn provider_tool_definition_to_openai_compatible_tool(
    definition: &ProviderToolDefinition,
    schema_projection_profile: VulcanToolSchemaProjectionProfile,
) -> Value {
    let parameters =
        project_tool_parameters_schema(&definition.parameters, schema_projection_profile);

    json!({
        "type": "function",
        "function": {
            "name": definition.name,
            "description": definition.description,
            "parameters": parameters
        }
    })
}
