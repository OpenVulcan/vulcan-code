//! Canonical conversation-message bridge helpers reused by snapshot assembly and provider-visible history projection.
//! 供快照组装与 Provider 可见历史投影复用的规范会话消息桥接辅助逻辑。

use crate::agent_runtime::provider_system_update_lowering::{
    ProviderSystemUpdateLowering, ProviderSystemUpdateLoweringPolicy, lower_provider_system_update,
};
use crate::conversation_protocol::{
    SessionConversationMessage, SessionConversationPayload, SessionConversationToolCall,
};
use crate::provider_capabilities::ResolvedModelCapabilities;
use crate::provider_config::ProviderApiFamily;
use crate::provider_input_normalizer::normalize_provider_user_input_for_model;
use std::collections::BTreeSet;
use vulcan_context::RuntimeSystemUpdateEvent;
use vulcan_core::types::{
    ConversationMessage as CoreConversationMessage, ConversationPayload as CoreConversationPayload,
    ConversationToolCall as CoreConversationToolCall,
};

/// Converts one session-owned canonical conversation message into the `vulcan-core` message shape.
/// 将一条会话持有的规范会话消息转换为 `vulcan-core` 消息形状。
pub(crate) fn session_message_to_core_message(
    message: &SessionConversationMessage,
) -> CoreConversationMessage {
    let payload = match &message.payload {
        SessionConversationPayload::UserText { text, attachments } => {
            CoreConversationPayload::UserText {
                text: text.clone(),
                attachments: attachments.clone(),
            }
        }
        SessionConversationPayload::Assistant {
            text,
            reasoning,
            provider_message_id,
            provider_message_state,
            usage,
            tool_calls,
        } => CoreConversationPayload::Assistant {
            text: text.clone(),
            reasoning: reasoning.clone(),
            provider_message_id: provider_message_id.clone(),
            provider_message_state: provider_message_state.clone(),
            usage: usage.clone(),
            tool_calls: tool_calls
                .iter()
                .map(session_tool_call_to_core_tool_call)
                .collect(),
        },
        SessionConversationPayload::ToolResult {
            tool_result_id,
            tool_call_id,
            text,
        } => CoreConversationPayload::ToolResult {
            tool_result_id: tool_result_id.clone(),
            tool_call_id: tool_call_id.clone(),
            text: text.clone(),
        },
    };

    CoreConversationMessage {
        id: message.id.clone(),
        payload,
    }
}

/// Converts one `vulcan-core` message back into the GUI-owned canonical session message shape.
/// 将一条 `vulcan-core` 消息转换回 GUI 持有的规范会话消息形状。
pub(crate) fn core_message_to_session_message(
    message: &CoreConversationMessage,
) -> SessionConversationMessage {
    let payload = match &message.payload {
        CoreConversationPayload::UserText { text, attachments } => {
            SessionConversationPayload::UserText {
                text: text.clone(),
                attachments: attachments.clone(),
            }
        }
        CoreConversationPayload::Assistant {
            text,
            reasoning,
            provider_message_id,
            provider_message_state,
            usage,
            tool_calls,
        } => SessionConversationPayload::Assistant {
            text: text.clone(),
            reasoning: reasoning.clone(),
            provider_message_id: provider_message_id.clone(),
            provider_message_state: provider_message_state.clone(),
            usage: usage.clone(),
            tool_calls: tool_calls
                .iter()
                .map(core_tool_call_to_session_tool_call)
                .collect(),
        },
        CoreConversationPayload::ToolResult {
            tool_result_id,
            tool_call_id,
            text,
        } => SessionConversationPayload::ToolResult {
            tool_result_id: tool_result_id.clone(),
            tool_call_id: tool_call_id.clone(),
            text: text.clone(),
        },
    };

    SessionConversationMessage {
        id: message.id.clone(),
        payload,
    }
}

/// Converts canonical core messages into the provider-visible history that will actually be sent upstream.
/// 将规范 core 消息转换为真正会发送给上游 Provider 的可见历史。
pub(crate) fn core_messages_to_provider_visible_history(
    messages: &[CoreConversationMessage],
) -> Vec<CoreConversationMessage> {
    core_messages_to_provider_visible_history_for_model(messages, None, None)
}

/// Converts canonical core messages into provider-visible history using API-family and model modality projection.
/// 使用接口族与模型模态投影将规范 core 消息转换为 Provider 可见历史。
pub(crate) fn core_messages_to_provider_visible_history_for_model(
    messages: &[CoreConversationMessage],
    api_family: Option<ProviderApiFamily>,
    capabilities: Option<&ResolvedModelCapabilities>,
) -> Vec<CoreConversationMessage> {
    core_messages_to_provider_visible_history_for_model_with_system_updates(
        messages,
        api_family,
        capabilities,
        &[],
        ProviderSystemUpdateLoweringPolicy::default(),
    )
    .expect("empty system update overlays should not fail")
}

/// Converts canonical core messages plus chronological system updates into provider-visible history.
/// 将规范 core 消息与时间顺序 system update 一起转换为 Provider 可见历史。
pub(crate) fn core_messages_to_provider_visible_history_for_model_with_system_updates(
    messages: &[CoreConversationMessage],
    api_family: Option<ProviderApiFamily>,
    capabilities: Option<&ResolvedModelCapabilities>,
    system_updates: &[RuntimeSystemUpdateEvent],
    system_update_policy: ProviderSystemUpdateLoweringPolicy,
) -> Result<Vec<CoreConversationMessage>, String> {
    let mut projected = project_core_messages_to_provider_visible_history_for_model(
        messages,
        api_family,
        capabilities,
    );
    apply_provider_visible_system_updates(&mut projected, system_updates, system_update_policy)?;

    #[cfg(debug_assertions)]
    validate_provider_visible_tool_topology(&projected)?;

    Ok(projected)
}

