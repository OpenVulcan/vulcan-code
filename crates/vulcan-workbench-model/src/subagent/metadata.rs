use super::*;

/// Returns the structured task-session payload attached to one direct or host-result tool metadata object when the result belongs to the host-managed subagent surface.
/// 当结果属于宿主管理的子代理工具面时，返回直接元数据或 host-result 元数据中附带的结构化 task-session 载荷。
pub fn task_session_payload_from_tool_metadata(metadata: &Value) -> Option<&Value> {
    if metadata
        .get("kind")
        .and_then(Value::as_str)
        .is_some_and(is_readable_subagent_task_kind)
        && let Some(payload) = metadata
            .get("payload")
            .filter(|payload| payload.is_object())
    {
        return Some(payload);
    }
    metadata
        .get("host_result")
        .and_then(Value::as_object)
        .and_then(|host_result| {
            host_result
                .get("kind")
                .and_then(Value::as_str)
                .is_some_and(is_readable_subagent_task_kind)
                .then(|| host_result.get("payload"))
                .flatten()
        })
        .filter(|payload| payload.is_object())
}

/// Returns the caller-facing task identifier stored inside one task-session payload when available.
/// 返回 task 会话载荷中保存的调用方面向任务标识。
pub fn task_session_public_id_from_payload(payload: &Value) -> Option<&str> {
    payload
        .get("task_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

/// Returns the stable human-readable task title stored inside one task-session payload when available.
/// 返回 task 会话载荷中保存的稳定人类可读任务标题。
pub fn task_session_display_title_from_payload(payload: &Value) -> Option<&str> {
    payload
        .get("display_title")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

/// Returns the internal child-session identifier stored inside one task-session payload.
/// 返回 task 会话载荷中保存的内部子会话标识。
pub fn task_session_internal_id_from_payload(payload: &Value) -> Option<SessionId> {
    payload
        .get(SUBAGENT_TASK_ID_KEY)
        .or_else(|| payload.get(LEGACY_TASK_SESSION_ID_KEY))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .and_then(|value| SessionId::parse_str(value).ok())
}

/// Returns the original `spawn_agent` tool arguments attached to one tool-display metadata object when the card is still in flight.
/// 当卡片仍处于进行中状态时，返回其工具显示元数据对象中附带的原始 `spawn_agent` 工具参数。
pub fn task_session_arguments_from_tool_metadata(metadata: &Value) -> Option<&Value> {
    (metadata.get("tool_name").and_then(Value::as_str) == Some("spawn_agent")).then_some(())?;
    metadata.get("arguments")
}

/// Returns the caller-facing task identifier from structured task-session payloads or the declared `spawn_agent.task_name` argument.
/// 从结构化 task-session 载荷或已声明的 `spawn_agent.task_name` 参数中返回调用方面向任务标识。
pub fn task_session_declared_task_id_from_tool_metadata(metadata: &Value) -> Option<&str> {
    task_session_payload_from_tool_metadata(metadata)
        .and_then(task_session_public_id_from_payload)
        .or_else(|| {
            task_session_arguments_from_tool_metadata(metadata)
                .and_then(|arguments| arguments.get("task_name"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
        })
}

/// Returns the effective execution status stored in one task-session metadata object, falling back to display-only state when no payload exists yet.
/// 返回一份 task-session 元数据对象中记录的有效执行状态；若尚无 payload，则回退到仅显示层状态。
pub fn task_session_execution_status_from_tool_metadata(metadata: &Value) -> Option<&str> {
    task_session_payload_from_tool_metadata(metadata)
        .and_then(|payload| payload.get("execution_status"))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            metadata
                .get("tool_display_state")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
        })
}

/// Returns one trimmed task-session prompt from structured task-session payloads or the declared `spawn_agent.message` argument.
/// 从结构化 task-session 载荷或已声明的 `spawn_agent.message` 参数中返回裁剪后的提示词。
pub fn task_session_prompt_text_from_tool_metadata(metadata: &Value) -> Option<&str> {
    task_session_payload_from_tool_metadata(metadata)
        .and_then(|payload| payload.get("prompt"))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            task_session_arguments_from_tool_metadata(metadata)
                .and_then(|arguments| arguments.get("message"))
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
        })
}

/// Returns one trimmed task-session description from structured task-session payloads or the declared `spawn_agent.description` argument.
/// 从结构化 task-session 载荷或已声明的 `spawn_agent.description` 参数中返回裁剪后的任务描述。
pub fn task_session_description_from_tool_metadata(metadata: &Value) -> Option<&str> {
    task_session_payload_from_tool_metadata(metadata)
        .and_then(|payload| payload.get("description"))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            task_session_arguments_from_tool_metadata(metadata)
                .and_then(|arguments| arguments.get("description"))
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
        })
}

