//! Session goal lifecycle validation helpers.
//! 会话目标生命周期 validation 辅助逻辑。

use super::*;

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

/// Normalizes one user-facing session goal objective through the shared store contract.
/// 通过共享存储契约规范化一个面向用户的会话目标文本。
pub(crate) fn normalize_session_goal_objective(objective: &str) -> Result<String, String> {
    normalize_thread_goal_objective(objective)
        .map(str::to_string)
        .map_err(|error| match error {
            ThreadGoalObjectiveError::EmptyObjective => String::from("目标内容不能为空。"),
            ThreadGoalObjectiveError::ObjectiveTooLong { max_chars } => {
                format!("目标内容不能超过 {max_chars} 个字符。")
            }
        })
}

/// Validates one optional user-facing session goal token budget.
/// 校验单个面向用户的可选会话目标 token 预算。
pub(crate) fn validate_session_goal_budget(token_budget: Option<i64>) -> Result<(), String> {
    validate_thread_goal_token_budget(token_budget)
        .map_err(|ThreadGoalTokenBudgetError| String::from("目标 token 预算必须是正整数。"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vulcan_workspace_store::workspace_store::MAX_THREAD_GOAL_OBJECTIVE_CHARS;

    /// Verifies the service adapter preserves localized objective errors while reusing store rules.
    /// 验证 service 适配器在复用存储规则时仍保留本地化目标文本错误。
    #[test]
    fn session_goal_objective_adapter_normalizes_and_localizes() {
        assert_eq!(
            normalize_session_goal_objective("  继续长期目标  "),
            Ok(String::from("继续长期目标"))
        );
        assert_eq!(
            normalize_session_goal_objective(" "),
            Err(String::from("目标内容不能为空。"))
        );
        assert_eq!(
            normalize_session_goal_objective(
                "x".repeat(MAX_THREAD_GOAL_OBJECTIVE_CHARS + 1).as_str()
            ),
            Err(format!(
                "目标内容不能超过 {MAX_THREAD_GOAL_OBJECTIVE_CHARS} 个字符。"
            ))
        );
    }

    /// Verifies the service adapter preserves its localized budget error.
    /// 验证 service 适配器保留本地化预算错误。
    #[test]
    fn session_goal_budget_adapter_localizes_domain_error() {
        assert_eq!(validate_session_goal_budget(None), Ok(()));
        assert_eq!(validate_session_goal_budget(Some(1)), Ok(()));
        assert_eq!(
            validate_session_goal_budget(Some(0)),
            Err(String::from("目标 token 预算必须是正整数。"))
        );
    }
}
