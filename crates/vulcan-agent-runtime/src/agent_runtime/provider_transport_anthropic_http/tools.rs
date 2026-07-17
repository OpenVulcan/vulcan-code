use super::*;

pub(super) fn parse_pending_anthropic_tool_use_input_json(
    tool_use: &PendingAnthropicToolUse,
) -> Result<Value, VulcanCoreError> {
    if tool_use
        .input_json
        .read(|input_json| input_json.trim().is_empty())
    {
        Ok(json!({}))
    } else {
        tool_use.input_json.read(|input_json| {
            serde_json::from_str::<Value>(input_json).map_err(|error| {
                VulcanCoreError::ProviderFailure {
                    provider_id: String::from("anthropic"),
                    message: format!("Anthropic 工具输入 JSON 解析失败：{error}"),
                }
            })
        })
    }
}

/// Converts one pending Anthropic tool-use assembly into one replayable Anthropic-native content block so future history replays preserve provider-visible identifiers and names.
/// 将一条待定 Anthropic 工具使用累计结果转换为一条可回放的 Anthropic 原生内容块，以便后续历史回放保留 Provider 可见标识与名称。
pub(super) fn anthropic_content_block_from_pending_tool_use(
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
pub(super) fn anthropic_content_block_from_pending_server_tool_use(
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
pub(super) fn replayable_anthropic_server_tool_use_id(
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
pub(super) fn replayable_anthropic_tool_use_id(
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
pub(super) fn replayable_core_tool_call_id(tool_call: &CoreConversationToolCall) -> String {
    tool_call
        .tool_call_id
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| tool_call.id.clone())
}

pub(super) fn core_tool_call_to_anthropic_tool_use(
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
pub(super) fn provider_tool_definition_to_anthropic_tool(
    definition: &ProviderToolDefinition,
) -> Value {
    json!({
        "name": definition.name,
        "description": definition.description,
        "input_schema": definition.parameters
    })
}

/// Finalizes one pending Anthropic tool-use block into the canonical core tool-call shape.
/// 将一条待定 Anthropic 工具使用块收尾为规范 core 工具调用形状。
pub(super) fn finalize_pending_anthropic_tool_use(
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
pub(super) fn resolve_provider_visible_anthropic_tool_name(
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
pub(super) fn resolve_host_owned_anthropic_tool_name(
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

/// Resolves a streamed provider tool name only when it exactly matches the frozen request tool set.
/// 仅当流式 Provider 工具名称精确匹配冻结请求工具集合时进行解析。
///
/// # Parameters
/// 参数
/// `provider_tool_name` is the complete name received in a content-block start event.
/// `provider_tool_name` 是内容块开始事件中接收的完整名称。
/// `provider_tool_definitions` is the exact provider-facing tool snapshot for this request.
/// `provider_tool_definitions` 是当前请求的精确 Provider 可见工具快照。
///
/// # Returns
/// 返回值
/// The canonical public tool name for an exact match, otherwise `None`.
/// 精确匹配时返回规范公开工具名称，否则返回 `None`。
pub(super) fn resolve_exact_streamed_anthropic_tool_name(
    provider_tool_name: &str,
    provider_tool_definitions: &[ProviderToolDefinition],
) -> Option<String> {
    provider_tool_definitions
        .iter()
        .find(|definition| definition.name == provider_tool_name)
        .map(|definition| definition.name.clone())
}

/// Builds one Anthropic provider-visible tool name only for replay compatibility of historical invalid host identifiers.
/// 仅为历史非法宿主标识的回放兼容构建一条 Anthropic Provider 可见工具名称。
pub(super) fn build_anthropic_provider_tool_name(host_tool_name: &str) -> String {
    build_direct_public_tool_name(host_tool_name)
}
