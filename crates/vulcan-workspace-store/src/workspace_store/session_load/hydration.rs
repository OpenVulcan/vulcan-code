//! Session hydration entry points and snapshot assembly.
//! 会话水合入口与快照组装逻辑。

use super::*;
use crate::conversation_protocol::SessionConversationPayload;
use std::collections::BTreeSet;
use vulcan_context::SessionConversationMessage;
use vulcan_core::types::ProviderUsageSnapshot;
use vulcan_workbench_model::{
    SessionMessageKind, SessionTurnStats, SessionTurnStatsKind, SessionTurnUsageGroupKind,
    SessionTurnUsageSegment, SessionTurnUsageSegmentKind,
};

/// Lazily loads one session by its deterministic project, created-at, and session id.
/// 按确定性的项目、创建时间与会话标识懒加载单个会话。
///
/// # Parameters
/// 参数
/// `controller` owns the workspace-store runtime bridge.
/// `controller` 持有工作区存储运行时桥。
/// `project_id` is the known owning project identifier used as the segmented project tag.
/// `project_id` 是已知的所属项目标识，用作分段项目标签。
/// `created_at` is the raw Unix-milliseconds creation timestamp from the session summary.
/// `created_at` 是会话摘要中的原始 Unix 毫秒创建时间戳。
/// `session_id` selects the session to hydrate.
/// `session_id` 选择要水合的会话。
/// `requested_limit` is the requested recent-turn window size before clamping.
/// `requested_limit` 是裁剪前请求的最近轮次窗口大小。
///
/// # Returns
/// 返回值
/// A hydration snapshot for the target session, or an empty snapshot if no segmented directory exists.
/// 返回目标会话的水合快照；若分段目录不存在则返回空快照。
pub async fn load_session_hydration_snapshot_for_project_created_at(
    controller: ControllerRuntimeHandle,
    project_id: &str,
    created_at: &str,
    session_id: SessionId,
    requested_limit: usize,
) -> Result<SessionHydrationSnapshot, String> {
    let locator = query_session_locator_for_project_created_at(
        &controller,
        project_id,
        created_at,
        session_id,
    )
    .await?;
    load_session_hydration_snapshot_from_locator(controller, session_id, requested_limit, locator)
        .await
}

/// Lazily loads a display-complete transcript window by deterministic session identity.
/// 通过确定性会话身份懒加载展示完整的转录窗口。
///
/// # Parameters
/// 参数
/// `controller` owns the workspace-store runtime bridge.
/// `controller` 持有工作区存储运行时桥。
/// `project_id` is the known owning project identifier used as the segmented project tag.
/// `project_id` 是已知的所属项目标识，用作分段项目标签。
/// `created_at` is the raw Unix-milliseconds creation timestamp from the session summary.
/// `created_at` 是会话摘要中的原始 Unix 毫秒创建时间戳。
/// `session_id` selects the session to hydrate.
/// `session_id` 选择要水合的会话。
/// `requested_limit` is the requested recent-turn window size before clamping.
/// `requested_limit` 是裁剪前请求的最近轮次窗口大小。
///
/// # Returns
/// 返回值
/// A display-complete hydration snapshot for the target session.
/// 返回目标会话的展示完整水合快照。
pub async fn load_session_display_hydration_snapshot_for_project_created_at(
    controller: ControllerRuntimeHandle,
    project_id: &str,
    created_at: &str,
    session_id: SessionId,
    requested_limit: usize,
) -> Result<SessionHydrationSnapshot, String> {
    let locator = query_session_locator_for_project_created_at(
        &controller,
        project_id,
        created_at,
        session_id,
    )
    .await?;
    load_session_hydration_snapshot_from_locator(controller, session_id, requested_limit, locator)
        .await
}

