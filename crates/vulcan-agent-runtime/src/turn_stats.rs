//! Shared turn-statistics construction for service and workbench transcript lifecycles.
//! 面向服务与工作台转录生命周期共享的轮次统计构建逻辑。

use serde_json::Value;
use vulcan_workbench_model::{
    SessionMessageKind, SessionMessageRecordKind, SessionTurnStats, SessionTurnStatsKind,
    SessionTurnStatsStatus, SessionTurnUsageSegment, SessionTurnUsageSegmentKind,
};

use crate::conversation_protocol::{
    SessionConversationMessage, SessionConversationPayload, SessionConversationRecord,
    SessionConversationState,
};

/// Builds one best-effort turn-statistics snapshot from the transcript rows bounded by the provided terminal closing record.
/// 根据给定终态闭合记录所界定的转录行，构建一份最佳努力的轮次统计快照。
pub fn build_session_turn_stats_from_state(
    conversation_state: &SessionConversationState,
    closing_record_id: &str,
    status: SessionTurnStatsStatus,
    provider_request_count_override: Option<usize>,
) -> Option<SessionTurnStats> {
    build_session_turn_stats_from_state_with_segments(
        conversation_state,
        closing_record_id,
        status,
        provider_request_count_override,
        SessionTurnStatsKind::UserTurn,
        Vec::new(),
    )
}

