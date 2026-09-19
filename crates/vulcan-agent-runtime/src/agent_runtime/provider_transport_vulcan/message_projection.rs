//! Canonical conversation and route-owned replay projection for requests and compaction.
//! 请求与压缩使用的规范会话及路由持有重放投影。

use super::provider_failure;
use crate::agent_runtime::provider_request_bridge_conversation::build_core_conversation_snapshot;
use serde_json::Value;
use std::collections::BTreeSet;
use vulcan_ai_contract::{
    AiMessage, ContentItem, MessageRole, ProviderReplayEnvelope, ProviderReplayProjection,
    ProviderReplayRoute, ToolCall as PublicToolCall, ToolResult,
    canonicalize_codex_responses_replay_items, is_replayable_codex_responses_item,
    project_provider_replay,
};
use vulcan_context::{SessionConversationState, SessionMessageEntry};
use vulcan_core::types::{
    ConversationPayload as CoreConversationPayload, CoreError as VulcanCoreError,
};
use vulcan_provider_adapters::ResolvedPublicAiExecution;

/// Converts one canonical controller message into the public contract.
/// 将一条规范控制器消息转换为公共契约。
/// Projects the exact planned canonical source through the same route used for compact dispatch.
/// 通过压缩分派使用的同一路由投影精确规划的规范来源。
///
/// # Parameters
/// 参数
/// `conversation_state` owns lossless canonical messages, `source` selects the exact planner window,
/// and `resolved` freezes route, model capabilities, media projection, and opaque replay identity.
/// `conversation_state` 持有无损规范消息，`source` 选择精确规划窗口，`resolved` 冻结路由、
/// 模型能力、媒体投影与不透明重放身份。
///
/// # Returns
/// 返回值
/// Ordered provider-neutral messages with tools, results, images, and prior compact items preserved,
/// or a pre-dispatch projection error without sending a provider request.
/// 返回保留工具、结果、图片与既有压缩项的有序供应商无关消息；投影失败时在发送前返回错误。
pub(crate) async fn build_responses_compaction_messages(
    conversation_state: &SessionConversationState,
    source: &[SessionMessageEntry],
    resolved: &std::sync::Arc<ResolvedPublicAiExecution>,
    session_scope_id: Option<&str>,
    media_projection: Option<&dyn crate::prompt_media_projection::PromptMediaProjectionRuntime>,
) -> Result<Vec<AiMessage>, String> {
    // sourceIds is the exact planner-selected canonical window and rejects ambiguous duplicates.
    // sourceIds 是规划器精确选中的规范窗口，并拒绝有歧义的重复标识。
    let source_ids = source
        .iter()
        .map(|message| message.id.as_str())
        .collect::<BTreeSet<_>>();
    if source_ids.len() != source.len() {
        return Err(String::from(
            "Remote Responses compaction source contains duplicate message identities.",
        ));
    }
    // The host projection uses the same ordered source window and frozen execution as compact dispatch.
    // 宿主投影使用与压缩分派相同的有序来源窗口和冻结执行。
    let mut projected_media = if let Some(projection) = media_projection {
        projection
            .project_readonly(
                &source
                    .iter()
                    .map(|message| message.id.clone())
                    .collect::<Vec<_>>(),
                crate::agent_runtime::events::ProviderRequestMediaContext {
                    execution: std::sync::Arc::clone(resolved),
                    session_scope_id: session_scope_id
                        .ok_or_else(|| {
                            String::from(
                                "Prompt-owned media compaction requires an exact session scope.",
                            )
                        })?
                        .to_string(),
                },
            )
            .await?
    } else {
        Default::default()
    };
    // snapshot validates canonical identities before any route-specific projection runs.
    // snapshot 在执行任何路由专属投影前校验规范身份。
    let snapshot = build_core_conversation_snapshot(conversation_state)?;
    // routeFingerprint must match both existing opaque checkpoints and the immutable dispatch route.
    // routeFingerprint 必须同时匹配既有不透明检查点与不可变分派路由。
    let route_fingerprint = resolved.route.request_url("/responses");
    // projectedIds proves every planner-selected source row was found in the canonical snapshot.
    // projectedIds 证明规划器选择的每条来源记录都存在于规范快照中。
    let mut projected_ids = BTreeSet::new();
    // messages retains canonical order after exact-window filtering.
    // messages 在精确窗口过滤后保留规范顺序。
    let mut messages = Vec::new();
    for message in &snapshot.messages {
        if !source_ids.contains(message.id.as_str()) {
            continue;
        }
        projected_ids.insert(message.id.as_str());
        if let Some(projected) = projected_media.remove(&message.id) {
            // The exact user prompt already has its native or safe-address content; never run the legacy cache projector over it.
            // 精确用户提示词已经具有原生或安全地址内容；绝不再对它运行旧缓存投影器。
            messages.push(projected);
            continue;
        }
        // publicMessage uses the normal request bridge so tool and provider item shapes cannot diverge.
        // publicMessage 使用普通请求桥接，避免工具与供应商项形态发生分歧。
        let public_message = core_message_to_public(
            message,
            resolved.route.provider.id.as_str(),
            resolved.route.connection.id.as_str(),
            route_fingerprint.as_str(),
            resolved.route.protocol_route.protocol_profile_id.as_str(),
            resolved.model.id.as_str(),
        )
        .map_err(|error| error.to_string())?;
        messages.push(public_message);
    }
    if projected_ids != source_ids || !projected_media.is_empty() {
        return Err(String::from(
            "Remote Responses compaction source no longer matches the canonical conversation.",
        ));
    }
    // Prompt-owned resources have already passed the shared chronological planner; no second budget/cache pass is permitted.
    // 提示词所有资源已经通过共享时序规划器；不允许第二次预算或缓存遍历。
    messages.retain(|message| {
        !message.content.is_empty() || !message.provider_context_items.is_empty()
    });
    Ok(messages)
}

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
            let (mut content, mut provider_context_items) = match projection {
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
            if provider_context_items.is_empty() {
                // Route-bound opaque compaction items keep their prior precedence over rebuilt local text.
                // 绑定路由的不透明压缩项保持其先于本地重建文本的既有优先级。
                provider_context_items = responses_compaction_items_for_route(
                    provider_message_state.as_ref(),
                    provider_id,
                    connection_id,
                    route_fingerprint,
                    protocol_profile_id,
                    model_id,
                )?;
            }
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
            let content = crate::agent_runtime::tool_result_media::project_tool_result_content(
                text,
                protocol_profile_id,
            )
            .map_err(|error| provider_failure(provider_id, error.as_str()))?;
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
                    .agent_message_model_text()
                    .ok_or_else(|| provider_failure("vulcan", "Agent message rendering failed."))?,
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

/// Reads route-bound remote Responses compaction items from one canonical assistant state.
/// 从一条规范助手状态读取绑定路由的远端 Responses 压缩项。
///
/// # Parameters
/// 参数
/// `provider_message_state` is the persisted assistant extension; the remaining identifiers freeze
/// the only route allowed to replay the ordered opaque Responses item list.
/// `provider_message_state` 是持久化助手扩展；其余标识冻结唯一允许重放有序不透明 Responses 项列表的路由。
///
/// # Returns
/// 返回值
/// A validated ordered Responses-item list, or an empty list when no exact route-bound state exists.
/// 返回已验证的有序 Responses 项列表；不存在精确路由绑定状态时返回空列表。
fn responses_compaction_items_for_route(
    provider_message_state: Option<&Value>,
    provider_id: &str,
    connection_id: &str,
    route_fingerprint: &str,
    protocol_profile_id: &str,
    model_id: &str,
) -> Result<Vec<Value>, VulcanCoreError> {
    let Some(value) =
        provider_message_state.and_then(|state| state.pointer("/vulcan_ai/responses_compaction"))
    else {
        return Ok(Vec::new());
    };
    let envelope = value.as_object().ok_or_else(|| {
        provider_failure(
            provider_id,
            "Persisted Responses compaction envelope must be an object.",
        )
    })?;
    if envelope.get("provider_id").and_then(Value::as_str) != Some(provider_id)
        || envelope.get("connection_id").and_then(Value::as_str) != Some(connection_id)
        || envelope.get("route_fingerprint").and_then(Value::as_str) != Some(route_fingerprint)
        || envelope.get("protocol_profile_id").and_then(Value::as_str) != Some(protocol_profile_id)
        || envelope.get("model_id").and_then(Value::as_str) != Some(model_id)
    {
        return Err(provider_failure(
            provider_id,
            "Persisted Responses compaction context belongs to a different provider route and cannot be discarded safely.",
        ));
    }
    let items = envelope
        .get("items")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            provider_failure(
                provider_id,
                "Persisted Responses compaction envelope is missing its item list.",
            )
        })?;
    // canonicalItems mirrors the official Codex deserializer alias before replay validation.
    // canonicalItems 在重放校验前镜像官方 Codex 反序列化器别名。
    let mut canonical_items = items.clone();
    canonicalize_codex_responses_replay_items(&mut canonical_items);
    let contains_compaction = canonical_items
        .iter()
        .any(|item| item.get("type").and_then(Value::as_str) == Some("compaction"));
    if canonical_items.is_empty()
        || !canonical_items
            .iter()
            .all(is_replayable_codex_responses_item)
        || !contains_compaction
    {
        return Err(provider_failure(
            provider_id,
            "Persisted Responses compaction envelope contains an invalid ordered replayable item.",
        ));
    }
    Ok(canonical_items)
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
            .agent_message_model_text()
            .ok_or_else(|| provider_failure("vulcan", "Agent message rendering failed.")),
    }
}

#[cfg(test)]
mod tests;
