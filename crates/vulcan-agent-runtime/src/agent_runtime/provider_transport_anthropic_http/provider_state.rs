use super::*;

/// Builds the Anthropic-native state payload stored in one scoped provider-state envelope.
/// 构建存入带作用域 Provider 状态信封中的 Anthropic 原生状态载荷。
pub(super) fn build_anthropic_provider_message_state(
    provider_id: &str,
    model_id: &str,
    content: &[AnthropicContentBlock],
) -> Value {
    let payload = json!({
        ANTHROPIC_PROVIDER_STATE_KEY: {
            "content": content
        }
    });
    serde_json::to_value(RuntimeProviderNativeStateEnvelope::new(
        "anthropic",
        provider_id,
        model_id,
        payload,
    ))
    .unwrap_or_else(|_| {
        json!({
            ANTHROPIC_PROVIDER_STATE_KEY: {
                "content": content
            }
        })
    })
}

/// Restores replayable Anthropic-native assistant content from the generic provider-message-state payload when future history replay needs provider-native fidelity.
/// 当后续历史回放需要 Provider 原生精度时，从通用 provider_message_state 载荷中恢复可回放的 Anthropic 原生助手内容。
pub(super) fn parse_anthropic_provider_message_state(
    state: &Value,
    provider_id: &str,
    model_id: &str,
) -> Option<Vec<AnthropicContentBlock>> {
    let payload =
        match serde_json::from_value::<RuntimeProviderNativeStateEnvelope>(state.clone()).ok() {
            Some(envelope) if envelope.matches_scope("anthropic", provider_id, model_id) => {
                envelope.payload
            }
            Some(_) => return None,
            None => state.clone(),
        };

    payload
        .get(ANTHROPIC_PROVIDER_STATE_KEY)?
        .get("content")
        .cloned()
        .and_then(|value| serde_json::from_value::<Vec<AnthropicContentBlock>>(value).ok())
}

pub(super) fn repair_replayable_anthropic_provider_content(
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
