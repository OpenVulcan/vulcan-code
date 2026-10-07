use super::*;
use crate::{
    SessionTurnStats, SessionTurnStatsKind, SessionTurnStatsStatus, SessionTurnUsageSegment,
};
use vulcan_context::SessionMessageKind;

/// Canonical host-result kind for child-agent control operations.
/// 智能体控制操作的规范宿主结果类型。
const SUBAGENT_CONTROL_KIND: &str = "subagent_control";

/// Structured display projection for one parent-to-child `send_message` tool result.
/// 父级发往子智能体的 `send_message` 工具结果对应的结构化显示投影。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubagentSendMessageProjection {
    /// Stable child task session id that receives the visible message row.
    /// 接收可见消息行的稳定子任务会话标识。
    pub task_session_id: SessionId,
    /// Stable display record id used to de-duplicate the projected row.
    /// 用于去重该投影行的稳定显示记录标识。
    pub message_id: String,
    /// User-visible message text sent to the child agent.
    /// 发送给子智能体的用户可见消息文本。
    pub message: String,
    /// Display-only metadata preserved on the child transcript row.
    /// 保存在子会话转录行上的仅显示元数据。
    pub display_metadata: Value,
}

/// Structured close-agent projection parsed from one `subagent_control` host result.
/// 从单个 `subagent_control` 宿主结果解析出的结构化关闭智能体投影。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubagentCloseAgentProjection {
    /// Task sessions that the close_agent call archived through matched registry entries.
    /// `close_agent` 通过匹配注册表条目归档的任务会话集合。
    pub matched_task_ids: Vec<SessionId>,
    /// Active task sessions that were stopped while closing the target tree.
    /// 关闭目标树时被停止的活动任务会话集合。
    pub stopped_task_ids: Vec<SessionId>,
    /// Union of matched and stopped task sessions that UI caches should remove.
    /// UI 缓存需要移除的匹配与停止任务会话并集。
    pub affected_task_ids: Vec<SessionId>,
}

/// Parent-visible display records generated from one completed child-agent execution.
/// 从一次已完成智能体执行生成的父会话可见显示记录。
#[derive(Debug, Clone)]
pub struct SubagentFeedbackDisplayRecords {
    /// Stable child execution id used to de-duplicate feedback rows.
    /// 用于反馈行去重的稳定智能体执行标识。
    pub execution_id: String,
    /// Display-only feedback card row that keeps child-agent result text in metadata.
    /// 将智能体结果文本保存在元数据中的仅显示反馈卡片行。
    pub feedback_entry: SessionMessageEntry,
    /// Display-only statistics row that contains the child-agent turn usage.
    /// 包含智能体轮次用量的仅显示统计行。
    pub stats_entry: SessionMessageEntry,
    /// Strongly typed inclusive statistics used for parent-ledger contribution materialization.
    /// 用于父账本贡献物化的强类型包容性统计。
    pub turn_stats: SessionTurnStats,
    /// Canonical child Turn Stats record that produced the inclusive contribution.
    /// 产生包容性贡献的规范子任务 Turn Stats 记录。
    pub source_turn_stats_record_id: Option<String>,
}

/// Task mirror metadata update parts used to repair a parent spawn-agent tool card.
/// 用于修复父级 spawn-agent 工具卡片的任务镜像元数据更新部件。
#[derive(Debug, Clone, PartialEq)]
pub struct TaskSessionMirrorDisplayMetadataUpdate {
    /// Payload used to locate the matching parent task-card records.
    /// 用于定位匹配父级任务卡片记录的载荷。
    pub payload: Value,
    /// Metadata update written back to matching projected and canonical rows.
    /// 写回匹配投影行与规范行的元数据更新。
    pub metadata_update: Value,
}

