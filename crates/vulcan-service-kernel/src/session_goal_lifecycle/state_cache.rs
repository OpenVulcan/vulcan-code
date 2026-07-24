//! Session goal lifecycle state cache helpers.
//! 会话目标生命周期 state cache 辅助逻辑。

use super::*;

/// Updates the in-memory session goal summary cache.
/// 更新内存中的会话目标摘要缓存。
pub(crate) fn cache_session_goal_summary(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    goal: Option<ThreadGoal>,
) -> Result<(), String> {
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    match goal.as_ref().map(session_goal_summary_from_thread_goal) {
        Some(summary) => {
            guard.session_goal_summaries.insert(session_id, summary);
        }
        None => {
            guard.session_goal_summaries.remove(&session_id);
        }
    }
    Ok(())
}

/// Merges durable accounting fields into the matching in-memory goal without overwriting user mutations.
/// 将持久记账字段合并到匹配的内存目标，同时不覆盖用户变更。
///
/// # Parameters
/// 参数
/// `state` owns the in-process goal projection.
/// `state` 持有进程内目标投影。
/// `session_id` identifies the session whose accounting changed.
/// `session_id` 标识记账发生变化的会话。
/// `goal` is the durable row returned by one accounting mutation.
/// `goal` 是一次记账变更返回的持久行。
///
/// # Returns
/// 返回值
/// The changed matching visible projection, no value when unchanged or replaced, or a state-lock error.
/// 返回变化后的匹配可见投影；未变化或已替换时无值；锁失败时返回错误。
pub(crate) fn merge_session_goal_accounting_projection(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    goal: &ThreadGoal,
) -> Result<Option<SessionGoalSummary>, String> {
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let Some(summary) = guard.session_goal_summaries.get_mut(&session_id) else {
        return Ok(None);
    };
    if summary.goal_id != goal.goal_id {
        return Ok(None);
    }
    // previousSummary preserves an exact change detector for lightweight delta emission.
    // previousSummary 为轻量增量发送保留精确的变化检测基线。
    let previous_summary = summary.clone();
    summary.tokens_used = goal.tokens_used;
    summary.usage_tokens = goal.tokens_used;
    summary.input_tokens = goal.uncached_input_tokens;
    summary.cached_input_tokens = goal.cached_input_tokens;
    summary.output_tokens = goal.output_tokens;
    summary.reasoning_tokens = goal.reasoning_tokens;
    summary.total_tokens = goal.total_tokens;
    summary.time_used_seconds = goal.time_used_seconds;
    summary.updated_at_ms = summary.updated_at_ms.max(goal.updated_at_ms);
    if summary.status == SessionGoalStatus::Active && goal.status == ThreadGoalStatus::BudgetLimited
    {
        summary.status = SessionGoalStatus::BudgetLimited;
    }
    Ok((*summary != previous_summary).then(|| summary.clone()))
}

/// Reads the current in-process Goal dispatch-authority version for one session.
/// 读取单个会话当前进程内的 Goal 分发权威版本。
///
/// # Parameters
/// 参数
/// `state` owns the version map under the shared service lock.
/// `state` 在共享服务锁下持有版本映射。
/// `session_id` identifies the Goal session.
/// `session_id` 标识 Goal 会话。
///
/// # Returns
/// 返回值
/// The current monotonic version, defaulting to zero before the first mutation.
/// 返回当前单调版本；首次变更前默认为零。
pub(crate) fn session_goal_dispatch_version(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
) -> Result<u64, String> {
    let guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    Ok(guard
        .session_goal_dispatch_versions
        .get(&session_id)
        .copied()
        .unwrap_or_default())
}

/// Advances the Goal dispatch-authority version after one authoritative Goal mutation commits.
/// 在一次权威 Goal 变更提交后推进 Goal 分发权威版本。
///
/// # Parameters
/// 参数
/// `state` owns the version map under the shared service lock.
/// `state` 在共享服务锁下持有版本映射。
/// `session_id` identifies the changed Goal session.
/// `session_id` 标识发生变更的 Goal 会话。
///
/// # Returns
/// 返回值
/// The new monotonic version, or an explicit overflow/state-lock error.
/// 返回新的单调版本，或显式的溢出/状态锁错误。
pub(crate) fn advance_session_goal_dispatch_version(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
) -> Result<u64, String> {
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let version = guard
        .session_goal_dispatch_versions
        .entry(session_id)
        .or_default();
    *version = version.checked_add(1).ok_or_else(|| {
        format!("Goal dispatch authority version overflow for session `{session_id}`")
    })?;
    Ok(*version)
}

/// Reloads the selected session goal from the persistent store into the WebUI summary cache.
/// 将选中会话的目标从持久化存储重新载入 WebUI 摘要缓存。
pub(crate) async fn sync_selected_session_goal_summary_from_store(
    state: &SharedServiceBridgeState,
    controller_runtime: ControllerRuntimeHandle,
    selected_session_id: SessionId,
) -> Result<BTreeMap<SessionId, SessionGoalSummary>, String> {
    let goal = if selected_session_id == SessionId::nil() {
        None
    } else {
        VulcanGoalStore::new(controller_runtime)
            .get_thread_goal(selected_session_id)
            .await?
    };
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    if selected_session_id != SessionId::nil() {
        match goal.as_ref().map(session_goal_summary_from_thread_goal) {
            Some(summary) => {
                guard
                    .session_goal_summaries
                    .insert(selected_session_id, summary);
            }
            None => {
                guard.session_goal_summaries.remove(&selected_session_id);
            }
        }
    }
    Ok(guard.session_goal_summaries.clone())
}

pub(crate) fn session_goal_summary_from_thread_goal(goal: &ThreadGoal) -> SessionGoalSummary {
    SessionGoalSummary {
        session_id: goal.thread_id.to_string(),
        goal_id: goal.goal_id.clone(),
        objective: goal.objective.clone(),
        status: session_goal_status_from_thread_goal_status(goal.status),
        token_budget: goal.token_budget,
        tokens_used: goal.tokens_used,
        usage_tokens: goal.tokens_used,
        api_call_count: 0,
        tool_call_count: 0,
        input_tokens: goal.uncached_input_tokens,
        cached_input_tokens: goal.cached_input_tokens,
        output_tokens: goal.output_tokens,
        reasoning_tokens: goal.reasoning_tokens,
        total_tokens: goal.total_tokens,
        time_used_seconds: goal.time_used_seconds,
        created_at_ms: goal.created_at_ms,
        updated_at_ms: goal.updated_at_ms,
    }
}

/// Converts the persisted store status into the strict service API status.
/// 将持久化存储状态转换为严格的 service API 状态。
pub(crate) const fn session_goal_status_from_thread_goal_status(
    status: ThreadGoalStatus,
) -> SessionGoalStatus {
    match status {
        ThreadGoalStatus::Active => SessionGoalStatus::Active,
        ThreadGoalStatus::Paused => SessionGoalStatus::Paused,
        ThreadGoalStatus::Blocked => SessionGoalStatus::Blocked,
        ThreadGoalStatus::UsageLimited => SessionGoalStatus::UsageLimited,
        ThreadGoalStatus::BudgetLimited => SessionGoalStatus::BudgetLimited,
        ThreadGoalStatus::Complete => SessionGoalStatus::Complete,
    }
}
