//! Shared subagent task-session projection helpers reused by WebUI and service.
//! 供 WebUI 与服务复用的共享子代理 task-session 投影辅助逻辑。

use crate::SessionId;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use vulcan_context::{
    SessionConversationRecord, SessionConversationState, SessionMessageEntry,
    SessionMessageRecordKind,
};
use vulcan_session_core::{
    ParentSessionRef, SessionContextDocument, SessionNodeId, SessionNodeStore,
};
use vulcan_task_session::{
    SegmentedTaskSessionStore, TaskSessionMetadata, TaskSessionStatus, TaskSessionStore,
};

/// Canonical user-facing stop summary reused by subagent cards, panels, and WebUI projections.
/// 供子代理卡片、面板与 WebUI 投影共同复用的规范用户可见停止摘要。
pub const SUBAGENT_TASK_STOPPED_SUMMARY_TEXT: &str = "子代理已按用户请求主动停止，当前没有完整返回结果。请基于现有上下文决定是否重试、改写任务，或直接继续主流程。";

/// Lifecycle states used by one cached subagent task panel.
/// 单个已缓存子代理任务面板使用的生命周期状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubagentTaskRunState {
    /// The subagent is still running and only a lightweight placeholder panel should be shown.
    /// 子代理仍在运行中，此时只应显示轻量占位面板。
    Running,
    /// The subagent was stopped before natural completion and the panel should preserve the partial transcript plus the stop summary.
    /// 子代理在自然完成前已被停止，此时面板应保留部分转录与停止摘要。
    Stopped,
    /// The subagent finished successfully and its final transcript snapshot can be reviewed.
    /// 子代理已成功完成，此时可以回看其最终转录快照。
    Completed,
    /// The subagent stopped in a failed state and the panel should surface the latest failure detail.
    /// 子代理以失败状态结束，此时面板应展示最近一次失败说明。
    Failed,
}

impl SubagentTaskRunState {
    /// Returns whether the panel state still represents one active running subagent.
    /// 返回当前面板状态是否仍表示一个处于活动运行中的子代理。
    pub fn is_running(self) -> bool {
        matches!(self, Self::Running)
    }
}

/// Active subagent panel selection shown in place of the main parent transcript.
/// 显示在主父会话转录位置上的活动子代理面板选择状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectedSubagentTaskPanel {
    /// Parent session identifier that owns the subagent task card and badge entry.
    /// 拥有该子代理任务卡片与徽标入口的父会话标识。
    pub parent_session_id: SessionId,
    /// Stable child task identifier whose transcript panel is currently opened.
    /// 当前已打开其转录面板的稳定子任务标识。
    pub task_id: SessionId,
}

/// Cached subagent task panel state reconstructed from runtime delegate events and persisted spawn-agent results.
/// 由运行时委派事件与持久化 spawn_agent 结果重建出的子代理任务面板缓存状态。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubagentTaskPanelState {
    /// Stable child task identifier used by badges, cards, and panel navigation.
    /// 被徽标、卡片与面板导航共同使用的稳定子任务标识。
    pub task_id: SessionId,
    /// Caller-facing task identifier preserved for parent-agent reasoning and human-visible summaries.
    /// 为父代理推理与人类可见摘要保留的调用方面向任务标识。
    pub public_task_id: String,
    /// Parent session identifier that owns the cached subagent task panel.
    /// 拥有该缓存子代理任务面板的父会话标识。
    pub parent_session_id: SessionId,
    /// Best-effort subagent label shown in badges and panel headers.
    /// 展示在徽标与面板标题中的最佳努力子代理标签。
    pub subagent_label: String,
    /// Optional user-facing task description attached to the task invocation.
    /// 附着在任务调用上的可选面向用户任务描述。
    pub description: Option<String>,
    /// Current lifecycle state of the cached subagent panel.
    /// 当前缓存子代理面板所处的生命周期状态。
    pub run_state: SubagentTaskRunState,
    /// Best-effort final or failure text shown in the child panel summary area.
    /// 展示在子面板摘要区中的最佳努力最终结果或失败文本。
    pub result_text: String,
    /// Optional canonical child conversation snapshot projected into the dedicated subagent panel.
    /// 可选的规范子会话快照，会被投影到独立子代理面板中。
    pub conversation_state: Option<SessionConversationState>,
}

/// Returns the structured task-session payload attached to one direct or host-result tool metadata object when the result belongs to the host-managed subagent surface.
/// 当结果属于宿主管理的子代理工具面时，返回直接元数据或 host-result 元数据中附带的结构化 task-session 载荷。
pub fn task_session_payload_from_tool_metadata(metadata: &Value) -> Option<&Value> {
    if metadata.get("kind").and_then(Value::as_str) == Some("task_session")
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
            (host_result.get("kind").and_then(Value::as_str) == Some("task_session"))
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
        .get("task_session_id")
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

/// Returns the best-effort caller-facing task identifier attached to one task card from either persisted payload metadata or in-flight tool arguments.
/// 从持久化载荷元数据或进行中工具参数中返回一条最佳努力的调用方面向任务标识。
pub fn task_session_declared_task_id_from_tool_metadata(metadata: &Value) -> Option<&str> {
    task_session_payload_from_tool_metadata(metadata)
        .and_then(task_session_public_id_from_payload)
        .or_else(|| {
            task_session_arguments_from_tool_metadata(metadata)
                .and_then(|arguments| {
                    arguments
                        .get("task_name")
                        .or_else(|| arguments.get("task_id"))
                        .or_else(|| arguments.get("name"))
                })
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

/// Returns one trimmed task-session prompt text extracted from either persisted payload metadata or in-flight tool arguments.
/// 从持久化 payload 元数据或进行中工具参数中提取一段裁剪后的 task-session 提示词文本。
pub fn task_session_prompt_text_from_tool_metadata(metadata: &Value) -> Option<&str> {
    task_session_payload_from_tool_metadata(metadata)
        .and_then(|payload| payload.get("prompt"))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            task_session_arguments_from_tool_metadata(metadata)
                .and_then(|arguments| arguments.get("message").or_else(|| arguments.get("prompt")))
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
        })
}

/// Returns one trimmed task-session description extracted from either persisted payload metadata or in-flight tool arguments.
/// 从持久化 payload 元数据或进行中工具参数中提取一段裁剪后的 task-session 描述文本。
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

/// Returns one trimmed task-session subagent label extracted from either persisted payload metadata or in-flight tool arguments.
/// 从持久化 payload 元数据或进行中工具参数中提取一段裁剪后的 task-session 子代理标签。
pub fn task_session_subagent_from_tool_metadata(metadata: &Value) -> Option<&str> {
    task_session_payload_from_tool_metadata(metadata)
        .and_then(|payload| payload.get("subagent"))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            task_session_arguments_from_tool_metadata(metadata)
                .and_then(|arguments| {
                    arguments
                        .get("agent_type")
                        .or_else(|| arguments.get("agent_profile"))
                        .or_else(|| arguments.get("subagent"))
                })
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
        })
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

    if let Some(public_task_id) = public_task_id {
        if SessionId::parse_str(public_task_id).is_err() {
            return public_task_id.to_string();
        }
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
                    Value::String(String::from("task_session")),
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
                    (host_result.get("kind").and_then(Value::as_str) == Some("task_session"))
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
/// A higher score for a better match, or zero when the payload does not match.
/// 返回更高分代表更好匹配；不匹配时返回零。
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

    let payload_prompt = payload
        .get("prompt")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty());
    let payload_description = payload
        .get("description")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty());
    let payload_subagent = payload
        .get("subagent")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty());

    let mut score = 0usize;
    if payload_prompt
        .zip(task_session_prompt_text_from_tool_metadata(metadata))
        .is_some_and(|(left, right)| left == right)
    {
        score += 4;
    }
    if payload_description
        .zip(task_session_description_from_tool_metadata(metadata))
        .is_some_and(|(left, right)| left == right)
    {
        score += 2;
    }
    if payload_subagent
        .zip(task_session_subagent_from_tool_metadata(metadata))
        .is_some_and(|(left, right)| left == right)
    {
        score += 1;
    }

    score
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
        || metadata.get("kind").and_then(Value::as_str) == Some("task_session")
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
        "kind": "task_session",
        "payload": {
            "task_id": public_task_id,
            "task_session_id": task_id.to_string(),
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
            "kind": "task_session",
            "tool_display_state": "stopped",
            "payload": {
                "task_id": task_id.to_string(),
                "task_session_id": task_id.to_string(),
                "execution_status": "stopped",
                "result": stop_summary,
                "updated_at": updated_at,
            },
        })
    })
}