/// Builds the parent-visible feedback rows for one completed child-agent execution.
/// 为一次已完成智能体执行构建父会话可见反馈行。
///
/// # Parameters
/// 参数
/// `parent_session_id` identifies the parent transcript that owns the display rows.
/// `parent_session_id` 标识拥有这些显示行的父级转录。
/// `metadata` is the delegate-finished metadata that carries the child execution payload.
/// `metadata` 是携带智能体执行载荷的委派完成元数据。
/// `state` is the normalized child panel state used for labels and visible result text.
/// `state` 是用于标签与可见结果文本的规范化智能体面板状态。
///
/// # Returns
/// 返回值
/// The two display records and their de-duplication execution id.
/// 返回两条显示记录及其去重执行标识。
pub fn subagent_feedback_display_records_from_tool_metadata(
    parent_session_id: SessionId,
    metadata: &Value,
    state: &SubagentTaskPanelState,
) -> Result<SubagentFeedbackDisplayRecords, String> {
    let execution_id = subagent_feedback_execution_id_from_tool_metadata(metadata)?;
    let status = subagent_feedback_stats_status(state.run_state);
    let turn_stats = subagent_feedback_turn_stats_from_tool_metadata(metadata, status)?;
    let source_turn_stats_record_id = task_session_payload_from_tool_metadata(metadata)
        .and_then(|payload| payload.get("source_turn_stats_record_id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let task_id_text = state.task_id.to_string();
    let result_text = state.result_text.trim();
    let feedback_payload = json!({
        "tool": "subagent_feedback",
        (SUBAGENT_TASK_ID_KEY): task_id_text,
        "execution_id": execution_id,
        "task_id": state.public_task_id.clone(),
        "subagent": state.subagent_label.clone(),
        "run_state": state.run_state.metadata_key(),
        "result": result_text,
    });
    let mut feedback_entry = SessionMessageEntry::new(
        SessionMessageKind::Assistant,
        "智能体返回处理结果",
        "智能体返回处理结果",
    );
    feedback_entry.record_kind = SessionMessageRecordKind::DisplayNote;
    feedback_entry.metadata = Some(json!({
        "display_note": true,
        "tool_name": "subagent_feedback",
        "display_state": state.run_state.metadata_key(),
        "tool_display_state": state.run_state.metadata_key(),
        "kind": "subagent_feedback",
        "subagent_feedback_record": true,
        "parent_session_id": parent_session_id.to_string(),
        (SUBAGENT_TASK_ID_KEY): state.task_id.to_string(),
        "execution_id": execution_id,
        "task_id": state.public_task_id.clone(),
        "subagent": state.subagent_label.clone(),
        "run_state": state.run_state.metadata_key(),
        "result": result_text,
        "host_result": {
            "kind": "subagent_feedback",
            "tool_name": "subagent_feedback",
            "payload": feedback_payload,
        },
    }));

    let mut stats_entry = SessionMessageEntry::new(
        SessionMessageKind::Assistant,
        format!("智能体统计 · {}", state.subagent_label),
        format!(
            "智能体：{}\n任务：{}\n\n{}",
            state.subagent_label,
            state.public_task_id,
            turn_stats.detail_body()
        ),
    );
    stats_entry.record_kind = SessionMessageRecordKind::TurnStats;
    stats_entry.metadata = Some(json!({
        "kind": "subagent_feedback_stats",
        "subagent_feedback_record": true,
        "parent_session_id": parent_session_id.to_string(),
        (SUBAGENT_TASK_ID_KEY): state.task_id.to_string(),
        "execution_id": execution_id,
        "task_id": state.public_task_id.clone(),
        "subagent": state.subagent_label.clone(),
        "turn_stats": turn_stats,
    }));

    Ok(SubagentFeedbackDisplayRecords {
        execution_id,
        feedback_entry,
        stats_entry,
        turn_stats,
        source_turn_stats_record_id,
    })
}

/// Builds the task-session metadata update used by a persisted task mirror.
/// 构建持久化任务镜像使用的 task-session 元数据更新。
///
/// # Parameters
/// 参数
/// `task_session_id` is the child task session id restored from the mirror.
/// `task_session_id` 是从镜像恢复出的智能体任务会话标识。
/// `execution_status` is the normalized display status selected by the caller.
/// `execution_status` 是调用方选定的规范展示状态。
/// `line_count` is the mirrored child segment line count.
/// `line_count` 是镜像的智能体分段行数。
/// `updated_at` is the mirrored update timestamp.
/// `updated_at` 是镜像的更新时间戳。
///
/// # Returns
/// 返回值
/// Matching payload and status-synchronized metadata update for parent tool-card repair.
/// 返回用于父级工具卡片修复的匹配载荷与状态同步后的元数据更新。
pub fn task_session_mirror_display_metadata_update(
    task_session_id: SessionId,
    execution_status: &str,
    line_count: u64,
    updated_at: &str,
) -> TaskSessionMirrorDisplayMetadataUpdate {
    let payload = json!({
        "task_id": task_session_id.to_string(),
        (SUBAGENT_TASK_ID_KEY): task_session_id.to_string(),
        "execution_status": execution_status,
        "line_count": line_count,
        "updated_at": updated_at,
    });
    let metadata_update = task_session_display_metadata_update(
        json!({
            "tool_name": "spawn_agent",
            "kind": SUBAGENT_TASK_KIND,
            "payload": payload,
        }),
        execution_status,
    );
    TaskSessionMirrorDisplayMetadataUpdate {
        payload,
        metadata_update,
    }
}

/// Returns whether one metadata slot already belongs to a display-only child-agent feedback row.
/// 判断单个元数据槽是否已经属于仅显示的智能体反馈行。
///
/// # Parameters
/// 参数
/// `record_metadata` is the metadata attached to a candidate transcript record.
/// `record_metadata` 是附着在候选转录记录上的元数据。
/// `task_id` is the internal child task session id encoded as text.
/// `task_id` 是文本形式的内部智能体任务会话标识。
/// `execution_id` is the concrete child execution id to match.
/// `execution_id` 是需要匹配的具体智能体执行标识。
///
/// # Returns
/// 返回值
/// `true` when the metadata belongs to the same child execution feedback record.
/// 当该元数据属于同一次智能体执行反馈记录时返回 `true`。
pub fn subagent_feedback_record_matches(
    record_metadata: Option<&Value>,
    task_id: &str,
    execution_id: &str,
) -> bool {
    record_metadata.is_some_and(|metadata| {
        metadata
            .get("subagent_feedback_record")
            .and_then(Value::as_bool)
            .unwrap_or(false)
            && metadata.get(SUBAGENT_TASK_ID_KEY).and_then(Value::as_str) == Some(task_id)
            && metadata.get("execution_id").and_then(Value::as_str) == Some(execution_id)
    })
}

/// Reads the concrete child execution id from a delegate-finished metadata payload.
/// 从委派完成元数据载荷中读取具体智能体执行标识。
pub fn subagent_feedback_execution_id_from_tool_metadata(
    metadata: &Value,
) -> Result<String, String> {
    let Some(payload) = task_session_payload_from_tool_metadata(metadata) else {
        return Err(String::from(
            "subagent feedback metadata is missing the task payload.",
        ));
    };
    let Some(execution_id) = payload
        .get("execution_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Err(String::from(
            "subagent feedback metadata is missing execution_id.",
        ));
    };
    Ok(execution_id.to_string())
}

