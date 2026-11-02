//! Canonical conversation and route-owned replay projection for requests.
//! 请求使用的规范会话及路由持有重放投影。

use super::provider_failure;
use serde_json::Value;
use vulcan_ai_contract::{
    AiMessage, ContentItem, MessageRole, ProviderReplayEnvelope, ProviderReplayProjection,
    ProviderReplayRoute, ToolCall as PublicToolCall, ToolResult, project_provider_replay,
};
use vulcan_core::types::{
    ConversationPayload as CoreConversationPayload, CoreError as VulcanCoreError,
};

/// Converts one canonical controller message into the public contract.
/// 将一条规范控制器消息转换为公共契约。
pub(super) fn core_message_to_public(
    message: &vulcan_core::types::ConversationMessage,
    provider_id: &str,
    connection_id: &str,
    route_fingerprint: &str,
    protocol_profile_id: &str,
    model_id: &str,
) -> Result<AiMessage, VulcanCoreError> {
    match &message.payload {
        CoreConversationPayload::UserText { text, attachments } => Ok(
            super::input_resources::unprepared_user_message(text, attachments),
        ),
        CoreConversationPayload::Assistant {
            text,
            reasoning,
            provider_message_state,
            tool_calls,
            ..
        } => {
            // projection delegates every durable replay decision to the shared strong-typed entry so
            // this legacy shape can never disagree with the V1 rebuild about what may be replayed.
            // projection 把每一项可持久化重放判定都交给共享的强类型入口，使这个旧形态绝不会与
            // V1 重建在“什么可以重放”上产生分歧。
            let projection = replay_projection_for_route(
                provider_message_state.as_ref(),
                provider_id,
                connection_id,
                route_fingerprint,
                protocol_profile_id,
                model_id,
            )?;
            let (mut content, provider_context_items) = match projection {
                ProviderReplayProjection::ProviderItems(items) => (Vec::new(), items),
                ProviderReplayProjection::CanonicalContent(content) => (content, Vec::new()),
                ProviderReplayProjection::Unusable(reason) => {
                    return Err(provider_failure(
                        provider_id,
                        format!("Persisted public replay envelope cannot be replayed: {reason:?}")
                            .as_str(),
                    ));
                }
                // A missing or foreign envelope contributes no durable provider-owned payload.
                // 缺失或外来封套不贡献任何可持久化供应商持有载荷。
                ProviderReplayProjection::RouteMismatch => (Vec::new(), Vec::new()),
            };
            if content.is_empty() && provider_context_items.is_empty() {
                // localContent is the last resort and never claims provider-owned fidelity.
                // localContent 是最后手段，绝不声称具备供应商持有的保真度。
                if !reasoning.is_empty() {
                    content.push(ContentItem::Reasoning {
                        text: reasoning.clone(),
                        signature: None,
                    });
                }
                if !text.is_empty() {
                    content.push(ContentItem::Text { text: text.clone() });
                }
                for call in tool_calls {
                    // Outbound encoding consumes the canonical wire id only; local runtime aliases never leak.
                    // 出站编码只使用规范线协议标识；本地运行时别名绝不泄漏。
                    let call_id = call
                        .tool_call_id
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .ok_or_else(|| {
                            provider_failure(
                                "vulcan",
                                "Canonical tool call is missing its provider-visible call identifier.",
                            )
                        })?;
                    content.push(ContentItem::ToolCall {
                        call: PublicToolCall {
                            id: call_id.to_string(),
                            name: call.tool_name.clone(),
                            arguments: call.arguments.clone(),
                            signature: None,
                        },
                    });
                }
            }
            Ok(AiMessage {
                role: MessageRole::Assistant,
                content,
                provider_context_items,
            })
        }
        CoreConversationPayload::ToolResult {
            tool_call_id,
            is_error,
            text,
        } => {
            let call_id = tool_call_id.clone().ok_or_else(|| {
                provider_failure(
                    "vulcan",
                    "Canonical tool result is missing its provider-visible call identifier.",
                )
            })?;
            let content = Value::String(text.clone());
            Ok(AiMessage {
                role: MessageRole::Tool,
                content: vec![ContentItem::ToolResult {
                    result: ToolResult {
                        call_id,
                        is_error: *is_error,
                        content,
                    },
                }],
                provider_context_items: Vec::new(),
            })
        }
        CoreConversationPayload::AgentMessage { .. } => Ok(AiMessage {
            role: MessageRole::User,
            content: vec![ContentItem::Text {
                text: message
                    .payload
                    .agent_message_model_text(&message.id)
                    .map_err(|error| provider_failure("vulcan", error.as_str()))?,
            }],
            provider_context_items: Vec::new(),
        }),
    }
}

/// Reads the shared strong-typed replay projection for the exact active execution route.
/// 为当前精确执行路由读取共享的强类型重放投影。
///
/// # Parameters
/// 参数
/// `provider_message_state` is the Core-persisted assistant extension; the remaining identifiers
/// define the currently selected execution route.
/// `provider_message_state` 是 Core 持久化的助手扩展；其余标识定义当前选定执行路由。
///
/// # Returns
/// 返回值
/// The same four-way projection the V1 rebuild uses, or a controlled error when a declared replay
/// envelope is malformed.
/// 返回与 V1 重建完全相同的四态投影；已声明封套损坏时返回受控错误。
fn replay_projection_for_route(
    provider_message_state: Option<&Value>,
    provider_id: &str,
    connection_id: &str,
    route_fingerprint: &str,
    protocol_profile_id: &str,
    model_id: &str,
) -> Result<ProviderReplayProjection, VulcanCoreError> {
    // The legacy Core shape keeps its namespaced wrapper, and only this reader may unwrap it.
    // 旧 Core 形态保留其命名空间包装，且只有本读取函数可以解开它。
    let Some(replay_value) =
        provider_message_state.and_then(|state| state.pointer("/vulcan_ai/replay"))
    else {
        return Ok(ProviderReplayProjection::RouteMismatch);
    };
    let envelope: ProviderReplayEnvelope =
        serde_json::from_value(replay_value.clone()).map_err(|error| {
            provider_failure(
                provider_id,
                format!("Persisted public replay envelope is invalid: {error}").as_str(),
            )
        })?;
    Ok(project_provider_replay(
        &envelope,
        &ProviderReplayRoute {
            provider_id,
            connection_id,
            route_fingerprint,
            protocol_profile_id,
            model_id,
        },
    ))
}

/// Returns the model-visible text of one canonical controller message.
/// 返回一条规范控制器消息的模型可见文本。
pub(super) fn core_message_text(
    message: &vulcan_core::types::ConversationMessage,
) -> Result<String, VulcanCoreError> {
    match &message.payload {
        CoreConversationPayload::UserText { text, .. } => Ok(text.clone()),
        CoreConversationPayload::Assistant { text, .. } => Ok(text.clone()),
        CoreConversationPayload::ToolResult { text, .. } => {
            crate::agent_runtime::tool_result_media::tool_result_text_preview(text)
                .map_err(|error| provider_failure("vulcan", error.as_str()))
        }
        CoreConversationPayload::AgentMessage { .. } => message
            .payload
            .agent_message_model_text(&message.id)
            .map_err(|error| provider_failure("vulcan", error.as_str())),
    }
}

#[cfg(test)]
mod tests;