/// Builds a session hydration snapshot from an already resolved segmented locator.
/// 根据已经解析出的分段定位信息构建会话水合快照。
///
/// # Parameters
/// 参数
/// `controller` owns the workspace-store runtime bridge.
/// `controller` 持有工作区存储运行时桥。
/// `session_id` selects the session to hydrate.
/// `session_id` 选择要水合的会话。
/// `requested_limit` is the requested recent-turn window size before clamping.
/// `requested_limit` 是裁剪前请求的最近轮次窗口大小。
/// `locator` points at the segmented session directory when one was found.
/// `locator` 在找到分段会话目录时指向该目录。
///
/// # Returns
/// 返回值
/// A hydration snapshot assembled from the locator and segmented state.
/// 返回基于定位信息与分段状态组装出的水合快照。
async fn load_session_hydration_snapshot_from_locator(
    controller: ControllerRuntimeHandle,
    session_id: SessionId,
    requested_limit: usize,
    locator: Option<StoredSessionLocatorRow>,
) -> Result<SessionHydrationSnapshot, String> {
    let requested_limit = requested_limit.clamp(1, SESSION_TRANSCRIPT_WINDOW_MAX_TURN_LIMIT);
    let compactions = Vec::new();
    let target_turn_count =
        resolve_initial_hydration_turn_count(requested_limit, &compactions, locator.as_ref());
    let segmented_window = load_segmented_window_messages(
        &controller,
        locator.as_ref(),
        session_id,
        None,
        target_turn_count,
    )?;
    let (
        has_more_history,
        mut messages,
        mut tool_detail_messages,
        earliest_ordinal,
        full_messages,
        boundary_conversation_state,
    ) = segmented_window
        .map(|window| {
            (
                window.has_more_history,
                window.messages,
                window.tool_detail_messages,
                window.earliest_ordinal,
                window.full_messages,
                window.boundary_conversation_state,
            )
        })
        .unwrap_or_else(|| (false, Vec::new(), Vec::new(), None, Vec::new(), None));
    let runtime_state =
        match load_segmented_runtime_state(&controller, locator.as_ref(), session_id)? {
            Some(runtime_state) => Some(runtime_state),
            None => hydrate_session_runtime_state(session_id, &full_messages).await?,
        }
        .map(|mut runtime_state| {
            let provider_conversation_state = runtime_state.conversation_state.clone();
            repair_hydration_turn_stats_from_runtime_usage(
                &mut messages,
                &provider_conversation_state,
            );
            if let Some(boundary_conversation_state) = boundary_conversation_state {
                apply_boundary_conversation_projection_to_runtime_state(
                    &mut runtime_state,
                    boundary_conversation_state,
                );
            }
            runtime_state
        });
    restore_hydration_window_from_runtime_state_if_empty(
        &mut messages,
        &mut tool_detail_messages,
        runtime_state.as_ref(),
    );

    Ok(SessionHydrationSnapshot {
        session_id,
        messages,
        tool_detail_messages,
        compactions,
        runtime_state,
        earliest_ordinal,
        has_more_history,
    })
}

/// Repairs visible hydrated turn-stat rows with provider usage retained in the runtime conversation state.
/// 使用运行态会话状态中保留的 Provider 用量修复可见水合轮次统计行。
fn repair_hydration_turn_stats_from_runtime_usage(
    messages: &mut [SessionMessageEntry],
    runtime_conversation_state: &SessionConversationState,
) {
    for stats_index in 0..messages.len() {
        if messages[stats_index].record_kind != SessionMessageRecordKind::TurnStats {
            continue;
        }
        let Some(existing_stats) = turn_stats_from_message(&messages[stats_index]) else {
            continue;
        };
        if existing_stats.turn_kind != SessionTurnStatsKind::UserTurn {
            continue;
        }
        let Some(turn_user_index) = hydrated_turn_user_message_index(messages, stats_index) else {
            continue;
        };
        let Some(runtime_start_index) = runtime_turn_start_index_for_display_user(
            runtime_conversation_state,
            &messages[turn_user_index],
        ) else {
            continue;
        };
        let runtime_end_index =
            runtime_next_turn_start_index(runtime_conversation_state, runtime_start_index + 1)
                .unwrap_or(runtime_conversation_state.messages.len());
        let runtime_messages =
            &runtime_conversation_state.messages[runtime_start_index..runtime_end_index];
        let Some(mut repaired_stats) =
            repaired_turn_stats_from_runtime_messages(&existing_stats, runtime_messages)
        else {
            continue;
        };
        let explicit_usage_segments = explicit_usage_segments_for_hydrated_turn(
            messages,
            turn_user_index,
            stats_index,
            &existing_stats,
        );
        merge_explicit_usage_segments_into_hydration_stats(
            &mut repaired_stats,
            explicit_usage_segments,
        );
        if !runtime_turn_stats_should_replace(&existing_stats, &repaired_stats) {
            continue;
        }
        apply_turn_stats_to_message(&mut messages[stats_index], repaired_stats);
    }
}

