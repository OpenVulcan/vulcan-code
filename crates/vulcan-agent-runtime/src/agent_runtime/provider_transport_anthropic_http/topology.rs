use super::*;

pub(super) fn coalesce_consecutive_anthropic_user_messages(messages: Vec<Value>) -> Vec<Value> {
    let mut merged_messages: Vec<Value> = Vec::with_capacity(messages.len());

    for message in messages {
        let Some(message_tool_results) = anthropic_user_tool_result_blocks(&message) else {
            merged_messages.push(message);
            continue;
        };

        let Some(previous_message) = merged_messages.last_mut() else {
            merged_messages.push(message);
            continue;
        };

        let previous_is_tool_result_only =
            anthropic_user_tool_result_blocks(previous_message).is_some();
        if previous_is_tool_result_only
            && let Some(previous_content) = previous_message
                .get_mut("content")
                .and_then(Value::as_array_mut)
        {
            // Merge only adjacent tool-result turns because Anthropic binds tool_result blocks to the immediately preceding assistant tool_use turn.
            // 仅合并相邻的工具结果回合，因为 Anthropic 会把 tool_result 内容块绑定到紧邻的上一条 assistant tool_use 回合。
            previous_content.extend(message_tool_results);
            continue;
        }

        merged_messages.push(message);
    }

    merged_messages
}

/// Extracts one Anthropic user message content array only when every block is a `tool_result`, so asynchronous task acknowledgements can be regrouped without mixing in ordinary user text.
/// 仅当一条 Anthropic user 消息的所有内容块都是 `tool_result` 时提取其内容数组，以便在不混入普通用户文本的前提下重组异步任务确认消息。
pub(super) fn anthropic_user_tool_result_blocks(message: &Value) -> Option<Vec<Value>> {
    let role = message.get("role").and_then(Value::as_str)?;
    if role != "user" {
        return None;
    }

    let content = message.get("content").and_then(Value::as_array)?;
    if !content.is_empty()
        && content
            .iter()
            .all(|block| block.get("type").and_then(Value::as_str) == Some("tool_result"))
    {
        Some(content.clone())
    } else {
        None
    }
}

/// Repairs Anthropic tool-result message topology before sending so provider switches never replay orphaned tool results as protocol-native blocks.
/// 在发送前修复 Anthropic 工具结果消息拓扑，避免供应商切换时把孤儿工具结果按协议原生块重放。
pub(super) fn repair_anthropic_tool_result_topology(messages: Vec<Value>) -> Vec<Value> {
    let mut repaired_messages = Vec::with_capacity(messages.len());
    let mut pending_tool_use_ids: Option<BTreeSet<String>> = None;

    for message in messages {
        if let Some(tool_use_ids) = anthropic_assistant_tool_use_ids(&message) {
            pending_tool_use_ids = Some(tool_use_ids);
            repaired_messages.push(message);
            continue;
        }

        let Some(user_content) = anthropic_user_content_blocks(&message) else {
            pending_tool_use_ids = None;
            repaired_messages.push(message);
            continue;
        };

        let (tool_results, ordinary_blocks) =
            split_anthropic_tool_result_blocks(user_content.clone());
        if tool_results.is_empty() {
            pending_tool_use_ids = None;
            repaired_messages.push(message);
            continue;
        }

        let every_tool_result_matches_pending_assistant =
            pending_tool_use_ids.as_ref().is_some_and(|tool_use_ids| {
                !tool_use_ids.is_empty()
                    && tool_results.iter().all(|block| {
                        anthropic_tool_result_call_id(block)
                            .map(|tool_use_id| tool_use_ids.contains(tool_use_id))
                            .unwrap_or(false)
                    })
            });

        if every_tool_result_matches_pending_assistant {
            push_anthropic_tool_result_user_message(&mut repaired_messages, tool_results);
            if !ordinary_blocks.is_empty() {
                repaired_messages.push(anthropic_user_message_from_content(ordinary_blocks));
                pending_tool_use_ids = None;
            }
            continue;
        }

        if ordinary_blocks.is_empty() {
            repaired_messages.push(anthropic_downgraded_tool_result_user_message(&tool_results));
        } else {
            repaired_messages.push(anthropic_mixed_downgraded_tool_result_user_message(
                ordinary_blocks,
                &tool_results,
            ));
        }
        pending_tool_use_ids = None;
    }

    repaired_messages
}

