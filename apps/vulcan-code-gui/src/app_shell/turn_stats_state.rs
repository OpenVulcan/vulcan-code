//! Turn statistics merge anchor extracted from the main workbench module.
//! 从主工作台模块中抽取的轮次统计合并锚点。

use super::*;

/// Stable merge anchor derived from the visible user row that belongs to one completed turn.
/// 从属于某个已完成轮次的可见用户行派生出来的稳定合并锚点。
#[derive(Debug, Clone)]
pub(super) struct TurnStatsMergeAnchor {
    /// Stable visible user record identifier for the turn.
    /// 该轮次的稳定可见用户记录标识。
    pub(super) user_record_id: Option<String>,
    /// Stable creation timestamp of the visible user row.
    /// 可见用户行的稳定创建时间戳。
    pub(super) user_created_at: Option<String>,
    /// Normalized visible user text for legacy rows that do not yet carry dedicated anchor metadata.
    /// 针对尚未携带专用锚点元数据的旧记录使用的归一化可见用户文本。
    pub(super) user_text: Option<String>,
}

impl PartialEq for TurnStatsMergeAnchor {
    /// Compares two turn anchors using the strongest available stable fields first.
    /// 按最强的稳定字段优先对比两个轮次锚点。
    fn eq(&self, other: &Self) -> bool {
        match (&self.user_record_id, &other.user_record_id) {
            (Some(left), Some(right)) => left == right,
            _ => self.user_created_at == other.user_created_at && self.user_text == other.user_text,
        }
    }
}

impl Eq for TurnStatsMergeAnchor {}