/// Builds one best-effort turn-statistics snapshot and merges explicit usage segments into the aggregate.
/// 构建一份最佳努力轮次统计快照，并将显式用量分段合并进聚合值。
pub fn build_session_turn_stats_from_state_with_segments(
    conversation_state: &SessionConversationState,
    closing_record_id: &str,
    status: SessionTurnStatsStatus,
    provider_request_count_override: Option<usize>,
    turn_kind: SessionTurnStatsKind,
    mut explicit_usage_segments: Vec<SessionTurnUsageSegment>,
) -> Option<SessionTurnStats> {
    let records = &conversation_state.records;
    let closing_index = records
        .iter()
        .position(|record| record.id == closing_record_id)?;
    let boundary_index = records[..closing_index]
        .iter()
        .rposition(record_closes_parent_turn_boundary)
        .map(|index| index + 1)
        .unwrap_or(0);
    let default_start_index = records[boundary_index..=closing_index]
        .iter()
        .position(record_starts_user_turn)
        .map(|offset| boundary_index + offset)
        .unwrap_or(boundary_index);
    let start_index = if turn_kind == SessionTurnStatsKind::ContextCompaction {
        context_compaction_turn_start_index(records, closing_index).unwrap_or(default_start_index)
    } else {
        default_start_index
    };
    let turn_records = &records[start_index..=closing_index];
    if turn_records.is_empty() {
        return None;
    }
    let include_assistant_response_usage = turn_kind == SessionTurnStatsKind::UserTurn;
    let turn_start_message_index = if include_assistant_response_usage {
        let turn_start_user_record = turn_records
            .iter()
            .find(|record| record_starts_user_turn(record));
        let visible_user_record_count_before_turn = records[..start_index]
            .iter()
            .filter(|record| record_starts_user_turn(record))
            .count();
        match turn_start_user_record {
            Some(user_record) => canonical_turn_start_message_index(
                conversation_state,
                visible_user_record_count_before_turn,
                user_record,
            )
            .unwrap_or(conversation_state.messages.len()),
            None => fallback_turn_start_message_index_without_user_record(conversation_state),
        }
    } else {
        conversation_state.messages.len()
    };
    let turn_messages = &conversation_state.messages[turn_start_message_index..];

    let mut stats = SessionTurnStats {
        status,
        turn_kind,
        usage_segments: Vec::new(),
        usage_groups: Vec::new(),
        provider_call_count: 0,
        unknown_usage_call_count: 0,
        source_ledger_revision: 0,
        child_contribution_revision: 0,
        contains_estimated_budget: false,
        tool_call_count: 0,
        input_tokens: 0,
        uncached_input_tokens: None,
        output_tokens: 0,
        reasoning_tokens: 0,
        total_tokens: 0,
        cached_input_tokens: None,
        cache_creation_input_tokens: None,
        duration_ms: None,
        output_tokens_per_second: None,
    };

    if turn_kind == SessionTurnStatsKind::UserTurn {
        stats.tool_call_count = count_turn_tool_calls_from_records(turn_records);
    }

    let mut assistant_segment = SessionTurnUsageSegment {
        kind: SessionTurnUsageSegmentKind::AssistantResponse,
        trigger: None,
        execution_profile: None,
        provider_id: None,
        provider_name_snapshot: None,
        model_id: None,
        model_name_snapshot: None,
        provider_call_count: 0,
        tool_call_count: 0,
        input_tokens: 0,
        uncached_input_tokens: None,
        output_tokens: 0,
        reasoning_tokens: 0,
        total_tokens: 0,
        cached_input_tokens: None,
        cache_creation_input_tokens: None,
    };

    for message in turn_messages {
        match &message.payload {
            SessionConversationPayload::Assistant {
                usage, tool_calls, ..
            } => {
                if assistant_payload_contributes_to_turn_stats(message) {
                    stats.provider_call_count = stats.provider_call_count.saturating_add(1);
                    assistant_segment.provider_call_count =
                        assistant_segment.provider_call_count.saturating_add(1);
                }
                if stats.tool_call_count == 0 {
                    stats.tool_call_count = stats.tool_call_count.saturating_add(tool_calls.len());
                }
                let Some(usage) = usage.as_ref() else {
                    continue;
                };
                stats.input_tokens = stats
                    .input_tokens
                    .saturating_add(usage.input_tokens.unwrap_or(0));
                assistant_segment.input_tokens = assistant_segment
                    .input_tokens
                    .saturating_add(usage.input_tokens.unwrap_or(0));
                if let Some(uncached_input_tokens) = usage.uncached_input_tokens {
                    let accumulated = stats.uncached_input_tokens.unwrap_or(0);
                    stats.uncached_input_tokens =
                        Some(accumulated.saturating_add(uncached_input_tokens));
                    let segment_accumulated = assistant_segment.uncached_input_tokens.unwrap_or(0);
                    assistant_segment.uncached_input_tokens =
                        Some(segment_accumulated.saturating_add(uncached_input_tokens));
                }
                stats.output_tokens = stats
                    .output_tokens
                    .saturating_add(usage.output_tokens.unwrap_or(0));
                assistant_segment.output_tokens = assistant_segment
                    .output_tokens
                    .saturating_add(usage.output_tokens.unwrap_or(0));
                stats.reasoning_tokens = stats
                    .reasoning_tokens
                    .saturating_add(usage.reasoning_tokens.unwrap_or(0));
                assistant_segment.reasoning_tokens = assistant_segment
                    .reasoning_tokens
                    .saturating_add(usage.reasoning_tokens.unwrap_or(0));
                // Canonical output already includes thinking; retain the reported total, including explicit zero.
                // 统一输出已经包含思考；保留报告总额，包括明确的零值。
                let total_tokens = usage.total_tokens.unwrap_or_else(|| {
                    usage
                        .input_tokens
                        .unwrap_or(0)
                        .saturating_add(usage.output_tokens.unwrap_or(0))
                });
                stats.total_tokens = stats.total_tokens.saturating_add(total_tokens);
                assistant_segment.total_tokens =
                    assistant_segment.total_tokens.saturating_add(total_tokens);
                if let Some(cached_input_tokens) = usage.cached_input_tokens {
                    let accumulated = stats.cached_input_tokens.unwrap_or(0);
                    stats.cached_input_tokens =
                        Some(accumulated.saturating_add(cached_input_tokens));
                    let segment_accumulated = assistant_segment.cached_input_tokens.unwrap_or(0);
                    assistant_segment.cached_input_tokens =
                        Some(segment_accumulated.saturating_add(cached_input_tokens));
                }
                if let Some(cache_creation_input_tokens) = usage.cache_creation_input_tokens {
                    let accumulated = stats.cache_creation_input_tokens.unwrap_or(0);
                    stats.cache_creation_input_tokens =
                        Some(accumulated.saturating_add(cache_creation_input_tokens));
                    let segment_accumulated =
                        assistant_segment.cache_creation_input_tokens.unwrap_or(0);
                    assistant_segment.cache_creation_input_tokens =
                        Some(segment_accumulated.saturating_add(cache_creation_input_tokens));
                }
            }
            SessionConversationPayload::ToolResult { .. }
            | SessionConversationPayload::UserText { .. }
            | SessionConversationPayload::AgentMessage { .. } => {}
        }
    }

    if let Some(provider_request_count_override) = provider_request_count_override
        && provider_request_count_override > 0
    {
        stats.provider_call_count = provider_request_count_override;
        if turn_kind == SessionTurnStatsKind::UserTurn {
            assistant_segment.provider_call_count = provider_request_count_override;
        }
    }

    if assistant_segment.provider_call_count > 0
        || assistant_segment.input_tokens > 0
        || assistant_segment.uncached_input_tokens.is_some()
        || assistant_segment.output_tokens > 0
        || assistant_segment.reasoning_tokens > 0
        || assistant_segment.total_tokens > 0
        || assistant_segment.tool_call_count > 0
        || assistant_segment.cached_input_tokens.is_some()
        || assistant_segment.cache_creation_input_tokens.is_some()
    {
        stats.merge_usage_segment_group(&assistant_segment);
        stats.usage_segments.push(assistant_segment);
    }
    // Child usage belongs to the root lifetime ledger, never the parent's Turn.
    // 子代理用量属于根生命周期累计，不属于父会话回合。
    for segment in explicit_usage_segments
        .drain(..)
        .filter(|segment| segment.kind != SessionTurnUsageSegmentKind::SubagentTask)
    {
        merge_explicit_usage_segment_into_turn_stats(&mut stats, &segment);
        stats.usage_segments.push(segment);
    }

    let started_at = turn_records
        .iter()
        .find(|record| {
            record.id != closing_record_id
                && record.record_kind != SessionMessageRecordKind::SummaryOutput
                && record.record_kind != SessionMessageRecordKind::TurnTerminated
                && record.record_kind != SessionMessageRecordKind::TurnStats
                && !record.created_at.trim().is_empty()
        })
        .and_then(|record| parse_unix_millis_text(&record.created_at))
        .or_else(|| {
            turn_records
                .iter()
                .find(|record| !record.created_at.trim().is_empty())
                .and_then(|record| parse_unix_millis_text(&record.created_at))
        });
    let finished_at = turn_records
        .iter()
        .rev()
        .find(|record| record.id == closing_record_id)
        .and_then(|record| parse_unix_millis_text(&record.created_at));
    stats.duration_ms = match (started_at, finished_at) {
        (Some(started_at), Some(finished_at)) if finished_at >= started_at => {
            Some((finished_at - started_at) as u64)
        }
        _ => None,
    };
    // Legacy turn records lack paired per-request first-token checkpoints; elapsed turn time is not output time.
    // 旧回合记录缺少配对的逐请求首 Token 时间点；整轮耗时不等于输出时长。
    stats.output_tokens_per_second = None;

    Some(stats)
}

