//! Shared turn-statistics construction for service and workbench transcript lifecycles.
//! 面向服务与工作台转录生命周期共享的轮次统计构建逻辑。

use serde_json::Value;
use vulcan_workbench_model::{
    SessionMessageKind, SessionMessageRecordKind, SessionTurnStats, SessionTurnStatsStatus,
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
    let records = &conversation_state.records;
    let closing_index = records
        .iter()
        .position(|record| record.id == closing_record_id)?;
    let boundary_index = records[..closing_index]
        .iter()
        .rposition(|record| {
            matches!(
                record.record_kind,
                SessionMessageRecordKind::SummaryOutput
                    | SessionMessageRecordKind::TurnTerminated
                    | SessionMessageRecordKind::TurnStats
            )
        })
        .map(|index| index + 1)
        .unwrap_or(0);
    let start_index = records[boundary_index..=closing_index]
        .iter()
        .position(|record| {
            matches!(record.kind, SessionMessageKind::User)
                && record.visible
                && record.record_kind == SessionMessageRecordKind::Transcript
        })
        .map(|offset| boundary_index + offset)
        .unwrap_or(boundary_index);
    let turn_records = &records[start_index..=closing_index];
    if turn_records.is_empty() {
        return None;
    }
    let visible_user_record_count_before_turn = records[..start_index]
        .iter()
        .filter(|record| {
            matches!(record.kind, SessionMessageKind::User)
                && record.visible
                && record.record_kind == SessionMessageRecordKind::Transcript
        })
        .count();
    let turn_start_message_index = canonical_turn_start_message_index(
        conversation_state,
        visible_user_record_count_before_turn,
    )
    .unwrap_or(0);
    let turn_messages = &conversation_state.messages[turn_start_message_index..];

    let mut stats = SessionTurnStats {
        status,
        provider_call_count: 0,
        tool_call_count: 0,
        input_tokens: 0,
        output_tokens: 0,
        reasoning_tokens: 0,
        total_tokens: 0,
        cached_input_tokens: None,
        cache_creation_input_tokens: None,
        duration_ms: None,
        output_tokens_per_second: None,
    };

    stats.tool_call_count = count_turn_tool_calls_from_records(turn_records);

    for message in turn_messages {
        match &message.payload {
            SessionConversationPayload::Assistant {
                usage, tool_calls, ..
            } => {
                if assistant_payload_contributes_to_turn_stats(message) {
                    stats.provider_call_count = stats.provider_call_count.saturating_add(1);
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
                stats.output_tokens = stats
                    .output_tokens
                    .saturating_add(usage.output_tokens.unwrap_or(0));
                stats.reasoning_tokens = stats
                    .reasoning_tokens
                    .saturating_add(usage.reasoning_tokens.unwrap_or(0));
                stats.total_tokens =
                    stats
                        .total_tokens
                        .saturating_add(usage.total_tokens.unwrap_or_else(|| {
                            usage
                                .input_tokens
                                .unwrap_or(0)
                                .saturating_add(usage.output_tokens.unwrap_or(0))
                                .saturating_add(usage.reasoning_tokens.unwrap_or(0))
                        }));
                if let Some(cached_input_tokens) = usage.cached_input_tokens {
                    let accumulated = stats.cached_input_tokens.unwrap_or(0);
                    stats.cached_input_tokens =
                        Some(accumulated.saturating_add(cached_input_tokens));
                }
                if let Some(cache_creation_input_tokens) = usage.cache_creation_input_tokens {
                    let accumulated = stats.cache_creation_input_tokens.unwrap_or(0);
                    stats.cache_creation_input_tokens =
                        Some(accumulated.saturating_add(cache_creation_input_tokens));
                }
            }
            SessionConversationPayload::ToolResult { .. }
            | SessionConversationPayload::UserText { .. } => {}
        }
    }

    if let Some(provider_request_count_override) = provider_request_count_override
        && provider_request_count_override > 0
    {
        stats.provider_call_count = provider_request_count_override;
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
    stats.output_tokens_per_second = stats.duration_ms.and_then(|duration_ms| {
        if duration_ms == 0 || stats.output_tokens == 0 {
            return None;
        }
        Some(stats.output_tokens as f64 / (duration_ms as f64 / 1000.0))
    });

    Some(stats)
}

/// Resolves the canonical message index where the active turn begins by matching visible user transcript rows.
/// 通过匹配可见用户转录条目数量解析当前活动轮次在规范消息数组中的起始下标。
fn canonical_turn_start_message_index(
    conversation_state: &SessionConversationState,
    visible_user_record_count_before_turn: usize,
) -> Option<usize> {
    let user_message_indexes = canonical_user_message_indexes_for_turn_mapping(conversation_state);
    if user_message_indexes.is_empty() {
        return None;
    }

    user_message_indexes
        .get(visible_user_record_count_before_turn)
        .copied()
        .or_else(|| user_message_indexes.last().copied())
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