/// Reads the structured turn statistics stored on one transcript message.
/// 读取单条转录消息上保存的结构化轮次统计。
fn turn_stats_from_message(message: &SessionMessageEntry) -> Option<SessionTurnStats> {
    message
        .metadata
        .as_ref()
        .and_then(|metadata| {
            metadata
                .get("turn_stats")
                .or_else(|| metadata.get("turnStats"))
        })
        .and_then(|value| serde_json::from_value::<SessionTurnStats>(value.clone()).ok())
}

/// Finds the visible user transcript row that owns one hydrated turn-stats row.
/// 查找拥有单条水合轮次统计行的可见用户转录行。
fn hydrated_turn_user_message_index(
    messages: &[SessionMessageEntry],
    stats_index: usize,
) -> Option<usize> {
    messages[..stats_index]
        .iter()
        .rposition(session_message_starts_visible_user_turn)
}

/// Returns whether one hydrated transcript row starts a user turn.
/// 返回单条水合转录行是否开启用户轮次。
fn session_message_starts_visible_user_turn(message: &SessionMessageEntry) -> bool {
    if message.kind != SessionMessageKind::User
        || message.record_kind != SessionMessageRecordKind::Transcript
        || !message.visible
    {
        return false;
    }
    message
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("starts_user_turn"))
        .and_then(Value::as_bool)
        .unwrap_or(true)
}

/// Locates the runtime user message matching one visible display user row.
/// 定位与单条可见展示用户行匹配的运行态用户消息。
fn runtime_turn_start_index_for_display_user(
    runtime_conversation_state: &SessionConversationState,
    display_user: &SessionMessageEntry,
) -> Option<usize> {
    let matches = runtime_conversation_state
        .messages
        .iter()
        .enumerate()
        .filter_map(|(index, message)| {
            runtime_user_message_matches_display_user(message, display_user).then_some(index)
        })
        .collect::<Vec<_>>();
    (matches.len() == 1).then(|| matches[0])
}

/// Returns whether one runtime user message describes the same input as a display row.
/// 返回单条运行态用户消息是否描述与展示行相同的输入。
fn runtime_user_message_matches_display_user(
    message: &SessionConversationMessage,
    display_user: &SessionMessageEntry,
) -> bool {
    let SessionConversationPayload::UserText { text, .. } = &message.payload else {
        return false;
    };
    let created_at = display_user.created_at.trim();
    if !created_at.is_empty() && message.id.contains(created_at) {
        return true;
    }
    text.trim() == display_user.body.trim()
}

/// Finds the next runtime message that begins a provider-visible turn.
/// 查找下一条开启 Provider 可见轮次的运行态消息。
fn runtime_next_turn_start_index(
    runtime_conversation_state: &SessionConversationState,
    start_index: usize,
) -> Option<usize> {
    runtime_conversation_state
        .messages
        .iter()
        .enumerate()
        .skip(start_index)
        .find_map(|(index, message)| runtime_message_starts_provider_turn(message).then_some(index))
}

/// Returns whether one runtime message starts a provider-visible turn.
/// 返回单条运行态消息是否开启 Provider 可见轮次。
fn runtime_message_starts_provider_turn(message: &SessionConversationMessage) -> bool {
    match &message.payload {
        SessionConversationPayload::UserText { .. } => true,
        SessionConversationPayload::AgentMessage { trigger_turn, .. } => *trigger_turn,
        SessionConversationPayload::Assistant { .. }
        | SessionConversationPayload::ToolResult { .. } => false,
    }
}

