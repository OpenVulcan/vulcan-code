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
        status: goal.status.as_str().to_string(),
        token_budget: goal.token_budget,
        tokens_used: goal.tokens_used,
        usage_tokens: goal.tokens_used,
        api_call_count: 0,
        tool_call_count: 0,
        input_tokens: 0,
        cached_input_tokens: 0,
        output_tokens: 0,
        total_tokens: 0,
        time_used_seconds: goal.time_used_seconds,
        created_at_ms: goal.created_at_ms,
        updated_at_ms: goal.updated_at_ms,
    }
}
