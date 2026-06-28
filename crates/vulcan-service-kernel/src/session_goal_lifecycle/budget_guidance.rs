//! Session goal lifecycle budget guidance helpers.
//! 会话目标生命周期 budget guidance 辅助逻辑。

use super::*;

/// Clears the in-memory budget-limit report marker for one goal id.
/// 清理单个目标标识对应的内存预算耗尽提示标记。
pub(crate) fn clear_budget_limit_reported_goal_id(
    state: &SharedServiceBridgeState,
    goal_id: &str,
) -> Result<(), String> {
    state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?
        .budget_limit_reported_goal_ids
        .remove(goal_id);
    Ok(())
}

/// Returns whether one budget-limited goal already received a wrap-up guidance.
/// 返回单个预算耗尽目标是否已经收到过收尾引导。
pub(crate) fn budget_limit_reported_goal_id_exists(
    state: &SharedServiceBridgeState,
    goal_id: &str,
) -> Result<bool, String> {
    let guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    Ok(guard.budget_limit_reported_goal_ids.contains(goal_id))
}

/// Marks one budget-limited goal as reported after its wrap-up guidance is queued.
/// 在预算耗尽收尾引导成功入队后，将单个目标标记为已提示。
pub(crate) fn mark_budget_limit_reported_goal_id(
    state: &SharedServiceBridgeState,
    goal_id: &str,
) -> Result<(), String> {
    state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?
        .budget_limit_reported_goal_ids
        .insert(goal_id.to_string());
    Ok(())
}

/// Queues one budget-limited stage wrap-up guidance for the active session.
/// 为活动会话排入一条预算耗尽阶段收尾引导。
pub(crate) fn enqueue_session_goal_budget_limited_guidance(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    goal: &SessionGoalSummary,
) -> Result<bool, String> {
    if budget_limit_reported_goal_id_exists(state, &goal.goal_id)? {
        return Ok(false);
    }
    let now = current_unix_millis_text();
    let entry = PendingSessionPromptEntry {
        id: format!("webui-goal-budget-limited-{}", goal.goal_id),
        prompt: render_goal_budget_limited_context(goal),
        hidden_model_guidance: None,
        preview: String::from("目标预算已用尽"),
        attachments: Vec::new(),
        model_reference: None,
        runtime_options: PendingPromptRuntimeOptions::default(),
        source: PendingSessionPromptSource::GoalContinuation,
        task_id: Some(goal.goal_id.clone()),
        panel_entry_id: Some(format!("webui-goal-budget-limited-{}", goal.goal_id)),
        created_at: now.clone(),
    };
    {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let Some(workspace_snapshot) = guard.workspace_snapshot.as_mut() else {
            return Ok(false);
        };
        let runtime_state = workspace_snapshot
            .session_runtime_state
            .entry(session_id)
            .or_insert_with(SessionRuntimeStateEntry::default);
        enqueue_internal_pending_prompt_entry(
            runtime_state,
            entry,
            true,
            now,
            "目标预算耗尽引导入队失败",
        )?;
    }
    mark_budget_limit_reported_goal_id(state, &goal.goal_id)?;
    Ok(true)
}

/// Queues Codex-compatible objective-updated steering for the next active provider boundary.
/// 为下一次活跃 Provider 边界排入与 Codex 兼容的目标更新引导。
pub(crate) fn enqueue_session_goal_objective_updated_guidance(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    goal: &SessionGoalSummary,
) -> Result<(), String> {
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let has_active_run = webui_session_has_active_run(&guard, session_id)
        || webui_session_kernel_has_active_turn(&guard, session_id);
    if !has_active_run {
        return Ok(());
    }
    let Some(workspace_snapshot) = guard.workspace_snapshot.as_mut() else {
        return Ok(());
    };
    let runtime_state = workspace_snapshot
        .session_runtime_state
        .entry(session_id)
        .or_insert_with(SessionRuntimeStateEntry::default);
    let now = current_unix_millis_text();
    let entry = PendingSessionPromptEntry {
        id: format!("webui-goal-objective-updated-{}", goal.goal_id),
        prompt: render_goal_objective_updated_context(goal),
        hidden_model_guidance: None,
        preview: String::from("目标已更新"),
        attachments: Vec::new(),
        model_reference: None,
        runtime_options: PendingPromptRuntimeOptions::default(),
        source: PendingSessionPromptSource::GoalContinuation,
        task_id: Some(goal.goal_id.clone()),
        panel_entry_id: Some(format!("webui-goal-objective-updated-{}", goal.goal_id)),
        created_at: now.clone(),
    };
    enqueue_internal_pending_prompt_entry(runtime_state, entry, true, now, "目标更新引导入队失败")?;
    Ok(())
}