/// Parses child-agent usage segments from one delegate-finished metadata payload.
/// 从单条委派完成元数据载荷中解析智能体用量分段。
fn subagent_feedback_usage_segments_from_tool_metadata(
    metadata: &Value,
) -> Result<Vec<SessionTurnUsageSegment>, String> {
    let Some(payload) = task_session_payload_from_tool_metadata(metadata) else {
        return Ok(Vec::new());
    };
    let Some(usage_segments) = payload.get("usage_segments") else {
        return Ok(Vec::new());
    };
    serde_json::from_value::<Vec<SessionTurnUsageSegment>>(usage_segments.clone())
        .map_err(|error| format!("failed to parse subagent feedback usage segments: {error}"))
}

/// Reads the strongly typed child statistic, with legacy segment reconstruction for old sessions.
/// 读取强类型子统计，并仅为旧会话保留分段重建兼容。
///
/// # Parameters
/// 参数
/// `metadata` is the exact delegate-finished or wait-agent result metadata.
/// `metadata` 是精确的委派完成或 wait-agent 结果元数据。
/// `status` is the current terminal panel status projected for the parent.
/// `status` 是投影给父级的当前终态面板状态。
///
/// # Returns
/// 返回值
/// Returns the ledger-derived inclusive statistic when present, otherwise a legacy reconstruction.
/// 存在时返回账本派生的包容性统计，否则返回旧版重建结果。
fn subagent_feedback_turn_stats_from_tool_metadata(
    metadata: &Value,
    status: SessionTurnStatsStatus,
) -> Result<SessionTurnStats, String> {
    if let Some(turn_stats) = task_session_payload_from_tool_metadata(metadata)
        .and_then(|payload| payload.get("turn_stats"))
    {
        let mut turn_stats = serde_json::from_value::<SessionTurnStats>(turn_stats.clone())
            .map_err(|error| format!("failed to parse subagent feedback Turn Stats: {error}"))?;
        turn_stats.status = status;
        turn_stats.turn_kind = SessionTurnStatsKind::SubagentFeedback;
        return Ok(turn_stats);
    }
    Ok(subagent_feedback_turn_stats(
        status,
        subagent_feedback_usage_segments_from_tool_metadata(metadata)?,
    ))
}