/// Builds the compact label used by one subagent dispatch row from its agent name and optional task description.
/// 根据代理名称与可选任务简介构建单条子代理调度行使用的紧凑标签。
pub fn subagent_navigation_task_label(state: &SubagentTaskPanelState) -> String {
    let label = state.subagent_label.trim();
    let description = state
        .description
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    match description {
        Some(description) if !label.contains(description) => {
            format!("{label}（{description}）")
        }
        _ if !label.is_empty() => label.to_string(),
        _ => String::from("子代理"),
    }
}

/// Converts one persisted execution-status string into the canonical subagent run state.
/// 将一条持久化 execution-status 字符串转换为规范子代理运行状态。
pub fn subagent_run_state_from_execution_status(status: Option<&str>) -> SubagentTaskRunState {
    match status.unwrap_or("completed") {
        "running" => SubagentTaskRunState::Running,
        "stopped" => SubagentTaskRunState::Stopped,
        "failed" => SubagentTaskRunState::Failed,
        _ => SubagentTaskRunState::Completed,
    }
}

/// Converts a persisted TaskSession status into the workbench-facing panel run state.
/// 将持久化 TaskSession 状态转换为工作台面板使用的运行状态。
pub fn subagent_run_state_from_task_session_status(
    status: TaskSessionStatus,
) -> SubagentTaskRunState {
    match status {
        TaskSessionStatus::Created
        | TaskSessionStatus::Running
        | TaskSessionStatus::WaitingChildren
        | TaskSessionStatus::WaitingFollowup
        | TaskSessionStatus::Compacting => SubagentTaskRunState::Running,
        TaskSessionStatus::Stopped => SubagentTaskRunState::Stopped,
        TaskSessionStatus::Failed => SubagentTaskRunState::Failed,
        TaskSessionStatus::Completed | TaskSessionStatus::Archived => {
            SubagentTaskRunState::Completed
        }
    }
}

/// Builds a shared workbench panel state from segmented TaskSession metadata and optional persisted context.
/// 根据分段 TaskSession 元数据和可选持久上下文构建共享工作台面板状态。
///
/// # Parameters
/// 参数
/// `metadata` is the canonical TaskSession metadata loaded from the task metadata file.
/// `metadata` 是从任务元数据文件加载的规范 TaskSession 元数据。
/// `context` is the optional persisted context document containing the reviewable conversation state.
/// `context` 是可选的持久上下文文档，里面包含可复看的会话状态。
/// `latest_event_payload` is the newest lifecycle payload used to recover full result text when available.
/// `latest_event_payload` 是最新生命周期载荷，用于在可用时恢复完整结果文本。
///
/// # Returns
/// 返回值
/// A display-ready subagent panel state, or `None` when the metadata has no direct parent.
/// 返回可直接显示的子代理面板状态；若元数据没有直接父级则返回 `None`。
pub fn subagent_task_panel_state_from_task_session(
    metadata: &TaskSessionMetadata,
    context: Option<&SessionContextDocument>,
    latest_event_payload: Option<&Value>,
) -> Option<SubagentTaskPanelState> {
    let parent_session_id = parent_session_id_from_task_session_parent(&metadata.parent)?;
    let conversation_state = context.and_then(|document| {
        serde_json::from_value::<SessionConversationState>(document.conversation_state.clone()).ok()
    });
    let result_text = latest_event_payload
        .and_then(|payload| payload.get("result"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| metadata.last_result_summary.clone())
        .unwrap_or_default();
    let subagent_label = task_session_tool_display_label(
        Some(metadata.display_title.as_str()),
        metadata.description.as_deref(),
        metadata.effective_config.subagent_profile_id.as_deref(),
        Some(metadata.public_task_id.as_str()),
        None,
    );

    Some(SubagentTaskPanelState {
        task_id: metadata.task_session_id.as_uuid(),
        public_task_id: metadata.public_task_id.clone(),
        parent_session_id,
        subagent_label,
        description: metadata.description.clone(),
        run_state: subagent_run_state_from_task_session_status(metadata.status),
        result_text,
        conversation_state,
    })
}

/// Loads direct child panel states from a segmented TaskSession store without reading every task event.
/// 从分段 TaskSession 存储加载直接子代理面板状态，并避免读取每个子任务的完整事件历史。
///
/// # Parameters
/// 参数
/// `store` is the segmented TaskSession store rooted at the main session directory.
/// `store` 是以主会话目录为根的分段 TaskSession 存储。
/// `parent_session_id` is the direct parent session whose child tasks should be listed.
/// `parent_session_id` 是需要列出子任务的直接父会话标识。
///
/// # Returns
/// 返回值
/// A best-effort list of display-ready child panel states; unreadable items are skipped.
/// 返回可直接显示的子代理面板状态列表；无法读取的条目会被跳过。
pub fn subagent_task_panel_states_from_segmented_task_session_store(
    store: &SegmentedTaskSessionStore,
    parent_session_id: SessionId,
) -> Vec<SubagentTaskPanelState> {
    let Some(parent) = subagent_parent_ref_from_task_session_store(store, parent_session_id) else {
        return Vec::new();
    };
    store
        .list_child_task_sessions(parent)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|metadata| subagent_task_panel_state_from_task_session(&metadata, None, None))
        .filter(|state| state.parent_session_id == parent_session_id)
        .collect()
}

