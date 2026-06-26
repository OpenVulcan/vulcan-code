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
        .position(|record| record_starts_user_turn(record))
        .map(|offset| boundary_index + offset)
        .unwrap_or(boundary_index);
    let turn_records = &records[start_index..=closing_index];
    if turn_records.is_empty() {
        return None;
    }
    let turn_start_user_record = turn_records
        .iter()
        .find(|record| record_starts_user_turn(record));
    let visible_user_record_count_before_turn = records[..start_index]
        .iter()
        .filter(|record| record_starts_user_turn(record))
        .count();
    let turn_start_message_index = turn_start_user_record
        .and_then(|user_record| {
            canonical_turn_start_message_index(
                conversation_state,
                visible_user_record_count_before_turn,
                user_record,
            )
        })
        .unwrap_or(conversation_state.messages.len());
    let turn_messages = &conversation_state.messages[turn_start_message_index..];

    let mut stats = SessionTurnStats {
        status,
        turn_kind,
        usage_segments: Vec::new(),
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

    let mut assistant_segment = SessionTurnUsageSegment {
        kind: SessionTurnUsageSegmentKind::AssistantResponse,
        trigger: None,
        execution_profile: None,
        provider_id: None,
        model_id: None,
        provider_call_count: 0,
        input_tokens: 0,
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
                let total_tokens = usage.total_tokens.unwrap_or_else(|| {
                    usage
                        .input_tokens
                        .unwrap_or(0)
                        .saturating_add(usage.output_tokens.unwrap_or(0))
                        .saturating_add(usage.reasoning_tokens.unwrap_or(0))
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
            | SessionConversationPayload::UserText { .. } => {}
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
        || assistant_segment.output_tokens > 0
        || assistant_segment.reasoning_tokens > 0
        || assistant_segment.total_tokens > 0
        || assistant_segment.cached_input_tokens.is_some()
        || assistant_segment.cache_creation_input_tokens.is_some()
    {
        stats.usage_segments.push(assistant_segment);
    }
    for segment in explicit_usage_segments.drain(..) {
        merge_usage_segment_into_stats(&mut stats, &segment);
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
    stats.output_tokens_per_second = stats.duration_ms.and_then(|duration_ms| {
        if duration_ms == 0 || stats.output_tokens == 0 {
            return None;
        }
        Some(stats.output_tokens as f64 / (duration_ms as f64 / 1000.0))
    });

    Some(stats)
}

/// Merges one explicit usage segment into aggregate turn statistics.
/// 将一个显式用量分段合并到聚合轮次统计中。
fn merge_usage_segment_into_stats(stats: &mut SessionTurnStats, segment: &SessionTurnUsageSegment) {
    stats.provider_call_count = stats
        .provider_call_count
        .saturating_add(segment.provider_call_count);
    stats.input_tokens = stats.input_tokens.saturating_add(segment.input_tokens);
    stats.output_tokens = stats.output_tokens.saturating_add(segment.output_tokens);
    stats.reasoning_tokens = stats
        .reasoning_tokens
        .saturating_add(segment.reasoning_tokens);
    stats.total_tokens = stats.total_tokens.saturating_add(segment.total_tokens);
    if let Some(cached_input_tokens) = segment.cached_input_tokens {
        let accumulated = stats.cached_input_tokens.unwrap_or(0);
        stats.cached_input_tokens = Some(accumulated.saturating_add(cached_input_tokens));
    }
    if let Some(cache_creation_input_tokens) = segment.cache_creation_input_tokens {
        let accumulated = stats.cache_creation_input_tokens.unwrap_or(0);
        stats.cache_creation_input_tokens =
            Some(accumulated.saturating_add(cache_creation_input_tokens));
    }
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
        .or_else(|| Some(user_record.body.as_str()))
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

#[cfg(test)]
mod tests {
    use serde_json::json;
    use vulcan_core::types::ProviderUsageSnapshot;

    use super::*;
    use crate::conversation_protocol::{SessionConversationMessage, SessionConversationToolCall};

    /// Builds one visible transcript user record for turn-boundary tests.
    /// 为轮次边界测试构建一条可见用户转录记录。
    fn user_record(id: &str, created_at: &str, body: &str) -> SessionConversationRecord {
        SessionConversationRecord {
            id: id.to_string(),
            kind: SessionMessageKind::User,
            record_kind: SessionMessageRecordKind::Transcript,
            visible: true,
            title: String::from("用户指令"),
            body: body.to_string(),
            metadata: None,
            created_at: created_at.to_string(),
        }
    }

    /// Builds one visible interrupt user record folded into the active turn process.
    /// 构建一条折叠进当前轮次过程的可见插队用户记录。
    fn interrupt_user_record(id: &str, created_at: &str, body: &str) -> SessionConversationRecord {
        let mut record = user_record(id, created_at, body);
        record.metadata = Some(json!({
            "source": "user_interrupt",
            "starts_user_turn": false,
            "turn_process": true,
            "collapsed_by_default": true,
            "editable": false,
            "canonical_text": body,
            "canonical_user_text": body,
        }));
        record
    }

    /// Builds one hidden summary record that closes a logical turn.
    /// 构建一条用于闭合逻辑轮次的隐藏摘要记录。
    fn summary_record(id: &str, created_at: &str) -> SessionConversationRecord {
        SessionConversationRecord {
            id: id.to_string(),
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::SummaryOutput,
            visible: false,
            title: String::from("Summary"),
            body: String::from("summary"),
            metadata: None,
            created_at: created_at.to_string(),
        }
    }

    /// Builds one visible turn-stats record that separates completed turns.
    /// 构建一条用于分隔已完成轮次的可见统计记录。
    fn turn_stats_record(id: &str, created_at: &str) -> SessionConversationRecord {
        SessionConversationRecord {
            id: id.to_string(),
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::TurnStats,
            visible: true,
            title: String::from("Turn Stats · completed"),
            body: String::from("stats"),
            metadata: None,
            created_at: created_at.to_string(),
        }
    }

    /// Builds one canonical user message with the provided local identifier and text.
    /// 使用指定本地标识与文本构建一条规范用户消息。
    fn user_message(id: &str, text: &str) -> SessionConversationMessage {
        SessionConversationMessage {
            id: id.to_string(),
            payload: SessionConversationPayload::UserText {
                text: text.to_string(),
                attachments: Vec::new(),
            },
        }
    }

    /// Builds one canonical assistant message carrying usage and one optional tool call.
    /// 构建一条携带用量与可选工具调用的规范助手消息。
    fn assistant_message(
        id: &str,
        usage: ProviderUsageSnapshot,
        include_tool_call: bool,
    ) -> SessionConversationMessage {
        SessionConversationMessage {
            id: id.to_string(),
            payload: SessionConversationPayload::Assistant {
                text: String::from("assistant output"),
                reasoning: String::new(),
                provider_message_id: None,
                provider_message_state: None,
                usage: Some(usage),
                tool_calls: include_tool_call
                    .then(|| SessionConversationToolCall {
                        id: String::from("tool-item-1"),
                        tool_call_id: Some(String::from("tool-call-1")),
                        tool_name: String::from("vulcan_file_list"),
                        arguments: json!({}),
                    })
                    .into_iter()
                    .collect(),
            },
        }
    }

    /// Verifies an unaligned current turn never inherits historical canonical usage or tool calls.
    /// 验证无法对齐的当前轮次不会继承历史规范用量或工具调用。
    #[test]
    fn current_turn_stats_do_not_inherit_historical_canonical_messages() {
        let closing_record_id = "summary-current";
        let conversation_state = SessionConversationState {
            messages: vec![
                user_message("session-1-protocol-user-1000", "上一轮"),
                assistant_message(
                    "session-1-protocol-assistant-1001",
                    ProviderUsageSnapshot {
                        input_tokens: Some(1000),
                        output_tokens: Some(200),
                        total_tokens: Some(1200),
                        cached_input_tokens: Some(800),
                        ..ProviderUsageSnapshot::default()
                    },
                    true,
                ),
            ],
            records: vec![
                user_record("user-previous", "1000", "上一轮"),
                summary_record("summary-previous", "1002"),
                turn_stats_record("stats-previous", "1003"),
                user_record("user-current", "2000", "当前无工具"),
                summary_record(closing_record_id, "2002"),
            ],
            continuation: Default::default(),
        };

        let stats = build_session_turn_stats_from_state(
            &conversation_state,
            closing_record_id,
            SessionTurnStatsStatus::Completed,
            None,
        )
        .expect("turn stats should be built");

        assert_eq!(stats.provider_call_count, 0);
        assert_eq!(stats.tool_call_count, 0);
        assert_eq!(stats.input_tokens, 0);
        assert_eq!(stats.output_tokens, 0);
        assert_eq!(stats.total_tokens, 0);
        assert_eq!(stats.cached_input_tokens, None);
    }

    /// Verifies display/runtime layer separation does not map the current record to the first historical user.
    /// 验证显示层与运行层分离时不会把当前记录映射到首个历史用户。
    #[test]
    fn current_turn_stats_align_current_record_when_records_only_hold_active_turn() {
        let closing_record_id = "summary-current";
        let conversation_state = SessionConversationState {
            messages: vec![
                user_message("session-1-protocol-user-1000", "上一轮"),
                assistant_message(
                    "session-1-protocol-assistant-1001",
                    ProviderUsageSnapshot {
                        input_tokens: Some(1000),
                        output_tokens: Some(200),
                        total_tokens: Some(1200),
                        cached_input_tokens: Some(800),
                        ..ProviderUsageSnapshot::default()
                    },
                    true,
                ),
                user_message("session-1-protocol-user-2000", "当前无工具"),
                assistant_message(
                    "session-1-protocol-assistant-2001",
                    ProviderUsageSnapshot {
                        input_tokens: Some(40),
                        output_tokens: Some(12),
                        total_tokens: Some(52),
                        cached_input_tokens: Some(16),
                        ..ProviderUsageSnapshot::default()
                    },
                    false,
                ),
            ],
            records: vec![
                user_record("user-current", "2000", "当前无工具"),
                summary_record(closing_record_id, "2002"),
            ],
            continuation: Default::default(),
        };

        let stats = build_session_turn_stats_from_state(
            &conversation_state,
            closing_record_id,
            SessionTurnStatsStatus::Completed,
            None,
        )
        .expect("turn stats should be built");

        assert_eq!(stats.provider_call_count, 1);
        assert_eq!(stats.tool_call_count, 0);
        assert_eq!(stats.input_tokens, 40);
        assert_eq!(stats.output_tokens, 12);
        assert_eq!(stats.total_tokens, 52);
        assert_eq!(stats.cached_input_tokens, Some(16));
    }

    /// Verifies repeated user text does not let a historical user satisfy the ordinal candidate.
    /// 验证重复用户文本不会让历史用户通过序号候选校验。
    #[test]
    fn current_turn_stats_reject_historical_candidate_with_repeated_user_text() {
        let closing_record_id = "summary-current";
        let conversation_state = SessionConversationState {
            messages: vec![
                user_message("session-1-protocol-user-1000", "继续"),
                assistant_message(
                    "session-1-protocol-assistant-1001",
                    ProviderUsageSnapshot {
                        input_tokens: Some(900),
                        output_tokens: Some(300),
                        total_tokens: Some(1200),
                        cached_input_tokens: Some(700),
                        ..ProviderUsageSnapshot::default()
                    },
                    true,
                ),
                user_message("session-1-protocol-user-2000", "继续"),
                assistant_message(
                    "session-1-protocol-assistant-2001",
                    ProviderUsageSnapshot {
                        input_tokens: Some(30),
                        output_tokens: Some(9),
                        total_tokens: Some(39),
                        cached_input_tokens: Some(12),
                        ..ProviderUsageSnapshot::default()
                    },
                    false,
                ),
            ],
            records: vec![
                user_record("user-current", "2000", "继续"),
                summary_record(closing_record_id, "2002"),
            ],
            continuation: Default::default(),
        };

        let stats = build_session_turn_stats_from_state(
            &conversation_state,
            closing_record_id,
            SessionTurnStatsStatus::Completed,
            None,
        )
        .expect("turn stats should be built");

        assert_eq!(stats.provider_call_count, 1);
        assert_eq!(stats.tool_call_count, 0);
        assert_eq!(stats.input_tokens, 30);
        assert_eq!(stats.output_tokens, 9);
        assert_eq!(stats.total_tokens, 39);
        assert_eq!(stats.cached_input_tokens, Some(12));
    }

    /// Verifies interleaved user prompt records stay inside the original running turn.
    /// 验证中途插入的用户提示词记录仍属于原始运行轮次。
    #[test]
    fn current_turn_stats_keep_interleaved_user_prompts_inside_same_turn() {
        let closing_record_id = "summary-current";
        let conversation_state = SessionConversationState {
            messages: vec![
                user_message("session-1-protocol-user-1000", "上一轮"),
                assistant_message(
                    "session-1-protocol-assistant-1001",
                    ProviderUsageSnapshot {
                        input_tokens: Some(500),
                        output_tokens: Some(100),
                        total_tokens: Some(600),
                        cached_input_tokens: Some(400),
                        ..ProviderUsageSnapshot::default()
                    },
                    true,
                ),
                user_message("session-1-protocol-user-2000", "当前任务"),
                user_message("session-1-protocol-user-2001", "中途补充"),
                assistant_message(
                    "session-1-protocol-assistant-2002",
                    ProviderUsageSnapshot {
                        input_tokens: Some(80),
                        output_tokens: Some(20),
                        total_tokens: Some(100),
                        cached_input_tokens: Some(30),
                        ..ProviderUsageSnapshot::default()
                    },
                    false,
                ),
            ],
            records: vec![
                user_record("user-current", "2000", "当前任务"),
                interrupt_user_record("user-interleaved", "2001", "中途补充"),
                summary_record(closing_record_id, "2003"),
            ],
            continuation: Default::default(),
        };

        let stats = build_session_turn_stats_from_state(
            &conversation_state,
            closing_record_id,
            SessionTurnStatsStatus::Completed,
            None,
        )
        .expect("turn stats should be built");

        assert_eq!(stats.provider_call_count, 1);
        assert_eq!(stats.tool_call_count, 0);
        assert_eq!(stats.input_tokens, 80);
        assert_eq!(stats.output_tokens, 20);
        assert_eq!(stats.total_tokens, 100);
        assert_eq!(stats.cached_input_tokens, Some(30));
    }

    /// Verifies prior interrupt user messages do not shift the next turn's canonical alignment.
    /// 验证上一轮插队用户消息不会挤偏下一轮的规范消息对齐。
    #[test]
    fn current_turn_stats_ignore_prior_interrupts_when_aligning_new_turn() {
        let closing_record_id = "summary-current";
        let conversation_state = SessionConversationState {
            messages: vec![
                user_message("session-1-protocol-user-1000", "上一轮"),
                user_message("session-1-protocol-user-1001", "上一轮插队"),
                assistant_message(
                    "session-1-protocol-assistant-1002",
                    ProviderUsageSnapshot {
                        input_tokens: Some(500),
                        output_tokens: Some(100),
                        total_tokens: Some(600),
                        cached_input_tokens: Some(400),
                        ..ProviderUsageSnapshot::default()
                    },
                    true,
                ),
                user_message("session-1-protocol-user-2000", "当前任务"),
                assistant_message(
                    "session-1-protocol-assistant-2001",
                    ProviderUsageSnapshot {
                        input_tokens: Some(70),
                        output_tokens: Some(14),
                        total_tokens: Some(84),
                        cached_input_tokens: Some(21),
                        ..ProviderUsageSnapshot::default()
                    },
                    false,
                ),
            ],
            records: vec![
                user_record("user-previous", "1000", "上一轮"),
                interrupt_user_record("user-previous-interrupt", "1001", "上一轮插队"),
                summary_record("summary-previous", "1003"),
                turn_stats_record("stats-previous", "1004"),
                user_record("user-current", "2000", "当前任务"),
                summary_record(closing_record_id, "2002"),
            ],
            continuation: Default::default(),
        };

        let stats = build_session_turn_stats_from_state(
            &conversation_state,
            closing_record_id,
            SessionTurnStatsStatus::Completed,
            None,
        )
        .expect("turn stats should be built");

        assert_eq!(stats.provider_call_count, 1);
        assert_eq!(stats.tool_call_count, 0);
        assert_eq!(stats.input_tokens, 70);
        assert_eq!(stats.output_tokens, 14);
        assert_eq!(stats.total_tokens, 84);
        assert_eq!(stats.cached_input_tokens, Some(21));
    }

    /// Verifies compacted windows can still align the current user by the shared creation timestamp.
    /// 验证压缩后的窗口仍可通过共享创建时间对齐当前用户。
    #[test]
    fn compacted_current_turn_stats_align_by_user_record_timestamp() {
        let closing_record_id = "summary-current";
        let conversation_state = SessionConversationState {
            messages: vec![
                user_message("session-1-protocol-user-2000", "当前有输出"),
                assistant_message(
                    "session-1-protocol-assistant-2001",
                    ProviderUsageSnapshot {
                        input_tokens: Some(50),
                        output_tokens: Some(25),
                        total_tokens: Some(75),
                        cached_input_tokens: Some(10),
                        ..ProviderUsageSnapshot::default()
                    },
                    false,
                ),
            ],
            records: vec![
                user_record("user-previous", "1000", "上一轮"),
                summary_record("summary-previous", "1002"),
                turn_stats_record("stats-previous", "1003"),
                user_record("user-current", "2000", "当前有输出"),
                summary_record(closing_record_id, "2002"),
            ],
            continuation: Default::default(),
        };

        let stats = build_session_turn_stats_from_state(
            &conversation_state,
            closing_record_id,
            SessionTurnStatsStatus::Completed,
            None,
        )
        .expect("turn stats should be built");

        assert_eq!(stats.provider_call_count, 1);
        assert_eq!(stats.tool_call_count, 0);
        assert_eq!(stats.input_tokens, 50);
        assert_eq!(stats.output_tokens, 25);
        assert_eq!(stats.total_tokens, 75);
        assert_eq!(stats.cached_input_tokens, Some(10));
    }

    /// Verifies embedded compaction usage segments are aggregated into a normal user turn.
    /// 验证嵌入式压缩用量分段会聚合进普通用户轮次。
    #[test]
    fn user_turn_stats_merge_embedded_context_compaction_segment() {
        let closing_record_id = "summary-current";
        let conversation_state = SessionConversationState {
            messages: vec![
                user_message("session-1-protocol-user-2000", "当前任务"),
                assistant_message(
                    "session-1-protocol-assistant-2001",
                    ProviderUsageSnapshot {
                        input_tokens: Some(40),
                        output_tokens: Some(12),
                        total_tokens: Some(52),
                        cached_input_tokens: Some(16),
                        ..ProviderUsageSnapshot::default()
                    },
                    false,
                ),
            ],
            records: vec![
                user_record("user-current", "2000", "当前任务"),
                summary_record(closing_record_id, "2002"),
            ],
            continuation: Default::default(),
        };
        let mut compaction_segment = SessionTurnUsageSegment::from_provider_usage(
            SessionTurnUsageSegmentKind::ContextCompaction,
            1,
            Some(&ProviderUsageSnapshot {
                input_tokens: Some(20),
                output_tokens: Some(4),
                reasoning_tokens: Some(2),
                total_tokens: Some(26),
                cached_input_tokens: Some(5),
                ..ProviderUsageSnapshot::default()
            }),
        );
        compaction_segment.trigger = Some(String::from("auto_budget"));
        compaction_segment.execution_profile = Some(String::from("cache_preserving_full_fidelity"));

        let stats = build_session_turn_stats_from_state_with_segments(
            &conversation_state,
            closing_record_id,
            SessionTurnStatsStatus::Completed,
            Some(1),
            SessionTurnStatsKind::UserTurn,
            vec![compaction_segment],
        )
        .expect("turn stats should be built");

        assert_eq!(stats.turn_kind, SessionTurnStatsKind::UserTurn);
        assert_eq!(stats.provider_call_count, 2);
        assert_eq!(stats.input_tokens, 60);
        assert_eq!(stats.output_tokens, 16);
        assert_eq!(stats.reasoning_tokens, 2);
        assert_eq!(stats.total_tokens, 78);
        assert_eq!(stats.cached_input_tokens, Some(21));
        assert_eq!(stats.usage_segments.len(), 2);
        assert_eq!(
            stats.usage_segments[0].kind,
            SessionTurnUsageSegmentKind::AssistantResponse
        );
        assert_eq!(
            stats.usage_segments[1].kind,
            SessionTurnUsageSegmentKind::ContextCompaction
        );
    }

    /// Verifies assistant provider retries remain visible when compaction usage is embedded.
    /// 验证嵌入压缩用量时仍能保留助手响应的 Provider 重试次数。
    #[test]
    fn user_turn_stats_apply_provider_override_to_assistant_segment_with_compaction_segment() {
        let closing_record_id = "summary-current";
        let conversation_state = SessionConversationState {
            messages: vec![
                user_message("session-1-protocol-user-2000", "当前任务"),
                assistant_message(
                    "session-1-protocol-assistant-2001",
                    ProviderUsageSnapshot {
                        input_tokens: Some(40),
                        output_tokens: Some(12),
                        total_tokens: Some(52),
                        cached_input_tokens: Some(16),
                        ..ProviderUsageSnapshot::default()
                    },
                    false,
                ),
            ],
            records: vec![
                user_record("user-current", "2000", "当前任务"),
                summary_record(closing_record_id, "2002"),
            ],
            continuation: Default::default(),
        };
        let compaction_segment = SessionTurnUsageSegment::from_provider_usage(
            SessionTurnUsageSegmentKind::ContextCompaction,
            1,
            Some(&ProviderUsageSnapshot {
                input_tokens: Some(20),
                output_tokens: Some(4),
                total_tokens: Some(24),
                ..ProviderUsageSnapshot::default()
            }),
        );

        let stats = build_session_turn_stats_from_state_with_segments(
            &conversation_state,
            closing_record_id,
            SessionTurnStatsStatus::Completed,
            Some(2),
            SessionTurnStatsKind::UserTurn,
            vec![compaction_segment],
        )
        .expect("turn stats should be built");

        assert_eq!(stats.provider_call_count, 3);
        assert_eq!(stats.usage_segments.len(), 2);
        assert_eq!(stats.usage_segments[0].provider_call_count, 2);
        assert_eq!(stats.usage_segments[1].provider_call_count, 1);
        assert_eq!(
            stats
                .usage_segments
                .iter()
                .map(|segment| segment.provider_call_count)
                .sum::<usize>(),
            stats.provider_call_count
        );
    }

    /// Verifies failed assistant requests remain represented when compaction usage is embedded.
    /// 验证嵌入压缩用量时失败的助手请求仍会保留统计分段。
    #[test]
    fn user_turn_stats_keep_failed_assistant_request_segment_with_compaction_segment() {
        let closing_record_id = "terminated-current";
        let conversation_state = SessionConversationState {
            messages: vec![user_message("session-1-protocol-user-2000", "当前任务")],
            records: vec![
                user_record("user-current", "2000", "当前任务"),
                SessionConversationRecord {
                    id: closing_record_id.to_string(),
                    kind: SessionMessageKind::Assistant,
                    record_kind: SessionMessageRecordKind::TurnTerminated,
                    visible: true,
                    title: String::from("执行失败"),
                    body: String::from(
                        "Provider request failed before assistant usage was recorded.",
                    ),
                    metadata: None,
                    created_at: String::from("2003"),
                },
            ],
            continuation: Default::default(),
        };
        let compaction_segment = SessionTurnUsageSegment::from_provider_usage(
            SessionTurnUsageSegmentKind::ContextCompaction,
            1,
            Some(&ProviderUsageSnapshot {
                input_tokens: Some(20),
                output_tokens: Some(4),
                total_tokens: Some(24),
                ..ProviderUsageSnapshot::default()
            }),
        );

        let stats = build_session_turn_stats_from_state_with_segments(
            &conversation_state,
            closing_record_id,
            SessionTurnStatsStatus::Failed,
            Some(2),
            SessionTurnStatsKind::UserTurn,
            vec![compaction_segment],
        )
        .expect("turn stats should be built");

        assert_eq!(stats.status, SessionTurnStatsStatus::Failed);
        assert_eq!(stats.provider_call_count, 3);
        assert_eq!(stats.usage_segments.len(), 2);
        assert_eq!(
            stats.usage_segments[0].kind,
            SessionTurnUsageSegmentKind::AssistantResponse
        );
        assert_eq!(stats.usage_segments[0].provider_call_count, 2);
        assert_eq!(stats.usage_segments[0].total_tokens, 0);
        assert_eq!(
            stats.usage_segments[1].kind,
            SessionTurnUsageSegmentKind::ContextCompaction
        );
        assert_eq!(
            stats
                .usage_segments
                .iter()
                .map(|segment| segment.provider_call_count)
                .sum::<usize>(),
            stats.provider_call_count
        );
    }

    /// Verifies manual compaction can produce an independent context-compaction turn without a user prompt.
    /// 验证手动压缩可以在没有用户提示词的情况下生成独立上下文压缩轮次。
    #[test]
    fn context_compaction_turn_stats_can_be_built_from_explicit_segment_only() {
        let closing_record_id = "compaction-end";
        let conversation_state = SessionConversationState {
            messages: Vec::new(),
            records: vec![SessionConversationRecord {
                id: closing_record_id.to_string(),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::CompactionStatus,
                visible: true,
                title: String::from("context_compaction.completed"),
                body: String::new(),
                metadata: None,
                created_at: String::from("3000"),
            }],
            continuation: Default::default(),
        };
        let compaction_segment = SessionTurnUsageSegment::from_provider_usage(
            SessionTurnUsageSegmentKind::ContextCompaction,
            1,
            Some(&ProviderUsageSnapshot {
                input_tokens: Some(100),
                output_tokens: Some(30),
                total_tokens: Some(130),
                cache_creation_input_tokens: Some(10),
                ..ProviderUsageSnapshot::default()
            }),
        );

        let stats = build_session_turn_stats_from_state_with_segments(
            &conversation_state,
            closing_record_id,
            SessionTurnStatsStatus::Completed,
            None,
            SessionTurnStatsKind::ContextCompaction,
            vec![compaction_segment],
        )
        .expect("context-compaction stats should be built");

        assert_eq!(stats.turn_kind, SessionTurnStatsKind::ContextCompaction);
        assert_eq!(stats.provider_call_count, 1);
        assert_eq!(stats.input_tokens, 100);
        assert_eq!(stats.output_tokens, 30);
        assert_eq!(stats.total_tokens, 130);
        assert_eq!(stats.cache_creation_input_tokens, Some(10));
        assert_eq!(stats.usage_segments.len(), 1);
        assert_eq!(
            stats.usage_segments[0].kind,
            SessionTurnUsageSegmentKind::ContextCompaction
        );
    }
}