/// Maps one child-agent panel state into the terminal status used by display-only statistics.
/// 将单个智能体面板状态映射为仅显示统计使用的终态状态。
fn subagent_feedback_stats_status(run_state: SubagentTaskRunState) -> SessionTurnStatsStatus {
    match run_state {
        SubagentTaskRunState::Failed => SessionTurnStatsStatus::Failed,
        SubagentTaskRunState::Stopped => SessionTurnStatsStatus::Stopped,
        SubagentTaskRunState::Running
        | SubagentTaskRunState::Completed
        | SubagentTaskRunState::Closed => SessionTurnStatsStatus::Completed,
    }
}

/// Builds one display-only statistics object from child-agent usage segments.
/// 根据智能体用量分段构建一份仅显示的统计对象。
fn subagent_feedback_turn_stats(
    status: SessionTurnStatsStatus,
    usage_segments: Vec<SessionTurnUsageSegment>,
) -> SessionTurnStats {
    let mut stats = SessionTurnStats {
        status,
        turn_kind: SessionTurnStatsKind::SubagentFeedback,
        ..SessionTurnStats::default()
    };
    for segment in usage_segments {
        stats.merge_usage_segment(&segment);
        stats.usage_segments.push(segment);
    }
    stats
}

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

/// Parses one completed tool metadata object into a close-agent projection when it carries a close_agent host result.
/// 当已完成工具元数据携带 close_agent 宿主结果时，将其解析为关闭智能体投影。
///
/// # Parameters
/// 参数
/// `metadata` is the completed runtime tool metadata that may contain a `subagent_control` host result.
/// `metadata` 是可能包含 `subagent_control` 宿主结果的已完成运行时工具元数据。
///
/// # Returns
/// 返回值
/// A close-agent projection, `None` for unrelated metadata, or an error for malformed identified close_agent payloads.
/// 返回关闭智能体投影；无关元数据返回 `None`；已识别 close_agent 载荷损坏时返回错误。
pub fn subagent_close_agent_projection_from_tool_metadata(
    metadata: &Value,
) -> Result<Option<SubagentCloseAgentProjection>, String> {
    let Some(host_result) = metadata.get("host_result").and_then(Value::as_object) else {
        return Ok(None);
    };
    let Some(kind) = host_result.get("kind").and_then(Value::as_str) else {
        return Ok(None);
    };
    if kind != SUBAGENT_CONTROL_KIND {
        return Ok(None);
    }
    let payload = host_result
        .get("payload")
        .filter(|payload| payload.is_object())
        .ok_or_else(|| String::from("subagent_control host result is missing object payload"))?;
    subagent_close_agent_projection_from_host_result(kind, payload)
}