/// Normalizes one cached panel state against the shared live runtime fact and an optional terminal persisted snapshot.
/// 根据共享实时运行事实与可选终态持久化快照，归一化单条缓存面板状态。
///
/// # Parameters
/// 参数
/// `state` is the cached panel state that may still contain a stale running flag.
/// `state` 是可能仍包含陈旧运行中标记的缓存面板状态。
/// `is_live_running` reports whether the shared TaskRuntime still owns the task abort handle.
/// `is_live_running` 表示共享 TaskRuntime 是否仍持有该任务的中止句柄。
/// `terminal_persisted_state` is a fresher non-running state loaded from segmented storage.
/// `terminal_persisted_state` 是从分段存储读取到的更新终态状态。
///
/// # Returns
/// 返回值
/// A panel state whose run-state matches the shared task lifecycle semantics.
/// 返回运行状态与共享任务生命周期语义一致的面板状态。
pub fn normalize_subagent_task_panel_state(
    state: &SubagentTaskPanelState,
    is_live_running: bool,
    terminal_persisted_state: Option<&SubagentTaskPanelState>,
) -> SubagentTaskPanelState {
    if is_live_running {
        let mut normalized_state = state.clone();
        normalized_state.run_state = SubagentTaskRunState::Running;
        return normalized_state;
    }
    if !state.run_state.is_running() {
        return state.clone();
    }
    if let Some(persisted_state) =
        terminal_persisted_state.filter(|state| !state.run_state.is_running())
    {
        return persisted_state.clone();
    }

    let mut normalized_state = state.clone();
    normalized_state.run_state =
        if normalized_state.result_text.trim() == SUBAGENT_TASK_STOPPED_SUMMARY_TEXT.trim() {
            SubagentTaskRunState::Stopped
        } else {
            SubagentTaskRunState::Completed
        };
    normalized_state
}

/// Merges segmented and previous live states into the canonical subagent panel cache.
/// 将分段存储与上一轮实时状态合并为规范子代理面板缓存。
///
/// # Parameters
/// 参数
/// `previous_states` is the frontend cache from the previous refresh.
/// `previous_states` 是前端上一轮刷新保留的缓存。
/// `segmented_states` are canonical TaskSession states loaded from the shared segmented store.
/// `segmented_states` 是从共享分段存储读取的规范 TaskSession 状态。
/// `is_task_live_running` is the host-supplied shared TaskRuntime lookup.
/// `is_task_live_running` 是宿主提供的共享 TaskRuntime 查询函数。
///
/// # Returns
/// 返回值
/// A task-id keyed cache map ready to replace the caller's UI cache bucket.
/// 返回按任务标识索引、可直接替换调用方 UI 缓存桶的映射。
pub fn merge_subagent_task_panel_cache<F>(
    previous_states: &BTreeMap<SessionId, SubagentTaskPanelState>,
    segmented_states: Vec<SubagentTaskPanelState>,
    mut is_task_live_running: F,
) -> BTreeMap<SessionId, SubagentTaskPanelState>
where
    F: FnMut(SessionId) -> bool,
{
    let persisted_states = segmented_states;
    let terminal_persisted_states = persisted_states
        .iter()
        .filter(|state| !state.run_state.is_running())
        .map(|state| (state.task_id, state.clone()))
        .collect::<BTreeMap<_, _>>();

    let mut merged = previous_states
        .iter()
        .filter_map(|(task_id, state)| {
            let is_live_running = is_task_live_running(*task_id);
            let normalized = normalize_subagent_task_panel_state(
                state,
                is_live_running,
                terminal_persisted_states.get(task_id),
            );
            (normalized.run_state.is_running() && is_live_running).then_some((*task_id, normalized))
        })
        .collect::<BTreeMap<_, _>>();

    for state in persisted_states {
        let normalized = if state.run_state.is_running() {
            previous_states
                .get(&state.task_id)
                .map(|existing| {
                    normalize_subagent_task_panel_state(
                        existing,
                        is_task_live_running(existing.task_id),
                        terminal_persisted_states.get(&existing.task_id),
                    )
                })
                .filter(|existing| !existing.run_state.is_running())
                .unwrap_or_else(|| {
                    normalize_subagent_task_panel_state(
                        &state,
                        is_task_live_running(state.task_id),
                        None,
                    )
                })
        } else {
            normalize_subagent_task_panel_state(
                &state,
                is_task_live_running(state.task_id),
                terminal_persisted_states.get(&state.task_id),
            )
        };
        merged.insert(normalized.task_id, normalized);
    }

    merged
}

/// Collects running subagent task states by walking the direct-child TaskSession tree from one parent.
/// 从一个父级开始遍历直接子 TaskSession 树，并收集仍在运行的子代理任务状态。
///
/// # Parameters
/// 参数
/// `root_parent_session_id` is the parent session whose child tree should be inspected.
/// `root_parent_session_id` 是需要检查其子任务树的父会话标识。
/// `child_states_for_parent` returns already refreshed and normalized direct-child task states for each visited parent.
/// `child_states_for_parent` 返回每个已访问父级下已经刷新并归一化的直接子任务状态。
///
/// # Returns
/// 返回值
/// A list of running child task states paired with the direct parent that owns each task card.
/// 返回仍在运行的子任务状态列表，并携带每个任务卡片所属的直接父级。
pub fn collect_running_subagent_task_states_for_tree<F>(
    root_parent_session_id: SessionId,
    mut child_states_for_parent: F,
) -> Vec<(SessionId, SubagentTaskPanelState)>
where
    F: FnMut(SessionId) -> Vec<SubagentTaskPanelState>,
{
    let mut running_states = Vec::new();
    let mut seen_parent_ids = BTreeSet::new();
    let mut pending_parent_ids = vec![root_parent_session_id];
    while let Some(parent_session_id) = pending_parent_ids.pop() {
        if !seen_parent_ids.insert(parent_session_id) {
            continue;
        }
        for state in child_states_for_parent(parent_session_id) {
            pending_parent_ids.push(state.task_id);
            if state.run_state.is_running() {
                running_states.push((parent_session_id, state));
            }
        }
    }
    running_states
}

/// Returns whether one shared panel state contains reviewable child conversation content.
/// 返回一条共享面板状态是否包含可复看的子会话内容。
///
/// # Parameters
/// 参数
/// `state` is the shared subagent panel state loaded from live runtime or the segmented TaskSession store.
/// `state` 是从实时运行时或分段 TaskSession 存储加载的共享子代理面板状态。
///
/// # Returns
/// 返回值
/// `true` when either canonical records or protocol messages can be projected into a child transcript.
/// 当规范记录或协议消息可以投影为子会话转录时返回 `true`。
pub fn subagent_task_panel_state_has_recoverable_conversation(
    state: &SubagentTaskPanelState,
) -> bool {
    state
        .conversation_state
        .as_ref()
        .is_some_and(|conversation_state| {
            !conversation_state.records.is_empty() || !conversation_state.messages.is_empty()
        })
}

/// Returns whether one normalized subagent panel may be opened by a UI surface.
/// 返回一条已归一化子代理面板是否允许被界面打开。
///
/// # Parameters
/// 参数
/// `state` is the normalized panel state shared by WebUI and service.
/// `state` 是 WebUI 与服务共用的已归一化面板状态。
/// `has_recoverable_detail` reports whether the host can still project a transcript for this task.
/// `has_recoverable_detail` 表示宿主是否仍能为该任务投影转录详情。
///
/// # Returns
/// 返回值
/// `true` when the task is running or has recoverable transcript detail.
/// 当任务仍在运行或仍有可复看的转录详情时返回 `true`。
pub fn subagent_task_panel_is_openable(
    state: &SubagentTaskPanelState,
    has_recoverable_detail: bool,
) -> bool {
    state.run_state.is_running() || has_recoverable_detail
}