/// Builds repaired stats from runtime messages when they contain richer usage than the display row.
/// 当运行态消息包含比展示行更完整的用量时构建修复后的统计。
fn repaired_turn_stats_from_runtime_messages(
    existing_stats: &SessionTurnStats,
    runtime_messages: &[SessionConversationMessage],
) -> Option<SessionTurnStats> {
    let mut provider_call_count = 0usize;
    let mut tool_call_count = 0usize;
    let mut cumulative_usage = ProviderUsageSnapshot::default();
    let mut has_usage = false;

    for message in runtime_messages {
        let SessionConversationPayload::Assistant {
            text,
            reasoning,
            usage,
            tool_calls,
            ..
        } = &message.payload
        else {
            continue;
        };
        if !text.trim().is_empty()
            || !reasoning.trim().is_empty()
            || !tool_calls.is_empty()
            || usage.is_some()
        {
            provider_call_count = provider_call_count.saturating_add(1);
        }
        tool_call_count = tool_call_count.saturating_add(tool_calls.len());
        if let Some(usage) = usage {
            cumulative_usage.accumulate_from(&usage);
            has_usage = true;
        }
    }

    if provider_call_count == 0 && !has_usage {
        return None;
    }

    let provider_call_count = provider_call_count.max(existing_stats.provider_call_count);
    let tool_call_count = tool_call_count.max(existing_stats.tool_call_count);
    let mut segment = SessionTurnUsageSegment::from_provider_usage(
        SessionTurnUsageSegmentKind::AssistantResponse,
        provider_call_count,
        has_usage.then_some(&cumulative_usage),
    );
    segment.tool_call_count = tool_call_count;

    let mut repaired_stats = SessionTurnStats {
        status: existing_stats.status,
        turn_kind: existing_stats.turn_kind,
        duration_ms: existing_stats.duration_ms,
        ..SessionTurnStats::default()
    };
    repaired_stats.merge_usage_segment(&segment);
    repaired_stats.usage_segments.push(segment);
    repaired_stats.output_tokens_per_second = repaired_stats.duration_ms.and_then(|duration_ms| {
        if duration_ms == 0 || repaired_stats.output_tokens == 0 {
            return None;
        }
        Some(repaired_stats.output_tokens as f64 / (duration_ms as f64 / 1000.0))
    });
    Some(repaired_stats)
}

/// Collects non-regular usage segments that already belong to one hydrated display turn.
/// 收集已属于单个水合展示回合的非常规用量分段。
///
/// # Parameters
/// 参数
/// `messages` are the hydrated display rows.
/// `messages` 是水合后的显示行。
/// `turn_user_index` is the visible user row that opens the turn.
/// `turn_user_index` 是开启当前回合的可见用户行。
/// `stats_index` is the terminal stats row being repaired.
/// `stats_index` 是正在修复的终态统计行。
/// `existing_stats` are the stats already stored on the terminal row.
/// `existing_stats` 是终态行上已存储的统计。
///
/// # Returns
/// 返回值
/// Unique non-assistant usage segments from the terminal row and prior display-only stats rows.
/// 返回来自终态行与前置显示统计行的唯一非助手用量分段。
fn explicit_usage_segments_for_hydrated_turn(
    messages: &[SessionMessageEntry],
    turn_user_index: usize,
    stats_index: usize,
    existing_stats: &SessionTurnStats,
) -> Vec<SessionTurnUsageSegment> {
    let mut segments = Vec::new();
    for segment in existing_stats
        .usage_segments
        .iter()
        .filter(|segment| segment.kind != SessionTurnUsageSegmentKind::AssistantResponse)
    {
        push_unique_usage_segment(&mut segments, segment.clone());
    }
    for message in &messages[turn_user_index.saturating_add(1)..stats_index] {
        let Some(stats) = turn_stats_from_message(message) else {
            continue;
        };
        for segment in stats
            .usage_segments
            .iter()
            .filter(|segment| segment.kind != SessionTurnUsageSegmentKind::AssistantResponse)
        {
            push_unique_usage_segment(&mut segments, segment.clone());
        }
    }
    segments
}