/// Parses one host-result kind and payload pair into a close-agent projection.
/// 将一组宿主结果类型与载荷解析为关闭智能体投影。
///
/// # Parameters
/// 参数
/// `kind` is the host-result kind reported by the runtime tool result.
/// `kind` 是运行时工具结果报告的宿主结果类型。
/// `payload` is the host-result payload reported by close_agent.
/// `payload` 是 close_agent 报告的宿主结果载荷。
///
/// # Returns
/// 返回值
/// A close-agent projection, `None` for non-close subagent controls, or an error for malformed close_agent payloads.
/// 返回关闭智能体投影；非 close 控制结果返回 `None`；close_agent 载荷损坏时返回错误。
pub fn subagent_close_agent_projection_from_host_result(
    kind: &str,
    payload: &Value,
) -> Result<Option<SubagentCloseAgentProjection>, String> {
    if kind != SUBAGENT_CONTROL_KIND {
        return Ok(None);
    }
    let tool_name = payload
        .get("tool")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| String::from("subagent_control payload is missing tool"))?;
    if tool_name != CLOSE_AGENT_TOOL {
        return Ok(None);
    }
    let matched_agents = payload
        .get("matched_agents")
        .and_then(Value::as_array)
        .ok_or_else(|| String::from("close_agent host result is missing matched_agents"))?;
    let matched_task_ids = matched_agents
        .iter()
        .map(|agent| subagent_task_id_from_payload_value(agent, "close_agent matched agent"))
        .collect::<Result<Vec<_>, _>>()?;
    if matched_task_ids.is_empty() {
        return Err(String::from(
            "close_agent did not report any closed agent tasks",
        ));
    }
    let stopped_task_values = payload
        .get(STOPPED_SUBAGENT_TASK_IDS_KEY)
        .and_then(Value::as_array)
        .ok_or_else(|| {
            String::from("close_agent host result is missing stopped_subagent_task_ids")
        })?;
    let stopped_task_ids = stopped_task_values
        .iter()
        .map(|task_id| subagent_task_id_from_scalar_value(task_id, STOPPED_SUBAGENT_TASK_IDS_KEY))
        .collect::<Result<Vec<_>, _>>()?;
    let mut affected_task_ids = BTreeSet::new();
    affected_task_ids.extend(matched_task_ids.iter().copied());
    affected_task_ids.extend(stopped_task_ids.iter().copied());
    Ok(Some(SubagentCloseAgentProjection {
        matched_task_ids,
        stopped_task_ids,
        affected_task_ids: affected_task_ids.into_iter().collect(),
    }))
}

/// Parses one parent-to-child `send_message` host-result metadata object into a visible child transcript projection.
/// 将一份父级发往子智能体的 `send_message` 宿主结果元数据解析为可见子转录投影。
///
/// # Parameters
/// 参数
/// `metadata` is the completed tool event metadata that may contain a `subagent_message` host result.
/// `metadata` 是可能包含 `subagent_message` 宿主结果的已完成工具事件元数据。
/// `event_title` is the runtime event title used only for legacy display-id recovery.
/// `event_title` 是仅用于旧显示标识恢复的运行时事件标题。
///
/// # Returns
/// 返回值
/// A parsed projection for child-targeted `send_message`, `None` for unrelated metadata, or an error for malformed identified payloads.
/// 对子智能体目标的 `send_message` 返回解析后的投影；无关元数据返回 `None`；已识别载荷损坏时返回错误。
pub fn subagent_send_message_projection_from_tool_metadata(
    metadata: &Value,
    event_title: &str,
) -> Result<Option<SubagentSendMessageProjection>, String> {
    let Some(host_result) = metadata.get("host_result").and_then(Value::as_object) else {
        return Ok(None);
    };
    if host_result.get("kind").and_then(Value::as_str) != Some("subagent_message") {
        return Ok(None);
    }
    let payload = host_result
        .get("payload")
        .filter(|payload| payload.is_object())
        .ok_or_else(|| String::from("subagent_message host result is missing object payload"))?;
    let tool_name = payload
        .get("tool")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| String::from("subagent_message payload is missing tool"))?;
    if tool_name != SEND_MESSAGE_TOOL {
        return Ok(None);
    }
    if payload.get("target").and_then(Value::as_str) == Some("root") {
        return Ok(None);
    }
    let task_id_text = payload
        .get(SUBAGENT_TASK_ID_KEY)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| String::from("subagent send_message payload is missing task_session_id"))?;
    let task_session_id = task_id_text.parse::<SessionId>().map_err(|error| {
        format!(
            "subagent send_message payload has invalid task_session_id `{task_id_text}`: {error}"
        )
    })?;
    let message = payload
        .get("message")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| String::from("subagent send_message payload is missing message"))?
        .to_string();
    let message_id =
        subagent_send_message_display_record_id_from_metadata(payload, metadata, event_title)
            .ok_or_else(|| String::from("subagent send_message payload is missing delivery id"))?;
    let display_metadata = json!({
        "display_note": true,
        "kind": "subagent_message",
        "tool": SEND_MESSAGE_TOOL,
        "parent_session_id": payload.get("parent_session_id").cloned(),
        "target": payload.get("target").cloned(),
        (SUBAGENT_TASK_ID_KEY): task_session_id.to_string(),
        "delivery_id": payload.get("delivery_id").cloned(),
        "delivery_mode": payload.get("delivery_mode").cloned(),
        "queued": payload.get("queued").cloned(),
        "interrupted": payload.get("interrupted").cloned(),
        "continues_task_turn": payload.get("continues_task_turn").cloned(),
    });
    Ok(Some(SubagentSendMessageProjection {
        task_session_id,
        message_id,
        message,
        display_metadata,
    }))
}