/// Extracts cloned content blocks from one Anthropic user message.
/// 从一条 Anthropic user 消息中提取克隆后的内容块。
pub(super) fn anthropic_user_content_blocks(message: &Value) -> Option<Vec<Value>> {
    let role = message.get("role").and_then(Value::as_str)?;
    if role != "user" {
        return None;
    }
    message.get("content").and_then(Value::as_array).cloned()
}

/// Splits Anthropic user content into protocol-native tool results and ordinary user blocks.
/// 将 Anthropic user 内容拆分为协议原生工具结果块与普通用户内容块。
pub(super) fn split_anthropic_tool_result_blocks(content: Vec<Value>) -> (Vec<Value>, Vec<Value>) {
    let mut tool_results = Vec::new();
    let mut ordinary_blocks = Vec::new();

    for block in content {
        if block.get("type").and_then(Value::as_str) == Some("tool_result") {
            tool_results.push(block);
        } else {
            ordinary_blocks.push(block);
        }
    }

    (tool_results, ordinary_blocks)
}

/// Builds one Anthropic user message from already normalized content blocks.
/// 根据已规范化的内容块构建一条 Anthropic user 消息。
pub(super) fn anthropic_user_message_from_content(content: Vec<Value>) -> Value {
    json!({
        "role": "user",
        "content": content
    })
}

/// Pushes or extends one Anthropic tool-result user message so consecutive valid tool results remain a single legal follow-up message.
/// 推入或扩展一条 Anthropic 工具结果 user 消息，确保连续合法工具结果仍保持为单条后续消息。
pub(super) fn push_anthropic_tool_result_user_message(
    messages: &mut Vec<Value>,
    tool_results: Vec<Value>,
) {
    if let Some(previous_message) = messages.last_mut()
        && anthropic_user_tool_result_blocks(previous_message).is_some()
        && let Some(previous_content) = previous_message
            .get_mut("content")
            .and_then(Value::as_array_mut)
    {
        previous_content.extend(tool_results);
        return;
    }

    messages.push(anthropic_user_message_from_content(tool_results));
}

/// Extracts provider-visible Anthropic tool-use identifiers from one assistant message when it can legally accept a following tool-result message.
/// 从一条可合法承接后续工具结果的 assistant 消息中提取 Provider 可见的 Anthropic 工具使用标识。
pub(super) fn anthropic_assistant_tool_use_ids(message: &Value) -> Option<BTreeSet<String>> {
    let role = message.get("role").and_then(Value::as_str)?;
    if role != "assistant" {
        return None;
    }

    let content = message.get("content").and_then(Value::as_array)?;
    let tool_use_ids = content
        .iter()
        .filter(|block| block.get("type").and_then(Value::as_str) == Some("tool_use"))
        .filter_map(|block| block.get("id").and_then(Value::as_str))
        .map(str::trim)
        .filter(|tool_use_id| !tool_use_id.is_empty())
        .map(String::from)
        .collect::<BTreeSet<_>>();

    (!tool_use_ids.is_empty()).then_some(tool_use_ids)
}

