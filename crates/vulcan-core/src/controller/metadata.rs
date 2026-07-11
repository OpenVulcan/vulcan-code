use super::*;

pub(super) fn user_turn_record_metadata(
    prompt: &str,
    attachments: &[ConversationAttachment],
) -> Option<Value> {
    if attachments.is_empty() {
        return None;
    }
    let serialized_attachments = serde_json::to_value(attachments).ok()?;
    Some(json!({
        "canonical_text": prompt,
        "canonical_user_text": prompt,
        "attachments": serialized_attachments.clone(),
        "transcript_attachments": serialized_attachments,
    }))
}

/// Builds transcript metadata for one controller-owned interjected user turn.
/// 为控制器持有的插队用户回合构建转录元数据。
///
/// # Parameters
/// 参数
/// `prompt` is the canonical interjected user text stored in the provider-visible message.
/// `prompt` 是存入 Provider 可见消息中的规范插队用户文本。
/// `attachments` are the structured attachments attached to the same interjected user turn.
/// `attachments` 是附着到同一插队用户回合的结构化附件。
///
/// # Returns
/// 返回值
/// Metadata marking the row as a non-editable active-turn process input.
/// 返回将该行标记为不可编辑的当前轮次过程输入的元数据。
pub(super) fn interjected_user_turn_record_metadata(
    prompt: &str,
    attachments: &[ConversationAttachment],
) -> Option<Value> {
    let serialized_attachments = (!attachments.is_empty())
        .then(|| serde_json::to_value(attachments))
        .transpose()
        .ok()
        .flatten();
    let mut metadata = json!({
        "canonical_text": prompt,
        "canonical_user_text": prompt,
        "source": "user_interrupt",
        "starts_user_turn": false,
        "turn_process": true,
        "collapsed_by_default": true,
        "editable": false,
    });
    if let Some(serialized_attachments) = serialized_attachments
        && let Some(object) = metadata.as_object_mut()
    {
        object.insert(String::from("attachments"), serialized_attachments.clone());
        object.insert(
            String::from("transcript_attachments"),
            serialized_attachments,
        );
    }
    Some(metadata)
}

/// Validates canonical message and record identifiers before a snapshot enters controller ownership.
/// 在快照进入控制器所有权前校验规范消息与记录标识。
///
/// # Parameters
/// 参数
/// `snapshot` is the complete candidate snapshot whose two identity namespaces are validated.
/// `snapshot` 是需要校验两个身份命名空间的完整候选快照。
///
/// # Returns
/// 返回值
/// Returns success for a canonical snapshot, or an invalid-state error naming the exact conflict.
/// 规范快照返回成功；否则返回指出准确冲突的无效状态错误。
pub(super) fn validate_conversation_snapshot_identities(
    snapshot: &ConversationSnapshot,
) -> Result<(), CoreError> {
    // message_ids owns the canonical message namespace observed so far.
    // message_ids 保存目前已观察到的规范消息命名空间。
    let mut message_ids = std::collections::BTreeSet::new();
    for (message_index, message) in snapshot.messages.iter().enumerate() {
        validate_snapshot_identifier(
            &mut message_ids,
            message.id.as_str(),
            "message",
            message_index,
        )?;
    }
    // record_ids owns the independent durable-record namespace observed so far.
    // record_ids 保存目前已观察到的独立持久化记录命名空间。
    let mut record_ids = std::collections::BTreeSet::new();
    for (record_index, record) in snapshot.records.iter().enumerate() {
        validate_snapshot_identifier(&mut record_ids, record.id.as_str(), "record", record_index)?;
    }
    Ok(())
}

/// Validates one identifier inside a single canonical snapshot namespace.
/// 在单个规范快照命名空间内校验一个标识。
///
/// # Parameters
/// 参数
/// `identifiers` contains identifiers already accepted in this namespace.
/// `identifiers` 包含该命名空间内已经接受的标识。
/// `identifier` is the exact candidate value without normalization.
/// `identifier` 是未经归一化的精确候选值。
/// `entity` names the canonical entity kind used in diagnostics.
/// `entity` 是诊断信息使用的规范实体类型名称。
/// `index` is the zero-based position of the candidate in its snapshot collection.
/// `index` 是候选项在快照集合中的零基位置。
///
/// # Returns
/// 返回值
/// Returns success after registration, or an invalid-state error without changing the snapshot.
/// 注册成功后返回成功；否则返回无效状态错误且不修改快照。
fn validate_snapshot_identifier<'a>(
    identifiers: &mut std::collections::BTreeSet<&'a str>,
    identifier: &'a str,
    entity: &str,
    index: usize,
) -> Result<(), CoreError> {
    // canonical_identifier proves callers did not rely on silent whitespace normalization.
    // canonical_identifier 证明调用方没有依赖静默空白归一化。
    let canonical_identifier = identifier.trim();
    if canonical_identifier.is_empty() {
        return Err(CoreError::InvalidState {
            message: format!("conversation {entity} {index} has an empty id"),
        });
    }
    if canonical_identifier != identifier {
        return Err(CoreError::InvalidState {
            message: format!(
                "conversation {entity} {index} has surrounding whitespace in id {identifier:?}"
            ),
        });
    }
    if !identifiers.insert(identifier) {
        return Err(CoreError::InvalidState {
            message: format!("conversation {entity} {index} duplicates id {identifier:?}"),
        });
    }
    Ok(())
}

/// Builds one stable JSON metadata object for a tool result record or event.
/// 为工具结果记录或事件构建一份稳定 JSON 元数据对象。
pub(super) fn build_tool_result_metadata(result: &ToolInvocationResult) -> Value {
    let mut base = json!({
        "tool_call_id": result.tool_call_id,
        "tool_name": result.tool_name,
        "is_error": result.is_error,
    });
    if let Some(host_result) = &result.host_result {
        if let (Some(base_object), Value::Object(host_object)) = (base.as_object_mut(), host_result)
        {
            merge_metadata_objects(base_object, host_object);
        } else {
            base["host_result"] = host_result.clone();
        }
    }
    base
}

/// Builds one stable JSON metadata object for a provider usage snapshot.
/// 为一份 Provider 用量快照构建稳定 JSON 元数据对象。
pub(super) fn build_provider_usage_metadata(usage: &crate::types::ProviderUsageSnapshot) -> Value {
    json!({
        "provider_usage": usage,
    })
}

/// Merges one host metadata object into the base metadata object without dropping existing controller fields.
/// 将一份宿主元数据对象合并到基础元数据对象中，同时不丢失控制器原有字段。
pub(super) fn merge_metadata_objects(
    base: &mut JsonMap<String, Value>,
    host: &JsonMap<String, Value>,
) {
    for (key, value) in host {
        base.insert(key.clone(), value.clone());
    }
}

/// Resolves one stable tool-call identifier from the provider-emitted tool-call payload.
/// 从 Provider 产出的工具调用载荷中解析一份稳定工具调用标识。
pub(super) fn resolved_tool_call_id(tool_call: &crate::types::ConversationToolCall) -> String {
    tool_call
        .tool_call_id
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| tool_call.id.clone())
}

/// Renders a JSON value into a pretty string and falls back to the compact representation on serialization failure.
/// 将 JSON 值渲染为美观字符串，并在序列化失败时回退到紧凑表示。
pub(super) fn render_json_pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

/// Returns the current Unix-millis timestamp formatted as text.
/// 返回以文本形式表示的当前 Unix 毫秒时间戳。
pub(super) fn current_unix_millis_text() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .to_string()
}