/// Returns whether the subagent navigation surface should be exposed for the current parent coordination round.
/// 返回当前父级协调轮次是否应该暴露子代理导航界面。
///
/// # Parameters
/// 参数
/// `has_visible_round` reports whether the parent session still owns a recorded navigation round.
/// `has_visible_round` 表示父会话是否仍拥有已记录的导航轮次。
/// `has_running_subagents` reports whether any direct or nested child task is still running.
/// `has_running_subagents` 表示是否仍有任意直接或嵌套子任务处于运行中。
/// `has_active_subagent_panel` reports whether the UI is currently rendering one child task surface.
/// `has_active_subagent_panel` 表示界面当前是否正在渲染某个子任务界面。
///
/// # Returns
/// 返回值
/// `true` when a UI should show the subagent switcher or keep the parent-return route available.
/// 当界面应显示子代理切换器或保留返回父会话路径时返回 `true`。
pub fn subagent_navigation_surface_is_visible(
    has_visible_round: bool,
    has_running_subagents: bool,
    has_active_subagent_panel: bool,
) -> bool {
    has_visible_round || has_running_subagents || has_active_subagent_panel
}

/// Returns whether a parent coordination round should keep its subagent navigation state alive after one child event settles.
/// 返回某个父级协调轮次在单个子事件收敛后是否仍应保留子代理导航状态。
///
/// # Parameters
/// 参数
/// `has_active_parent_run` reports whether the parent session is still executing or preparing a run.
/// `has_active_parent_run` 表示父会话是否仍在执行或准备运行。
/// `has_running_subagents` reports whether any direct or nested child task is still running.
/// `has_running_subagents` 表示是否仍有任意直接或嵌套子任务处于运行中。
/// `_has_queued_parent_work` is ignored because parent queued prompts are tracked separately.
/// `_has_queued_parent_work` 会被忽略，因为父级排队提示词已单独跟踪。
///
/// # Returns
/// 返回值
/// `true` when the current round still owns blocking coordination work and should remain visible.
/// 当当前轮次仍拥有阻塞协调工作并应继续显示时返回 `true`。
pub fn subagent_navigation_round_should_remain_visible(
    has_active_parent_run: bool,
    has_running_subagents: bool,
    _has_queued_parent_work: bool,
) -> bool {
    has_active_parent_run || has_running_subagents
}

/// Selects the subagent states that should appear in a navigation surface for the current parent.
/// 为当前父级选择应该出现在子代理导航界面中的状态列表。
///
/// # Parameters
/// 参数
/// `normalized_states` is the already refreshed and normalized child panel state list.
/// `normalized_states` 是已经刷新并归一化的子面板状态列表。
/// `requested_active_task_id` is the task that is currently opened, when any.
/// `requested_active_task_id` 是当前已打开的任务标识；没有打开项时为空。
/// `has_recoverable_detail` reports whether a non-running active task can still be reviewed.
/// `has_recoverable_detail` 表示非运行中的活动任务是否仍可复看。
///
/// # Returns
/// 返回值
/// The visible navigation states plus the still-valid active task id.
/// 返回可见导航状态列表，以及仍然有效的活动任务标识。
pub fn select_subagent_navigation_panel_states<F>(
    normalized_states: &[SubagentTaskPanelState],
    requested_active_task_id: Option<SessionId>,
    mut has_recoverable_detail: F,
) -> (Vec<SubagentTaskPanelState>, Option<SessionId>)
where
    F: FnMut(&SubagentTaskPanelState) -> bool,
{
    let mut navigation_states = normalized_states
        .iter()
        .filter(|state| state.run_state.is_running())
        .cloned()
        .collect::<Vec<_>>();

    if let Some(active_task_id) = requested_active_task_id
        && !navigation_states
            .iter()
            .any(|state| state.task_id == active_task_id)
        && let Some(active_state) = normalized_states.iter().find(|state| {
            state.task_id == active_task_id
                && subagent_task_panel_is_openable(state, has_recoverable_detail(state))
        })
    {
        navigation_states.push(active_state.clone());
    }

    let active_task_id = requested_active_task_id.filter(|task_id| {
        navigation_states
            .iter()
            .any(|state| state.task_id == *task_id)
    });
    (navigation_states, active_task_id)
}

/// Resolves the persisted parent reference used to list direct children for either a root session or a nested TaskSession.
/// 为根会话或嵌套 TaskSession 解析列出直接子节点时使用的持久父级引用。
///
/// # Parameters
/// 参数
/// `store` is the shared TaskSession store.
/// `store` 是共享 TaskSession 存储。
/// `parent_session_id` is either the root main session id or a TaskSession id inside that root store.
/// `parent_session_id` 可以是根主会话标识，也可以是该根存储下的 TaskSession 标识。
///
/// # Returns
/// 返回值
/// The direct parent reference understood by `TaskSessionStore`, or `None` if metadata lookup fails unexpectedly.
/// 返回 `TaskSessionStore` 能理解的直接父级引用；若元数据读取异常则返回 `None`。
pub fn subagent_parent_ref_from_task_session_store<S>(
    store: &S,
    parent_session_id: SessionId,
) -> Option<ParentSessionRef>
where
    S: TaskSessionStore + ?Sized,
{
    let parent_node_id = SessionNodeId::from(parent_session_id);
    match store.load_task_session(parent_node_id) {
        Ok(Some(metadata)) => Some(ParentSessionRef::TaskSession {
            root_session_id: metadata.root_session_id,
            parent_task_session_id: parent_node_id,
        }),
        Ok(None) => Some(ParentSessionRef::MainSession {
            root_session_id: parent_node_id,
        }),
        Err(_) => None,
    }
}

/// Loads one child panel detail from a segmented TaskSession store using only the selected task id.
/// 仅使用选中的子任务标识从分段 TaskSession 存储加载单个子代理详情。
///
/// # Parameters
/// 参数
/// `store` is the segmented TaskSession store rooted at the main session directory.
/// `store` 是以主会话目录为根的分段 TaskSession 存储。
/// `task_id` is the internal TaskSession identifier to open.
/// `task_id` 是需要打开的内部 TaskSession 标识。
///
/// # Returns
/// 返回值
/// A detail-ready panel state with persisted context when available.
/// 返回包含可用持久上下文的详情面板状态。
pub fn subagent_task_panel_state_detail_from_segmented_task_session_store(
    store: &SegmentedTaskSessionStore,
    task_id: SessionNodeId,
) -> Option<SubagentTaskPanelState> {
    let metadata = store.load_task_session(task_id).ok().flatten()?;
    let context = store.load_context(task_id).ok().flatten();
    let latest_payload = store.load_latest_event_payload(task_id).ok().flatten();
    subagent_task_panel_state_from_task_session(
        &metadata,
        context.as_ref(),
        latest_payload.as_ref(),
    )
}

/// Resolves the direct parent identifier from a TaskSession parent reference.
/// 从 TaskSession 父级引用解析直接父级标识。
fn parent_session_id_from_task_session_parent(parent: &ParentSessionRef) -> Option<SessionId> {
    match parent {
        ParentSessionRef::None => None,
        ParentSessionRef::MainSession { root_session_id } => Some(root_session_id.as_uuid()),
        ParentSessionRef::TaskSession {
            parent_task_session_id,
            ..
        } => Some(parent_task_session_id.as_uuid()),
    }
}

