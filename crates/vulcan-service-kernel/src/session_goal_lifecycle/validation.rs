//! Session goal lifecycle validation helpers.
//! 会话目标生命周期 validation 辅助逻辑。

use super::*;
use vulcan_session_core::v1::config::MAX_GOAL_OBJECTIVE_CHARS;

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
        Err(format!("Session to {action_label} not found: {session_id}"))
    }
}

/// Ensures Plan mode is not entered while an unfinished goal exists.
/// 确认存在未完成目标时不会进入计划模式。
pub(crate) async fn ensure_session_can_enter_plan_mode(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
) -> Result<(), String> {
    let handle = session_v1_handle_from_state(state, session_id).await?;
    let goal_enabled = handle
        .read(|document| document.metadata.mode.goal.enabled)
        .map_err(|error| format!("Failed to read the current V1 goal state: {error}"))?;
    if goal_enabled {
        return Err(String::from(
            "Plan mode and goal mode cannot be on at the same time.",
        ));
    }
    Ok(())
}

/// Normalizes one user-facing session Goal objective against the V1 domain limit.
/// 按 V1 领域上限规范化一个面向用户的会话 Goal 文本。
pub(crate) fn normalize_session_goal_objective(objective: &str) -> Result<String, String> {
    let normalized = objective.trim();
    if normalized.is_empty() {
        return Err(String::from("The goal content must not be empty."));
    }
    if normalized.chars().count() > MAX_GOAL_OBJECTIVE_CHARS {
        return Err(format!(
            "The goal content must not exceed {MAX_GOAL_OBJECTIVE_CHARS} characters."
        ));
    }
    Ok(normalized.to_string())
}

/// Validates one optional user-facing session goal token budget.
/// 校验单个面向用户的可选会话目标 token 预算。
pub(crate) fn validate_session_goal_budget(token_budget: Option<i64>) -> Result<(), String> {
    if token_budget.is_some_and(|budget| budget <= 0) {
        return Err(String::from(
            "The goal token budget must be a positive integer.",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Verifies the service adapter preserves localized objective errors under the V1 contract.
    /// 验证 service 适配器在 V1 契约下保留本地化目标文本错误。
    #[test]
    fn session_goal_objective_adapter_normalizes_and_localizes() {
        assert_eq!(
            normalize_session_goal_objective("  继续长期目标  "),
            Ok(String::from("继续长期目标"))
        );
        assert_eq!(
            normalize_session_goal_objective(" "),
            Err(String::from("The goal content must not be empty."))
        );
        assert_eq!(
            normalize_session_goal_objective("x".repeat(MAX_GOAL_OBJECTIVE_CHARS + 1).as_str()),
            Err(format!(
                "The goal content must not exceed {MAX_GOAL_OBJECTIVE_CHARS} characters."
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
            Err(String::from(
                "The goal token budget must be a positive integer."
            ))
        );
    }
}