/// Returns one trimmed task-session subagent label from structured task-session payloads.
/// 从结构化 task-session 载荷中返回裁剪后的子代理标签。
pub fn task_session_subagent_from_tool_metadata(metadata: &Value) -> Option<&str> {
    task_session_payload_from_tool_metadata(metadata)
        .and_then(|payload| payload.get("subagent"))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
}

/// Returns one stable human-readable task title extracted from persisted payload metadata when present.
/// 当存在时，从持久化 payload 元数据中返回一条稳定的人类可读任务标题。
pub fn task_session_display_title_from_tool_metadata(metadata: &Value) -> Option<&str> {
    task_session_payload_from_tool_metadata(metadata)
        .and_then(task_session_display_title_from_payload)
}

/// Builds one compact human-facing task-session label from task metadata and an optional precomputed prompt preview.
/// 根据任务元数据与可选的预计算提示词预览构建一条紧凑的人类可读 task-session 标签。
pub fn task_session_tool_display_label(
    display_title: Option<&str>,
    description: Option<&str>,
    subagent: Option<&str>,
    public_task_id: Option<&str>,
    prompt_preview: Option<&str>,
) -> String {
    if let Some(display_title) = display_title
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return display_title.to_string();
    }
    let description = description.map(str::trim).filter(|value| !value.is_empty());
    let subagent = subagent.map(str::trim).filter(|value| !value.is_empty());
    let public_task_id = public_task_id
        .map(str::trim)
        .filter(|value| !value.is_empty());

    match (description, subagent) {
        (Some(description), Some(subagent)) if description != subagent => {
            return format!("{description}（{subagent}）");
        }
        (Some(description), _) => return description.to_string(),
        (_, Some(subagent)) => return subagent.to_string(),
        _ => {}
    }

    if let Some(public_task_id) = public_task_id
        && SessionId::parse_str(public_task_id).is_err()
    {
        return public_task_id.to_string();
    }
    if let Some(prompt_preview) = prompt_preview
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return prompt_preview.to_string();
    }
    if let Some(public_task_id) = public_task_id {
        return public_task_id.to_string();
    }
    String::from("子代理")
}

/// Injects one display-only execution state flag into tool-card metadata.
/// 向工具卡片元数据注入一个仅用于显示的执行状态标记。
///
/// # Parameters
/// 参数
/// `metadata` is the metadata object that should receive the display state.
/// `metadata` 是需要接收显示状态的元数据对象。
/// `state` is the display state label stored under `tool_display_state`.
/// `state` 是写入 `tool_display_state` 的显示状态标签。
///
/// # Returns
/// 返回值
/// The metadata object with the display-state field attached.
/// 返回已附加显示状态字段的元数据对象。
pub fn attach_tool_display_state_to_metadata(
    metadata: Option<Value>,
    state: &str,
) -> Option<Value> {
    let mut metadata_object = match metadata {
        Some(Value::Object(object)) => object,
        _ => serde_json::Map::new(),
    };
    metadata_object.insert(
        String::from("tool_display_state"),
        Value::String(state.to_string()),
    );
    Some(Value::Object(metadata_object))
}

