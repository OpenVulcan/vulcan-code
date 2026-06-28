//! Session goal lifecycle accounting helpers.
//! 会话目标生命周期 accounting 辅助逻辑。

use super::*;

/// Accounts one completed provider report against the selected session goal.
/// 将单次完成的 Provider 报告计入所选会话目标。
pub(crate) async fn account_session_goal_from_report(
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    report: &ExecutionRunReport,
    elapsed_seconds: i64,
) -> Result<(), String> {
    if report
        .preparation
        .as_ref()
        .is_some_and(|preparation| preparation.prompt_purpose != ExecutionPromptPurpose::UserTurn)
    {
        return Ok(());
    }
    let is_regular_session = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?
        .workspace_snapshot
        .as_ref()
        .is_some_and(|workspace_snapshot| {
            workspace_snapshot_contains_regular_session(workspace_snapshot, report.session_id)
        });
    if !is_regular_session {
        return Ok(());
    }
    let turn_stats = report
        .conversation_state
        .as_ref()
        .and_then(|conversation_state| {
            latest_session_turn_stats_from_state(
                conversation_state,
                Some(report.provider_request_count),
                report.embedded_turn_usage_segments.clone(),
            )
        });
    let token_delta = turn_stats
        .as_ref()
        .and_then(goal_token_delta_for_turn_stats)
        .or_else(|| {
            goal_token_delta_for_provider_usage(report.last_successful_provider_usage.as_ref())
        })
        .unwrap_or_default();
    if token_delta <= 0 && elapsed_seconds <= 0 && report.error.is_none() {
        return Ok(());
    }
    let goal_store = VulcanGoalStore::new(controller_runtime.clone());
    if token_delta > 0 || elapsed_seconds > 0 {
        let outcome = goal_store
            .account_thread_goal_usage(
                report.session_id,
                token_delta,
                elapsed_seconds,
                GoalAccountingMode::ActiveOrComplete,
                None,
            )
            .await?;
        match outcome {
            GoalAccountingOutcome::Updated(goal) => {
                let summary = session_goal_summary_from_thread_goal(&goal);
                if goal.status == ThreadGoalStatus::BudgetLimited
                    && let Err(error) = enqueue_session_goal_budget_limited_guidance(
                        state,
                        report.session_id,
                        &summary,
                    )
                {
                    eprintln!(
                        "[webui-service] enqueue goal budget-limited guidance failed: {error}"
                    );
                }
                cache_session_goal_summary(state, report.session_id, Some(goal))?;
            }
            GoalAccountingOutcome::Unchanged(goal) => {
                if let Some(goal) = goal {
                    let summary = session_goal_summary_from_thread_goal(&goal);
                    if goal.status == ThreadGoalStatus::BudgetLimited
                        && let Err(error) = enqueue_session_goal_budget_limited_guidance(
                            state,
                            report.session_id,
                            &summary,
                        )
                    {
                        eprintln!(
                            "[webui-service] enqueue unchanged goal budget-limited guidance failed: {error}"
                        );
                    }
                    cache_session_goal_summary(state, report.session_id, Some(goal))?;
                }
            }
        }
        if let Some(conversation_state) = report.conversation_state.as_ref() {
            cache_session_goal_live_usage_from_checkpoint(
                state,
                report.session_id,
                conversation_state,
                report.provider_request_count,
                false,
            )?;
        }
    }
    if let Some(error) = report.error.as_deref() {
        apply_session_goal_error_status(state, &goal_store, report.session_id, error).await?;
    }
    Ok(())
}

/// Updates the in-memory goal summary with live turn usage from one checkpoint.
/// 使用单个检查点中的当前轮次用量更新内存目标摘要。
pub(crate) fn cache_session_goal_live_usage_from_checkpoint(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    conversation_state: &SessionConversationState,
    provider_request_count: usize,
    add_turn_usage_to_accounted_total: bool,
) -> Result<(), String> {
    let Some(turn_stats) = latest_session_turn_stats_from_state(
        conversation_state,
        Some(provider_request_count),
        Vec::new(),
    ) else {
        return Ok(());
    };
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let Some(summary) = guard.session_goal_summaries.get_mut(&session_id) else {
        return Ok(());
    };
    if summary.status == ThreadGoalStatus::Complete.as_str() {
        return Ok(());
    }
    let turn_usage = goal_token_delta_for_turn_stats(&turn_stats).unwrap_or_default();
    summary.usage_tokens = if add_turn_usage_to_accounted_total {
        summary.tokens_used.saturating_add(turn_usage)
    } else {
        summary.tokens_used
    };
    summary.api_call_count = turn_stats.provider_call_count;
    summary.tool_call_count = turn_stats.tool_call_count;
    summary.input_tokens = i64::try_from(turn_stats.input_tokens).unwrap_or(i64::MAX);
    summary.cached_input_tokens =
        i64::try_from(turn_stats.cached_input_tokens.unwrap_or_default()).unwrap_or(i64::MAX);
    summary.output_tokens = i64::try_from(turn_stats.output_tokens).unwrap_or(i64::MAX);
    summary.total_tokens = i64::try_from(turn_stats.total_tokens).unwrap_or(i64::MAX);
    Ok(())
}