/// Merges one explicit usage segment into aggregate turn statistics.
/// 将一个显式用量分段合并到聚合轮次统计中。
///
/// # Parameters
/// 参数
/// `stats` is the aggregate turn-statistics snapshot being updated.
/// `stats` 是正在更新的聚合轮次统计快照。
/// `segment` is one provider, tool, or child-agent usage segment.
/// `segment` 是单个 Provider、工具或智能体用量分段。
pub fn merge_usage_segment_into_turn_stats(
    stats: &mut SessionTurnStats,
    segment: &SessionTurnUsageSegment,
) {
    if segment.kind == SessionTurnUsageSegmentKind::SubagentTask {
        return;
    }
    stats.merge_usage_segment(segment);
}

/// Merges one explicit usage segment while preserving parent-turn ownership boundaries.
/// 合并一个显式用量分段，同时保留父级轮次归属边界。
///
/// # Parameters
/// 参数
/// `stats` is the aggregate turn-statistics snapshot being updated.
/// `stats` 是正在更新的聚合轮次统计快照。
/// `segment` is one explicit provider, tool, compaction, or child-agent usage segment.
/// `segment` 是单个显式 Provider、工具、压缩或智能体用量分段。
pub fn merge_explicit_usage_segment_into_turn_stats(
    stats: &mut SessionTurnStats,
    segment: &SessionTurnUsageSegment,
) {
    if segment.kind == SessionTurnUsageSegmentKind::SubagentTask {
        return;
    }
    stats.merge_usage_segment(segment);
}

/// Returns whether one record closes a parent conversation turn for statistics slicing.
/// 返回单条记录是否为父会话统计切片的轮次闭合边界。
fn record_closes_parent_turn_boundary(record: &SessionConversationRecord) -> bool {
    match record.record_kind {
        SessionMessageRecordKind::SummaryOutput | SessionMessageRecordKind::TurnTerminated => true,
        SessionMessageRecordKind::TurnStats => !record_is_subagent_feedback_turn_stats(record),
        _ => false,
    }
}

/// Returns the first record that belongs to the same context-compaction display section as the closing record.
/// 返回与闭合记录属于同一上下文压缩显示分段的第一条记录。
fn context_compaction_turn_start_index(
    records: &[SessionConversationRecord],
    closing_index: usize,
) -> Option<usize> {
    let closing_section_id = context_compaction_section_id(records.get(closing_index)?)?;
    records[..=closing_index]
        .iter()
        .position(|record| context_compaction_section_id(record) == Some(closing_section_id))
        .or(Some(closing_index))
}