/// Shallow-merges one tool-call metadata object with a later tool-result or delegate metadata object.
/// 将工具调用元数据对象与后续工具结果或委派元数据对象做浅层合并。
///
/// # Parameters
/// 参数
/// `tool_call_metadata` is the earlier metadata object attached to the parent tool card.
/// `tool_call_metadata` 是较早附着在父级工具卡片上的元数据对象。
/// `tool_result_metadata` is the later metadata object reported by the runtime.
/// `tool_result_metadata` 是运行时后续报告的元数据对象。
///
/// # Returns
/// 返回值
/// The merged metadata object while preserving the most advanced task-session payload.
/// 返回合并后的元数据对象，并保留执行阶段更靠后的 task-session 载荷。
pub fn merge_tool_display_metadata(
    tool_call_metadata: Option<Value>,
    tool_result_metadata: Option<Value>,
) -> Option<Value> {
    match (tool_call_metadata, tool_result_metadata) {
        (Some(Value::Object(mut tool_call_object)), Some(Value::Object(tool_result_object))) => {
            let preferred_task_session_payload = match (
                task_session_payload_execution_rank_from_metadata_object(&tool_call_object),
                task_session_payload_execution_rank_from_metadata_object(&tool_result_object),
            ) {
                (Some(call_rank), Some(result_rank)) if call_rank > result_rank => {
                    task_session_payload_from_metadata_object(&tool_call_object).cloned()
                }
                (Some(call_rank), Some(result_rank)) if result_rank > call_rank => {
                    task_session_payload_from_metadata_object(&tool_result_object).cloned()
                }
                _ => None,
            };
            tool_call_object.extend(tool_result_object);
            if let Some(preferred_payload) = preferred_task_session_payload {
                tool_call_object.insert(
                    String::from("kind"),
                    Value::String(String::from(SUBAGENT_TASK_KIND)),
                );
                tool_call_object.insert(String::from("payload"), preferred_payload);
            }
            Some(Value::Object(tool_call_object))
        }
        (Some(metadata), None) => Some(metadata),
        (None, Some(metadata)) => Some(metadata),
        (Some(_), Some(metadata)) => Some(metadata),
        (None, None) => None,
    }
}

/// Returns the nested direct or host-result task-session payload stored inside one metadata object when available.
/// 当存在时，返回单份元数据对象中嵌套保存的直接或 host-result task-session 载荷。
fn task_session_payload_from_metadata_object(
    metadata: &serde_json::Map<String, Value>,
) -> Option<&Value> {
    metadata
        .get("payload")
        .filter(|payload| payload.is_object())
        .or_else(|| {
            metadata
                .get("host_result")
                .and_then(Value::as_object)
                .and_then(|host_result| {
                    host_result
                        .get("kind")
                        .and_then(Value::as_str)
                        .is_some_and(is_readable_subagent_task_kind)
                        .then(|| host_result.get("payload"))
                        .flatten()
                })
                .filter(|payload| payload.is_object())
        })
}