/// Builds live turn statistics from the latest conversation record.
/// 从最新会话记录构建实时轮次统计。
pub(crate) fn latest_session_turn_stats_from_state(
    conversation_state: &SessionConversationState,
    provider_request_count_override: Option<usize>,
    usage_segments: Vec<SessionTurnUsageSegment>,
) -> Option<SessionTurnStats> {
    let closing_record_id = conversation_state.records.last()?.id.as_str();
    build_session_turn_stats_from_state_with_segments(
        conversation_state,
        closing_record_id,
        SessionTurnStatsStatus::Completed,
        provider_request_count_override,
        SessionTurnStatsKind::UserTurn,
        usage_segments,
    )
}

/// Converts one turn-statistics snapshot into Codex-compatible goal usage tokens.
/// 将一份轮次统计转换为 Codex 兼容的目标用量 Token。
pub(crate) fn goal_token_delta_for_turn_stats(stats: &SessionTurnStats) -> Option<i64> {
    let input_tokens = stats.input_tokens;
    let cached_input_tokens = stats.cached_input_tokens.unwrap_or_default();
    let output_tokens = stats.output_tokens.saturating_add(stats.reasoning_tokens);
    let delta = input_tokens
        .saturating_sub(cached_input_tokens)
        .saturating_add(output_tokens);
    if delta > 0 {
        return Some(i64::try_from(delta).unwrap_or(i64::MAX));
    }
    (stats.total_tokens > 0).then(|| i64::try_from(stats.total_tokens).unwrap_or(i64::MAX))
}

/// Marks an active goal with the terminal status implied by a failed run.
/// 根据失败运行暗示的终态标记当前 active 目标。
pub(crate) async fn apply_session_goal_error_status(
    state: &SharedServiceBridgeState,
    goal_store: &VulcanGoalStore,
    session_id: SessionId,
    error: &str,
) -> Result<(), String> {
    let Some(goal) = goal_store.get_thread_goal(session_id).await? else {
        return Ok(());
    };
    if goal.status != ThreadGoalStatus::Active {
        cache_session_goal_summary(state, session_id, Some(goal))?;
        return Ok(());
    }
    let status = session_goal_error_status(error);
    let updated_goal = goal_store
        .update_thread_goal(
            session_id,
            GoalUpdate {
                objective: None,
                status: Some(status),
                token_budget: None,
                expected_goal_id: Some(goal.goal_id),
            },
        )
        .await?;
    if let Some(updated_goal) = updated_goal {
        cache_session_goal_summary(state, session_id, Some(updated_goal))?;
    }
    Ok(())
}

/// Classifies a failed run into a Codex-compatible goal stop status.
/// 将失败运行分类为与 Codex 兼容的目标停止状态。
pub(crate) fn session_goal_error_status(error: &str) -> ThreadGoalStatus {
    let normalized = error.to_ascii_lowercase();
    if normalized.contains("usage limit")
        || normalized.contains("rate limit")
        || normalized.contains("quota")
        || normalized.contains("insufficient_quota")
    {
        ThreadGoalStatus::UsageLimited
    } else {
        ThreadGoalStatus::Blocked
    }
}

/// Calculates the Codex-compatible token delta for goal accounting.
/// 计算与 Codex 兼容的目标记账 token 增量。
pub(crate) fn goal_token_delta_for_provider_usage(
    usage: Option<&ProviderUsageSnapshot>,
) -> Option<i64> {
    let usage = usage?;
    let input_tokens = usage.input_tokens.unwrap_or_default();
    let cached_input_tokens = usage.cached_input_tokens.unwrap_or_default();
    let output_tokens = usage.output_tokens.unwrap_or_default();
    let delta = input_tokens
        .saturating_sub(cached_input_tokens)
        .saturating_add(output_tokens);
    if delta > 0 {
        return Some(i64::try_from(delta).unwrap_or(i64::MAX));
    }
    usage
        .total_tokens
        .map(|total| i64::try_from(total).unwrap_or(i64::MAX))
        .filter(|total| *total > 0)
}