/// Parses one persisted task-session payload object into a cacheable subagent task panel state.
/// 将一份持久化 task 会话载荷对象解析为可缓存的子代理任务面板状态。
pub fn parse_subagent_task_panel_payload(
    parent_session_id: SessionId,
    payload: &Value,
    prompt_preview: Option<&str>,
) -> Option<SubagentTaskPanelState> {
    let task_id = task_session_internal_id_from_payload(payload)?;
    let run_state = subagent_run_state_from_execution_status(
        payload.get("execution_status").and_then(Value::as_str),
    );
    let description = payload
        .get("description")
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|value| !value.trim().is_empty());
    let public_task_id = task_session_public_id_from_payload(payload)
        .map(str::to_string)
        .unwrap_or_else(|| task_id.to_string());
    let subagent_label = task_session_tool_display_label(
        task_session_display_title_from_payload(payload),
        description.as_deref(),
        payload.get("subagent").and_then(Value::as_str),
        Some(public_task_id.as_str()),
        prompt_preview,
    );

    Some(SubagentTaskPanelState {
        task_id,
        public_task_id,
        parent_session_id,
        subagent_label,
        description,
        run_state,
        result_text: payload
            .get("result")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_default(),
        // Tool-card payload snapshots are intentionally not treated as reviewable TaskSession transcripts; full child context must come from the segmented TaskSession store.
        // 工具卡片载荷快照会被有意排除在可复看 TaskSession 转录之外；完整子会话上下文必须来自分段 TaskSession 存储。
        conversation_state: None,
    })
}

/// Parses one merged task-session metadata object into a cacheable subagent task panel state.
/// 将一份合并后的 task 会话元数据对象解析为可缓存的子代理任务面板状态。
pub fn parse_subagent_task_panel_metadata(
    parent_session_id: SessionId,
    metadata: &Value,
    prompt_preview: Option<&str>,
) -> Option<SubagentTaskPanelState> {
    let payload = task_session_payload_from_tool_metadata(metadata)?;
    parse_subagent_task_panel_payload(parent_session_id, payload, prompt_preview)
}