/// Extracts one Anthropic tool-result identifier from a content block.
/// 从一个 Anthropic 工具结果内容块中提取工具使用标识。
pub(super) fn anthropic_tool_result_call_id(block: &Value) -> Option<&str> {
    let block_type = block.get("type").and_then(Value::as_str)?;
    if block_type != "tool_result" {
        return None;
    }
    block
        .get("tool_use_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|tool_use_id| !tool_use_id.is_empty())
}

/// Converts unsafe Anthropic-native tool results into ordinary user text while preserving their diagnostic payload.
/// 将不安全的 Anthropic 原生工具结果转换为普通用户文本，同时保留其诊断载荷。
pub(super) fn anthropic_downgraded_tool_result_user_message(tool_results: &[Value]) -> Value {
    json!({
        "role": "user",
        "content": [{
            "type": "text",
            "text": anthropic_downgraded_tool_result_text(tool_results)
        }]
    })
}

/// Converts unsafe Anthropic-native tool results into ordinary user text while preserving any non-tool user content that had been batched with them.
/// 将不安全的 Anthropic 原生工具结果转换为普通用户文本，同时保留与其同批混入的非工具用户内容。
pub(super) fn anthropic_mixed_downgraded_tool_result_user_message(
    mut ordinary_blocks: Vec<Value>,
    tool_results: &[Value],
) -> Value {
    ordinary_blocks.push(json!({
        "type": "text",
        "text": anthropic_downgraded_tool_result_text(tool_results)
    }));
    anthropic_user_message_from_content(ordinary_blocks)
}

/// Builds a replay-safe text representation for tool results that cannot satisfy Anthropic's strict adjacency rule.
/// 为无法满足 Anthropic 严格相邻规则的工具结果构建可安全回放的文本表示。
pub(super) fn anthropic_downgraded_tool_result_text(tool_results: &[Value]) -> String {
    let mut text = format!(
        "{ANTHROPIC_DOWNGRADED_TOOL_RESULT_MARKER} because no immediately preceding matching tool_use was present."
    );

    for tool_result in tool_results {
        text.push_str("\n\n<tool_result");
        if let Some(tool_use_id) = anthropic_tool_result_call_id(tool_result) {
            text.push_str(" tool_use_id=\"");
            text.push_str(tool_use_id);
            text.push('"');
        }
        text.push_str(">\n");
        text.push_str(
            &serde_json::to_string_pretty(tool_result).unwrap_or_else(|_| tool_result.to_string()),
        );
        text.push_str("\n</tool_result>");
    }

    text
}

/// Emits an audit event when the final Anthropic payload contains downgraded tool results.
/// 当最终 Anthropic 载荷包含已降级工具结果时发出审计事件。
pub(super) fn emit_anthropic_lowering_repair_if_needed(
    live_sender: &ProviderLiveDeltaSender,
    provider_id: &str,
    model_id: &str,
    payload: &Value,
) {
    let affected_items = anthropic_downgraded_tool_result_count(payload);
    if affected_items == 0 {
        return;
    }

    let event = RuntimeProviderLoweringRepairEvent {
        turn_id: None,
        repair_id: format!(
            "anthropic-tool-result-topology-{}",
            stable_anthropic_lowering_repair_hash(provider_id, model_id, payload)
        ),
        provider_id: provider_id.to_string(),
        model_id: model_id.to_string(),
        provider_family: String::from("anthropic"),
        action: String::from("downgrade_orphan_tool_result_to_text"),
        reason: String::from(
            "Anthropic requires each tool_result to immediately follow a matching assistant tool_use.",
        ),
        affected_items,
    };
    let _ = live_sender.send(Ok(ProviderStreamDelta::LoweringRepair(event)));
}

/// Counts downgraded Anthropic tool results in the final wire payload.
/// 统计最终线协议载荷中已降级的 Anthropic 工具结果数量。
pub(super) fn anthropic_downgraded_tool_result_count(payload: &Value) -> usize {
    payload
        .get("messages")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|message| message.get("content").and_then(Value::as_array))
        .flat_map(|content| content.iter())
        .filter_map(|block| {
            if block.get("type").and_then(Value::as_str) != Some("text") {
                return None;
            }
            block.get("text").and_then(Value::as_str)
        })
        .filter(|text| text.contains(ANTHROPIC_DOWNGRADED_TOOL_RESULT_MARKER))
        .map(|text| text.matches("<tool_result").count().max(1))
        .sum()
}

/// Builds a stable audit hash for one Anthropic lowering repair event.
/// 为单个 Anthropic 转换修复事件构建稳定审计哈希。
pub(super) fn stable_anthropic_lowering_repair_hash(
    provider_id: &str,
    model_id: &str,
    payload: &Value,
) -> String {
    let mut hasher = DefaultHasher::new();
    provider_id.hash(&mut hasher);
    model_id.hash(&mut hasher);
    if let Ok(serialized_payload) = serde_json::to_string(payload) {
        serialized_payload.hash(&mut hasher);
    }
    format!("{:016x}", hasher.finish())
}