/// Projects canonical messages into provider-visible history before runtime system-update overlays are applied.
/// 在应用 runtime system-update 覆盖之前，将规范消息投影为 Provider 可见历史。
fn project_core_messages_to_provider_visible_history_for_model(
    messages: &[CoreConversationMessage],
    api_family: Option<ProviderApiFamily>,
    capabilities: Option<&ResolvedModelCapabilities>,
) -> Vec<CoreConversationMessage> {
    let mut projected = Vec::with_capacity(messages.len());
    let mut index = 0usize;

    while index < messages.len() {
        let message = &messages[index];
        match &message.payload {
            CoreConversationPayload::UserText { text, attachments } => {
                projected.push(CoreConversationMessage {
                    id: message.id.clone(),
                    payload: normalized_user_text_payload(
                        text,
                        attachments,
                        api_family,
                        capabilities,
                    ),
                });
                index += 1;
            }
            CoreConversationPayload::Assistant {
                text,
                reasoning,
                provider_message_id,
                provider_message_state,
                usage,
                tool_calls,
            } => {
                if is_legacy_explore_assistant_projection(
                    text,
                    reasoning,
                    provider_message_id,
                    provider_message_state,
                    usage.as_ref(),
                    tool_calls,
                ) {
                    index += 1;
                    continue;
                }

                if tool_calls.is_empty() {
                    projected.push(CoreConversationMessage {
                        id: message.id.clone(),
                        payload: CoreConversationPayload::Assistant {
                            text: text.clone(),
                            reasoning: String::new(),
                            provider_message_id: provider_message_id.clone(),
                            provider_message_state: provider_message_state.clone(),
                            usage: usage.clone(),
                            tool_calls: Vec::new(),
                        },
                    });
                    index += 1;
                    continue;
                }

                let (result_end, completed_tool_call_ids) =
                    consecutive_tool_result_span(messages, index + 1);
                if result_end == index + 1 {
                    projected.push(CoreConversationMessage {
                        id: message.id.clone(),
                        payload: CoreConversationPayload::UserText {
                            text: partial_tool_batch_text(text, reasoning, tool_calls, &[]),
                            attachments: Vec::new(),
                        },
                    });
                } else if tool_calls_are_fully_completed(tool_calls, &completed_tool_call_ids) {
                    let replayable_tool_call_ids = replayable_tool_call_ids(tool_calls);
                    {
                        projected.push(CoreConversationMessage {
                            id: message.id.clone(),
                            payload: CoreConversationPayload::Assistant {
                                text: text.clone(),
                                reasoning: String::new(),
                                provider_message_id: provider_message_id.clone(),
                                provider_message_state: provider_message_state.clone(),
                                usage: usage.clone(),
                                tool_calls: tool_calls.clone(),
                            },
                        });
                        let mut active_tool_call_ids = replayable_tool_call_ids;
                        for result_message in &messages[index + 1..result_end] {
                            projected.push(project_tool_result_message(
                                result_message,
                                &mut active_tool_call_ids,
                            ));
                        }
                    }
                } else {
                    projected.push(CoreConversationMessage {
                        id: message.id.clone(),
                        payload: CoreConversationPayload::UserText {
                            text: partial_tool_batch_text(
                                text,
                                reasoning,
                                tool_calls,
                                &messages[index + 1..result_end],
                            ),
                            attachments: Vec::new(),
                        },
                    });
                }
                index = result_end;
            }
            CoreConversationPayload::ToolResult { .. } => {
                let mut active_tool_call_ids = BTreeSet::new();
                projected.push(project_tool_result_message(
                    message,
                    &mut active_tool_call_ids,
                ));
                index += 1;
            }
        }
    }

    projected
}

/// Applies lowered runtime system updates to the provider-visible history in chronological anchor order.
/// 按时间锚点顺序把已转换的 runtime system update 应用到 Provider 可见历史。
fn apply_provider_visible_system_updates(
    projected: &mut Vec<CoreConversationMessage>,
    system_updates: &[RuntimeSystemUpdateEvent],
    policy: ProviderSystemUpdateLoweringPolicy,
) -> Result<(), String> {
    for update in system_updates {
        let text = match lower_provider_system_update(update, policy) {
            ProviderSystemUpdateLowering::UserWrapper { text } => text,
            ProviderSystemUpdateLowering::NativeSystem { .. } => {
                return Err(format!(
                    "runtime system update `{}` requires native chronological system support that the canonical provider-visible bridge cannot represent",
                    update.update_id
                ));
            }
            ProviderSystemUpdateLowering::Reject { reason } => {
                return Err(format!(
                    "runtime system update `{}` rejected during provider lowering: {reason}",
                    update.update_id
                ));
            }
        };
        let message = CoreConversationMessage {
            id: provider_visible_system_update_message_id(update),
            payload: CoreConversationPayload::UserText {
                text,
                attachments: Vec::new(),
            },
        };
        insert_provider_visible_system_update(projected, update, message);
    }
    validate_provider_visible_tool_topology(projected)
}

/// Inserts one lowered system-update message after its effective runtime anchor, or at the end when no anchor is present.
/// 在生效 runtime 锚点之后插入一条已转换 system-update 消息；没有锚点时插入末尾。
fn insert_provider_visible_system_update(
    projected: &mut Vec<CoreConversationMessage>,
    update: &RuntimeSystemUpdateEvent,
    message: CoreConversationMessage,
) {
    let insert_index = provider_visible_system_update_insert_index(projected, update);
    projected.insert(insert_index, message);
}

/// Resolves the safe provider-visible insertion index for one lowered system update.
/// 为一条已转换 system update 解析安全的 Provider 可见插入位置。
fn provider_visible_system_update_insert_index(
    projected: &[CoreConversationMessage],
    update: &RuntimeSystemUpdateEvent,
) -> usize {
    let insert_index = update
        .effective_after_event_id
        .as_deref()
        .and_then(|anchor| projected.iter().position(|message| message.id == anchor))
        .map(|index| index + 1)
        .unwrap_or(projected.len());
    advance_insert_index_past_tool_batch(projected, insert_index)
}

/// Moves an insertion point out of a live assistant tool-call batch when possible.
/// 在可能时将插入点移出正在进行的助手工具调用批次。
fn advance_insert_index_past_tool_batch(
    projected: &[CoreConversationMessage],
    insert_index: usize,
) -> usize {
    if insert_index == 0 || insert_index > projected.len() {
        return insert_index.min(projected.len());
    }
    let previous_index = insert_index - 1;
    match &projected[previous_index].payload {
        CoreConversationPayload::Assistant { tool_calls, .. } if !tool_calls.is_empty() => {
            completed_tool_batch_end_after_assistant(projected, previous_index, insert_index)
                .unwrap_or(insert_index)
        }
        CoreConversationPayload::ToolResult { .. } => {
            contiguous_tool_result_end(projected, insert_index)
        }
        _ => insert_index,
    }
}

/// Returns the first index after the complete tool-result batch for one assistant message.
/// 返回单条助手消息完整工具结果批次之后的第一个索引。
fn completed_tool_batch_end_after_assistant(
    projected: &[CoreConversationMessage],
    assistant_index: usize,
    result_start_index: usize,
) -> Option<usize> {
    let CoreConversationPayload::Assistant { tool_calls, .. } =
        &projected.get(assistant_index)?.payload
    else {
        return None;
    };
    let mut pending_tool_call_groups = tool_call_identifier_groups(tool_calls);
    if pending_tool_call_groups.is_empty() {
        return None;
    }
    let mut cursor = result_start_index;
    while cursor < projected.len() && !pending_tool_call_groups.is_empty() {
        let CoreConversationPayload::ToolResult { tool_call_id, .. } = &projected[cursor].payload
        else {
            break;
        };
        let Some(tool_call_key) = tool_call_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            break;
        };
        let Some(group_index) = pending_tool_call_groups.iter().position(|identifiers| {
            identifiers
                .iter()
                .any(|identifier| identifier == tool_call_key)
        }) else {
            break;
        };
        pending_tool_call_groups.remove(group_index);
        cursor += 1;
    }
    pending_tool_call_groups.is_empty().then_some(cursor)
}