/// Extracts the task prompt text from one merged task-session metadata object when the host payload keeps it.
/// 当宿主载荷保留了任务提示词时，从一份合并后的 task 会话元数据对象中提取该提示词文本。
pub fn parse_subagent_task_prompt_metadata(metadata: &Value) -> Option<String> {
    task_session_payload_from_tool_metadata(metadata)?
        .get("prompt")
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|value| !value.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::BTreeMap;
    use vulcan_context::{SessionMessageKind, conversation_state_from_session_messages};
    use vulcan_session_core::SessionNodeStore;

    /// Verifies shared task-session payload parsing keeps WebUI-visible labels and lifecycle state.
    /// 验证共享 task-session payload 解析会保留 WebUI 可见标签与生命周期状态。
    #[test]
    fn parses_task_session_payload_into_subagent_state() {
        let parent_session_id = SessionId::new_v4();
        let task_id = SessionId::new_v4();
        let payload = json!({
            "task_id": "task-public",
            "task_session_id": task_id.to_string(),
            "execution_status": "running",
            "description": "检查工程结构",
            "subagent": "explorer",
            "result": "",
        });

        let state = parse_subagent_task_panel_payload(parent_session_id, &payload, None)
            .expect("subagent state");

        assert_eq!(state.parent_session_id, parent_session_id);
        assert_eq!(state.task_id, task_id);
        assert_eq!(state.public_task_id, "task-public");
        assert_eq!(state.subagent_label, "检查工程结构（explorer）");
        assert_eq!(state.run_state, SubagentTaskRunState::Running);
    }

    /// Verifies in-flight task arguments and persisted payloads share one metadata helper surface.
    /// 验证进行中的 task 参数与持久化 payload 会共用同一套元数据辅助入口。
    #[test]
    fn extracts_task_session_fields_from_payload_or_arguments() {
        let payload_metadata = json!({
            "kind": "task_session",
            "payload": {
                "task_id": "public-a",
                "task_session_id": SessionId::new_v4().to_string(),
                "execution_status": "completed",
                "prompt": "执行检查",
                "description": "检查",
                "subagent": "worker",
            },
        });
        let argument_metadata = json!({
        "tool_name": "spawn_agent",
            "arguments": {
                "task_id": "public-b",
                "prompt": "执行测试",
                "description": "测试",
                "subagent": "tester",
            },
        });

        assert_eq!(
            task_session_declared_task_id_from_tool_metadata(&payload_metadata),
            Some("public-a")
        );
        assert_eq!(
            task_session_declared_task_id_from_tool_metadata(&argument_metadata),
            Some("public-b")
        );
        assert_eq!(
            task_session_subagent_from_tool_metadata(&argument_metadata),
            Some("tester")
        );
    }

    /// Verifies host-result task-session metadata uses the same shared payload helpers as direct delegate metadata.
    /// 验证 host-result task-session 元数据会使用与直接委派元数据相同的共享载荷辅助入口。
    #[test]
    fn extracts_task_session_fields_from_host_result_metadata() {
        let parent_session_id = SessionId::new_v4();
        let task_id = SessionId::new_v4();
        let metadata = json!({
            "tool_name": "spawn_agent",
            "host_result": {
                "kind": "task_session",
                "payload": {
                    "task_id": "public-host",
                    "task_session_id": task_id.to_string(),
                    "execution_status": "running",
                    "prompt": "生成图片",
                    "description": "图片子代理",
                    "subagent": "image-worker"
                }
            },
            "tool_display_state": "completed"
        });

        assert_eq!(
            task_session_declared_task_id_from_tool_metadata(&metadata),
            Some("public-host")
        );
        assert_eq!(
            task_session_execution_status_from_tool_metadata(&metadata),
            Some("running")
        );
        let state = parse_subagent_task_panel_metadata(parent_session_id, &metadata, None)
            .expect("host-result task-session metadata should parse");
        assert_eq!(state.task_id, task_id);
        assert_eq!(state.run_state, SubagentTaskRunState::Running);
        assert_eq!(state.subagent_label, "图片子代理（image-worker）");
    }

    /// Verifies the shared task-card lookup prefers canonical record matches over weaker projected-message matches.
    /// 验证共享 task 卡片查找会优先选择规范记录中的强匹配，而不是投影消息里的弱匹配。
    #[test]
    fn finds_task_session_tool_message_id_across_messages_and_records() {
        let task_session_id = SessionId::new_v4();
        let payload = json!({
            "task_id": "public-task",
            "task_session_id": task_session_id.to_string(),
            "prompt": "执行审核",
        });
        let weak_message = SessionMessageEntry {
            id: String::from("message-weak"),
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::ToolCallOutput,
            visible: true,
            title: String::from("task"),
            body: String::new(),
            metadata: Some(json!({
                "tool_name": "spawn_agent",
                "arguments": {
                    "prompt": "执行审核"
                }
            })),
            created_at: String::from("1"),
        };
        let exact_record = SessionConversationRecord {
            id: String::from("record-exact"),
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::ToolCallOutput,
            visible: true,
            title: String::from("task"),
            body: String::new(),
            metadata: Some(json!({
                "tool_name": "spawn_agent",
                "kind": "task_session",
                "payload": {
                    "task_id": "public-task",
                    "task_session_id": task_session_id.to_string(),
                    "execution_status": "completed"
                }
            })),
            created_at: String::from("2"),
        };

        let message_id = find_task_session_tool_message_id_for_payload(
            [&weak_message].into_iter(),
            [&exact_record].into_iter(),
            &payload,
        );

        assert_eq!(message_id.as_deref(), Some("record-exact"));
    }

    /// Verifies newly written stopped task metadata keeps only an index and summary.
    /// 验证新写入的停止态 task 元数据只保留索引与摘要。
    #[test]
    fn stopped_task_session_metadata_omits_embedded_conversation_state() {
        let task_id = SessionId::new_v4();

        let metadata = build_stopped_task_session_metadata(None, None, task_id, "已停止", "1000");

        assert_eq!(metadata["payload"]["task_session_id"], task_id.to_string());
        assert_eq!(metadata["payload"]["execution_status"], "stopped");
        assert!(
            metadata["payload"].get("conversation_state").is_none(),
            "new stopped metadata must not embed child transcripts"
        );
    }

    /// Verifies shared tool metadata merging keeps the most advanced task-session payload.
    /// 验证共享工具元数据合并会保留执行阶段最靠后的 task-session 载荷。
    #[test]
    fn merge_tool_display_metadata_preserves_advanced_task_session_payload() {
        let running_metadata = json!({
            "tool_name": "spawn_agent",
            "kind": "task_session",
            "payload": {
                "task_id": "reviewer",
                "task_session_id": "child-1",
                "execution_status": "running"
            }
        });
        let completed_metadata = json!({
            "tool_name": "spawn_agent",
            "kind": "task_session",
            "payload": {
                "task_id": "reviewer",
                "task_session_id": "child-1",
                "execution_status": "completed",
                "result": "done"
            }
        });

        let merged = merge_tool_display_metadata(
            Some(completed_metadata.clone()),
            Some(running_metadata.clone()),
        )
        .expect("metadata should merge");
        assert_eq!(
            merged
                .get("payload")
                .and_then(|payload| payload.get("execution_status"))
                .and_then(Value::as_str),
            Some("completed")
        );

        let merged = merge_tool_display_metadata(Some(running_metadata), Some(completed_metadata))
            .expect("metadata should merge");
        assert_eq!(
            merged
                .get("payload")
                .and_then(|payload| payload.get("execution_status"))
                .and_then(Value::as_str),
            Some("completed")
        );
    }

    /// Verifies metadata merging ranks host-result task-session payloads instead of trusting the generic completed display state.
    /// 验证元数据合并会按 host-result task-session 载荷排名，而不是信任通用的已完成显示状态。
    #[test]
    fn merge_tool_display_metadata_ranks_host_result_task_session_payload() {
        let running_metadata = json!({
            "tool_name": "spawn_agent",
            "tool_display_state": "completed",
            "host_result": {
                "kind": "task_session",
                "payload": {
                    "task_id": "reviewer",
                    "task_session_id": "child-1",
                    "execution_status": "running"
                }
            }
        });
        let completed_metadata = json!({
            "tool_name": "spawn_agent",
            "kind": "task_session",
            "payload": {
                "task_id": "reviewer",
                "task_session_id": "child-1",
                "execution_status": "completed",
                "result": "done"
            }
        });

        let merged = merge_tool_display_metadata(Some(completed_metadata), Some(running_metadata))
            .expect("metadata should merge");

        assert_eq!(
            task_session_payload_from_tool_metadata(&merged)
                .and_then(|payload| payload.get("execution_status"))
                .and_then(Value::as_str),
            Some("completed")
        );
    }

    /// Verifies spawn-agent card payloads cannot recover embedded child transcripts as full TaskSession panels.
    /// 验证 spawn_agent 工具卡片 payload 不能把内嵌子会话转录恢复为完整 TaskSession 面板。
    #[test]
    fn task_card_payload_ignores_embedded_conversation_state() {
        let parent_session_id = SessionId::new_v4();
        let task_id = SessionId::new_v4();
        let payload = json!({
            "task_id": "visible-task",
            "task_session_id": task_id.to_string(),
            "execution_status": "completed",
            "description": "旧任务",
            "subagent": "worker",
            "result": "工具卡片 payload 只保留返回摘要。",
            "conversation_state": SessionConversationState::default(),
        });

        let state = parse_subagent_task_panel_payload(parent_session_id, &payload, None)
            .expect("subagent panel state");

        assert_eq!(state.task_id, task_id);
        assert_eq!(state.result_text, "工具卡片 payload 只保留返回摘要。");
        assert!(
            state.conversation_state.is_none(),
            "tool-card payload snapshots must not become reviewable child transcripts"
        );
    }

    /// Verifies spawn-agent card payload parsing requires a canonical task_session_id.
    /// 验证 spawn_agent 工具卡片 payload 解析必须具备规范 task_session_id。
    #[test]
    fn task_card_payload_requires_task_session_id() {
        let parent_session_id = SessionId::new_v4();
        let payload = json!({
            "task_id": SessionId::new_v4().to_string(),
            "execution_status": "completed",
            "result": "缺少内部会话标识。",
        });

        assert!(
            parse_subagent_task_panel_payload(parent_session_id, &payload, None).is_none(),
            "task_id is a public task identifier and must not be treated as the internal session id"
        );
    }

    /// Verifies segmented TaskSession metadata can produce the same panel state used by both WebUI and service.
    /// 验证分段 TaskSession 元数据可以生成 WebUI 与服务共用的同一面板状态。
    #[test]
    fn builds_panel_state_from_task_session_metadata() {
        let root_session_id = vulcan_session_core::SessionNodeId::new_v4();
        let task_session_id = vulcan_session_core::SessionNodeId::new_v4();
        let mut metadata = TaskSessionMetadata::new(
            root_session_id,
            task_session_id,
            ParentSessionRef::MainSession { root_session_id },
            "audit-task",
            "代码审核",
            vulcan_session_core::SessionRequestedConfig::default(),
            vulcan_session_core::SessionEffectiveConfig::default(),
            1,
            10,
        );
        metadata.status = TaskSessionStatus::Completed;
        metadata.description = Some(String::from("审核当前代码"));
        metadata.last_result_summary = Some(String::from("摘要结果"));
        let context = SessionContextDocument {
            schema_version: 1,
            node_id: task_session_id,
            provider_id: None,
            model_id: None,
            reasoning_effort: None,
            system_prompt_layers: Vec::new(),
            conversation_state: json!(SessionConversationState::default()),
            compaction_anchor: None,
            context_budget_fingerprint: None,
            tool_catalog_snapshot: Value::Null,
            provider_request_preview: None,
        };

        let state = subagent_task_panel_state_from_task_session(&metadata, Some(&context), None)
            .expect("panel state");

        assert_eq!(state.parent_session_id, root_session_id.as_uuid());
        assert_eq!(state.task_id, task_session_id.as_uuid());
        assert_eq!(state.public_task_id, "audit-task");
        assert_eq!(state.subagent_label, "代码审核");
        assert_eq!(state.run_state, SubagentTaskRunState::Completed);
        assert!(state.conversation_state.is_some());
    }

    /// Verifies the segmented list route uses metadata only and leaves detail loading to the selected task.
    /// 验证分段列表路由仅使用元数据，并将详情加载留给选中的任务。
    #[test]
    fn loads_panel_states_from_segmented_task_session_store() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let store = vulcan_task_session::SegmentedTaskSessionStore::new(temp_dir.path());
        let root_session_id = vulcan_session_core::SessionNodeId::new_v4();
        let task_session_id = vulcan_session_core::SessionNodeId::new_v4();
        let mut metadata = TaskSessionMetadata::new(
            root_session_id,
            task_session_id,
            ParentSessionRef::MainSession { root_session_id },
            "audit-task",
            "代码审核",
            vulcan_session_core::SessionRequestedConfig::default(),
            vulcan_session_core::SessionEffectiveConfig::default(),
            1,
            10,
        );
        metadata.status = TaskSessionStatus::Completed;
        store
            .create_task_session(metadata)
            .expect("create segmented task session");
        store
            .append_event(vulcan_session_core::SessionEventRecord::new(
                "event-1",
                task_session_id,
                0,
                vulcan_session_core::SessionEventKind::AssistantOutput,
                20,
                json!({ "result": "完整结果" }),
            ))
            .expect("append segmented task event");
        let conversation_state =
            conversation_state_from_session_messages(&[SessionMessageEntry::new(
                SessionMessageKind::Assistant,
                "AI",
                "完整结果",
            )
            .with_runtime_metadata("assistant-1", "20")]);
        store
            .save_context(SessionContextDocument {
                schema_version: 1,
                node_id: task_session_id,
                provider_id: None,
                model_id: None,
                reasoning_effort: None,
                system_prompt_layers: Vec::new(),
                conversation_state: json!(conversation_state),
                compaction_anchor: None,
                context_budget_fingerprint: None,
                tool_catalog_snapshot: Value::Null,
                provider_request_preview: None,
            })
            .expect("save segmented context");

        let states = subagent_task_panel_states_from_segmented_task_session_store(
            &store,
            root_session_id.as_uuid(),
        );

        assert_eq!(states.len(), 1);
        assert_eq!(states[0].task_id, task_session_id.as_uuid());
        assert!(states[0].result_text.is_empty());
        assert!(!subagent_task_panel_state_has_recoverable_conversation(
            &states[0]
        ));

        let detail = subagent_task_panel_state_detail_from_segmented_task_session_store(
            &store,
            task_session_id,
        )
        .expect("detail state");
        assert_eq!(detail.result_text, "完整结果");
        assert!(subagent_task_panel_state_has_recoverable_conversation(
            &detail
        ));
    }

    /// Verifies shared running-task collection walks nested child trees and skips terminal children.
    /// 验证共享运行任务收集逻辑会遍历嵌套子树，并跳过已进入终态的子任务。
    #[test]
    fn collects_running_subagent_task_states_for_tree() {
        let root_session_id = SessionId::new_v4();
        let parent_task_id = SessionId::new_v4();
        let running_child_task_id = SessionId::new_v4();
        let completed_child_task_id = SessionId::new_v4();
        let build_state =
            |task_id: SessionId, run_state: SubagentTaskRunState| SubagentTaskPanelState {
                task_id,
                public_task_id: task_id.to_string(),
                parent_session_id: root_session_id,
                subagent_label: String::from("worker"),
                description: None,
                run_state,
                result_text: String::new(),
                conversation_state: None,
            };

        let running_states =
            collect_running_subagent_task_states_for_tree(root_session_id, |parent_id| {
                if parent_id == root_session_id {
                    vec![build_state(parent_task_id, SubagentTaskRunState::Running)]
                } else if parent_id == parent_task_id {
                    vec![
                        build_state(running_child_task_id, SubagentTaskRunState::Running),
                        build_state(completed_child_task_id, SubagentTaskRunState::Completed),
                    ]
                } else {
                    Vec::new()
                }
            });

        assert_eq!(running_states.len(), 2);
        assert_eq!(running_states[0].0, root_session_id);
        assert_eq!(running_states[0].1.task_id, parent_task_id);
        assert_eq!(running_states[1].0, parent_task_id);
        assert_eq!(running_states[1].1.task_id, running_child_task_id);
    }

    /// Verifies shared cache merging preserves live entries while using segmented TaskSession states as the persisted source.
    /// 验证共享缓存合并会保留实时条目，并以分段 TaskSession 状态作为持久来源。
    #[test]
    fn merges_subagent_task_panel_cache_from_segmented_and_live_states() {
        let parent_session_id = SessionId::new_v4();
        let live_task_id = SessionId::new_v4();
        let stale_task_id = SessionId::new_v4();
        let segmented_task_id = SessionId::new_v4();
        let mut previous_states = BTreeMap::new();
        previous_states.insert(
            live_task_id,
            test_subagent_state(
                parent_session_id,
                live_task_id,
                SubagentTaskRunState::Running,
            ),
        );
        previous_states.insert(
            stale_task_id,
            test_subagent_state(
                parent_session_id,
                stale_task_id,
                SubagentTaskRunState::Running,
            ),
        );

        let mut segmented_state = test_subagent_state(
            parent_session_id,
            segmented_task_id,
            SubagentTaskRunState::Completed,
        );
        segmented_state.result_text = String::from("segmented result");

        let merged =
            merge_subagent_task_panel_cache(&previous_states, vec![segmented_state], |task_id| {
                task_id == live_task_id
            });

        assert_eq!(
            merged.get(&live_task_id).map(|state| state.run_state),
            Some(SubagentTaskRunState::Running)
        );
        assert_eq!(
            merged.get(&stale_task_id).map(|state| state.run_state),
            None
        );
        assert_eq!(
            merged
                .get(&segmented_task_id)
                .map(|state| state.result_text.as_str()),
            Some("segmented result")
        );
    }

    /// Verifies live runtime state wins over stale completed payloads.
    /// 验证实时运行态优先于陈旧的已完成载荷。
    #[test]
    fn live_runtime_state_overrides_stale_completed_payload() {
        let parent_session_id = SessionId::new_v4();
        let task_id = SessionId::new_v4();
        let mut completed_state =
            test_subagent_state(parent_session_id, task_id, SubagentTaskRunState::Completed);
        completed_state.result_text = String::from("stale completed result");

        let normalized = normalize_subagent_task_panel_state(&completed_state, true, None);

        assert_eq!(normalized.run_state, SubagentTaskRunState::Running);

        let merged = merge_subagent_task_panel_cache(
            &BTreeMap::new(),
            vec![completed_state],
            |candidate_task_id| candidate_task_id == task_id,
        );

        assert_eq!(
            merged.get(&task_id).map(|state| state.run_state),
            Some(SubagentTaskRunState::Running)
        );
    }

    /// Verifies recoverable child conversations may come from protocol messages even when records are empty.
    /// 验证即使 records 为空，也可以从协议 messages 判断子会话内容可复看。
    #[test]
    fn panel_state_recoverable_conversation_accepts_messages() {
        let task_id = SessionId::new_v4();
        let mut conversation_state = SessionConversationState::default();
        vulcan_context::append_assistant_text_message(
            &mut conversation_state,
            "assistant-1",
            "done",
        );
        let state = SubagentTaskPanelState {
            task_id,
            public_task_id: task_id.to_string(),
            parent_session_id: SessionId::new_v4(),
            subagent_label: String::from("worker"),
            description: None,
            run_state: SubagentTaskRunState::Completed,
            result_text: String::new(),
            conversation_state: Some(conversation_state),
        };

        assert!(subagent_task_panel_state_has_recoverable_conversation(
            &state
        ));
    }

    /// Verifies shared navigation selection keeps running tasks and the opened reviewable terminal task.
    /// 验证共享导航选择会保留运行中任务以及当前打开且可复看的终态任务。
    #[test]
    fn selects_subagent_navigation_states_with_active_recoverable_terminal_task() {
        let parent_session_id = SessionId::new_v4();
        let running_task_id = SessionId::new_v4();
        let completed_task_id = SessionId::new_v4();
        let hidden_task_id = SessionId::new_v4();
        let states = vec![
            test_subagent_state(
                parent_session_id,
                running_task_id,
                SubagentTaskRunState::Running,
            ),
            test_subagent_state(
                parent_session_id,
                completed_task_id,
                SubagentTaskRunState::Completed,
            ),
            test_subagent_state(
                parent_session_id,
                hidden_task_id,
                SubagentTaskRunState::Completed,
            ),
        ];

        let (navigation_states, active_task_id) =
            select_subagent_navigation_panel_states(&states, Some(completed_task_id), |state| {
                state.task_id == completed_task_id
            });

        assert_eq!(active_task_id, Some(completed_task_id));
        assert_eq!(navigation_states.len(), 2);
        assert!(
            navigation_states
                .iter()
                .any(|state| state.task_id == running_task_id)
        );
        assert!(
            navigation_states
                .iter()
                .any(|state| state.task_id == completed_task_id)
        );
        assert!(
            !navigation_states
                .iter()
                .any(|state| state.task_id == hidden_task_id)
        );
    }

    /// Verifies the shared navigation visibility gate preserves the active child return route without keeping stale inactive history visible.
    /// 验证共享导航可见性门禁会保留活动子界面的返回路径，同时不会让无活动历史常驻显示。
    #[test]
    fn subagent_navigation_surface_visibility_matches_current_round() {
        assert!(!subagent_navigation_surface_is_visible(false, false, false));
        assert!(subagent_navigation_surface_is_visible(true, false, false));
        assert!(subagent_navigation_surface_is_visible(false, true, false));
        assert!(subagent_navigation_surface_is_visible(false, false, true));
    }

    /// Verifies the shared round retention gate only keeps navigation while parent coordination work remains.
    /// 验证共享轮次保留门禁只会在父级协调工作仍存在时继续保留导航。
    #[test]
    fn subagent_navigation_round_visibility_tracks_coordination_work() {
        assert!(!subagent_navigation_round_should_remain_visible(
            false, false, false
        ));
        assert!(subagent_navigation_round_should_remain_visible(
            true, false, false
        ));
        assert!(subagent_navigation_round_should_remain_visible(
            false, true, false
        ));
        assert!(!subagent_navigation_round_should_remain_visible(
            false, false, true
        ));
    }

    /// Verifies the segmented store route lists nested TaskSessions by their direct task-session parent.
    /// 验证分段存储路由可以按直接 TaskSession 父级列出嵌套子代理。
    #[test]
    fn loads_nested_panel_states_from_segmented_task_session_store() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let store = vulcan_task_session::SegmentedTaskSessionStore::new(temp_dir.path());
        let root_session_id = vulcan_session_core::SessionNodeId::new_v4();
        let parent_task_session_id = vulcan_session_core::SessionNodeId::new_v4();
        let child_task_session_id = vulcan_session_core::SessionNodeId::new_v4();
        store
            .create_task_session(TaskSessionMetadata::new(
                root_session_id,
                parent_task_session_id,
                ParentSessionRef::MainSession { root_session_id },
                "parent-task",
                "父子代理",
                vulcan_session_core::SessionRequestedConfig::default(),
                vulcan_session_core::SessionEffectiveConfig::default(),
                1,
                10,
            ))
            .expect("create parent task session");
        store
            .create_task_session(TaskSessionMetadata::new(
                root_session_id,
                child_task_session_id,
                ParentSessionRef::TaskSession {
                    root_session_id,
                    parent_task_session_id,
                },
                "child-task",
                "嵌套子代理",
                vulcan_session_core::SessionRequestedConfig::default(),
                vulcan_session_core::SessionEffectiveConfig::default(),
                2,
                20,
            ))
            .expect("create child task session");

        let root_states = subagent_task_panel_states_from_segmented_task_session_store(
            &store,
            root_session_id.as_uuid(),
        );
        let nested_states = subagent_task_panel_states_from_segmented_task_session_store(
            &store,
            parent_task_session_id.as_uuid(),
        );

        assert_eq!(root_states.len(), 1);
        assert_eq!(root_states[0].task_id, parent_task_session_id.as_uuid());
        assert_eq!(nested_states.len(), 1);
        assert_eq!(
            nested_states[0].parent_session_id,
            parent_task_session_id.as_uuid()
        );
        assert_eq!(nested_states[0].task_id, child_task_session_id.as_uuid());
    }

    /// Verifies restart-time list projection restores completed state from metadata without reading task JSONL files.
    /// 验证重启时的列表投影会从元数据恢复完成态，而不读取子任务 JSONL 文件。
    #[test]
    fn restores_completed_panel_state_from_segmented_task_session_store() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let store = vulcan_task_session::SegmentedTaskSessionStore::new(temp_dir.path());
        let root_session_id = vulcan_session_core::SessionNodeId::new_v4();
        let task_session_id = vulcan_session_core::SessionNodeId::new_v4();
        let mut effective_config = vulcan_session_core::SessionEffectiveConfig::default();
        effective_config.subagent_profile_id = Some(String::from("image-agent"));
        let mut metadata = TaskSessionMetadata::new(
            root_session_id,
            task_session_id,
            ParentSessionRef::MainSession { root_session_id },
            "image-worker-a",
            "生成图片 A",
            vulcan_session_core::SessionRequestedConfig::default(),
            effective_config,
            1,
            10,
        );
        metadata.description = Some(String::from("生成湖景图片"));
        store
            .create_task_session(metadata.clone())
            .expect("create task session");
        metadata.status = TaskSessionStatus::Completed;
        metadata.last_result_summary = Some(String::from("metadata fallback result"));
        metadata.updated_at = 20;
        store
            .save_task_session(metadata)
            .expect("save completed metadata");
        store
            .append_event(vulcan_session_core::SessionEventRecord::new(
                "task-result",
                task_session_id,
                1,
                vulcan_session_core::SessionEventKind::TaskFeedback,
                20,
                json!({ "result": "latest event image result" }),
            ))
            .expect("append latest result event");

        let states = subagent_task_panel_states_from_segmented_task_session_store(
            &store,
            root_session_id.as_uuid(),
        );

        assert_eq!(states.len(), 1);
        assert_eq!(states[0].task_id, task_session_id.as_uuid());
        assert_eq!(states[0].public_task_id, "image-worker-a");
        assert_eq!(states[0].run_state, SubagentTaskRunState::Completed);
        assert_eq!(states[0].description.as_deref(), Some("生成湖景图片"));
        assert_eq!(states[0].result_text, "metadata fallback result");
        assert_eq!(states[0].subagent_label, "生成图片 A");

        let detail = subagent_task_panel_state_detail_from_segmented_task_session_store(
            &store,
            task_session_id,
        )
        .expect("detail state");
        assert_eq!(detail.result_text, "latest event image result");
    }

    /// Builds one compact test panel state with the requested lifecycle state.
    /// 构建一条带指定生命周期状态的紧凑测试面板状态。
    fn test_subagent_state(
        parent_session_id: SessionId,
        task_id: SessionId,
        run_state: SubagentTaskRunState,
    ) -> SubagentTaskPanelState {
        SubagentTaskPanelState {
            task_id,
            public_task_id: task_id.to_string(),
            parent_session_id,
            subagent_label: String::from("worker"),
            description: None,
            run_state,
            result_text: String::new(),
            conversation_state: None,
        }
    }
}