/// Reads the stable context-compaction section id from one transcript record.
/// 从单条转录记录读取稳定的上下文压缩分段标识。
fn context_compaction_section_id(record: &SessionConversationRecord) -> Option<&str> {
    record
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("context_compaction_section_id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

/// Returns whether one turn-statistics record is only child-agent feedback accounting.
/// 返回单条轮次统计记录是否只是智能体反馈用量记录。
fn record_is_subagent_feedback_turn_stats(record: &SessionConversationRecord) -> bool {
    record.record_kind == SessionMessageRecordKind::TurnStats
        && record
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("turn_stats"))
            .and_then(|turn_stats| turn_stats.get("turn_kind"))
            .and_then(Value::as_str)
            .is_some_and(|turn_kind| turn_kind == "subagent_feedback")
}

/// Returns whether one visible transcript row starts a user-authored turn.
/// 返回单条可见转录行是否开启用户编写轮次。
fn record_starts_user_turn(record: &SessionConversationRecord) -> bool {
    if !matches!(record.kind, SessionMessageKind::User)
        || !record.visible
        || record.record_kind != SessionMessageRecordKind::Transcript
    {
        return false;
    }
    record
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("starts_user_turn"))
        .and_then(Value::as_bool)
        .unwrap_or(true)
}

/// Resolves the canonical message index where the active turn begins by matching visible user transcript rows.
/// 通过匹配可见用户转录条目数量解析当前活动轮次在规范消息数组中的起始下标。
fn canonical_turn_start_message_index(
    conversation_state: &SessionConversationState,
    visible_user_record_count_before_turn: usize,
    user_record: &SessionConversationRecord,
) -> Option<usize> {
    let user_message_indexes = canonical_user_message_indexes_for_turn_mapping(conversation_state);
    if user_message_indexes.is_empty() {
        return None;
    }

    if let Some(candidate_index) = user_message_indexes
        .get(visible_user_record_count_before_turn)
        .copied()
        && message_created_at_matches_record(
            &conversation_state.messages[candidate_index],
            user_record,
        )
    {
        return Some(candidate_index);
    }

    canonical_user_message_index_for_record(conversation_state, &user_message_indexes, user_record)
}

/// Resolves one canonical user message by the visible transcript record that opened the turn.
/// 通过打开当前轮次的可见转录记录解析对应的规范用户消息。
fn canonical_user_message_index_for_record(
    conversation_state: &SessionConversationState,
    user_message_indexes: &[usize],
    user_record: &SessionConversationRecord,
) -> Option<usize> {
    let created_at = user_record.created_at.trim();
    if !created_at.is_empty() {
        let matching_indexes = user_message_indexes
            .iter()
            .copied()
            .filter(|index| conversation_state.messages[*index].id.contains(created_at))
            .collect::<Vec<_>>();
        if matching_indexes.len() == 1 {
            return matching_indexes.first().copied();
        }
    }

    let matching_indexes = user_message_indexes
        .iter()
        .copied()
        .filter(|index| {
            canonical_user_message_matches_record(&conversation_state.messages[*index], user_record)
        })
        .collect::<Vec<_>>();
    (matching_indexes.len() == 1).then(|| matching_indexes[0])
}

/// Returns whether one canonical user message and visible record describe the same user input.
/// 返回单条规范用户消息与可见记录是否描述同一条用户输入。
fn canonical_user_message_matches_record(
    message: &SessionConversationMessage,
    user_record: &SessionConversationRecord,
) -> bool {
    let SessionConversationPayload::UserText { text, .. } = &message.payload else {
        return false;
    };
    if message_created_at_matches_record(message, user_record) {
        return true;
    }
    canonical_user_record_text(user_record).is_some_and(|record_text| record_text == text)
}

/// Returns whether the local protocol message id contains the visible record timestamp.
/// 返回本地协议消息标识是否包含可见记录时间戳。
fn message_created_at_matches_record(
    message: &SessionConversationMessage,
    user_record: &SessionConversationRecord,
) -> bool {
    let created_at = user_record.created_at.trim();
    !created_at.is_empty() && message.id.contains(created_at)
}

/// Reads the canonical user text preserved on a visible user transcript record.
/// 读取可见用户转录记录上保留的规范用户文本。
fn canonical_user_record_text(user_record: &SessionConversationRecord) -> Option<&str> {
    user_record
        .metadata
        .as_ref()
        .and_then(|metadata| {
            metadata
                .get("canonical_user_text")
                .or_else(|| metadata.get("canonical_text"))
        })
        .and_then(Value::as_str)
        .or(Some(user_record.body.as_str()))
}