/// Returns one comparable execution-rank score for the task-session payload embedded in metadata.
/// 返回元数据中嵌入的 task-session 载荷的可比较执行阶段分值。
fn task_session_payload_execution_rank_from_metadata_object(
    metadata: &serde_json::Map<String, Value>,
) -> Option<u8> {
    let execution_status = task_session_payload_from_metadata_object(metadata)
        .and_then(|payload| payload.get("execution_status"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    Some(match execution_status {
        "running" | "created" => 0,
        "completed" | "stopped" | "failed" => 2,
        _ => 1,
    })
}

/// Returns the matching score between one task-card metadata object and one delegate payload.
/// 返回某条 task 卡片元数据与某份委派 payload 的匹配分数。
///
/// # Parameters
/// 参数
/// `metadata` is metadata attached to a visible or persisted task card.
/// `metadata` 是附着在可见或持久化 task 卡片上的元数据。
/// `payload` is the delegate task-session payload reported by the runtime.
/// `payload` 是运行时回报的委派 task-session 载荷。
///
/// # Returns
/// 返回值
/// A positive exact-identity score, or zero when the payload does not match.
/// 返回正数精确身份分；不匹配时返回零。
pub fn task_session_message_match_score(metadata: &Value, payload: &Value) -> usize {
    let payload_internal_task_id = task_session_internal_id_from_payload(payload);
    let metadata_internal_task_id = task_session_payload_from_tool_metadata(metadata)
        .and_then(task_session_internal_id_from_payload);
    if payload_internal_task_id
        .zip(metadata_internal_task_id)
        .is_some_and(|(left, right)| left == right)
    {
        return usize::MAX;
    }
    if task_session_public_id_from_payload(payload)
        .zip(task_session_declared_task_id_from_tool_metadata(metadata))
        .is_some_and(|(left, right)| left == right)
    {
        return usize::MAX.saturating_sub(1);
    }
    0
}

/// Finds the best parent task-card message id for one task-session payload across projected messages and canonical records.
/// 在投影消息与规范记录中，为一份 task-session 载荷查找最匹配的父级 task 卡片消息标识。
///
/// # Parameters
/// 参数
/// `messages` yields projected parent transcript messages from the UI surface.
/// `messages` 产出界面投影中的父级转录消息。
/// `records` yields canonical parent conversation records from runtime state.
/// `records` 产出运行时状态中的父级规范会话记录。
/// `payload` is the task-session payload emitted by the shared task runtime.
/// `payload` 是共享任务运行时发出的 task-session 载荷。
///
/// # Returns
/// 返回值
/// The best matching message id when one can be resolved.
/// 解析到匹配项时返回最佳消息标识。
pub fn find_task_session_tool_message_id_for_payload<'a>(
    messages: impl Iterator<Item = &'a SessionMessageEntry>,
    records: impl Iterator<Item = &'a SessionConversationRecord>,
    payload: &Value,
) -> Option<String> {
    let mut best_match: Option<(usize, String)> = None;
    if collect_best_task_session_message_match(messages, payload, &mut best_match) {
        return best_match.map(|(_, message_id)| message_id);
    }
    if collect_best_task_session_record_match(records, payload, &mut best_match) {
        return best_match.map(|(_, message_id)| message_id);
    }
    best_match.map(|(_, message_id)| message_id)
}

/// Updates the best task-card match from projected session messages.
/// 从投影会话消息中更新最佳 task 卡片匹配项。
fn collect_best_task_session_message_match<'a>(
    messages: impl Iterator<Item = &'a SessionMessageEntry>,
    payload: &Value,
    best_match: &mut Option<(usize, String)>,
) -> bool {
    for message in messages {
        if message.record_kind != SessionMessageRecordKind::ToolCallOutput {
            continue;
        }
        let Some(metadata) = message.metadata.as_ref() else {
            continue;
        };
        if !task_session_metadata_can_match_tool_card(metadata) {
            continue;
        }
        if update_task_session_best_match(metadata, payload, message.id.clone(), best_match) {
            return true;
        }
    }
    false
}

/// Updates the best task-card match from canonical session conversation records.
/// 从规范会话记录中更新最佳 task 卡片匹配项。
fn collect_best_task_session_record_match<'a>(
    records: impl Iterator<Item = &'a SessionConversationRecord>,
    payload: &Value,
    best_match: &mut Option<(usize, String)>,
) -> bool {
    for record in records {
        if record.record_kind != SessionMessageRecordKind::ToolCallOutput {
            continue;
        }
        let Some(metadata) = record.metadata.as_ref() else {
            continue;
        };
        if !task_session_metadata_can_match_tool_card(metadata) {
            continue;
        }
        if update_task_session_best_match(metadata, payload, record.id.clone(), best_match) {
            return true;
        }
    }
    false
}