/// Returns the first index after the contiguous tool-result run starting at the insertion point.
/// 返回从插入点开始的连续工具结果序列之后的第一个索引。
fn contiguous_tool_result_end(
    projected: &[CoreConversationMessage],
    mut insert_index: usize,
) -> usize {
    while insert_index < projected.len()
        && matches!(
            projected[insert_index].payload,
            CoreConversationPayload::ToolResult { .. }
        )
    {
        insert_index += 1;
    }
    insert_index
}

/// Builds a stable synthetic message id for one provider-visible system-update wrapper.
/// 为一条 Provider 可见 system-update 包装消息构建稳定的合成消息标识。
fn provider_visible_system_update_message_id(update: &RuntimeSystemUpdateEvent) -> String {
    format!("runtime-system-update:{}", update.update_id)
}

/// Validates that provider-visible tool calls and tool results form a strict replayable topology.
/// 校验 Provider 可见工具调用与工具结果是否形成严格可回放的拓扑。
pub(crate) fn validate_provider_visible_tool_topology(
    messages: &[CoreConversationMessage],
) -> Result<(), String> {
    let mut pending_tool_call_groups = Vec::<Vec<String>>::new();
    let mut pending_assistant_index = None::<usize>;
    for (index, message) in messages.iter().enumerate() {
        match &message.payload {
            CoreConversationPayload::Assistant { tool_calls, .. } if !tool_calls.is_empty() => {
                if !pending_tool_call_groups.is_empty() {
                    return Err(format!(
                        "assistant message at index {index} started a new tool batch before index {} completed",
                        pending_assistant_index.unwrap_or(index)
                    ));
                }
                pending_tool_call_groups = tool_call_identifier_groups(tool_calls);
                pending_assistant_index = Some(index);
                if pending_tool_call_groups.is_empty() {
                    return Err(format!(
                        "assistant message at index {index} contains tool calls without replayable identifiers"
                    ));
                }
            }
            CoreConversationPayload::Assistant { .. }
            | CoreConversationPayload::UserText { .. } => {
                if !pending_tool_call_groups.is_empty() {
                    return Err(format!(
                        "message at index {index} interrupted assistant tool batch from index {}",
                        pending_assistant_index.unwrap_or(index)
                    ));
                }
            }
            CoreConversationPayload::ToolResult { tool_call_id, .. } => {
                let Some(tool_call_key) = tool_call_id
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                else {
                    return Err(format!(
                        "tool result at index {index} does not carry a replayable tool_call_id"
                    ));
                };
                let Some(group_index) = pending_tool_call_groups.iter().position(|identifiers| {
                    identifiers
                        .iter()
                        .any(|identifier| identifier == tool_call_key)
                }) else {
                    return Err(format!(
                        "tool result at index {index} references unmatched tool call `{tool_call_key}`"
                    ));
                };
                pending_tool_call_groups.remove(group_index);
                if pending_tool_call_groups.is_empty() {
                    pending_assistant_index = None;
                }
            }
        }
    }
    if !pending_tool_call_groups.is_empty() {
        return Err(format!(
            "assistant tool batch from index {} ended without all tool results",
            pending_assistant_index.unwrap_or(0)
        ));
    }
    Ok(())
}

/// Detects legacy hidden Explore records that were accidentally mirrored as assistant messages.
/// 识别曾被错误镜像成助手消息的旧版隐藏 Explore 记录。
fn is_legacy_explore_assistant_projection(
    text: &str,
    reasoning: &str,
    provider_message_id: &Option<String>,
    provider_message_state: &Option<serde_json::Value>,
    usage: Option<&vulcan_core::types::ProviderUsageSnapshot>,
    tool_calls: &[CoreConversationToolCall],
) -> bool {
    if !reasoning.trim().is_empty()
        || provider_message_id.is_some()
        || provider_message_state.is_some()
        || usage.is_some()
        || !tool_calls.is_empty()
    {
        return false;
    }

    let trimmed = text.trim_start();
    trimmed.starts_with("This session is bound to project `")
        || trimmed.starts_with("This session is bound to project id `")
        || trimmed.starts_with("This session is not bound to an explicit project directory.")
}

/// Normalizes one user-text payload for the selected provider API family and model capabilities.
/// 根据所选 Provider API 族与模型能力规范化一条用户文本载荷。
fn normalized_user_text_payload(
    text: &str,
    attachments: &[vulcan_core::types::ConversationAttachment],
    api_family: Option<ProviderApiFamily>,
    capabilities: Option<&ResolvedModelCapabilities>,
) -> CoreConversationPayload {
    match api_family {
        Some(api_family) => {
            let normalized = normalize_provider_user_input_for_model(
                api_family,
                capabilities,
                text,
                attachments,
            );
            CoreConversationPayload::UserText {
                text: normalized.text,
                attachments: normalized.attachments,
            }
        }
        None => CoreConversationPayload::UserText {
            text: text.to_string(),
            attachments: attachments.to_vec(),
        },
    }
}

/// Returns the consecutive tool-result span after one assistant message and the completed tool-call ids inside that span.
/// 返回某条助手消息后的连续工具结果区间，以及该区间内已完成的工具调用标识集合。
fn consecutive_tool_result_span(
    messages: &[CoreConversationMessage],
    start: usize,
) -> (usize, BTreeSet<String>) {
    let mut completed_tool_call_ids = BTreeSet::new();
    let mut index = start;
    while let Some(message) = messages.get(index) {
        let CoreConversationPayload::ToolResult { tool_call_id, .. } = &message.payload else {
            break;
        };
        if let Some(tool_call_id) = tool_call_id.as_deref().map(str::trim)
            && !tool_call_id.is_empty()
        {
            completed_tool_call_ids.insert(tool_call_id.to_string());
        }
        index += 1;
    }
    (index, completed_tool_call_ids)
}

/// Projects one tool-result message either as a native provider tool response or as plain fallback text.
/// 将一条工具结果消息投影为原生 Provider 工具响应，或投影为普通文本回退。
fn project_tool_result_message(
    message: &CoreConversationMessage,
    active_tool_call_ids: &mut BTreeSet<String>,
) -> CoreConversationMessage {
    let CoreConversationPayload::ToolResult {
        tool_result_id,
        tool_call_id,
        text,
    } = &message.payload
    else {
        return message.clone();
    };
    let tool_call_key = tool_call_id.as_deref().unwrap_or_default().trim();
    let payload = if !tool_call_key.is_empty() && active_tool_call_ids.remove(tool_call_key) {
        CoreConversationPayload::ToolResult {
            tool_result_id: tool_result_id.clone(),
            tool_call_id: tool_call_id.clone(),
            text: text.clone(),
        }
    } else {
        CoreConversationPayload::UserText {
            text: orphan_tool_result_text(tool_result_id, tool_call_id, text),
            attachments: Vec::new(),
        }
    };
    CoreConversationMessage {
        id: message.id.clone(),
        payload,
    }
}

