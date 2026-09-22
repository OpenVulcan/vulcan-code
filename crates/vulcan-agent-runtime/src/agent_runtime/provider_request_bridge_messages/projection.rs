use super::*;

/// Converts canonical core messages into the provider-visible history that will actually be sent upstream.
/// 将规范 core 消息转换为真正会发送给上游 Provider 的可见历史。
pub(crate) fn core_messages_to_provider_visible_history(
    messages: &[CoreConversationMessage],
) -> Result<Vec<CoreConversationMessage>, String> {
    core_messages_to_provider_visible_history_for_model(messages, None, None)
}

/// Converts canonical core messages into provider-visible history using API-family and model modality projection.
/// 使用接口族与模型模态投影将规范 core 消息转换为 Provider 可见历史。
pub(crate) fn core_messages_to_provider_visible_history_for_model(
    messages: &[CoreConversationMessage],
    protocol_profile_id: Option<&str>,
    capabilities: Option<&ModelCapabilities>,
) -> Result<Vec<CoreConversationMessage>, String> {
    core_messages_to_provider_visible_history_for_model_with_system_updates(
        messages,
        protocol_profile_id,
        capabilities,
        &[],
        ProviderSystemUpdateLoweringPolicy::default(),
    )
}

/// Converts canonical core messages plus chronological system updates into provider-visible history.
/// 将规范 core 消息与时间顺序 system update 一起转换为 Provider 可见历史。
pub(crate) fn core_messages_to_provider_visible_history_for_model_with_system_updates(
    messages: &[CoreConversationMessage],
    protocol_profile_id: Option<&str>,
    capabilities: Option<&ModelCapabilities>,
    system_updates: &[RuntimeSystemUpdateEvent],
    system_update_policy: ProviderSystemUpdateLoweringPolicy,
) -> Result<Vec<CoreConversationMessage>, String> {
    core_messages_to_provider_visible_history_for_model_with_system_updates_and_tool_bridges(
        messages,
        protocol_profile_id,
        capabilities,
        &[],
        system_updates,
        system_update_policy,
    )
}

/// Converts canonical messages with exact visible host-tool bridges into provider-visible history.
/// 使用精确可见宿主工具桥接将规范消息转换为 Provider 可见历史。
///
/// # Parameters
/// 参数
/// `bridge_tool_ids` contains the exact resolved tool identifiers for this execution turn.
/// `bridge_tool_ids` 包含本次执行回合精确解析出的工具标识。
///
/// # Returns
/// 返回值
/// Provider-visible history after attachment and system-update projection, or a precise lowering error.
/// 完成附件与系统更新投影后的 Provider 可见历史，或精确的降级错误。
pub(crate) fn core_messages_to_provider_visible_history_for_model_with_system_updates_and_tool_bridges(
    messages: &[CoreConversationMessage],
    protocol_profile_id: Option<&str>,
    capabilities: Option<&ModelCapabilities>,
    bridge_tool_ids: &[String],
    system_updates: &[RuntimeSystemUpdateEvent],
    system_update_policy: ProviderSystemUpdateLoweringPolicy,
) -> Result<Vec<CoreConversationMessage>, String> {
    let mut projected = project_core_messages_to_provider_visible_history_for_model(
        messages,
        protocol_profile_id,
        capabilities,
        bridge_tool_ids,
    )?;
    apply_provider_visible_system_updates(&mut projected, system_updates, system_update_policy)?;

    validate_provider_visible_tool_topology(&projected)?;

    Ok(projected)
}

