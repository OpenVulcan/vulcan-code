use super::*;

pub(super) fn build_openai_compatible_provider_message_state(
    assistant_text: &str,
    finalized_tool_calls: &[CoreConversationToolCall],
    flattened_reasoning_text: Option<String>,
    mut provider_visible_fields: JsonMap<String, Value>,
    provider_id: &str,
    model_id: &str,
) -> Option<Value> {
    if provider_visible_fields.is_empty() && flattened_reasoning_text.is_none() {
        return None;
    }

    provider_visible_fields.insert(
        String::from("role"),
        Value::String(String::from("assistant")),
    );
    if !assistant_text.is_empty() {
        provider_visible_fields.insert(
            String::from("content"),
            Value::String(assistant_text.to_string()),
        );
    }
    if let Some(reasoning_content) = flattened_reasoning_text.filter(|_| {
        !provider_visible_fields.contains_key("reasoning_content")
            && !provider_visible_fields.contains_key("reasoning")
            && !provider_visible_fields.contains_key("reasoning_details")
    }) {
        provider_visible_fields.insert(
            String::from("reasoning_content"),
            Value::String(reasoning_content),
        );
    }
    if !finalized_tool_calls.is_empty() {
        provider_visible_fields.insert(
            String::from("tool_calls"),
            Value::Array(
                finalized_tool_calls
                    .iter()
                    .map(core_tool_call_to_openai_compatible_tool_call)
                    .collect(),
            ),
        );
    }
    ensure_openai_compatible_assistant_message_has_required_payload(&mut provider_visible_fields);

    let payload = Value::Object(JsonMap::from_iter([
        (
            String::from("protocol"),
            Value::String(String::from(
                OPENAI_COMPATIBLE_PROVIDER_MESSAGE_STATE_PROTOCOL,
            )),
        ),
        (
            String::from("assistant_message"),
            Value::Object(provider_visible_fields),
        ),
    ]));
    Some(
        serde_json::to_value(RuntimeProviderNativeStateEnvelope::new(
            OPENAI_COMPATIBLE_PROVIDER_MESSAGE_STATE_PROTOCOL,
            provider_id,
            model_id,
            payload.clone(),
        ))
        .unwrap_or(payload),
    )
}

/// Restores one previously persisted OpenAI-compatible provider_message_state into the raw assistant-message object expected by the wire protocol.
/// 将一份已持久化的 OpenAI 兼容 provider_message_state 恢复为线协议期望的原生助手消息对象。
pub(super) fn parse_openai_compatible_provider_message_state(
    state: &Value,
    provider_id: &str,
    model_id: &str,
) -> Option<JsonMap<String, Value>> {
    let payload =
        match serde_json::from_value::<RuntimeProviderNativeStateEnvelope>(state.clone()).ok() {
            Some(envelope)
                if envelope.matches_scope(
                    OPENAI_COMPATIBLE_PROVIDER_MESSAGE_STATE_PROTOCOL,
                    provider_id,
                    model_id,
                ) =>
            {
                envelope.payload
            }
            Some(_) => return None,
            None => state.clone(),
        };
    let object = payload.as_object()?;
    if object.get("protocol")?.as_str()? != OPENAI_COMPATIBLE_PROVIDER_MESSAGE_STATE_PROTOCOL {
        return None;
    }
    object.get("assistant_message")?.as_object().cloned()
}

/// Ensures one OpenAI-compatible assistant message object always carries provider-required replay fields by injecting an empty `content` string when neither visible content nor tool calls are present.
/// 确保单条 OpenAI 兼容助手消息对象始终携带 Provider 要求的回放字段；当既没有可见内容也没有工具调用时，会注入空字符串 `content`。
pub(super) fn ensure_openai_compatible_assistant_message_has_required_payload(
    object: &mut JsonMap<String, Value>,
) {
    let has_content = object.get("content").is_some_and(|content| match content {
        Value::Null => false,
        Value::Array(items) => !items.is_empty(),
        Value::String(_) => true,
        _ => true,
    });
    let has_tool_calls = object
        .get("tool_calls")
        .is_some_and(|tool_calls| match tool_calls {
            Value::Null => false,
            Value::Array(items) => !items.is_empty(),
            _ => true,
        });
    if !has_content && !has_tool_calls {
        object.insert(String::from("content"), Value::String(String::new()));
    }
}

/// Merges one streamed provider-visible assistant field into the accumulated replay state while preserving common append semantics for string, array, and object payloads.
/// 将一条流式 Provider 可见助手字段合并进累计回放状态，同时为字符串、数组与对象载荷保留常见的追加语义。
pub(super) fn merge_openai_compatible_assistant_field(
    fields: &mut JsonMap<String, Value>,
    field_name: String,
    field_value: Value,
) {
    match fields.get_mut(&field_name) {
        Some(Value::String(existing)) => {
            if let Value::String(incoming) = field_value {
                existing.push_str(&incoming);
            } else {
                fields.insert(field_name, field_value);
            }
        }
        Some(Value::Array(existing)) => {
            if let Value::Array(mut incoming) = field_value {
                existing.append(&mut incoming);
            } else {
                fields.insert(field_name, field_value);
            }
        }
        Some(Value::Object(existing)) => {
            if let Value::Object(incoming) = field_value {
                for (nested_key, nested_value) in incoming {
                    existing.insert(nested_key, nested_value);
                }
            } else {
                fields.insert(field_name, field_value);
            }
        }
        Some(existing) if existing.is_null() => {
            *existing = field_value;
        }
        Some(_) => {
            fields.insert(field_name, field_value);
        }
        None => {
            fields.insert(field_name, field_value);
        }
    }
}

/// Returns whether the active model can safely replay provider-native OpenAI-compatible interleaved assistant fields.
/// 返回当前模型是否可以安全回放 Provider 原生 OpenAI 兼容交错助手字段。
pub(super) fn openai_compatible_supports_native_interleaved_replay(
    interleaved_reasoning: Option<&ModelInterleavedCapability>,
) -> bool {
    match interleaved_reasoning {
        Some(ModelInterleavedCapability::Enabled(true)) => true,
        Some(ModelInterleavedCapability::Field(field)) => !field.field.trim().is_empty(),
        _ => false,
    }
}

pub(super) fn openai_compatible_textual_interleaved_reasoning_field(
    interleaved_reasoning: Option<&ModelInterleavedCapability>,
) -> Option<&str> {
    match interleaved_reasoning {
        Some(ModelInterleavedCapability::Field(field))
            if field.field.trim().eq_ignore_ascii_case("reasoning_content") =>
        {
            Some("reasoning_content")
        }
        _ => None,
    }
}