/// Returns the canonical user-message indexes used to align visible transcript turns back onto the canonical conversation window.
/// 返回用于将可见转录轮次重新对齐到规范会话窗口上的规范用户消息下标集合。
fn canonical_user_message_indexes_for_turn_mapping(
    conversation_state: &SessionConversationState,
) -> Vec<usize> {
    let user_message_indexes = conversation_state
        .messages
        .iter()
        .enumerate()
        .filter_map(|(index, message)| {
            matches!(message.payload, SessionConversationPayload::UserText { .. }).then_some(index)
        })
        .collect::<Vec<_>>();
    let protocol_user_indexes = conversation_state
        .messages
        .iter()
        .enumerate()
        .filter_map(|(index, message)| {
            matches!(message.payload, SessionConversationPayload::UserText { .. })
                .then_some((&message.id, index))
        })
        .filter(|(message_id, _)| message_id.contains("-protocol-user-"))
        .map(|(_, index)| index)
        .collect::<Vec<_>>();

    if protocol_user_indexes.is_empty() {
        user_message_indexes
    } else {
        protocol_user_indexes
    }
}

/// Resolves a turn start from canonical messages when no visible user record exists.
/// 当不存在可见用户记录时，从规范消息中解析轮次起点。
///
/// # Parameters
/// 参数
/// `conversation_state` is the canonical conversation state used for statistics.
/// `conversation_state` 是用于统计的规范会话状态。
///
/// # Returns
/// 返回值
/// The best canonical message index for the current turn, or `messages.len()` when no provider-visible turn exists.
/// 返回当前轮次的最佳规范消息下标；若不存在 Provider 可见轮次则返回 `messages.len()`。
fn fallback_turn_start_message_index_without_user_record(
    conversation_state: &SessionConversationState,
) -> usize {
    if let Some(index) = conversation_state
        .messages
        .iter()
        .rposition(canonical_message_starts_provider_turn)
    {
        return index;
    }
    conversation_state
        .messages
        .iter()
        .rposition(assistant_payload_contributes_to_turn_stats)
        .unwrap_or(conversation_state.messages.len())
}

/// Returns whether one canonical message can start a provider-visible turn.
/// 返回一条规范消息是否能开启 Provider 可见轮次。
///
/// # Parameters
/// 参数
/// `message` is the canonical message being inspected.
/// `message` 是正在检查的规范消息。
///
/// # Returns
/// 返回值
/// `true` when the message is user text or a turn-triggering inter-agent message.
/// 当消息是用户文本或触发轮次的智能体消息时返回 `true`。
fn canonical_message_starts_provider_turn(message: &SessionConversationMessage) -> bool {
    match &message.payload {
        SessionConversationPayload::UserText { .. } => true,
        SessionConversationPayload::AgentMessage { trigger_turn, .. } => *trigger_turn,
        SessionConversationPayload::Assistant { .. }
        | SessionConversationPayload::ToolResult { .. } => false,
    }
}

/// Counts unique tool-call records inside the current turn using transcript metadata.
/// 使用转录元数据统计当前轮次中的唯一工具调用记录数量。
fn count_turn_tool_calls_from_records(turn_records: &[SessionConversationRecord]) -> usize {
    let mut unique_tool_call_ids = std::collections::BTreeSet::new();
    for record in turn_records {
        if record.record_kind != SessionMessageRecordKind::ToolCallOutput {
            continue;
        }
        let stable_key = record
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("tool_call_id"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| record.id.clone());
        unique_tool_call_ids.insert(stable_key);
    }
    unique_tool_call_ids.len()
}

/// Returns whether one assistant payload carries meaningful provider work that should count as a provider call.
/// 返回某条助手载荷是否承载了应计入轮次摘要的有效 Provider 工作。
fn assistant_payload_contributes_to_turn_stats(message: &SessionConversationMessage) -> bool {
    match &message.payload {
        SessionConversationPayload::Assistant {
            text,
            reasoning,
            usage,
            tool_calls,
            ..
        } => {
            !text.trim().is_empty()
                || !reasoning.trim().is_empty()
                || !tool_calls.is_empty()
                || usage.is_some()
        }
        _ => false,
    }
}

/// Parses a Unix millisecond timestamp from a persisted text value.
/// 从持久化文本值中解析 Unix 毫秒时间戳。
fn parse_unix_millis_text(value: &str) -> Option<u128> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    trimmed.parse::<u128>().ok()
}

#[cfg(test)]
mod tests;
