use super::*;

pub(super) fn build_anthropic_request_payload(
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
    let max_tokens = extract_anthropic_max_tokens(model_id, &mut payload);
    payload.insert(String::from("max_tokens"), json!(max_tokens));
    payload.insert(String::from("stream"), Value::Bool(true));

    if !system_prompt.trim().is_empty() {
        let mut system_block = json!({
            "type": "text",
            "text": system_prompt
        });
        apply_anthropic_system_cache_control(&mut system_block);
        payload.insert(String::from("system"), Value::Array(vec![system_block]));
    }

    if let Some(temperature) = snapshot.temperature {
        payload.insert(String::from("temperature"), json!(temperature));
    }

    let anthropic_messages = current_history
        .iter()
        .map(|message| {
            core_message_to_anthropic_message(
                message,
                provider_tool_definitions,
                snapshot.provider_id.as_str(),
                snapshot.model_id.as_str(),
            )
        })
        .chain(std::iter::once(core_message_to_anthropic_message(
            &current_prompt,
            provider_tool_definitions,
            snapshot.provider_id.as_str(),
            snapshot.model_id.as_str(),
        )))
        .collect::<Result<Vec<_>, _>>()?;
    let anthropic_messages = coalesce_consecutive_anthropic_user_messages(anthropic_messages);
    let mut anthropic_messages = repair_anthropic_tool_result_topology(anthropic_messages);
    apply_anthropic_message_cache_control(&mut anthropic_messages);
    payload.insert(String::from("messages"), Value::Array(anthropic_messages));

    append_anthropic_function_tools(&mut payload, provider_tool_definitions)?;
    apply_anthropic_tool_definition_cache_control(&mut payload);

    Ok(Value::Object(payload))
}

/// Applies Anthropic ephemeral cache-control to one selected system block when the neutral policy chooses the system breakpoint.
/// 当中立策略选择 system 断点时，为单个 Anthropic system block 应用 ephemeral cache-control。
pub(super) fn apply_anthropic_system_cache_control(system_block: &mut Value) {
    let selected_targets = selected_anthropic_cache_targets();
    let should_cache_system = selected_targets.contains(&ProviderCacheHintTarget::System);
    if should_cache_system && let Some(object) = system_block.as_object_mut() {
        insert_anthropic_ephemeral_cache_control(object);
    }
}

/// Returns the provider-neutral cache targets that Anthropic can express in this request.
/// 返回 Anthropic 在本次请求中可以表达的 Provider 中立缓存目标。
pub(super) fn selected_anthropic_cache_targets() -> Vec<ProviderCacheHintTarget> {
    lower_provider_cache_policy(
        &default_provider_cache_candidates(),
        ProviderCacheHintSupport::AnthropicEphemeral {
            max_breakpoints: ANTHROPIC_MAX_CACHE_BREAKPOINTS,
        },
    )
    .selected
    .into_iter()
    .map(|breakpoint| breakpoint.target)
    .collect()
}

/// Applies Anthropic ephemeral cache-control to selected message content blocks after topology repair.
/// 在拓扑修复完成后，把 Anthropic ephemeral 缓存控制应用到已选消息内容块。
pub(super) fn apply_anthropic_message_cache_control(messages: &mut [Value]) {
    let selected_targets = selected_anthropic_cache_targets();
    if selected_targets.contains(&ProviderCacheHintTarget::LastToolResult) {
        apply_anthropic_cache_control_to_latest_tool_result(messages);
    }
    if selected_targets.contains(&ProviderCacheHintTarget::LastUser) {
        apply_anthropic_cache_control_to_latest_user_block(messages);
    }
}

/// Applies cache-control to the latest Anthropic function tool definition when the neutral policy selects tool-definition caching.
/// 当中立策略选择工具定义缓存时，将缓存控制应用到最新的 Anthropic 函数工具定义。
pub(super) fn apply_anthropic_tool_definition_cache_control(payload: &mut JsonMap<String, Value>) {
    let selected_targets = selected_anthropic_cache_targets();
    if !selected_targets.contains(&ProviderCacheHintTarget::LastToolDefinition) {
        return;
    }

    let Some(tools) = payload.get_mut("tools").and_then(Value::as_array_mut) else {
        return;
    };
    let Some(tool) = tools.iter_mut().rev().find(|tool| {
        tool.get("input_schema").is_some()
            && tool
                .get("name")
                .and_then(Value::as_str)
                .is_some_and(|name| !name.trim().is_empty())
    }) else {
        return;
    };
    if let Some(object) = tool.as_object_mut() {
        insert_anthropic_ephemeral_cache_control(object);
    }
}

/// Applies cache-control to the latest valid Anthropic `tool_result` block.
/// 将缓存控制应用到最新的合法 Anthropic `tool_result` 内容块。
pub(super) fn apply_anthropic_cache_control_to_latest_tool_result(messages: &mut [Value]) {
    for message in messages.iter_mut().rev() {
        let Some(content) = message.get_mut("content").and_then(Value::as_array_mut) else {
            continue;
        };
        let Some(block) = content
            .iter_mut()
            .rev()
            .find(|block| block.get("type").and_then(Value::as_str) == Some("tool_result"))
        else {
            continue;
        };
        if let Some(object) = block.as_object_mut() {
            insert_anthropic_ephemeral_cache_control(object);
        }
        return;
    }
}

/// Applies cache-control to the latest user-authored Anthropic content block.
/// 将缓存控制应用到最新的用户创作 Anthropic 内容块。
pub(super) fn apply_anthropic_cache_control_to_latest_user_block(messages: &mut [Value]) {
    for message in messages.iter_mut().rev() {
        if message.get("role").and_then(Value::as_str) != Some("user") {
            continue;
        }
        let Some(content) = message.get_mut("content").and_then(Value::as_array_mut) else {
            continue;
        };
        let Some(block) = content
            .iter_mut()
            .rev()
            .find(|block| block.get("type").and_then(Value::as_str) != Some("tool_result"))
        else {
            continue;
        };
        if let Some(object) = block.as_object_mut() {
            insert_anthropic_ephemeral_cache_control(object);
        }
        return;
    }
}

/// Inserts Anthropic ephemeral cache-control into one JSON object block.
/// 向单个 JSON 对象内容块写入 Anthropic ephemeral 缓存控制。
pub(super) fn insert_anthropic_ephemeral_cache_control(object: &mut JsonMap<String, Value>) {
    object.insert(
        String::from("cache_control"),
        json!({ "type": "ephemeral" }),
    );
}

/// Appends Anthropic function tools without replacing provider-native server tools already present in the payload.
/// 追加 Anthropic 函数工具，同时不替换载荷中已经存在的 Provider 原生服务端工具。
pub(super) fn append_anthropic_function_tools(
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
pub(super) fn extract_anthropic_max_tokens(
    model_id: &str,
    payload: &mut JsonMap<String, Value>,
) -> u64 {
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
pub(super) fn core_message_to_anthropic_message(
    message: &CoreConversationMessage,
    provider_tool_definitions: &[ProviderToolDefinition],
    provider_id: &str,
    model_id: &str,
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
            if let Some(content) = provider_message_state.as_ref().and_then(|state| {
                parse_anthropic_provider_message_state(state, provider_id, model_id)
            }) {
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
        CoreConversationPayload::AgentMessage { .. } => Ok(json!({
            "role": "user",
            "content": [
                {
                    "type": "text",
                    "text": message.payload.agent_message_model_text().unwrap_or_default()
                }
            ]
        })),
    }
}