/// Returns whether one display row is a UI-only parent-to-child send_message note.
/// 返回单条显示行是否为仅 UI 使用的父级到子智能体 send_message 记录。
///
/// # Parameters
/// 参数
/// `message` is the display transcript row to classify.
/// `message` 是需要分类的显示转录行。
///
/// # Returns
/// 返回值
/// `true` when the row matches the canonical subagent send_message display-note metadata.
/// 当该行符合规范智能体 send_message display note 元数据时返回 `true`。
pub fn session_message_is_subagent_send_message_display_note(
    message: &SessionMessageEntry,
) -> bool {
    message.record_kind == SessionMessageRecordKind::DisplayNote
        && subagent_send_message_display_note_metadata(message.metadata.as_ref())
}

/// Returns whether one canonical conversation record is a UI-only parent-to-child send_message note.
/// 返回单条规范会话记录是否为仅 UI 使用的父级到子智能体 send_message 记录。
///
/// # Parameters
/// 参数
/// `record` is the canonical conversation record to classify.
/// `record` 是需要分类的规范会话记录。
///
/// # Returns
/// 返回值
/// `true` when the record matches the canonical subagent send_message display-note metadata.
/// 当该记录符合规范智能体 send_message display note 元数据时返回 `true`。
pub fn session_conversation_record_is_subagent_send_message_display_note(
    record: &SessionConversationRecord,
) -> bool {
    record.visible
        && record.record_kind == SessionMessageRecordKind::DisplayNote
        && subagent_send_message_display_note_metadata(record.metadata.as_ref())
}

/// Returns whether one metadata payload carries the canonical parent-to-child send_message identity.
/// 返回单个元数据载荷是否携带规范父级到子智能体 send_message 身份。
fn subagent_send_message_display_note_metadata(metadata: Option<&Value>) -> bool {
    metadata.is_some_and(|metadata| {
        metadata.get("kind").and_then(Value::as_str) == Some("subagent_message")
            && metadata.get("tool").and_then(Value::as_str) == Some(SEND_MESSAGE_TOOL)
    })
}

/// Resolves the stable display record id used to de-duplicate a projected `send_message` prompt.
/// 解析用于去重投影 `send_message` 提示词的稳定显示记录标识。
fn subagent_send_message_display_record_id_from_metadata(
    payload: &Value,
    metadata: &Value,
    event_title: &str,
) -> Option<String> {
    if let Some(delivery_id) = payload
        .get("delivery_id")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
    {
        return Some(delivery_id.to_string());
    }
    metadata
        .get("tool_call_id")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .or_else(|| parse_tool_finished_event_title_call_id(event_title))
}

/// Parses the standard runtime tool-finished title into a tool-call id.
/// 将标准运行时工具完成标题解析为工具调用标识。
fn parse_tool_finished_event_title_call_id(title: &str) -> Option<String> {
    let mut segments = title.split(" · ");
    match (segments.next(), segments.next()) {
        (Some("工具返回"), Some(tool_call_id)) if !tool_call_id.trim().is_empty() => {
            Some(tool_call_id.trim().to_string())
        }
        _ => None,
    }
}

/// Parses the canonical child task id from one object payload.
/// 从单个对象载荷中解析规范子任务标识。
fn subagent_task_id_from_payload_value(value: &Value, label: &str) -> Result<SessionId, String> {
    let Some(task_id) = value
        .get(SUBAGENT_TASK_ID_KEY)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Err(format!("{label} is missing subagent_task_id"));
    };
    task_id
        .parse::<SessionId>()
        .map_err(|error| format!("{label} has invalid subagent_task_id `{task_id}`: {error}"))
}

/// Parses the canonical child task id from one scalar payload value.
/// 从单个标量载荷值中解析规范子任务标识。
fn subagent_task_id_from_scalar_value(value: &Value, label: &str) -> Result<SessionId, String> {
    let Some(task_id) = value
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Err(format!("{label} contains a non-string task id"));
    };
    task_id
        .parse::<SessionId>()
        .map_err(|error| format!("{label} contains invalid task id `{task_id}`: {error}"))
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
    String::from("智能体")
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