/// Pushes one usage segment only when it has not already been collected.
/// 仅当用量分段尚未被收集时才追加。
///
/// # Parameters
/// 参数
/// `segments` is the target unique segment list.
/// `segments` 是目标唯一分段列表。
/// `segment` is the candidate segment.
/// `segment` 是候选分段。
fn push_unique_usage_segment(
    segments: &mut Vec<SessionTurnUsageSegment>,
    segment: SessionTurnUsageSegment,
) {
    if !segments.iter().any(|existing| existing == &segment) {
        segments.push(segment);
    }
}

/// Merges explicit non-regular segments into repaired hydration stats.
/// 将显式非常规分段合并进修复后的水合统计。
///
/// # Parameters
/// 参数
/// `stats` is the repaired turn statistics snapshot.
/// `stats` 是修复后的轮次统计快照。
/// `usage_segments` are display-owned explicit usage segments.
/// `usage_segments` 是显示层已有的显式用量分段。
fn merge_explicit_usage_segments_into_hydration_stats(
    stats: &mut SessionTurnStats,
    usage_segments: Vec<SessionTurnUsageSegment>,
) {
    for segment in usage_segments {
        if stats.turn_kind == SessionTurnStatsKind::UserTurn
            && segment.kind == SessionTurnUsageSegmentKind::SubagentTask
        {
            stats.merge_usage_segment_group(&segment);
        } else {
            stats.merge_usage_segment(&segment);
        }
        stats.usage_segments.push(segment);
    }
}

/// Returns whether repaired runtime stats are strictly richer than the stored stats row.
/// 返回运行态修复统计是否严格比已存统计行更完整。
fn runtime_turn_stats_should_replace(
    existing_stats: &SessionTurnStats,
    repaired_stats: &SessionTurnStats,
) -> bool {
    repaired_stats.total_tokens > existing_stats.total_tokens
        || repaired_stats.input_tokens > existing_stats.input_tokens
        || repaired_stats.output_tokens > existing_stats.output_tokens
        || repaired_stats.reasoning_tokens > existing_stats.reasoning_tokens
        || repaired_stats.cached_input_tokens.unwrap_or_default()
            > existing_stats.cached_input_tokens.unwrap_or_default()
        || non_regular_usage_group_total(repaired_stats)
            > non_regular_usage_group_total(existing_stats)
}

/// Sums the displayed usage total for non-regular usage groups.
/// 汇总非常规用量分组的展示总量。
///
/// # Parameters
/// 参数
/// `stats` is the turn statistics snapshot to inspect.
/// `stats` 是需要检查的轮次统计快照。
///
/// # Returns
/// 返回值
/// Total tokens from usage groups other than the regular assistant group.
/// 返回常规助手分组以外的用量总 Tokens。
fn non_regular_usage_group_total(stats: &SessionTurnStats) -> usize {
    stats
        .usage_groups
        .iter()
        .filter(|group| group.kind != SessionTurnUsageGroupKind::Regular)
        .map(|group| group.total_tokens)
        .sum()
}