/// Returns whether one metadata object represents a spawn-agent card that can be matched with task-session payloads.
/// 返回单个元数据对象是否表示可与 task-session 载荷匹配的 spawn_agent 工具卡片。
fn task_session_metadata_can_match_tool_card(metadata: &Value) -> bool {
    metadata.get("tool_name").and_then(Value::as_str) == Some("spawn_agent")
        || metadata
            .get("kind")
            .and_then(Value::as_str)
            .is_some_and(is_readable_subagent_task_kind)
}

/// Applies one candidate score to the current best task-card match.
/// 将单个候选分数应用到当前最佳 task 卡片匹配项。
fn update_task_session_best_match(
    metadata: &Value,
    payload: &Value,
    message_id: String,
    best_match: &mut Option<(usize, String)>,
) -> bool {
    let score = task_session_message_match_score(metadata, payload);
    if score == 0 {
        return false;
    }
    if best_match
        .as_ref()
        .is_none_or(|(best_score, _)| score > *best_score)
    {
        *best_match = Some((score, message_id));
    }
    score == usize::MAX
}

/// Builds canonical stopped task-session metadata for a parent task card.
/// 为父级 task 卡片构建规范的已停止 task-session 元数据。
///
/// # Parameters
/// 参数
/// `existing_metadata` is the metadata currently attached to the parent task card.
/// `existing_metadata` 是当前附着在父级 task 卡片上的元数据。
/// `state_snapshot` is the latest cached subagent panel state when available.
/// `state_snapshot` 是可用时最近的子代理面板缓存状态。
/// `task_id` is the stable child task session identifier.
/// `task_id` 是稳定的子任务会话标识。
/// `stop_summary` is the user-facing stop summary to persist.
/// `stop_summary` 是需要持久化的用户可见停止摘要。
/// `updated_at` is the timestamp text to write into the task payload.
/// `updated_at` 是写入任务载荷的时间戳文本。
///
/// # Returns
/// 返回值
/// A metadata object with `execution_status` and `tool_display_state` set to `stopped`.
/// 返回将 `execution_status` 与 `tool_display_state` 设置为 `stopped` 的元数据对象。
pub fn build_stopped_task_session_metadata(
    existing_metadata: Option<&Value>,
    state_snapshot: Option<&SubagentTaskPanelState>,
    task_id: SessionId,
    stop_summary: &str,
    updated_at: &str,
) -> Value {
    let prompt = existing_metadata
        .and_then(task_session_prompt_text_from_tool_metadata)
        .map(str::to_string);
    let description = existing_metadata
        .and_then(task_session_description_from_tool_metadata)
        .map(str::to_string)
        .or_else(|| state_snapshot.and_then(|state| state.description.clone()));
    let subagent = existing_metadata
        .and_then(task_session_subagent_from_tool_metadata)
        .map(str::to_string)
        .or_else(|| {
            state_snapshot.and_then(|state| {
                (!state.subagent_label.trim().is_empty()).then(|| state.subagent_label.clone())
            })
        });
    let public_task_id = existing_metadata
        .and_then(task_session_declared_task_id_from_tool_metadata)
        .map(str::to_string)
        .or_else(|| state_snapshot.map(|state| state.public_task_id.clone()))
        .unwrap_or_else(|| task_id.to_string());
    let metadata = json!({
        "tool_name": "spawn_agent",
        "kind": SUBAGENT_TASK_KIND,
        "payload": {
            "task_id": public_task_id,
            (SUBAGENT_TASK_ID_KEY): task_id.to_string(),
            "execution_status": "stopped",
            "prompt": prompt,
            "description": description,
            "subagent": subagent,
            "result": stop_summary,
            "updated_at": updated_at,
        },
    });
    attach_tool_display_state_to_metadata(Some(metadata), "stopped").unwrap_or_else(|| {
        json!({
            "tool_name": "spawn_agent",
            "kind": SUBAGENT_TASK_KIND,
            "tool_display_state": "stopped",
            "payload": {
                "task_id": task_id.to_string(),
                (SUBAGENT_TASK_ID_KEY): task_id.to_string(),
                "execution_status": "stopped",
                "result": stop_summary,
                "updated_at": updated_at,
            },
        })
    })
}