/// Builds all provider-replayable identifiers for one assistant tool-call batch.
/// 为一批助手工具调用构建所有可回放到 Provider 的标识。
fn replayable_tool_call_ids(tool_calls: &[CoreConversationToolCall]) -> BTreeSet<String> {
    tool_calls
        .iter()
        .flat_map(|tool_call| {
            [
                (!tool_call.id.trim().is_empty()).then(|| tool_call.id.clone()),
                tool_call
                    .tool_call_id
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string),
            ]
        })
        .flatten()
        .collect()
}

/// Returns whether each tool call in one assistant batch has at least one completed result id.
/// 判断某个助手工具调用批次中的每个调用是否至少有一个已完成结果标识。
fn tool_calls_are_fully_completed(
    tool_calls: &[CoreConversationToolCall],
    completed_tool_call_ids: &BTreeSet<String>,
) -> bool {
    tool_calls.iter().all(|tool_call| {
        tool_call_identifiers(tool_call)
            .iter()
            .any(|identifier| completed_tool_call_ids.contains(identifier))
    })
}

/// Builds the provider and local identifiers that may match one tool result.
/// 构建一个工具结果可能匹配到的 Provider 与本地标识。
fn tool_call_identifiers(tool_call: &CoreConversationToolCall) -> Vec<String> {
    [
        (!tool_call.id.trim().is_empty()).then(|| tool_call.id.clone()),
        tool_call
            .tool_call_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// Builds grouped replay identifiers so local and provider ids complete the same tool call.
/// 构建分组后的可回放标识，使本地 id 与 Provider id 完成同一个工具调用。
fn tool_call_identifier_groups(tool_calls: &[CoreConversationToolCall]) -> Vec<Vec<String>> {
    tool_calls
        .iter()
        .map(tool_call_identifiers)
        .filter(|identifiers| !identifiers.is_empty())
        .collect()
}

/// Builds a provider-visible plain-text fallback for a tool result whose matching tool call was compacted away.
/// 为匹配工具调用已被压缩移除的工具结果构建 Provider 可见的纯文本回退内容。
fn orphan_tool_result_text(
    tool_result_id: &str,
    tool_call_id: &Option<String>,
    text: &str,
) -> String {
    let call_label = tool_call_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("unknown");
    format!(
        "ERROR: Tool result `{}` cannot be replayed as a tool response because matching assistant tool call `{}` is no longer present in context. Treat the following as historical text only.\n\n{}",
        tool_result_id, call_label, text
    )
}

/// Builds plain provider-visible history text for a partially completed assistant tool-call batch.
/// 为一批未完整完成的助手工具调用构建 Provider 可见的普通历史文本。
fn partial_tool_batch_text(
    text: &str,
    reasoning: &str,
    tool_calls: &[CoreConversationToolCall],
    result_messages: &[CoreConversationMessage],
) -> String {
    let mut lines = Vec::new();
    lines.push(String::from(
        "NOTE: A previous assistant tool-call batch was interrupted before every requested tool returned. Treat this block as historical text only, not as live tool protocol state.",
    ));
    if !reasoning.trim().is_empty() {
        lines.push(format!("Assistant reasoning: {}", reasoning.trim()));
    }
    if !text.trim().is_empty() {
        lines.push(format!("Assistant text: {}", text.trim()));
    }
    lines.push(String::from("Requested tools:"));
    for tool_call in tool_calls {
        let call_label = tool_call
            .tool_call_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| tool_call.id.trim());
        lines.push(format!("- {} ({})", tool_call.tool_name, call_label));
    }
    if !result_messages.is_empty() {
        lines.push(String::from("Completed tool results:"));
        for result_message in result_messages {
            if let CoreConversationPayload::ToolResult {
                tool_result_id,
                tool_call_id,
                text,
            } = &result_message.payload
            {
                let call_label = tool_call_id
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or("unknown");
                lines.push(format!(
                    "- result={} call={}: {}",
                    tool_result_id,
                    call_label,
                    text.trim()
                ));
            }
        }
    }
    lines.join("\n")
}

/// Converts one session-owned canonical tool-call entry into the `vulcan-core` tool-call shape.
/// 将一条会话持有的规范工具调用项转换为 `vulcan-core` 工具调用形状。
fn session_tool_call_to_core_tool_call(
    tool_call: &SessionConversationToolCall,
) -> CoreConversationToolCall {
    CoreConversationToolCall {
        id: tool_call.id.clone(),
        tool_call_id: tool_call.tool_call_id.clone(),
        tool_name: tool_call.tool_name.clone(),
        arguments: tool_call.arguments.clone(),
    }
}

/// Converts one `vulcan-core` tool-call entry back into the GUI-owned canonical tool-call shape.
/// 将一条 `vulcan-core` 工具调用项转换回 GUI 持有的规范工具调用形状。
fn core_tool_call_to_session_tool_call(
    tool_call: &CoreConversationToolCall,
) -> SessionConversationToolCall {
    SessionConversationToolCall {
        id: tool_call.id.clone(),
        tool_call_id: tool_call.tool_call_id.clone(),
        tool_name: tool_call.tool_name.clone(),
        arguments: tool_call.arguments.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        core_messages_to_provider_visible_history_for_model,
        core_messages_to_provider_visible_history_for_model_with_system_updates,
        validate_provider_visible_tool_topology,
    };
    use crate::agent_runtime::provider_system_update_lowering::ProviderSystemUpdateLoweringPolicy;
    use crate::provider_capabilities::ResolvedModelCapabilities;
    use crate::provider_config::ProviderApiFamily;
    use vulcan_context::{
        RuntimeSystemUpdateEvent, RuntimeSystemUpdateLoweringHint, RuntimeSystemUpdateTrust,
    };
    use vulcan_core::types::{
        ConversationAttachment, ConversationAttachmentKind,
        ConversationMessage as CoreConversationMessage,
        ConversationPayload as CoreConversationPayload,
        ConversationToolCall as CoreConversationToolCall,
    };

    /// Builds one replayable attachment payload used by provider-history projection tests.
    /// 构建供 Provider 历史投影测试使用的可回放附件载荷。
    fn replayable_attachment(
        kind: ConversationAttachmentKind,
        file_name: &str,
        mime_type: &str,
    ) -> ConversationAttachment {
        ConversationAttachment {
            kind,
            file_name: file_name.to_string(),
            local_path: std::env::current_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from("."))
                .join("Cargo.toml")
                .to_string_lossy()
                .to_string(),
            source_path: None,
            asset_id: Some(format!("asset-{file_name}")),
            byte_len: Some(4),
            preview_local_path: None,
            mime_type: mime_type.to_string(),
        }
    }

    /// Builds one runtime system update used by provider-visible projection tests.
    /// 构建一条供 Provider 可见投影测试使用的 runtime system update。
    fn runtime_system_update(
        update_id: &str,
        effective_after_event_id: Option<&str>,
        hint: RuntimeSystemUpdateLoweringHint,
    ) -> RuntimeSystemUpdateEvent {
        RuntimeSystemUpdateEvent {
            turn_id: Some(String::from("turn-1")),
            update_id: update_id.to_string(),
            effective_after_event_id: effective_after_event_id.map(str::to_string),
            text: String::from("Use the compacted project rules from now on."),
            trust: RuntimeSystemUpdateTrust::TrustedHost,
            lowering_hint: hint,
            fingerprint: Some(format!("fp-{update_id}")),
            source: Some(String::from("context_compaction")),
        }
    }

    /// Verifies chronological system updates lower to user-scoped wrapper messages when the provider bridge has no native system-message slot.
    /// 验证当 Provider 桥接层没有原生 system 消息槽位时，时间顺序 system update 会转换为用户作用域包装消息。
    #[test]
    fn provider_visible_history_applies_system_update_as_user_wrapper() {
        let messages = vec![CoreConversationMessage {
            id: String::from("user-1"),
            payload: CoreConversationPayload::UserText {
                text: String::from("继续"),
                attachments: Vec::new(),
            },
        }];
        let updates = vec![runtime_system_update(
            "system-update-1",
            Some("user-1"),
            RuntimeSystemUpdateLoweringHint::Auto,
        )];

        let projected = core_messages_to_provider_visible_history_for_model_with_system_updates(
            &messages,
            Some(ProviderApiFamily::OpenAiChatCompatible),
            None,
            &updates,
            ProviderSystemUpdateLoweringPolicy::default(),
        )
        .expect("system update wrapper should project");

        assert_eq!(projected.len(), 2);
        assert_eq!(projected[1].id, "runtime-system-update:system-update-1");
        let CoreConversationPayload::UserText { text, attachments } = &projected[1].payload else {
            panic!("system update should become user-scoped wrapper text");
        };
        assert!(attachments.is_empty());
        assert!(text.contains("user scoped"));
        assert!(text.contains("system-update-1"));
        assert!(text.contains("compacted project rules"));
    }

    /// Verifies system updates anchored after an assistant tool call are moved after the matching tool result.
    /// 验证锚定在助手工具调用之后的 system update 会移动到匹配工具结果之后。
    #[test]
    fn provider_visible_history_moves_system_update_after_tool_result_gap() {
        let messages = vec![
            CoreConversationMessage {
                id: String::from("assistant-1"),
                payload: CoreConversationPayload::Assistant {
                    text: String::new(),
                    reasoning: String::new(),
                    provider_message_id: None,
                    provider_message_state: None,
                    usage: None,
                    tool_calls: vec![CoreConversationToolCall {
                        id: String::from("local-call-1"),
                        tool_call_id: Some(String::from("provider-call-1")),
                        tool_name: String::from("read_file"),
                        arguments: serde_json::json!({ "path": "Cargo.toml" }),
                    }],
                },
            },
            CoreConversationMessage {
                id: String::from("tool-result-1"),
                payload: CoreConversationPayload::ToolResult {
                    tool_result_id: String::from("result-1"),
                    tool_call_id: Some(String::from("provider-call-1")),
                    text: String::from("file body"),
                },
            },
        ];
        let updates = vec![runtime_system_update(
            "system-update-gap",
            Some("assistant-1"),
            RuntimeSystemUpdateLoweringHint::Auto,
        )];

        let projected = core_messages_to_provider_visible_history_for_model_with_system_updates(
            &messages,
            Some(ProviderApiFamily::OpenAiChatCompatible),
            None,
            &updates,
            ProviderSystemUpdateLoweringPolicy::default(),
        )
        .expect("system update in a tool-result gap should be moved");

        assert_eq!(projected.len(), 3);
        assert_eq!(projected[0].id, "assistant-1");
        assert_eq!(projected[1].id, "tool-result-1");
        assert_eq!(projected[2].id, "runtime-system-update:system-update-gap");
        validate_provider_visible_tool_topology(&projected).expect("valid provider topology");
    }

    /// Verifies system updates anchored on one result inside a parallel tool batch move after every contiguous result.
    /// 验证锚定在并行工具批次某个结果上的 system update 会移动到所有连续结果之后。
    #[test]
    fn provider_visible_history_moves_system_update_after_parallel_tool_results() {
        let messages = vec![
            CoreConversationMessage {
                id: String::from("assistant-1"),
                payload: CoreConversationPayload::Assistant {
                    text: String::new(),
                    reasoning: String::new(),
                    provider_message_id: None,
                    provider_message_state: None,
                    usage: None,
                    tool_calls: vec![
                        CoreConversationToolCall {
                            id: String::from("local-call-1"),
                            tool_call_id: Some(String::from("provider-call-1")),
                            tool_name: String::from("read_file"),
                            arguments: serde_json::json!({ "path": "Cargo.toml" }),
                        },
                        CoreConversationToolCall {
                            id: String::from("local-call-2"),
                            tool_call_id: Some(String::from("provider-call-2")),
                            tool_name: String::from("list_files"),
                            arguments: serde_json::json!({}),
                        },
                    ],
                },
            },
            CoreConversationMessage {
                id: String::from("tool-result-1"),
                payload: CoreConversationPayload::ToolResult {
                    tool_result_id: String::from("result-1"),
                    tool_call_id: Some(String::from("provider-call-1")),
                    text: String::from("file body"),
                },
            },
            CoreConversationMessage {
                id: String::from("tool-result-2"),
                payload: CoreConversationPayload::ToolResult {
                    tool_result_id: String::from("result-2"),
                    tool_call_id: Some(String::from("provider-call-2")),
                    text: String::from("file list"),
                },
            },
        ];
        let updates = vec![runtime_system_update(
            "system-update-parallel-gap",
            Some("tool-result-1"),
            RuntimeSystemUpdateLoweringHint::Auto,
        )];

        let projected = core_messages_to_provider_visible_history_for_model_with_system_updates(
            &messages,
            Some(ProviderApiFamily::OpenAiChatCompatible),
            None,
            &updates,
            ProviderSystemUpdateLoweringPolicy::default(),
        )
        .expect("system update in a parallel tool-result gap should be moved");

        assert_eq!(projected.len(), 4);
        assert_eq!(projected[0].id, "assistant-1");
        assert_eq!(projected[1].id, "tool-result-1");
        assert_eq!(projected[2].id, "tool-result-2");
        assert_eq!(
            projected[3].id,
            "runtime-system-update:system-update-parallel-gap"
        );
        validate_provider_visible_tool_topology(&projected).expect("valid provider topology");
    }

    /// Verifies historical attachments are downgraded before transport encoding when the selected model is text-only.
    /// 验证历史附件会在传输编码前根据纯文本模型能力降级。
    #[test]
    fn provider_visible_history_downgrades_model_unsupported_attachments() {
        let capabilities = ResolvedModelCapabilities {
            input_modalities: vec![String::from("text")],
            ..ResolvedModelCapabilities::default()
        };
        let messages = vec![CoreConversationMessage {
            id: String::from("user-1"),
            payload: CoreConversationPayload::UserText {
                text: String::from("继续分析"),
                attachments: vec![replayable_attachment(
                    ConversationAttachmentKind::Image,
                    "demo.png",
                    "image/png",
                )],
            },
        }];

        let projected = core_messages_to_provider_visible_history_for_model(
            &messages,
            Some(ProviderApiFamily::OpenAiChatCompatible),
            Some(&capabilities),
        );

        let CoreConversationPayload::UserText { text, attachments } = &projected[0].payload else {
            panic!("expected projected user text");
        };
        assert!(attachments.is_empty());
        assert!(text.contains("selected model"));
    }

    /// Verifies legacy project-scope Explore records are removed from provider-visible history.
    /// 验证旧版项目范围 Explore 记录会从 Provider 可见历史中移除。
    #[test]
    fn provider_visible_history_filters_legacy_explore_assistant_projection() {
        let messages = vec![
            CoreConversationMessage {
                id: String::from("user-1"),
                payload: CoreConversationPayload::UserText {
                    text: String::from("介绍下你自己"),
                    attachments: Vec::new(),
                },
            },
            CoreConversationMessage {
                id: String::from("explore-1"),
                payload: CoreConversationPayload::Assistant {
                    text: String::from(
                        "This session is bound to project `test-project` with root directory `D:\\openvulcan\\test-project`. Future explore and triage steps should prioritize that scope.",
                    ),
                    reasoning: String::new(),
                    provider_message_id: None,
                    provider_message_state: None,
                    usage: None,
                    tool_calls: Vec::new(),
                },
            },
            CoreConversationMessage {
                id: String::from("user-2"),
                payload: CoreConversationPayload::UserText {
                    text: String::from("继续"),
                    attachments: Vec::new(),
                },
            },
        ];

        let projected = core_messages_to_provider_visible_history_for_model(
            &messages,
            Some(ProviderApiFamily::OpenAiChatCompatible),
            None,
        );

        assert_eq!(projected.len(), 2);
        assert!(projected.iter().all(|message| message.id != "explore-1"));
        assert!(matches!(
            &projected[0].payload,
            CoreConversationPayload::UserText { text, .. } if text == "介绍下你自己"
        ));
        assert!(matches!(
            &projected[1].payload,
            CoreConversationPayload::UserText { text, .. } if text == "继续"
        ));
    }

    /// Verifies matched tool results remain protocol tool responses in provider-visible history.
    /// 验证已匹配的工具结果在 Provider 可见历史中仍保持协议工具响应形态。
    #[test]
    fn provider_visible_history_preserves_matched_tool_result() {
        let messages = vec![
            CoreConversationMessage {
                id: String::from("assistant-1"),
                payload: CoreConversationPayload::Assistant {
                    text: String::new(),
                    reasoning: String::from("local reasoning"),
                    provider_message_id: None,
                    provider_message_state: None,
                    usage: None,
                    tool_calls: vec![CoreConversationToolCall {
                        id: String::from("local-call"),
                        tool_call_id: Some(String::from("provider-call")),
                        tool_name: String::from("read_file"),
                        arguments: serde_json::json!({"path": "Cargo.toml"}),
                    }],
                },
            },
            CoreConversationMessage {
                id: String::from("tool-result-1"),
                payload: CoreConversationPayload::ToolResult {
                    tool_result_id: String::from("result-1"),
                    tool_call_id: Some(String::from("provider-call")),
                    text: String::from("file body"),
                },
            },
        ];

        let projected = core_messages_to_provider_visible_history_for_model(
            &messages,
            Some(ProviderApiFamily::OpenAiChatCompatible),
            None,
        );

        assert!(matches!(
            &projected[1].payload,
            CoreConversationPayload::ToolResult { text, .. } if text == "file body"
        ));
    }

    /// Verifies projected provider history satisfies the canonical tool topology validator.
    /// 验证投影后的 Provider 历史满足规范工具拓扑校验器。
    #[test]
    fn provider_visible_history_passes_canonical_tool_topology_validator() {
        let messages = vec![
            CoreConversationMessage {
                id: String::from("assistant-1"),
                payload: CoreConversationPayload::Assistant {
                    text: String::new(),
                    reasoning: String::new(),
                    provider_message_id: None,
                    provider_message_state: None,
                    usage: None,
                    tool_calls: vec![
                        CoreConversationToolCall {
                            id: String::from("local-call-1"),
                            tool_call_id: Some(String::from("provider-call-1")),
                            tool_name: String::from("first_tool"),
                            arguments: serde_json::json!({}),
                        },
                        CoreConversationToolCall {
                            id: String::from("local-call-2"),
                            tool_call_id: Some(String::from("provider-call-2")),
                            tool_name: String::from("second_tool"),
                            arguments: serde_json::json!({}),
                        },
                    ],
                },
            },
            CoreConversationMessage {
                id: String::from("tool-result-1"),
                payload: CoreConversationPayload::ToolResult {
                    tool_result_id: String::from("result-1"),
                    tool_call_id: Some(String::from("provider-call-1")),
                    text: String::from("first done"),
                },
            },
            CoreConversationMessage {
                id: String::from("tool-result-2"),
                payload: CoreConversationPayload::ToolResult {
                    tool_result_id: String::from("result-2"),
                    tool_call_id: Some(String::from("local-call-2")),
                    text: String::from("second done"),
                },
            },
        ];

        let projected = core_messages_to_provider_visible_history_for_model(
            &messages,
            Some(ProviderApiFamily::OpenAiChatCompatible),
            None,
        );

        validate_provider_visible_tool_topology(&projected).expect("valid provider topology");
    }

    /// Verifies the canonical tool topology validator rejects interrupted live tool batches.
    /// 验证规范工具拓扑校验器会拒绝被打断的实时工具批次。
    #[test]
    fn provider_visible_tool_topology_validator_rejects_interrupted_batch() {
        let messages = vec![
            CoreConversationMessage {
                id: String::from("assistant-1"),
                payload: CoreConversationPayload::Assistant {
                    text: String::new(),
                    reasoning: String::new(),
                    provider_message_id: None,
                    provider_message_state: None,
                    usage: None,
                    tool_calls: vec![CoreConversationToolCall {
                        id: String::from("local-call-1"),
                        tool_call_id: Some(String::from("provider-call-1")),
                        tool_name: String::from("ask"),
                        arguments: serde_json::json!({}),
                    }],
                },
            },
            CoreConversationMessage {
                id: String::from("user-1"),
                payload: CoreConversationPayload::UserText {
                    text: String::from("new prompt"),
                    attachments: Vec::new(),
                },
            },
        ];

        let error = validate_provider_visible_tool_topology(&messages)
            .expect_err("interrupted batch should be rejected");
        assert!(error.contains("interrupted assistant tool batch"));
    }

    /// Verifies the canonical tool topology validator rejects orphan tool results.
    /// 验证规范工具拓扑校验器会拒绝孤立工具结果。
    #[test]
    fn provider_visible_tool_topology_validator_rejects_orphan_result() {
        let messages = vec![CoreConversationMessage {
            id: String::from("tool-result-1"),
            payload: CoreConversationPayload::ToolResult {
                tool_result_id: String::from("result-1"),
                tool_call_id: Some(String::from("missing-call")),
                text: String::from("orphan body"),
            },
        }];

        let error = validate_provider_visible_tool_topology(&messages)
            .expect_err("orphan result should be rejected");
        assert!(error.contains("unmatched tool call"));
    }

    /// Verifies orphaned tool results are downgraded to plain user-visible text before provider transport.
    /// 验证孤立工具结果会在 Provider 传输前降级为普通用户可见文本。
    #[test]
    fn provider_visible_history_downgrades_orphan_tool_result() {
        let messages = vec![CoreConversationMessage {
            id: String::from("tool-result-1"),
            payload: CoreConversationPayload::ToolResult {
                tool_result_id: String::from("result-1"),
                tool_call_id: Some(String::from("missing-call")),
                text: String::from("orphan body"),
            },
        }];

        let projected = core_messages_to_provider_visible_history_for_model(
            &messages,
            Some(ProviderApiFamily::OpenAiChatCompatible),
            None,
        );

        let CoreConversationPayload::UserText { text, attachments } = &projected[0].payload else {
            panic!("expected orphan tool result to become plain user text");
        };
        assert!(attachments.is_empty());
        assert!(text.contains("cannot be replayed"));
        assert!(text.contains("orphan body"));
    }

    /// Verifies partially completed parallel tool batches are downgraded before provider replay.
    /// 验证部分完成的并行工具批次会在 Provider 回放前降级。
    #[test]
    fn provider_visible_history_downgrades_partial_tool_batch() {
        let messages = vec![
            CoreConversationMessage {
                id: String::from("assistant-1"),
                payload: CoreConversationPayload::Assistant {
                    text: String::new(),
                    reasoning: String::from("call two tools"),
                    provider_message_id: Some(String::from("resp-1")),
                    provider_message_state: None,
                    usage: None,
                    tool_calls: vec![
                        CoreConversationToolCall {
                            id: String::from("local-call-1"),
                            tool_call_id: Some(String::from("provider-call-1")),
                            tool_name: String::from("first_tool"),
                            arguments: serde_json::json!({"step": 1}),
                        },
                        CoreConversationToolCall {
                            id: String::from("local-call-2"),
                            tool_call_id: Some(String::from("provider-call-2")),
                            tool_name: String::from("second_tool"),
                            arguments: serde_json::json!({"step": 2}),
                        },
                    ],
                },
            },
            CoreConversationMessage {
                id: String::from("tool-result-1"),
                payload: CoreConversationPayload::ToolResult {
                    tool_result_id: String::from("result-1"),
                    tool_call_id: Some(String::from("provider-call-1")),
                    text: String::from("first done"),
                },
            },
        ];

        let projected = core_messages_to_provider_visible_history_for_model(
            &messages,
            Some(ProviderApiFamily::OpenAiChatCompatible),
            None,
        );

        assert_eq!(projected.len(), 1);
        let CoreConversationPayload::UserText { text, attachments } = &projected[0].payload else {
            panic!("partial tool batch should downgrade to text");
        };
        assert!(attachments.is_empty());
        assert!(text.contains("interrupted before every requested tool returned"));
        assert!(text.contains("first_tool"));
        assert!(text.contains("second_tool"));
        assert!(text.contains("first done"));
    }

    /// Verifies interrupted tool batches with no results are never replayed as live provider tool calls.
    /// 验证没有任何结果的中断工具批次绝不会作为实时 Provider 工具调用回放。
    #[test]
    fn provider_visible_history_downgrades_unanswered_tool_batch() {
        let messages = vec![CoreConversationMessage {
            id: String::from("assistant-1"),
            payload: CoreConversationPayload::Assistant {
                text: String::from("I need to ask the user."),
                reasoning: String::from("call ask"),
                provider_message_id: Some(String::from("resp-1")),
                provider_message_state: None,
                usage: None,
                tool_calls: vec![CoreConversationToolCall {
                    id: String::from("local-call-1"),
                    tool_call_id: Some(String::from("provider-call-1")),
                    tool_name: String::from("ask"),
                    arguments: serde_json::json!({
                        "questions": [{
                            "header": "Choice",
                            "question": "Pick one?",
                            "options": []
                        }]
                    }),
                }],
            },
        }];

        let projected = core_messages_to_provider_visible_history_for_model(
            &messages,
            Some(ProviderApiFamily::OpenAiChatCompatible),
            None,
        );

        assert_eq!(projected.len(), 1);
        let CoreConversationPayload::UserText { text, attachments } = &projected[0].payload else {
            panic!("unanswered tool batch should downgrade to text");
        };
        assert!(attachments.is_empty());
        assert!(text.contains("interrupted before every requested tool returned"));
        assert!(text.contains("ask"));
        assert!(text.contains("provider-call-1"));
    }

    /// Verifies the plain-text canonical fixture keeps user and assistant turns in deterministic order.
    /// 验证普通文本 canonical fixture 会以确定顺序保留用户与助手回合。
    #[test]
    fn canonical_fixture_plain_text_history_projects_stably() {
        let messages = vec![
            CoreConversationMessage {
                id: String::from("user-1"),
                payload: CoreConversationPayload::UserText {
                    text: String::from("你好"),
                    attachments: Vec::new(),
                },
            },
            CoreConversationMessage {
                id: String::from("assistant-1"),
                payload: CoreConversationPayload::Assistant {
                    text: String::from("你好，我在。"),
                    reasoning: String::from("internal reasoning should be stripped"),
                    provider_message_id: Some(String::from("provider-1")),
                    provider_message_state: None,
                    usage: None,
                    tool_calls: Vec::new(),
                },
            },
            CoreConversationMessage {
                id: String::from("user-2"),
                payload: CoreConversationPayload::UserText {
                    text: String::from("继续"),
                    attachments: Vec::new(),
                },
            },
        ];

        let projected = core_messages_to_provider_visible_history_for_model(
            &messages,
            Some(ProviderApiFamily::OpenAiChatCompatible),
            None,
        );

        assert_eq!(projected.len(), 3);
        assert_eq!(projected[0].id, "user-1");
        assert_eq!(projected[1].id, "assistant-1");
        assert_eq!(projected[2].id, "user-2");
        let CoreConversationPayload::Assistant {
            text,
            reasoning,
            tool_calls,
            ..
        } = &projected[1].payload
        else {
            panic!("second projected message should be assistant text");
        };
        assert_eq!(text, "你好，我在。");
        assert!(reasoning.is_empty());
        assert!(tool_calls.is_empty());
    }

    /// Verifies the tool-call canonical fixture keeps matched tool calls and results as live protocol state.
    /// 验证工具调用 canonical fixture 会把已匹配的工具调用与结果保留为实时协议状态。
    #[test]
    fn canonical_fixture_tool_call_and_result_projects_as_protocol_pair() {
        let messages = vec![
            CoreConversationMessage {
                id: String::from("assistant-1"),
                payload: CoreConversationPayload::Assistant {
                    text: String::from("读取文件。"),
                    reasoning: String::from("read first"),
                    provider_message_id: Some(String::from("provider-1")),
                    provider_message_state: None,
                    usage: None,
                    tool_calls: vec![CoreConversationToolCall {
                        id: String::from("local-call-1"),
                        tool_call_id: Some(String::from("provider-call-1")),
                        tool_name: String::from("read_file"),
                        arguments: serde_json::json!({ "path": "Cargo.toml" }),
                    }],
                },
            },
            CoreConversationMessage {
                id: String::from("tool-result-1"),
                payload: CoreConversationPayload::ToolResult {
                    tool_result_id: String::from("result-1"),
                    tool_call_id: Some(String::from("provider-call-1")),
                    text: String::from("file body"),
                },
            },
        ];

        let projected = core_messages_to_provider_visible_history_for_model(
            &messages,
            Some(ProviderApiFamily::OpenAiChatCompatible),
            None,
        );

        assert_eq!(projected.len(), 2);
        let CoreConversationPayload::Assistant { tool_calls, .. } = &projected[0].payload else {
            panic!("first projected message should keep assistant tool calls");
        };
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(
            tool_calls[0].tool_call_id.as_deref(),
            Some("provider-call-1")
        );
        let CoreConversationPayload::ToolResult {
            tool_call_id, text, ..
        } = &projected[1].payload
        else {
            panic!("second projected message should keep the matching tool result");
        };
        assert_eq!(tool_call_id.as_deref(), Some("provider-call-1"));
        assert_eq!(text, "file body");
        validate_provider_visible_tool_topology(&projected).expect("tool topology should be valid");
    }

    /// Verifies a provider-switch fixture degrades stale live tool state before the next user turn is projected.
    /// 验证 provider 切换 fixture 会在下一条用户回合投影前降级过期实时工具状态。
    #[test]
    fn canonical_fixture_provider_switch_downgrades_stale_tool_state() {
        let messages = vec![
            CoreConversationMessage {
                id: String::from("assistant-old-provider"),
                payload: CoreConversationPayload::Assistant {
                    text: String::from("旧 provider 准备调用工具。"),
                    reasoning: String::new(),
                    provider_message_id: Some(String::from("old-provider-message")),
                    provider_message_state: Some(serde_json::json!({
                        "protocol": "anthropic",
                        "payload": { "id": "old" }
                    })),
                    usage: None,
                    tool_calls: vec![CoreConversationToolCall {
                        id: String::from("local-old-call"),
                        tool_call_id: Some(String::from("old-provider-call")),
                        tool_name: String::from("image_generation"),
                        arguments: serde_json::json!({ "prompt": "demo" }),
                    }],
                },
            },
            CoreConversationMessage {
                id: String::from("user-after-switch"),
                payload: CoreConversationPayload::UserText {
                    text: String::from("切到 MiniMax 后继续"),
                    attachments: Vec::new(),
                },
            },
        ];

        let projected = core_messages_to_provider_visible_history_for_model(
            &messages,
            Some(ProviderApiFamily::OpenAiChatCompatible),
            None,
        );

        assert_eq!(projected.len(), 2);
        let CoreConversationPayload::UserText { text, attachments } = &projected[0].payload else {
            panic!("stale tool state should become historical user text");
        };
        assert!(attachments.is_empty());
        assert!(text.contains("historical text only"));
        assert!(text.contains("old-provider-call"));
        let CoreConversationPayload::UserText { text, .. } = &projected[1].payload else {
            panic!("latest user prompt should remain user text");
        };
        assert_eq!(text, "切到 MiniMax 后继续");
        validate_provider_visible_tool_topology(&projected)
            .expect("downgraded switch history should be topology-safe");
    }

    /// Verifies the injection fixture projects dynamic runtime overlays after the stable user baseline.
    /// 验证注入 fixture 会在稳定用户基线之后投影动态 runtime 覆盖。
    #[test]
    fn canonical_fixture_dynamic_injection_projects_as_system_update_wrapper() {
        let messages = vec![CoreConversationMessage {
            id: String::from("user-1"),
            payload: CoreConversationPayload::UserText {
                text: String::from("继续任务"),
                attachments: Vec::new(),
            },
        }];
        let updates = vec![runtime_system_update(
            "dynamic-injection-1",
            Some("user-1"),
            RuntimeSystemUpdateLoweringHint::Auto,
        )];

        let projected = core_messages_to_provider_visible_history_for_model_with_system_updates(
            &messages,
            Some(ProviderApiFamily::OpenAiChatCompatible),
            None,
            &updates,
            ProviderSystemUpdateLoweringPolicy::default(),
        )
        .expect("dynamic injection wrapper should project");

        assert_eq!(projected.len(), 2);
        assert_eq!(projected[0].id, "user-1");
        assert_eq!(projected[1].id, "runtime-system-update:dynamic-injection-1");
        let CoreConversationPayload::UserText { text, attachments } = &projected[1].payload else {
            panic!("dynamic injection should become user-scoped wrapper text");
        };
        assert!(attachments.is_empty());
        assert!(text.contains("dynamic-injection-1"));
        assert!(text.contains("user scoped"));
    }

    /// Verifies multimodal canonical fixtures preserve image and video attachments only when provider and model support them.
    /// 验证多模态 canonical fixture 只会在 provider 与模型均支持时保留图片和视频附件。
    #[test]
    fn canonical_fixture_image_and_video_understanding_respects_model_capabilities() {
        let capabilities = ResolvedModelCapabilities {
            input_modalities: vec![
                String::from("text"),
                String::from("image"),
                String::from("video"),
            ],
            ..ResolvedModelCapabilities::default()
        };
        let messages = vec![CoreConversationMessage {
            id: String::from("user-multimodal"),
            payload: CoreConversationPayload::UserText {
                text: String::from("理解图片和视频"),
                attachments: vec![
                    replayable_attachment(
                        ConversationAttachmentKind::Image,
                        "scene.png",
                        "image/png",
                    ),
                    replayable_attachment(
                        ConversationAttachmentKind::Video,
                        "clip.mp4",
                        "video/mp4",
                    ),
                ],
            },
        }];

        let gemini_projected = core_messages_to_provider_visible_history_for_model(
            &messages,
            Some(ProviderApiFamily::GoogleGeminiGenerateContent),
            Some(&capabilities),
        );
        let CoreConversationPayload::UserText {
            attachments: gemini_attachments,
            ..
        } = &gemini_projected[0].payload
        else {
            panic!("Gemini fixture should remain user text");
        };
        assert_eq!(gemini_attachments.len(), 2);

        let openai_projected = core_messages_to_provider_visible_history_for_model(
            &messages,
            Some(ProviderApiFamily::OpenAiChatCompatible),
            Some(&capabilities),
        );
        let CoreConversationPayload::UserText {
            text,
            attachments: openai_attachments,
        } = &openai_projected[0].payload
        else {
            panic!("OpenAI-compatible fixture should remain user text");
        };
        assert_eq!(openai_attachments.len(), 1);
        assert_eq!(
            openai_attachments[0].kind,
            ConversationAttachmentKind::Image
        );
        assert!(text.contains("this API path"));
    }
}
