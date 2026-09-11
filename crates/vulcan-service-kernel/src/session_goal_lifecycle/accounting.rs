//! Session Goal V1 statistics projection helpers.
//! 会话 Goal V1 统计投影辅助逻辑。

use vulcan_session_core::v1::config::GoalState;

use super::*;
use crate::session_runtime_registry_v1::{
    SessionFlushReason, SessionMutationAdmission, SessionMutationChange,
};

/// Refreshes one completed run from V1 statistics and records only a real Goal error transition.
/// 从 V1 统计刷新一次已完成运行，并且只记录真实的 Goal 错误转换。
///
/// # Parameters
/// 参数
/// `state` owns the V1 registry and visible projection; the remaining legacy
/// parameters preserve the current caller API until the provider owner removes
/// its obsolete accounting callback.
/// `state` 持有 V1 注册表与可见投影；其余遗留参数在 Provider 所有者删除过时记账回调前保持当前调用 API。
///
/// # Returns
/// 返回值
/// Returns whether the visible V1 projection changed.
/// 返回可见 V1 投影是否发生变化。
pub(crate) async fn account_session_goal_from_report(
    state: &SharedServiceBridgeState,
    _controller_runtime: &HostRuntimeHandle,
    report: &ExecutionRunReport,
    expected_goal_id: Option<&str>,
    _elapsed_seconds: i64,
) -> Result<bool, String> {
    if report
        .preparation
        .as_ref()
        .is_some_and(|preparation| preparation.prompt_purpose != ExecutionPromptPurpose::UserTurn)
    {
        return Ok(false);
    }
    let Some(expected_goal_id) = expected_goal_id else {
        return Ok(false);
    };
    let handle = session_v1_handle_from_state(state, report.session_id).await?;
    let before = session_goal_summary_from_v1_handle(handle.as_ref(), report.session_id)?;
    if !matches!(before.as_ref(), Some(summary) if summary.goal_id == expected_goal_id) {
        return Ok(false);
    }
    if let Some(error) = report.error.as_deref() {
        let next_state = session_goal_error_state(error);
        let now_ms = current_v1_goal_unix_millis()?;
        handle
            .mutate_durable_if_changed(
                SessionMutationAdmission::ExistingTurn,
                SessionFlushReason::MetadataChanged,
                move |document| {
                    if document.metadata.mode.goal.enabled
                        && document.metadata.mode.goal.state == GoalState::Active
                    {
                        debug_assert_eq!(next_state, GoalState::Blocked);
                        document.block_goal(session_goal_error_reason_code(error), now_ms)?;
                        Ok(SessionMutationChange::Changed(()))
                    } else {
                        Ok(SessionMutationChange::Unchanged(()))
                    }
                },
            )
            .map_err(|write_error| format!("持久化 V1 Goal 错误状态失败：{write_error}"))?;
    }
    let after = session_goal_summary_from_v1_handle(handle.as_ref(), report.session_id)?;
    cache_session_goal_summary(state, report.session_id, after.clone())?;
    Ok(before != after)
}

/// Resolves the compatibility Goal marker bound to a physical Provider request.
/// 解析绑定到物理 Provider 请求的兼容 Goal 标记。
///
/// The marker is not a storage or scheduling authority. V1 metadata and the
/// open Turn remain authoritative.
/// 该标记不是存储或调度权威；V1 metadata 与开放 Turn 保持权威。
pub(crate) fn session_goal_id_for_provider_request(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    previously_bound_goal_id: Option<&str>,
) -> Result<Option<String>, String> {
    let handle = registered_session_v1_handle_from_state(state, session_id)?;
    let goal = session_goal_summary_from_v1_handle(handle.as_ref(), session_id)?;
    let Some(goal) = goal else {
        return Ok(previously_bound_goal_id.map(str::to_owned));
    };
    if goal.status == SessionGoalStatus::Active
        || previously_bound_goal_id.is_some_and(|goal_id| goal_id == goal.goal_id)
    {
        return Ok(Some(goal.goal_id.clone()));
    }
    Ok(None)
}

/// Refreshes the Goal projection after V1 `api_stats` becomes durable.
/// 在 V1 `api_stats` 可靠落盘后刷新 Goal 投影。
///
/// # Parameters
/// 参数
/// The legacy usage and controller arguments are intentionally ignored because
/// the provider sink already wrote the only authoritative usage fact.
/// 遗留 usage 与 controller 参数被有意忽略，因为 Provider sink 已写入唯一权威用量事实。
///
/// # Returns
/// 返回值
/// Returns whether the request-bound visible projection changed.
/// 返回请求绑定的可见投影是否发生变化。
pub(crate) async fn account_session_goal_provider_usage(
    state: &SharedServiceBridgeState,
    _controller_runtime: &HostRuntimeHandle,
    session_id: SessionId,
    expected_goal_id: &str,
    _usage: &ProviderRequestCompletedUsage,
) -> Result<bool, String> {
    let handle = session_v1_handle_from_state(state, session_id).await?;
    let summary = session_goal_summary_from_v1_handle(handle.as_ref(), session_id)?;
    let Some(summary) = summary else {
        cache_session_goal_summary(state, session_id, None)?;
        return Ok(false);
    };
    if summary.goal_id != expected_goal_id {
        return Ok(false);
    }
    let changed = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        guard.session_goal_summaries.get(&session_id) != Some(&summary)
    };
    cache_session_goal_summary(state, session_id, Some(summary))?;
    Ok(changed)
}

/// Classifies a failed run into the V1 Goal state.
/// 将失败运行分类为 V1 Goal 状态。
pub(crate) fn session_goal_error_state(_error: &str) -> GoalState {
    GoalState::Blocked
}

/// Returns a stable display reason without exposing provider error text.
/// 返回稳定显示原因，同时不暴露 Provider 错误正文。
fn session_goal_error_reason_code(error: &str) -> &'static str {
    let normalized = error.to_ascii_lowercase();
    if normalized.contains("usage limit")
        || normalized.contains("rate limit")
        || normalized.contains("quota")
        || normalized.contains("insufficient_quota")
    {
        "goal_usage_limited"
    } else {
        "goal_blocked"
    }
}