/// Writes repaired turn statistics back onto the hydrated display message.
/// 将修复后的轮次统计写回水合展示消息。
fn apply_turn_stats_to_message(message: &mut SessionMessageEntry, turn_stats: SessionTurnStats) {
    message.title = format!("Turn Stats · {}", turn_stats.status.label());
    message.body = turn_stats.detail_body();
    let Ok(turn_stats_value) = serde_json::to_value(&turn_stats) else {
        return;
    };
    let metadata = message
        .metadata
        .get_or_insert_with(|| serde_json::json!({}));
    if !metadata.is_object() {
        *metadata = serde_json::json!({});
    }
    if let Some(metadata_object) = metadata.as_object_mut() {
        metadata_object.insert(String::from("turn_stats"), turn_stats_value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use vulcan_workbench_model::SessionTurnStatsStatus;

    /// Verifies hydration repairs legacy stats rows that kept only the last provider usage.
    /// 验证水合会修复仅保留最后一次 Provider 用量的历史统计行。
    #[test]
    fn repair_hydration_turn_stats_uses_runtime_cumulative_usage() {
        let mut user = SessionMessageEntry::new(SessionMessageKind::User, "用户指令", "分析项目");
        user.id = String::from("display-user-100");
        user.created_at = String::from("100");

        let old_stats = SessionTurnStats {
            status: SessionTurnStatsStatus::Completed,
            turn_kind: SessionTurnStatsKind::UserTurn,
            provider_call_count: 2,
            input_tokens: 20,
            output_tokens: 3,
            total_tokens: 23,
            cached_input_tokens: Some(8),
            ..SessionTurnStats::default()
        };
        let mut stats_entry = SessionMessageEntry::new(
            SessionMessageKind::Assistant,
            "Turn Stats · completed",
            old_stats.detail_body(),
        );
        stats_entry.record_kind = SessionMessageRecordKind::TurnStats;
        stats_entry.metadata = Some(json!({ "turn_stats": old_stats }));

        let runtime_conversation_state = SessionConversationState {
            messages: vec![
                SessionConversationMessage {
                    id: String::from("session-test-protocol-user-100"),
                    payload: SessionConversationPayload::UserText {
                        text: String::from("分析项目"),
                        attachments: Vec::new(),
                    },
                },
                assistant_message_with_usage("assistant-1", 10, 2, 12, 4),
                assistant_message_with_usage("assistant-2", 20, 3, 23, 8),
            ],
            ..SessionConversationState::default()
        };
        let mut messages = vec![user, stats_entry];

        repair_hydration_turn_stats_from_runtime_usage(&mut messages, &runtime_conversation_state);

        let repaired_stats = turn_stats_from_message(&messages[1]).expect("repaired stats");
        assert_eq!(repaired_stats.provider_call_count, 2);
        assert_eq!(repaired_stats.input_tokens, 30);
        assert_eq!(repaired_stats.output_tokens, 5);
        assert_eq!(repaired_stats.total_tokens, 35);
        assert_eq!(repaired_stats.cached_input_tokens, Some(12));
        assert_eq!(repaired_stats.usage_groups.len(), 1);
        assert_eq!(repaired_stats.usage_groups[0].total_tokens, 35);
        assert!(messages[1].body.contains("总 Tokens：35"));
    }

    /// Verifies hydration preserves display-only subagent usage when repairing terminal stats.
    /// 验证水合修复终态统计时会保留显示层已有的智能体用量。
    #[test]
    fn repair_hydration_turn_stats_preserves_subagent_usage_groups() {
        let mut user = SessionMessageEntry::new(SessionMessageKind::User, "用户指令", "分析项目");
        user.id = String::from("display-user-100");
        user.created_at = String::from("100");

        let subagent_usage = ProviderUsageSnapshot {
            input_tokens: Some(100),
            output_tokens: Some(20),
            reasoning_tokens: Some(10),
            total_tokens: Some(130),
            cached_input_tokens: Some(50),
            ..ProviderUsageSnapshot::default()
        };
        let mut subagent_segment = SessionTurnUsageSegment::from_provider_usage(
            SessionTurnUsageSegmentKind::SubagentTask,
            1,
            Some(&subagent_usage),
        );
        subagent_segment.trigger = Some(String::from("review-agent"));
        let subagent_stats = SessionTurnStats {
            status: SessionTurnStatsStatus::Completed,
            turn_kind: SessionTurnStatsKind::SubagentFeedback,
            usage_segments: vec![subagent_segment],
            ..SessionTurnStats::default()
        };
        let mut subagent_entry = SessionMessageEntry::new(
            SessionMessageKind::Assistant,
            "智能体统计 · review-agent",
            subagent_stats.detail_body(),
        );
        subagent_entry.record_kind = SessionMessageRecordKind::TurnStats;
        subagent_entry.metadata = Some(json!({ "turn_stats": subagent_stats }));

        let old_stats = SessionTurnStats {
            status: SessionTurnStatsStatus::Completed,
            turn_kind: SessionTurnStatsKind::UserTurn,
            provider_call_count: 2,
            input_tokens: 20,
            output_tokens: 3,
            total_tokens: 23,
            cached_input_tokens: Some(8),
            ..SessionTurnStats::default()
        };
        let mut stats_entry = SessionMessageEntry::new(
            SessionMessageKind::Assistant,
            "Turn Stats · completed",
            old_stats.detail_body(),
        );
        stats_entry.record_kind = SessionMessageRecordKind::TurnStats;
        stats_entry.metadata = Some(json!({ "turn_stats": old_stats }));

        let runtime_conversation_state = SessionConversationState {
            messages: vec![
                SessionConversationMessage {
                    id: String::from("session-test-protocol-user-100"),
                    payload: SessionConversationPayload::UserText {
                        text: String::from("分析项目"),
                        attachments: Vec::new(),
                    },
                },
                assistant_message_with_usage("assistant-1", 10, 2, 12, 4),
                assistant_message_with_usage("assistant-2", 20, 3, 23, 8),
            ],
            ..SessionConversationState::default()
        };
        let mut messages = vec![user, subagent_entry, stats_entry];

        repair_hydration_turn_stats_from_runtime_usage(&mut messages, &runtime_conversation_state);

        let repaired_stats = turn_stats_from_message(&messages[2]).expect("repaired stats");
        assert_eq!(repaired_stats.total_tokens, 35);
        assert_eq!(repaired_stats.usage_groups.len(), 2);
        assert_eq!(
            repaired_stats.usage_groups[0].kind,
            SessionTurnUsageGroupKind::Regular
        );
        assert_eq!(repaired_stats.usage_groups[0].total_tokens, 35);
        assert_eq!(
            repaired_stats.usage_groups[1].kind,
            SessionTurnUsageGroupKind::Subagent
        );
        assert_eq!(repaired_stats.usage_groups[1].total_tokens, 130);
        assert!(messages[2].body.contains("智能体"));
    }

    /// Builds one assistant runtime message carrying provider usage.
    /// 构建一条携带 Provider 用量的助手运行态消息。
    fn assistant_message_with_usage(
        id: &str,
        input_tokens: usize,
        output_tokens: usize,
        total_tokens: usize,
        cached_input_tokens: usize,
    ) -> SessionConversationMessage {
        SessionConversationMessage {
            id: id.to_string(),
            payload: SessionConversationPayload::Assistant {
                text: String::from("ok"),
                reasoning: String::new(),
                provider_message_id: None,
                provider_message_state: None,
                usage: Some(ProviderUsageSnapshot {
                    input_tokens: Some(input_tokens),
                    output_tokens: Some(output_tokens),
                    total_tokens: Some(total_tokens),
                    cached_input_tokens: Some(cached_input_tokens),
                    ..ProviderUsageSnapshot::default()
                }),
                tool_calls: Vec::new(),
            },
        }
    }
}

/// Repairs duplicate canonical record ids inside one loaded conversation state so older broken sessions can recover during hydration.
/// 修复单份已加载会话状态中的重复规范记录标识，确保旧的损坏会话能在水合阶段自动恢复。
pub(super) fn repair_duplicate_conversation_record_ids(
    session_id: SessionId,
    conversation_state: &mut SessionConversationState,
) -> bool {
    let mut seen_ids = BTreeSet::new();
    let mut repaired = false;

    for (record_index, record) in conversation_state.records.iter_mut().enumerate() {
        if seen_ids.insert(record.id.clone()) {
            continue;
        }

        let created_at = non_empty_timestamp_text(&record.created_at);
        let mut candidate_id = format!("session-{session_id}-message-{created_at}-{record_index}");
        let mut collision_suffix = 1usize;
        while !seen_ids.insert(candidate_id.clone()) {
            candidate_id = format!(
                "session-{session_id}-message-{created_at}-{record_index}-{collision_suffix}"
            );
            collision_suffix = collision_suffix.saturating_add(1);
        }
        record.id = candidate_id;
        repaired = true;
    }

    repaired
}