/// Projects canonical messages into provider-visible history before runtime system-update overlays are applied.
/// 在应用 runtime system-update 覆盖之前，将规范消息投影为 Provider 可见历史。
fn project_core_messages_to_provider_visible_history_for_model(
    messages: &[CoreConversationMessage],
    protocol_profile_id: Option<&str>,
    capabilities: Option<&ModelCapabilities>,
    bridge_tool_ids: &[String],
) -> Result<Vec<CoreConversationMessage>, String> {
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
                        protocol_profile_id,
                        capabilities,
                        bridge_tool_ids,
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
                    usage.as_deref(),
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
                            reasoning: reasoning.clone(),
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
                    let mut active_tool_call_identities =
                        provider_visible_tool_call_identities(tool_calls);
                    {
                        projected.push(CoreConversationMessage {
                            id: message.id.clone(),
                            payload: CoreConversationPayload::Assistant {
                                text: text.clone(),
                                reasoning: reasoning.clone(),
                                provider_message_id: provider_message_id.clone(),
                                provider_message_state: provider_message_state.clone(),
                                usage: usage.clone(),
                                tool_calls: normalized_provider_visible_tool_calls(tool_calls),
                            },
                        });
                        for result_message in &messages[index + 1..result_end] {
                            projected.push(project_tool_result_message(
                                result_message,
                                &mut active_tool_call_identities,
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
                let mut active_tool_call_identities = Vec::new();
                projected.push(project_tool_result_message(
                    message,
                    &mut active_tool_call_identities,
                ));
                index += 1;
            }
            CoreConversationPayload::AgentMessage { .. } => {
                projected.push(CoreConversationMessage {
                    id: message.id.clone(),
                    payload: CoreConversationPayload::UserText {
                        text: message.payload.agent_message_model_text(&message.id)?,
                        attachments: Vec::new(),
                    },
                });
                index += 1;
            }
        }
    }

    Ok(projected)
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
            ProviderSystemUpdateLowering::User { text } => text,
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
    let mut pending_tool_call_ids = BTreeSet::<String>::new();
    let mut pending_assistant_index = None::<usize>;
    for (index, message) in messages.iter().enumerate() {
        match &message.payload {
            CoreConversationPayload::Assistant { tool_calls, .. } if !tool_calls.is_empty() => {
                if !pending_tool_call_ids.is_empty() {
                    return Err(format!(
                        "assistant message at index {index} started a new tool batch before index {} completed",
                        pending_assistant_index.unwrap_or(index)
                    ));
                }
                pending_tool_call_ids = tool_calls
                    .iter()
                    .filter_map(provider_visible_tool_call_id)
                    .collect();
                pending_assistant_index = Some(index);
                if pending_tool_call_ids.len() != tool_calls.len() {
                    return Err(format!(
                        "assistant message at index {index} contains tool calls without unique provider-visible identifiers"
                    ));
                }
            }
            CoreConversationPayload::Assistant { .. }
            | CoreConversationPayload::UserText { .. }
            | CoreConversationPayload::AgentMessage { .. } => {
                if !pending_tool_call_ids.is_empty() {
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
                if !pending_tool_call_ids.remove(tool_call_key) {
                    return Err(format!(
                        "tool result at index {index} references unmatched tool call `{tool_call_key}`"
                    ));
                }
                if pending_tool_call_ids.is_empty() {
                    pending_assistant_index = None;
                }
            }
        }
    }
    if !pending_tool_call_ids.is_empty() {
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
    protocol_profile_id: Option<&str>,
    capabilities: Option<&ModelCapabilities>,
    bridge_tool_ids: &[String],
) -> CoreConversationPayload {
    match protocol_profile_id {
        Some(protocol_profile_id) => {
            let normalized = normalize_provider_user_input_for_model_with_tool_bridges(
                protocol_profile_id,
                capabilities,
                text,
                attachments,
                bridge_tool_ids,
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
    active_tool_call_identities: &mut Vec<(String, Vec<String>)>,
) -> CoreConversationMessage {
    let CoreConversationPayload::ToolResult {
        tool_call_id,
        is_error,
        text,
    } = &message.payload
    else {
        return message.clone();
    };
    let tool_call_key = tool_call_id.as_deref().unwrap_or_default().trim();
    let matching_identity_index = if tool_call_key.is_empty() {
        None
    } else {
        active_tool_call_identities
            .iter()
            .position(|(_, aliases)| aliases.iter().any(|alias| alias == tool_call_key))
    };
    let payload = if let Some(identity_index) = matching_identity_index {
        let (provider_visible_id, _) = active_tool_call_identities.remove(identity_index);
        CoreConversationPayload::ToolResult {
            tool_call_id: Some(provider_visible_id),
            is_error: *is_error,
            text: text.clone(),
        }
    } else {
        CoreConversationPayload::UserText {
            text: orphan_tool_result_text(&message.id, tool_call_id, text),
            attachments: Vec::new(),
        }
    };
    CoreConversationMessage {
        id: message.id.clone(),
        payload,
    }
}

/// Selects the exact identifier serialized for one provider-visible tool call.
/// 选择单个 Provider 可见工具调用在序列化时使用的精确标识。
///
/// # Parameters
/// 参数
/// `tool_call` is the canonical stored tool call that may contain local and provider identifiers.
/// `tool_call` 是可能同时包含本地标识与 Provider 标识的规范存储工具调用。
///
/// # Returns
/// 返回值
/// The non-empty canonical provider-visible identifier.
/// 返回非空的规范 Provider 可见标识。
fn provider_visible_tool_call_id(tool_call: &CoreConversationToolCall) -> Option<String> {
    tool_call
        .tool_call_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// Normalizes assistant tool calls to the identifiers emitted by provider wire encoders.
/// 将助手工具调用归一化为 Provider 线协议编码器实际发出的标识。
///
/// # Parameters
/// 参数
/// `tool_calls` contains the canonical stored calls for one completed assistant batch.
/// `tool_calls` 包含一批已完成助手调用的规范存储记录。
///
/// # Returns
/// 返回值
/// Cloned calls whose `tool_call_id` exactly matches their provider-visible identifier.
/// 返回克隆后的调用，其 `tool_call_id` 与 Provider 可见标识完全一致。
fn normalized_provider_visible_tool_calls(
    tool_calls: &[CoreConversationToolCall],
) -> Vec<CoreConversationToolCall> {
    tool_calls
        .iter()
        .map(|tool_call| {
            let mut normalized = tool_call.clone();
            normalized.tool_call_id = provider_visible_tool_call_id(tool_call);
            normalized
        })
        .collect()
}

/// Builds provider-visible identifiers together with every accepted historical alias.
/// 构建 Provider 可见标识及其可接受的全部历史别名。
///
/// # Parameters
/// 参数
/// `tool_calls` contains the canonical stored calls for one completed assistant batch.
/// `tool_calls` 包含一批已完成助手调用的规范存储记录。
///
/// # Returns
/// 返回值
/// Ordered identity groups used to rewrite matching tool results exactly once.
/// 返回有序身份组，用于将匹配的工具结果精确改写一次。
fn provider_visible_tool_call_identities(
    tool_calls: &[CoreConversationToolCall],
) -> Vec<(String, Vec<String>)> {
    tool_calls
        .iter()
        .filter_map(|tool_call| {
            provider_visible_tool_call_id(tool_call)
                .map(|provider_visible_id| (provider_visible_id, tool_call_identifiers(tool_call)))
        })
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
fn orphan_tool_result_text(message_id: &str, tool_call_id: &Option<String>, text: &str) -> String {
    let call_label = tool_call_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("unknown");
    format!(
        "ERROR: Tool result `{}` cannot be replayed as a tool response because matching assistant tool call `{}` is no longer present in context. Treat the following as historical text only.\n\n{}",
        message_id, call_label, text
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
                tool_call_id,
                is_error,
                text,
            } = &result_message.payload
            {
                let call_label = tool_call_id
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or("unknown");
                lines.push(format!(
                    "- message={} call={} error={}: {}",
                    result_message.id,
                    call_label,
                    is_error,
                    text.trim()
                ));
            }
        }
    }
    lines.join("\n")
}
