//! Shared subagent task-session projection helpers reused by iced and WebUI.
//! 供 iced 与 WebUI 复用的共享子代理 task-session 投影辅助逻辑。

use crate::SessionId;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use vulcan_context::SessionConversationState;

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

/// Cached subagent task panel state reconstructed from runtime delegate events and persisted task tool results.
/// 由运行时委派事件与持久化 task 工具结果重建出的子代理任务面板缓存状态。
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

/// Returns the structured task-session payload attached to one tool-display metadata object when the tool result belongs to the host-managed `task` surface.
/// 当工具结果属于宿主管理的 `task` 表面时，返回其工具显示元数据对象中附带的结构化 task-session 载荷。
pub fn task_session_payload_from_tool_metadata(metadata: &Value) -> Option<&Value> {
    (metadata.get("kind").and_then(Value::as_str) == Some("task_session")).then_some(())?;
    metadata.get("payload")
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

/// Returns the internal child-session identifier stored inside one task-session payload, falling back to legacy payloads that only persisted `task_id`.
/// 返回 task 会话载荷中保存的内部子会话标识；若旧版本只持久化了 `task_id`，则回退兼容解析。
pub fn task_session_internal_id_from_payload(payload: &Value) -> Option<SessionId> {
    payload
        .get("task_session_id")
        .and_then(Value::as_str)
        .or_else(|| payload.get("task_id").and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .and_then(|value| SessionId::parse_str(value).ok())
}

/// Returns the original `task` tool arguments attached to one tool-display metadata object when the card is still in flight.
/// 当卡片仍处于进行中状态时，返回其工具显示元数据对象中附带的原始 `task` 工具参数。
pub fn task_session_arguments_from_tool_metadata(metadata: &Value) -> Option<&Value> {
    (metadata.get("tool_name").and_then(Value::as_str) == Some("task")).then_some(())?;
    metadata.get("arguments")
}

/// Returns the best-effort caller-facing task identifier attached to one task card from either persisted payload metadata or in-flight tool arguments.
/// 从持久化载荷元数据或进行中工具参数中返回一条最佳努力的调用方面向任务标识。
pub fn task_session_declared_task_id_from_tool_metadata(metadata: &Value) -> Option<&str> {
    task_session_payload_from_tool_metadata(metadata)
        .and_then(task_session_public_id_from_payload)
        .or_else(|| {
            task_session_arguments_from_tool_metadata(metadata)
                .and_then(|arguments| arguments.get("task_id"))
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
                .and_then(|arguments| arguments.get("prompt"))
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
                .and_then(|arguments| arguments.get("subagent"))
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
    let conversation_state = state_snapshot.and_then(|state| state.conversation_state.clone());
    let metadata = json!({
        "tool_name": "task",
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
            "conversation_state": conversation_state,
        },
    });
    attach_tool_display_state_to_metadata(Some(metadata), "stopped").unwrap_or_else(|| {
        json!({
            "tool_name": "task",
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

/// Resolves the localized terminal status label injected into delegated completion follow-up prompts.
/// 解析注入到委派完成跟进提示词中的本地化终态标签。
pub fn delegate_completion_status_label(run_state: SubagentTaskRunState) -> &'static str {
    match run_state {
        SubagentTaskRunState::Running => "仍在进行中",
        SubagentTaskRunState::Completed => "已完成",
        SubagentTaskRunState::Stopped => "已停止",
        SubagentTaskRunState::Failed => "已失败",
    }
}

/// Builds one queue-strip preview line for a delegated subagent completion callback.
/// 为一次委派子代理完成回调构建队列条预览文案。
pub fn build_delegate_completion_followup_preview(state: &SubagentTaskPanelState) -> String {
    format!(
        "子代理 {} {}",
        state.subagent_label,
        delegate_completion_status_label(state.run_state)
    )
}

/// Builds one synthetic follow-up prompt that tells the parent coordinator a delegated subagent has reported back.
/// 构建一条合成跟进提示词，用于告知父协调代理某个委派子代理已经回报结果。
pub fn build_delegate_completion_followup_prompt(state: &SubagentTaskPanelState) -> String {
    let result_text = state.result_text.trim();
    let safe_result = if result_text.is_empty() {
        "当前没有更多结构化返回内容，请根据任务状态继续主流程。"
    } else {
        result_text
    };
    format!(
        "<system-reminder>\n子代理“{}”已{}。\n任务标识：{}。\n\n子代理返回内容：\n{}\n\n请立即吸收这条结果，并继续推进主流程；如果还有其他子代理仍在执行，请同时维持协调与收敛。\n</system-reminder>",
        state.subagent_label,
        delegate_completion_status_label(state.run_state),
        state.public_task_id,
        safe_result
    )
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
        conversation_state: payload
            .get("conversation_state")
            .cloned()
            .and_then(|value| serde_json::from_value(value).ok()),
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

    /// Verifies shared task-session payload parsing keeps iced-visible labels and lifecycle state.
    /// 验证共享 task-session payload 解析会保留 iced 可见标签与生命周期状态。
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
            "tool_name": "task",
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
}