/// Builds one best-effort turn-statistics snapshot from the transcript rows bounded by the provided terminal closing record.
/// 根据给定终态闭合记录所界定的转录行，构建一份最佳努力的轮次统计快照。
pub(super) fn build_session_turn_stats_from_state(
    conversation_state: &crate::conversation_protocol::SessionConversationState,
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
            crate::conversation_protocol::SessionConversationPayload::Assistant {
                usage,
                tool_calls,
                ..
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
            crate::conversation_protocol::SessionConversationPayload::ToolResult { .. }
            | crate::conversation_protocol::SessionConversationPayload::UserText { .. } => {}
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

/// Resolves the canonical message index where the active turn begins by matching the count of visible user transcript rows that appeared before the turn boundary.
/// 通过匹配轮次边界之前已出现的可见用户转录条目数量，解析当前活动轮次在规范消息数组中的起始下标。
fn canonical_turn_start_message_index(
    conversation_state: &crate::conversation_protocol::SessionConversationState,
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
    conversation_state: &crate::conversation_protocol::SessionConversationState,
) -> Vec<usize> {
    let user_message_indexes = conversation_state
        .messages
        .iter()
        .enumerate()
        .filter_map(|(index, message)| {
            matches!(
                message.payload,
                crate::conversation_protocol::SessionConversationPayload::UserText { .. }
            )
            .then_some(index)
        })
        .collect::<Vec<_>>();
    let protocol_user_indexes = conversation_state
        .messages
        .iter()
        .enumerate()
        .filter_map(|(index, message)| {
            matches!(
                message.payload,
                crate::conversation_protocol::SessionConversationPayload::UserText { .. }
            )
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

/// Counts unique tool-call records inside the current turn using transcript metadata so UI projection ids never affect the final statistic.
/// 使用转录元数据统计当前轮次中的唯一工具调用记录数量，确保界面投影标识不会影响最终统计值。
fn count_turn_tool_calls_from_records(
    turn_records: &[crate::conversation_protocol::SessionConversationRecord],
) -> usize {
    let mut unique_tool_call_ids = std::collections::BTreeSet::new();
    for record in turn_records {
        if record.record_kind != SessionMessageRecordKind::ToolCallOutput {
            continue;
        }
        let stable_key = record
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("tool_call_id"))
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| record.id.clone());
        unique_tool_call_ids.insert(stable_key);
    }
    unique_tool_call_ids.len()
}

/// Returns whether one assistant payload carries meaningful provider work that should count as a provider call in the turn summary.
/// 返回某条助手载荷是否承载了应计入轮次摘要的有效 Provider 工作。
fn assistant_payload_contributes_to_turn_stats(
    message: &crate::conversation_protocol::SessionConversationMessage,
) -> bool {
    match &message.payload {
        crate::conversation_protocol::SessionConversationPayload::Assistant {
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

/// Merges one freshly projected conversation-state slice back into the already hydrated visible transcript window so older on-demand history rows are preserved instead of disappearing during live updates.
/// 将一段新鲜投影出的 conversation-state 切片重新合并回已经水合的可见转录窗口，确保按需加载的旧历史行不会在实时更新期间消失。
pub(super) fn merge_projected_session_messages_into_existing_window(
    existing_messages: &[SessionMessageEntry],
    projected_messages: Vec<SessionMessageEntry>,
) -> Vec<SessionMessageEntry> {
    if existing_messages.is_empty() {
        return projected_messages;
    }
    if projected_messages.is_empty() {
        return existing_messages.to_vec();
    }

    let projected_by_id = projected_messages
        .iter()
        .cloned()
        .map(|message| (message.id.clone(), message))
        .collect::<BTreeMap<_, _>>();
    let projected_index_by_id = projected_messages
        .iter()
        .enumerate()
        .map(|(index, message)| (message.id.clone(), index))
        .collect::<BTreeMap<_, _>>();
    let mut consumed_projected_ids = BTreeSet::new();
    let mut merged_messages =
        Vec::with_capacity(existing_messages.len().max(projected_messages.len()));
    let mut latest_existing_projection_index = existing_messages
        .iter()
        .filter_map(|message| projected_index_by_id.get(&message.id).copied())
        .max();

    for (existing_index, existing_message) in existing_messages.iter().enumerate() {
        if let Some(projected_message) = projected_by_id.get(&existing_message.id).cloned() {
            consumed_projected_ids.insert(existing_message.id.clone());
            merged_messages.push(projected_message);
        } else if let Some(projected_message) = projected_messages
            .iter()
            .enumerate()
            .find(|(projected_index, projected_message)| {
                !consumed_projected_ids.contains(&projected_message.id)
                    && turn_stats_message_semantically_matches(
                        existing_messages,
                        existing_index,
                        existing_message,
                        &projected_messages,
                        *projected_index,
                        projected_message,
                    )
            })
            .map(|(_, projected_message)| projected_message.clone())
        {
            if let Some(projected_index) = projected_index_by_id.get(&projected_message.id).copied()
            {
                latest_existing_projection_index = Some(
                    latest_existing_projection_index
                        .map(|latest_index| latest_index.max(projected_index))
                        .unwrap_or(projected_index),
                );
            }
            consumed_projected_ids.insert(projected_message.id.clone());
            merged_messages.push(projected_message);
        } else {
            merged_messages.push(existing_message.clone());
        }
    }

    for (projected_index, projected_message) in projected_messages.into_iter().enumerate() {
        if consumed_projected_ids.contains(&projected_message.id) {
            continue;
        }
        if latest_existing_projection_index
            .is_some_and(|latest_index| projected_index <= latest_index)
        {
            continue;
        }
        merged_messages.push(projected_message);
    }

    merged_messages
}

/// Returns whether two visible turn-statistics rows should be treated as the same logical row even when their runtime ids or recalculated payload fields drift after reload or reprojection.
/// 返回两条可见轮次统计行在重载或重新投影后即使运行时 id 或重算后的载荷字段漂移，也应视为同一逻辑行。
fn turn_stats_message_semantically_matches(
    existing_messages: &[SessionMessageEntry],
    existing_index: usize,
    existing_message: &SessionMessageEntry,
    projected_messages: &[SessionMessageEntry],
    projected_index: usize,
    projected_message: &SessionMessageEntry,
) -> bool {
    if existing_message.record_kind != SessionMessageRecordKind::TurnStats
        || projected_message.record_kind != SessionMessageRecordKind::TurnStats
        || existing_message.kind != projected_message.kind
        || existing_message.visible != projected_message.visible
        || existing_message.title != projected_message.title
    {
        return false;
    }

    if existing_message.body == projected_message.body
        && existing_message.created_at == projected_message.created_at
        && existing_message.metadata == projected_message.metadata
    {
        return true;
    }

    let existing_closing_record_id =
        turn_stats_message_metadata_string(existing_message, "turn_closing_record_id");
    let projected_closing_record_id =
        turn_stats_message_metadata_string(projected_message, "turn_closing_record_id");
    if existing_closing_record_id.is_some()
        && existing_closing_record_id == projected_closing_record_id
    {
        return true;
    }

    let existing_anchor =
        turn_stats_previous_visible_user_anchor(existing_messages, existing_index);
    let projected_anchor =
        turn_stats_previous_visible_user_anchor(projected_messages, projected_index);
    existing_anchor.is_some() && existing_anchor == projected_anchor
}

/// Returns one structured anchor for a turn-stats row based on the visible user row that immediately precedes it.
/// 根据紧邻其前的可见用户行返回该轮次统计行的结构化锚点。
fn turn_stats_previous_visible_user_anchor(
    messages: &[SessionMessageEntry],
    turn_stats_index: usize,
) -> Option<TurnStatsMergeAnchor> {
    if turn_stats_index == 0 {
        return None;
    }

    let user_message = messages[..turn_stats_index].iter().rev().find(|message| {
        message.visible
            && message.kind == SessionMessageKind::User
            && message.record_kind == SessionMessageRecordKind::Transcript
    })?;
    Some(TurnStatsMergeAnchor {
        user_record_id: Some(user_message.id.clone()),
        user_created_at: Some(user_message.created_at.clone()),
        user_text: Some(turn_stats_anchor_text(user_message)),
    })
}

/// Returns one stable text anchor for a user row by preferring canonical metadata and falling back to the visible body.
/// 为用户行返回稳定文本锚点，优先使用规范元数据，必要时回退到可见正文。
fn turn_stats_anchor_text(message: &SessionMessageEntry) -> String {
    let text = turn_stats_message_metadata_string(message, "canonical_user_text")
        .unwrap_or_else(|| message.body.clone());
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Reads one string field from turn-stats metadata when it exists and is non-empty.
/// 当轮次统计元数据中存在且非空时读取其中一个字符串字段。
fn turn_stats_message_metadata_string(message: &SessionMessageEntry, key: &str) -> Option<String> {
    message
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get(key))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}