/// Builds task-session display metadata with every status alias kept in sync.
/// 构建 task-session 显示元数据，并保持所有状态别名同步。
///
/// # Parameters
/// 参数
/// `metadata_update` is the base task-session metadata update.
/// `metadata_update` 是基础 task-session 元数据更新。
/// `execution_status` is the canonical display state derived by the caller.
/// `execution_status` 是调用方推导出的规范显示状态。
///
/// # Returns
/// 返回值
/// Metadata update with top-level and payload-level display states aligned.
/// 返回顶层与 payload 层显示状态已对齐的元数据更新。
pub fn task_session_display_metadata_update(
    mut metadata_update: Value,
    execution_status: &str,
) -> Value {
    let Some(object) = metadata_update.as_object_mut() else {
        return json!({
            "tool_name": "spawn_agent",
            "kind": SUBAGENT_TASK_KIND,
            "display_state": execution_status,
            "tool_display_state": execution_status,
        });
    };
    object.insert(
        String::from("display_state"),
        Value::String(execution_status.to_string()),
    );
    object.insert(
        String::from("tool_display_state"),
        Value::String(execution_status.to_string()),
    );
    if let Some(payload) = object.get_mut("payload").and_then(Value::as_object_mut) {
        payload.insert(
            String::from("execution_status"),
            Value::String(execution_status.to_string()),
        );
    }
    if let Some(host_payload) = object
        .get_mut("host_result")
        .and_then(Value::as_object_mut)
        .and_then(|host_result| host_result.get_mut("payload"))
        .and_then(Value::as_object_mut)
    {
        host_payload.insert(
            String::from("execution_status"),
            Value::String(execution_status.to_string()),
        );
    }
    metadata_update
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
                (Some(_), None) => {
                    task_session_payload_from_metadata_object(&tool_call_object).cloned()
                }
                (None, Some(_)) => {
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

/// Merges task-mirror display metadata while allowing live running state to override stale terminal payloads.
/// 合并任务镜像展示元数据，并允许实时运行态覆盖陈旧终态载荷。
///
/// # Parameters
/// 参数
/// `current_metadata` is the metadata already attached to the parent tool card.
/// `current_metadata` 是父级工具卡片已有的元数据。
/// `metadata_update` is the task-mirror-derived display metadata.
/// `metadata_update` 是从任务镜像推导出的展示元数据。
/// `live_running_override` indicates that the host runtime still owns active task work.
/// `live_running_override` 表示宿主运行时仍持有活动任务。
///
/// # Returns
/// 返回值
/// Merged metadata for the parent tool card.
/// 返回父级工具卡片的合并后元数据。
pub fn merge_task_mirror_tool_display_metadata(
    current_metadata: Option<Value>,
    metadata_update: Value,
    live_running_override: bool,
) -> Option<Value> {
    if !live_running_override {
        return merge_tool_display_metadata(current_metadata, Some(metadata_update));
    }
    match (current_metadata, metadata_update) {
        (Some(Value::Object(mut current_object)), Value::Object(update_object)) => {
            current_object.extend(update_object);
            Some(Value::Object(current_object))
        }
        (_, update) => Some(update),
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
        "created" | "running" | "waiting_children" | "waiting_followup" | "compacting" => 0,
        "completed" | "stopped" | "failed" | "archived" => 2,
        _ => return None,
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
    if let (Some(payload_internal_task_id), Some(metadata_internal_task_id)) =
        (payload_internal_task_id, metadata_internal_task_id)
    {
        return if payload_internal_task_id == metadata_internal_task_id {
            usize::MAX
        } else {
            0
        };
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
/// A metadata object with `execution_id`, `execution_status`, and `tool_display_state` set for stopped delivery.
/// 返回为停止投递设置 `execution_id`、`execution_status` 与 `tool_display_state` 的元数据对象。
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
    let execution_id = format!("stopped-{task_id}-{updated_at}");
    let metadata = json!({
        "tool_name": "spawn_agent",
        "kind": SUBAGENT_TASK_KIND,
        "payload": {
            "task_id": public_task_id,
            (SUBAGENT_TASK_ID_KEY): task_id.to_string(),
            "execution_id": execution_id,
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
                "execution_id": execution_id,
                "execution_status": "stopped",
                "result": stop_summary,
                "updated_at": updated_at,
            },
        })
    })
}
