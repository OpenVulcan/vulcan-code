//! Session goal lifecycle validation helpers.
//! 会话目标生命周期 validation 辅助逻辑。

use super::*;

pub(crate) const MAX_SESSION_GOAL_OBJECTIVE_CHARS: usize = 4_000;

/// Resolves a session id for one user-facing goal request.
/// 为单个面向用户的目标请求解析会话标识。
pub(crate) fn session_goal_request_session_id(
    state: &SharedServiceBridgeState,
    conversation_id: Option<&str>,
) -> Result<SessionId, String> {
    if let Some(session_id) = conversation_id.and_then(parse_session_id) {
        return Ok(session_id);
    }
    selected_session_id_from_state(state)
}

/// Ensures one regular persisted session exists before mutating goal state.
/// 在修改目标状态前确认常规持久化会话存在。
pub(crate) fn ensure_regular_session_exists(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    action_label: &str,
) -> Result<(), String> {
    let guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let workspace_snapshot = guard.workspace_snapshot.as_ref().ok_or_else(|| {
        String::from("service is still starting; workspace snapshot is not ready")
    })?;
    if workspace_snapshot_contains_regular_session(workspace_snapshot, session_id) {
        Ok(())
    } else {
        Err(format!("未找到要{action_label}的会话：{session_id}"))
    }
}

/// Ensures Plan mode is not entered while an unfinished goal exists.
/// 确认存在未完成目标时不会进入计划模式。
pub(crate) async fn ensure_session_can_enter_plan_mode(
    controller_runtime: &ControllerRuntimeHandle,
    session_id: SessionId,
) -> Result<(), String> {
    let goal_store = VulcanGoalStore::new(controller_runtime.clone());
    let existing_goal = goal_store
        .get_thread_goal(session_id)
        .await
        .map_err(|error| format!("读取当前目标失败：{error}"))?;
    if existing_goal
        .as_ref()
        .is_some_and(|goal| goal.status != ThreadGoalStatus::Complete)
    {
        return Err(String::from("计划模式与目标模式不能同时开启。"));
    }
    Ok(())
}

/// Ensures a goal cannot be created or resumed while Plan mode is active.
/// 确认计划模式激活时不能创建或继续目标。
pub(crate) fn ensure_session_not_in_plan_mode_for_goal(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
) -> Result<(), String> {
    let guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let is_plan = guard
        .workspace_snapshot
        .as_ref()
        .and_then(|workspace| workspace.session_runtime_state.get(&session_id))
        .and_then(|runtime_state| runtime_state.active_work_mode.as_deref())
        .is_some_and(|mode| mode.eq_ignore_ascii_case("plan"));
    if is_plan {
        return Err(String::from("计划模式与目标模式不能同时开启。"));
    }
    Ok(())
}

/// Validates one user-facing session goal objective.
/// 校验单个面向用户的会话目标文本。
pub(crate) fn validate_session_goal_objective(objective: &str) -> Result<(), String> {
    let objective = objective.trim();
    if objective.is_empty() {
        return Err(String::from("目标内容不能为空。"));
    }
    if objective.chars().count() > MAX_SESSION_GOAL_OBJECTIVE_CHARS {
        return Err(format!(
            "目标内容不能超过 {MAX_SESSION_GOAL_OBJECTIVE_CHARS} 个字符。"
        ));
    }
    Ok(())
}

/// Validates one optional user-facing session goal token budget.
/// 校验单个面向用户的可选会话目标 token 预算。
pub(crate) fn validate_session_goal_budget(token_budget: Option<i64>) -> Result<(), String> {
    if let Some(value) = token_budget
        && value <= 0
    {
        return Err(String::from("目标 token 预算必须是正整数。"));
    }
    Ok(())
}

/// Parses one WebUI goal status key.
/// 解析单个 WebUI 目标状态键。
pub(crate) fn parse_session_goal_status(status: &str) -> Result<ThreadGoalStatus, String> {
    match status.trim() {
        "active" => Ok(ThreadGoalStatus::Active),
        "paused" => Ok(ThreadGoalStatus::Paused),
        "blocked" => Ok(ThreadGoalStatus::Blocked),
        "usage_limited" => Ok(ThreadGoalStatus::UsageLimited),
        "budget_limited" => Ok(ThreadGoalStatus::BudgetLimited),
        "complete" => Ok(ThreadGoalStatus::Complete),
        other => Err(format!("不支持的目标状态：{other}")),
    }
}
